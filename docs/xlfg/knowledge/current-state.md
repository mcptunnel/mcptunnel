# Current state — 2026-10-22

This file supersedes the pre-modernization feature inventory. See [the user guide](../../USER_GUIDE.md) and [engineering report](../../ENGINEERING_REPORT.md) for behavior and dated evidence.

- One Rust binary runs the CLI and daemon. Local administrative IPC uses Unix sockets or Windows named pipes; local HTTP is the default, and `--public` explicitly adds a Cloudflare Quick Tunnel.
- Exact mount paths and per-port listeners support both local and public routes. Quick ingress excludes local mounts. Persistent mount configuration does not make temporary provider URLs stable.
- rmcp 3.5.1 bridges modern stateless requests and supported legacy initialized sessions to stdio. Modern operations start a fresh process: upstream in-memory history does not survive across calls. Legacy sessions reuse one child bound to the mount and authenticated principal.
- Basic authentication is the default, with a generated 128-bit password. Creation prints header credentials once; URLs and inspection do not expose passwords. Explicit `--auth public` disables authentication.
- OAuth is external only: HTTPS issuer discovery/JWKS and RS256 access-token validation. The external authorization server owns login, consent, registration, PKCE and refresh. Built-in OAuth and inherit auth fail closed. Browser authorization flows are unverified; one existing Codex account passed a Basic-authenticated MCP connection.
- POST supports synchronous JSON and, on capable independent hosting, request-scoped progress SSE. GET subscription streams return 405. Quick suppresses progress and uses synchronous JSON.
- Policy filters tool discovery/invocation, resources, prompts and completion. Upstream processes retain the operator's permissions; this is not an OS sandbox.
- Tasks, subscriptions/replay, interactive MRTR, sampling/elicitation callbacks, logging and MCP Apps are unsupported. Managed hosting is unavailable. Full conformance does not pass.

Historical 0.3.1 evidence covers 66 Rust tests per platform and five native candidates. It is not proof for subsequent source changes; current focused checks belong in the engineering report. GitHub Actions is disabled pending explicit authorization.
