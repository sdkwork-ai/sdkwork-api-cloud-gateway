//! SDKWork web-framework integration for gateway-owned and split-mode proxy surfaces.

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::Response;
use axum::Router;
use sdkwork_api_cloud_gateway_config::{
    normalize_gateway_route_path, GatewayMode, GatewayRuntimeConfig,
};
use sdkwork_web_axum::{with_server_request_identity, with_web_request_context, WebFrameworkLayer};
use sdkwork_web_contract::{HttpMethod, HttpRoute, RouteAuth};
use sdkwork_web_core::{
    CorsPolicy, DefaultWebRequestContextResolver, HeaderSecurityPolicy, HttpRouteManifest,
    RequestSizeLimitPolicy, SecurityPolicy, WebApiSurface, WebCallInterceptor,
    WebCallInterceptorChain, WebCallRuntime, WebCallStage, WebCallState, WebFrameworkError,
    WebRequestContextProfile, WebRequestContextResolver,
};
use sdkwork_web_core::{StandardWebCallInterceptor, StandardWebCallInterceptorKind};

const GATEWAY_ROUTE_MANIFEST: &[HttpRoute] = &[
    HttpRoute::new(
        HttpMethod::Get,
        "/healthz",
        "Gateway",
        "getGatewayHealth",
        RouteAuth::Public,
    ),
    HttpRoute::new(
        HttpMethod::Get,
        "/readyz",
        "Gateway",
        "getGatewayReadiness",
        RouteAuth::Public,
    ),
    HttpRoute::new(
        HttpMethod::Get,
        "/openapi/index.json",
        "Gateway",
        "listGatewayApiDependencies",
        RouteAuth::Public,
    ),
    HttpRoute::new(
        HttpMethod::Get,
        "/openapi.json",
        "Gateway",
        "getGatewayOpenApiDocument",
        RouteAuth::Public,
    ),
];

/// Parses `SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED` style toggle values.
pub fn parse_web_framework_toggle(value: &str) -> bool {
    sdkwork_utils_rust::boolean::parse_bool(value).unwrap_or(false)
}

/// Whether the gateway outer router should mount the sdkwork-web-framework pipeline.
pub fn web_framework_enabled() -> bool {
    match std::env::var("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED") {
        Ok(value) => parse_web_framework_toggle(value.as_str()),
        Err(_) => true,
    }
}

/// Whether split mode should rely on the outer web-framework pipeline instead of legacy inner layers.
pub(crate) fn split_mode_web_framework_active(config: &GatewayRuntimeConfig) -> bool {
    config.mode == GatewayMode::Split && web_framework_enabled()
}

/// Accepts browser CORS preflight requests so the outer framework pipeline can attach CORS headers.
pub(crate) fn with_preflight_options_support<S>(router: Router<S>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(from_fn(preflight_options_middleware))
}

async fn preflight_options_middleware(request: axum::extract::Request, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .expect("empty preflight response builds");
    }
    next.run(request).await
}

/// Applies sdkwork-web-framework bootstrap to the gateway router.
///
/// Split mode runs the standard interceptor chain before upstream proxy dispatch.
/// Embedded mode only assigns request identity on the outer router because embedded
/// dependency routers already mount their own WebFramework layers.
pub fn finalize_gateway_router(config: &GatewayRuntimeConfig, router: Router) -> Router {
    if !web_framework_enabled() {
        return with_server_request_identity(router);
    }

    match config.mode {
        GatewayMode::Split => wrap_split_router(config, router),
        GatewayMode::Embedded => with_server_request_identity(router),
    }
}

/// Async finalize that prefers IAM database-backed credential resolution when available.
pub async fn finalize_gateway_router_async(
    config: &GatewayRuntimeConfig,
    router: Router,
) -> Router {
    if !web_framework_enabled() {
        return with_server_request_identity(router);
    }

    match config.mode {
        GatewayMode::Split => wrap_split_router_async(config, router).await,
        GatewayMode::Embedded => with_server_request_identity(router),
    }
}

fn wrap_split_router(config: &GatewayRuntimeConfig, router: Router) -> Router {
    let layer =
        build_split_web_framework_layer(config, DefaultWebRequestContextResolver::default());
    with_web_request_context(router, layer)
}

async fn wrap_split_router_async(config: &GatewayRuntimeConfig, router: Router) -> Router {
    #[cfg(feature = "foundation-appbase")]
    {
        let resolver = sdkwork_iam_web_adapter::iam_web_request_context_resolver_from_env().await;
        let layer = build_split_web_framework_layer(config, resolver);
        with_web_request_context(router, layer)
    }

    #[cfg(not(feature = "foundation-appbase"))]
    {
        wrap_split_router(config, router)
    }
}

fn build_split_web_framework_layer<R>(
    config: &GatewayRuntimeConfig,
    resolver: R,
) -> WebFrameworkLayer<R>
where
    R: WebRequestContextResolver + Clone,
{
    WebFrameworkLayer::new(resolver)
        .with_profile(split_proxy_profile(config))
        .with_route_manifest(HttpRouteManifest::new(GATEWAY_ROUTE_MANIFEST))
        .with_security_policy(split_security_policy(config))
        .with_call_chain(split_proxy_call_chain())
}

