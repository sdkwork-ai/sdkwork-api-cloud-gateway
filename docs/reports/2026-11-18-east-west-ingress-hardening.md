# 2026-11-18 — East-west ingress hardening after the `421 host_not_allowed` fix

Follow-up regression sweep for the same production deployment
(`sdkwork-api-cloud-gateway-production-i1-gateway-1`, image `0.1.85`). The
previous commit taught the ingress host allowlist to accept this process's own
published origin. This pass audited every other request-path control that an
in-process or cluster-internal call traverses, using the production
configuration actually running in the container
(`/etc/sdkwork/api-gateway/config.toml` + `GATEWAY_*` env).

## 1. Confirmed non-issues

| Control | Production value | Why internal calls pass |
| --- | --- | --- |
| `security.maxHeaderBytes` / `maxHeaderCount` | 65536 / 128 | identity headers of an internal call are a few hundred bytes |
| `security.csrfGuardEnabled` | `true` | the guard only arms on a request that carries a session `Cookie`; SDK-to-SDK hops carry no cookie, and proxy-forwarded browser requests carry a registered client `Origin` |
| `security.rejectCredentialHeadersOnAnonymousAuthRoutes` | `true` | the rule is scoped to device-authorization / token endpoints only |
| `[policy]` body limits | 16 MiB default write, 32 MiB global | the inner hop carries the same body the outer request already passed |
| `[traffic].globalMaxInFlight` | 4096 | a nested hop costs two permits and no per-path concurrency rule matches `/vidu/**` or `/app/v3/api/generations/**` |
| `[security.waf]` query/header scan | enforce | internal requests carry no query string or user-controlled headers |
| `[cors]` | — | server-to-server calls send no `Origin` |

## 2. Finding A — every client shared one rate-limit bucket

`traffic.rs` resolved a `client-ip` rate-limit identity from `X-Forwarded-For`
alone and returned the single string `unknown` whenever the header was absent or
its last hop was outside `trustedProxyCidrs`.

Production sets `trustedProxyCidrs = 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16`
and the edge nginx appends the client address as the last chain entry, so the
last hop was a **public** address and therefore "untrusted" — every public
client also collapsed into `unknown`.

Impact: `global-ip-default` (capacity 200, refill 100/s) was a single
instance-wide bucket shared by public traffic, direct clients **and** every
east-west registry call. One noisy caller could 429 the gateway's own media and
agent hops, and a legitimate internal burst competed with unrelated public
traffic. (No 429s were recorded yet in production, so this was latent rather
than active.)

Fix:

- the listener is served through
  `Router::into_make_service_with_connect_info::<SocketAddr>()`, so the socket
  peer reaches the router;
- `traffic_middleware` passes that peer to `GatewayTrafficState::acquire`;
- `client_ip_for_rate_limit` now delegates to the shared
  `sdkwork_utils_rust::trusted_proxy::extract_client_ip`: the peer is the
  authority, and `X-Forwarded-For` / `X-Real-IP` are honoured only behind a
  trusted peer (right-to-left walk that skips trusted hops).
- `unknown` remains only when a router runs without connect info (embedded hosts
  that have not adopted the connect-info service yet).

## 3. Finding B — the WAF body stage truncated large JSON bodies

`security::inspect_json_body` buffered the body with the 1 MiB WAF budget and, on
overflow, replaced the body with `Body::empty()` and let the request continue.
The handler then received zero bytes, producing a `400`/parse failure instead of
the client's payload. Media generation requests carry inline images well past
1 MiB, and any JSON write surface (canvas documents, notes, prompts) could hit
the same path.

Fix:

- a declared `Content-Length` beyond the 1 MiB budget skips inspection without
  reading the stream;
- a streamed body (no declared length) is buffered up to
  `request.maxBodyBytes`; beyond the gateway cap the stage rejects with `413
  request_body_too_large` (the same problem the downstream body-limit
  enforcement produces) instead of handing the handler an empty body;
- the buffered bytes are always returned to the handler unchanged;
- bodies between 1 MiB and the gateway cap are forwarded uninspected, matching
  the documented "bodies larger than this are skipped by the WAF stage" intent.

## 4. Verification

- `cargo test -p sdkwork-api-cloud-gateway --lib` — 64 passed, including:
  - oversized JSON body forwarded byte-identical;
  - declared oversized `Content-Length` forwarded without inspection;
  - streamed body beyond the gateway cap rejected with `413`;
  - baseline signature inside the budget still blocked with `403`;
  - socket peer isolates rate-limit buckets, forwarded chain honoured only
    behind a trusted peer, untrusted peer cannot spoof identity.
- `cargo check -p sdkwork-api-cloud-gateway --bin sdkwork-api-cloud-gateway`.
- `cargo fmt -- --check`, Rust naming/manifest, source-config and
  backend-composition gates.

## 5. Operator notes / remaining decisions

- A caller that dials the gateway by an address the process does not publish
  (for example a host LAN IP in a bridge network) must still be covered by
  `[security].allowedHosts`, or the deployment must set
  `GATEWAY_PRIVATE_HTTP_URL` to that origin so the registry advertises it. The
  gateway deliberately keeps rejecting foreign IP literals by default
  (`allowIpHosts = false`); loosening that is a security policy change and needs
  review.
- `[policy]` `default-write-limit = 16777216` and
  `request.maxBodyBytes = 33554432` are the effective media ceilings. Inline
  image payloads above 16 MiB on any write surface return `413` by policy, not
  by a gateway defect.
- No build, tag, or deploy was performed.
