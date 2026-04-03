# Patterns

- Admin API: newline-delimited JSON over Unix domain socket. Request = `AdminRequest` (tagged enum), Response = `AdminResponse`.
- Mount registry: `Arc<Mutex<Connection>>` shared across threads, all DB ops take the lock.
- Session manager: `Arc<RwLock<HashMap<String, Session>>>` in-memory, not persisted to DB during session lifetime.
- Upstream children: stored in `AppState.upstream_children: RwLock<HashMap<String, UpstreamChild>>` keyed by session ID.
- Policy filtering: always check deny list first, then allow list. Empty allow list = allow all.
- rmcp client pattern: `().serve(TokioChildProcess::new(cmd)?).await?` returns `RunningService<RoleClient, ()>` which derefs to `Peer<RoleClient>`.
- Auth check pattern in gateway: match on `mount.auth_mode` then call `check_basic_auth` / `check_oauth_auth`. Applied to POST, GET, and DELETE handlers.
- Credential comparison: always use `subtle::ConstantTimeEq` for auth secrets; never plain `==`.
- DB migration pattern: `ALTER TABLE ... ADD COLUMN` guarded by a test SELECT in `init_db`. Column index in `row_to_mount` must be updated when adding columns.
