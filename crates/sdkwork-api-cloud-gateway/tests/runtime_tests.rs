use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    routing::get,
    Json, Router,
};
use sdkwork_api_cloud_gateway::{
    build_sdkwork_api_cloud_gateway_router_with_embedded_routers, load_gateway_config,
};
use sdkwork_api_cloud_gateway_config::{
    DependencyApiSurfaceConfig, DependencyRuntimeMode, GatewayRuntimeConfig,
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path};
use tower::ServiceExt;

fn surface(service_id: &str, prefix: &str) -> DependencyApiSurfaceConfig {
    DependencyApiSurfaceConfig {
        service_id: service_id.to_string(),
        workspace: "sdkwork-test".to_string(),
        sdk_family: "sdkwork-test-app-sdk".to_string(),
        api_authority: service_id.to_string(),
        surface: "app".to_string(),
        api_prefix: prefix.to_string(),
        runtime_mode: DependencyRuntimeMode::Embedded,
        same_origin_allowed: true,
        executable_export: Some(
            "sdkwork_test_gateway_assembly::assemble_application_router".to_string(),
        ),
        cargo_feature: Some("foundation-test".to_string()),
        cargo_dependency: Some("sdkwork-test-gateway-assembly".to_string()),
        coverage: "embedded-assembly-routes".to_string(),
    }
}

fn config(surfaces: Vec<DependencyApiSurfaceConfig>) -> GatewayRuntimeConfig {
    GatewayRuntimeConfig {
        dependency_surfaces: surfaces,
        ..GatewayRuntimeConfig::default()
    }
}

#[test]
fn explicit_profile_configs_keep_component_dependency_surface_parity() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let manifest = GatewayRuntimeConfig::from_application_root(root).expect("load app manifest");
    let expected = manifest
        .dependency_surfaces
        .iter()
        .map(|surface| {
            (
                surface.service_id.as_str(),
                surface.api_prefix.as_str(),
                surface.surface.as_str(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();

    for profile in ["development", "production", "test"] {
        let config = load_gateway_config(
            [
                "--config".to_string(),
                format!("etc/sdkwork-api-cloud-gateway.{profile}.toml"),
            ],
            &HashMap::new(),
            root,
        )
        .unwrap_or_else(|error| panic!("load {profile} gateway config: {error}"));
        let actual = config
            .dependency_surfaces
            .iter()
            .map(|surface| {
                (
                    surface.service_id.as_str(),
                    surface.api_prefix.as_str(),
                    surface.surface.as_str(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "{profile} config diverges from app manifest"
        );
    }
}

#[test]
fn missing_embedded_router_fails_before_serving() {
    let result = build_sdkwork_api_cloud_gateway_router_with_embedded_routers(
        config(vec![surface("drive", "/app/v3/api/drive")]),
        std::iter::empty::<(&str, Router)>(),
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn dependency_request_is_delegated_to_embedded_router() {
    let dependency = Router::new().route(
        "/app/v3/api/drive/sandboxes",
        get(|| async { Json(json!({"code": 0, "data": {"items": []}})) }),
    );
    let router = build_sdkwork_api_cloud_gateway_router_with_embedded_routers(
        config(vec![surface("drive", "/app/v3/api/drive")]),
        [("drive", dependency)],
    )
    .expect("embedded gateway router");

    let response = router
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/drive/sandboxes?page=1&page_size=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["code"], 0);
}

#[tokio::test]
async fn specific_dependency_prefix_wins_over_broad_prefix() {
    let iam = Router::new().fallback(|| async { (StatusCode::OK, "iam") });
    let drive = Router::new().fallback(|| async { (StatusCode::OK, "drive") });
    let router = build_sdkwork_api_cloud_gateway_router_with_embedded_routers(
        config(vec![
            surface("iam", "/app/v3/api"),
            surface("drive", "/app/v3/api/drive"),
        ]),
        [("iam", iam), ("drive", drive)],
    )
    .expect("embedded gateway router");

    let response = router
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/drive/sandboxes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), b"drive");
}

#[tokio::test]
async fn unknown_route_returns_standard_not_found_problem() {
    let router = build_sdkwork_api_cloud_gateway_router_with_embedded_routers(
        config(Vec::new()),
        std::iter::empty::<(&str, Router)>(),
    )
    .expect("empty embedded gateway router");
    let response = router
        .oneshot(
            Request::builder()
                .uri("/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
