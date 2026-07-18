use axum::{body::Body, http::Request, routing::get, Router};
use sdkwork_api_cloud_gateway::{
    finalize_gateway_router, parse_web_framework_toggle, web_framework_enabled,
};
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
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
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(response.headers().contains_key("x-request-id"));
    let _ = web_framework_enabled();
}
