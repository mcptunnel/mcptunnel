# Tunnel plans and enforcement

| Mode | URL ownership/lifetime | Auth and synchronous JSON MCP | Request SSE | Status |
|---|---|---|---|---|
| Free Quick | Random `https://<random>.trycloudflare.com`; changes on restart/reconnect | Free; Public, Basic, or configured external OAuth | Unsupported by Cloudflare | Implemented; provider/parser tests and real public Filesystem HTTPS verified on Windows |
| Independent local hosting | Operator supplies a reverse proxy and optionally `external_origin` | Free OSS functionality | Request-scoped progress, when the proxy supports SSE | Implemented; loopback fixture tested; independent proxy E2E NOT TESTED |
| Official paid managed | Trusted service reserves/owns a stable hostname for an authenticated entitled account | Planned | Requires a compatible provider | UNSUPPORTED: no entitlement/provisioning service exists |

Localhost is the default; `--public` explicitly selects Quick for a mount. It does not require an account or billing. Basic is the default authentication mode. Anonymous authentication (`--auth public`) is explicit; it exposes allowed operations to anyone who obtains the URL. `--public` alone retains Basic authentication. OAuth requires the operator's external authorization server. Core diagnostics and policy are free.

`--tunnel managed`, persisted managed mounts, and `--hostname` do not activate a working managed service. Selection fails with an actionable unavailable-backend error. Quick/local mounts reject a hostname assignment. No local plan, environment variable, database flag, or client token can prove payment.

## Future hosted integration boundary

`src/tunnel/managed.rs` defines `ManagedProvisioner::provision`, `renew`, and `revoke`. Its current implementation always returns `Unavailable` and issues no lease.

A future implementation must call a trusted hosted service using an authenticated service access token. That service, not the local gateway, must:

1. Validate account identity and current paid entitlement.
2. Atomically reserve hostname ownership in its trusted database.
3. Provision a provider route and return an expiring lease with a verified public origin.
4. Revalidate entitlement and hostname ownership on renewal.
5. Revoke provider routes on entitlement loss according to the published subscription policy.

`Unauthenticated`, `NotEntitled`, `Revoked`, and `HostnameNotOwned` are explicit failure states. Tokens must never be serialized to logs or returned by inspection. The unavailable implementation is tested to reject provision/renew/revoke; actual entitlement revocation and provider lifecycle need hosted integration tests before managed mode can ship.

There is no billing server, payment simulation, permissive local entitlement implementation, dashboard, or uptime claim in this repository.

## Independent infrastructure under MIT

Operators may use their own domains/tunnels/reverse proxies. Set `[tunnel] default_mode = "local"` and an HTTPS origin in `external_origin`, run the loopback gateway, and route that origin to `daemon.bind`. This consumes no mcptunnel hosted hostname and requires no paid plan. A stable hostname in independently owned infrastructure is not official managed provisioning. The reverse proxy must preserve the Host, authorization, MCP, and Origin headers and permit request SSE if progress is wanted.

Quick mounts always use the temporary origin returned by their provider, never `external_origin`. The legacy global quick configuration rejects `external_origin`. New CLI registrations are local unless `--public` or explicit `--tunnel quick` is used; the old `tunnel.default_mode` field is retained only for configuration compatibility. The gateway trusts only its configured local authority and its active verified public origin, never arbitrary forwarded headers.

## Reconnection and transport limits

The supervisor clears the active public origin on tunnel failure. Status reports no public origin while reconnecting; the local endpoint remains available. A replacement URL must be copied into clients, and external OAuth authorization must acquire a token for the new full MCP resource URL. Old access-token audiences are not accepted on the replacement origin.

Quick responses remain bounded synchronous JSON. Progress notifications are dropped at this boundary rather than opening an unsupported stream. Tools requiring interactive callbacks, subscriptions, or streaming-only semantics cannot be made compatible by buffering them. Use independently hosted SSE-capable infrastructure for request-scoped progress; the bridge still does not advertise Tasks, subscriptions, or interactive MRTR.

Cloudflare documents Quick Tunnel development limits and no SSE in [its official documentation](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/) (retrieved 2026-10-08). Gateway limits are intentionally lower: 64 global requests and, by default, 8 active requests per mount/session. A stdio request times out after 60 seconds, with a separate 15-second startup limit. There are no uptime guarantees.

## Port and visibility routing (0.3.0)

Each requested port has one loopback listener and an optional separate, filtered Cloudflare ingress. A Quick provider connects only to that ingress; it never points at the listener serving local-only mounts. Visibility filtering precedes MCP/metadata dispatch and does not depend on Host or forwarded headers. Same-port public mounts share an origin; different ports have separate providers/origins, allowing the same endpoint path on both. Routes use exact persisted paths. Last-public removal stops/reaps its provider; last-mount removal closes the port. No provider starts for local-only registrations.
