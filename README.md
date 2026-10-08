# mcptunnel

Bridge a local stdio MCP server to Streamable HTTP. Modern clients can invoke legacy stdio tools without initializing a remote session; legacy clients retain isolated initialization sessions.

**Free Quick mode uses temporary random URLs and synchronous JSON. Basic authentication is the default. Official managed stable hosting is unavailable until a trusted paid provisioning service exists.**

## Install and free quick start

Requirements: Rust 1.88+, your stdio server runtime, and [cloudflared](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/). Node 22+ is needed for repository tests and the demo fixture.

```sh
git clone https://github.com/mcptunnel/mcptunnel.git
cd mcptunnel
cargo install --locked --path .
mcptunnel -- node tests/fixtures/legacy.cjs
```

Replace `node tests/fixtures/legacy.cjs` with your installed stdio server command. The daemon starts automatically, probes the server, and opens a free Quick Tunnel. The demo is a deterministic test fixture, not a production tool service.

Creation prints a plain `https://<random>.trycloudflare.com/<slug>/mcp` URL and an `Authorization: Basic ...` header once. Save the header privately. Inspection does not reveal credentials; recreate a Basic mount if you lose them. `--json` creation also contains credentials and must be stored privately. A pending URL means the provider has not announced an origin; inspect `mcptunnel tunnel status` and `mcptunnel doctor`.

Use the complete `/slug/mcp` URL and the creation header in a client supporting HTTP headers. Do not put credentials in URL userinfo. ChatGPT Basic support is unverified; use its documented external OAuth or explicit no-auth flow. Claude documents fixed Authorization headers for limited beta organizations. Actual client tests remain NOT TESTED. See [copy/paste client configurations and the evidence matrix](docs/CLIENT_COMPATIBILITY.md).

Windows: use `node`, an absolute executable path, or the full `npx.cmd` path when a package launcher is not found. The mount captures the creating CLI's working directory; `--cwd` overrides it. The daemon uses a local named pipe on Windows and a restricted Unix socket on Linux/macOS.

No binary release or registry publication was performed by this modernization. CI produces build artifacts; source installation is the verified installation path.

## Register, restrict, inspect, stop

```sh
mcptunnel run --slug demo --allow-tool echo -- node tests/fixtures/legacy.cjs
mcptunnel list
mcptunnel inspect demo
mcptunnel doctor demo --json
mcptunnel logs demo
mcptunnel stop demo
mcptunnel daemon stop --force
```

All mounts share the daemon's provider. Deny rules override allow rules. `--hide-resources` and `--hide-prompts` block discovery and invocation; completion requests honor these policies too. Logs contain sanitized methods/statuses, not upstream stderr, credentials, request arguments, or tool output.

An intentionally public endpoint is explicit:

```sh
mcptunnel run --slug public-demo --auth public --allow-tool echo -- node tests/fixtures/legacy.cjs
```

Anyone with that URL can invoke the exposed operations. Public mode is not selected automatically for client compatibility.

## Protocol and limits

| Connection | Implemented behavior |
|---|---|
| 2026-07-28 | Per-request version/client capabilities metadata, `server/discover`, no mandatory initialize or protocol session ID |
| 2025-11-25 / 2025-06-18 / 2025-03-26 | Initialize, returned session ID, mount/principal binding, session cleanup |
| Modern HTTP to legacy stdio | SDK discovery/initialization negotiation; typed request adaptation; preserved schemas, content, structured content, pagination, errors and relevant metadata |
| Quick Tunnel | Synchronous JSON only; progress suppressed; temporary random origin |
| Independently hosted local mode | JSON and request-scoped progress SSE; proxy must support streaming |

Modern operations spawn an isolated stdio process per request. Legacy sessions reuse one process each. This protects user/session state at the cost of process startup on every modern operation. Upstream processes run with the operator's permissions; the gateway is not an OS sandbox. Process groups/jobs terminate descendants during cleanup.

Limits: 1 MiB HTTP body, 10-second body timeout, 4 MiB stdio message, 15-second startup, 60-second request, 64 global requests, default 8 requests per mount/session, default 8 sessions per mount, and 256 sessions globally. Discovery is bounded to 4,096 entries, 64 pages and 16 MiB per catalog. Request timeout and SSE disconnect cancellation are verified. Reinitialize a legacy session after its upstream crashes.

Tasks, subscriptions/replay, interactive MRTR, sampling/elicitation callbacks, logging, and MCP Apps are unsupported. They are not advertised. GET subscription streams return 405. No deprecated HTTP+SSE transport is added. Independent request-scoped progress does not imply subscription or interactive support.

