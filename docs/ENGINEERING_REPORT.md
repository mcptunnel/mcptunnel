# Modernization engineering report — 2026-10-08

Version **0.2.2**, local Windows verification with Rust 1.88.0 and Node 22.23.2. Baseline `f4515c5`; verified implementation commits `2bab731` (0.2.0) and `a580aad` (0.2.1), followed by the final 0.2.2 lifecycle/documentation commit. No push, deployment, package publication or remote infrastructure modification occurred.

**Achieved locally:** a current stateless HTTP client invokes a legacy stdio tool; legacy initialization sessions continue working; supported synchronous gateway conformance scenarios pass; authentication/session/process isolation is tested; Quick configuration permits only temporary origins; unimplemented managed provisioning fails closed. **Full conformance and actual AI-client E2E have not been achieved.** The exact remaining evidence is below.

## Changes by file

Paths are repository-relative. Small supporting files are grouped with their associated behavior.

| Files | Final behavior |
|---|---|
| `Cargo.toml`, `Cargo.lock` | rmcp 1.3 → verified 3.5.1; Rust 1.88 MSRV; package 0.2.2; SDK HTTP/client features; process-wrap used directly for process groups/jobs; yanked transitive spin 0.9.8 → compatible 0.9.9; package includes source, fixtures, scripts, license and public docs |
| `src/lib.rs`, `src/main.rs` | Library enables real integration fixtures; normal ExitCode return permits async runtime cleanup |
| `src/gateway/mcp_handler.rs` | Stateless SDK dispatch; modern version errors; body/admission limits; request-lifetime permits; principal-bound legacy sessions; notification and DELETE semantics; no-store responses |
| `src/gateway/bridge.rs` | Typed discovery/tool/resource/prompt/completion bridge; modern-to-legacy adaptation; capabilities from probe; policy enforcement; private cache/completion metadata; local request progress forwarding |
| `src/gateway/security.rs`, `mod.rs`, `health.rs`, `inspection.rs` | Exact Host/Origin boundary, duplicate Authorization rejection, mount-specific Basic/OAuth identity and scopes; protected-resource metadata routing; diagnostic URL reflects active provider |
| `src/upstream/child.rs`, `mod.rs`, `progress.rs` | SDK auto lifecycle negotiation, per-request modern child/session-owned legacy child, token-free stdio context, safe progress routing, hard timeout/cancellation and sanitized errors |
| `src/upstream/transport.rs` | SDK parsing with bounded 4 MiB newline messages; Unix process groups and Windows job objects kill descendants even after launcher exit; immediate termination/reaping avoids response delays from upstream background timers |
| `src/upstream/probe.rs` | Complete bounded paginated discovery, full tool schemas for header validation, repeated-cursor rejection and advertised-capability checks |
| `src/auth/basic.rs`, `public.rs` | 128-bit generated password, constant-time comparison, header credentials, explicit public auth metadata without replacing existing declarations |
| `src/auth/oauth.rs` | HTTPS bounded issuer/JWKS discovery, cached/throttled refresh, RS256 issuer/subject/audience/expiry/nbf/scope validation, resource-bound session identity |
| `src/auth/oauth_server.rs` | Removes insecure built-in authorization endpoints; provides external protected-resource metadata; external AS owns PKCE/CIMD/DCR/registration/signing persistence |
| `src/session/mod.rs`, `types.rs` | Immediate expiry checks, per-mount/global limits, principal/version binding and concurrency permits |
| `src/policy/filter.rs` | Tool discovery/invocation and resource/prompt/completion policies; avoids reserved JSON-RPC error range |
| `src/tunnel/mod.rs`, `managed.rs` | Quick/local/managed separation; rejects stable hostname assignment in free mode; unavailable managed provision/renew/revoke boundary with explicit entitlement/ownership/revocation errors |
| `src/tunnel/quick.rs`, `supervisor.rs` | Strict temporary origin parser; stale URL clearing, restart/retry behavior and provider process/task cleanup |
| `src/daemon/mod.rs`, `state.rs`, `ipc.rs`, `admin_api.rs` | Loopback server, bounded local admin requests, portable named-pipe/Unix IPC, config/mode validation, session eviction, worker cancellation and shutdown cleanup |
| `src/config.rs` | Invalid configuration fails closed; isolated config/state paths; independent external HTTPS origin; validated transport modes/limits |
| `src/mount/mod.rs`, `types.rs`, `db.rs`, `slug.rs` | Reject unimplemented auth/managed configuration; persist full probe schemas; bound session settings; retain reserved route names |
| `src/cli/commands.rs`, `mod.rs`, `output.rs` | Portable admin connection/autostart; capture caller cwd; creation-only credential output; no URL secrets; rejects unsupported built-in auth/managed flows |
| `src/doctor.rs`, `logging.rs` | Fresh bounded upstream probe, header/version/auth/provider/client guidance, bounded sanitized recent method/status diagnostics |
| `tests/gateway.rs`, `tests/fixtures/legacy.cjs`, `tests/cli.rs` | Real HTTP/stdio and daemon IPC regressions, concurrent state isolation, cancellation, crash recovery, auth/limits and cwd/config checks |
| `examples/conformance_gateway.rs`, `scripts/prepare-conformance.mjs`, `run-conformance.mjs`, `summarize-conformance.mjs` | Official pinned factory adapted to stdio; initialized HTTP fixture, supported/full requirement runs, cleanup and durable check identities/results |
| `.github/workflows/ci.yml` | fmt/strict Clippy/tests/release on Linux, macOS and Windows; pinned supported conformance source on Linux; binary/report artifacts |
| `LICENSE`, `README.md`, `docs/RESEARCH.md`, `PROTOCOL_AUDIT.md`, `TUNNEL_PLANS.md` | MIT license; correct installation repo; verified sources/audit; secure quick start; free/paid boundaries and limitations |
| `scripts/compatibility-matrix.mjs`, `verify-evidence.mjs`, `docs/CLIENT_COMPATIBILITY.md`, `CLIENT_COMPATIBILITY.csv`, `CONFORMANCE_RESULTS.json`, `IMPLEMENTATION_PLAN.md`, this report | Reproducible 2,880-combination matrix with three evidence levels, evidence arithmetic/link checks, copy/paste client templates, actual conformance results and completion record |

