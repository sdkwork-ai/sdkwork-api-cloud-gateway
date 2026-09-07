# Repository Guidelines

This repository is an SDKWork application root. Global SDKWork standards live in
`../sdkwork-specs/`; repository contracts narrow those standards but never replace them.

## SDKWORK Soul

Read `../sdkwork-specs/SOUL.md` before repository or application work. Apply specs before memory,
dictionary before context, exact sources before inference, and evidence before completion.

## SDKWORK Standards

The canonical standards entrypoint is `../sdkwork-specs/README.md`. Agent entrypoints follow
`../sdkwork-specs/AGENTS_SPEC.md`; repository layout follows
`../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`. Do not copy global normative bodies into this file.

## Application Identity

- Application id: `sdkwork-api-cloud-gateway`
- Runtime family: Rust/Axum platform cloud gateway
- Primary process and public crate: `crates/sdkwork-api-cloud-gateway`
- Application declaration: `sdkwork.app.config.json`
- Source configuration entrypoint: `etc/sdkwork.deployment.config.json`
- Component and dependency surface contract: `specs/component.spec.json`

`sdkwork.app.config.json` declares application and release identity. Concrete environment, profile,
bind, database selection, and runtime TOML values live under `etc/`.

## Local Dictionary Structure

- `AGENTS.md`: repository agent execution entrypoint.
- `CLAUDE.md`, `GEMINI.md`, `CODEX.md`: compatibility shims that point back here.
- `sdkwork.app.config.json`: SDKWork App Standard v3 identity and release declaration.
- `etc/`: safe source-controlled deployment profiles and gateway runtime configuration.
- `specs/component.spec.json`: executable exports, dependency surfaces, Cargo evidence, and checks.
- `specs/process-database-pool.spec.json`: process-shared PostgreSQL pool contract.
- `.sdkwork/`: source-controlled local AI metadata only; runtime state and secrets stay ignored.
- `apis/`: gateway-owned API contracts and review inputs.
- `crates/`: Rust workspace members for config, registry, observability, and gateway runtime.
- `sdks/`: gateway-owned SDK families when present; generated output is never hand-edited.
- `scripts/`: thin development, packaging, and workflow entrypoints.
- `docs/`: Canon documentation; `docs/superpowers/` is historical only.

Documentation Canon:

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Spec Resolution Order

Use dynamic progressive loading before reading implementation files:

1. Read the nearest `AGENTS.md`.
2. Read `sdkwork.app.config.json` when application identity, runtime, release, or packaging is in scope.
3. Read the nearest module `specs/` and this root `specs/` when their contracts are touched.
4. Read `.sdkwork/README.md` and only the relevant local skill/plugin metadata when applicable.
5. Resolve the task row in `../sdkwork-specs/README.md`.
6. Read only the task-specific global specs selected by that row or the component contract.
7. Inspect the affected implementation files.

## Required Specs By Task Type

- Agent/workflow changes: `SOUL.md`, `AGENTS_SPEC.md`, `SDKWORK_WORKSPACE_SPEC.md`,
  `DOCUMENTATION_SPEC.md`, and `TEST_SPEC.md`.
- Any authored code: `CODE_STYLE_SPEC.md`, `NAMING_SPEC.md`, and only the touched language spec.
- Build/dev Node runners: `CODE_STYLE_SPEC.md` section 7, `TYPESCRIPT_CODE_SPEC.md` section 5,
  `PNPM_SCRIPT_SPEC.md`, and `TEST_SPEC.md`.
- Rust/Cargo: `RUST_CODE_SPEC.md` and `TEST_SPEC.md`.
- Source config/runtime environment: `SOURCE_CONFIG_SPEC.md`, `CONFIG_SPEC.md`,
  `ENVIRONMENT_SPEC.md`, `RUNTIME_DIRECTORY_SPEC.md`, `DEPLOYMENT_SPEC.md`, `DOCKER_SPEC.md`, and `TEST_SPEC.md`.
