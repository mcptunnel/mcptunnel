# Client compatibility — 2026-10-08

mcptunnel 0.2.2 bridges current stateless HTTP requests to real legacy stdio servers. This has passed automated gateway tests. **No listed AI client was tested end to end.** No client account, API credential, external authorization server or public tunnel was configured during these tests.

## Evidence levels and every combination

The complete [CSV matrix](CLIENT_COMPATIBILITY.csv) has **2,880 unique combinations**: eight client surfaces × four revisions × three transports × three authentication modes × ten operations. OpenAI API/Codex and Claude Code/Desktop are separated because their configuration and hosting differ.

Each row has three independent statuses:

1. **Official documented capability:** exact revision/transport/auth/operation combination. Generic HTTP documentation does not prove a specific revision, so these exact combinations remain NOT TESTED unless explicitly unsupported. The table below records generic documented capabilities separately.
2. **Automated protocol test:** an executed gateway/stdio fixture, independent of the named AI client. Repeated PASS rows across clients represent the same gateway evidence, not eight client runs.
3. **Actual client E2E:** NOT TESTED for available combinations. Paid managed mode is UNSUPPORTED because there is no backend; token validation in public mode is UNSUPPORTED because it has no tokens.

PASS means the specified check passed. FAIL means an executed check failed. UNSUPPORTED means an unavailable or inapplicable feature. NOT TESTED means no evidence establishes that exact combination. Missing positive coverage is not treated as PASS. Full conformance failures are recorded separately in [CONFORMANCE_RESULTS.json](CONFORMANCE_RESULTS.json).

The matrix's Quick and independent transport tests use a loopback HTTP listener with the corresponding gateway mode. They **do not test Cloudflare's edge or an external reverse proxy**. Its long-running PASS checks the real 60-second timeout and cancellation, not unlimited execution. OAuth PASS uses locally signed JWTs and injected cached issuer keys; it does not test TLS discovery, PKCE, CIMD, consent or refresh. Reconnect PASS for legacy local mode is the official session lifecycle fixture, not provider reconnection.

Regenerate or verify the matrix:

```sh
node scripts/compatibility-matrix.mjs
node scripts/compatibility-matrix.mjs --check
```

## Generic official documentation

Retrieved 2026-10-08. PASS here means documentation explicitly supplies the generic configuration, not that mcptunnel or the newest revision has been tested in that client.

