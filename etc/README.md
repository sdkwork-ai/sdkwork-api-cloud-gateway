# Source Configuration

`sdkwork.deployment.config.json` is the source-controlled deployment profile index for the
SDKWork API Cloud Gateway. It selects a typed profile from `topology/`; the selected profile owns
the lifecycle environment, deployment profile, runtime target, bind address, gateway runtime TOML,
and database engine selection.

## Supported Profiles

| Profile | Purpose |
| --- | --- |
| `standalone.development` | Default `pnpm dev` profile; starts the local gateway with PostgreSQL. |
| `cloud.development` | Validates the deployed development gateway URL and starts no local service. |
| `standalone.production` | Source template for a standalone server deployment. |
| `cloud.production` | Source template for the hosted platform gateway deployment. |

The gateway runtime files are:

- `sdkwork-api-cloud-gateway.development.toml`
- `sdkwork-api-cloud-gateway.test.toml`
- `sdkwork-api-cloud-gateway.production.toml`

Dependency API inventory does not live in these TOML files. `specs/component.spec.json`, Cargo
workspace dependencies, Cargo features, and executable router exports are the integration
authority; runtime bootstrap projects that component contract into the selected operational
profile.

## Local Overrides And Secrets

The tracked profiles contain no passwords, tokens, private keys, or developer-specific paths.
Local PostgreSQL credentials come from ignored `.env.postgres`, bootstrapped from
`.env.postgres.example` when missing. Files matching `etc/**/*.local.*` and `etc/secrets/` are
private and must remain untracked. Production secrets are injected by the deployment platform or
referenced protected secret files.

Installed runtime configuration is materialized under the platform path defined by
`RUNTIME_DIRECTORY_SPEC.md`; source `etc/` is the reviewed input, not the installed directory.

## Verification

```powershell
node ../sdkwork-specs/tools/check-source-config-standard.mjs --root .
node --test scripts/dev/sdkwork-api-cloud-gateway-source-config.test.mjs
pnpm check
```
