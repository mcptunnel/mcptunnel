# Harness Rules

- `cargo build` must succeed with zero errors before commit.
- `cargo test` must pass all tests before commit.
- Zero warnings policy: `cargo build` must produce no warnings.
- rusqlite uses `bundled` feature — no system SQLite dependency.
- rmcp features: `client`, `transport-child-process` — no server features needed.
