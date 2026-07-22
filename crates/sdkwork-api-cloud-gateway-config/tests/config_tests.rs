use std::{
    collections::{HashMap, HashSet},
    fs,
};

use sdkwork_api_cloud_gateway_config::{DependencyRuntimeMode, GatewayMode, GatewayRuntimeConfig};

#[test]
fn default_config_is_embedded_only() {
    let config = GatewayRuntimeConfig::default();
    assert_eq!(config.mode, GatewayMode::Embedded);
    assert!(config
        .dependency_surfaces
        .iter()
        .all(|surface| surface.runtime_mode == DependencyRuntimeMode::Embedded));
}

#[test]
fn retired_gateway_modes_are_rejected() {
    assert!(GatewayMode::parse("split").is_err());
    assert!(GatewayMode::parse("split-or-embedded").is_err());
}

#[test]
fn retired_dependency_modes_are_rejected() {
    let toml = r#"
        mode = "embedded"

        [[dependencySurfaces]]
        serviceId = "legacy"
        workspace = "legacy"
        sdkFamily = "legacy"
        apiAuthority = "legacy"
        surface = "app"
        apiPrefix = "/legacy"
        runtimeMode = "split"
    "#;
    assert!(toml::from_str::<GatewayRuntimeConfig>(toml).is_err());
}

#[test]
fn per_dependency_base_urls_are_rejected() {
    let toml = r#"
        [[dependencySurfaces]]
        serviceId = "sdkwork-drive-app-api"
        workspace = "sdkwork-drive"
        sdkFamily = "sdkwork-drive-app-sdk"
        apiAuthority = "sdkwork-drive-app-api"
        surface = "app"
        apiPrefix = "/app/v3/api/drive"
        runtimeMode = "embedded"
        baseUrl = "http://127.0.0.1:4900"
        executableExport = "sdkwork_api_drive_assembly::assemble_business_routes_from_env"
        cargoFeature = "foundation-drive"
        cargoDependency = "sdkwork-api-drive-assembly"
        coverage = "embedded-assembly-routes"
    "#;
    assert!(GatewayRuntimeConfig::from_toml_str(toml).is_err());

    let manifest = r#"{
      "appId": "sdkwork-api-cloud-gateway",
      "sdkDependencies": [{
        "serviceId": "sdkwork-drive-app-api",
        "workspace": "sdkwork-drive",
        "sdkFamily": "sdkwork-drive-app-sdk",
        "apiAuthority": "sdkwork-drive-app-api",
        "surface": "app",
        "apiPrefix": "/app/v3/api/drive",
        "runtimeMode": "embedded",
        "upstreamUrl": "http://127.0.0.1:4900",
        "executableExport": "sdkwork_api_drive_assembly::assemble_business_routes_from_env",
        "cargoFeature": "foundation-drive",
        "cargoDependency": "sdkwork-api-drive-assembly",
        "coverage": "embedded-assembly-routes"
      }]
    }"#;
    assert!(GatewayRuntimeConfig::from_sdkwork_manifest_str(manifest).is_err());
}

#[test]
fn embedded_surfaces_require_complete_assembly_evidence() {
    let toml = r#"
        [[dependencySurfaces]]
        serviceId = "sdkwork-drive-app-api"
        workspace = "sdkwork-drive"
        sdkFamily = "sdkwork-drive-app-sdk"
        apiAuthority = "sdkwork-drive-app-api"
        surface = "app"
        apiPrefix = "/app/v3/api/drive"
        runtimeMode = "embedded"
    "#;
    assert!(GatewayRuntimeConfig::from_toml_str(toml).is_err());
}

#[test]
fn production_app_api_rejects_permissive_or_empty_cors() {
    for cors in [
        "allowAnyOrigin = true\nallowedOrigins = []",
        "allowAnyOrigin = false\nallowedOrigins = []",
    ] {
        let toml = format!(
            r#"
            [service]
            environment = "production"

            [[dependencySurfaces]]
            serviceId = "sdkwork-iam-app-api"
            workspace = "sdkwork-appbase"
            sdkFamily = "sdkwork-iam-app-sdk"
            apiAuthority = "sdkwork-iam-app-api"
            surface = "app"
            apiPrefix = "/app/v3/api"
            runtimeMode = "embedded"

            [cors]
            {cors}
            "#
        );
        assert!(GatewayRuntimeConfig::from_toml_str(&toml).is_err());
    }
}

