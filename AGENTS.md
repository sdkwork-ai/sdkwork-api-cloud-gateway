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

- Application id: `sdkwork-api-cloud-gateway`
- Runtime family: Rust backend service
- Primary service process: `crates/sdkwork-api-cloud-gateway`
- Public gateway crate: `crates/sdkwork-api-cloud-gateway`
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

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

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

Build scripts, dev runners, and `pnpm clean` must follow `CODE_STYLE_SPEC.md` §7 (Build Source Integrity And Self-Healing). Git-tracked build-critical source files must be verified before builds and self-healed from git when missing; `clean` must not delete them.

## Build, Test, and Verification

Run commands from this repository root:

- `cargo metadata --no-deps --format-version 1`
- `cargo fmt -- --check` (workspace members only; do not use `--all` because optional foundation path
  dependencies resolve sibling application workspaces outside this repository authority)
- `cargo test --workspace`
- `pnpm check`
- `pnpm verify`
- `pnpm workflow:validate`
- `pnpm gateway:matrix:standalone`
- `pnpm gateway:package:standalone`
- `pnpm gateway:validate:standalone`

Use narrower commands first while developing, then run the broader verification set before claiming
completion, committing, or pushing. When `pnpm dev` behavior changes, smoke-test `/healthz` with a
temporary bind address and stop the process after the check.

## Agent Execution Rules

- The gateway supports independent startup through `crates/sdkwork-api-cloud-gateway` and
  embedding through public router builders from `sdkwork-api-cloud-gateway`.
- Gateway integration authority is Cargo-workspace-first. Do not create a separate gateway catalog;
  use root `Cargo.toml`, crate Cargo features, `sdkwork.app.config.json`, and
  `specs/component.spec.json`.
- Appbase IAM routes are dependency routes. Do not reimplement `/app/v3/api/auth`,
  `/app/v3/api/iam`, `/app/v3/api/oauth`, `/app/v3/api/system/iam`, or `/backend/v3/api/iam`
  handlers in this product.
- Foundation dependency surfaces are embedded-only. Each published surface requires a Cargo
  feature, an assembly dependency, and a verified executable router export; legacy split runtime
  values and per-dependency base URL overrides are rejected.
- `sdkwork-clawrouter`, `sdkwork-im`, and other applications should integrate foundation APIs through
  `sdkwork-api-cloud-gateway` instead of directly depending on those APIs themselves.
- Do not hand-edit generated SDK output. Fix the source contract, generator input, approved facade,
  or Cargo workspace dependency and then regenerate through the approved toolchain.

## App SDK Consumer Imports



Application, feature, shell, and service packages `MUST` consume HTTP SDKs through scoped composed consumer packages, not generator transport package names.



- App API clients: `@sdkwork/<application-code>-app-sdk`

- Backend API clients (`backend-admin` only): `@sdkwork/<application-code>-backend-sdk`

- Federated Claw Router domain surfaces: `@sdkwork/clawrouter-app-sdk/domains` and `@sdkwork/clawrouter-backend-sdk/domains`

- Open/domain API clients: `@sdkwork/<domain>-sdk`



Canonical examples (IAM):



```typescript

import { createClient, type SdkworkAppClient } from '@sdkwork/iam-app-sdk';

import type { SdkworkBackendClient } from '@sdkwork/iam-backend-sdk'; // backend-admin only

import { createClient as createClawRouterDomainsClient } from '@sdkwork/clawrouter-app-sdk/domains';

```



Forbidden in application `apps/`, `packages/`, bootstrap, services, UI, contract tests, and composed SDK `src/**` outside generator ownership:



- `sdkwork-*-app-sdk-generated-typescript`, `sdkwork-*-backend-sdk-generated-typescript`, and other generator transport names as consumer imports

- `@sdkwork/commerce-app-sdk`, `@sdkwork/commerce-backend-sdk`, `@sdkwork/clawrouter-*-domain-transport-sdk`

- filesystem paths containing `domain-transport-typescript`, `domain-transport-sdk`, or sibling `*-typescript/generated` hops from composed `src/**`

- deep imports into `generated/server-openapi/src/*` from consumers when a composed facade exists



Allowed:



- Composed facade entry imports such as `@sdkwork/iam-app-sdk`, `@sdkwork/knowledgebase-app-sdk`, and `@sdkwork/clawrouter-app-sdk/domains`

- Composed re-exports that import only from `../generated/**` within the same `*-sdk-typescript` family root

- Generated transport ownership inside `sdks/**/generated/**` only



Each SDK family `MUST` expose the composed TypeScript facade at `sdks/<sdk-family>/<sdk-family>-typescript/src/index.ts` (and optional subpath exports such as `./domains`) with `package.json#name` equal to the scoped consumer package.



Before completing SDK integration or frontend service work, run:



```bash

node <sdkwork-specs>/tools/check-app-sdk-consumer-imports.mjs --workspace <workspace-root>

```



Authority: `APP_SDK_INTEGRATION_SPEC.md` section 9, `SDK_SPEC.md` package naming table, `SDK_WORKSPACE_GENERATION_SPEC.md` composed facade rules.

## HTTP API Response Envelope

All L2+ SDKWork-owned custom HTTP contracts, including `app-api`, `backend-api`, and SDKWork-owned business `open-api`, `MUST` follow `API_SPEC.md` section 4.5, section 14, and section 15:

- **Default classification:** omitted `x-sdkwork-wire-protocol` means SDKWork-owned custom API (`sdkwork-v3`); only operation-level `x-sdkwork-wire-protocol: external` plus `x-sdkwork-external-protocol-id` identifies a third-party compatibility `open-api` operation.
- **Input:** typed request bodies, section 14.1 list/search/command input, `SdkWorkListQuery`, and `q` for free-text search.
- **Success output:** `SdkWorkApiResponse` with `{ "code": 0, "data": <payload>, "traceId": "<server-uuid>" }`.
- **Error output:** HTTP 4xx/5xx `application/problem+json` (`ProblemDetail`) with numeric `code` and `traceId`.
- Success `code` is numeric `int32`; HTTP 2xx JSON bodies `MUST` use `0` only. REST semantics remain on HTTP status (`201`, `202`, etc.).
- Platform error codes are numeric non-zero values per section 15.3 (`40001`, `40101`, `40401`, …).
- Single resource: `data.item`
- Lists: `data.items` + `data.pageInfo` (`PageInfo.mode` is `offset` or `cursor`)
- Commands: `data.accepted` plus optional `resourceId` / `status`
- Async accept (`202`): `data.operationId`, `data.status`, optional `pollUrl`
- Operation patterns: retrieve/list/search/create/update/delete/command/async/bulk semantics follow `API_SPEC.md` section 15.4; create uses `201`, delete uses `204` with no JSON body, and `PUT`/`PATCH` use SDK action `update`.

Vendor compatibility `open-api` routes that mirror upstream tool or provider wire (for example OpenAI `/v1/*`, Anthropic/Claude `/anthropic/v1/*`, Google/Gemini `/google/v1beta/*`, Claude Code, or Codex) `MAY` opt out only when every exempt operation declares operation-level `x-sdkwork-wire-protocol: external` and `x-sdkwork-external-protocol-id` per `API_SPEC.md` section 4.5.2. SDKWork-owned business `open-api` operations `MUST NOT` opt out. Mixed OpenAPI documents are validated per operation; one external operation never exempts SDKWork-owned operations in the same document.

Errors `MUST` use HTTP 4xx/5xx with `application/problem+json` (`ProblemDetail`) including required numeric `code` and `traceId`. Business failures `MUST NOT` use HTTP 2xx with non-zero `code`, string wire codes, `success`, or human `message`.

Forbidden legacy envelopes and fields: `PlusApiResult`, `AppbaseApiResult`, `StoreApiResult`, `SdkWorkResponse`, per-domain `*ApiResult`, wire field `requestId`, bare domain DTOs at the HTTP root, and top-level `{ items, pageInfo, traceId }` without `data`.

Handlers `MUST` serialize success and map errors through `sdkwork-web-framework` response mapping. Generated HTTP SDKs (`--standard-profile sdkwork-v3`) unwrap `data` by default and expose typed numeric `ProblemDetail.code` / `traceId` on errors; use `.raw` when the full envelope is required.

Before completing API contract, SDK generation, or frontend service work, run:

```bash
node <sdkwork-specs>/tools/check-api-operation-patterns.mjs --workspace <workspace-root>
node <sdkwork-specs>/tools/check-api-response-envelope.mjs --workspace <workspace-root>
```

Authority: `sdkwork-specs/API_SPEC.md` section 4.5 and sections 14–16, `SDK_SPEC.md` section 4.2, `FRONTEND_SPEC.md`, `MIGRATION_SPEC.md` section 4.2.

## List And Search Pagination

All L2+ list/search APIs and their backing services, repositories, SDK consumers, and interactive frontend lists `MUST` follow `PAGINATION_SPEC.md`:

- **Input:** standard `SdkWorkListQuery` or query params (`page`/`page_size` or `cursor`/`page_size` per `API_SPEC.md` §14.1); default `page_size` `20`; max `200` unless a documented exception exists.
- **Output:** `SdkWorkApiResponse.data.items` + `data.pageInfo` with `PageInfo.mode` (`offset` or `cursor`) per `API_SPEC.md` §16.
- **Store-level pagination:** push filtering, sorting, and page selection to SQL `LIMIT`/keyset or incrementally maintained indexes — never unbounded collect then `skip`/`take`/`slice` in process memory (`PAGINATION_SPEC.md` §2).
- **SDK and frontend:** interactive lists request one page at a time from the server; no default `listAll*` on P0/P1 paths; no client-side `slice` pagination over full downloads.

Before completing list/search API, repository, SDK list helper, projection read model, or paginated UI work, run:

```bash
node <sdkwork-specs>/tools/check-pagination.mjs --workspace <workspace-root>
```

Authority: `PAGINATION_SPEC.md`, `API_SPEC.md` §14.1/§16, `DATABASE_SPEC.md` §20.5, `WEB_BACKEND_SPEC.md` §12, `SDK_SPEC.md` §4.2/§6, `FRONTEND_SPEC.md`, `APP_SDK_INTEGRATION_SPEC.md` §9.

## Human Review Rules

Human review is required for breaking standards changes, public API or SDK contract changes,
security exceptions, migration compatibility windows, release/publication policy changes,
destructive filesystem operations, or changes that would alter generated SDK ownership.
