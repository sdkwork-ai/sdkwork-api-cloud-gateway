# Deployments

Purpose: deployment topology, infrastructure descriptors, release handoff files, and deployment runbooks.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: Docker, Kubernetes, systemd, environment topology, release handoff, and deployment runbook examples. Declarative `deployments/webserver/` is deploy-time source for the **independent** sdkwork-webserver reverse-proxy product — this gateway runtime does **not** install nginx site mappings.

Forbidden content: live secrets, private keys, local override files, runtime user config, and mutable runtime state.

Related specs: `../sdkwork-specs/APPLICATION_DEPLOY_LAYOUT_SPEC.md`, `../sdkwork-specs/RUNTIME_DIRECTORY_SPEC.md`, `../sdkwork-specs/DEPLOYMENT_SPEC.md`, `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`.

Canonical install / container config root (`topology.applicationCode = api_gateway`):

| Purpose | Path |
| --- | --- |
| Config | `/etc/sdkwork/api_gateway/config.toml` |
| Process env | `/etc/sdkwork/api_gateway/api_gateway.env` |
| Secrets (host) | `/etc/sdkwork/api_gateway/secrets/` |
| Data | `/var/lib/sdkwork/api_gateway` |
| Install | `/opt/sdkwork/api_gateway` |

Verification: `node ../sdkwork-specs/tools/check-application-deploy-layout.mjs --root .`

## Port Matrix

Every deployment mode and environment occupies a distinct host port so
development, test, production, Docker, and direct WSL/Ubuntu deployments can
run side by side without conflicts. Container-internal bind stays `3900`
(single HTTP ingress); only the host-side mapping differs.

| Port | Owner | Deployment | Environment |
| --- | --- | --- | --- |
| 3900 | Gateway HTTP (127.0.0.1) | `pnpm dev` / WSL direct run | development |
| 3901 | Vite dev server | `pnpm dev` renderer | development |
| 3902 | Reserved client-only ingress | — | — |
| 3903 | sdkwork-cloudrouter container | docker compose | shared |
| 3904 | sdkwork-knowledgebase container | docker compose | shared |
| 3905 | sdkwork-cloudrouter dev standalone server | `pnpm dev` (cloudrouter workspace) | development |
| 3906 | Gateway HTTP (127.0.0.1) | WSL direct run (`etc/*.test.toml`) | test |
| 3910 | Gateway container → 3900 | `docker compose` (`GATEWAY_HOST_PORT`) | development/standalone |
| 3911 | Gateway container → 3900 | `docker compose` (`GATEWAY_HOST_PORT=3911`) | test |
| 3912 | Gateway container → 3900 | `docker compose` (`GATEWAY_HOST_PORT=3912`) | staging |
| 3913 | Gateway container → 3900 | `docker compose` (`GATEWAY_HOST_PORT=3913`) | production |
| 3920–3922 | Reserved: container IM realtime link mappings | docker compose | opt-in |
| 18079 | sdkwork-im standalone container | docker compose | shared |
| 19000–19002 | IM realtime TCP/UDP/QUIC link binds (127.0.0.1) | WSL direct run, opt-in | development |
| 19010–19012 | IM realtime TCP/UDP/QUIC link binds (127.0.0.1) | WSL direct run, opt-in | test |
| 19020–19022 | IM realtime TCP/UDP/QUIC link binds (127.0.0.1) | WSL direct run, opt-in | production |

The realtime link range is per environment so development, test, and production
gateways can host embedded IM link transports side by side. QUIC requires
`SDKWORK_IM_REALTIME_QUIC_TLS_CERT_PATH` (and key); without TLS the QUIC
listener is skipped with a warning.

Database per environment: development → `sdkwork_ai_dev`, test →
`sdkwork_ai_test`, staging → `sdkwork_ai_staging`, production → `sdkwork_ai_prod`
(same-named schema), injected via `SDKWORK_DATABASE_*`. Containers and systemd
run the explicit `--migrate-databases` lifecycle command before service startup;
runtime auto-migration remains disabled for production-like environments.
