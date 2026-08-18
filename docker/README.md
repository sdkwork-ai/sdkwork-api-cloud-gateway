# Docker deployment assets

Multi-environment standalone container deployment for WSL Ubuntu / Linux hosts.

## Quick start (external PostgreSQL + Redis)

```bash
# 1. Provision databases (sdkwork_ai_dev/test/staging/prod)
pnpm docker:provision:db

# 2. Configure secrets in docker/env/<environment>.env (from *.env.example)

# 3. Build image and deploy
pnpm build:container
pnpm docker:deploy development --validate
# or: pnpm docker:deploy all --validate

# 4. Install nginx port-80 sites
sudo pnpm docker:nginx:install all
```

## Port and domain matrix

| Environment | Host port | Database | Domains |
| --- | --- | --- | --- |
| development | 3910 | `sdkwork_ai_dev` | `api-dev.{sdkwork,birdcoder,dtupay}.com` |
| test | 3911 | `sdkwork_ai_test` | `api-test.{sdkwork,birdcoder,dtupay}.com` |
| staging | 3912 | `sdkwork_ai_staging` | `api-staging.{sdkwork,birdcoder,dtupay}.com` |
| production | 3913 | `sdkwork_ai_prod` | `api.{sdkwork,birdcoder,dtupay}.com` |

## Layout

```
docker/
├── config/           # Container TOML override mount
├── env/              # Per-environment compose env (examples committed; *.env gitignored)
├── nginx/            # Port 80/443 reverse-proxy site configs
├── postgres/         # External schema SQL + compose init scripts
└── ca/               # Per-host PostgreSQL CA bundle (gitignored except README)
```

## Commands

| Script | Purpose |
| --- | --- |
| `scripts/deploy-docker-environment.sh` | Start/stop one or all environments (external mode) |
| `scripts/install-nginx-sites.sh` | Install nginx sites to `/etc/nginx/sites-enabled/sdkwork/` |
| `scripts/provision-external-databases.sh` | Create sdkwork-specs workspace databases on external PG |
| `scripts/package-docker-environments.sh` | Bundle per-env deploy packages to `dist/docker-packages/` |

Full operator guide: [`docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md`](../docs/guides/operator/REMOTE_DEPLOY_UBUNTU_DOCKER.md).
