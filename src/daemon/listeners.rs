//! One local listener and at most one filtered Quick ingress/provider per requested port.
use super::state::AppState;
use crate::{
    config::TunnelMode,
    error::{McpTunnelError, Result},
    gateway,
    tunnel::{quick::QuickTunnel, TunnelState},
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(crate) struct Worker {
    cancellation: CancellationToken,
    tasks: Vec<JoinHandle<()>>,
}
impl Worker {
    async fn stop(mut self) {
        self.cancellation.cancel();
        for mut task in self.tasks.drain(..) {
            if tokio::time::timeout(Duration::from_secs(3), &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancellation.cancel();
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub(crate) struct PortRuntime {
    local: Worker,
    public: Option<Worker>,
}

impl AppState {
    pub async fn ensure_listener(self: &Arc<Self>, port: u16) -> Result<()> {
        let mut listeners = self.listeners.lock().await;
        if listeners.contains_key(&port) {
            return Ok(());
        }
        let mut bind: std::net::SocketAddr = self.config.daemon.bind.parse().map_err(|_| {
            McpTunnelError::CliUsage("daemon.bind must be a loopback IP:port".into())
        })?;
        if !bind.ip().is_loopback() || port == 0 {
            return Err(McpTunnelError::CliUsage(
                "listeners require a loopback address and port 1–65535".into(),
            ));
        }
        bind.set_port(port);
        let listener = tokio::net::TcpListener::bind(bind).await.map_err(|error| {
            McpTunnelError::CliUsage(format!(
                "cannot listen on {bind}: {error}; choose another --port"
            ))
        })?;
        let cancellation = self.cancellation.child_token();
        let router = gateway::build_listener_router(self.clone(), port, false);
        let stop = cancellation.clone();
        let task = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await
            {
                tracing::error!(port, %error, "Local listener stopped");
            }
        });
        listeners.insert(
            port,
            PortRuntime {
                local: Worker {
                    cancellation,
                    tasks: vec![task],
                },
                public: None,
            },
        );
        Ok(())
    }

    pub async fn ensure_public(self: &Arc<Self>, port: u16) -> Result<()> {
        let mut listeners = self.listeners.lock().await;
        let runtime = listeners
            .get_mut(&port)
            .ok_or_else(|| McpTunnelError::Other("listener missing".into()))?;
        if runtime.public.is_some() {
            drop(listeners);
            if self
                .wait_for_tunnel_ready(port, Duration::from_secs(30))
                .await
            {
                return Ok(());
            }
            return Err(McpTunnelError::TunnelCreateFailed(
                "public provider unavailable; inspect tunnel status and retry".into(),
            ));
        }
        // Visibility is determined by this separate listener, never by a spoofable Host header.
        let ingress = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let bind = ingress.local_addr()?.to_string();
        let cancellation = self.cancellation.child_token();
        let router = gateway::build_listener_router(self.clone(), port, true);
        let stop = cancellation.clone();
        let server = tokio::spawn(async move {
            let _ = axum::serve(ingress, router)
                .with_graceful_shutdown(stop.cancelled_owned())
                .await;
        });
        let mut worker = Worker {
            cancellation,
            tasks: vec![server],
        };
        let mut tunnel = QuickTunnel::new(&self.config.tunnel.cloudflared_path, &bind);
        let origin = match tokio::select! {
            _ = self.cancellation.cancelled() => Err(McpTunnelError::TunnelCreateFailed("daemon shutting down".into())),
            result = tunnel.start() => result,
        } {
            Ok(origin) => origin,
            Err(error) => {
                tunnel.stop().await;
                worker.stop().await;
                return Err(error);
            }
        };
        self.set_tunnel_origin(port, Some(origin)).await;
        let state = self.clone();
        let stop = worker.cancellation.clone();
        worker.tasks.push(tokio::spawn(async move {
            loop {
                tokio::select! { _ = stop.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(5)) => {} }
                if tunnel.status().status == TunnelState::Ready { continue; }
                state.set_tunnel_origin(port, None).await;
                match tokio::select! { _ = stop.cancelled() => break, result = tunnel.start() => result } {
                    Ok(origin) => state.set_tunnel_origin(port, Some(origin)).await,
                    Err(error) => tracing::warn!(port, %error, "Quick tunnel unavailable; retrying"),
                }
            }
            state.set_tunnel_origin(port, None).await;
            tunnel.stop().await;
        }));
        runtime.public = Some(worker);
        Ok(())
    }

    /// Call while holding registration lock, after deleting or rolling back a mount.
    pub async fn prune_listeners(&self) -> Result<()> {
        let mounts = self.mount_registry.list()?;
        let mut listeners = self.listeners.lock().await;
        let ports: Vec<_> = listeners.keys().copied().collect();
        for port in ports {
            if !mounts.iter().any(|m| m.port == port) {
                if let Some(runtime) = listeners.remove(&port) {
                    if let Some(public) = runtime.public {
                        public.stop().await;
                    }
                    runtime.local.stop().await;
                }
            } else if !mounts
                .iter()
                .any(|m| m.port == port && m.tunnel_mode == TunnelMode::Quick)
            {
                if let Some(public) = listeners.get_mut(&port).and_then(|r| r.public.take()) {
                    public.stop().await;
                }
            }
            if !mounts
                .iter()
                .any(|m| m.port == port && m.tunnel_mode == TunnelMode::Quick)
            {
                self.set_tunnel_origin(port, None).await;
            }
        }
        Ok(())
    }

    pub async fn stop_listeners(&self) {
        let listeners: BTreeMap<_, _> = std::mem::take(&mut *self.listeners.lock().await);
        for (_, runtime) in listeners {
            if let Some(public) = runtime.public {
                public.stop().await;
            }
            runtime.local.stop().await;
        }
    }

    pub async fn listener_status(&self) -> serde_json::Value {
        let listeners = self.listeners.lock().await;
        let origins = self.tunnel_origins.read().await;
        serde_json::Value::Array(listeners.iter().map(|(port, runtime)| serde_json::json!({
            "port": port, "local_origin": format!("http://localhost:{port}"),
            "public_origin": origins.get(port),
            "public_status": if runtime.public.is_none() { "not_started" } else if origins.contains_key(port) { "ready" } else { "reconnecting" },
        })).collect())
    }
}
