# Modernization plan — 2026-10-08

Verification first: establish the baseline, use a real legacy stdio fixture behind an HTTP listener, run regression tests and both released conformance requirement sets, then fmt, strict clippy, all-target tests, and release build. No remote publishing or infrastructure changes.

1. Research current specification, SDK, transports, auth and client documentation; audit relevant source. Baseline: Windows compilation fails on unconditional Unix socket imports.
2. Upgrade rmcp to verified 3.5.1 (MSRV 1.88); delegate modern metadata, headers, errors and HTTP semantics to its transport. Bridge to auto-negotiated stdio with bounded requests; keep legacy sessions bound to mount and authenticated subject.
3. Enforce free temporary tunnels and fail closed for unimplemented managed provisioning. Harden authentication, local IPC, process cleanup, policies and diagnostics.
4. Add real process/HTTP regressions, official conformance fixture/runner, platform CI and packaging. Record actual results without baselines hiding failures.
5. Update README, protocol audit, compatibility matrix, tunnel plans and engineering report. Commit coherent verified work items with version bumps.

Status: complete for the supported synchronous scope. fmt, strict Clippy, 63 tests, release build, isolated installation and source package verification PASS. Supported conformance subset: 50 scenarios PASS; both full sets executed and their unsupported interactive/extension failures retained. Account-backed E2E, hosted OAuth/provider tests, managed backend and non-Windows CI are explicitly unverified/unavailable. See ENGINEERING_REPORT.md for actual results and P0/P1/P2 priorities.
