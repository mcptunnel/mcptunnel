# Modernization plan — 2026-10-08

Verification first: establish the baseline, use a real legacy stdio fixture behind an HTTP listener, run regression tests and both released conformance requirement sets, then fmt, strict clippy, all-target tests, and release build. No remote publishing or infrastructure changes.

1. Research current specification, SDK, transports, auth and client documentation; audit relevant source. Baseline: Windows compilation fails on unconditional Unix socket imports.
2. Upgrade rmcp to verified 3.5.1 (MSRV 1.88); delegate modern metadata, headers, errors and HTTP semantics to its transport. Bridge to auto-negotiated stdio with bounded requests; keep legacy sessions bound to mount and authenticated subject.
3. Enforce free temporary tunnels and fail closed for unimplemented managed provisioning. Harden authentication, local IPC, process cleanup, policies and diagnostics.
4. Add real process/HTTP regressions, official conformance fixture/runner, platform CI and packaging. Record actual results without baselines hiding failures.
5. Update README, protocol audit, compatibility matrix, tunnel plans and engineering report. Commit coherent verified work items with version bumps.

Historical status (0.2.2–0.2.3, 2026-10-08): complete for the selected synchronous scope, not full protocol support. fmt, strict Clippy, 63 tests, release build, isolated installation and source package verification PASS. Supported conformance subset: 50 scenarios PASS; both full sets executed and their unsupported interactive/extension failures retained. Linux/macOS/Windows CI subsequently passed after the authorized push. Version 0.2.3 adds binary distribution/installers and verified official Filesystem MCP checks over local HTTP and public Quick HTTPS. Account-backed E2E, hosted OAuth, live provider reconnection and the managed backend remain unverified/unavailable. See ENGINEERING_REPORT.md and RELEASING.md for actual results and remaining priorities.

## Pre-public corrections — 2026-10-22

The current workboard is [td#596](https://tdtd.dev/596). Onboarding uses pinned, read-only Filesystem operations instead of stateful Sequential Thinking. Modern requests discard process memory between calls; only initialized legacy sessions reuse a child. Runtime pins, cancellation and diagnostics are separate work items with their own focused checks. GitHub Actions remains disabled; this plan does not authorize cloud runs or publication. Historical multi-platform results above do not verify subsequent source changes.
