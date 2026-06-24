use sdkwork_api_cloud_gateway_config::{DependencyApiSurfaceConfig, DependencyRuntimeMode};
use sdkwork_api_cloud_gateway_registry::{
    GatewayRouteRegistry, RouteExecutionTarget, RouteProtocol, RouteVisibility,
};

fn custom_dependency_surface() -> DependencyApiSurfaceConfig {
    DependencyApiSurfaceConfig {
        service_id: "custom-open-api".to_string(),
        workspace: "custom-service".to_string(),
        sdk_family: "sdkwork-custom-open-sdk".to_string(),
        api_authority: "custom-open-api".to_string(),
        surface: "open".to_string(),
        api_prefix: "/custom/v1".to_string(),
        runtime_mode: DependencyRuntimeMode::SplitOrEmbedded,
        same_origin_allowed: true,
        executable_export: Some("custom_service::build_router".to_string()),
        cargo_feature: None,
        cargo_dependency: None,
        coverage: "custom-open-api-routes".to_string(),
        required_base_url_key: Some("CUSTOM_OPEN_API_BASE_URL".to_string()),
    }
}

fn dependency_surface(
    service_id: &str,
    workspace: &str,
    sdk_family: &str,
    surface: &str,
    api_prefix: &str,
) -> DependencyApiSurfaceConfig {
    DependencyApiSurfaceConfig {
        service_id: service_id.to_string(),
        workspace: workspace.to_string(),
        sdk_family: sdk_family.to_string(),
        api_authority: service_id.to_string(),
        surface: surface.to_string(),
        api_prefix: api_prefix.to_string(),
        runtime_mode: DependencyRuntimeMode::Split,
        same_origin_allowed: true,
        executable_export: None,
        cargo_feature: None,
        cargo_dependency: None,
        coverage: format!("{service_id}-coverage"),
        required_base_url_key: Some(format!(
            "{}_BASE_URL",
            service_id.replace('-', "_").to_uppercase()
        )),
    }
}

#[test]
fn default_registry_resolves_appbase_iam_routes_as_dependency_routes() {
    let registry = GatewayRouteRegistry::sdkwork_default();
    let descriptor = registry
        .resolve("POST", "/app/v3/api/auth/login")
        .expect("auth route resolves");

    assert_eq!(descriptor.service_id, "sdkwork-appbase-app-api");
    assert_eq!(
        descriptor.execution_target,
        RouteExecutionTarget::Dependency
    );
    assert_eq!(descriptor.protocol, RouteProtocol::Http);
    assert_eq!(descriptor.visibility, RouteVisibility::App);
    assert!(descriptor.requires_executable_dependency);
}

#[test]
fn default_registry_does_not_treat_unknown_app_paths_as_appbase_iam() {
    let registry = GatewayRouteRegistry::sdkwork_default();

    assert!(registry
        .resolve("GET", "/app/v3/api/chat/threads")
        .is_none());
}

#[test]
fn registry_from_dependency_surfaces_routes_non_appbase_prefixes_without_expanding_appbase() {
    let mut appbase_surface = custom_dependency_surface();
    appbase_surface.service_id = "sdkwork-appbase-app-api".to_string();
    appbase_surface.api_prefix = "/app/v3/api".to_string();
    appbase_surface.surface = "app".to_string();

    let registry = GatewayRouteRegistry::from_dependency_surfaces(&[
        appbase_surface,
        custom_dependency_surface(),
    ]);

    assert!(registry
        .resolve("GET", "/app/v3/api/chat/threads")
        .is_none());

    let descriptor = registry
        .resolve("POST", "/custom/v1/chat/completions")
        .expect("custom dependency prefix resolves");
    assert_eq!(descriptor.service_id, "custom-open-api");
    assert_eq!(descriptor.visibility, RouteVisibility::Open);
    assert_eq!(
        descriptor.execution_target,
        RouteExecutionTarget::Dependency
    );
    assert!(descriptor.requires_executable_dependency);
}

