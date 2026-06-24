# Local Skills

Purpose: application-local agent or operator workflows for SDKWork API Gateway.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: lowercase kebab-case skill directories containing `SKILL.md`, deterministic helper
scripts, references, and assets needed by the skill.

Forbidden content: runtime source code, generated SDK output, live secrets, local credentials,
runtime data, and standards copied from `../sdkwork-specs/`.

Related specs: `../../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md` and
`../../sdkwork-specs/AGENTS_SPEC.md`.

Verification: every real skill directory must contain `SKILL.md` and cite the relevant root specs.
