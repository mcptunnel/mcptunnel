# Public readiness — 2026-10-22

This document records the 0.3.6 preparation audit. The subsequent authorized [v0.3.7 public release](https://github.com/mcptunnel/mcptunnel/releases/tag/v0.3.7) completed: all ten release jobs passed and anonymous public-URL installation plus installed-binary MCP checks succeeded. [Exact deployment evidence](PUBLIC_RELEASE_RESULTS.json) and [td#606](https://tdtd.dev/606) record completion. Statements below about no push/publication or a not-started release describe the preparation phase.

**Development preparation is complete for the documented synchronous scope. The audit established no remaining product defect from the two follow-ups below: one is environment-specific DNS behavior, the other is a release-stage operation.** The current local candidate is **0.3.6**, macOS ARM64. No release, tag, push, visibility change or GitHub Actions execution was performed in this work session.

The authoritative task document is [td#596](https://tdtd.dev/596). It was written before three Astra agents worked on separate documentation, cancellation and diagnostics files. Each problem has its own task; future features are not silently treated as launch defects.

## Now: corrected and verified

| Task | Problem / action | Result and evidence |
|---|---|---|
| [td#597](https://tdtd.dev/597) | Stateful Sequential Thinking was an unsafe first example for modern per-request isolation | Replaced with read-only, disk-backed Filesystem. Real discovery/read/list and denied writes pass. |
| [td#598](https://tdtd.dev/598) | Unpinned upstream and ambiguous runtime requirements | Filesystem pinned to 2026.8.31; Node requirements and date-version deprecation explicit. No unverified latest-package upgrade. |
| [td#599](https://tdtd.dev/599) | Current docs contradicted actual auth/protocol support | Removed built-in OAuth/inherit/GET subscription/full-support claims; dated historical evidence and current limitations. |
| [td#600](https://tdtd.dev/600) | Lint results varied with installed Rust | Rust 1.88.0 pinned with fmt/Clippy; strict checks pass. |
| [td#601](https://tdtd.dev/601) | Separate legacy cancellation did not reach active stdio calls | Authenticated mount/session/typed-ID routing, duplicate protection and cleanup implemented. Real HTTP/stdio regression passes. |
| [td#602](https://tdtd.dev/602) | Provider URL announcement obscured DNS/HTTP failures | Doctor independently checks local health, announcement, normal DNS and public HTTP, with actionable bounded failures. Real output: local HTTP PASS, public DNS WARN. DNS failure itself remains td#613. |
| [td#603](https://tdtd.dev/603) | No actual named AI-client evidence | Codex CLI 0.146.0, existing ChatGPT login, Basic/local HTTP, revision 2025-06-18: initialize, discover, read_text_file and DELETE pass. [Redacted result](CODEX_E2E_RESULTS.json). |
| [td#604](https://tdtd.dev/604) | External OAuth/hosting claims exceeded evidence | Signed-JWT HTTP/resource/scope/expiry/session tests pass; local routing passes. Browser authorization and independent HTTPS proxy remain explicitly unverified. |
| [td#605](https://tdtd.dev/605) | Current fixes lacked a native candidate | 0.3.6 macOS ARM64 release build, archive, real installer/reinstaller/PATH/tamper checks and real Filesystem smoke pass. |

## Follow-ups: environment and release execution

The original report incorrectly called both items launch blockers. Neither establishes unfinished product implementation.

| Task | Classification | What it means |
|---|---|---|
| [td#613](https://tdtd.dev/613) | Environment observation; optional investigation | This Mac's normal resolver failed for temporary Quick hostnames during the public test. The same public flow passed through an alternate resolver. This points to the local resolver/network path; repairing this Mac's DNS is not a prerequisite for publishing the project. Ordinary connectivity from this environment is still not claimed verified. |
| [td#606](https://tdtd.dev/606) | Release operation; not started | After publishing release assets, download through the public URL and confirm installation. The local archive installer already passed. This last deployment check cannot happen before a public artifact exists and is not a missing product feature. Existing authorization requirements for publishing/Actions still apply. [Release procedure](RELEASING.md#current-publication-gate). |

The preparation master is DONE. The historical 0.3.1 five-platform candidate lacks current fixes; current source verification is scoped to this Mac. Building the advertised release assets belongs to the explicitly requested release operation.

## What works and what is not established

| Area | Current evidence |
|---|---|
| Local CLI, persistent mounts, default Basic auth, shared/separate ports, exact paths | PASS; real CLI/HTTP/stdio routing smoke, restart and process cleanup |
| Modern synchronous tools/resources/prompts/completion | PASS in gateway fixtures; real official Filesystem tools pass. Modern upstream in-memory state resets on each request. |
| Legacy sessions and explicit cancellation | PASS in real HTTP/stdio regression; mount/principal/session/typed-ID isolation. Global HTTP admission limit still applies. |
| Local request progress/SSE | PASS in fixture; GET subscription streams remain unsupported. |
| Temporary public Quick | Provider connects, but ordinary public reachability is blocked here by DNS. Quick is synchronous JSON, development-only and has changing URLs. |
| External OAuth | Resource-server JWT/metadata/scope/expiry/session checks PASS. Browser consent, refresh, discovery/key-rotation integration and client registration flows NOT TESTED. |
| Actual AI client | Only Codex 0.146.0 / local HTTP / Basic / 2025-06-18 initialization, discovery and one read operation PASS. GET returns 405 without blocking that flow. |
| Full protocol conformance | Not achieved. Historical full-suite failures concern unimplemented interactive/optional features; selected supported 50-scenario results are historical, not a new run. |
| Managed hosting / billing | Unavailable and fail closed. |

## Executed verification and candidate

- Integrated runtime at **0.3.5**, commit `5058f31`: fmt, strict all-target/all-feature Clippy, **70 Rust tests** (59 unit + 1 CLI + 10 gateway) PASS. Includes real 60-second timeout and OAuth HTTP tests.
- **0.3.6** adds only the version/evidence documents, exact client-matrix evidence and a doctor wording correction; gateway behavior is unchanged from the integrated test run. Release build and final strict lint pass.
- Final 0.3.6 release binary: routing smoke and pinned official Filesystem via `npx` PASS (Basic, discovery, read/list, write denial, missing-auth rejection).
- Final archive: actual install/reinstall, executable version, spaces/quoting, zsh/bash PATH activation and corrupt-archive rejection PASS. Test state/profiles/processes were isolated and cleaned up.
- Codex proof ran against 0.3.5 with per-process configuration and an observation-only loopback proxy. The generated secret/value were not retained; the actual MCP tool event matched the unknown test value. No global client configuration changed.
- Actual Quick doctor output was validated from `daemon_checks`: local HTTP 200, provider announced, public DNS WARN. The first harness assertion read the CLI-only `checks` field; correcting the evidence parser established the expected daemon diagnostic. This is not a normal public HTTPS PASS.
- Documentation/matrix/evidence checks pass. Only **3 of 2,880 exact client rows** gain actual-client PASS; other missing evidence stays NOT TESTED/UNSUPPORTED.

Local archive: `target/dist/mcptunnel-v0.3.6-aarch64-apple-darwin.tar.gz`.

SHA-256: `e903dea83615cc068909f39fb41d661a867d70a38d3714072f7238c61f41cd2f`.

This candidate is local, unsigned and unpublished. Its checksum identifies the tested archive, not an independent publisher signature. Redacted local execution logs are under `target/td-public-readiness/`.

## Future: separate backlog

| Category | Work | Task |
|---|---|---|
| Future | Modern stateful continuity with explicit principal/session ownership | [td#607](https://tdtd.dev/607) |
| Future | Interactive MRTR, sampling/elicitation, Tasks, subscriptions/replay, Apps | [td#608](https://tdtd.dev/608) |
| Future | Catalog refresh and workload-driven performance work | [td#609](https://tdtd.dev/609) |
| Future | Additional named-client/hosting/platform evidence for concrete support claims | [td#610](https://tdtd.dev/610) |
| Future — revenue | Trusted stable-hostname provisioning, ownership, renewal/revocation | [td#611](https://tdtd.dev/611) |
| Future — revenue | Accounts, entitlements, billing/plans and hosted operations | [td#612](https://tdtd.dev/612) |
