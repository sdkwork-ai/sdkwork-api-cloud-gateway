# sdkwork-api-cloud-gateway — Host Provisioning Utilities (infra only)

> **Channel contract (`MODULE_BIN_SPEC.md` §1)**: the operator surface is the
> `bin/` nine-entrypoint family (`bin/docker-image.sh`, `bin/docker-deploy.sh`,
> ...). The former deploy toolkit orchestrators (`apply.sh`, `apply.ps1`,
> `wsl-redeploy-envs.sh`, `wsl-up-three-envs.sh`, `generate-nginx-conf.sh`,
> `install-certs.sh`) have been removed; `sdkwork-webserver` owns the public
> reverse proxy edge.

Remaining files are host/TLS provisioning infrastructure consumed by the
compose topology — they never start or upgrade the application stack:

| File | Responsibility |
| --- | --- |
| `generate-certs.sh` | Generate the PostgreSQL/RPC TLS material under `docker/ca/` |
| `prepare-host.sh` | Install/start host PostgreSQL + Redis for external-deps mode |
| `wsl-reset-db-passwords.sh` | Reset local development database passwords |
| `lib/` | Shared helpers for the scripts above |
