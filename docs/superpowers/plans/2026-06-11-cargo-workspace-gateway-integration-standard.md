# Cargo Workspace Gateway Integration Standard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make SDKWork API Gateway integration standards rely on Cargo workspace/features and existing SDKWork specs, without adding a separate gateway catalog file.

**Architecture:** `sdkwork-api-cloud-gateway` keeps runtime dependency execution in Cargo manifests and router exports. `sdkwork-specs` defines how Cargo evidence, `sdkwork.app.config.json`, `component.spec.json`, and SDK assembly metadata align. Business applications consume `sdkwork-api-cloud-gateway` instead of each independently composing foundation API runtimes.

**Tech Stack:** Rust Cargo workspace, Axum gateway runtime, JSON component specs, Markdown SDKWork standards.

---

### Task 1: Establish Cargo Feature Evidence In Gateway

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/sdkwork-api-cloud-gateway/Cargo.toml`
- Modify: `crates/sdkwork-api-cloud-gateway/src/lib.rs`
- Modify: `crates/sdkwork-api-cloud-gateway/src/runtime.rs`

- [ ] **Step 1: Run a red static check**

Run: a `cargo metadata`/PowerShell assertion that expects runtime feature `foundation-appbase` to exist.
Expected: FAIL before editing because the feature is not declared.

- [ ] **Step 2: Add the feature**

Declare `foundation-appbase` in `sdkwork-api-cloud-gateway`, make `sdkwork_iam_http` optional there, and keep it in default features.

- [ ] **Step 3: Gate appbase embedded exports**

Wrap appbase-specific imports, exports, and helper functions with `#[cfg(feature = "foundation-appbase")]`. Keep split-mode gateway behavior available without the feature.

- [ ] **Step 4: Verify Cargo feature evidence**

Run the same static check.
Expected: PASS and `cargo metadata` exposes `foundation-appbase`.

### Task 2: Record The Standard In Existing Gateway Files

**Files:**
- Modify: `README.md`
- Modify: `specs/component.spec.json`
- Modify: `sdkwork.app.config.json`

- [ ] **Step 1: Document no-catalog policy**

Add gateway-local wording that foundation API integration is driven by Cargo workspace dependencies/features plus existing SDKWork specs.

- [ ] **Step 2: Add existing-spec metadata only**

Add fields to the existing manifest/component spec for the current appbase surface, such as `cargoFeature` and `cargoDependency`, without creating a new catalog file.

- [ ] **Step 3: Validate config parsing**

Run relevant config tests to prove unknown or added metadata does not break runtime config loading.

### Task 3: Update SDKWork Global Standards

**Files:**
- Modify: `../sdkwork-specs/DEPENDENCY_MANAGEMENT_SPEC.md`
- Modify: `../sdkwork-specs/APP_SDK_INTEGRATION_SPEC.md`
- Modify: `../sdkwork-specs/SDK_SPEC.md`
- Modify: `../sdkwork-specs/COMPONENT_SPEC.md`
- Modify: `../sdkwork-specs/CONFIG_SPEC.md`

- [ ] **Step 1: Define Cargo-first gateway integration**

Specify that Rust gateway foundations use Cargo workspace dependencies and feature names as executable integration evidence.

- [ ] **Step 2: Preserve spec/runtime separation**

Clarify that Cargo proves executable linkage, while `dependencyApiSurfaces`, manifest entries, and SDK assemblies prove SDKWork API semantics.

- [ ] **Step 3: Forbid separate gateway catalogs**

State that a standalone gateway catalog must not become a parallel source of truth when Cargo and existing SDKWork specs already carry the facts.

- [ ] **Step 4: Define app migration boundary**

State that product applications must not directly compose foundation API runtime crates when the shared gateway provides the surface; product APIs remain independent SDKWork surfaces.

### Task 4: Verify And Report

**Files:**
- Read: modified files and VCS diffs

- [ ] **Step 1: Run formatting**

Run: `cargo fmt --all -- --check`
Expected: PASS.

- [ ] **Step 2: Run workspace tests**

Run: `cargo test --workspace`
Expected: PASS, or report exact failures.

- [ ] **Step 3: Verify docs/spec changes**

Run targeted `rg` checks for `gateway catalog`, `foundation-appbase`, `cargoFeature`, and `Cargo workspace`.
Expected: relevant standards and gateway files contain the new policy.

- [ ] **Step 4: Summarize result**

Report modified files, verification evidence, and any residual migration work for `sdkwork-cloudrouter` and `sdkwork-im`.
