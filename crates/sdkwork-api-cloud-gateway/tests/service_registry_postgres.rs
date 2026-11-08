//! End-to-end gateway registration against a real PostgreSQL database.
//!
//! Skipped unless `SDKWORK_TEST_DATABASE_URL` points at a disposable database.
//! The test creates a unique schema, starts the gateway's registry runtime,
//! verifies that every composed surface/alias is registered for this exact
//! port, and verifies graceful deregistration.

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use sdkwork_api_cloud_gateway::service_registry::ServiceRegistryRuntime;
use sdkwork_api_cloud_gateway_config::{
    DependencyApiSurfaceConfig, DependencyRuntimeMode, GatewayRuntimeConfig,
};
use sdkwork_discovery_client::{HttpRegistry, RegistryIdentity};
use sqlx::postgres::PgPoolOptions;

fn test_database_url() -> Option<String> {
    std::env::var("SDKWORK_TEST_DATABASE_URL")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn unique_schema() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("gateway_registry_test_{}_{}", std::process::id(), nanos)
}

fn surface(workspace: &str, service_id: &str, api_prefix: &str) -> DependencyApiSurfaceConfig {
    DependencyApiSurfaceConfig {
        service_id: service_id.to_owned(),
        workspace: workspace.to_owned(),
        sdk_family: "sdkwork-test".to_owned(),
        api_authority: "test".to_owned(),
        surface: "app".to_owned(),
        api_prefix: api_prefix.to_owned(),
        runtime_mode: DependencyRuntimeMode::Embedded,
        same_origin_allowed: true,
        executable_export: None,
        cargo_feature: None,
        cargo_dependency: None,
        coverage: "test".to_owned(),
    }
}

#[tokio::test]
async fn gateway_registers_every_surface_for_its_port_and_deregisters() {
    let Some(base_url) = test_database_url() else {
        eprintln!("SDKWORK_TEST_DATABASE_URL is not set; skipping gateway registry contract");
        return;
    };
    let schema = unique_schema();
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&base_url)
        .await
        .expect("test database connects");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
        .execute(&admin)
        .await
        .expect("test schema creates");

    let separator = if base_url.contains('?') { '&' } else { '?' };
    let schema_url = format!("{base_url}{separator}options=-c%20search_path%3D{schema}");
    // SAFETY: this integration test binary owns its process environment.
    std::env::set_var("SDKWORK_DISCOVERY_DATABASE_URL", &schema_url);
    std::env::set_var("SDKWORK_DISCOVERY_NAMESPACE", "platform");
    std::env::set_var("SDKWORK_ENVIRONMENT", "integration");
    std::env::set_var("SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID", "integration");
    std::env::set_var(
        "SDKWORK_API_CLOUD_GATEWAY_PRIVATE_HTTP_URL",
        "http://127.0.0.1:18080",
    );
    std::env::set_var("SDKWORK_API_CLOUD_GATEWAY_REGISTRY_PRIORITY", "20");
    std::env::set_var("SDKWORK_API_CLOUD_GATEWAY_REGISTRY_WEIGHT", "70");

    let mut config = GatewayRuntimeConfig::default();
    config.service.environment = "integration".to_owned();
    config.dependency_surfaces = vec![
        surface("sdkwork-cloudrouter", "sdkwork-cloudrouter-open-api", "/v1"),
        surface(
            "sdkwork-agents",
            "sdkwork-agents-app-api",
            "/app/v3/api/ai/agents",
        ),
    ];

    let runtime = ServiceRegistryRuntime::start(&config, SocketAddr::from(([127, 0, 0, 1], 18080)))
        .await
        .expect("the gateway registry runtime starts");

    let registry = HttpRegistry::connect(
        &schema_url,
        RegistryIdentity::new("platform", "integration"),
        2,
    )
    .await
    .expect("registry connects");
    let cloudrouter = registry
        .discover("cloudrouter")
        .await
        .expect("cloudrouter discovers");
    assert_eq!(
        cloudrouter.len(),
        1,
        "one gateway instance registers cloudrouter"
    );
    assert!(
        cloudrouter[0].instance_id.starts_with("integration:"),
        "instance id carries the profile id: {}",
        cloudrouter[0].instance_id
    );
    assert!(
        cloudrouter[0].instance_id.ends_with(":18080"),
        "instance id carries the exact port: {}",
        cloudrouter[0].instance_id
    );
    assert_eq!(cloudrouter[0].endpoint, "http://127.0.0.1:18080");
    assert_eq!(
        cloudrouter[0].priority, 20,
        "registration priority honors the env override"
    );
    assert_eq!(
        cloudrouter[0].weight, 70,
        "registration weight honors the env override"
    );

    let services = registry
        .discover_services()
        .await
        .expect("services discover");
    for expected in [
        "sdkwork-api-cloud-gateway",
        "api-gateway",
        "sdkwork-cloudrouter",
        "cloudrouter",
        "sdkwork-agents",
        "agents",
    ] {
        assert!(
            services.iter().any(|service| service == expected),
            "service `{expected}` was not registered: {services:?}"
        );
    }

    runtime.shutdown().await;
    assert!(
        registry
            .discover("cloudrouter")
            .await
            .expect("discover after shutdown")
            .is_empty(),
        "graceful shutdown deregisters the instance"
    );

    registry.close().await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .expect("test schema drops");
    admin.close().await;

    for key in [
        "SDKWORK_DISCOVERY_DATABASE_URL",
        "SDKWORK_DISCOVERY_NAMESPACE",
        "SDKWORK_ENVIRONMENT",
        "SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID",
        "SDKWORK_API_CLOUD_GATEWAY_PRIVATE_HTTP_URL",
        "SDKWORK_API_CLOUD_GATEWAY_REGISTRY_PRIORITY",
        "SDKWORK_API_CLOUD_GATEWAY_REGISTRY_WEIGHT",
    ] {
        std::env::remove_var(key);
    }
}
