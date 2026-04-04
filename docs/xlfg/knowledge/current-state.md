# Current State

## Architecture
Single Rust binary (`mcptunnel`) with subcommand dispatch. Daemon runs HTTP gateway (axum) + UDS admin API + auto-started cloudflared tunnel. CLI commands communicate with daemon over Unix domain socket.

## Module Layout
```
src/
  main.rs, config.rs, error.rs, logging.rs, doctor.rs
  cli/{mod,commands,output}.rs
  daemon/{mod,admin_api,state}.rs
  tunnel/{mod,quick,managed,supervisor}.rs
  mount/{mod,db,slug,types}.rs
  gateway/{mod,mcp_handler,inspection,health}.rs
  session/{mod,types}.rs
  upstream/{mod,probe,child}.rs
  policy/{mod,filter,types}.rs
  auth/{mod,public,oauth,oauth_server,types}.rs
```

## Key Crates
- rmcp 1.3 (client + transport-child-process) — upstream stdio MCP client
- axum 0.8 — HTTP gateway
- rusqlite 0.32 (bundled) — SQLite persistence
- clap 4 — CLI
- tokio 1 — async runtime
- tracing — structured logging
- jsonwebtoken 9 — JWT validation + signing
- rsa 0.9 — RSA key pair generation (pure Rust)
- tokio-stream — SSE streaming
- base64 0.22 — HTTP Basic Auth decoding
- subtle 2 — constant-time credential comparison

## Test Coverage
65 unit tests covering: config parsing, policy filtering, session lifecycle, mount CRUD, slug generation, auth helpers (public, basic, oauth, oauth_server), logging utilities, tunnel URL parsing, JWKS cache, basic auth credential generation/validation/DB round-trip, RSA key generation, JWT signing/verification, PKCE S256, DCR state, auth code lifecycle, HTML escaping/XSS prevention, tunnel-wait (ready/timeout/hot-path/managed-bypass).

## Feature Completeness
- CLI: all 9 subcommands implemented (run, list, inspect, logs, stop, doctor, daemon, tunnel, gc)
- Daemon: auto-start from CLI, session eviction, UDS admin API
- Tunnel: quick mode auto-starts on daemon startup with supervision and restart; `wait_for_tunnel_ready` in admin handlers ensures mount creation/inspect waits for tunnel URL (bounded 30s poll)
- Gateway: POST/GET/DELETE on /<slug>/mcp, SSE with keepalive + notification sender infrastructure
- Auth: **basic (default)**, public (noauth injection), inherit, OAuth (built-in AS + external AS)
  - Basic Auth: auto-generated credentials (mct + 16-char hex), constant-time validation (subtle crate), credentials-in-URL output
  - OAuth (built-in AS): full MCP-spec-compliant OAuth 2.1 Authorization Server — DCR (RFC 7591), PKCE S256, authorization code flow, JWT token issuance (RS256), JWKS endpoint, AS metadata (RFC 8414), Protected Resource Metadata (RFC 9728). Auto-generated user/password. Compatible with ChatGPT MCP integration.
  - OAuth (external AS): JWKS-backed JWT validation from external OIDC provider via `--oidc-issuer`
- Policy: tool/resource/prompt filtering at list and invoke time
- Observability: structured logging, per-mount log ring buffer, doctor diagnostics, inspect

## No Remaining Gaps
All spec features are implemented. The SSE notification forwarding infrastructure is in place
(per-session SSE senders registered in AppState, send_sse_event API available). Full upstream
server-initiated notification capture would require a custom rmcp ClientHandler implementation
which is an enhancement beyond the spec's MVP scope.
