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
    policy = policy.with_registered_sdkwork_client_origins();
    // The registered console host pattern is validated by the config layer, so a
    // failure here is unreachable; degrade to the exact-origin policy rather
    // than panicking inside request-path assembly.
    if let Some(hosts) = config.cors.registered_console_hosts.clone() {
        match policy.clone().with_registered_console_hosts(hosts) {
            Ok(configured) => policy = configured,
            Err(error) => {
                tracing::error!(error = %error, "registered console host pattern rejected; falling back to exact origins");
            }
        }
    }
    policy
}

/// Returns whether `origin` is a trusted first-party browser origin for this
/// gateway beyond the exact allowlist and the console-host pattern.
///
/// This is the opt-in `SDKWORK_CORS_SUBDOMAIN_BASE_DOMAINS` rule
/// (`CORS_SPEC.md` §5.1): a deployment may declare whole registered base domains
/// trusted, which is how operator-chosen hosts (`platform.birdcoder.com`), the
/// apex (`sdkwork.com`) and deeper names (`region.console.sdkwork.com`) work
/// without enumerating them. Base domains stay restricted to the registered
/// platform registry by the config layer, so this never widens to a third-party
/// domain.
pub(crate) fn gateway_allows_subdomain_origin(config: &GatewayRuntimeConfig, origin: &str) -> bool {
    config
        .cors
        .registered_subdomain_hosts
        .as_ref()
        .is_some_and(|hosts| hosts.matches(origin))
}

/// Builds the effective CORS policy for one request origin.
///
/// The framework policy type carries exact origins only; the sub-domain rule is
/// evaluated here and, on a match, the concrete origin is admitted to a copy of
/// the policy so the shared response-header writer (`Access-Control-Allow-*`,
/// `Vary: Origin`) echoes it exactly instead of emitting `*` with credentials.
pub(crate) fn gateway_cors_policy_for_origin(
    config: &GatewayRuntimeConfig,
    origin: Option<&str>,
) -> CorsPolicy {
    let mut policy = gateway_cors_policy(config);
    if let Some(origin) = origin.map(str::trim).filter(|value| !value.is_empty()) {
        if !policy
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin)
            && gateway_allows_subdomain_origin(config, origin)
        {
            policy.allowed_origins.push(origin.to_owned());
        }
    }
    policy
}

/// Builds the gateway CORS layer.
///
/// Same-origin requests (`Origin` authority == request `Host`) are always
/// accepted for dependency surfaces that declare `sameOriginAllowed`, on top of
/// the canonical policy (development private-network semantics + the
/// `SDKWORK_CORS_ALLOWED_ORIGINS` allowlist + the registered console host
/// pattern + the opt-in sub-domain rule). The predicate accepts the canonical
/// policy origins first and only then widens to the declared extras, so
/// first-party browser flows keep working without weakening the configured
/// cross-origin policy for anyone else.
pub(crate) fn gateway_cors_layer(config: &GatewayRuntimeConfig) -> tower_http::cors::CorsLayer {
    let policy = gateway_cors_policy(config);
    let mut layer = sdkwork_web_axum::cors_layer_from_policy(policy.clone());
    let same_origin_allowed = config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.same_origin_allowed);
    let subdomain_hosts = config.cors.registered_subdomain_hosts.clone();
    if same_origin_allowed || subdomain_hosts.is_some() {
        layer = layer.allow_origin(tower_http::cors::AllowOrigin::predicate(
            move |origin, request_parts| {
                let origin = origin.to_str().unwrap_or_default();
                policy.allows_origin_value(origin)
                    || subdomain_hosts
                        .as_ref()
                        .is_some_and(|hosts| hosts.matches(origin))
                    || (same_origin_allowed
                        && request_parts
                            .headers
                            .get(axum::http::header::HOST)
                            .and_then(|host| host.to_str().ok())
                            .map(|host| same_origin(origin, host))
                            .unwrap_or(false))
            },
        ));
    }
    layer
}