| Client surface | HTTP documented | No auth documented | Explicit auth header documented | OAuth documented | Exact 2026-07-28 E2E |
|---|---|---|---|---|---|
| [ChatGPT](https://developers.openai.com/api/docs/guides/custom-mcp-server) | PASS | PASS | NOT TESTED for Basic | PASS | NOT TESTED |
| [OpenAI Agents API](https://developers.openai.com/api/docs/guides/agents-api/tools/mcp) | PASS | PASS | PASS: HTTP transport headers | PASS: supplied access token; interactive flow NOT TESTED | NOT TESTED |
| [Codex](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) | PASS | PASS | PASS: configured headers | PASS | NOT TESTED |
| [Claude remote](https://claude.com/docs/connectors/custom/add-unlisted) | PASS | PASS | PASS: limited beta organizations | PASS | NOT TESTED |
| [Claude Code](https://code.claude.com/docs/en/mcp) | PASS | PASS | PASS: HTTP `--header` | PASS | NOT TESTED |
| [Claude Desktop remote](https://claude.com/docs/connectors/custom/add-unlisted) | PASS | PASS | PASS: same limited connector beta | PASS | NOT TESTED |
| [Cursor](https://cursor.com/docs/mcp) | PASS | PASS | PASS: headers | PASS | NOT TESTED |
| [VS Code / Copilot](https://code.visualstudio.com/docs/agent-customization/mcp-servers) | PASS | PASS | PASS: headers | PASS | NOT TESTED |

Plan, organization and client version restrictions can change. Record the exact client version and negotiated protocol when performing E2E. Resources and prompts passing the gateway fixture does not establish a corresponding client UI.

## Copy/paste connection configurations

First create a mount using [README quick start](../README.md). Replace `https://RANDOM.trycloudflare.com/demo/mcp` with the full URL printed by creation. Use the **header value**, such as `Basic BASE64_CREDENTIALS`, without the `Authorization:` label. These templates follow documented client interfaces but have not been executed against accounts.

### Codex

Add to your user or trusted project `config.toml`. Supply `MCPTUNNEL_AUTHORIZATION` in the environment of the Codex process with the full Basic header value. The client supports `env_http_headers`; this keeps credentials out of the file. [Configuration reference](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).

```toml
[mcp_servers.demo]
url = "https://RANDOM.trycloudflare.com/demo/mcp"
env_http_headers = { Authorization = "MCPTUNNEL_AUTHORIZATION" }
```

For an external OAuth mount, omit the fixed header and use the client's documented MCP login. Its authorization server must support the client's registration flow and issue tokens for the current mount URL.

### Claude Code

```sh
claude mcp add --transport http demo https://RANDOM.trycloudflare.com/demo/mcp --header "Authorization: Basic BASE64_CREDENTIALS"
```

For external OAuth, add the HTTP URL without a fixed Authorization header, then use `/mcp` to authenticate. A fixed invalid header must be corrected rather than assumed to trigger OAuth. [CLI reference](https://code.claude.com/docs/en/mcp).

### Cursor

Save in `.cursor/mcp.json`. Set the environment variable before starting Cursor. For external OAuth, omit the Basic header and follow the client's OAuth setup. [Remote configuration](https://cursor.com/docs/mcp).

```json
{
  "mcpServers": {
    "demo": {
      "url": "https://RANDOM.trycloudflare.com/demo/mcp",
      "headers": { "Authorization": "${env:MCPTUNNEL_AUTHORIZATION}" }
    }
  }
}
```

### VS Code / GitHub Copilot

Save in `.vscode/mcp.json`. The input asks privately for the full header value. For external OAuth, omit the fixed header/input and use the client's authentication prompt. [HTTP and input configuration](https://code.visualstudio.com/docs/agent-customization/mcp-servers).

```json
{
  "servers": {
    "demo": {
      "type": "http",
      "url": "https://RANDOM.trycloudflare.com/demo/mcp",
      "headers": { "Authorization": "${input:mcptunnelAuthorization}" }
    }
  },
  "inputs": [
    {
      "id": "mcptunnelAuthorization",
      "type": "promptString",
      "description": "mcptunnel full Basic Authorization value",
      "password": true
    }
  ]
}
```

### OpenAI API

For the current **Agents API**, configure the nested HTTP transport in the MCP tool at session creation. Use `headers.Authorization` for Basic. For OAuth use `authorization: "Bearer ACCESS_TOKEN"` instead; use one Authorization source. Anonymous mounts omit both. [API transport reference](https://developers.openai.com/api/docs/guides/agents-api/tools/mcp).

```json
{
  "type": "mcp",
  "server_label": "demo",
  "transport": {
    "type": "http",
    "server_url": "https://RANDOM.trycloudflare.com/demo/mcp",
    "headers": { "Authorization": "Basic BASE64_CREDENTIALS" }
  },
  "required": true
}
```

This is an Agents API fragment; do not assume identical fields in other API surfaces. API invocation, billing and token acquisition were NOT TESTED.

### ChatGPT

Use the custom MCP connection flow with the full HTTPS URL. Select external OAuth for a private mount or no authentication only for an explicitly public mount. Basic configuration is NOT TESTED; do not put credentials in the URL or change the mount to public implicitly. [Connection reference](https://developers.openai.com/api/docs/guides/custom-mcp-server), [auth reference](https://developers.openai.com/plugins/build/auth).

### Claude remote / Desktop remote connector

In Customize → Connectors, add the full HTTPS URL. For OAuth select sign-in and configure the external authorization server's client identity/registration. In organizations with the Request headers beta, choose No sign-in and set `Authorization` to the full `Basic BASE64_CREDENTIALS` value. If that beta is absent, use OAuth. OAuth connections reserve Authorization. Desktop remote connectors use this flow; local desktop extensions are separate. [Settings reference](https://claude.com/docs/connectors/custom/add-unlisted).

## Reproducible wire check without a client account

The automated tests create real listeners and spawn `tests/fixtures/legacy.cjs` using Node 22. They exercise Basic and public requests, three legacy revisions, per-request process isolation, crash recovery, policy, JSON/SSE, malformed input and cancellation. OAuth HTTP tests live in `src/auth/oauth.rs`.

For an existing `demo` fixture mount, save this as `request.json`:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "counter",
    "arguments": {},
    "_meta": {
      "io.modelcontextprotocol/protocolVersion": "2026-07-28",
      "io.modelcontextprotocol/clientInfo": { "name": "wire-check", "version": "1" },
      "io.modelcontextprotocol/clientCapabilities": {}
    }
  }
}
```

```sh
curl --no-buffer "https://RANDOM.trycloudflare.com/demo/mcp" \
  -H "Authorization: Basic BASE64_CREDENTIALS" \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -H "MCP-Protocol-Version: 2026-07-28" \
  -H "Mcp-Method: tools/call" -H "Mcp-Name: counter" \
  --data-binary @request.json
```

Use `curl.exe` on Windows if `curl` is a PowerShell alias. Current requests need no initialize or session ID. Headers must agree with the body; tools with `x-mcp-header` annotations additionally require their schema-declared `Mcp-Param-*` headers. The fixture's counter has no such annotation. Quick returns JSON; independent local mode can stream request progress when requested.

Legacy clients initialize with a supported revision, retain the returned `Mcp-Session-Id`, and send the negotiated `MCP-Protocol-Version` on later requests. The `legacy_versions_sessions_isolation_and_notifications` regression is the executable reference.

Run regression tests and the pinned official fixture using the commands in the [engineering report](ENGINEERING_REPORT.md). No automated login, paid provisioning, publication or external infrastructure changes are needed.

## Unsupported paths and outstanding evidence

| Path | Gateway status | Evidence |
|---|---|---|
| Quick request progress/SSE | UNSUPPORTED | JSON response regression; [provider limitation](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/) |
| Independent request progress/SSE | PASS in local fixture | Progress and disconnect regressions; official progress scenario |
| MRTR interactive continuations | UNSUPPORTED; full current checks FAIL | Full suite retains ten scored failures; no callback/continuation bridge |
| Tasks, subscriptions, sampling/elicitation, logging, MCP Apps | UNSUPPORTED | Not advertised; Tasks and legacy callback/subscription checks retain failures |
| Stable managed hostname | UNSUPPORTED | No trusted paid backend; provision/renew/revoke fail closed |
| External browser OAuth/CIMD/DCR/PKCE | NOT TESTED | Resource server is implemented; external AS/client flow unavailable |
| Live Quick edge, client URL update after reconnect, independent HTTPS proxy | NOT TESTED | URL parser/restart behavior tested with a fake provider process only |
| Linux/macOS platform run | NOT TESTED locally | Three-platform CI configured; not executed or pushed in this task |

Before claiming a real client PASS, record client build, account eligibility, revision, URL transport, auth mode and results for all ten operations. Use dedicated test credentials, retain redacted evidence, and explicitly record unsupported resource/prompt UI behavior. Changing a client's version requires re-verification.
