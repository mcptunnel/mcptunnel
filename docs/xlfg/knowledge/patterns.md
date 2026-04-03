# Patterns

- Admin API: newline-delimited JSON over Unix domain socket. Request = `AdminRequest` (tagged enum), Response = `AdminResponse`.
- Mount registry: `Arc<Mutex<Connection>>` shared across threads, all DB ops take the lock.
- Session manager: `Arc<RwLock<HashMap<String, Session>>>` in-memory, not persisted to DB during session lifetime.
- Upstream children: stored in `AppState.upstream_children: RwLock<HashMap<String, UpstreamChild>>` keyed by session ID.
- Policy filtering: always check deny list first, then allow list. Empty allow list = allow all.
- rmcp client pattern: `().serve(TokioChildProcess::new(cmd)?).await?` returns `RunningService<RoleClient, ()>` which derefs to `Peer<RoleClient>`.
