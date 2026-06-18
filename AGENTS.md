# Repository Guidelines

This repository is an SDKWork application root. Before changing runtime behavior, config, API
surfaces, SDK wiring, release metadata, packaging, or repository standards, follow the workspace
rules in `../AGENTS.md` and the canonical SDKWork specs under `../sdkwork-specs/`.

## SDKWORK Soul

Read `../sdkwork-specs/SOUL.md` before executing repository or application tasks. It defines the
shared SDKWork execution rules: specs before memory, dictionary before context, exact source before
inference, and evidence before completion.

## SDKWORK Standards

The canonical standards entrypoint is `../sdkwork-specs/README.md`. This repository follows those
root standards and does not maintain a local copy of SDKWork-wide rules. Agent entrypoints and tool
compatibility shims are governed by `../sdkwork-specs/AGENTS_SPEC.md`; the repository/application
layout is governed by `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.

## Application Identity

- Application id: `sdkwork-api-gateway`
- Runtime family: Rust backend service
- Primary service process: `crates/sdkwork-api-gateway-api-server`
- Public gateway crate: `crates/sdkwork-api-gateway`
- Manifest source of truth: `sdkwork.app.config.json`
- Component contract: `specs/component.spec.json`

## Local Dictionary Structure

- `AGENTS.md`: repository/application agent execution entrypoint.
- `CLAUDE.md`, `GEMINI.md`, `CODEX.md`: compatibility shims that point back to this file.
- `sdkwork.app.config.json`: application identity, runtime defaults, env bindings, API surfaces, and
  dependency SDK surfaces.
- `specs/component.spec.json`: executable modules, public exports, gateway integration authority,
  dependency surfaces, and verification commands.
- `.sdkwork/`: source-controlled local AI workspace metadata, skills, and plugins only; runtime
  state, caches, secrets, and temp files stay ignored.
- `apis/`: authored API contract inputs and API review material when this gateway owns an API.
- `crates/`: Rust workspace members for config, registry, observability, gateway runtime, and API
  server process crates.
- `sdks/`: SDK family and generated SDK workspaces when gateway-owned SDKs exist.
- `configs/`: safe tracked TOML templates and non-secret defaults. Do not place live tokens,
  credentials, or host-local overrides in tracked files.
- `scripts/`: thin command entrypoints for packaging and workflow integration; reusable logic belongs
  in `tools/` or a crate.
- `docs/`: repository documentation and historical execution records. `docs/superpowers/` is
  historical only and is not a current source of truth.

## Spec Resolution Order

1. Read this `AGENTS.md`.
2. Read `sdkwork.app.config.json` for application identity and runtime surfaces.
3. Read `specs/component.spec.json` and any relevant local specs.
4. Read `.sdkwork/README.md`, `.sdkwork/skills/`, and `.sdkwork/plugins/` when the task touches
   local agent/plugin metadata.
5. Resolve `../sdkwork-specs/README.md`, `../sdkwork-specs/SOUL.md`, and
   `../sdkwork-specs/AGENTS_SPEC.md`.
6. Read only the task-specific root specs from `../sdkwork-specs/`.
7. Inspect implementation files after the relevant standards are clear.

## Required Specs By Task Type

- Agent/workflow or tool shim changes: `../sdkwork-specs/SOUL.md`,
  `../sdkwork-specs/AGENTS_SPEC.md`, `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`, and
  `../sdkwork-specs/DOCUMENTATION_SPEC.md`.
- Repository layout changes: `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`,
  `../sdkwork-specs/NAMING_SPEC.md`, and `../sdkwork-specs/DOCUMENTATION_SPEC.md`.
- Any authored code change: `../sdkwork-specs/CODE_STYLE_SPEC.md`,
  `../sdkwork-specs/NAMING_SPEC.md`, and only the touched language/framework spec.
- Rust source or Cargo changes: `../sdkwork-specs/RUST_CODE_SPEC.md` and
  `../sdkwork-specs/TEST_SPEC.md`.
- API, SDK, or dependency surface changes: `../sdkwork-specs/API_SPEC.md`,
  `../sdkwork-specs/SDK_SPEC.md`, `../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`,
  `../sdkwork-specs/COMPONENT_SPEC.md`, and `../sdkwork-specs/TEST_SPEC.md`.
- Runtime config or environment changes: `../sdkwork-specs/CONFIG_SPEC.md`,
  `../sdkwork-specs/ENVIRONMENT_SPEC.md`, and `../sdkwork-specs/RUNTIME_DIRECTORY_SPEC.md`.
- GitHub workflow, packaging, release, or deployment changes: `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`,
  `../sdkwork-specs/DEPLOYMENT_SPEC.md`, `../sdkwork-specs/RELEASE_SPEC.md`, and
  `../sdkwork-specs/SUPPLY_CHAIN_SECURITY_SPEC.md`.
- Security, auth, tenant context, or IAM route work: `../sdkwork-specs/IAM_SPEC.md`,
  `../sdkwork-specs/IAM_LOGIN_INTEGRATION_SPEC.md`, `../sdkwork-specs/SECURITY_SPEC.md`, and
  `../sdkwork-specs/PRIVACY_SPEC.md`.

## Code Style Rules

Use the repository's existing Rust/Axum patterns and keep edits scoped to the affected capability.
Rust crates must keep responsibility-specific names from `../sdkwork-specs/RUST_CODE_SPEC.md` and
`../sdkwork-specs/NAMING_SPEC.md`. `src/lib.rs` stays limited to module declarations, re-exports,
and lightweight wiring; handlers, proxy behavior, config parsing, and validation belong in focused
modules.

## Build, Test, and Verification

Run commands from this repository root:

- `cargo metadata --no-deps --format-version 1`
- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `pnpm workflow:validate`
- `pnpm workflow:matrix:server`
- `pnpm package:server`
- `pnpm package:server:validate`

Use narrower commands first while developing, then run the broader verification set before claiming
completion, committing, or pushing. When `pnpm dev` behavior changes, smoke-test `/healthz` with a
temporary bind address and stop the process after the check.

## Agent Execution Rules

- The gateway supports independent startup through `crates/sdkwork-api-gateway-api-server` and
  embedding through public router builders from `sdkwork-api-gateway`.
- Gateway integration authority is Cargo-workspace-first. Do not create a separate gateway catalog;
  use root `Cargo.toml`, crate Cargo features, `sdkwork.app.config.json`, and
  `specs/component.spec.json`.
- Appbase IAM routes are dependency routes. Do not reimplement `/app/v3/api/auth`,
  `/app/v3/api/iam`, `/app/v3/api/oauth`, `/app/v3/api/system/iam`, or `/backend/v3/api/iam`
  handlers in this product.
- Split mode requires upstream dependency base URLs at startup. Embedded mode requires an executable
  embedded router for each required dependency surface when no upstream is configured.
- `sdkwork-claw-router`, `sdkwork-im`, and other applications should integrate foundation APIs through
  `sdkwork-api-gateway` instead of directly depending on those APIs themselves.
- Do not hand-edit generated SDK output. Fix the source contract, generator input, approved facade,
  or Cargo workspace dependency and then regenerate through the approved toolchain.

## Human Review Rules

Human review is required for breaking standards changes, public API or SDK contract changes,
security exceptions, migration compatibility windows, release/publication policy changes,
destructive filesystem operations, or changes that would alter generated SDK ownership.