- Component or gateway composition: `COMPONENT_SPEC.md`, `COMPOSABLE_ARCHITECTURE_SPEC.md`,
  `APPLICATION_GATEWAY_SPEC.md`, `APP_RUNTIME_TOPOLOGY_SPEC.md`, `APP_COMPOSITION_SPEC.md`, and
  `TEST_SPEC.md`.
- API/SDK surfaces: `API_SPEC.md`, `SDK_SPEC.md`, `APP_SDK_INTEGRATION_SPEC.md`,
  `COMPONENT_SPEC.md`, and `TEST_SPEC.md`; add `PAGINATION_SPEC.md` for list/search work.
- Security/auth/IAM: `IAM_SPEC.md`, `IAM_LOGIN_INTEGRATION_SPEC.md`, `SECURITY_SPEC.md`, and
  `PRIVACY_SPEC.md`.
- Workflow/package/release/deploy: `GITHUB_WORKFLOW_SPEC.md`, `DEPLOYMENT_SPEC.md`, `DOCKER_SPEC.md`,
  `RELEASE_SPEC.md`, `SUPPLY_CHAIN_SECURITY_SPEC.md`, and `PNPM_SCRIPT_SPEC.md`.

## Int64 Wire Contract (API_SPEC §13.6)

- OpenAPI `int64` fields and parameters `MUST` be `type: string`, `format: int64`,
  a decimal `pattern` such as `^-?[0-9]+$`, and `x-sdkwork-int64-string: true`.
  `type: integer, format: int64` is a contract violation: generated TypeScript
  SDKs then emit `number`, and browsers silently round ids past
  `Number.MAX_SAFE_INTEGER` (2^53), replaying wrong ids into lookups.
- Rust response DTOs `MUST` serialize `i64` wire fields with
  `#[serde(with = "sdkwork_utils_rust::serde_int64")]` (or `::option`); request
  boundaries parse inbound strings with the same helper.
- Generated TypeScript SDKs keep `int64` as `string`; frontend code `MUST NOT`
  convert ids/snowflake ids/sequence ids to `number` for storage, comparison,
  or submission.
- Verification: `node <sdkwork-specs>/tools/check-api-operation-patterns.mjs --workspace .`

## Code Style Rules

- Keep Rust `src/lib.rs` limited to module declarations, re-exports, and lightweight wiring.
- Put handlers, config parsing, validation, runtime composition, and dispatch behavior in focused
  responsibility modules following `RUST_CODE_SPEC.md`.
- Node scripts follow `TYPESCRIPT_CODE_SPEC.md`; shared workflow logic belongs in reusable tools,
  while repository scripts remain thin entrypoints.
- Use canonical lower-kebab file/package identities and Rust lower-snake module names from
  `NAMING_SPEC.md`.

## Agent Execution Rules

- Independent startup uses `crates/sdkwork-api-cloud-gateway`; embedding uses public router builders
  from `sdkwork-api-cloud-gateway`.
- Cargo workspace dependencies, crate features, executable router exports, and
  `specs/component.spec.json` are the gateway integration authority. Do not create another gateway
  catalog or copy dependency surfaces into runtime TOML or the app manifest.
- Foundation dependency surfaces are embedded-only. Every surface requires a Cargo feature,
  assembly dependency, executable router export, and coverage evidence.
- Do not reimplement dependency-owned Appbase IAM routes under `/app/v3/api/auth`,
  `/app/v3/api/iam`, `/app/v3/api/oauth`, `/app/v3/api/system/iam`, or `/backend/v3/api/iam`.
- Other applications integrate foundation APIs through this gateway rather than directly binding
  those API implementations.
- Do not hand-edit generated SDK output. Fix the source contract, generator input, approved facade,
  or native dependency and regenerate through the approved toolchain.
- Build-critical tracked sources are verified and self-healed before builds per
  `CODE_STYLE_SPEC.md` section 7. `pnpm clean` must never remove them.

## Build, Test, and Verification

