<p align="center">
  <img src="assets/mcptunnel-icon.svg" alt="mcptunnel" width="96" height="96" />
</p>

<h1 align="center">mcptunnel</h1>

<p align="center">
  Your stdio MCP servers, reachable over HTTP.
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/USER_GUIDE.md">Documentation</a>
</p>

Run an existing MCP server on **localhost**, or add **`--public`** for a temporary Cloudflare HTTPS URL. mcptunnel bridges stdio to Streamable HTTP and keeps your servers running in the background.

- **One command to start.** The daemon starts automatically.
- **One port, multiple servers.** Paths come from server names; choose your own path or port when needed.
- **Authentication by default.** Both local and public endpoints require Basic authentication.
- **Native binaries.** Windows, macOS and Linux. No Rust or Cargo required.

## Install

**Pre-release:** no public release is available yet. Repository collaborators can [download the verified 0.3.1 native candidate](https://github.com/mcptunnel/mcptunnel/actions/runs/37863330366) and [install it with automatic PATH setup](docs/USER_GUIDE.md#install-a-native-candidate).

<details>
<summary>One-command installers — available after the first public release</summary>

**macOS / Linux**

```sh
curl -fsSL https://github.com/mcptunnel/mcptunnel/releases/latest/download/install.sh | sh
```

**Windows — PowerShell**

```powershell
irm https://github.com/mcptunnel/mcptunnel/releases/latest/download/install.ps1 | iex
```

The installer verifies downloads, installs `cloudflared` if missing and configures your user PATH without administrator access. Existing PATH entries and shell settings are preserved. On macOS/Linux, open a new terminal or use the activation command printed by the installer.

```sh
mcptunnel --version
```

</details>

See [supported platforms and requirements](docs/USER_GUIDE.md#platform-requirements). Each MCP server also needs its own runtime.

## Quick start

Pass your MCP server command after `--`, with any mcptunnel options before it:

```text
mcptunnel [options] -- <mcp-command> [args...]
```

For example, run the official [Sequential Thinking MCP server](https://github.com/modelcontextprotocol/servers/tree/main/src/sequentialthinking). With [Node.js 22+](https://nodejs.org/en/download) installed, launch it directly:

```sh
# Localhost, with the default port and an automatic path.
mcptunnel -- npx -y @modelcontextprotocol/server-sequential-thinking

# Another server on the same port, at a path you choose.
mcptunnel --path /thinking -- npx -y @modelcontextprotocol/server-sequential-thinking

# Another port creates a separate listener.
mcptunnel --port 9000 -- npx -y @modelcontextprotocol/server-sequential-thinking

# Share a server over public HTTPS.
mcptunnel --public --path /public-thinking -- npx -y @modelcontextprotocol/server-sequential-thinking
```

These commands work in PowerShell, bash and zsh. Each registers a separate server and returns to your terminal. The first command prints:

```text
Mount created: sequential-thinking
Tunnel mode: local
Auth mode: basic
MCP URL:
http://localhost:8000/sequential-thinking

Authorization header (store privately; shown only at creation):
Authorization: Basic <authorization>
```

The final `--public` command prints a public MCP URL and its own Authorization header:

```text
https://<random>.trycloudflare.com/public-thinking
```

`<random>` and `<authorization>` are placeholders; actual output contains your URL and credentials. The first `npx` run downloads the MCP server. If that download delays startup, [cache the package and retry](docs/USER_GUIDE.md#ports-and-paths).

## Connect your client

Choose **Streamable HTTP**, paste the complete MCP URL, and set the `Authorization` header to the printed `Basic …` value. Save it privately; credentials are shown only at creation.

Local clients can use localhost. Remote services need the public HTTPS URL. See [client configurations](docs/CLIENT_COMPATIBILITY.md#copypaste-connection-configurations) for Codex, Claude Code, Cursor and other clients, including external OAuth. Account-backed client connections remain unverified.

## Choose your endpoint

All three flags are optional:

| Flag | Default | What it changes |
|---|---|---|
| `--public` | Off; localhost only | Adds temporary public HTTPS for this server |
| `--port <port>` | `8000` | Same port shares a listener; another port opens another listener |
| `--path <path>` | `/<server-name>` | Sets the exact MCP path, with no added `/mcp` suffix |

Automatic name collisions get a suffix such as `/sequential-thinking-2`. Explicit duplicate paths on the same port fail. Public and local servers can share a port; only public servers enter the Cloudflare tunnel. [Routing and persistence details](docs/USER_GUIDE.md#ports-and-paths).

## Manage your servers

```sh
mcptunnel list
mcptunnel inspect sequential-thinking
mcptunnel doctor sequential-thinking
mcptunnel stop sequential-thinking
```

Use the mount name printed at creation or by `list`. You can also allow or deny individual tools, restrict resources and prompts, and configure an external OAuth issuer. [Access controls and management](docs/USER_GUIDE.md#restrict-inspect-stop).

## Documentation

| Guide | Covers |
|---|---|
| [User guide](docs/USER_GUIDE.md) | Installation, routing, auth, hosting and troubleshooting |
| [Client configurations](docs/CLIENT_COMPATIBILITY.md) | Connection templates and compatibility evidence |
| [Protocol support](docs/USER_GUIDE.md#protocol-and-limits) | Supported revisions, transport behavior and limits |
| [Engineering report](docs/ENGINEERING_REPORT.md) | Test results, conformance gaps and security findings |
| [Releases](docs/RELEASING.md) | Native targets, installers and maintainer procedure |

Public Quick Tunnels are for development: URLs can change, SSE is unavailable, and uptime is not guaranteed. For a stable domain, use [your own HTTPS reverse proxy](docs/USER_GUIDE.md#independent-hosting). Managed hosting is unavailable. See [Cloudflare's Quick Tunnel documentation](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/).

## Contributing

Contributions are welcome. Building from source requires Rust 1.88+ and Node.js 22+. See the [local verification commands](docs/USER_GUIDE.md#verification-and-contributing) and [repository instructions](AGENTS.md) before submitting a PR.

Licensed under [MIT](LICENSE).
