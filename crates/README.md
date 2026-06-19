# Crates

Purpose: authored Rust crates for the SDKWork API Gateway application root.

Owner: `sdkwork-api-gateway` maintainers.

Allowed content: gateway library crates, API server crates, config, registry, observability, focused
Rust modules, package-local tests, and crate-local component metadata when required.

Forbidden content: generated SDK transport output, live secrets, runtime config, deployment-only
state, and dependency-owned source copied from sibling repositories.

Related specs: `../sdkwork-specs/RUST_CODE_SPEC.md`, `../sdkwork-specs/NAMING_SPEC.md`, and
`../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.

Verification: run `cargo fmt -- --check` and `cargo test --workspace` from the repository root.
