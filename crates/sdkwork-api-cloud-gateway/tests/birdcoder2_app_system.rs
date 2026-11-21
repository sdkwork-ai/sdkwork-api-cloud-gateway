//! The BirdCoder2 new app system must be a first-class gateway dependency.
//!
//! A dependency surface the component contract declares but the embedded
//! bootstrap never mounts answers 404 at the edge. That is exactly how the
//! About surface's `GET /app/v3/api/installation` regressed: the assembly
//! existed, but the gateway never composed it. These tests pin both halves of
//! the contract:
//!
//! - the component contract selects the `foundation-birdcoder2` surface set
//!   (mobile app-api, installation app-api, host-runtime internal-api);
//! - the embedded assembly really publishes every route of that set under the
//!   declared prefixes, with the auth shape each surface requires.

use std::collections::BTreeSet;
use std::path::Path;

use sdkwork_api_cloud_gateway_config::{DependencyRuntimeMode, GatewayRuntimeConfig};
use sdkwork_web_contract::RouteAuth;

fn application_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

#[test]
fn component_contract_selects_the_birdcoder2_app_system_surfaces() {
    let config = GatewayRuntimeConfig::from_application_root(application_root())
        .expect("gateway component contract parses");
    let surfaces = config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.workspace == "sdkwork-birdcoder2")
        .collect::<Vec<_>>();
    assert_eq!(surfaces.len(), 3, "one declaration per served surface");

    let prefixes = surfaces
        .iter()
        .map(|surface| (surface.surface.as_str(), surface.api_prefix.as_str()))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        prefixes,
        BTreeSet::from([
            ("app", "/app/v3/api/birdcoder"),
            ("app", "/app/v3/api/installation"),
            ("internal", "/internal/v3/api/birdcoder"),
        ])
    );

    for surface in surfaces {
        assert_eq!(
            surface.runtime_mode,
            DependencyRuntimeMode::Embedded,
            "{} must stay embedded",
            surface.service_id
        );
        assert_eq!(
            surface.cargo_feature.as_deref(),
            Some("foundation-birdcoder2")
        );
        assert_eq!(
            surface.cargo_dependency.as_deref(),
            Some("sdkwork-api-birdcoder2-assembly")
        );
        assert_eq!(
            surface.sdk_family, "sdkwork-birdcoder2-app-sdk",
            "the new app system addresses one SDK family"
        );
    }
}

#[cfg(feature = "foundation-birdcoder2")]
#[tokio::test]
async fn embedded_module_publishes_every_declared_app_system_route() {
    let module = sdkwork_api_birdcoder2_assembly::web_module()
        .await
        .expect("BirdCoder2 assembly composes without a database");
    let contribution = module
        .contributions()
        .first()
        .expect("BirdCoder2 module carries its route contribution");

    for (method, path, auth) in [
        ("GET", "/app/v3/api/installation", RouteAuth::DualToken),
        ("GET", "/app/v3/api/birdcoder/hosts", RouteAuth::DualToken),
        (
            "POST",
            "/app/v3/api/birdcoder/agent_sessions/{sessionId}/turns",
            RouteAuth::DualToken,
        ),
        (
            "POST",
            "/internal/v3/api/birdcoder/host_runtimes/hello",
            RouteAuth::IngressToken,
        ),
    ] {
        let route = contribution
            .route_manifest
            .match_route(method, path)
            .unwrap_or_else(|| panic!("{method} {path} must be in the BirdCoder2 manifest"));
        assert_eq!(
            route.auth, auth,
            "{method} {path} must keep its surface auth shape"
        );
    }
}

#[cfg(feature = "foundation-birdcoder2")]
#[tokio::test]
async fn composed_framework_manifest_carries_the_about_installation_read() {
    let module = sdkwork_api_birdcoder2_assembly::web_module()
        .await
        .expect("BirdCoder2 assembly composes without a database");
    let mut registry = sdkwork_web_bootstrap::ApiModuleRegistry::new();
    registry.add_module(module);
    let composed = registry
        .try_compose("SDKWork Platform API")
        .expect("BirdCoder2 module composes with the Web Framework");

    let route = composed
        .route_manifest
        .match_route("GET", "/app/v3/api/installation")
        .expect("the About installation read is part of the composed manifest");
    assert_eq!(route.operation_id, "installation.retrieve");
    assert!(
        composed.openapi["paths"]["/app/v3/api/installation"].is_object(),
        "the composed OpenAPI index must publish the installation read"
    );
}
