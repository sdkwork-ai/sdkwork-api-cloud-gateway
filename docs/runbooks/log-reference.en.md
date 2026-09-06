# Log Runbook (sdkwork-api-cloud-gateway)

Log policy (OPERATIONS_SPEC.md §2): `json-file` driver, `max-size 50m`,
`max-file 3`; services write to stdout/stderr only. Read logs through
`docker compose logs` — never open `/var/lib/docker/containers/...` directly.

## 1. Reading logs (bounded by default, never hangs)

```bash
bin/docker-deploy.sh logs --environment demo                       # instance 1, last 200 lines
bin/docker-deploy.sh logs --environment demo --tail 1000
bin/docker-deploy.sh logs --environment demo --since 15m
bin/docker-deploy.sh logs --environment demo --instance 2
bin/docker-deploy.sh logs --environment demo --service sdkwork-api-cloud-gateway
bin/docker-deploy.sh logs --environment demo --follow              # opt-in streaming
```

## 2. Exporting logs (incident attachment)

```bash
bin/docker-deploy.sh logs --environment demo --tail 5000 --export ./incident
# writes incident/gateway-demo-i1-| Environment | Health port |
|---|---|
| development | 3910 |
| test | 3911 |
| staging | 3912 |
| demo | 3914 |
| production | 3913 |-<UTC>.log.gz + .sha256
```

Treat exports as secret-adjacent material.

## 3. Health ports

/readyz

Probe: `bin/doctor.sh --environment demo` checks container health and
`http://127.0.0.1:<port>GATEWAY_POSTGRES_*`.

## 4. Known log signatures

| Signature | Meaning | Action |
|---|---|---|
| `panicked at` | process-level crash | container restarts; capture logs and escalate |
| many `connection refused` (DB/Redis) | dependency unreachable | check `GATEWAY_POSTGRES_*` HOST/PORT/password |
| `not healthy` / healthcheck timeout | slow start or blocked dependency | `doctor.sh` compose + probe checks |
| no output after start | restart loop | `doctor.sh` health check reports the count |

## 5. Log levels

`RUST_LOG` (or the equivalent key) controls the level: `development/test`
default to `debug`, `staging/demo/production` to `info`. Enabling `debug` in
production requires a change record and an expiry.
