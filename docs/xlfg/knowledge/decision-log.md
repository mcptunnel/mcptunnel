# Decision Log

<!-- Key architectural and design decisions. Format: date, decision, rationale. -->

- **2026-04-03**: Default auth mode changed from `public` to `basic` (HTTP Basic Auth, RFC 7617). Rationale: safe-by-default; auto-generated credentials with zero setup. OAuth too complex for default; custom token non-standard per MCP spec; Basic Auth is standard HTTP, embeddable in URL. Credentials: `mct` + 16-char hex password.
- **2026-04-03**: Credentials excluded from `MountList` response; shown only in `MountCreate` and `MountInspect`. Rationale: prevent accidental credential leakage in list operations.