#[test]
fn production_app_api_accepts_exact_cors_origins() {
    let toml = r#"
        [service]
        environment = "production"

        [[dependencySurfaces]]
        serviceId = "sdkwork-iam-app-api"
        workspace = "sdkwork-appbase"
        sdkFamily = "sdkwork-iam-app-sdk"
        apiAuthority = "sdkwork-iam-app-api"
        surface = "app"
        apiPrefix = "/app/v3/api"
        runtimeMode = "embedded"
        executableExport = "sdkwork_api_iam_assembly::assemble_business_router"
        cargoFeature = "foundation-appbase"
        cargoDependency = "sdkwork-api-iam-assembly"
        coverage = "embedded-assembly-routes"

        [cors]
        allowAnyOrigin = false
        allowedOrigins = ["https://app.sdkwork.com", "http://127.0.0.1:1420"]
    "#;
    GatewayRuntimeConfig::from_toml_str(toml).expect("exact origins should be accepted");
}

#[test]
fn development_cors_can_be_overridden_for_lan_clients() {
    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_CORS_ALLOW_ANY_ORIGIN".to_owned(),
        "true".to_owned(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_CORS_ALLOWED_ORIGINS".to_owned(),
        String::new(),
    );

    let config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [service]
        environment = "development"

        [cors]
        allowAnyOrigin = false
        allowedOrigins = ["http://127.0.0.1:5173"]
        "#,
    )
    .expect("development config")
    .with_env_overrides(&env)
    .expect("LAN CORS overrides");

    assert!(config.cors.allow_any_origin);
    assert!(config.cors.allowed_origins.is_empty());
}

#[test]
fn production_cors_env_override_cannot_enable_any_origin() {
    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_CORS_ALLOW_ANY_ORIGIN".to_owned(),
        "true".to_owned(),
    );

    let config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [service]
        environment = "production"

        [cors]
        allowAnyOrigin = false
        allowedOrigins = ["https://app.sdkwork.com"]
        "#,
    )
    .expect("production config");

    assert!(config.with_env_overrides(&env).is_err());
}

#[test]
fn production_app_api_rejects_non_origin_cors_entries() {
    for origin in [
        "*",
        "https://*.sdkwork.com",
        "https://app.sdkwork.com/login",
        "https://user@app.sdkwork.com",
        "file://desktop",
    ] {
        let toml = format!(
            r#"
            [service]
            environment = "production"

            [[dependencySurfaces]]
            serviceId = "sdkwork-iam-app-api"
            workspace = "sdkwork-appbase"
            sdkFamily = "sdkwork-iam-app-sdk"
            apiAuthority = "sdkwork-iam-app-api"
            surface = "app"
            apiPrefix = "/app/v3/api"
            runtimeMode = "embedded"

            [cors]
            allowAnyOrigin = false
            allowedOrigins = ["{origin}"]
            "#
        );
        assert!(
            GatewayRuntimeConfig::from_toml_str(&toml).is_err(),
            "origin should be rejected: {origin}"
        );
    }
}

#[test]
fn application_component_publishes_only_embedded_assembly_surfaces() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("load app manifest");

    assert!(!config.dependency_surfaces.is_empty());
    for surface in &config.dependency_surfaces {
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::Embedded);
        assert!(surface
            .cargo_feature
            .as_deref()
            .is_some_and(|value| !value.is_empty()));
        assert!(surface
            .cargo_dependency
            .as_deref()
            .is_some_and(|value| !value.is_empty()));
        assert!(surface
            .executable_export
            .as_deref()
            .is_some_and(|value| !value.is_empty()));
    }
}

