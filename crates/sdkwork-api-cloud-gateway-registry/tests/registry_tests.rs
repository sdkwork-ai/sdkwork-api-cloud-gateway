use sdkwork_api_cloud_gateway_config::{DependencyApiSurfaceConfig, DependencyRuntimeMode};
use sdkwork_api_cloud_gateway_registry::{
    GatewayRouteRegistry, RouteDescriptor, RouteExecutionTarget, RouteProtocol, RouteRegistryError,
    RouteVisibility,
};

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
        cargo_dependency: Some("sdkwork-api-test-assembly".to_string()),
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

fn exact_route(service_id: &str, method: &str, path: &str) -> RouteDescriptor {
    RouteDescriptor {
        service_id: service_id.to_owned(),
        path_pattern: path.to_owned(),
        api_prefix: "/app/v3/api".to_owned(),
        surface: "app".to_owned(),
        methods: vec![method.to_owned()],
        protocol: RouteProtocol::Http,
        visibility: RouteVisibility::App,
        execution_target: RouteExecutionTarget::Dependency,
        sdk_target: Some("sdkwork-test-app-sdk".to_owned()),
        requires_executable_dependency: true,
    }
}

#[test]
fn exact_registry_matches_named_path_parameters_and_methods() {
    let registry = GatewayRouteRegistry::try_from_exact_routes(vec![exact_route(
        "orders",
        "GET",
        "/app/v3/api/orders/{orderId}",
    )])
    .expect("valid exact registry");

    assert!(registry.resolve("GET", "/app/v3/api/orders/1001").is_some());
    assert!(registry
        .resolve("POST", "/app/v3/api/orders/1001")
        .is_none());
    assert!(registry.resolve("GET", "/app/v3/api/orders").is_none());
    assert!(registry
        .resolve("GET", "/app/v3/api/orders/1001/items")
        .is_none());
}

#[test]
fn exact_registry_rejects_normalized_cross_owner_collisions() {
    let error = GatewayRouteRegistry::try_from_exact_routes(vec![
        exact_route("orders", "GET", "/app/v3/api/orders/{orderId}"),
        exact_route("legacy-orders", "get", "/app/v3/api/orders/{id}"),
    ])
    .expect_err("normalized collision must fail");

    assert!(matches!(error, RouteRegistryError::RouteCollision { .. }));
}

#[test]
fn exact_registry_allows_same_template_for_different_methods() {
    let registry = GatewayRouteRegistry::try_from_exact_routes(vec![
        exact_route("orders", "GET", "/app/v3/api/orders/{orderId}"),
        exact_route("orders", "PATCH", "/app/v3/api/orders/{orderId}"),
    ])
    .expect("method-specific routes do not collide");

    assert_eq!(registry.routes().len(), 2);
}

#[test]
fn exact_registry_rejects_prefix_catch_all_entries() {
    let error = GatewayRouteRegistry::try_from_exact_routes(vec![exact_route(
        "orders",
        "GET",
        "/app/v3/api/orders/{*path}",
    )])
    .expect_err("catch-all cannot be exact route authority");

    assert!(matches!(
        error,
        RouteRegistryError::CatchAllNotAllowed { .. }
    ));
}
