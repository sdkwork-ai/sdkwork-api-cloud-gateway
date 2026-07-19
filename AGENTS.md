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
  `ENVIRONMENT_SPEC.md`, `RUNTIME_DIRECTORY_SPEC.md`, `DEPLOYMENT_SPEC.md`, and `TEST_SPEC.md`.
- Component or gateway composition: `COMPONENT_SPEC.md`, `COMPOSABLE_ARCHITECTURE_SPEC.md`,
  `APPLICATION_GATEWAY_SPEC.md`, `APP_RUNTIME_TOPOLOGY_SPEC.md`, `APP_COMPOSITION_SPEC.md`, and
  `TEST_SPEC.md`.
- API/SDK surfaces: `API_SPEC.md`, `SDK_SPEC.md`, `APP_SDK_INTEGRATION_SPEC.md`,
  `COMPONENT_SPEC.md`, and `TEST_SPEC.md`; add `PAGINATION_SPEC.md` for list/search work.
- Security/auth/IAM: `IAM_SPEC.md`, `IAM_LOGIN_INTEGRATION_SPEC.md`, `SECURITY_SPEC.md`, and
  `PRIVACY_SPEC.md`.
- Workflow/package/release/deploy: `GITHUB_WORKFLOW_SPEC.md`, `DEPLOYMENT_SPEC.md`,
  `RELEASE_SPEC.md`, `SUPPLY_CHAIN_SECURITY_SPEC.md`, and `PNPM_SCRIPT_SPEC.md`.

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
