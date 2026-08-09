# Deployments

Purpose: deployment topology, infrastructure descriptors, release handoff files, and deployment runbooks.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: Docker, Kubernetes, systemd, nginx, environment topology, release handoff, and deployment runbook examples.

Forbidden content: live secrets, private keys, local override files, runtime user config, and mutable runtime state.

Related specs: `../sdkwork-specs/DEPLOYMENT_SPEC.md`, `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`, and `../sdkwork-specs/RUNTIME_DIRECTORY_SPEC.md`.

Verification: deployment artifacts must be validated by `sdkwork.workflow.json` lifecycle steps or documented smoke checks before release.

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
| 3906 | Gateway HTTP (127.0.0.1) | WSL direct run (`etc/*.test.toml`) | test |
| 3910 | Gateway container → 3900 | `docker compose` (`GATEWAY_HOST_PORT`) | development/standalone |
| 3911 | Gateway container → 3900 | `docker compose` (`GATEWAY_HOST_PORT=3911`) | test |
| 3912 | Gateway container → 3900 | `docker compose` (`GATEWAY_HOST_PORT=3912`) | production |
| 3913–3915 | Reserved: container IM realtime link mappings | docker compose | opt-in |
| 18079 | sdkwork-im standalone container | docker compose | shared |
| 19000 | IM realtime TCP link (127.0.0.1) | WSL direct run, opt-in | any |
| 19001 | IM realtime UDP link (127.0.0.1) | WSL direct run, opt-in | any |
| 19002 | IM realtime QUIC link (127.0.0.1) | WSL direct run, opt-in | any |

Database per environment: development → `sdkwork_ai_dev`, test →
`sdkwork_ai_test`, production → `sdkwork_ai_prod` (same-named schema), injected
via `SDKWORK_DATABASE_*` env; `SDKWORK_DATABASE_AUTO_MIGRATE=true` creates the
schema and tables on first start.

