# Commercial API Gateway Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Phase 1 commercial API gateway foundations: policy chain, observability, baseline
security, in-memory traffic control, docs, and verification.

**Architecture:** Keep public router builders stable. Add focused modules inside
`sdkwork-api-cloud-gateway` and extend `sdkwork-api-cloud-gateway-config` plus
`sdkwork-api-cloud-gateway-observability`. Use typed config, in-memory runtime state, and Axum middleware
style helpers. Start with memory-only stores; leave Redis for Phase 2.

**Tech Stack:** Rust 2021, Axum 0.8, Tower, Reqwest, Serde, Tokio, tracing.

---

## Files

- Modify: `crates/sdkwork-api-cloud-gateway-config/src/lib.rs`
- Modify: `crates/sdkwork-api-cloud-gateway-config/tests/config_tests.rs`
- Modify: `crates/sdkwork-api-cloud-gateway-observability/src/lib.rs`
- Modify: `crates/sdkwork-api-cloud-gateway/src/lib.rs`
- Modify: `crates/sdkwork-api-cloud-gateway/src/runtime.rs`
- Create: `crates/sdkwork-api-cloud-gateway/src/policy.rs`
- Create: `crates/sdkwork-api-cloud-gateway/src/security.rs`
- Create: `crates/sdkwork-api-cloud-gateway/src/traffic.rs`
- Create: `crates/sdkwork-api-cloud-gateway/src/metrics.rs`
- Modify: `crates/sdkwork-api-cloud-gateway/tests/runtime_tests.rs`
- Modify: `config/*.toml.example`
- Modify: `sdkwork.app.config.json`
- Modify: `specs/component.spec.json`
- Modify: `README.md`

## Task 1: Typed Commercial Config

- [ ] Write failing config tests for `observability`, `security`, and `traffic` sections.
- [ ] Run `cargo test -p sdkwork-api-cloud-gateway-config --test config_tests` and confirm missing fields fail.
- [ ] Add typed config structs with safe defaults.
- [ ] Add env overrides for selected keys.
- [ ] Update TOML examples.
- [ ] Run config tests and template parse tests.

## Task 2: Metrics Endpoint And Counters

- [ ] Write failing runtime test for `/metrics` disabled by default in embedded custom config and enabled by config.
- [ ] Write failing runtime test proving proxied requests increment request and upstream metrics.
- [ ] Add runtime metrics state and text exposition.
- [ ] Add low-cardinality metric names.
- [ ] Run targeted runtime tests.

## Task 3: Security Headers And Guards

- [ ] Write failing tests for security headers.
- [ ] Write failing tests for host allowlist.
- [ ] Write failing tests for header count/byte limits.
- [ ] Implement `security` helpers and integrate before route delegation.
- [ ] Return problem JSON for guard failures.
- [ ] Run targeted runtime tests.

## Task 4: In-Memory Rate Limit And Concurrency Limit

- [ ] Write failing tests for route prefix token-bucket rate limiting.
- [ ] Write failing tests for `Retry-After`, `RateLimit`, and `RateLimit-Policy`.
- [ ] Write failing tests for global concurrency rejection.
- [ ] Implement `traffic` module with in-memory state.
- [ ] Integrate traffic checks before upstream/embedded dispatch.
- [ ] Run targeted runtime tests.

## Task 5: Structured Access Logs And Trace Propagation

- [ ] Write tests for `traceparent` propagation to upstream.
- [ ] Add access log event helper with redaction list.
- [ ] Add trace id in problem responses when available.
- [ ] Run targeted runtime tests.

## Task 6: Contract And Documentation Sync

- [ ] Update manifest env bindings.
- [ ] Update component runtime behavior contract.
- [ ] Update README.
- [ ] Validate JSON files.

## Task 7: Full Verification

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo build --workspace`.
- [ ] Run binary smoke for `/healthz`, `/readyz`, and `/metrics` when enabled.