## Supported revisions and limits

| Revision | Executed evidence |
|---|---|
| 2026-07-28 | Stateless discovery and core synchronous tool/resource/prompt/completion bridge; metadata/header/error/schema/cache checks; no initialize or session ID; official current subset and real legacy stdio fixture |
| 2025-11-25 | Initialization/session lifecycle and tool/resource/prompt/completion bridge; previous stable subset and real stdio fixture |
| 2025-06-18 | Initialize, notification, stateful tool calls, session deletion and cross-mount isolation regression; broader resource/prompt/client combinations NOT TESTED |
| 2025-03-26 | Same focused legacy regression; broader combinations NOT TESTED |

Modern requests create one upstream process per operation; legacy sessions own reusable children. This trades startup cost and absent cross-request modern state for reliable principal isolation. A process is not a sandbox: it retains the operator's OS permissions. Upstream HTTP credentials are never forwarded to stdio. Discovery/schema snapshots require a fresh probe after upstream changes.

HTTP bodies: 1 MiB/10 seconds. Stdio: 4 MiB per line, 15-second startup, 60-second request. Admission: 64 global requests, default 8 per mount/session; 8 sessions per mount/256 global. Discovery: 64 pages, 4,096 entries and 16 MiB per catalog. Request-body permits survive through SSE stream completion/disconnect. Progress does not reset the hard timeout.

Quick emits bounded JSON and suppresses progress because the provider has no SSE. Independent local hosting supports request-scoped progress SSE. GET subscriptions, replay, deprecated HTTP+SSE, interactive MRTR, Tasks, sampling/elicitation, logging and Apps are unsupported and unadvertised. Separate legacy `notifications/cancelled` requests are not correlated to active calls; stream disconnect and timeout cancellation are verified.

## Actual verification

