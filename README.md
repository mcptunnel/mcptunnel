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

Use the official [Filesystem MCP server](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem) for a read-only first connection. Install [Node.js 22+](https://nodejs.org/en/download), then create a directory containing only data you intend to share:

```sh
mkdir mcptunnel-demo
printf 'Hello from mcptunnel!\n' > mcptunnel-demo/hello.txt
mcptunnel --allow-tool read_text_file --allow-tool list_directory -- npx -y @modelcontextprotocol/server-filesystem@2026.8.31 ./mcptunnel-demo
# → http://localhost:8000/filesystem
```

In PowerShell, replace the `printf` line with `Set-Content mcptunnel-demo/hello.txt 'Hello from mcptunnel!'`. Connect with the printed Basic header, list the directory, and read `hello.txt`. The allowlist blocks write tools. File contents persist on disk between requests; the example does not depend on process memory. Use `--path /files`, `--port 9000`, or `--public` before `--` to change the endpoint or add temporary public HTTPS.

The package version is deliberately pinned to the recorded verified example. npm marks this date-based release deprecated because the package moved to semantic versions; this is not a recommendation to install an untested `latest`. The historical Node 22 verification and current local check are described in the [engineering report](docs/ENGINEERING_REPORT.md#pre-public-documentation-check--2026-10-22).

**Stateful-server limit:** modern (`2026-07-28`) requests start a fresh upstream process for every operation. In-memory conversation/history state does **not** survive between calls. Sequential Thinking is therefore unsuitable as this quick start. Legacy clients using a supported initialized session reuse one process until that session ends. [Protocol and state limits](docs/USER_GUIDE.md#protocol-and-limits).

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

Automatic name collisions get a suffix such as `/filesystem-2`. Explicit duplicate paths on the same port fail. Public and local servers can share a port; only public servers enter the Cloudflare tunnel. [Routing and persistence details](docs/USER_GUIDE.md#ports-and-paths).

## Manage your servers

```sh
mcptunnel list
mcptunnel inspect filesystem
mcptunnel doctor filesystem
mcptunnel stop filesystem
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

Contributions are welcome. Building from source requires Rust 1.88.0 (pinned by `rust-toolchain.toml`) and Node.js 22+. See the [local verification commands](docs/USER_GUIDE.md#verification-and-contributing) and [repository instructions](AGENTS.md) before submitting a PR.

Licensed under [MIT](LICENSE).
