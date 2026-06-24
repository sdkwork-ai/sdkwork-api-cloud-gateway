# Tests

Purpose: cross-package, contract, integration, end-to-end, fixture, and static verification content.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: repository-level test fixtures, static checks, contract tests, and integration/e2e test harnesses.

Forbidden content: package-local unit tests that belong beside the crate they verify, live secrets, customer data, runtime logs, and databases.

Related specs: `../sdkwork-specs/TEST_SPEC.md`, `../sdkwork-specs/QUALITY_GATE_SPEC.md`, and `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.

Verification: current Rust verification is package-local and runs with `cargo test --workspace`.