#[test]
fn catalog_app_authority_publishes_every_owned_prefix() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("load app manifest");
    let catalog = config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.service_id == "sdkwork-catalog-app-api")
        .collect::<Vec<_>>();
    let prefixes = catalog
        .iter()
        .map(|surface| surface.api_prefix.as_str())
        .collect::<HashSet<_>>();

    assert_eq!(
        prefixes,
        HashSet::from([
            "/app/v3/api/addresses",
            "/app/v3/api/cart",
            "/app/v3/api/catalog",
        ])
    );
    assert!(catalog.iter().all(|surface| {
        surface.workspace == "sdkwork-catalog"
            && surface.cargo_feature.as_deref() == Some("foundation-catalog")
            && surface.cargo_dependency.as_deref() == Some("sdkwork-api-catalog-assembly")
            && surface.executable_export.as_deref()
                == Some("sdkwork_api_catalog_assembly::assemble_api_router_from_env")
    }));
}

#[test]
fn source_runtime_configs_publish_the_manager_operations_suite() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let expected_prefixes = [
        "/backend/v3/api/drive",
        "/backend/v3/api/orders",
        "/backend/v3/api/after_sales",
        "/backend/v3/api/shipments",
        "/backend/v3/api/account_value_packages",
        "/backend/v3/api/token_bank_plans",
        "/backend/v3/api/refund_requests",
        "/backend/v3/api/withdrawal_requests",
        "/backend/v3/api/promotions",
        "/backend/v3/api/payments",
    ];

    for profile in ["development", "production", "test"] {
        let path = root
            .join("etc")
            .join(format!("sdkwork-api-cloud-gateway.{profile}.toml"));
        let config = GatewayRuntimeConfig::from_path(&path)
            .and_then(|config| config.with_application_component_surfaces(root))
            .unwrap_or_else(|error| panic!("load {}: {error}", path.display()));
        assert_eq!(config.mode, GatewayMode::Embedded);
        for prefix in expected_prefixes {
            assert!(
                config
                    .dependency_surfaces
                    .iter()
                    .any(|surface| surface.api_prefix == prefix),
                "{} contains {prefix}",
                path.display()
            );
        }
    }
}

#[test]
fn im_and_clawrouter_use_their_api_assemblies() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("load app manifest");
    let im = config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.workspace == "sdkwork-im")
        .collect::<Vec<_>>();
    let im_prefixes = im
        .iter()
        .map(|surface| surface.api_prefix.as_str())
        .collect::<HashSet<_>>();
    let expected_im_prefixes = HashSet::from([
        "/app/v3/api/automation",
        "/app/v3/api/chat/conversations",
        "/app/v3/api/media",
        "/app/v3/api/notifications",
        "/app/v3/api/portal/access",
        "/app/v3/api/portal/automation",
        "/app/v3/api/portal/conversations",
        "/app/v3/api/portal/dashboard",
        "/app/v3/api/portal/governance",
        "/app/v3/api/portal/home",
        "/app/v3/api/portal/media",
        "/app/v3/api/portal/realtime",
        "/app/v3/api/portal/workspace",
        "/app/v3/api/streams",
        "/im/v3/api",
        "/backend/v3/api/control",
        "/backend/v3/api/audit",
        "/backend/v3/api/ops",
    ]);
    assert_eq!(im_prefixes, expected_im_prefixes);
    let im_service_ids = im
        .iter()
        .map(|surface| surface.service_id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        im_service_ids,
        HashSet::from([
            "sdkwork-im-app-api",
            "sdkwork-im-backend-api",
            "sdkwork-im-open-api",
        ])
    );
    assert!(im.iter().all(|surface| {
        surface.cargo_feature.as_deref() == Some("foundation-im")
            && surface.cargo_dependency.as_deref() == Some("sdkwork-api-im-assembly")
            && surface.executable_export.as_deref()
                == Some("sdkwork_api_im_assembly::assemble_api_router")
    }));

    let clawrouter = config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.workspace == "sdkwork-clawrouter")
        .collect::<Vec<_>>();
    let clawrouter_prefixes = clawrouter
        .iter()
        .map(|surface| surface.api_prefix.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        clawrouter_prefixes,
        HashSet::from([
            "/app/v3/api/ai/channel_groups",
            "/app/v3/api/ai/dashboard",
            "/app/v3/api/ai/gateway",
            "/app/v3/api/ai/generations",
            "/app/v3/api/ai/routing",
            "/app/v3/api/ai/usage",
            "/app/v3/api/memory",
            "/app/v3/api/notification",
            "/app/v3/api/runtime",
            "/app/v3/api/system/site",
            "/backend/v3/api/ai/channel_groups",
            "/backend/v3/api/ai/route_explain",
            "/backend/v3/api/integration",
            "/backend/v3/api/sites",
            "/backend/v3/api/system",
            "/anthropic/v1",
            "/google/v1beta",
            "/vidu/ent",
            "/suno/v1",
            "/volcengine/api",
            "/kling/v1",
            "/midjourney/v1",
            "/nano-banana/v1",
            "/v1",
        ])
    );
    let clawrouter_service_ids = clawrouter
        .iter()
        .map(|surface| surface.service_id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(
        clawrouter_service_ids,
        HashSet::from([
            "sdkwork-clawrouter-app-api",
            "sdkwork-clawrouter-backend-api",
            "sdkwork-clawrouter-open-api",
        ])
    );
    assert!(clawrouter.iter().all(|surface| {
        surface.cargo_feature.as_deref() == Some("foundation-clawrouter")
            && surface.cargo_dependency.as_deref() == Some("sdkwork-api-clawrouter-assembly")
    }));
}

