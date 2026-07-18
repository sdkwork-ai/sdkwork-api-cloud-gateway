use std::{collections::HashMap, fs};

use sdkwork_api_cloud_gateway_config::{
    DependencyRuntimeMode, GatewayMode, GatewayRuntimeConfig,
};

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

        [cors]
        allowAnyOrigin = false
        allowedOrigins = ["https://app.sdkwork.com", "http://127.0.0.1:1420"]
    "#;
    GatewayRuntimeConfig::from_toml_str(toml).expect("exact origins should be accepted");
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
fn application_manifest_publishes_only_embedded_assembly_surfaces() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace root");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("load app manifest");

    assert!(!config.dependency_surfaces.is_empty());
    for surface in &config.dependency_surfaces {
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::Embedded);
        assert!(surface.cargo_feature.as_deref().is_some_and(|value| !value.is_empty()));
        assert!(surface.cargo_dependency.as_deref().is_some_and(|value| !value.is_empty()));
        assert!(surface.executable_export.as_deref().is_some_and(|value| !value.is_empty()));
    }
}

#[test]
fn packaged_configs_publish_the_manager_operations_suite() {
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
            .join("configs")
            .join(format!("sdkwork-api-cloud-gateway.{profile}.toml.example"));
        let config = GatewayRuntimeConfig::from_path(&path)
            .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
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
fn im_and_clawrouter_use_their_gateway_assemblies() {
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
    assert_eq!(im.len(), 4);
    assert!(im.iter().all(|surface| {
        surface.cargo_feature.as_deref() == Some("foundation-im")
            && surface.cargo_dependency.as_deref() == Some("sdkwork-im-gateway-assembly")
            && surface.executable_export.as_deref()
                == Some("sdkwork_im_gateway_assembly::assemble_application_router")
    }));

    let clawrouter = config
        .dependency_surfaces
        .iter()
        .find(|surface| surface.workspace == "sdkwork-clawrouter")
        .expect("ClawRouter surface");
    assert_eq!(clawrouter.api_prefix, "/v1");
    assert_eq!(clawrouter.cargo_feature.as_deref(), Some("foundation-clawrouter"));
    assert_eq!(
        clawrouter.cargo_dependency.as_deref(),
        Some("sdkwork-clawrouter-gateway-assembly")
    );
}

#[test]
fn membership_app_and_backend_use_one_embedded_gateway_assembly() {
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
            && surface.cargo_dependency.as_deref()
                == Some("sdkwork-membership-gateway-assembly")
            && surface.executable_export.as_deref()
                == Some(
                    "sdkwork_membership_gateway_assembly::assemble_application_business_router_from_env",
                )
    }));
}

#[test]
fn env_cannot_select_a_retired_gateway_mode() {
    let env = HashMap::from([(
        "SDKWORK_API_CLOUD_GATEWAY_MODE".to_string(),
        "split".to_string(),
    )]);
    assert!(GatewayRuntimeConfig::default().with_env_overrides(&env).is_err());
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
