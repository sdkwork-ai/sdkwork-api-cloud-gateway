# HTTP Service Registry Runbook

Database-backed HTTP service registration and resolution for SDKWork backend
SDKs (`DISCOVERY_SPEC.md` §12.1, `APP_SDK_INTEGRATION_SPEC.md` §5.2).

## 1. What is stored

The canonical table is `discovery_service_instance` (same schema as the
`sdkwork-discovery` baseline). One row = one `(namespace, environment,
service_name, instance_id)`:

| Column | Meaning |
| --- | --- |
| `namespace`, `environment` | Registry partition, e.g. `platform` / `production` |
| `service_name` | Canonical service or SDK alias (`sdkwork-cloudrouter`, `cloudrouter`) |
| `instance_id` | `<profile-id>:<hostname>:<port>` — unique per process/port |
| `endpoint` | Absolute `http(s)://host[:port]` base URL, LAN-reachable in cluster deployments |
| `protocol` | `http` for backend SDK base URLs |
| `status` | `serving`, `degraded`, `not_serving` |
| `priority`, `weight` | QoS ordering (lower priority first, then higher weight) |
| `lease_id`, `expires_at_ms` | Lease identity and absolute expiry |
| `metadata_json` | `deployment_profile`, `runtime_target`, `gateway_environment`, `api_prefix` |

The unique constraint `uk_discovery_service_instance_identity` makes
registration idempotent.

## 2. Producer: the gateway

At startup the gateway:

1. binds its listener and learns the real port;
2. connects to `SDKWORK_DISCOVERY_DATABASE_URL` (falls back to
   `SDKWORK_DATABASE_URL`) and creates the table when missing;
3. registers **every composed surface and SDK alias** for
   `instance_id = <profile-id>:<hostname>:<port>`;
4. installs the process-global resolver **before** building the router, so
   dependency assemblies that create HTTP clients during assembly see it;
5. heartbeats every 10s against a 30s lease, re-registers when a lease is lost,
   sweeps expired rows every minute, and deregisters on graceful shutdown.

Registration is fail-open: a registry outage logs a warning and the gateway
keeps serving with authored topology values.

## 3. Consumer: any Rust backend SDK

Resolution order (`sdkwork_utils_rust::service_base_url`):

1. explicit per-service override;
2. the surface composed in-process (no HTTP base URL);
3. a healthy registry instance (cache refreshed from the database);
4. the authored topology public HTTP URL;
5. the documented split default (degradation).

`registry_base_url_or(service, configured)` returns the registry target when
present, otherwise the configured value. `report_registry_result(...)` feeds
success/failure back to the resolver so a failing target is demoted for a
cooldown. Adapters resolve per call and pool clients by resolved base URL; the
media tool family pools by `(base URL, auth token)`, so a multi-process cluster
load-balances without pinning one process.

Embedded consumers inherit the gateway's installed resolver. Standalone hosts
must call:

```rust
let _registry = sdkwork_discovery_client::install_resolver_from_env(
    sdkwork_discovery_client::registry_identity_from_env(),
    sdkwork_discovery_client::HttpRegistryConfig::default(),
).await;
```

before constructing SDK clients.

## 4. Configuration

| Key | Default | Meaning |
| --- | --- | --- |
| `SDKWORK_DISCOVERY_DATABASE_URL` | app DB | Dedicated registry database. **All services in one environment MUST use the same value.** |
| `SDKWORK_DISCOVERY_NAMESPACE` | `platform` | Registry partition |
| `SDKWORK_DISCOVERY_HTTP_REGISTRY_ENABLED` | `true` | `false` disables registration and registry-first resolution |
| `SDKWORK_API_CLOUD_GATEWAY_PRIVATE_HTTP_URL` | auto-detected LAN IP | Authored LAN origin published to the cluster registry (`http://10.0.0.5:8080`). The public edge domain is not registered. |
| `SDKWORK_API_CLOUD_GATEWAY_ADVERTISE_IP` | unset | Explicit LAN IP combined with the bound port when the private URL is empty. |
| `SDKWORK_DISCOVERY_HTTP_REGISTRY_LOAD_BALANCER` | `priority` | `priority`, `round_robin`, `random`, or `weighted_random`. |
| `SDKWORK_API_CLOUD_GATEWAY_REGISTRY_PRIORITY` | `100` | Registration priority; lower values are preferred tiers. |
| `SDKWORK_API_CLOUD_GATEWAY_REGISTRY_WEIGHT` | `100` | Registration weight inside one priority tier. |
| `SDKWORK_DISCOVERY_HTTP_REGISTRY_REFRESH_MS` | `5000` | Snapshot refresh interval |
| `SDKWORK_DISCOVERY_HTTP_REGISTRY_FAILURE_THRESHOLD` | `3` | Consecutive failures before demotion |
| `SDKWORK_DISCOVERY_HTTP_REGISTRY_FAILURE_COOLDOWN_MS` | `30000` | Demotion cooldown |
| `SDKWORK_DISCOVERY_HTTP_REGISTRY_HEALTH_PATH` | unset | Opt-in active health probe path (for example `/healthz`); unset keeps failure-driven detection only |
| `SDKWORK_DISCOVERY_HTTP_REGISTRY_HEALTH_TIMEOUT_MS` | `2000` | Per-probe timeout |