| Command/check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS; zero warnings |
| `cargo test --all-targets` | PASS: 53 unit + 1 CLI + 9 gateway = **63 tests**; real timeout test takes 60 seconds |
| `cargo test --test cli` after final diagnostic text update | PASS: 1; real Windows daemon/IPC, caller cwd, fresh probe, secret-free inspection, fail-closed config and shutdown |
| `cargo build --release` | PASS: final 0.2.2 binary |
| `cargo install --locked --path . --root <workspace>/target/install-smoke`; installed `mcptunnel --version` | PASS: isolated local installation reports 0.2.2; user PATH/global installation untouched |
| `cargo build --example conformance_gateway` | PASS |
| `node scripts/run-conformance.mjs` | PASS: final cleanup/fixture rerun, 26 current + 24 previous scenarios |
| `node scripts/run-conformance.mjs --full` | FAIL, exit 1; both full requirement sets were actually run on 0.2.2 |
| `node scripts/summarize-conformance.mjs target/conformance-source <full-report-root>` | PASS; committed redacted check IDs/counts in `CONFORMANCE_RESULTS.json` |
| `node scripts/compatibility-matrix.mjs --check` | PASS: deterministic 2,880 unique combinations; allowed statuses checked |
| `cargo package --list --allow-dirty` | PASS: code, tests, docs, scripts and MIT license included |
| `cargo package --allow-dirty --locked` with `CARGO_TARGET_DIR` set to the workspace target | PASS: final source archive built and verified. The first duplicate-cache attempt exhausted disk; scoped `cargo clean` reclaimed 2.2 GiB before verification succeeded with the existing cache. No package publication. |
| `node scripts/verify-evidence.mjs`; script syntax; CI YAML; `git diff --check` | PASS: matrix, both durable result files and local links; three-platform workflow parsed |

Full results use official conformance source commit `c37eec888e1c6ff140af79987a40008548b7cc5f`, package 0.2.0-alpha.12. Published stable npm 0.1.16 lacked the necessary requirements option. This source snapshot choice is explicit, not a claim that an alpha package is a stable release.

| Requirement set | Scored pass/fail | All checks pass/fail | All warnings / skipped / info | Full verdict |
|---|---|---|---|---|
| 2026-07-28 | **93 / 10** | **142 / 35** | 4 / 6 / 1 | FAIL |
| 2025-11-25 | **53 / 8** | **64 / 8** | 0 / 0 / 4 | FAIL |

Current scored failures are ten interactive `input-required-result-*` scenarios. The additional 25 failures are unscored optional Tasks checks. Previous scored failures are logging (2), sampling (1), elicitation/default/enums (3), and resource subscribe/unsubscribe (2). Current warnings concern three interactive protocol paths and optional data-URI behavior; skipped checks concern subscriptions and a Task notification. These are retained and are not counted as passes. Stateless checks pass 25/25; standard and custom HTTP header checks pass 14/14 and 10/10; caching 8/8 and progress 2/2. No expected-failure baseline changes FAIL to PASS.

The stdio fixture projects the official reusable everything-server factory, plus three noninteractive diagnostics for missing client capability, log suppression and annotated headers. Its HTTP-only MRTR/Tasks diagnostic tools are absent. Consequently the full failures establish unsupported paths and an incomplete full-fixture contract; they are not evidence that a successfully implemented MRTR/Tasks bridge was exercised. Stateless stream checks do not establish interactive elicitation support. The bridge intentionally rejects interactive continuations and does not advertise these extensions. [Exact check identities and failure messages](CONFORMANCE_RESULTS.json).

A [pre-fix full run](CONFORMANCE_STRESSED_RESULTS.json) is also retained: current scored 91 pass/12 fail, total 140 pass/37 fail. Two additional stateless stream failures reproduced after disk recovery. One cause was waiting up to two seconds for a completed child's background timers before returning its HTTP result. Completed requests now terminate and reap the group/job immediately. Canceled requests alone have a bounded one-second grace to consume their notification before forced termination; cancellation has no protocol acknowledgement. A real timer-owning child regression checks normal cleanup completes within one second. The immediate-kill prototype failed two cancellation regressions; the conditional grace fixes that race without delaying completed responses.

