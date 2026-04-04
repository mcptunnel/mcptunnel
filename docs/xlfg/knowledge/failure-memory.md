# Failure Memory

<!-- Past failures, root causes, and lessons learned. -->

- **2026-04-03 — Tunnel URL race condition**: `mcptunnel run` after `daemon restart` returned `127.0.0.1` instead of Cloudflare tunnel URL. Root cause: UDS admin socket accepts before tunnel background task sets `tunnel_origin`. Fix: `wait_for_tunnel_ready(30s)` poll in `handle_mount_create` and `handle_mount_inspect`. Lesson: when background tasks set shared state, admin API handlers that read that state must wait or handle the unset case explicitly.
