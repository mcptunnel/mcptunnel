# Current State

## Architecture
Single Rust binary (`mcptunnel`) with subcommand dispatch. Daemon runs HTTP gateway (axum) + UDS admin API. CLI commands communicate with daemon over Unix domain socket.

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

## Test Coverage
35 unit tests covering: config parsing, policy filtering, session lifecycle, mount CRUD, slug generation, auth helpers, logging utilities, tunnel URL parsing.

## Known Gaps
- Full JWT signature validation requires JWKS fetching/caching (current OAuth gate checks token format only)
- SSE stream sends keepalives; server-initiated notifications from upstream not yet forwarded
- `logs` command shows guidance only (no log streaming from daemon)
- Tunnel is not auto-started as part of daemon startup (must be started via future tunnel management)
