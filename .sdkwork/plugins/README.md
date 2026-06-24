# Local Plugins

Purpose: application-local Codex or SDKWork agent plugin bundles for SDKWork API Gateway.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: lowercase kebab-case plugin directories with `.codex-plugin/plugin.json`, plugin
skills, MCP server definitions, app definitions, and deterministic helper scripts when required.

Forbidden content: application/runtime plugin source packages that belong in top-level `plugins/`,
generated SDK output, vendored unrelated toolchains, live secrets, runtime data, and local machine
state.

Related specs: `../../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md` and
`../../sdkwork-specs/AGENTS_SPEC.md`.

Verification: installable plugin directories must contain `.codex-plugin/plugin.json`; plugin skills
must follow the same rules as `.sdkwork/skills/`.
