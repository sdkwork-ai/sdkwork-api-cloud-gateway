# Superpowers Work Records

Purpose: historical execution records for agent-led design and implementation work on SDKWork API
Gateway.

Owner: `sdkwork-api-cloud-gateway` maintainers.

Allowed content: dated design notes, implementation plans, review notes, and verification summaries
created during agent execution.

Forbidden content: current SDKWork standards, live runtime configuration, generated SDK output,
secrets, local machine state, and replacement copies of root specs.

Related specs: `../../../sdkwork-specs/DOCUMENTATION_SPEC.md`,
`../../../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`, and `../../../sdkwork-specs/README.md`.

Verification: run `cargo test -p sdkwork-api-cloud-gateway-config` when changing these work records or
their guardrails.

These files are historical execution records and are not the current source of truth for gateway
architecture, public exports, runtime config, or dependency integration. Current readers should use
the repository [README](../../README.md), [AGENTS](../../AGENTS.md),
[component spec](../../specs/component.spec.json), and root SDKWork standards at
`../../../sdkwork-specs/README.md` before using any dated work record.
