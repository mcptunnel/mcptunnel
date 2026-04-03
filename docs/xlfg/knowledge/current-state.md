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
  auth/{mod,public,oauth,types}.rs
```

## Key Crates
- rmcp 1.3 (client + transport-child-process) — upstream stdio MCP client
- axum 0.8 — HTTP gateway
- rusqlite 0.32 (bundled) — SQLite persistence
- clap 4 — CLI
- tokio 1 — async runtime
- tracing — structured logging
- jsonwebtoken 9 — JWT validation
- tokio-stream — SSE streaming

## Test Coverage
36 unit tests covering: config parsing, policy filtering, session lifecycle, mount CRUD, slug generation, auth helpers, logging utilities, tunnel URL parsing, JWKS cache.

## Feature Completeness
- CLI: all 9 subcommands implemented (run, list, inspect, logs, stop, doctor, daemon, tunnel, gc)
- Daemon: auto-start from CLI, session eviction, UDS admin API
- Tunnel: quick mode auto-starts on daemon startup with supervision and restart
- Gateway: POST/GET/DELETE on /<slug>/mcp, SSE with keepalive + notification sender infrastructure
- Auth: public (noauth injection), inherit, OAuth (JWKS-backed JWT validation, scope checking)
- Policy: tool/resource/prompt filtering at list and invoke time
- Observability: structured logging, per-mount log ring buffer, doctor diagnostics, inspect

## No Remaining Gaps
All spec features are implemented. The SSE notification forwarding infrastructure is in place
(per-session SSE senders registered in AppState, send_sse_event API available). Full upstream
server-initiated notification capture would require a custom rmcp ClientHandler implementation
which is an enhancement beyond the spec's MVP scope.
