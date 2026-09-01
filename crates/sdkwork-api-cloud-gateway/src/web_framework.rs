//! SDKWork web-framework integration for the embedded platform gateway.

use axum::Router;
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_web_axum::with_server_request_identity;
use sdkwork_web_core::CorsPolicy;

pub fn parse_web_framework_toggle(value: &str) -> bool {
    sdkwork_utils_rust::boolean::parse_bool(value).unwrap_or(false)
}

pub fn web_framework_enabled() -> bool {
    match std::env::var("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED") {
        Ok(value) => parse_web_framework_toggle(value.as_str()),
        Err(_) => true,
    }
}

pub fn finalize_gateway_router(_config: &GatewayRuntimeConfig, router: Router) -> Router {
    with_server_request_identity(router)
}

pub async fn finalize_gateway_router_async(
    _config: &GatewayRuntimeConfig,
    router: Router,
) -> Router {
    with_server_request_identity(router)
}

pub(crate) fn gateway_cors_policy(config: &GatewayRuntimeConfig) -> CorsPolicy {
    let mut policy = if matches!(
        config
            .service
            .environment
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "dev" | "development" | "test" | "testing" | "local"
    ) {
        CorsPolicy::development_private_network()
    } else {
        CorsPolicy::default()
    };
    for origin in &config.cors.allowed_origins {
        if !policy.allowed_origins.contains(origin) {
            policy.allowed_origins.push(origin.clone());
        }
    }
    policy.with_registered_sdkwork_client_origins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_api_cloud_gateway_config::{CorsConfig, GatewayRuntimeConfig, GatewayServiceConfig};

    #[test]
    fn gateway_cors_policy_always_allows_registered_desktop_origins() {
        let config = GatewayRuntimeConfig {
            service: GatewayServiceConfig {
                name: "sdkwork-api-cloud-gateway".to_owned(),
                environment: "production".to_owned(),
            },
            cors: CorsConfig {
                allow_any_origin: false,
                allowed_origins: vec!["https://api.sdkwork.com".to_owned()],
            },
            ..GatewayRuntimeConfig::default()
        };
        let policy = gateway_cors_policy(&config);
        policy
            .validate_origin_value("app://dsh")
            .expect("registered desktop origin");
        policy
            .validate_origin_value("https://servicewechat.com")
            .expect("registered mini program origin");
        policy
            .validate_origin_value("https://api.sdkwork.com")
            .expect("configured production origin");
    }

    #[tokio::test]
    async fn gateway_cors_preflight_allows_registered_desktop_origin() {
        use axum::body::Body;
        use axum::http::{header, Method, Request, StatusCode};
        use axum::routing::get;
        use axum::Router;
        use sdkwork_web_axum::cors_layer_from_policy;
        use tower::ServiceExt;

        let config = GatewayRuntimeConfig {
            service: GatewayServiceConfig {
                name: "sdkwork-api-cloud-gateway".to_owned(),
                environment: "production".to_owned(),
            },
            cors: CorsConfig {
                allow_any_origin: false,
                allowed_origins: vec!["https://console.sdkwork.com".to_owned()],
            },
            ..GatewayRuntimeConfig::default()
        };
        let router = Router::new()
            .route(
                "/app/v3/api/auth/sessions/current",
                get(|| async { StatusCode::OK }),
            )
            .layer(cors_layer_from_policy(gateway_cors_policy(&config)));

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/app/v3/api/auth/sessions/current")
                    .header(header::ORIGIN, "app://dsh")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .expect("preflight request"),
            )
            .await
            .expect("preflight response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
            Some("app://dsh")
        );
    }
}
