use axum::http::header;
use axum::{body::Body, http::Request, routing::get, Router};
use sdkwork_api_cloud_gateway::build_sdkwork_api_cloud_gateway_router;
use sdkwork_api_cloud_gateway::web_framework::split_proxy_call_chain;
use sdkwork_api_cloud_gateway::web_framework::{
    finalize_gateway_router, finalize_gateway_router_async, parse_web_framework_toggle,
    web_framework_enabled,
};
use sdkwork_api_cloud_gateway_config::{GatewayMode, GatewayRuntimeConfig};
use sdkwork_web_core::{is_canonical_uuid, WebCallInterceptorChain};
use std::sync::Mutex;
use tower::ServiceExt;

static WEB_FRAMEWORK_ENV_LOCK: Mutex<()> = Mutex::new(());

struct WebFrameworkEnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    key: &'static str,
    previous: Option<String>,
}

impl WebFrameworkEnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let lock = WEB_FRAMEWORK_ENV_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let previous = std::env::var(key).ok();
        unsafe {
            std::env::set_var(key, value);
        }
        Self {
            _lock: lock,
            key,
            previous,
        }
    }
}

impl Drop for WebFrameworkEnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

#[test]
fn web_framework_dependency_is_enabled_by_default() {
    let lock = WEB_FRAMEWORK_ENV_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let key = "SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED";
    let previous = std::env::var(key).ok();
    unsafe {
        std::env::remove_var(key);
    }
    assert!(web_framework_enabled());
    unsafe {
        match &previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
    drop(lock);
}

#[test]
fn parse_web_framework_toggle_accepts_standard_truthy_values() {
    for value in ["1", "true", "TRUE", " yes ", "On"] {
        assert!(
            parse_web_framework_toggle(value),
            "expected truthy toggle for {value:?}"
        );
    }
}

#[test]
fn parse_web_framework_toggle_rejects_standard_falsy_values() {
    for value in ["0", "false", "no", "off", "disabled", ""] {
        assert!(
            !parse_web_framework_toggle(value),
            "expected falsy toggle for {value:?}"
        );
    }
}

#[test]
fn split_proxy_interceptor_chain_adds_unknown_surface_bypass_stage() {
    let chain = split_proxy_call_chain::<sdkwork_web_core::DefaultWebRequestContextResolver>();
    assert_eq!(19, chain.interceptor_count());
}

#[test]
fn standard_interceptor_chain_preserves_required_stage_count() {
    let chain =
        WebCallInterceptorChain::<sdkwork_web_core::DefaultWebRequestContextResolver>::standard();
    assert_eq!(18, chain.interceptor_count());
}

#[tokio::test]
async fn split_mode_mounts_health_route_through_web_framework() {
    let config = GatewayRuntimeConfig {
        mode: GatewayMode::Split,
        ..GatewayRuntimeConfig::default()
    };
    let router = finalize_gateway_router(
        &config,
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

    assert_eq!(response.status(), 200);
    assert!(
        response.headers().get("x-request-id").is_some(),
        "framework request identity stage should assign x-request-id"
    );
}

#[tokio::test]
async fn embedded_mode_uses_request_identity_without_full_framework_on_outer_router() {
    let config = GatewayRuntimeConfig {
        mode: GatewayMode::Embedded,
        ..GatewayRuntimeConfig::default()
    };
    let router = finalize_gateway_router(
        &config,
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

    assert_eq!(response.status(), 200);
    assert!(response.headers().get("x-request-id").is_some());
}

#[tokio::test]
async fn api_server_async_finalize_accepts_split_mode_health_checks() {
    let config = GatewayRuntimeConfig {
        mode: GatewayMode::Split,
        ..GatewayRuntimeConfig::default()
    };
    let router = finalize_gateway_router_async(
        &config,
        Router::new().route("/healthz", get(|| async { "ok" })),
    )
    .await;

    let response = router
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn split_mode_with_web_framework_disabled_applies_tower_cors() {
    let _guard =
        WebFrameworkEnvGuard::set("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED", "false");
    assert!(!web_framework_enabled());

    let mut config = GatewayRuntimeConfig {
        mode: GatewayMode::Split,
        ..GatewayRuntimeConfig::default()
    };
    config.cors.allow_any_origin = false;
    config.cors.allowed_origins = vec!["https://console.example.com".to_string()];
    let app = build_sdkwork_api_cloud_gateway_router(config).expect("router builds");

    let allowed_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/healthz")
                .header(header::ORIGIN, "https://console.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed_response.status(), 200);
    assert_eq!(
        allowed_response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("https://console.example.com")
    );

    let blocked_response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/healthz")
                .header(header::ORIGIN, "https://evil.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(blocked_response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_none());
}

#[tokio::test]
async fn split_mode_with_web_framework_disabled_returns_gateway_fallback_for_unknown_routes() {
    let _guard = WebFrameworkEnvGuard::set("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED", "0");
    assert!(!web_framework_enabled());

    let config = GatewayRuntimeConfig {
        mode: GatewayMode::Split,
        ..GatewayRuntimeConfig::default()
    };
    let app = build_sdkwork_api_cloud_gateway_router(config).expect("router builds");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
}

#[tokio::test]
async fn split_mode_with_web_framework_disabled_assigns_uuid_request_id() {
    let _guard =
        WebFrameworkEnvGuard::set("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED", "off");
    assert!(!web_framework_enabled());

    let config = GatewayRuntimeConfig {
        mode: GatewayMode::Split,
        ..GatewayRuntimeConfig::default()
    };
    let app = build_sdkwork_api_cloud_gateway_router(config).expect("router builds");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .expect("disabled mode still assigns x-request-id");
    assert!(
        is_canonical_uuid(request_id),
        "expected UUID v4 request id, got {request_id}"
    );
}

#[tokio::test]
async fn split_mode_with_web_framework_disabled_applies_gateway_secure_headers() {
    let _guard = WebFrameworkEnvGuard::set("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED", "no");
    assert!(!web_framework_enabled());

    let mut config = GatewayRuntimeConfig {
        mode: GatewayMode::Split,
        ..GatewayRuntimeConfig::default()
    };
    config.security.secure_headers_enabled = true;
    let app = build_sdkwork_api_cloud_gateway_router(config).expect("router builds");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        response
            .headers()
            .get("content-security-policy")
            .and_then(|value| value.to_str().ok()),
        Some("default-src 'none'; frame-ancestors 'none'")
    );
}