fn split_security_policy(config: &GatewayRuntimeConfig) -> SecurityPolicy {
    SecurityPolicy {
        cors: CorsPolicy {
            allow_all_origins: config.cors.allow_any_origin,
            allowed_origins: config.cors.allowed_origins.clone(),
            ..CorsPolicy::default()
        },
        request_size_limit: RequestSizeLimitPolicy {
            max_content_length: Some(config.request.max_body_bytes as u64),
        },
        header_security: if config.security.secure_headers_enabled {
            HeaderSecurityPolicy {
                content_type_options: true,
                frame_options_deny: true,
                referrer_policy: Some("no-referrer".to_owned()),
                permissions_policy: Some("camera=(), microphone=(), geolocation=()".to_owned()),
                strict_transport_security: Some("max-age=31536000; includeSubDomains".to_owned()),
            }
        } else {
            HeaderSecurityPolicy {
                content_type_options: false,
                frame_options_deny: false,
                referrer_policy: None,
                permissions_policy: None,
                strict_transport_security: None,
            }
        },
        ..SecurityPolicy::default()
    }
}

/// Split-mode proxy chain: standard 18-stage pipeline plus gateway unknown-surface bypass.
pub fn split_proxy_call_chain<R>() -> WebCallInterceptorChain<R>
where
    R: WebRequestContextResolver + Clone,
{
    use StandardWebCallInterceptorKind as Stage;

    WebCallInterceptorChain::new()
        .with_interceptor(StandardWebCallInterceptor::new(Stage::RequestIdentity))
        .with_interceptor(StandardWebCallInterceptor::new(
            Stage::SurfaceClassification,
        ))
        .with_interceptor(GatewayUnknownSurfacePublicInterceptor)
        .with_interceptor(StandardWebCallInterceptor::new(Stage::Cors))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::MethodGuard))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::CrossSiteRequest))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::SqlInjectionGuard))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::RequestSizeLimit))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::RateLimit))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::Idempotency))
        .with_interceptor(StandardWebCallInterceptor::new(
            Stage::RequestContextResolution,
        ))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::Authentication))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::Authorization))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::TenantIsolation))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::ContextInjection))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::Logging))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::Audit))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::HeaderSecurity))
        .with_interceptor(StandardWebCallInterceptor::new(Stage::ResponseIdentity))
}

/// Transparent split proxy defers auth to upstream dependency APIs. Unclassified paths must still
/// reach gateway fallback handlers (for example hidden diagnostics and route-not-found problems).
#[derive(Clone, Copy, Debug, Default)]
struct GatewayUnknownSurfacePublicInterceptor;

#[async_trait]
impl<R> WebCallInterceptor<R> for GatewayUnknownSurfacePublicInterceptor
where
    R: WebRequestContextResolver + Clone,
{
    fn name(&self) -> &'static str {
        "gateway_unknown_surface_public"
    }

    fn stage(&self) -> WebCallStage {
        WebCallStage::SurfaceClassification
    }

    async fn before(
        &self,
        state: &mut WebCallState,
        _request: &mut axum::extract::Request,
        _runtime: &WebCallRuntime<R>,
    ) -> Result<(), WebFrameworkError> {
        if state.api_surface == WebApiSurface::Unknown {
            state.public_path = true;
        }
        Ok(())
    }
}

fn split_proxy_profile(config: &GatewayRuntimeConfig) -> WebRequestContextProfile {
    let mut public_path_prefixes = vec![
        "/healthz".to_owned(),
        "/readyz".to_owned(),
        "/openapi.json".to_owned(),
        "/openapi/".to_owned(),
    ];

    if config.observability.metrics_enabled {
        let metrics_path = config.observability.metrics_path.trim();
        if !metrics_path.is_empty() {
            public_path_prefixes.push(normalize_gateway_route_path(metrics_path, "/metrics"));
        }
    }

    if config.diagnostics.enabled {
        let diagnostics_path = config.diagnostics.path.trim();
        if !diagnostics_path.is_empty() {
            public_path_prefixes.push(normalize_gateway_route_path(
                diagnostics_path,
                "/internal/gateway/diagnostics",
            ));
        }
    }

    let mut open_api_prefixes = Vec::new();
    for surface in config.dependency_surfaces.iter() {
        let prefix = surface.api_prefix.trim();
        if prefix.is_empty() {
            continue;
        }
        let normalized = normalize_gateway_route_path(prefix, prefix);
        if surface.surface == "open"
            && !open_api_prefixes
                .iter()
                .any(|existing| existing == &normalized)
        {
            open_api_prefixes.push(normalized.clone());
        }
        if !public_path_prefixes
            .iter()
            .any(|existing| existing == &normalized)
        {
            public_path_prefixes.push(normalized);
        }
    }

    if open_api_prefixes.is_empty() {
        open_api_prefixes.push("/open/v3/api".to_owned());
    }

    WebRequestContextProfile {
        open_api_prefixes,
        public_path_prefixes,
        gateway_api_prefixes: vec!["/v1".to_owned()],
        ..WebRequestContextProfile::default()
    }
}
