# Scripts

Purpose: thin command entrypoints for SDKWork API Gateway build, workflow, package, and validation
tasks.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: small Node, PowerShell, shell, or other command wrappers that call canonical build,
test, workflow, package, or release commands.

Forbidden content: reusable validators, generators, migration engines, long-lived operator CLIs,
runtime business logic, generated SDK output, live secrets, and local machine state.

Related specs: `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`,
`../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`, and `../sdkwork-specs/RELEASE_SPEC.md`.

Verification: run `pnpm workflow:validate`, `pnpm workflow:matrix:server`, and
`pnpm gateway:validate:standalone` and `pnpm gateway:validate:cloud` after changing packaging or workflow scripts.
