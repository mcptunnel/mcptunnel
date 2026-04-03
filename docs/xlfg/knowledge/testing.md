# Testing

- 47 unit tests, all inline (`#[cfg(test)]` modules).
- Async tests use `#[tokio::test]`.
- SQLite tests use `Connection::open_in_memory()`.
- No integration tests requiring external processes (cloudflared, real MCP servers) in the test suite.
- `tempfile` crate available as dev-dependency for filesystem tests.
