# SDKs

Purpose: SDK family workspaces, SDK generation manifests, authority materialization outputs, and
generated SDK artifacts owned by SDKWork API Gateway.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: gateway-owned SDK family workspaces, generated SDK language packages, SDK assembly
metadata, SDK component specs, and generation inputs derived from gateway-owned API authorities.

Forbidden content: authored API contracts that belong in `apis/`, dependency-owned SDKs copied from
sibling repositories, runtime state, live secrets, and hand-edited generated transport output.

Related specs: `../sdkwork-specs/SDK_SPEC.md`,
`../sdkwork-specs/SDK_WORKSPACE_GENERATION_SPEC.md`, and
`../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.

Verification: generated SDK changes must be reproduced from their source contracts and verified by
the relevant SDK generation command plus `cargo test --workspace` when Rust gateway contracts depend
on them.