Run commands from this repository root. Use narrow checks while developing, then the merge-ready
aggregate before completion:

```powershell
cargo metadata --no-deps --format-version 1
cargo fmt -- --check
cargo test --workspace
pnpm check
pnpm verify
pnpm workflow:validate
pnpm gateway:matrix:standalone
pnpm gateway:package:standalone
pnpm gateway:validate:standalone
```

Do not use `cargo fmt --all`; optional foundation paths resolve sibling application workspaces
outside this repository authority. When `pnpm dev` changes, start it on a temporary bind, verify
`/healthz`, and stop the process after the smoke check.

## Packaging And Deployment (bin/ entrypoints — MANDATORY)

**All image build, package, deploy, rollback, log, backup, and config operations MUST go through
the `bin/` entrypoints.** Never invoke `docker build`, `docker compose`, `cargo build --release`,
or hand-edit a deployed bundle directly — the entrypoints resolve the image tag from
`sdkwork.app.config.json` → `release.currentVersion`, bridge Windows→WSL automatically
(`sdkwork_local_run`), pass the full registry reference required by `DOCKER_SPEC.md` §2.1, and
append audit evidence to `target/bin-evidence/evidence.log`. Raw `docker`/`cargo` calls bypass
version pinning, WSL bridging, and the evidence trap.

Full reference: [`bin/README.md`](bin/README.md). Entrypoint contract:
`sdkwork-specs/MODULE_BIN_SPEC.md`.

### Release workflow (build → deploy → verify)

```sh
# 0. Bump release.currentVersion in sdkwork.app.config.json FIRST (tag source of truth).
# 1. Build the container image (cargo release + docker build, runs in WSL; 10-30 min).
bin/docker-image.sh build --image-tag 0.1.3
# 2. Verify + deploy per environment (deployment mutations in production require --yes).
bin/docker-deploy.sh install --environment development
bin/docker-deploy.sh install --environment test
bin/docker-deploy.sh install --environment staging
bin/docker-deploy.sh install --environment demo
bin/docker-deploy.sh install --environment production --yes
# 3. Verify after each environment: containers healthy + healthz probe + status.
bin/docker-deploy.sh status --environment <env>
wsl -e bash -lc "curl -s -H 'Host: gateway' http://127.0.0.1:<port>/healthz"   # dev=3910 test=3911 staging=3912 demo=3914 prod=3913
```

Versioning rules: a behavior-change rebuild always gets a **new version tag** — bump
`release.currentVersion` before building; never reuse or overwrite an existing tag so rollback
targets stay addressable. If `cargo metadata --locked` fails with "cannot update the lock file",
the lock is out of sync with the workspace manifests: regenerate it (run `cargo metadata
--format-version 1` with the same feature set) and commit `Cargo.lock` **before** rebuilding.

### Operational entrypoints

```sh
bin/docker-image.sh <build|push|save|load|update|inspect> [--image-tag <v>]
bin/docker-deploy.sh <install|upgrade|rollback|status|logs|down|start|stop|restart|check-config|doctor> --environment <env>
                     [--image-tag <v>] [--deps external|embedded] [--to <version>] [--yes] [--dry-run]
bin/config.sh  <list|show|get|set|diff|validate|edit> --environment <env>   # live bundle config, secrets redacted
bin/doctor.sh  --environment <env>                                          # read-only, exit 70 on failure
bin/backup.sh  <create|list|verify|restore> --environment <env>
bin/docker-deploy.sh logs --environment <env> [--tail N|all] [--since <d>] [--follow] [--export <dir>]
```

Safety rules: run `--dry-run` first when the command is unfamiliar or the environment is not
dev; production mutations always require `--yes`; `--purge` requires `--yes` in every
environment; prefer `upgrade` over `down`+`install` so bundle state and release history persist;
`rollback --to <version>` uses bundle-owned release history. The host-native channel
(`bin/apps-build.sh` / `apps-package.sh` / `apps-deploy.sh`) covers **production-only** bare-metal
delivery — every containerized environment uses the `docker-image.sh`/`docker-deploy.sh` path.

