# mcptunnel

A local MCP gateway that turns any **stdio MCP server** into a **public HTTPS endpoint** — one command, one tunnel, multiple servers.

```bash
mcptunnel -- npx -y ollama-mcp
```

```
Mount created: ollama
Tunnel mode: quick
Auth mode: public
Public MCP URL:
https://bold-leaf-42.trycloudflare.com/ollama/mcp
```

Paste the URL into [ChatGPT](https://chatgpt.com), [Claude Desktop](https://claude.ai), or any MCP client. Done.

## Why

You have a local MCP server. You want to share it over the internet — maybe with ChatGPT, maybe with a teammate, maybe with yourself from another machine. You don't want to set up nginx, deal with certificates, or write a proxy.

mcptunnel handles:

- Spawning and probing your MCP server
- Running a local HTTP gateway that speaks MCP Streamable HTTP
- Managing a Cloudflare Tunnel for public HTTPS
- Routing multiple servers through one tunnel
- Session isolation (one child process per remote session)
- Tool/resource/prompt policy filtering
- Optional OAuth endpoint protection

## Install

### From source

```bash
git clone https://github.com/flrngel/mcptunnel.git
cd mcptunnel
cargo install --path .
```

### Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- [cloudflared](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/) installed and on PATH

## Quick start

### Expose a single MCP server

```bash
mcptunnel -- npx -y ollama-mcp
```

The daemon starts automatically, creates a Cloudflare quick tunnel, probes the MCP server, and prints the public URL.

### Expose multiple servers on one tunnel

```bash
mcptunnel --slug painter -- npx -y @anthropic/mcp-painter
mcptunnel --slug search  -- npx -y @anthropic/mcp-search
```

Both share the same tunnel and hostname:

```
https://<random>.trycloudflare.com/painter/mcp
https://<random>.trycloudflare.com/search/mcp
```

### Check what's running

```bash
mcptunnel list
```

```
SLUG             STATUS     TUNNEL   AUTH     SESS TOOLS URL
painter          ready      quick    public   0    3     https://bold-leaf-42.trycloudflare.com/painter/mcp
search           ready      quick    public   0    5     https://bold-leaf-42.trycloudflare.com/search/mcp
```

### Inspect a mount

```bash
mcptunnel inspect painter
```

### View logs

```bash
mcptunnel logs painter
mcptunnel logs painter --follow
```

### Stop a mount

```bash
mcptunnel stop painter
```

### Run diagnostics

```bash
mcptunnel doctor
mcptunnel doctor painter
```

## Tunnel modes

### Quick (default)

```bash
mcptunnel -- npx -y my-mcp-server
```

- Zero setup — just needs `cloudflared` installed
- Random `*.trycloudflare.com` subdomain
- No SSE support (POST/DELETE only)
- URL changes on restart
- For **development and testing**

### Managed

```bash
mcptunnel run \
  --tunnel managed \
  --hostname mcp.example.com \
  --slug painter \
  -- npx -y my-mcp-server
```

- Stable hostname with your own domain
- Full SSE streaming support
- Requires `cloudflared` login and tunnel setup
- For **production and sustained use**

## Security

### Policy filtering

Control which tools, resources, and prompts are exposed:

```bash
# Only expose specific tools
mcptunnel run --allow-tool paint.draw --allow-tool paint.fill -- npx -y mcp-painter

# Block dangerous tools
mcptunnel run --deny-tool exec --deny-tool shell -- npx -y my-server

# Hide resources and prompts entirely
mcptunnel run --hide-resources --hide-prompts -- npx -y my-server
```

Policy is enforced at both **listing time** and **invocation time** — denied tools don't appear in `tools/list` and direct `tools/call` attempts are rejected without forwarding upstream.

### Auth modes

| Mode | Flag | Behavior |
|------|------|----------|
| **public** (default) | `--auth public` | No auth. Prominent warning printed. |
| **inherit** | `--auth inherit` | Preserves upstream auth metadata as-is. |
| **oauth** | `--auth oauth` | Gateway-managed OAuth with JWT validation. |

### OAuth example

```bash
mcptunnel run \
  --auth oauth \
  --oidc-issuer https://auth.example.com \
  --scope painter.use \
  --tunnel managed \
  --hostname mcp.example.com \
  -- npx -y mcp-painter
```

- Validates JWT signatures via JWKS auto-discovery
- Checks issuer, audience, expiry, and scopes
- Returns `401` + `WWW-Authenticate` for unauthenticated requests
- Exposes `/.well-known/oauth-protected-resource` metadata

## Architecture

```
CLI (clap) ──> Daemon ──> HTTP Gateway (axum)
                 │              │
                 │         /<slug>/mcp ──> Session Manager ──> Upstream Adapter (rmcp)
                 │                                                    │
                 │                                              stdio MCP child
                 │
                 ├── Mount Registry (SQLite)
                 ├── Tunnel Manager (cloudflared)
                 ├── Policy / Auth Overlay
                 └── Admin API (Unix Domain Socket)
```

- **Single binary** — daemon forks from the same `mcptunnel` binary
- **One tunnel, many mounts** — path-prefix routing (`/<slug>/mcp`)
- **One child per session** — each remote MCP session gets its own stdio child process
- **Loopback only** — daemon binds `127.0.0.1`, Cloudflare handles public exposure

## CLI reference

| Command | Description |
|---------|-------------|
| `mcptunnel -- <cmd>` | Mount a server (shorthand for `run`) |
| `mcptunnel run [opts] -- <cmd>` | Mount with options |
| `mcptunnel list [--json]` | List all mounts |
| `mcptunnel inspect <slug> [--json]` | Detailed mount info |
| `mcptunnel logs <slug> [--follow]` | View mount logs |
| `mcptunnel stop <slug>` | Stop a mount |
| `mcptunnel doctor [slug] [--json]` | Run diagnostics |
| `mcptunnel daemon status` | Show daemon state |
| `mcptunnel daemon start` | Start daemon in foreground |
| `mcptunnel daemon stop` | Stop daemon |
| `mcptunnel tunnel status` | Show tunnel state |
| `mcptunnel gc` | Clean up stale state |

All list/inspect/doctor/status commands support `--json` for scripting.

## Configuration

Optional config file at `~/.config/mcptunnel/config.toml`:

```toml
[daemon]
bind = "127.0.0.1:43123"
auto_start = true

[tunnel]
default_mode = "quick"
cloudflared_path = "cloudflared"

[defaults]
auth_mode = "public"
session_idle_ttl = "15m"
session_hard_ttl = "12h"
max_sessions_per_mount = 8
log_level = "info"
```

## How it works

1. You run `mcptunnel -- <your mcp server command>`
2. The daemon auto-starts if not running
3. A Cloudflare quick tunnel starts, giving you a public HTTPS URL
4. Your command is probed to verify it speaks MCP
5. A mount is created at `/<slug>/mcp`
6. When a remote client POSTs to the public URL:
   - A new session is created
   - A child process is spawned running your command
   - MCP requests are bridged between HTTP and stdio
   - The session ID is returned in `MCP-Session-Id`
7. Idle sessions are evicted after 15 minutes

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `2` | CLI usage error |
| `10` | Daemon unavailable |
| `11` | cloudflared missing |
| `12` | Tunnel creation failed |
| `20` | Upstream spawn failed |
| `21` | Upstream probe failed |
| `22` | Mount not found |

## License

MIT
