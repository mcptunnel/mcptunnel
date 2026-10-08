use crate::mount::types::CommandConfig;
use rmcp::model::{ClientRequest, ProtocolVersion, RequestMetaObject};
use rmcp::service::{
    ClientLifecycleMode, ClientServiceExt, PeerRequestOptions, RoleClient, RunningService,
    ServiceError,
};
use rmcp::ErrorData;
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// SDK-owned stdio lifecycle; Auto probes discovery, then initializes legacy peers.
pub struct UpstreamChild {
    client: RunningService<RoleClient, ()>,
}

impl UpstreamChild {
    pub async fn spawn(
        config: &CommandConfig,
        cwd: Option<&str>,
        env_vars: &[(String, String)],
    ) -> Result<Self, crate::error::McpTunnelError> {
        let mut cmd = Command::new(&config.program);
        cmd.args(&config.args).kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        cmd.envs(env_vars.iter().cloned());
        // Upstream stderr may contain secrets; do not inherit it into daemon logs.
        let (transport, _) = rmcp::transport::TokioChildProcess::builder(cmd)
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|_| {
                crate::error::McpTunnelError::UpstreamSpawnFailed(
                    "cannot start configured stdio command".into(),
                )
            })?;
        let client = tokio::time::timeout(
            STARTUP_TIMEOUT,
            ().serve_with_lifecycle(
                transport,
                ClientLifecycleMode::Auto {
                    preferred_versions: vec![ProtocolVersion::V_2026_07_28],
                    legacy_version: Some(ProtocolVersion::V_2025_11_25),
                },
            ),
        )
        .await
        .map_err(|_| {
            crate::error::McpTunnelError::UpstreamProbeFailed(
                "stdio discovery/initialization timed out (15s)".into(),
            )
        })?
        .map_err(|_| {
            crate::error::McpTunnelError::UpstreamProbeFailed(
                "stdio discovery/initialization failed; run doctor".into(),
            )
        })?;
        Ok(Self { client })
    }

    pub fn peer(&self) -> &rmcp::Peer<RoleClient> {
        self.client.peer()
    }

    /// Preserve typed SDK results, errors and pagination; never forward HTTP credentials.
    pub async fn request(
        &self,
        method: &str,
        mut params: serde_json::Value,
        mut meta: RequestMetaObject,
        ct: &CancellationToken,
    ) -> Result<serde_json::Value, ErrorData> {
        if let Some(obj) = params.as_object_mut() {
            obj.remove("_meta");
        }
        // Our stdio client supports no interactive callbacks. Keep tracing/custom metadata only.
        meta.0.retain(|key, _| {
            !key.starts_with("io.modelcontextprotocol/") && key != "progressToken"
        });
        let request: ClientRequest =
            serde_json::from_value(serde_json::json!({"method": method, "params": params}))
                .map_err(|_| ErrorData::invalid_params("invalid MCP parameters", None))?;
        let handle = self
            .client
            .send_cancellable_request(
                request,
                PeerRequestOptions::with_timeout(REQUEST_TIMEOUT).with_meta(meta),
            )
            .await
            .map_err(map_error)?;
        let upstream_id = handle.id.clone();
        let result = tokio::select! {
            biased;
            _ = ct.cancelled() => {
                let notification = rmcp::model::CancelledNotificationParam::new(Some(upstream_id), Some("downstream cancelled".into()));
                let _ = self.client.notify_cancelled(notification).await;
                return Err(ErrorData::internal_error("request cancelled", None));
            }
            result = handle.await_response() => result.map_err(map_error)?,
        };
        serde_json::to_value(result)
            .map_err(|_| ErrorData::internal_error("invalid upstream result", None))
    }

    pub async fn shutdown(mut self) {
        let _ = self.client.close_with_timeout(Duration::from_secs(3)).await;
    }
    pub fn cancel(&self) {
        self.client.cancellation_token().cancel();
    }
}

fn map_error(error: ServiceError) -> ErrorData {
    match error {
        ServiceError::McpError(error) => error,
        ServiceError::Timeout { .. } => {
            ErrorData::internal_error("upstream request timed out (60s)", None)
        }
        _ => ErrorData::internal_error(
            "upstream disconnected or returned an invalid result; reconnect to retry",
            None,
        ),
    }
}
