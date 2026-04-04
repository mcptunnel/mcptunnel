# Patterns

- Admin API: newline-delimited JSON over Unix domain socket. Request = `AdminRequest` (tagged enum), Response = `AdminResponse`.
- Mount registry: `Arc<Mutex<Connection>>` shared across threads, all DB ops take the lock.
- Session manager: `Arc<RwLock<HashMap<String, Session>>>` in-memory, not persisted to DB during session lifetime.
- Upstream children: stored in `AppState.upstream_children: RwLock<HashMap<String, UpstreamChild>>` keyed by session ID.
- Policy filtering: always check deny list first, then allow list. Empty allow list = allow all.
- rmcp client pattern: `().serve(TokioChildProcess::new(cmd)?).await?` returns `RunningService<RoleClient, ()>` which derefs to `Peer<RoleClient>`.
- Auth check pattern in gateway: match on `mount.auth_mode` then call `check_basic_auth` / `check_oauth_auth`. Applied to POST, GET, and DELETE handlers. OAuth check covers all methods including initialize (no exemptions).
- Credential comparison: always use `subtle::ConstantTimeEq` for auth secrets; never plain `==`.
- OAuth built-in vs external: `mount.oauth_config.builtin` flag determines mode. Built-in uses `state.oauth_server.signing_key.decoding_key` for validation; external uses JWKS cache.
- HTML template safety: always use `html_escape()` for user-controlled values in HTML templates to prevent XSS.
- DB migration pattern: `ALTER TABLE ... ADD COLUMN` guarded by a test SELECT in `init_db`. Column index in `row_to_mount` must be updated when adding columns.
- Tunnel readiness pattern: `AppState::wait_for_tunnel_ready(timeout)` polls `tunnel_origin` every 200ms up to timeout. Quick mode only; Managed mode returns immediately. Called before `public_url()` in `handle_mount_create` and `handle_mount_inspect`. NOT in `handle_mount_list` (uses `pending:` URL fallback).
