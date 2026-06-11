# SDKWork API Gateway Specs

Purpose: This directory is the local specification entrypoint for the SDKWork API Gateway
application root. It documents executable component boundaries for this application and points to
the canonical SDKWork standards without copying them.

Owner: SDKWork API Gateway maintainers.

Allowed content: Local component contracts, API gateway integration notes, and component-specific
constraints that narrow or document this repository's behavior. `specs/component.spec.json` is the
machine-readable component contract for the gateway's executable modules, public exports,
dependency API surfaces, route precedence, and verification commands.

Forbidden content:

- Do not copy root SDKWork standards into this directory.
- Do not add a separate gateway catalog.
- Do not place generated SDK transport output, live runtime config, secrets, local overrides, or
  dependency implementation code here.
- Gateway dependency facts must continue to come from Cargo workspace metadata,
  `sdkwork.app.config.json`, and `specs/component.spec.json`.

Related specs:

- `../sdkwork-specs/COMPONENT_SPEC.md`
- `../sdkwork-specs/DOCUMENTATION_SPEC.md`
- `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`
- `../sdkwork-specs/TEST_SPEC.md`

Verification:

- `cargo test -p sdkwork-api-gateway-config local_specs_readme_documents_component_contract_boundary`
- `cargo test -p sdkwork-api-gateway-config component_spec_uses_cargo_metadata_as_gateway_integration_authority`
- `cargo test --workspace`
