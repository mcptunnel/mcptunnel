# Testing — 2026-10-22

- Rust tests include unit tests and real HTTP/stdio/CLI integration tests under `tests/`; they are not limited to inline or in-memory fixtures.
- `scripts/smoke-filesystem.mjs` checks the pinned official Filesystem server with isolated state, Basic authentication, discovery, read/list and denied writes. It cleans up its daemon and temporary directory.
- `scripts/smoke-routing.mjs` checks listener/routing boundaries with a deterministic fake provider. It is distinct from a real public HTTPS check.
- `scripts/verify-evidence.mjs` checks documentation links and recorded evidence consistency. It does not exercise live client accounts or repeat protocol conformance.
- Historical 0.3.1 evidence records 66 Rust tests per platform, 50 selected supported conformance scenarios and five native candidates. Full conformance has recorded failures; current Codex evidence is limited to local Basic-authenticated initialization, discovery and one tool call. See [engineering report](../../ENGINEERING_REPORT.md).
- Choose the smallest check proving each change, run it locally before committing, and clean up child processes. Do not expand into repeated platform or full-conformance runs without a concrete need.
- GitHub Actions is disabled. Do not enable, dispatch, rerun, or push a release tag without explicit authorization for that execution; a commit/push request alone is insufficient.