/// Compare an Origin authority with the request Host (case-insensitive,
/// port-sensitive like the browser's same-origin policy).
fn same_origin(origin: &str, host: &str) -> bool {
    let origin_host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .and_then(|authority| authority.split_once('/').map(|(host, _)| host))
        .unwrap_or(origin);
    origin_host.eq_ignore_ascii_case(host.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_api_cloud_gateway_config::{
        CorsConfig, GatewayRuntimeConfig, GatewayServiceConfig,
    };
    use sdkwork_web_core::RegisteredSubdomainHosts;

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
                registered_console_hosts: None,
                registered_subdomain_hosts: None,
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
                registered_console_hosts: None,
                registered_subdomain_hosts: None,
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

    fn subdomain_config() -> GatewayRuntimeConfig {
        GatewayRuntimeConfig {
            service: GatewayServiceConfig {
                name: "sdkwork-api-cloud-gateway".to_owned(),
                environment: "production".to_owned(),
            },
            cors: CorsConfig {
                allow_any_origin: false,
                allowed_origins: vec!["http://localhost:3913".to_owned()],
                registered_console_hosts: None,
                registered_subdomain_hosts: Some(RegisteredSubdomainHosts::new(
                    vec![
                        "sdkwork.com".to_owned(),
                        "birdcoder.com".to_owned(),
                        "sdkwork.cn".to_owned(),
                        "birdcoder.cn".to_owned(),
                    ],
                    vec!["http".to_owned(), "https".to_owned()],
                )),
            },
            ..GatewayRuntimeConfig::default()
        }
    }

    #[test]
    fn subdomain_rule_accepts_operator_hosts_and_rejects_others() {
        let config = subdomain_config();
        assert!(gateway_allows_subdomain_origin(
            &config,
            "https://platform.birdcoder.com"
        ));
        assert!(gateway_allows_subdomain_origin(
            &config,
            "https://a.b.sdkwork.com"
        ));
        assert!(gateway_allows_subdomain_origin(
            &config,
            "https://sdkwork.com"
        ));
        assert!(!gateway_allows_subdomain_origin(
            &config,
            "https://evil.example.com"
        ));
        assert!(!gateway_allows_subdomain_origin(
            &config,
            "https://sdkwork.com.attacker.example"
        ));
    }

    #[test]
    fn per_origin_policy_echoes_the_matched_subdomain_origin() {
        let config = subdomain_config();
        let policy = gateway_cors_policy_for_origin(&config, Some("https://a.b.sdkwork.com"));
        assert!(policy.allows_origin_value("https://a.b.sdkwork.com"));
        assert!(!policy.allows_origin_value("https://other.sdkwork.com"));

        let unmatched = gateway_cors_policy_for_origin(&config, Some("https://evil.example.com"));
        assert!(!unmatched.allows_origin_value("https://evil.example.com"));
    }

    #[tokio::test]
    async fn gateway_cors_preflight_allows_subdomain_origin_and_conditional_headers() {
        use axum::body::Body;
        use axum::http::{header, Method, Request, StatusCode};
        use axum::routing::put;
        use axum::Router;
        use tower::ServiceExt;

        let config = subdomain_config();
        let router = Router::new()
            .route(
                "/backend/v3/api/ai/upstream_accounts/1",
                put(|| async { StatusCode::OK }),
            )
            .layer(gateway_cors_layer(&config));

        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/backend/v3/api/ai/upstream_accounts/1")
                    .header(header::ORIGIN, "https://platform.birdcoder.com")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "PUT")
                    .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "if-match")
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
            Some("https://platform.birdcoder.com")
        );
        let allowed_headers = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(
            allowed_headers
                .split(',')
                .any(|value| value.trim() == "if-match"),
            "preflight must allow if-match, got: {allowed_headers}"
        );
    }
}