#[test]
fn local_router_uses_conflict_free_canonical_open_api_prefixes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("load app manifest");
    let prefixes = config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.workspace == "sdkwork-local-router")
        .map(|surface| surface.api_prefix.as_str())
        .collect::<HashSet<_>>();

    assert!(prefixes.contains("/app/v3/api/local_router"));
    assert!(prefixes.contains("/backend/v3/api/local_router"));
    assert!(prefixes.contains("/local-router/v1"));
    assert!(prefixes.contains("/local-router/anthropic"));
    assert!(prefixes.contains("/local-router/google"));
    assert!(!prefixes.contains("/v1"));
    assert!(!prefixes.contains("/anthropic"));
    assert!(!prefixes.contains("/google"));
}

#[test]
fn membership_app_and_backend_use_one_embedded_api_assembly() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("load app manifest");
    let membership = config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.workspace == "sdkwork-membership")
        .collect::<Vec<_>>();

    assert_eq!(membership.len(), 2);
    assert!(membership
        .iter()
        .any(|surface| surface.api_prefix == "/app/v3/api/memberships"));
    assert!(membership
        .iter()
        .any(|surface| surface.api_prefix == "/backend/v3/api/memberships"));
    assert!(membership.iter().all(|surface| {
        surface.runtime_mode == DependencyRuntimeMode::Embedded
            && surface.cargo_feature.as_deref() == Some("foundation-membership")
            && surface.cargo_dependency.as_deref() == Some("sdkwork-api-membership-assembly")
            && surface.executable_export.as_deref()
                == Some("sdkwork_api_membership_assembly::assemble_api_router_from_env")
    }));
}

#[test]
fn env_cannot_select_a_retired_gateway_mode() {
    let env = HashMap::from([(
        "SDKWORK_API_CLOUD_GATEWAY_MODE".to_string(),
        "split".to_string(),
    )]);
    assert!(GatewayRuntimeConfig::default()
        .with_env_overrides(&env)
        .is_err());
}

#[test]
fn repository_contracts_contain_no_retired_runtime_markers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    for relative in ["sdkwork.app.config.json", "specs/component.spec.json"] {
        let source = fs::read_to_string(root.join(relative)).expect("contract source");
        assert!(!source.contains("split-or-embedded"));
        assert!(!source.contains("\"split\""));
        assert!(!source.contains("upstream-routes"));
    }
}
