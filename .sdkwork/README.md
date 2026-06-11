# .sdkwork

Purpose: source-controlled SDKWork workspace metadata for this application root.

Owner: `sdkwork-api-gateway` maintainers.

Allowed content: application-local agent skills, application-local agent plugin bundles,
non-secret workspace metadata, and optional manifests that support repository automation without
becoming API, SDK, dependency, or runtime sources of truth.

Authoritative directories: `.sdkwork/skills/` for application-local agent workflows and
`.sdkwork/plugins/` for application-local agent plugin bundles. Optional manifests may be added
under `.sdkwork/manifests/` only when tooling needs them.

Local-only directories such as `.sdkwork/local/`, `.sdkwork/tmp/`, `.sdkwork/cache/`,
`.sdkwork/secrets/`, and `.sdkwork/state/` are ignored and must not be committed. This directory is
not runtime state; runtime config, caches, logs, temp files, and secrets stay in user-private or
process-private paths governed by the root standards.

Forbidden content: runtime state, generated SDK output, SDK generator control-plane output, access
tokens, private certificates, live secrets, local credentials, and user-private files.

Related specs: `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`, `../sdkwork-specs/AGENTS_SPEC.md`,
and `../sdkwork-specs/SOUL.md`.

Verification: follow `AGENTS.md` for execution order and run
`cargo test -p sdkwork-api-gateway-config sdkwork_workspace_readmes_follow_repository_workspace_standard`
when changing `.sdkwork/` documentation or workspace metadata.