The gateway passes the explicitly selected supported conformance subset. **Full conformance does not pass:** current scored checks are 93 pass/10 fail, previous stable checks 53 pass/8 fail. Remaining failures concern interactive MRTR and unimplemented logging/callback/subscription scenarios. [Exact results and reproducible commands](docs/ENGINEERING_REPORT.md) are retained; this is not full protocol certification.

## External OAuth

The insecure built-in authorization server has been removed. Existing built-in OAuth and inherit-auth mounts fail closed. Recreate them with Basic, explicit public mode, or an external issuer:

```sh
mcptunnel run --slug private --auth oauth --oidc-issuer https://issuer.example --scope read -- node tests/fixtures/legacy.cjs
```

Configure your authorization server to issue **RS256 JWT access tokens** whose audience is the exact current full MCP URL. Issuer, subject, audience, expiration, not-before and scopes are validated. Discovery/JWKS uses HTTPS, bounded documents and issuer-scoped cached keys. Tokens never enter stdio parameters or environment variables.

Protected-resource metadata is available at `/.well-known/oauth-protected-resource/<slug>/mcp`. Your external authorization server owns PKCE, CIMD/registration, consent and refresh tokens. Signing/registration persistence is its responsibility. Tokens for a former Quick URL are rejected after a URL change. Opaque tokens, introspection, other signing algorithms, and an embedded login service are not implemented. Local JWT/HTTP checks pass; external browser authorization and account-backed connector flows remain NOT TESTED.

## Independent hosting and paid managed hosting

For your own reverse proxy/domain, use a configuration file:

```toml
[daemon]
bind = "127.0.0.1:43123"
[tunnel]
default_mode = "local"
external_origin = "https://mcp.your-domain.example"
```

Point `MCPTUNNEL_CONFIG` at this file, restart the daemon, and create mounts with `--tunnel local`. Route your HTTPS origin to the loopback bind, preserving Host/auth/MCP headers and disabling proxy buffering for SSE. Recreate mounts when changing their transport mode. `MCPTUNNEL_STATE_DIR` optionally isolates state; otherwise `doctor` reports platform paths. Invalid configuration fails closed.

Independent infrastructure is allowed by the MIT license and needs no mcptunnel account or plan. Quick mode rejects `external_origin` and hostname assignment. Official `--tunnel managed` fails with an unavailable-backend error: a local plan flag cannot prove paid entitlement. [Tunnel architecture and future server-side provision/renew/revoke boundary](docs/TUNNEL_PLANS.md).

Cloudflare documents Quick Tunnels as development-only and without SSE. URLs change on restart/reconnect, clients need the replacement URL, and there is no uptime guarantee. Live public Quick connectivity was not exercised in this task. [Provider documentation](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

## Troubleshooting

| Symptom | Action |
|---|---|
| 400 / -32020 | Match protocol/method/name/annotated parameter headers to the body; current requests require per-request `_meta` |
| 400 / -32022 | Select one of the four supported revisions; keep the negotiated legacy session version |
| 401 | Supply the creation Basic header or a valid resource-bound external JWT; do not use credential URLs |
| 403 | Check Host/Origin and required OAuth scopes |
| JSON-RPC -32010 | The mount policy denies this operation; check allow/deny and resource/prompt settings |
| 404 | Use the full mount URL; reinitialize an expired legacy session; check unsupported method names |
| 405 / 406 / 415 | GET subscriptions are unsupported; POST accepts JSON plus SSE in Accept and requires application/json Content-Type |
| 429 | Reduce active requests/sessions or review configured limits |
| Missing provider URL | Run doctor/tunnel status; install cloudflared and check network access |
| Upstream failure | Verify executable path, cwd/env and clean JSON-RPC stdout; use eager probing or doctor |
| OAuth reconnect fails | Authorize for the replacement Quick resource URL; verify AS PKCE/CIMD/registration configuration |

## Verification and contributing

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Tests include real HTTP/stdio processes, Windows IPC, process-tree cleanup, and the real 60-second timeout. CI runs these checks on Linux, macOS and Windows and the supported official conformance subset on Linux. Linux/macOS CI execution and account-backed clients remain unverified until those runs occur.

See [research](docs/RESEARCH.md), [protocol/security audit](docs/PROTOCOL_AUDIT.md), [client compatibility](docs/CLIENT_COMPATIBILITY.md), and [engineering report](docs/ENGINEERING_REPORT.md). Licensed under [MIT](LICENSE).
