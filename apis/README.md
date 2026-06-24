# APIs

Purpose: authored API contract inputs for SDKWork API Gateway owned surfaces.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: OpenAPI/RPC/event contracts, API examples, changelogs, route materialization inputs, and validation fixtures owned by this gateway.

Forbidden content: generated SDK transport packages, runtime source code, live secrets, dependency-owned copied API contracts, and generated `.sdkwork/` control-plane output.

Related specs: `../sdkwork-specs/API_SPEC.md`, `../sdkwork-specs/SDK_WORKSPACE_GENERATION_SPEC.md`, and `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.

Verification: gateway-owned API sources must be reflected in `specs/component.spec.json` and covered by `cargo test --workspace`.
