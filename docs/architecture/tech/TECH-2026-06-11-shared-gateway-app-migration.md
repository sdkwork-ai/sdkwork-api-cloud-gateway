> Migrated from `docs/superpowers/plans/2026-06-11-shared-gateway-app-migration.md` on 2026-06-24.
> Owner: SDKWork maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the shared SDKWork API Gateway integration standard in `sdkwork-cloudrouter` and `sdkwork-im` without introducing a parallel gateway catalog.

**Architecture:** Foundation API integration evidence stays in native build tools and existing SDKWork specs: Cargo workspace dependencies/features, `agent.app.config.json`, `specs/component.spec.json`, local dependency surface specs, SDK assembly metadata, and runtime config. Product apps document the gateway-backed target state and enforce static guardrails while legacy product-local foundation mounts remain only as migration compatibility.

**Tech Stack:** Rust Cargo workspaces, SDKWork application manifests, component specs, Node.js governance tests, JSON/TOML config examples.

---

### Task 1: SDKWork Cloud Router Gateway Standard

**Files:**
- Modify: `<workspace-root>/sdkwork-cloudrouter/specs/component.spec.json`
- Modify: `<workspace-root>/sdkwork-cloudrouter/specs/dependency-api-surfaces.json`
- Modify: `<workspace-root>/sdkwork-cloudrouter/specs/README.md`
- Modify: `<workspace-root>/sdkwork-cloudrouter/scripts/run-cloud-router-product.test.mjs`
- Optional existing config templates only if already present: `.env.release.example`, `apps/sdkwork-cloudrouter-pc/.env.example`

- [ ] Add failing governance assertions for shared gateway integration metadata and no standalone gateway catalog.
- [ ] Mark foundation dependency surfaces with gateway-backed target metadata and legacy compatibility status.
- [ ] Add the common gateway SDK root config key to existing environment examples when those files already carry SDK base URL settings.
- [ ] Run the focused Node governance test.

### Task 2: Craw Chat Gateway Standard

**Files:**
- Modify: `<workspace-root>/sdkwork-im/specs/component.spec.json`
- Modify: `<workspace-root>/sdkwork-im/specs/im-app-api-sdk-integration.spec.md`
- Modify: `<workspace-root>/sdkwork-im/scripts/dependency-management-standard.test.mjs`
- Optional existing config templates only if already present: app PC env examples or runtime script docs

- [ ] Add failing dependency-management assertions for shared gateway target metadata and no standalone gateway catalog.
- [ ] Mark Craw Chat local web gateway as migration compatibility for foundation aggregation, while keeping IM product APIs product-owned.
- [ ] Add the common gateway SDK root config key to existing runtime config docs/templates when the local files already define SDK roots.
- [ ] Run the focused Node dependency-management test and runtime-standard test if impacted.

### Task 3: Verification And Report

- [ ] Validate changed JSON files with native JSON parsing.
- [ ] Run focused Node tests in both application roots.
- [ ] Inspect diffs to ensure only intended files changed.
- [ ] Report verification evidence and remaining migration risks.

