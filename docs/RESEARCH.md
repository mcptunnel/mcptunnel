# Research record

Retrieved 2026-10-08. Official documentation establishes documented capability; it does not establish an account-backed client test.

| Evidence | Verified fact and implementation consequence |
|---|---|
| [Specification index](https://modelcontextprotocol.io/specification/) and [2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28) | The index resolves to 2026-07-28, the current stable revision on the retrieval date. |
| [Changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog), [versioning](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning), [release announcement](https://blog.modelcontextprotocol.io/posts/2026-07-28/) | Current requests declare protocol version and client capabilities in per-request `_meta`; discovery replaces mandatory initialization. Earlier revisions use initialization. |
| [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http) | Validate method/name/parameter headers against the body. Current protocol has no session header requirement; notifications return 202. POST can return JSON or request-scoped SSE. GET need not be supported. |
| [Authorization](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization) | Protected-resource metadata, resource-bound tokens, issuer validation, scopes, and HTTPS matter. PKCE and CIMD belong to the authorization server/client flow. DCR is a compatibility fallback, not a reason to implement a permissive authorization server. |
| [Rust SDK release](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.5.1) | rmcp 3.5.1 is the verified stable release; its MSRV is Rust 1.88. It supplies stateless HTTP, typed results, modern metadata/header validation, and automatic modern-to-legacy client lifecycle negotiation. |
| [Official conformance](https://github.com/modelcontextprotocol/conformance) | The published stable npm package 0.1.16 lacks the required `--requirements` option. Tests use official source pinned to commit `c37eec888e1c6ff140af79987a40008548b7cc5f` (package 0.2.0-alpha.12), which supports both requirement sets. These are source snapshot results, not results from a stable published conformance package. |
| [Cloudflare Quick Tunnels](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/) | Temporary random URLs, development use, concurrent request limits, and no SSE. mcptunnel must retain synchronous JSON operation and cannot promise streaming or uptime on this provider. |
| [OpenAI custom MCP](https://developers.openai.com/api/docs/guides/custom-mcp-server), [plugin auth](https://developers.openai.com/plugins/build/auth), [API remote MCP](https://developers.openai.com/api/docs/guides/agents-api/tools/mcp) | Remote HTTP connections and OAuth/no-auth integration are documented. This does not verify the client's negotiated revision or every resource/prompt UI. Basic-auth support is not inferred from a URL field. |
| [Codex MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) | HTTP server configuration and authorization headers/token configuration are documented. Per-client current-revision behavior still requires a real test. |
| [Claude remote connectors](https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp), [custom connector settings](https://claude.com/docs/connectors/custom/add-unlisted), [Claude Code MCP](https://code.claude.com/docs/en/mcp) | Remote connector and HTTP CLI flows are documented. Fixed Authorization headers, including Basic, are documented for limited beta organizations. OAuth uses its own Authorization header. Desktop local extensions and remote connectors have different connection flows; account restrictions apply. |
| [Cursor MCP](https://cursor.com/docs/mcp), [VS Code MCP](https://code.visualstudio.com/docs/agent-customization/mcp-servers) | HTTP configuration is documented, including configurable headers in developer clients. Transport support does not prove 2026-07-28 support. |

## Corroborating security research and community evidence

[MCP security landscape research](https://arxiv.org/abs/2503.23278) motivates treating upstream processes and tool output as separate trust boundaries. It is supplementary research, not a normative protocol source. The code audit independently found credential URLs, unbound legacy sessions, and a permissive memory-backed authorization server.

[Cursor community report on dropped headers](https://forum.cursor.com/t/streamable-http-mcp-mcp-json-headers-besides-x-domainhost-key-are-dropped-on-tools-call/169015) describes client header behavior. It is unverified community evidence, not a compatibility PASS. Together with the normative header requirements and SDK inspection, it motivates reproducible header consistency tests and recording the exact client version during future E2E tests.

No community post or paper is used to override the official standard. Sources agree that transport connectivity, authorization correctness, and negotiated protocol revision require separate evidence.

## SDK migration decisions

- Replace custom HTTP dispatch with `StreamableHttpService`, `NeverSessionManager`, and `ServerHandler`.
- Use `ClientLifecycleMode::Auto` to discover modern stdio peers and initialize older ones. Remote modern calls are adapted to typed legacy requests; transport credentials and modern capability claims are not forwarded to legacy children.
- Add modern completion discriminator/server metadata to legacy results. Keep upstream content, schemas, structured content, pagination, custom metadata, and JSON-RPC error data.
- Wrap SDK transport validation only for application authentication, isolated legacy session ownership, limits, and the SDK's consistent unknown-version-header discrepancy (`-32022` versus `-32020`).
- Optional callbacks, Tasks, subscriptions, and MCP Apps are not advertised. Review their support separately from synchronous operations.

The old SDK migration is validated by compiling every target and exercising a real stdio child over HTTP. No documentation claim substitutes for those checks.

## Binary distribution and real quick start — 2026-10-08

The official [Filesystem server](https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem) documents npx installation, Windows cmd launchers and the read_text_file/list_directory tools. npm reported package 2026.8.31; the example and release smoke pin that exact package. The actual Windows packaged binary and README launcher passed Basic authentication, modern stateless tool discovery, reading/listing a directory with spaces, denied writes and unauthenticated rejection.

[Playwright MCP](https://github.com/microsoft/playwright-mcp) is another widely used candidate, corroborated by this [community discussion](https://www.reddit.com/r/MCPservers/comments/1wc06hk/opensource_mcp_of_the_week_37k_stars_playwright/). Its official documentation emphasizes persistent browser state. Filesystem was selected for the first example because its reads are independently meaningful with the gateway's modern per-request process isolation. No Playwright end-to-end PASS is claimed.

The earlier [security landscape paper](https://arxiv.org/abs/2503.23278), repository process-isolation tests and official server behavior support retaining a narrow read-only tool policy rather than exposing an unrestricted filesystem example. Scholarly/community material is supporting context; executable server checks provide the compatibility evidence.

[GitHub runner documentation](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) provides native x64/ARM64 runners; [release CLI documentation](https://cli.github.com/manual/gh_release_create) supports uploading assets before publishing. [Cloudflared 2026.10.0](https://github.com/cloudflare/cloudflared/releases/tag/2026.10.0) supplies the official per-asset SHA-256 digests pinned by the installers. Windows download/hash/version checks actually passed. Checksums protect download integrity; unsigned release checksums are not independent publisher signatures.
