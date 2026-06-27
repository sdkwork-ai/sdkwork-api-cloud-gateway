# API Gateway Workspace Layout Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align `sdkwork-api-cloud-gateway` project directories, Cargo crate names, config template placement, and documentation with `sdkwork-specs`.

**Architecture:** Keep the gateway behavior unchanged while moving source roots to the SDKWork standard dictionary. Replace the forbidden Rust `runtime` crate suffix with the gateway crate family, move the runnable server out of top-level `services/`, and migrate source-controlled config templates from `config/` to `configs/`. Preserve existing uncommitted foundation-surface changes.

**Tech Stack:** Rust Cargo workspace, Axum service, Node package scripts, SDKWork workflow config, PowerShell verification.

---

### Task 1: Standard Root Directory Dictionary

**Files:**
- Create: `apis/README.md`
- Create: `apps/README.md`
- Create: `jobs/README.md`
- Create: `tools/README.md`
- Create: `plugins/README.md`
- Create: `examples/README.md`
- Create: `configs/README.md`
- Create: `deployments/README.md`
- Create: `tests/README.md`
- Modify: `README.md`

- [x] Create tracked README placeholders for inactive or newly standardized top-level directories.
- [x] Document purpose, owner, allowed content, forbidden content, related specs, and verification checklist in each README.
- [x] Add a root layout section to `README.md` explaining active directories and intentionally narrow placeholders.

### Task 2: Move Config Templates To `configs/`

**Files:**
- Move: `config/sdkwork-api-cloud-gateway.development.toml.example` to `configs/sdkwork-api-cloud-gateway.development.toml.example`
- Move: `config/sdkwork-api-cloud-gateway.production.toml.example` to `configs/sdkwork-api-cloud-gateway.production.toml.example`
- Move: `config/sdkwork-api-cloud-gateway.test.toml.example` to `configs/sdkwork-api-cloud-gateway.test.toml.example`
- Delete: `config/` when empty
- Modify: `.gitignore`
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `package.json`
- Modify: `scripts/package-server.mjs`
- Modify: `crates/sdkwork-api-cloud-gateway/tests/service_config_tests.rs` before server move, then final path under `crates/`
- Modify: `crates/sdkwork-api-cloud-gateway-config/tests/config_tests.rs`

- [x] Move source-controlled config templates to `configs/`.
- [x] Update all source/test/docs references from `config/` to `configs/`.
- [x] Keep packaged archive internal runtime directory as `config/` for operator ergonomics.
- [x] Update ignored local override pattern from `config/*.local.toml` to `configs/*.local.toml`.

### Task 3: Rename Runtime Crate And Server Crate Layout

**Files:**
- Move: `crates/sdkwork-api-cloud-gateway/` to `crates/sdkwork-api-cloud-gateway/`
- Move: `crates/sdkwork-api-cloud-gateway/` to `crates/sdkwork-api-cloud-gateway/`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/sdkwork-api-cloud-gateway/Cargo.toml`
- Modify: `crates/sdkwork-api-cloud-gateway/Cargo.toml`
- Modify: `crates/sdkwork-api-cloud-gateway/src/main.rs`
- Modify: `crates/sdkwork-api-cloud-gateway/tests/service_config_tests.rs`
- Modify: `package.json`
- Modify: `scripts/package-server.mjs`
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `specs/component.spec.json`
- Modify: `crates/sdkwork-api-cloud-gateway-config/tests/config_tests.rs`

- [x] Rename library package from `sdkwork-api-cloud-gateway` to `sdkwork-api-cloud-gateway`.
- [x] Rename Rust import paths from `sdkwork_api_gateway` to `sdkwork_api_cloud_gateway`.
- [x] Rename runnable server package from `sdkwork-api-cloud-gateway` to `sdkwork-api-cloud-gateway`.
- [x] Move runnable server into `crates/` and remove top-level `services/`.
- [x] Update component spec entrypoints and verification commands.

### Task 4: Verification And Cleanup

**Files:**
- No new implementation files expected.

- [x] Run `cargo metadata --no-deps --format-version 1`.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `cargo test --workspace`.
- [x] Run `pnpm workflow:validate`.
- [x] Run `pnpm workflow:matrix:server`.
- [x] Run `pnpm package:server`.
- [x] Run `pnpm package:server:validate`.
- [x] Smoke `pnpm dev` against `/healthz` on a temporary port.
- [x] Run `git diff --check`.
- [x] Confirm `git status --short` separates pre-existing user changes from this alignment work.