## Task-Specific Gates

- App SDK consumer changes:
  `node ../sdkwork-specs/tools/check-app-sdk-consumer-imports.mjs --workspace .`
- API contract changes:
  `node ../sdkwork-specs/tools/check-api-operation-patterns.mjs --workspace .` and
  `node ../sdkwork-specs/tools/check-api-response-envelope.mjs --workspace .`
- Pagination changes:
  `node ../sdkwork-specs/tools/check-pagination.mjs --workspace .`
- Source config changes:
  `node ../sdkwork-specs/tools/check-source-config-standard.mjs --root .`
- Agent entrypoint changes:
  `node ../sdkwork-specs/tools/check-agent-workflow-standard.mjs --root .`

## Human Review Rules

Human review is required for breaking public API/SDK or standards changes, security exceptions,
migration compatibility windows, release/publication policy changes, destructive filesystem work,
and generated SDK ownership changes.

<!-- SDKWORK-NAMING-STANDARD: v1 -->
## Rust Naming And Dependency Declaration

Authority: `../sdkwork-specs/NAMING_SPEC.md` section 3.1 and section 3.2.

Two identifier planes exist in every Rust crate and they MUST NOT be mixed: the package plane
(Cargo, filesystem, lock file) uses kebab-case, and the crate plane (lib target, modules, source
imports) uses snake_case.

- `[package].name`, the crate directory, `[features]` keys, and `[[bin]].name` use kebab-case.
- `[lib].name`, module files, module directories, and Rust imports use snake_case.
- A crate whose `[package].name` contains a hyphen SHOULD declare `[lib].name` explicitly
  (default: package name with every `-` replaced by `_`). A shorter lib name is allowed only
  when declared explicitly and used consistently by every consumer.
- Cargo dependency keys, `[workspace.dependencies]` keys, and `Cargo.lock` entries use the
  dependency package name. Use `package = "..."` when an alias is required.
- Every external crate referenced by `src/` MUST be declared in that crate's `[dependencies]`.
  Test-only crates belong in `[dev-dependencies]`; `build.rs` crates belong in
  `[build-dependencies]`.
- Never delete a dependency line, and never demote one from `[dependencies]` to
  `[dev-dependencies]`, while `src/` still imports it. Verify manifest cleanups with the
  command below before committing them.
- Regenerate and commit `Cargo.lock` in the same change as any dependency table edit.

Verification:

```bash
node ../sdkwork-specs/tools/check-rust-crate-naming-standard.mjs --root .
```
<!-- /SDKWORK-NAMING-STANDARD: v1 -->

<!-- SDKWORK-RUST-CODE-STANDARD: v1 -->
## Rust Code Standard

Authority: `../sdkwork-specs/RUST_CODE_SPEC.md` (v2, industry-best baseline); package/crate
naming and dependency declaration are normative in `../sdkwork-specs/NAMING_SPEC.md` section 3.1
and 3.2.

- Crates are responsibility-shaped: service, repository-sqlx, routes, service-host, native-host,
  worker, assembly, gateway. No generic `core`/`common`/`backend`/`runtime` suffixes.
- Errors are typed enums (`thiserror`) implementing `std::error::Error` with a `source` chain.
  `anyhow` only at binary/CLI/test boundaries, never in lib `[dependencies]`.
- No `unsafe` without a `// SAFETY:` comment; crates default to `unsafe_code = "forbid"`.
  No `unwrap`/`expect`/`panic!`/`todo!`/`dbg!` in library code reachable from public API.
- No lock guard held across `.await`; every external await has a timeout; spawned tasks are
  awaited/detached with a documented owner; retries are bounded, jittered, and idempotent.
- Public API is minimal, documented, `#[must_use]` where applicable, and semver-clean. Leaking
  framework types (`sqlx::Row`, axum extractors) through public signatures is forbidden.
