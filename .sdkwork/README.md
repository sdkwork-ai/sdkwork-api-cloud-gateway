# .sdkwork

Purpose: source-controlled SDKWork workspace metadata for this application root.

Owner: `sdkwork-api-gateway` maintainers.

Authoritative directories: `.sdkwork/skills/` for application-local agent workflows and
`.sdkwork/plugins/` for application-local agent plugin bundles. Optional manifests may be added
under `.sdkwork/manifests/` only when tooling needs them.

Local-only directories such as `.sdkwork/local/`, `.sdkwork/tmp/`, `.sdkwork/cache/`,
`.sdkwork/secrets/`, and `.sdkwork/state/` are ignored and must not be committed.

Forbidden content: runtime state, generated SDK output, SDK generator control-plane output, access
tokens, private certificates, live secrets, local credentials, and user-private files.

Related specs: `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md` and `../sdkwork-specs/SOUL.md`.