The final large-fixture calls also measured 489–581 ms at an idle build directory, bordering the official 500 ms stream deadline. Bundling its existing dependency graph reduced four observed calls to 313–416 ms. This fixture packaging preserves its implementation and avoids repeated module resolution; it is not a gateway latency benchmark. The official checks, deadlines and expected results are unchanged. Per-request startup performance under contention remains unqualified.

## Reproduce official conformance

From the repository root with Rust 1.88 and Node 22:

```sh
git clone https://github.com/modelcontextprotocol/conformance.git target/conformance-source
git -C target/conformance-source checkout c37eec888e1c6ff140af79987a40008548b7cc5f
```

In `target/conformance-source`, run `npm ci` and `npm run build`, then return to the mcptunnel root:

```sh
node scripts/prepare-conformance.mjs
cargo build --locked --example conformance_gateway
node scripts/run-conformance.mjs
node scripts/run-conformance.mjs --full
```

The runner waits for probe/readiness, uses port 43123, writes timestamped raw reports under ignored `target/conformance-results`, and shuts down/reaps the gateway/children. The full command currently exits 1. To update the durable summary, pass the emitted full-run directory as the third argument to `summarize-conformance.mjs`.

## Client evidence and infrastructure blockers

[Client compatibility](CLIENT_COMPATIBILITY.md) distinguishes official configuration documentation from protocol fixtures and account-backed E2E. All actual AI-client tests remain NOT TESTED. Basic public/current and external JWT/legacy HTTP combinations have focused tests; a transport-supported client is not assumed to negotiate 2026-07-28. No client configuration was altered. No secrets appear in durable evidence.

Live Cloudflare connectivity/reconnection, a public reverse proxy, external AS TLS discovery/key rotation/browser PKCE/CIMD/DCR/refresh, client accounts and OpenAI API billing require infrastructure/credentials unavailable in this task. Linux/macOS CI is configured but was not executed; Windows cross-user named-pipe ACL behavior remains untested.

Managed hosting requires a trusted service that authenticates accounts, verifies paid entitlement, records hostname ownership, provisions routes, renews expiring leases and revokes routing. No such service exists here. `ManagedProvisioner` is a fail-closed future boundary; local config cannot enable managed provisioning. Entitlement loss and real provider revocation require hosted tests before enabling it. Independent MIT hosting remains available without an account.

## Remaining priorities

| Priority | Remaining issue / completion requirement |
|---|---|
| P0 | No known unaddressed P0 finding within the implemented synchronous scope. This is not a security certification; upstream tools retain user permissions. Built-in/inherit auth and managed mode remain disabled rather than claiming safety without their required services. |
| P1 | Full interactive conformance does not pass. Implement and test a real callback/MRTR bridge before claiming that functionality; add official HTTP-only fixture diagnostics when doing so. |
| P1 | Run actual named-client E2E, external authorization flows/key rotation, public Quick and independent proxy tests. Record exact client builds and revisions instead of inferring them. |
| P1 | Execute Linux/macOS CI, and verify Windows IPC against a different local user; platform support outside the local Windows run is provisional. |
| P1 | Legacy explicit cross-request cancellation is not mapped, and JSON-response connection aborts are not directly tested. Add authenticated session/request-ID routing and a JSON-abort regression before claiming all cancellation paths. SSE disconnect and hard timeout cancellation pass. |
| P2 | Benchmark cold-process latency and sustained load. Process isolation bounds state but is not yet performance-qualified for a production workload. |
| P2 | Define schema/probe refresh policy for dynamically changing upstream catalogs; current schema snapshot requires an operator probe. |
| P2 | Evaluate optional Tasks/subscriptions/Apps only with a justified use case and conformance coverage. A real paid backend/provider lifecycle is separate work, not a hidden local feature. |

The code and docs intentionally limit their claims to these results. Local helper processes are cleaned up; the final Git work is committed with version bumps, without remote publication.
