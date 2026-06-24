# Deployments

Purpose: deployment topology, infrastructure descriptors, release handoff files, and deployment runbooks.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: Docker, Kubernetes, systemd, nginx, environment topology, release handoff, and deployment runbook examples.

Forbidden content: live secrets, private keys, local override files, runtime user config, and mutable runtime state.

Related specs: `../sdkwork-specs/DEPLOYMENT_SPEC.md`, `../sdkwork-specs/GITHUB_WORKFLOW_SPEC.md`, and `../sdkwork-specs/RUNTIME_DIRECTORY_SPEC.md`.

Verification: deployment artifacts must be validated by `sdkwork.workflow.json` lifecycle steps or documented smoke checks before release.