#[test]
fn registry_resolves_specific_foundation_routes_before_broad_commerce_fallbacks() {
    let registry = GatewayRouteRegistry::from_dependency_surfaces(&[
        dependency_surface(
            "sdkwork-commerce-app-api",
            "sdkwork-commerce",
            "sdkwork-commerce-app-sdk",
            "app",
            "/app/v3/api",
        ),
        dependency_surface(
            "sdkwork-commerce-backend-api",
            "sdkwork-commerce",
            "sdkwork-commerce-backend-sdk",
            "backend",
            "/backend/v3/api",
        ),
        dependency_surface(
            "sdkwork-appbase-backend-api",
            "sdkwork-appbase",
            "sdkwork-appbase-backend-sdk",
            "backend",
            "/backend/v3/api/iam",
        ),
        dependency_surface(
            "sdkwork-drive-app-api",
            "sdkwork-drive",
            "sdkwork-drive-app-sdk",
            "app",
            "/app/v3/api/drive",
        ),
        dependency_surface(
            "sdkwork-notary-app-api",
            "sdkwork-notary",
            "sdkwork-notary-app-sdk",
            "app",
            "/app/v3/api/notary",
        ),
        dependency_surface(
            "sdkwork-rtc-app-api",
            "sdkwork-rtc",
            "sdkwork-rtc-app-sdk",
            "app",
            "/app/v3/api/rtc",
        ),
        dependency_surface(
            "sdkwork-rtc-backend-api",
            "sdkwork-rtc",
            "sdkwork-rtc-backend-sdk",
            "backend",
            "/backend/v3/api/rtc",
        ),
        dependency_surface(
            "sdkwork-agent-app-api",
            "sdkwork-kernel",
            "sdkwork-agent-app-sdk",
            "app",
            "/app/v3/api/ai",
        ),
        dependency_surface(
            "sdkwork-agent-backend-api",
            "sdkwork-kernel",
            "sdkwork-agent-backend-sdk",
            "backend",
            "/backend/v3/api/ai",
        ),
        dependency_surface(
            "sdkwork-agent-open-api",
            "sdkwork-kernel",
            "sdkwork-agent-sdk",
            "open",
            "/agent/v3/api",
        ),
        dependency_surface(
            "sdkwork-aiot-app-api",
            "sdkwork-aiot",
            "sdkwork-aiot-app-sdk",
            "app",
            "/app/v3/api/iot",
        ),
        dependency_surface(
            "sdkwork-aiot-backend-api",
            "sdkwork-aiot",
            "sdkwork-aiot-backend-sdk",
            "backend",
            "/backend/v3/api/iot",
        ),
        dependency_surface(
            "sdkwork-appbase-app-api",
            "sdkwork-appbase",
            "sdkwork-appbase-app-sdk",
            "app",
            "/app/v3/api",
        ),
    ]);

    assert_eq!(
        registry
            .resolve("POST", "/app/v3/api/auth/login")
            .expect("appbase app auth route resolves")
            .service_id,
        "sdkwork-appbase-app-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/app/v3/api/drive/files")
            .expect("Drive app route resolves")
            .service_id,
        "sdkwork-drive-app-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/app/v3/api/notary/cases")
            .expect("Notary app route resolves")
            .service_id,
        "sdkwork-notary-app-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/app/v3/api/rtc/media_sessions")
            .expect("RTC app route resolves")
            .service_id,
        "sdkwork-rtc-app-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/backend/v3/api/rtc/provider_profiles")
            .expect("RTC backend route resolves")
            .service_id,
        "sdkwork-rtc-backend-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/app/v3/api/ai/agents")
            .expect("Agent app route resolves")
            .service_id,
        "sdkwork-agent-app-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/backend/v3/api/ai/agents")
            .expect("Agent backend route resolves")
            .service_id,
        "sdkwork-agent-backend-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/agent/v3/api/agents")
            .expect("Agent open route resolves")
            .service_id,
        "sdkwork-agent-open-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/app/v3/api/iot/devices")
            .expect("AIoT app route resolves")
            .service_id,
        "sdkwork-aiot-app-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/backend/v3/api/iot/devices")
            .expect("AIoT backend route resolves")
            .service_id,
        "sdkwork-aiot-backend-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/app/v3/api/orders")
            .expect("Commerce app fallback route resolves")
            .service_id,
        "sdkwork-commerce-app-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/backend/v3/api/iam/users")
            .expect("Appbase backend IAM route resolves")
            .service_id,
        "sdkwork-appbase-backend-api"
    );
    assert_eq!(
        registry
            .resolve("GET", "/backend/v3/api/orders")
            .expect("Commerce backend fallback route resolves")
            .service_id,
        "sdkwork-commerce-backend-api"
    );
}