- Workspace root declares `[workspace.package]` (edition, rust-version) and `[workspace.lints]`
  (RUST_CODE_SPEC.md section 13 baseline); every member inherits both with
  `edition.workspace = true` and `[lints] workspace = true`.

Verification:

```bash
node ../sdkwork-specs/tools/check-rust-crate-naming-standard.mjs --root .
node ../sdkwork-specs/tools/check-rust-manifest-standard.mjs --root .
# when service/repository/route/gateway dependencies change:
node ../sdkwork-specs/tools/check-rust-backend-composition.mjs --root .
```
<!-- /SDKWORK-RUST-CODE-STANDARD: v1 -->

<!-- SDKWORK-PNPM-WORKSPACE-STANDARD: v1 -->
## pnpm Workspace Dependency And Package Import

Authority: `../sdkwork-specs/PNPM_WORKSPACE_DEPENDENCY_SPEC.md` (companion to
`../sdkwork-specs/DEPENDENCY_MANAGEMENT_SPEC.md`).

Sibling SDKWork repositories are consumed through a dual-track model that MUST stay consistent:

- **Local development** (`pnpm dev`, `pnpm build`): pnpm workspace protocol. Each sibling
  package is declared ONCE in this repository root `pnpm-workspace.yaml` `packages:` as a
  `../sdkwork-*` relative path, and consumed with `workspace:*` in `package.json`. Never use
  `file:`/`link:`/git-URL specifiers for SDKWork sibling packages in any environment.
- **CI / release packaging**: git-repository dependency checkout. Every sibling referenced by the
  local workspace MUST have a matching `dependencies[]` entry in `sdkwork.workflow.json` so CI
  clones the sibling into the same `../sdkwork-*` relative layout (`GITHUB_WORKFLOW_SPEC.md`).
  `package.json` is never rewritten for CI.

Import rules for sibling SDKWork packages:

- Import by package name only: `import { X } from "@sdkwork/package-name"`. The specifier MUST
  equal the target package's `package.json` `name` exactly - no shortening, renaming, or alias.
- Forbidden: relative imports that cross a package boundary into another SDKWork repository or
  another workspace package's `src/` (for example `import ... from "../../sdkwork-appbase/.../src/..."`).
- Consume only the public `exports` surface of a package; never deep-import sibling `src/` internals.
- Every non-relative import in a workspace member MUST resolve to that member's own
  `dependencies`/`devDependencies`/`peerDependencies` (import closure).
- Vite aliases MUST NOT rename or redirect `@sdkwork/*` packages, MUST NOT be added to make a
  resolution error pass, and are allowed only for documented bootstrap/SDK-generation entrypoints.
- Fix a resolution failure by correcting the workspace declaration or the package `exports`,
  not by adding an alias.

Verification:

```bash
node ../sdkwork-specs/tools/verify-repo.mjs --root .
node ../sdkwork-specs/tools/check-workspace-member-protocol.mjs --root .
node ../sdkwork-specs/tools/check-dependency-list-completeness.mjs --target <repo-name>
```
<!-- /SDKWORK-PNPM-WORKSPACE-STANDARD: v1 -->

<!-- SDKWORK-SDK-GENERATION-STANDARD: v1 -->
## Generated SDK Output Is Generator-Owned

Authority: `../sdkwork-specs/SDK_SPEC.md` and `../sdkwork-specs/SDK_WORKSPACE_GENERATION_SPEC.md`.

Everything generated under `sdks/` — `generated/server-openapi/` trees, generated language
workspaces, `dist/` build output, generated `sdkwork-sdk.json`, generated
`.sdkwork/sdkwork-generator-*` reports, and standardizer-synced OpenAPI snapshots — is produced by
the canonical SDK generator `../sdkwork-sdk-generator/bin/sdkgen.js` (`@sdkwork/sdk-generator`).

- Do not hand-edit generated SDK files, including type definitions, dist bundles, and generated
  package metadata. Manual edits are overwritten by the next generation run and break
  reproducibility and contract audits.
