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

pub fn finalize_gateway_router(config: &GatewayRuntimeConfig, router: Router) -> Router {
    attach_vendor_credential_bridge(config, with_server_request_identity(router))
}

pub async fn finalize_gateway_router_async(
    config: &GatewayRuntimeConfig,
    router: Router,
) -> Router {
    attach_vendor_credential_bridge(config, with_server_request_identity(router))
}

/// Presents a vendor-compatible surface's published bearer credential as the
/// canonical `X-API-Key` credential, before the Web Framework pipeline runs.
///
/// OpenAI/Anthropic-compatible operations publish `Authorization: Bearer <key>`
/// (`API_SPEC.md` §4.5.2); the profile that serves them reads `X-API-Key`. The
/// bridge keeps the published wire usable without relaxing that profile, and it
/// is scoped to the composed dependency prefixes that actually speak that wire.
///
/// Both assembled outer routers need this layer: the registry-only assemblies go
/// through [`finalize_gateway_router`], while the embedded platform composition
/// assembles its router directly and applies the bridge itself. Without it a
/// vendor client's single bearer is rejected as an incomplete credential profile
/// before any credential channel runs.
pub(crate) fn attach_vendor_credential_bridge(
    config: &GatewayRuntimeConfig,
    router: Router,
) -> Router {
    let prefixes = std::sync::Arc::new(crate::vendor_open_api_auth::vendor_open_api_prefixes(
        config,
    ));
    if prefixes.is_empty() {
        return router;
    }
    router.layer(axum::middleware::from_fn(
        move |mut request: axum::extract::Request, next: axum::middleware::Next| {
            let prefixes = std::sync::Arc::clone(&prefixes);
            async move {
                if crate::vendor_open_api_auth::is_vendor_open_api_path(
                    request.uri().path(),
                    prefixes.as_slice(),
                ) {
                    crate::vendor_open_api_auth::bridge_vendor_authorization(request.headers_mut());
                }
                next.run(request).await
            }
        },
    ))
}

/// Builds the platform CORS settings contribution.
///
/// The contribution exposes `GET`/`PATCH /backend/v3/api/system/cors/settings`
/// through the composed pipeline (dual token + resolved tenant). The handler
/// writes the platform-scoped `framework_cors_policy` row, whose `version`
/// bump is the change signal the edge's periodic scan consumes, so an operator
/// save applies on every process without a restart or deploy.
#[cfg(feature = "foundation-appbase")]
pub(crate) fn cors_admin_contribution(
    pool: sqlx::PgPool,
    config: &GatewayRuntimeConfig,
) -> Result<sdkwork_web_bootstrap::ApiAssemblyContribution, crate::GatewayRuntimeError> {
    let router = crate::cors_admin::cors_admin_router(pool, config);
    let manifest =
        sdkwork_web_core::HttpRouteManifest::new(crate::cors_admin::CORS_SETTINGS_ROUTES);
    sdkwork_web_bootstrap::ApiAssemblyContribution::from_manifest(
        "sdkwork-platform-cors-admin",
        "SDKWork Platform CORS Administration",
        router,
        manifest,
        Vec::new(),
        std::sync::Arc::new(sdkwork_web_bootstrap::AlwaysReady),
    )
    .map_err(|message| crate::GatewayRuntimeError::ApiAssemblyComposition { message })
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
    // CORS_SPEC §5.1: the sub-domain rule lives on the policy itself (not only
    // in this module) so the edge layer, the response-header writer and the
    // embedded framework pipeline all evaluate the same allowlist. Without this
    // the edge admitted `https://mall.sdkwork.com` while the pipeline rejected
    // the same origin with 40301 `failedStage: cors`.
    if let Some(hosts) = config.cors.registered_subdomain_hosts.clone() {
        match policy.clone().with_registered_subdomain_hosts(hosts) {
            Ok(configured) => policy = configured,
            Err(error) => {
                tracing::error!(error = %error, "registered sub-domain host pattern rejected; falling back to exact origins");
            }
        }
    }
    policy
}

/// Builds the gateway CORS layer over the static configuration floor.
///
/// Production runtime uses [`gateway_cors_layer_with_policy`], which consults
/// the live (static floor + admin-managed) snapshot; this helper is the
/// static-only form used by tests and by assemblies without a database.
#[cfg(test)]
pub(crate) fn gateway_cors_layer(config: &GatewayRuntimeConfig) -> tower_http::cors::CorsLayer {
    let live = crate::cors_dynamic::GatewayCorsPolicy::new(gateway_cors_policy(config));
    gateway_cors_layer_with_policy(config, live)
}

