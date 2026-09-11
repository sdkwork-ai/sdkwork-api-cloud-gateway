# sdkwork-api-cloud-gateway — Host Provisioning Utilities (infra only)

> **Channel contract (`MODULE_BIN_SPEC.md` §1)**: the operator surface is the
> `bin/` nine-entrypoint family (`bin/docker-image.sh`, `bin/docker-deploy.sh`,
> ...). The former deploy toolkit orchestrators (`apply.sh`, `apply.ps1`,
> `wsl-redeploy-envs.sh`, `wsl-up-three-envs.sh`, `generate-nginx-conf.sh`,
> `install-certs.sh`) have been removed; `sdkwork-webserver` owns the public
> reverse proxy edge and the host TLS material it terminates.

Remaining files are host / WSL provisioning infrastructure consumed by the
compose topology — they never start or upgrade the application stack:

| File | Responsibility |
| --- | --- |
| `prepare-host.sh` | Install/start host PostgreSQL + Redis for external-deps mode; issue the PostgreSQL CA + server cert and write `docker/ca/postgres-ca-bundle.crt` (the gateway's TLS trust mount) |
| `provision-external-databases.sh` | Create the per-environment database / schema / role from `external-schema.sql` |
| `external-schema.sql` | Parameterized DDL applied by `provision-external-databases.sh` |
| `wsl-reset-db-passwords.sh` | Reset local development database passwords |
| `bind-windows-hosts.ps1` | Bind the registered public domains to `127.0.0.1` in the Windows hosts file |

These scripts are standalone: they source nothing from `../lib/` — that
directory holds the entrypoint libraries (`bootstrap.sh`, `module.sh`) used only
by the nine `bin/*.sh` entrypoints. The canonical deployment matrix
(environments, ports, database identities) is
`scripts/deployment-environments.mjs`; a shell copy is deliberately not kept
(single source of truth).

Domain matrix note: `bind-windows-hosts.ps1` carries the registered base-domain
family inline. It `MUST` stay aligned with `scripts/deployment-environments.mjs`
and `etc/sdkwork.deployment.config.json` `cloudApiBaseUrl`; the alignment is
asserted by `scripts/deployment-contract.test.mjs`.
