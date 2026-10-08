# mcptunnel

Bridge a local stdio MCP server to Streamable HTTP. Modern clients can invoke legacy stdio tools without initializing a remote session; legacy clients retain isolated initialization sessions.

**Free Quick mode uses temporary random URLs and synchronous JSON. Basic authentication is the default. Official managed stable hosting is unavailable until a trusted paid provisioning service exists.**

## Install

Download a ready-to-run binary. **No Rust, Cargo, or repository clone required.** The installer also installs Cloudflare's official `cloudflared` binary if it is missing. Both downloads are checked against SHA-256 digests before installation.

**Windows — PowerShell**

```powershell
irm https://github.com/mcptunnel/mcptunnel/releases/latest/download/install.ps1 | iex
mcptunnel --version
```

**macOS / Linux**

```sh
curl -fsSL https://github.com/mcptunnel/mcptunnel/releases/latest/download/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
mcptunnel --version
```

Windows installs into `%LOCALAPPDATA%\mcptunnel\bin` and adds it to your user PATH. macOS/Linux installs into `~/.local/bin`; add the shown export to your shell profile for future terminals. Neither installer needs administrator access.

Prefer a manual download? [Get the archive for your OS and CPU](https://github.com/mcptunnel/mcptunnel/releases/latest), check `SHA256SUMS`, extract it, and put the executable on PATH. Releases cover Windows x64, macOS Intel/Apple Silicon, and Linux x64/ARM64. Linux requires OpenSSL 3 and glibc 2.35+ on x64 (Ubuntu 22.04+) or glibc 2.39+ on ARM64 (Ubuntu 24.04+); Alpine/musl is not supported. MCP servers still require their own runtime. The example below requires [Node.js 22+](https://nodejs.org/en/download).

## Quick start

Expose the official [Filesystem MCP](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem) over HTTPS. This example shares only a new `shared` directory and allows two read-only tools.

**macOS / Linux**

```sh
mkdir -p shared
printf 'Hello from Filesystem MCP!\n' > shared/hello.txt
mcptunnel run --slug files --allow-tool read_text_file --allow-tool list_directory -- npx -y @modelcontextprotocol/server-filesystem@2026.8.31 "$PWD/shared"
```

**Windows — PowerShell**

```powershell
New-Item -ItemType Directory -Path shared -Force | Out-Null
Set-Content shared/hello.txt 'Hello from Filesystem MCP!'
mcptunnel run --slug files --allow-tool read_text_file --allow-tool list_directory -- cmd /d /c npx -y @modelcontextprotocol/server-filesystem@2026.8.31 "$PWD/shared"
```

The daemon starts automatically and opens a free Quick Tunnel. Connect your client using the printed URL and Authorization header, then ask it to list the shared directory or read `hello.txt` using its absolute path. File-writing tools are neither listed nor callable through this mount.

The package version is pinned to the real server used in our release verification. The first `npx` run downloads it. On a slow connection, cache it first with `npm exec --yes --package=@modelcontextprotocol/server-filesystem@2026.8.31 -- node --version`, then repeat the command.

Creation prints a plain `https://<random>.trycloudflare.com/<slug>/mcp` URL and an `Authorization: Basic ...` header once. Save the header privately. Inspection does not reveal credentials; recreate a Basic mount if you lose them. `--json` creation also contains credentials and must be stored privately. A pending URL means the provider has not announced an origin; inspect `mcptunnel tunnel status` and `mcptunnel doctor`.

Use the complete `/slug/mcp` URL and the creation header in a client supporting HTTP headers. Do not put credentials in URL userinfo. ChatGPT Basic support is unverified; use its documented external OAuth or explicit no-auth flow. Claude documents fixed Authorization headers for limited beta organizations. Actual client tests remain NOT TESTED. See [copy/paste client configurations and the evidence matrix](docs/CLIENT_COMPATIBILITY.md).

The mount captures the creating CLI's working directory; `--cwd` overrides it. Windows package launchers use `cmd /d /c`; executable paths and `node` work directly.

## Register, restrict, inspect, stop

```sh
mcptunnel list
mcptunnel inspect files
mcptunnel doctor files --json
mcptunnel logs files
mcptunnel stop files
mcptunnel daemon stop --force
```

All mounts share the daemon's provider. Deny rules override allow rules. `--hide-resources` and `--hide-prompts` block discovery and invocation; completion requests honor these policies too. Logs contain sanitized methods/statuses, not upstream stderr, credentials, request arguments, or tool output.

An intentionally public endpoint is explicit:

```sh
mcptunnel run --slug public-files --auth public --allow-tool read_text_file --allow-tool list_directory -- npx -y @modelcontextprotocol/server-filesystem@2026.8.31 "$PWD/shared"
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
mcptunnel run --slug private --auth oauth --oidc-issuer https://issuer.example --scope read --allow-tool read_text_file --allow-tool list_directory -- npx -y @modelcontextprotocol/server-filesystem@2026.8.31 "$PWD/shared"
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

Cloudflare documents Quick Tunnels as development-only and without SSE. URLs change on restart/reconnect, clients need the replacement URL, and there is no uptime guarantee. The official Filesystem MCP example passed actual public Quick HTTPS checks with Basic auth, discovery, read/list and write denial on Windows. A newly announced URL can briefly precede DNS readiness. [Provider documentation](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

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

Building from source is for contributors (Rust 1.88+, Node 22+):

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

Tests include real HTTP/stdio processes, Windows IPC, process-tree cleanup, and the real 60-second timeout. Linux, macOS and Windows CI plus the supported official conformance subset passed for 0.2.2. Each tagged release must repeat these checks and pass actual binary-installation and official Filesystem MCP checks on all five release targets before publication. Actual account-backed AI clients remain unverified.

Maintainers: see [release procedure and installer options](docs/RELEASING.md).

See [research](docs/RESEARCH.md), [protocol/security audit](docs/PROTOCOL_AUDIT.md), [client compatibility](docs/CLIENT_COMPATIBILITY.md), and [engineering report](docs/ENGINEERING_REPORT.md). Licensed under [MIT](LICENSE).
