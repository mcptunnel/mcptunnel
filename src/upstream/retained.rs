//! Application-owned upstream contexts. These are never MCP wire sessions.
use super::{
    child::{UpstreamChild, UpstreamFailure},
    progress::ProgressSink,
};
use crate::{
    gateway::security::AuthenticatedOwner,
    mount::types::MountRecord,
    session::{
        budget::{ContextBudget, Reservation},
        types::SessionConfig,
    },
};
use rmcp::{
    model::{ErrorCode, ErrorData, RequestMetaObject},
    service::RequestContext,
    RoleServer,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::{mpsc, oneshot, watch, OwnedSemaphorePermit, Semaphore},
    time::{sleep_until, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum Status {
    Starting,
    Ready,
    Closing(&'static str),
    Lost(&'static str),
    StartupFailed,
}
type SharedReservation = Arc<Mutex<Option<Arc<Reservation>>>>;
struct Entry {
    sender: mpsc::Sender<Operation>,
    gate: Arc<Semaphore>,
    status: Arc<Mutex<Status>>,
    cancel: CancellationToken,
    done: watch::Receiver<bool>,
    _reservation: SharedReservation,
}
struct MountContexts {
    generation: u64,
    closing: bool,
    restored: bool,
    entries: HashMap<AuthenticatedOwner, Arc<Entry>>,
}
#[derive(Default)]
struct RegistryState {
    stopping: bool,
    mounts: HashMap<uuid::Uuid, MountContexts>,
}
pub struct RetainedRegistry {
    inner: Mutex<RegistryState>,
    config: SessionConfig,
    budget: Arc<ContextBudget>,
}
pub enum AcquireError {
    Busy,
    State(ErrorData),
}
pub struct RetainedLease {
    entry: Arc<Entry>,
    _permit: OwnedSemaphorePermit,
    slug: String,
}
struct Operation {
    method: String,
    params: Value,
    meta: RequestMetaObject,
    ct: CancellationToken,
    sink: Option<ProgressSink>,
    reply: oneshot::Sender<Result<Value, ErrorData>>,
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
struct Completion(watch::Sender<bool>);
impl Drop for Completion {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

pub fn state_lost(slug: &str, reason: &'static str) -> ErrorData {
    ErrorData::new(
        ErrorCode(-32030),
        "upstream state lost; explicitly reset the mount",
        Some(json!({"reason":reason,"action":format!("mcptunnel reset {slug}")})),
    )
}
impl RetainedRegistry {
    pub fn new(config: SessionConfig, budget: Arc<ContextBudget>) -> Self {
        Self {
            inner: Mutex::new(RegistryState::default()),
            config,
            budget,
        }
    }
    pub fn register(&self, id: uuid::Uuid, restored: bool) {
        self.inner
            .lock()
            .unwrap()
            .mounts
            .entry(id)
            .or_insert_with(|| MountContexts {
                generation: 0,
                closing: false,
                restored,
                entries: HashMap::new(),
            });
    }
    pub fn acquire(
        &self,
        mount: &MountRecord,
        owner: &AuthenticatedOwner,
    ) -> Result<Arc<RetainedLease>, AcquireError> {
        if !matches!(
            mount.auth_mode,
            crate::config::AuthModeConfig::Basic | crate::config::AuthModeConfig::OAuth
        ) || matches!(owner, AuthenticatedOwner::Public)
        {
            return Err(AcquireError::State(ErrorData::invalid_request(
                "keep-alive requires authenticated Basic or OAuth ownership",
                None,
            )));
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.stopping {
            return Err(AcquireError::State(state_lost(
                &mount.slug,
                "daemon_stopping",
            )));
        }
        let Some(contexts) = inner.mounts.get_mut(&mount.id) else {
            return Err(AcquireError::State(state_lost(
                &mount.slug,
                "mount_stopped",
            )));
        };
        if contexts.closing {
            return Err(AcquireError::State(state_lost(&mount.slug, "resetting")));
        }
        if contexts.restored {
            return Err(AcquireError::State(state_lost(
                &mount.slug,
                "daemon_restarted",
            )));
        }
        contexts.entries.retain(|_, entry| {
            !(matches!(*entry.status.lock().unwrap(), Status::StartupFailed)
                && *entry.done.borrow())
        });
        if !contexts.entries.contains_key(owner) {
            let reservation = Arc::new(Mutex::new(Some(
                self.budget
                    .reserve(&mount.slug)
                    .map_err(|_| AcquireError::Busy)?,
            )));
            let (sender, receiver) = mpsc::channel(1);
            let (completion, done) = watch::channel(false);
            let entry = Arc::new(Entry {
                sender,
                gate: Arc::new(Semaphore::new(1)),
                status: Arc::new(Mutex::new(Status::Starting)),
                cancel: CancellationToken::new(),
                done,
                _reservation: reservation.clone(),
            });
            tokio::spawn(worker(
                mount.clone(),
                self.config.clone(),
                receiver,
                entry.status.clone(),
                entry.cancel.clone(),
                Completion(completion),
                reservation,
            ));
            contexts.entries.insert(owner.clone(), entry);
        }
        let entry = contexts.entries.get(owner).unwrap().clone();
        match *entry.status.lock().unwrap() {
            Status::Closing(reason) | Status::Lost(reason) => {
                return Err(AcquireError::State(state_lost(&mount.slug, reason)))
            }
            Status::StartupFailed => return Err(AcquireError::Busy),
            _ => {}
        }
        let permit = entry
            .gate
            .clone()
            .try_acquire_owned()
            .map_err(|_| AcquireError::Busy)?;
        Ok(Arc::new(RetainedLease {
            entry,
            _permit: permit,
            slug: mount.slug.clone(),
        }))
    }
    /// Fence first, await owners outside the registry lock, then retire the generation.
    pub async fn reset(&self, id: uuid::Uuid, remove: bool) -> bool {
        let (generation, entries) = {
            let mut inner = self.inner.lock().unwrap();
            let Some(mount) = inner.mounts.get_mut(&id) else {
                return false;
            };
            mount.closing = true;
            mount.generation += 1;
            (
                mount.generation,
                mount.entries.values().cloned().collect::<Vec<_>>(),
            )
        };
        for entry in &entries {
            entry.cancel.cancel();
        }
        futures::future::join_all(entries.iter().map(|entry| async move {
            let mut done = entry.done.clone();
            while !*done.borrow_and_update() {
                if done.changed().await.is_err() {
                    break;
                }
            }
        }))
        .await;
        let mut inner = self.inner.lock().unwrap();
        if let Some(mount) = inner.mounts.get_mut(&id) {
            if mount.generation == generation {
                mount.entries.clear();
                mount.restored = false;
                mount.closing = false;
                if remove {
                    inner.mounts.remove(&id);
                }
            }
        }
        true
    }
    pub async fn shutdown(&self) {
        let ids = {
            let mut inner = self.inner.lock().unwrap();
            inner.stopping = true;
            inner.mounts.keys().copied().collect::<Vec<_>>()
        };
        futures::future::join_all(ids.into_iter().map(|id| self.reset(id, true))).await;
    }
    pub fn summary(&self, id: uuid::Uuid) -> Value {
        let inner = self.inner.lock().unwrap();
        let Some(mount) = inner.mounts.get(&id) else {
            return json!({"contexts":0,"states":[]});
        };
        let states = mount
            .entries
            .values()
            .map(|e| match *e.status.lock().unwrap() {
                Status::Starting => json!({"status":"starting"}),
                Status::Ready => json!({"status":"ready"}),
                Status::Closing(reason) => json!({"status":"closing","reason":reason}),
                Status::Lost(reason) => json!({"status":"lost","reason":reason}),
                Status::StartupFailed => json!({"status":"startup_failed"}),
            })
            .collect::<Vec<_>>();
        json!({"contexts":states.len(),"generation":mount.generation,"needs_reset":mount.restored,"reason":mount.restored.then_some("daemon_restarted"),"states":states})
    }
}
impl RetainedLease {
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        context: RequestContext<RoleServer>,
        sink: Option<ProgressSink>,
    ) -> Result<Value, ErrorData> {
        let (reply, response) = oneshot::channel();
        let ct = context.ct.child_token();
        let _cancel = CancelOnDrop(ct.clone());
        self.entry
            .sender
            .try_send(Operation {
                method: method.into(),
                params,
                meta: context.meta,
                ct,
                sink,
                reply,
            })
            .map_err(|_| state_lost(&self.slug, "upstream_uncertain"))?;
        response
            .await
            .unwrap_or_else(|_| Err(state_lost(&self.slug, "upstream_uncertain")))
    }
}

async fn worker(
    mount: MountRecord,
    config: SessionConfig,
    mut receiver: mpsc::Receiver<Operation>,
    status: Arc<Mutex<Status>>,
    cancel: CancellationToken,
    _completion: Completion,
    reservation: SharedReservation,
) {
    let mut child: Option<UpstreamChild> = None;
    let mut created = Instant::now();
    let mut last_completed = created;
    let reason = loop {
        let closed = child
            .as_ref()
            .map(UpstreamChild::closed)
            .unwrap_or_default();
        let deadline = if child.is_some() {
            (last_completed + config.idle_ttl).min(created + config.hard_ttl)
        } else {
            Instant::now() + config.idle_ttl
        };
        let operation = tokio::select! {
            biased;
            _ = cancel.cancelled() => break "resetting",
            _ = closed.cancelled(), if child.is_some() => break "upstream_crashed",
            _ = sleep_until(deadline) => {
                if child.is_none() { *status.lock().unwrap() = Status::StartupFailed; reservation.lock().unwrap().take(); return; }
                break if Instant::now() >= created + config.hard_ttl { "hard_expired" } else { "idle_expired" };
            },
            op = receiver.recv() => match op { Some(op) => op, None => break "mount_stopped" },
        };
        if child.is_none() {
            created = Instant::now();
            let started = tokio::select! {
                biased;
                _ = cancel.cancelled() => { let _ = operation.reply.send(Err(state_lost(&mount.slug, "resetting"))); break "resetting"; },
                _ = operation.ct.cancelled() => { *status.lock().unwrap() = Status::StartupFailed; reservation.lock().unwrap().take(); return; },
                _ = sleep_until(created + config.hard_ttl) => { let _ = operation.reply.send(Err(state_lost(&mount.slug,"hard_expired"))); break "hard_expired"; },
                result = UpstreamChild::spawn(&mount.command, mount.cwd.as_deref(), &mount.env_vars) => result,
            };
            match started {
                Ok(started) => {
                    child = Some(started);
                    *status.lock().unwrap() = Status::Ready;
                }
                Err(_) => {
                    *status.lock().unwrap() = Status::StartupFailed;
                    reservation.lock().unwrap().take();
                    let _ = operation.reply.send(Err(ErrorData::internal_error(
                        "upstream startup failed; run mcptunnel doctor",
                        None,
                    )));
                    return;
                }
            }
        }
        if operation.ct.is_cancelled() {
            let _ = operation
                .reply
                .send(Err(state_lost(&mount.slug, "upstream_uncertain")));
            break "upstream_uncertain";
        }
        let upstream = child.as_ref().unwrap();
        let outcome = tokio::select! {
            biased;
            _ = cancel.cancelled() => { let _ = operation.reply.send(Err(state_lost(&mount.slug, "resetting"))); break "resetting"; },
            _ = sleep_until(created + config.hard_ttl) => { let _ = operation.reply.send(Err(state_lost(&mount.slug, "hard_expired"))); break "hard_expired"; },
            result = upstream.request_with_progress_detailed(&operation.method, operation.params, operation.meta, &operation.ct, operation.sink) => result,
            _ = operation.ct.cancelled() => { let _ = operation.reply.send(Err(state_lost(&mount.slug, "upstream_uncertain"))); break "upstream_uncertain"; },
        };
        match outcome {
            Ok(value) => {
                let _ = operation.reply.send(Ok(value));
            }
            Err(UpstreamFailure::Rpc(error)) => {
                let _ = operation.reply.send(Err(error));
            }
            Err(error) => {
                let reason = error.loss_reason();
                let _ = operation.reply.send(Err(state_lost(&mount.slug, reason)));
                break reason;
            }
        }
        last_completed = Instant::now();
    };
    *status.lock().unwrap() = Status::Closing(reason);
    if let Some(child) = child {
        child.shutdown().await;
    }
    *status.lock().unwrap() = Status::Lost(reason);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{AuthModeConfig, TunnelMode, UpstreamLifetime},
        mount::{
            db,
            types::{CommandConfig, MountCreateParams},
            MountRegistry,
        },
        policy::types::PolicyConfig,
        session::SessionManager,
    };
    async fn call(
        lease: &RetainedLease,
        name: &str,
        ct: CancellationToken,
    ) -> Result<Value, ErrorData> {
        let (reply, response) = oneshot::channel();
        lease
            .entry
            .sender
            .try_send(Operation {
                method: "tools/call".into(),
                params: json!({"name":name}),
                meta: RequestMetaObject::default(),
                ct,
                sink: None,
                reply,
            })
            .unwrap_or_else(|_| panic!("worker unavailable"));
        response.await.unwrap()
    }
    async fn wait_lost(lease: &RetainedLease) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if matches!(*lease.entry.status.lock().unwrap(), Status::Lost(_)) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn retained_lifecycle_fences_loss_resets_and_shares_budget() {
        let temp = tempfile::tempdir().unwrap();
        let events = temp.path().join("events");
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        let mounts = MountRegistry::new(Arc::new(Mutex::new(conn)));
        let mount = mounts
            .create(MountCreateParams {
                upstream_lifetime: UpstreamLifetime::Retained,
                slug: Some("browser".into()),
                port: 8000,
                path: None,
                display_name: None,
                command: CommandConfig {
                    program: "node".into(),
                    args: vec![format!(
                        "{}/tests/fixtures/legacy.cjs",
                        env!("CARGO_MANIFEST_DIR")
                    )],
                },
                cwd: None,
                tunnel_mode: TunnelMode::Local,
                hostname: None,
                auth_mode: AuthModeConfig::Basic,
                basic_credentials: None,
                oauth_config: None,
                policy: PolicyConfig::default(),
                env_vars: vec![
                    (
                        "FIXTURE_EVENTS".into(),
                        events.to_string_lossy().into_owned(),
                    ),
                    ("FIXTURE_SLOW_MS".into(), "200".into()),
                ],
            })
            .unwrap();
        let config = SessionConfig {
            idle_ttl: std::time::Duration::from_secs(1),
            hard_ttl: std::time::Duration::from_secs(10),
            max_sessions_per_mount: 1,
            ..Default::default()
        };
        let legacy = SessionManager::new(config.clone());
        let registry = RetainedRegistry::new(config, legacy.budget.clone());
        registry.register(mount.id, true);
        let owner = AuthenticatedOwner::Basic(mount.id);
        assert!(matches!(
            registry.acquire(&mount, &owner),
            Err(AcquireError::State(_))
        ));
        registry.reset(mount.id, false).await;
        let failed_mount = MountRecord {
            id: uuid::Uuid::new_v4(),
            slug: "failed".into(),
            command: CommandConfig {
                program: "mcptunnel-intentionally-missing-executable".into(),
                args: vec![],
            },
            ..mount.clone()
        };
        registry.register(failed_mount.id, false);
        let failed = registry
            .acquire(&failed_mount, &AuthenticatedOwner::Basic(failed_mount.id))
            .ok()
            .unwrap();
        assert!(call(&failed, "counter", CancellationToken::new())
            .await
            .is_err());
        let released = legacy
            .create_session(&failed_mount.slug)
            .await
            .expect("initial spawn failure must free its reservation without reset");
        legacy.delete_session(&released.session_id).await;
        drop(released);
        drop(failed);
        registry.reset(failed_mount.id, true).await;
        let lease = registry.acquire(&mount, &owner).ok().unwrap();
        assert!(matches!(
            registry.acquire(&mount, &owner),
            Err(AcquireError::Busy)
        ));
        assert!(legacy.create_session(&mount.slug).await.is_err());
        assert_eq!(
            call(&lease, "counter", CancellationToken::new())
                .await
                .unwrap()["structuredContent"]["value"],
            1
        );
        let slow_lease = lease.clone();
        let slow =
            tokio::spawn(async move { call(&slow_lease, "slow", CancellationToken::new()).await });
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !std::fs::read_to_string(&events)
                .unwrap_or_default()
                .contains("call ")
            {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(2)).await;
        assert!(matches!(*lease.entry.status.lock().unwrap(), Status::Ready));
        assert!(
            !slow.is_finished(),
            "idle expiry must not terminate an active call"
        );
        tokio::time::resume();
        slow.await.unwrap().unwrap();
        tokio::task::yield_now().await;
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(2)).await;
        tokio::time::resume();
        wait_lost(&lease).await;
        assert!(matches!(
            *lease.entry.status.lock().unwrap(),
            Status::Lost("idle_expired")
        ));
        drop(lease);
        assert!(
            legacy.create_session(&mount.slug).await.is_err(),
            "lost tombstones remain bounded"
        );
        registry.reset(mount.id, false).await;
        let session = legacy.create_session(&mount.slug).await.unwrap();
        assert!(matches!(
            registry.acquire(&mount, &owner),
            Err(AcquireError::Busy)
        ));
        legacy.delete_session(&session.session_id).await;
        drop(session);
        let lease = registry.acquire(&mount, &owner).ok().unwrap();
        assert_eq!(
            call(&lease, "counter", CancellationToken::new())
                .await
                .unwrap()["structuredContent"]["value"],
            1
        );
        assert_eq!(
            call(&lease, "error", CancellationToken::new())
                .await
                .unwrap_err()
                .code,
            ErrorCode(-32602)
        );
        assert_eq!(
            call(&lease, "counter", CancellationToken::new())
                .await
                .unwrap()["structuredContent"]["value"],
            2
        );
        assert_eq!(
            call(&lease, "crash", CancellationToken::new())
                .await
                .unwrap_err()
                .code,
            ErrorCode(-32030)
        );
        wait_lost(&lease).await;
        drop(lease);
        registry.reset(mount.id, false).await;
        let lease = registry.acquire(&mount, &owner).ok().unwrap();
        call(&lease, "counter", CancellationToken::new())
            .await
            .unwrap();
        let ct = CancellationToken::new();
        let ct2 = ct.clone();
        let lease2 = lease.clone();
        let hang = tokio::spawn(async move { call(&lease2, "hang", ct2).await });
        tokio::task::yield_now().await;
        ct.cancel();
        assert_eq!(hang.await.unwrap().unwrap_err().code, ErrorCode(-32030));
        wait_lost(&lease).await;
        assert!(matches!(
            *lease.entry.status.lock().unwrap(),
            Status::Lost("upstream_uncertain")
        ));
        drop(lease);
        registry.reset(mount.id, false).await;
        let lease = registry.acquire(&mount, &owner).ok().unwrap();
        call(&lease, "counter", CancellationToken::new())
            .await
            .unwrap();
        let lease2 = lease.clone();
        let hang =
            tokio::spawn(async move { call(&lease2, "hang", CancellationToken::new()).await });
        tokio::task::yield_now().await;
        tokio::time::pause();
        tokio::time::advance(std::time::Duration::from_secs(11)).await;
        tokio::time::resume();
        let error = hang.await.unwrap().unwrap_err();
        assert_eq!(error.data.unwrap()["reason"], "hard_expired");
        wait_lost(&lease).await;
        drop(lease);
        registry.reset(mount.id, false).await;
        let lease = registry.acquire(&mount, &owner).ok().unwrap();
        call(&lease, "counter", CancellationToken::new())
            .await
            .unwrap();
        registry.shutdown().await;
        drop(lease);
        assert!(legacy.create_session(&mount.slug).await.is_ok());
        let pids = std::fs::read_to_string(events).unwrap();
        for line in pids.lines().filter(|s| s.starts_with("start ")) {
            let pid = line.split_whitespace().nth(1).unwrap();
            assert!(std::process::Command::new("node").args(["-e","try{process.kill(Number(process.argv[1]),0);process.exit(1)}catch{process.exit(0)}",pid]).status().unwrap().success(),"owned child survived cleanup");
        }
    }
}