/// Builds the gateway CORS layer over a live policy snapshot.
///
/// Same-origin requests (`Origin` authority == request `Host`) are always
/// accepted for dependency surfaces that declare `sameOriginAllowed`, on top of
/// the canonical policy (development private-network semantics + the
/// `SDKWORK_CORS_ALLOWED_ORIGINS` allowlist + the registered console host
/// pattern + the registered sub-domain rule + admin-managed origins). The
/// predicate reads one immutable snapshot per preflight, so an operator change
/// is visible on the next scan without restarting the process or the layer.
pub(crate) fn gateway_cors_layer_with_policy(
    config: &GatewayRuntimeConfig,
    live: std::sync::Arc<crate::cors_dynamic::GatewayCorsPolicy>,
) -> tower_http::cors::CorsLayer {
    let mut layer = sdkwork_web_axum::cors_layer_from_policy(live.snapshot().policy().clone());
    let same_origin_allowed = config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.same_origin_allowed);
    layer = layer.allow_origin(tower_http::cors::AllowOrigin::predicate(
        move |origin, request_parts| {
            let origin = origin.to_str().unwrap_or_default();
            live.allows_origin(origin)
                || (same_origin_allowed
                    && request_parts
                        .headers
                        .get(axum::http::header::HOST)
                        .and_then(|host| host.to_str().ok())
                        .map(|host| same_origin(origin, host))
                        .unwrap_or(false))
        },
    ));
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
                ..CorsConfig::default()
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
                ..CorsConfig::default()
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
                ..CorsConfig::default()
            },
            ..GatewayRuntimeConfig::default()
        }
    }

    #[test]
    fn subdomain_rule_accepts_operator_hosts_and_rejects_others() {
        // The rule is attached to the policy itself, so the edge layer, the
        // response-header writer and the embedded pipeline share one answer.
        let policy = gateway_cors_policy(&subdomain_config());
        assert!(policy.allows_origin_value("https://platform.birdcoder.com"));
        assert!(policy.allows_origin_value("https://a.b.sdkwork.com"));
        assert!(policy.allows_origin_value("https://region.console.eu.sdkwork.com"));
        assert!(policy.allows_origin_value("https://sdkwork.com"));
        assert!(!policy.allows_origin_value("https://evil.example.com"));
        assert!(!policy.allows_origin_value("https://sdkwork.com.attacker.example"));
    }

    #[test]
    fn live_policy_admits_subdomain_origins_without_enumerating_them() {
        let config = subdomain_config();
        let live = crate::cors_dynamic::GatewayCorsPolicy::new(gateway_cors_policy(&config));
        assert!(live.allows_origin("https://a.b.sdkwork.com"));
        assert!(live.allows_origin("https://x.y.z.birdcoder.cn"));
        assert!(!live.allows_origin("https://other.example.com"));
        // Static floor only: no dynamic overrides in this snapshot.
        assert!(!live.snapshot().has_overrides());
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

    #[tokio::test]
    async fn gateway_cors_preflight_allows_second_third_and_fourth_level_wildcard_origins() {
        use axum::body::Body;
        use axum::http::{header, Method, Request, StatusCode};
        use axum::routing::get;
        use axum::Router;
        use tower::ServiceExt;

        // One allowlist line covers the whole first-party sub-domain space:
        // `https://**.sdkwork.com` is the production-safe form an operator (or
        // the admin console) adds without enumerating every host.
        let mut config = subdomain_config();
        config.cors.registered_subdomain_hosts = None;
        config.cors.allowed_origins = vec![
            "https://api.sdkwork.com".to_owned(),
            "https://**.sdkwork.com".to_owned(),
        ];
        let live = crate::cors_dynamic::GatewayCorsPolicy::new(gateway_cors_policy(&config));
        let router = Router::new()
            .route("/app/v3/api/catalog/spus", get(|| async { StatusCode::OK }))
            .layer(gateway_cors_layer_with_policy(&config, live));

        for origin in [
            "https://mall.sdkwork.com",
            "https://admin.region.sdkwork.com",
            "https://x.y.z.sdkwork.com",
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::OPTIONS)
                        .uri("/app/v3/api/catalog/spus")
                        .header(header::ORIGIN, origin)
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
                Some(origin),
                "preflight must echo `{origin}`"
            );
        }

        let rejected = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/app/v3/api/catalog/spus")
                    .header(header::ORIGIN, "https://evil-sdkwork.com")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .expect("preflight request"),
            )
            .await
            .expect("preflight response");
        assert!(rejected
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }

    #[cfg(feature = "foundation-appbase")]
    #[tokio::test]
    async fn cors_admin_contribution_composes_the_settings_routes() {
        // Composition proof for the console-managed CORS surface: the
        // contribution must carry the dual-token GET/PATCH routes and a valid
        // OpenAPI document so the composed pipeline can authorize the call.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://sdkwork@127.0.0.1:5432/sdkwork")
            .expect("lazy pool");
        let config = subdomain_config();
        let contribution = cors_admin_contribution(pool, &config).expect("contribution");
        let mut registry = sdkwork_web_bootstrap::ApiModuleRegistry::new();
        registry.add_module(contribution);
        let composed = registry
            .try_compose("SDKWork Platform API")
            .expect("compose CORS settings module");
        let operations = composed
            .route_manifest
            .routes()
            .iter()
            .filter(|route| route.path.ends_with("/system/cors/settings"))
            .map(|route| route.operation_id)
            .collect::<Vec<_>>();
        assert_eq!(
            vec![
                "system.cors.settings.retrieve",
                "system.cors.settings.update"
            ],
            operations
        );
        assert!(!composed.openapi["paths"]
            .as_object()
            .expect("openapi paths")
            .is_empty());
    }
}
