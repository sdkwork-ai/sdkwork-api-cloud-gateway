# Plugins

Purpose: application/runtime plugin source packages.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: installable runtime extension source packages and plugin component specs.

Forbidden content: repository-local agent plugins, which belong under `.sdkwork/plugins/`, generated SDK output, runtime state, and credentials.

Related specs: `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`, `../sdkwork-specs/COMPONENT_SPEC.md`, and `../sdkwork-specs/MODULE_SPEC.md`.

Verification: no runtime plugin capability is active yet; plugin packages must add component specs and tests when introduced.