OEM / new cluster / new domain:

- point every service at the same registry database;
- set a distinct `SDKWORK_DISCOVERY_NAMESPACE` per tenant/platform partition;
- set `GATEWAY_PRIVATE_HTTP_URL` (or `GATEWAY_ADVERTISE_IP`) to a LAN address
  reachable by every cluster member; the registry is east-west traffic and the
  public edge domain is intentionally not registered. External consumers use an
  explicit per-service override or the public edge;
- choose `GATEWAY_DISCOVERY_LOAD_BALANCER` (`priority`, `round_robin`, `random`,
  `weighted_random`) for the traffic pattern;
- for a published host port (for example `3913:8080`), set
  `GATEWAY_PRIVATE_HTTP_URL=http://<host-lan-ip>:3913`; `GATEWAY_ADVERTISE_IP`
  uses the bound container port and suits host/overlay networking where that
  port is directly reachable;
- no port or domain is hard-coded in SDK clients: the registry supplies the
  endpoint.

## 5. Verification

Inspect the table from the gateway database:

```sql
SELECT service_name, instance_id, endpoint, status, priority, weight,
       expires_at_ms > (EXTRACT(EPOCH FROM clock_timestamp()) * 1000) AS live
FROM discovery_service_instance
WHERE deleted_at IS NULL
ORDER BY service_name, priority, weight DESC;
```

Gateway startup log markers:

```text
HTTP service registry registered this gateway instance
embedded cloud router turn surface installed for in-process dispatch
```

Health:

```sh
curl -fsS -H 'Host: <allowed-host>' http://127.0.0.1:3913/readyz
```

Behaviour checks:

- restart the gateway: the row count must not grow (idempotent upsert);
- stop the gateway: `discover` no longer returns its rows (deregistered);
- kill the gateway without shutdown: rows disappear after the 30s lease, and
  the one-minute sweep soft-deletes them;
- remove the registry database: the gateway still starts and uses topology
  values (fail-open).

## 6. Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| Turn/media call dials `http://127.0.0.1:3900` | process profile is `split` without a registry target | ensure the canonical `SDKWORK_DEPLOYMENT_PROFILE=standalone` and the gateway registered; check the table |
| Registry table empty after startup | DB unreachable / no DDL permission | check gateway logs for `HTTP service registry ...` warnings; grant CREATE/INSERT/UPDATE on the schema |
| Repeated `lease expired; re-registering` | heartbeat blocked or DB latency | check DB connectivity/timeouts; lease is 30s, heartbeat 10s |
| Consumers outside the compose network cannot reach the endpoint | the registry advertises a LAN address by design | use an explicit per-service override or the public edge; do not register the public domain |
| Registry row shows a public domain | old `GATEWAY_PUBLIC_HTTP_URL` mapping | set `GATEWAY_PRIVATE_HTTP_URL` / `GATEWAY_ADVERTISE_IP`; public domains are not registry endpoints |
| Stale rows accumulate | no discovery control plane and no producer sweep | the gateway sweeps every minute; verify `reap_expired` logs |

## 7. Security

- The registry contains endpoints and metadata, never credentials.
- Database access uses the process service account; do not grant cross-tenant
  registry visibility without a namespace policy.
- Media/agent identity headers are still applied per call; the registry only
  selects the base URL.
