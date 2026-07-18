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
    policy
}
