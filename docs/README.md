# Docs

Purpose: repository documentation, architecture notes, implementation plans, runbooks, changelogs,
and engineering decision records for SDKWork API Gateway.

Owner: `sdkwork-api-gateway` maintainers.

Allowed content: design notes, implementation plans, runbooks, release notes, architecture decision
records, and non-secret operational documentation.

Forbidden content: generated SDK output, runtime logs, private customer data, live secrets, local
machine state, and source config templates that belong in `configs/`.

Related specs: `../sdkwork-specs/DOCUMENTATION_SPEC.md`,
`../sdkwork-specs/ARCHITECTURE_DECISION_SPEC.md`, and `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.

Verification: documentation that changes runtime, API, SDK, packaging, or deployment expectations
must be backed by the relevant Cargo, pnpm, workflow, or package verification commands.
