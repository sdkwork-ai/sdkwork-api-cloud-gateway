# 2026-11-08 — Cloud Router 50301 incident and the database-backed HTTP service registry

## 1. Incident

Production `https://api.<domain>/app/v3/api/ai/agents/{agentId}/sessions/{sessionId}/turns`
returned:

```json
{
  "type": "https://docs.sdkwork.com/problems/50301",
  "status": 503,
  "detail": "cloud router turn failed: provider_error: cloud router chat completion failed: http error: error sending request for url (http://127.0.0.1:3900/v1/chat/completions)",
  "code": 50301
}
```

Host: `ubuntu@api.noaper.com`; container
`sdkwork-api-cloud-gateway-production-i1-gateway-1`, image `0.1.36`.

## 2. Root cause

The gateway is an all-in-one composition root: it mounts `sdkwork-agents` and
`sdkwork-cloudrouter` in-process and installs the in-process Cloud Router turn
surface at startup (confirmed in the container log).

The container declared only the **application-scoped** profile:

```text
SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=standalone
```

It did **not** declare the canonical profile that every embedded Rust SDK
consumer reads (`APP_SDK_INTEGRATION_SPEC.md` §5.2):

```text
SDKWORK_DEPLOYMENT_PROFILE  # absent
```

`sdkwork-agents-tool-cloudrouter` therefore classified the process as `split`
(`mode_of_deployment_profile(None) == Split`), skipped the in-process surface,
and fell back to the split default `http://127.0.0.1:3900`. The listener was
`0.0.0.0:8080` (webserver attach contract), so nothing listened on 3900:

```text
3900:000   (connection refused)
8080:200   (/healthz)
```

The error was not a port-forwarding or domain problem: the process was dialling
a loopback port it did not own.

## 3. Production hotfix (applied)

Backups:

- `/opt/deploy/sdkwork-api-cloud-gateway/bundle/compose/docker-compose.bundle.yml.bak-20261107T174715Z`
- `/opt/deploy/sdkwork-api-cloud-gateway/bundle/env/production.env.bak-20261107T174715Z`

Changes:

1. bundle compose gateway environment:
   `SDKWORK_DEPLOYMENT_PROFILE: ${GATEWAY_DEPLOYMENT_PROFILE:-standalone}`
2. live `env/production.env`: `GATEWAY_DEPLOYMENT_PROFILE=standalone`
3. `bash deploy.sh --environment production` from the bundle (same image,
   recreated gateway only).

Verification:

- container env now has both
  `SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE=standalone` and
  `SDKWORK_DEPLOYMENT_PROFILE=standalone`;
- `/readyz` → 200, container `healthy`, `RestartCount=0`;
- startup log: `embedded cloud router turn surface installed for in-process dispatch`;
- no `http://127.0.0.1:3900` turn failure after the restart.

## 4. Durable fix (0.1.37 source)

- `listener_main` publishes the canonical `SDKWORK_DEPLOYMENT_PROFILE` from
  the explicit value, then the application alias, then the manifest default,
  **before** the runtime thread and dependency assemblies start. Invalid
  values fail bootstrap; a conflicting explicit value wins with a warning.
- `bin/container/entrypoint` and every compose input publish both names and
  validate `standalone|cloud`.
- `CloudRouterTurnTransport::resolve` prefers a composition-root-installed
  in-process surface over the profile string, so an all-in-one container that
  declares `cloud` still never dials its own listener.
- Unit tests cover app-alias publication, default publication, explicit
  precedence, invalid rejection, and in-process-over-profile resolution.

## 5. Database-backed HTTP service registry

New capability (`DISCOVERY_SPEC.md` §12.1):

- `sdkwork-discovery-client` (direct PostgreSQL, no gRPC hop):
  - idempotent upsert on
    `(namespace, environment, service_name, instance_id)`;
  - a live instance keeps its lease id across re-registration;
  - heartbeat renews `expires_at_ms`; discovery excludes expired,
    soft-deleted, and `not_serving` rows;
  - HTTP discovery ordered by `priority ASC, weight DESC`;
  - soft deregistration and status reporting.