- When generated or compiled SDK output does not meet a contract or standard, fix the upstream
  source — authored API contract, route manifest, OpenAPI authority, derived `*.sdkgen.*` input,
  generator profile, or `custom/` runtime build scripts — then regenerate through the standard
  generation command. Do not patch generated output in place.
- Remove stale generated files by re-running the family generation command, which owns cleanup of
  disappeared routes and models; do not hand-prune generated trees.
- The only approved handwritten surfaces are `custom/` roots inside generated workspaces and
  authored `composed/` facades outside `generated/server-openapi`.

Verification:

```bash
node ../sdkwork-specs/tools/sync-agent-sdk-generation-standard.mjs --root . --check
```
<!-- /SDKWORK-SDK-GENERATION-STANDARD: v1 -->

## Deployment Standard (bin/)

Per `../sdkwork-specs/MODULE_BIN_SPEC.md`, this Rust module ships the
standardized `bin/` entrypoint family; all build/package/deploy work `MUST`
go through them (or the repository pnpm scripts they delegate to):

| Entrypoint | Delegates to |
| --- | --- |
| `bin/docker-image.sh` | `pnpm build:container` → `registry.sdkwork.com/apps/sdkwork-api-cloud-gateway:<version>` |
| `bin/docker-deploy.sh` | `scripts/docker/bundle/deploy.sh` (WSL or remote Ubuntu via `--host`) |
| `bin/apps-build.sh` | `server` → `cargo build --release` |
| `bin/apps-package.sh` | `scripts/package-server.mjs package --deployment-profile <p> --version <v>` → archive collected into `target/bin-packages/` with a sidecar `.sha256` |
| `bin/apps-deploy.sh` | host-native `install/install-ubuntu.sh` on the Ubuntu target + systemd enable/health probe (`production` only; every other environment uses the container bundle) |

| `bin/config.sh` | `ops-config.sh` — deployed-configuration `list/show/get/set/diff/validate/edit`, secrets redacted, backup + validate before mutation (`OPERATIONS_SPEC.md` §3) |
| `bin/doctor.sh` | `ops-observe.sh` — read-only environment diagnostics (9 checks, exit 70 on FAIL, `--json`/`--export`) |
| `bin/backup.sh` | `ops-backup.sh` — backup/list/verify/restore into `/opt/deploy/sdkwork-api-cloud-gateway/backups/` (`OPERATIONS_SPEC.md` §5); the database dump uses the `GATEWAY_POSTGRES_*` keys |
| `bin/docker-deploy.sh logs` | `--instance/--service/--tail/--since/--follow/--export` (bounded read by default) |
- App types declared: `server`; environments: `development`, `test`,
  `staging`, `demo`, `production`.
- The install bundle is synced to
  `/opt/deploy/sdkwork-api-cloud-gateway/bundle` on the target and
  `deploy.sh` always runs with that directory as its working directory;
  `--image-tag`, `--replicas` and `--deps external|embedded` are forwarded to
  it. `rollback` uses the bundle-owned `release.sh` (`--to <version>`).
- Container port `3900`, host ports `3910-3914` per environment
  (`DOCKER_SPEC.md` §3.2); image naming and bundle layout per
  `DOCKER_SPEC.md`; external host-system PostgreSQL/Redis per
  `DEPLOYMENT_SPEC.md` §6.1. The gateway `MUST NOT` publish public
  `:80`/`:443` — the public edge is `sdkwork-webserver`.
- The host-native server package ships a single production config template,
  so that channel covers `production` only; every other environment uses the
  container path, and the entrypoint fails fast with that guidance.
- Mutating commands support `--dry-run`; production mutations require
  `--yes`; `--purge` requires `--yes` in every environment; each run appends
  its exit status to `target/bin-evidence/evidence.log`.
- Validate with: `node ../sdkwork-specs/tools/check-module-bin.mjs --root .`
