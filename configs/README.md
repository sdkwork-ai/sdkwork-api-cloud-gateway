# Configs

Purpose: source-controlled safe configuration templates, profile examples, schemas, and non-secret defaults.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: `*.toml.example`, config schemas, profile examples, and non-secret runtime defaults.

Forbidden content: live secrets, local override files, runtime user config, databases, logs, and cache files.

Related specs: `../sdkwork-specs/CONFIG_SPEC.md`, `../sdkwork-specs/ENVIRONMENT_SPEC.md`, and `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.

Verification: run `cargo test -p sdkwork-api-cloud-gateway-config` to parse tracked config templates.
