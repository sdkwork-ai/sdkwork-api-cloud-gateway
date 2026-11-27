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
fn exact_registry_accepts_prefix_catch_all_as_lowest_specificity() {
    let registry = GatewayRouteRegistry::try_from_exact_routes(vec![
        exact_route("orders", "GET", "/app/v3/api/orders/{*path}"),
        exact_route("orders", "GET", "/app/v3/api/orders/{orderId}"),
    ])
    .expect("catch-all fallbacks are allowed below concrete patterns");

    assert_eq!(registry.routes().len(), 2);
    // Concrete patterns resolve before the catch-all fallback.
    let resolved = registry
        .resolve("GET", "/app/v3/api/orders/1001")
        .expect("concrete pattern wins over catch-all");
    assert_eq!(resolved.path_pattern, "/app/v3/api/orders/{orderId}");
    // The catch-all still resolves paths outside concrete patterns.
    let fallback = registry
        .resolve("GET", "/app/v3/api/orders/1001/items")
        .expect("catch-all covers remaining paths");
    assert_eq!(fallback.path_pattern, "/app/v3/api/orders/{*path}");
}

/// Regression for the Cloud Router `VM 实例` console (`/console/vm-instances`),
/// which reported "虚拟机实例列表不可用" with
/// "route is not registered in the gateway route manifest".
///
/// The console calls `GET /app/v3/api/sandbox/sandbox_instances`. Because the
/// broad appbase app-api surface (`/app/v3/api/auth`, `/app/v3/api/iam`, ...)
/// does not cover `/app/v3/api/sandbox`, the request resolved to nothing and
/// the gateway answered 40401 before any handler ran. Declaring the sandbox
/// dependency surface is what makes this resolve, so this test pins the
/// service identity and every operation verb the console issues.
#[test]
fn sandbox_instance_console_paths_resolve_to_the_sandbox_workspace() {
    let registry = GatewayRouteRegistry::from_dependency_surfaces(&[
        surface("sdkwork-iam-app-api", "/app/v3/api/auth"),
        surface("sdkwork-iam-app-api", "/app/v3/api/iam"),
        surface("sdkwork-sandbox-app-api", "/app/v3/api/sandbox"),
    ]);

    let listing = registry
        .resolve("GET", "/app/v3/api/sandbox/sandbox_instances")
        .expect("the VM instance listing must resolve, not 40401");
    assert_eq!(listing.service_id, "sdkwork-sandbox-app-api");
    assert_eq!(listing.api_prefix, "/app/v3/api/sandbox");

    // Create, retrieve, update and retire: the remaining console operations.
    for (method, path) in [
        ("POST", "/app/v3/api/sandbox/sandbox_instances"),
        ("GET", "/app/v3/api/sandbox/sandbox_instances/1234"),
        ("PATCH", "/app/v3/api/sandbox/sandbox_instances/1234"),
        ("DELETE", "/app/v3/api/sandbox/sandbox_instances/1234"),
    ] {
        let resolved = registry
            .resolve(method, path)
            .unwrap_or_else(|| panic!("{method} {path} must resolve"));
        assert_eq!(resolved.service_id, "sdkwork-sandbox-app-api");
    }

    // The internal face has its own stable prefix and must not be shadowed by
    // the app surface.
    assert!(
        registry
            .resolve(
                "GET",
                "/internal/v3/api/intelligence/sandbox/sandbox_instances"
            )
            .is_none(),
        "the internal face is not exposed on the app surface"
    );
}
