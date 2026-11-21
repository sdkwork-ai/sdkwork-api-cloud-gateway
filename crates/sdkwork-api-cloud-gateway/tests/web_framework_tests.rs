use axum::{
    body::Body,
    http::{header, Request},
    routing::{get, post},
    Router,
};
use sdkwork_api_cloud_gateway::{
    finalize_gateway_router, parse_web_framework_toggle, web_framework_enabled,
};
use sdkwork_api_cloud_gateway_config::{
    DependencyApiSurfaceConfig, DependencyRuntimeMode, GatewayRuntimeConfig, GatewayServiceConfig,
};
use tower::ServiceExt;

#[test]
fn framework_toggle_parses_standard_boolean_values() {
    assert!(parse_web_framework_toggle("true"));
    assert!(!parse_web_framework_toggle("false"));
}

#[tokio::test]
async fn embedded_outer_router_assigns_request_identity() {
    let router = finalize_gateway_router(
        &GatewayRuntimeConfig::default(),
        Router::new().route("/healthz", get(|| async { "ok" })),
    );
    let response = router
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(response.headers().contains_key("x-request-id"));
    let _ = web_framework_enabled();
}

#[tokio::test]
async fn embedded_outer_router_bridges_the_vendor_bearer_credential() {
    let response = vendor_wire_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header(header::AUTHORIZATION, "Bearer login-token-without-a-prefix")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = echo_headers(response.into_body()).await;
    let headers = body.as_str();
    // The published vendor wire is a single `Authorization: Bearer`; the profile
    // that serves it reads `X-API-Key`, so the bridge presents it canonically and
    // drops the vendor header rather than leaving both (a mixed profile the
    // framework rejects).
    assert!(
        headers.contains("x-api-key=login-token-without-a-prefix"),
        "vendor bearer must reach the dependency as x-api-key: {headers}"
    );
    assert!(
        !headers.contains("authorization="),
        "the bridged request must not carry both credential headers: {headers}"
    );
}

#[tokio::test]
async fn vendor_bridge_leaves_non_vendor_paths_untouched() {
    let response = vendor_wire_router()
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/auth/sessions/current")
                .header(header::AUTHORIZATION, "Bearer dual-token-pair")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let headers = echo_headers(response.into_body()).await;
    assert!(
        headers.contains("authorization=Bearer dual-token-pair"),
        "a platform path keeps its Authorization header: {headers}"
    );
    assert!(
        !headers.contains("x-api-key="),
        "the bridge must not rewrite a platform path: {headers}"
    );
}

/// Router whose vendor prefix reports the credential headers it received.
fn vendor_wire_router() -> Router {
    let config = GatewayRuntimeConfig {
        service: GatewayServiceConfig {
            name: "sdkwork-api-cloud-gateway".to_owned(),
            environment: "production".to_owned(),
        },
        dependency_surfaces: vec![DependencyApiSurfaceConfig {
            service_id: "sdkwork-cloudrouter-open-api".to_owned(),
            workspace: "sdkwork-cloudrouter".to_owned(),
            sdk_family: "cloudrouter-open-sdk".to_owned(),
            api_authority: "sdkwork-cloudrouter-open-api".to_owned(),
            surface: "open".to_owned(),
            api_prefix: "/v1".to_owned(),
            runtime_mode: DependencyRuntimeMode::Embedded,
            same_origin_allowed: true,
            executable_export: None,
            cargo_feature: None,
            cargo_dependency: None,
            coverage: String::new(),
        }],
        ..GatewayRuntimeConfig::default()
    };
    finalize_gateway_router(
        &config,
        Router::new()
            .route("/v1/chat/completions", post(report_credential_headers))
            .route(
                "/app/v3/api/auth/sessions/current",
                get(report_credential_headers),
            ),
    )
}

/// Report the credential headers a request arrived with.
async fn report_credential_headers(headers: axum::http::HeaderMap) -> String {
    let mut reported = Vec::new();
    for name in ["authorization", "x-api-key"] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            reported.push(format!("{name}={value}"));
        }
    }
    reported.join(";")
}

/// Read a response body as text.
async fn echo_headers(body: Body) -> String {
    let bytes = axum::body::to_bytes(body, 8192).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[test]
fn embedded_iam_uses_owner_api_surface_assembly() {
    let source = include_str!("../src/embedded_dependency_routes.rs");

    // Embedded IAM routes consume the IAM assembly's module-based owner API
    // surface (`web_module_with_pool`); the legacy bootstrap entrypoint is gone.
    assert!(source.contains("sdkwork_api_iam_assembly::web_module_with_pool"));
    assert!(source.contains("workspace_surface_enabled(config, \"sdkwork-iam\")"));
    assert!(!source.contains("bootstrap_iam_for_application"));
    assert!(!source.contains("wrap_router_with_iam_owner_web_framework"));
}

#[test]
fn embedded_platform_composition_applies_the_vendor_credential_bridge() {
    // The embedded runtime assembles its outer router directly, so it must install
    // the vendor bridge itself: a lone `Authorization: Bearer` on a vendor prefix
    // is otherwise rejected as an incomplete credential profile before any
    // credential channel (API key or login token) can authenticate it.
    let source = include_str!("../src/runtime.rs");
    assert!(
        source.contains("attach_vendor_credential_bridge"),
        "the embedded router must apply the vendor credential bridge"
    );
}

#[test]
fn shipped_component_spec_puts_the_cloudrouter_vendor_wire_behind_the_bridge() {
    // The bridge is scoped by the declared dependency prefixes, so the shipped
    // contract — not a hand-written list — must cover Cloud Router's `/v1` face.
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/component.spec.json");
    let config = GatewayRuntimeConfig::from_component_spec_path(&path)
        .expect("the shipped component contract parses");
    let prefixes =
        sdkwork_api_cloud_gateway::vendor_open_api_auth::vendor_open_api_prefixes(&config);
    assert!(
        prefixes.iter().any(|prefix| prefix == "/v1"),
        "the cloudrouter open surface must declare the /v1 wire: {prefixes:?}"
    );
    assert!(
        sdkwork_api_cloud_gateway::vendor_open_api_auth::is_vendor_open_api_path(
            "/v1/chat/completions",
            &prefixes
        ),
        "POST /v1/chat/completions must be bridged: {prefixes:?}"
    );
}
