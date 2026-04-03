# Harness Rules

- `cargo build` must succeed with zero errors before commit.
- `cargo test` must pass all tests before commit.
- Warnings are tolerated (unused code warnings expected for not-yet-wired paths).
- rusqlite uses `bundled` feature — no system SQLite dependency.
- rmcp features: `client`, `transport-child-process` — no server features needed.
