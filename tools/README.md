# Tools

Purpose: reusable developer, validation, generation, migration, and operator tools.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: reusable validators, generators, migration tools, parsers, CLIs, and operator utilities.

Forbidden content: thin package-script wrappers that belong in `scripts/`, runtime source code, generated SDK output, and secrets.

Related specs: `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`, `../sdkwork-specs/TYPESCRIPT_CODE_SPEC.md`, and `../sdkwork-specs/TEST_SPEC.md`.

Verification: tool changes must define their own command or be covered by `cargo test --workspace` / package scripts.
