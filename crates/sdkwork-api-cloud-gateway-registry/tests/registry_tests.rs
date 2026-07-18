use sdkwork_api_cloud_gateway_config::{DependencyApiSurfaceConfig, DependencyRuntimeMode};
use sdkwork_api_cloud_gateway_registry::GatewayRouteRegistry;

fn surface(service_id: &str, prefix: &str) -> DependencyApiSurfaceConfig {
    DependencyApiSurfaceConfig {
        service_id: service_id.to_string(),
        workspace: "workspace".to_string(),
        sdk_family: "sdk".to_string(),
        api_authority: service_id.to_string(),
        surface: "app".to_string(),
        api_prefix: prefix.to_string(),
        runtime_mode: DependencyRuntimeMode::Embedded,
        same_origin_allowed: true,
        executable_export: Some("assembly::router".to_string()),
        cargo_feature: Some("foundation-test".to_string()),
        cargo_dependency: Some("test-gateway-assembly".to_string()),
        coverage: "embedded-assembly-routes".to_string(),
    }
}

#[test]
fn more_specific_embedded_surface_wins() {
    let registry = GatewayRouteRegistry::from_dependency_surfaces(&[
        surface("iam", "/app/v3/api"),
        surface("drive", "/app/v3/api/drive"),
    ]);
    let route = registry
        .resolve("GET", "/app/v3/api/drive/sandboxes")
        .expect("Drive route");
    assert_eq!(route.service_id, "drive");
}
