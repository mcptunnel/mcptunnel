# User guide

Start with the [README quick start](../README.md#quick-start). This guide covers installation, endpoint behavior, access controls and operational limits.

[Installation](#installation) · [Ports and paths](#ports-and-paths) · [Clients](#connecting-clients) · [Access controls](#restrict-inspect-stop) · [OAuth](#external-oauth) · [Hosting](#independent-hosting) · [Protocol](#protocol-and-limits) · [Troubleshooting](#troubleshooting)

## Installation

**Pre-release:** no GitHub release has been published. The repository currently requires access. Collaborators can install the verified 0.3.1 native candidate below; anonymous one-command installation requires the first public release.

### Install a native candidate

1. Open the [verified candidate run](https://github.com/mcptunnel/mcptunnel/actions/runs/37863330366), then download the `release-<target>` artifact matching your platform from the table below. Artifact downloads require GitHub sign-in and repository access.
2. Extract the artifact ZIP. Keep the `mcptunnel-v0.3.1-<target>` archive and its `.sha256` sidecar together; do not unpack the inner archive yet.
3. Save the matching installer, [install.ps1](../install.ps1) or [install.sh](../install.sh), in that directory. Run the commands below from that directory. The installer checks the archive before installing and sets up PATH automatically.

**Windows — PowerShell**

```powershell
Copy-Item mcptunnel-v0.3.1-x86_64-pc-windows-msvc.zip.sha256 SHA256SUMS
& ./install.ps1 -Version v0.3.1 -AssetsDirectory .
mcptunnel --version
```

**macOS / Linux**

```sh
# Run in the extracted artifact directory, containing exactly one archive/sidecar.
cp mcptunnel-v0.3.1-*.sha256 SHA256SUMS
MCPTUNNEL_VERSION=v0.3.1 MCPTUNNEL_ASSET_DIR="$PWD" sh ./install.sh
```

Open a new terminal, then run `mcptunnel --version`, or use the activation command printed by the installer. The offline archive option still downloads `cloudflared` when missing; use `-SkipCloudflared` or `MCPTUNNEL_SKIP_CLOUDFLARED=1` for a fully offline local-only installation. A Windows execution policy that blocks local scripts must be handled according to your machine's policy.

### Public release installers

The commands below become available after the first public release. Download a ready-to-run binary. **No Rust, Cargo, or repository clone required.** The installer also installs Cloudflare's official `cloudflared` binary if it is missing. Both downloads are checked against SHA-256 digests before installation.

**Windows — PowerShell**

```powershell
irm https://github.com/mcptunnel/mcptunnel/releases/latest/download/install.ps1 | iex
mcptunnel --version
```

**macOS / Linux**

```sh
curl -fsSL https://github.com/mcptunnel/mcptunnel/releases/latest/download/install.sh | sh
```

Open a new terminal, then run `mcptunnel --version`. To keep using the current terminal, run the activation command printed by the installer instead.

Windows installs into `%LOCALAPPDATA%\mcptunnel\bin` and adds it to your user PATH. macOS/Linux installs into `~/.local/bin` and automatically registers it in your zsh, bash or fish startup files. Existing PATH entries and shell settings are preserved; reinstalling does not duplicate them. Neither installer needs administrator access.

After publication, you can also [download the archive for your OS and CPU](https://github.com/mcptunnel/mcptunnel/releases/latest), check `SHA256SUMS`, extract it, and put the executable on PATH manually.

### Platform requirements

| Platform | Artifact target | Runtime requirements |
|---|---|---|
| Windows x64 | `x86_64-pc-windows-msvc` | 64-bit Windows; also runs under Windows ARM64 emulation |
| macOS Intel | `x86_64-apple-darwin` | macOS 15+ |
| macOS Apple Silicon | `aarch64-apple-darwin` | macOS 14+ |
| Linux x64 | `x86_64-unknown-linux-gnu` | glibc 2.35+, OpenSSL 3; Ubuntu 22.04+ |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | glibc 2.39+, OpenSSL 3; Ubuntu 24.04+ |

Archives are not code-signed or Apple-notarized. Follow your operating system's normal approval procedure for downloaded executables. Alpine/musl and native Windows ARM64 builds are unavailable. MCP servers need their own runtime; the README quick start and Filesystem examples below use Node.js 22+.

## Ports and paths

| Option | Default | Behavior |
|---|---|---|
| `--public` | Off | Opens Cloudflare Quick HTTPS for this server; the local URL also remains available |
| `--port <port>` | `8000` | Same port shares one listener; another port creates another listener |
| `--path <path>` | `/<server-name>` | Exact MCP endpoint, such as `/team/files`; no extra `/mcp` suffix |

Paths are inferred from common package/executable names: Filesystem → `/filesystem`, `uvx mcp-server-git` → `/git`, `npx @playwright/mcp` → `/playwright`. An inferred collision becomes `/filesystem-2`, `/filesystem-3`, and so on. An explicitly duplicated path on the same port is an error. The same path on different ports is allowed. An occupied port produces an error with a `--port` hint. A custom `daemon.bind` changes the default port for that daemon.

Local and public servers may share a port. Only `--public` servers enter its Cloudflare ingress; local routes, inspection and OAuth metadata are excluded. Public servers on the same port share one temporary origin; different ports use separate origins. The last public server's removal stops its tunnel, and the last server's removal closes its listener. Ports, paths and visibility survive daemon restart; Quick origins can change. Existing pre-0.3 mounts retain their old port and `/slug/mcp` path.

The first `npx` run downloads the requested MCP server. On a slow connection, cache the README example first with `npm exec --yes --package=@modelcontextprotocol/server-sequential-thinking -- node --version`, then repeat the command. The Filesystem-specific examples below pin the version used in their recorded verification. The mount captures the creating CLI's working directory; `--cwd` overrides it. Windows `npx` launchers are resolved automatically.

## Connecting clients

Clients running on your machine can use localhost. Remote services need `--public` or your own HTTPS infrastructure. Use a client supporting HTTP Authorization headers for Basic. ChatGPT Basic support is unverified; use its documented external OAuth or an explicitly anonymous mount. [Client configurations and evidence](CLIENT_COMPATIBILITY.md).

Set the client's Streamable HTTP URL to the complete printed MCP URL and its `Authorization` header value to `Basic <authorization>` (without the `Authorization:` label). Each mount has separate credentials. Save the value privately: it is shown only when the mount is created, not by inspection. JSON creation output also contains credentials. Angle brackets in examples are placeholders.

Public mode prints the provider's announced URL; DNS can take a moment to become ready. If provisioning fails, registration fails and is rolled back. Only explicitly public mounts are exposed through Cloudflare; Basic authentication is still required unless you explicitly select anonymous authentication.

## Restrict, inspect, stop

For a read-only Filesystem endpoint, create a directory you intend to share and allow only the two intended tools:

```sh
mkdir shared
mcptunnel --slug readonly --path /readonly --allow-tool read_text_file --allow-tool list_directory -- npx -y @modelcontextprotocol/server-filesystem@2026.8.31 ./shared
mcptunnel list
mcptunnel inspect readonly
mcptunnel doctor readonly --json
mcptunnel logs readonly
mcptunnel stop readonly
mcptunnel daemon stop --force
```

Use the mount name printed by creation or `list` for management commands. `run` accepts the same options as the bare form. Deny rules override allow rules. `--hide-resources` and `--hide-prompts` block discovery and invocation; completion requests honor these policies too. Logs contain sanitized methods/statuses, not upstream stderr, credentials, request arguments, or tool output.

An anonymous public endpoint requires both choices explicitly:

```sh
mcptunnel --public --path /anonymous-files --auth public --allow-tool read_text_file --allow-tool list_directory -- npx -y @modelcontextprotocol/server-filesystem@2026.8.31 ./shared
```

`--public` selects exposure; `--auth public` selects no authentication. Anyone who obtains this URL can invoke its allowed operations.

## External OAuth

The insecure built-in authorization server has been removed. Existing built-in OAuth and inherit-auth mounts fail closed. Recreate them with Basic, explicit public mode, or an external issuer:

```sh
mcptunnel --public --path /private --auth oauth --oidc-issuer https://issuer.example --scope read --allow-tool read_text_file --allow-tool list_directory -- npx -y @modelcontextprotocol/server-filesystem@2026.8.31 "$PWD/shared"
```

Configure your authorization server to issue **RS256 JWT access tokens** whose audience is the exact current full MCP URL. Issuer, subject, audience, expiration, not-before and scopes are validated. Discovery/JWKS uses HTTPS, bounded documents and issuer-scoped cached keys. Tokens never enter stdio parameters or environment variables.

Protected-resource metadata is available at `/.well-known/oauth-protected-resource<path>`; for `/team/files`, use `/.well-known/oauth-protected-resource/team/files`. Your external authorization server owns PKCE, CIMD/registration, consent and refresh tokens. Signing/registration persistence is its responsibility. Tokens for a former Quick URL are rejected after a URL change. Opaque tokens, introspection, other signing algorithms, and an embedded login service are not implemented. Local JWT/HTTP checks pass; external browser authorization and account-backed connector flows remain NOT TESTED.

## Independent hosting

For your own reverse proxy/domain, use a configuration file:

```toml
[daemon]
bind = "127.0.0.1:8000"
[tunnel]
default_mode = "local"
external_origin = "https://mcp.your-domain.example"
```

Point `MCPTUNNEL_CONFIG` at this file, restart the daemon, and create local mounts on that port. Route your HTTPS origin to the loopback bind, preserving Host/auth/MCP headers and disabling proxy buffering for SSE. Recreate mounts when changing their exposure. `MCPTUNNEL_STATE_DIR` optionally isolates state; otherwise `doctor` reports platform paths. Invalid configuration fails closed.

Independent infrastructure is allowed by the MIT license and needs no mcptunnel account or plan. Quick mounts use only their temporary provider origin and reject hostname assignment. Official `--tunnel managed` fails with an unavailable-backend error: a local plan flag cannot prove paid entitlement. [Tunnel architecture and future server-side provision/renew/revoke boundary](TUNNEL_PLANS.md).

Cloudflare documents Quick Tunnels as development-only and without SSE. URLs change on restart/reconnect, clients need the replacement URL, and there is no uptime guarantee. The official Filesystem MCP example passed actual public Quick HTTPS checks with Basic auth, discovery, read/list and write denial on Windows. A newly announced URL can briefly precede DNS readiness. [Provider documentation](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

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

The gateway passes the explicitly selected supported conformance subset. **Full conformance does not pass:** current scored checks are 93 pass/10 fail, previous stable checks 53 pass/8 fail. Remaining failures concern interactive MRTR and unimplemented logging/callback/subscription scenarios. [Exact results and reproducible commands](ENGINEERING_REPORT.md) are retained; this is not full protocol certification.

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
| Port occupied / path duplicated | Choose `--port` / `--path`; explicit conflicts never silently change your endpoint |
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
cargo build --example fake_cloudflared
node scripts/smoke-routing.mjs
```

Tests include real HTTP/stdio processes, Windows IPC, process-tree cleanup, and the real 60-second timeout. Version 0.3.1 passed Linux/macOS/Windows CI: 66 Rust tests per platform, shared/multiple listener and public isolation checks, and 50 supported conformance scenarios. All five native targets passed real installation and Filesystem MCP checks; Unix targets also verified automatic persistent PATH in native shells. [Current verification and candidate checksums](INSTALLER_PATH_RESULTS.json) and [earlier routing/public Quick verification](LOCAL_ROUTING_RESULTS.json) are recorded. Each tagged release must repeat these checks and pass actual binary-installation and official Filesystem MCP checks on all five release targets before publication. Actual account-backed AI clients remain unverified.

Maintainers: see [release procedure and installer options](RELEASING.md).

See [research](RESEARCH.md), [protocol/security audit](PROTOCOL_AUDIT.md), [client compatibility](CLIENT_COMPATIBILITY.md), and [engineering report](ENGINEERING_REPORT.md). Licensed under [MIT](../LICENSE).