- `HttpRegistryResolver` implements
  `sdkwork_utils_rust::service_base_url::HttpServiceRegistry`:
  - cache-first, background snapshot refresh, no per-call DB round trip;
  - consecutive-failure demotion with a bounded cooldown;
  - success resets the failure streak;
  - last-known-good snapshot survives refresh failure;
  - an opt-in active health probe
    (`SDKWORK_DISCOVERY_HTTP_REGISTRY_HEALTH_PATH` + timeout) applies the same
    threshold/cooldown on every refresh; unset keeps failure-driven detection.
- Gateway:
  - binds the listener and starts the registry **before** building the router,
    so dependency assemblies that construct HTTP SDK clients during assembly
    (generations media providers, drive facades, image host) see the installed
    resolver. A resolver installed after router construction was invisible to
    them and they kept the configured fallback;
  - registers **every instance/port** for its own service, each composed
    dependency surface, and the short SDK alias
    (`sdkwork-cloudrouter` / `cloudrouter`, `sdkwork-agents` / `agents`, ...);
  - 30s lease, 10s heartbeat, re-register on lease loss;
  - a bounded expiry sweep every minute soft-deletes rows whose lease elapsed,
    so an embedded deployment without the discovery control plane does not
    accumulate dead instances;
  - graceful deregistration after the HTTP server drains;
  - fail-open: registry outage degrades to authored topology values and never
    fails startup.
- Standalone hosts:
  - `sdkwork-api-webserver-standalone-gateway` and
    `sdkwork-webserver-agent` call
    `sdkwork_discovery_client::install_resolver_from_env` before constructing
    their HTTP SDK clients, so the webserver edge resolves drive/knowledgebase/
    webserver peers through the shared registry;
  - the registry gate now also fails a standalone host that is present but does
    not install the resolver.
- Resolution precedence (`sdkwork-utils-rust`):
  explicit override → in-process → healthy registry instance → authored
  topology URL → split default (`BaseUrlSource::Registry` provenance).
- Configuration:
  `GATEWAY_DISCOVERY_NAMESPACE`, `GATEWAY_DISCOVERY_HTTP_REGISTRY_ENABLED`,
  `GATEWAY_PUBLIC_HTTP_URL`, `GATEWAY_DISCOVERY_DATABASE_URL`.

## 6. Media app-api audit (video, audio, image, music)

- `sdkwork-api-video-assembly`, `sdkwork-api-audio-assembly`, and
  `sdkwork-api-music-assembly` currently return empty route manifests
  (`Router::new()`); the real media generation surface is
  `sdkwork-api-generations-assembly` plus `sdkwork-api-image-assembly`.
- The generations media provider registry builds a
  `CloudRouterMediaGateway` from environment settings. `CloudRouterMediaGateway`
  re-resolves `cloudrouter` on every call and rebuilds its SDK client when the
  registry target changes, so a failed instance is replaced without a process
  restart; the configured URL remains the degradation path. Transport failures
  from the media adapters, the agent media tools, and HTTP chat turns are fed
  back with `report_current_registry_failure` / `report_registry_result`, so
  media and turn traffic demote an unreachable target. HTTP status errors are
  not reported as target failures (the target answered; the upstream may be at
  fault). Integration tests `media_registry_resolution`,
  `media_client_registry`, and `turn_transport_registry` prove target selection
  and failure reporting.
- The agent media tool family (image/audio/video/music) constructs a
  `CloudRouterMediaClient` per call. `cloudrouter_base_url()` now resolves
  `cloudrouter` through the registry, replacing the old hard-coded
  `http://127.0.0.1:3900` fallback when a registry instance exists. Integration
  test `tests/media_client_registry.rs` proves the media client picks the
  registry target.
- The image app-api injects an in-process provider when embedded in the
  gateway (no HTTP self-call); the standalone image gateway builds
  `ImageGenerationHost::from_runtime_env`, whose
  `ImageGenerationProviderAdapter::from_settings` re-resolves `cloudrouter` per
  call and rebuilds its SDK client on a target change. Integration test
  `registry_failover` proves the switch without a restart.- Video/audio/music standalone provider adapters receive an injected
  `SdkworkAiClient`; no production constructor was found. Any future host is
  covered by the registry gate (`new_with_base_url` / `SdkworkConfig::new`).

## 7. Verification performed

- `cargo test -p sdkwork-discovery-client` — QoS ordering, cooldown demotion,
  success reset, cache-only resolution, and the active health probe (4 unit +
  3 selection tests).
- `cargo test -p sdkwork-discovery-client --test registry_postgres` against a
  real PostgreSQL 18 — idempotent re-registration (one row, stable lease),
  heartbeat extension, weight ordering, status demotion, deregistration, and the
  producer-side expiry sweep (1 test, skipped without
  `SDKWORK_TEST_DATABASE_URL`).
- `cargo test -p sdkwork-api-cloud-gateway --test service_registry_postgres`
  against the same database — the real gateway runtime registers every composed
  surface and SDK alias for its exact port (18080), the endpoint is the
  authored public URL, and graceful shutdown deregisters the rows (1 test).
- `cargo test -p sdkwork-utils-rust --lib service_base_url` — 22 tests
  including registry precedence and embedded bypass.
- `cargo test -p sdkwork-api-cloud-gateway --lib` — 41 tests including profile
  publication and surface alias registration.
- `cargo test -p sdkwork-agents-memory-bridge -p sdkwork-agents-knowledgebase-bridge`
  and `cargo test -p sdkwork-generations-provider-adapter` (53 tests) pass
  after the registry-first conversion.
- Media paths: `media_registry_resolution` (generations media gateway,
  including an online target switch), `media_client_registry` (agents
  image/audio/video/music tool client), `registry_failover` (standalone image
  adapter online switch), and `turn_transport_registry` (failed HTTP turn
  reports the target) pass and prove target selection, failover, and failure
  reporting.
- `cargo test` for the remaining converted consumers: canvas 4, notes 4,
  course 4, deployments 18, image host 4, knowledgebase agent provider 36,
  voice 0 (compiles). Total 70 additional tests.
- The webserver standalone-gateway and agent crates were type-checked on Linux
  via WSL (`cargo check` finished in 1m51s); the local Windows `gcc.exe` cannot
  compile `ring`/`aws-lc-sys` (`0xc0000409`). The subsequent test-binary build
  hit a WSL environment bus error / I/O failure after the successful check; no
  code error was reported.
- `node --test scripts/deployment-contract.test.mjs` — 25/25.
- `node scripts/validate-docker-deployment.mjs --matrix` — 10 profiles.
- `node sdkwork-specs/tools/check-http-registry-resolution.mjs --workspace .`
  — 6,124 Rust files scanned, 0 legacy sites baselined, no bypass.
- `cargo metadata --locked`, `cargo fmt --check`, Rust naming/manifest gates,
  `check-discovery-standard` — pass.

## 8. Remaining rollout

- Operator runbook: [`docs/guides/operator/HTTP_SERVICE_REGISTRY.md`](../guides/operator/HTTP_SERVICE_REGISTRY.md)
  covers the table, producer/consumer behaviour, env keys, OEM/multi-cluster
  guidance, verification SQL, and troubleshooting.
- Build and deploy `0.1.37` on a Docker-capable host (this workspace currently
  has no Docker daemon); the source and release notes are ready.
- Long-lived image host: `ImageGenerationHost` resolves through the reloading
  `ImageGenerationProviderAdapter` (target changes are picked up per call); the
  generations media gateway follows target changes as well.
- Add the registry keys to the live bundle before/with the 0.1.37 deploy
  (`GATEWAY_DISCOVERY_NAMESPACE`, `GATEWAY_PUBLIC_HTTP_URL`,
  `GATEWAY_DISCOVERY_HTTP_REGISTRY_ENABLED`). The existing production env is
  preserved across deploys, so `deploy.sh --set` (or `bin/config.sh set`) is the
  safe path. `GATEWAY_PUBLIC_HTTP_URL` must be the real public domain when
  consumers outside the compose network resolve the gateway; otherwise the
  derived container-hostname URL works only inside the deployment network.
- Consumer sweep: every detected backend Rust service that constructs a
  generated HTTP SDK now resolves the base URL through
  `sdkwork_utils_rust::service_base_url` / `registry_base_url_or`
  (`DISCOVERY_SPEC.md` §12.1). Converted: the gateway, the embedded
  agents/cloudrouter path, agents memory/knowledgebase bridges, aiot
  intelligence bridge, generations provider adapter, knowledgebase
  memory/drive/agent-provider, canvas and notes drive facades, course drive
  adapter, voice and image generation bootstrap, deployments web/drive/content
  ports, webserver website runtime, and the webserver agent control-plane
  client. The two remaining test-only loopback stubs carry an explicit
  `registry:allow` marker. The gate runs with an empty baseline and fails any
  new bypass.
