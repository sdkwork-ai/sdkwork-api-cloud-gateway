use std::{
    collections::HashMap,
    error::Error as StdError,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Router,
};
use http_body_util::LengthLimitError;
#[cfg(feature = "foundation-appbase")]
use sdkwork_api_cloud_gateway_config::GatewayMode;
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_api_cloud_gateway_observability::{
    build_gateway_access_log_event, build_gateway_audit_log_event, describe_gateway_startup,
    GatewayAccessLogInput, GatewayAuditLogInput,
};
use sdkwork_api_cloud_gateway_registry::{
    GatewayRouteRegistry, RouteDescriptor, RouteExecutionTarget, RouteProtocol, RouteVisibility,
};
use sdkwork_web_bootstrap::{ApiAssemblyContribution, ComposedApiAssembly};
use sdkwork_web_contract::{ApiSurface, HttpMethod};
use sdkwork_web_core::new_request_id;
use serde_json::json;
use tower::ServiceExt;
use tracing::info;
use tracing::Instrument;

use crate::{
    diagnostics, health,
    metrics::{escape_label_value, GatewayMetrics},
    openapi, policy, security, traffic, web_framework, GatewayRuntimeError,
};

#[derive(Clone, Copy, Debug)]
struct GatewayFailureClass(&'static str);

struct GatewayAuditLogEmitInput<'a> {
    category: &'a str,
    action: &'a str,
    reason: &'a str,
    scope: Option<&'a str>,
    status: StatusCode,
    method: &'a str,
    path: &'a str,
    headers: &'a HeaderMap,
}

#[derive(Clone)]
pub struct GatewayRuntime {
    config: GatewayRuntimeConfig,
    registry: GatewayRouteRegistry,
    embedded_routers: HashMap<String, Router>,
    composed_router: Option<Router>,
}

#[derive(Clone)]
pub(crate) struct GatewayState {
    pub(crate) config: GatewayRuntimeConfig,
    pub(crate) registry: GatewayRouteRegistry,
    pub(crate) embedded_routers: HashMap<String, Router>,
    pub(crate) composed_router: Option<Router>,
    pub(crate) metrics: GatewayMetrics,
    pub(crate) traffic: traffic::GatewayTrafficState,
}

impl GatewayRuntime {
    pub fn new(config: GatewayRuntimeConfig) -> Result<Self, GatewayRuntimeError> {
        let registry = registry_for_config(&config);
        Self::with_registry_and_embedded_routers(config, registry, HashMap::new())
    }

    pub fn with_registry(
        config: GatewayRuntimeConfig,
        registry: GatewayRouteRegistry,
    ) -> Result<Self, GatewayRuntimeError> {
        Self::with_registry_and_embedded_routers(config, registry, HashMap::new())
    }

    pub fn with_registry_and_embedded_routers(
        config: GatewayRuntimeConfig,
        registry: GatewayRouteRegistry,
        embedded_routers: HashMap<String, Router>,
    ) -> Result<Self, GatewayRuntimeError> {
        validate_preflight(&config, &embedded_routers, false)?;

        Ok(Self {
            config,
            registry,
            embedded_routers,
            composed_router: None,
        })
    }

    fn with_registry_and_composed_router(
        config: GatewayRuntimeConfig,
        registry: GatewayRouteRegistry,
        composed_router: Router,
    ) -> Result<Self, GatewayRuntimeError> {
        validate_preflight(&config, &HashMap::new(), true)?;
        Ok(Self {
            config,
            registry,
            embedded_routers: HashMap::new(),
            composed_router: Some(composed_router),
        })
    }

    pub fn router(self) -> Router {
        let config = self.config.clone();
        web_framework::finalize_gateway_router(&config, self.assemble_router())
    }

    fn assemble_router(self) -> Router {
        let summary = describe_gateway_startup(&self.config, &self.registry);
        info!(
            service_name = %summary.service_name,
            bind = %summary.bind,
            mode = ?summary.mode,
            route_count = summary.route_count,
            "starting sdkwork api gateway"
        );

        let uses_composed_host = self.composed_router.is_some();
        let cors = cors_layer(&self.config);
        let metrics_enabled = self.config.observability.metrics_enabled;
        let metrics_path = metrics_route_path(self.config.observability.metrics_path.as_str());
        let diagnostics_enabled = self.config.diagnostics.enabled;
        let diagnostics_path = gateway_route_path(
            self.config.diagnostics.path.as_str(),
            "/internal/gateway/diagnostics",
        );
        let trusted_proxy_cidrs = self.config.security.trusted_proxy_cidrs.clone();
        let max_body_bytes = self.config.request.max_body_bytes;
        let traffic = traffic::GatewayTrafficState::from_config(
            &trusted_proxy_cidrs,
            self.config.traffic.redis_url.clone(),
            self.config.traffic.redis_key_prefix.clone(),
        );
        let state = Arc::new(GatewayState {
            config: self.config,
            registry: self.registry,
            embedded_routers: self.embedded_routers,
            composed_router: self.composed_router,
            metrics: GatewayMetrics::default(),
            traffic,
        });

        let mut router = Router::new()
            .route("/openapi/index.json", get(openapi::openapi_index))
            .fallback(gateway_fallback);
        if !uses_composed_host {
            router = router
                .route("/healthz", get(health::healthz))
                .route("/livez", get(health::livez))
                .route("/readyz", get(health::readyz))
                .route("/openapi.json", get(openapi::openapi_document));
        }
        if metrics_enabled && !uses_composed_host {
            router = router.route(metrics_path.as_str(), get(metrics_endpoint));
        }
        if diagnostics_enabled {
            router = router.route(
                diagnostics_path.as_str(),
                get(diagnostics::diagnostics_endpoint),
            );
        }

        let mut router = router
            .layer(middleware::from_fn_with_state(
                state.clone(),
                traffic_middleware,
            ))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                security_middleware,
            ))
            // Streaming request body limit: rejects oversized requests from
            // Content-Length and truncates the stream without buffering the
            // whole body, so large uploads never allocate O(max_body_bytes)
            // per in-flight request.
            .layer(tower_http::limit::RequestBodyLimitLayer::new(
                max_body_bytes,
            ))
            .with_state(state);

        router = router.layer(cors);

        router
    }
}

impl fmt::Debug for GatewayRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewayRuntime")
            .field("service", &self.config.service.name)
            .field("mode", &self.config.mode)
            .field(
                "embedded_routers",
                &self.embedded_routers.keys().collect::<Vec<_>>(),
            )
            .field("composed_router", &self.composed_router.is_some())
            .finish()
    }
}

pub async fn build_sdkwork_api_cloud_gateway_router(
    config: GatewayRuntimeConfig,
) -> Result<Router, GatewayRuntimeError> {
    #[cfg(not(feature = "foundation-appbase"))]
    {
        let _ = config;
        return Err(GatewayRuntimeError::ApiAssemblyComposition {
            message: "foundation-appbase is required to host authenticated platform APIs"
                .to_owned(),
        });
    }

    #[cfg(feature = "foundation-appbase")]
    {
        let pool = crate::embedded_database_bootstrap::bootstrap_embedded_databases(&config)
            .await?
            .ok_or_else(|| GatewayRuntimeError::DatabasePoolBootstrap {
                message: "the embedded cloud gateway requires a process-shared PostgreSQL pool"
                    .to_owned(),
            })?;
        let dependency_bootstrap =
            crate::embedded_dependency_routes::bootstrap_embedded_dependency_contributions(
                &config, &pool,
            )
            .await?;
        let registry =
            exact_registry_for_contributions(&config, &dependency_bootstrap.contributions)?;
        let composed = ComposedApiAssembly::try_compose(
            "SDKWork Platform API",
            dependency_bootstrap.contributions,
        )
        .map_err(|message| GatewayRuntimeError::ApiAssemblyComposition { message })?;
        let framework_manifest = composed.route_manifest.clone();
        let resolver =
            sdkwork_iam_web_adapter::IamWebRequestContextResolver::from_database_pool(Some(pool));
        let framework = sdkwork_iam_web_adapter::build_web_framework_builder(
            resolver,
            framework_manifest,
            vec!["/openapi/index.json".to_owned()],
        );
        let hosted = composed.into_hosted(framework);
        let router = hosted
            .router
            .layer(Extension(Arc::new(dependency_bootstrap.runtime)));
        GatewayRuntime::with_registry_and_composed_router(config, registry, router)
            .map(GatewayRuntime::assemble_router)
    }
}

pub fn build_sdkwork_api_cloud_gateway_router_with_registry(
    config: GatewayRuntimeConfig,
    registry: GatewayRouteRegistry,
) -> Result<Router, GatewayRuntimeError> {
    GatewayRuntime::with_registry(config, registry).map(GatewayRuntime::router)
}

pub fn build_sdkwork_api_cloud_gateway_router_with_embedded_routers<I, K>(
    config: GatewayRuntimeConfig,
    embedded_routers: I,
) -> Result<Router, GatewayRuntimeError>
where
    I: IntoIterator<Item = (K, Router)>,
    K: Into<String>,
{
    let embedded_routers = embedded_routers
        .into_iter()
        .map(|(service_id, router)| (service_id.into(), router))
        .collect::<HashMap<_, _>>();
    let registry = registry_for_config(&config);
    GatewayRuntime::with_registry_and_embedded_routers(config, registry, embedded_routers)
        .map(GatewayRuntime::router)
}

#[cfg(feature = "foundation-appbase")]
pub async fn build_sdkwork_api_cloud_gateway_router_with_embedded_appbase(
    mut config: GatewayRuntimeConfig,
) -> Result<Router, GatewayRuntimeError> {
    config.mode = GatewayMode::Embedded;
    build_sdkwork_api_cloud_gateway_router(config).await
}

async fn metrics_endpoint(State(state): State<Arc<GatewayState>>) -> Response {
    let mut body = state.metrics.render(state.config.service.name.as_str());
    body.push_str(
        "# HELP sdkwork_api_cloud_gateway_rate_limit_redis_fallbacks_total Rate-limit decisions that fell back to in-process buckets because the Redis backend was unreachable.\n",
    );
    body.push_str("# TYPE sdkwork_api_cloud_gateway_rate_limit_redis_fallbacks_total counter\n");
    body.push_str(&format!(
        "sdkwork_api_cloud_gateway_rate_limit_redis_fallbacks_total{{service=\"{}\"}} {}\n",
        escape_label_value(state.config.service.name.as_str()),
        state.traffic.redis_fallback_count()
    ));
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn apply_configured_secure_response_headers(state: &GatewayState, response: Response) -> Response {
    security::apply_secure_response_headers(&state.config.security, false, response)
}

async fn security_middleware(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let headers = request.headers().clone();
    let inspection = match security::validate_request(&state.config.security, &request) {
        Ok(inspection) => inspection,
        Err(rejection) => {
            for event in rejection.events() {
                state.metrics.record_security_event(
                    state.config.service.name.as_str(),
                    event.kind,
                    event.action,
                    event.rule,
                );
            }
            if rejection.events().is_empty() {
                emit_audit_log(
                    state.as_ref(),
                    GatewayAuditLogEmitInput {
                        category: "security",
                        action: "reject",
                        reason: rejection.title,
                        scope: None,
                        status: rejection.status,
                        method: method.as_str(),
                        path: path.as_str(),
                        headers: &headers,
                    },
                );
            } else {
                for event in rejection.events() {
                    emit_audit_log(
                        state.as_ref(),
                        GatewayAuditLogEmitInput {
                            category: "security",
                            action: event.action,
                            reason: security_audit_reason(event),
                            scope: Some(event.rule),
                            status: rejection.status,
                            method: method.as_str(),
                            path: path.as_str(),
                            headers: &headers,
                        },
                    );
                }
            }
            let mut response = problem_response_with_request_headers(
                rejection.status,
                rejection.title,
                rejection.detail.as_str(),
                request.headers(),
            );
            rejection.apply_headers(response.headers_mut());
            return apply_configured_secure_response_headers(state.as_ref(), response);
        }
    };

    for event in inspection.events() {
        state.metrics.record_security_event(
            state.config.service.name.as_str(),
            event.kind,
            event.action,
            event.rule,
        );
    }

    let (request, waf_body_events) =
        match security::inspect_json_body(&state.config.security, request).await {
            Ok((request, events)) => (request, events),
            Err(rejection) => {
                for event in rejection.events() {
                    state.metrics.record_security_event(
                        state.config.service.name.as_str(),
                        event.kind,
                        event.action,
                        event.rule,
                    );
                    emit_audit_log(
                        state.as_ref(),
                        GatewayAuditLogEmitInput {
                            category: "security",
                            action: event.action,
                            reason: security_audit_reason(event),
                            scope: Some(event.rule),
                            status: rejection.status,
                            method: method.as_str(),
                            path: path.as_str(),
                            headers: &headers,
                        },
                    );
                }
                let mut response = problem_response_with_request_headers(
                    rejection.status,
                    rejection.title,
                    rejection.detail.as_str(),
                    &headers,
                );
                rejection.apply_headers(response.headers_mut());
                return apply_configured_secure_response_headers(state.as_ref(), response);
            }
        };
    for event in &waf_body_events {
        state.metrics.record_security_event(
            state.config.service.name.as_str(),
            event.kind,
            event.action,
            event.rule,
        );
    }

    let mut response = next.run(request).await;
    inspection.apply_headers(response.headers_mut());
    for event in inspection.events().iter().chain(waf_body_events.iter()) {
        emit_audit_log(
            state.as_ref(),
            GatewayAuditLogEmitInput {
                category: "security",
                action: event.action,
                reason: security_audit_reason(event),
                scope: Some(event.rule),
                status: response.status(),
                method: method.as_str(),
                path: path.as_str(),
                headers: &headers,
            },
        );
    }
    apply_configured_secure_response_headers(state.as_ref(), response)
}

async fn traffic_middleware(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let span = request_span(&state.config, &request);
    let _span_guard = span.enter();
    let service_id = state
        .registry
        .resolve(method.as_str(), path.as_str())
        .map(|route| route.service_id.clone());
    let permit = match state
        .traffic
        .acquire(
            &state.config.traffic,
            path.as_str(),
            service_id.as_deref(),
            request.headers(),
        )
        .await
    {
        Ok(permit) => permit,
        Err(rejection) => {
            state.metrics.record_traffic_rejection(
                state.config.service.name.as_str(),
                rejection.kind,
                rejection.scope.as_str(),
                rejection.status,
            );
            emit_audit_log(
                state.as_ref(),
                GatewayAuditLogEmitInput {
                    category: "traffic",
                    action: "reject",
                    reason: rejection.kind,
                    scope: Some(rejection.scope.as_str()),
                    status: rejection.status,
                    method: method.as_str(),
                    path: path.as_str(),
                    headers: request.headers(),
                },
            );
            let mut response = problem_response_with_request_headers(
                rejection.status,
                rejection.title,
                rejection.detail.as_str(),
                request.headers(),
            );
            rejection.apply_headers(response.headers_mut());
            return apply_configured_secure_response_headers(state.as_ref(), response);
        }
    };

    let mut response = next.run(request).instrument(span.clone()).await;
    permit.apply_headers(response.headers_mut());
    response
}

/// Structured request span when `observability.tracingEnabled` is on. The
/// span carries method, path, request id, and the W3C trace id so access and
/// audit events stay correlated across the request lifecycle.
fn request_span(config: &GatewayRuntimeConfig, request: &Request<Body>) -> tracing::Span {
    if !config.observability.tracing_enabled {
        return tracing::Span::none();
    }
    let request_id = request_id_for_headers(request.headers());
    let trace_id = trace_id_for_headers(request.headers());
    let span = tracing::info_span!(
        "sdkwork_gateway_request",
        method = %request.method(),
        path = %request.uri().path(),
        request_id = %request_id,
        trace_id = ?trace_id,
    );
    span
}

async fn gateway_fallback(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let path = uri.path().to_string();
    let descriptor = state
        .registry
        .resolve(method.as_str(), path.as_str())
        .cloned();
    let started_at = Instant::now();

    match descriptor {
        Some(route) if route.execution_target == RouteExecutionTarget::Dependency => {
            let policy_decision = match policy::evaluate_request(
                &state.config.policy,
                &route,
                method.as_str(),
                path.as_str(),
                &headers,
            ) {
                Ok(decision) => decision,
                Err(rejection) => {
                    let response = policy_rejection_response(
                        state.as_ref(),
                        rejection,
                        method.as_str(),
                        path.as_str(),
                        &headers,
                    );
                    record_gateway_request(
                        state.as_ref(),
                        &route,
                        method.as_str(),
                        response.status(),
                    );
                    record_gateway_request_duration(
                        state.as_ref(),
                        &route,
                        method.as_str(),
                        response.status(),
                        started_at.elapsed(),
                    );
                    emit_access_log(
                        state.as_ref(),
                        Some(&route),
                        method.as_str(),
                        path.as_str(),
                        &response,
                        &headers,
                    );
                    return response;
                }
            };

            let mut response = route_dependency_request(
                state.clone(),
                route.clone(),
                request,
                policy_decision.clone(),
            )
            .await;
            apply_policy_dry_run_events(
                state.as_ref(),
                &policy_decision,
                method.as_str(),
                path.as_str(),
                &headers,
                &mut response,
            );
            record_gateway_request(state.as_ref(), &route, method.as_str(), response.status());
            record_gateway_request_duration(
                state.as_ref(),
                &route,
                method.as_str(),
                response.status(),
                started_at.elapsed(),
            );
            emit_access_log(
                state.as_ref(),
                Some(&route),
                method.as_str(),
                path.as_str(),
                &response,
                &headers,
            );
            response
        }
        Some(route) => {
            let response = mark_failure_class(
                problem_response_with_request_headers(
                    StatusCode::NOT_IMPLEMENTED,
                    "gateway_route_not_implemented",
                    "the matched route is owned by the gateway but no handler is registered",
                    &headers,
                ),
                "gateway_route_not_implemented",
            );
            record_gateway_request(state.as_ref(), &route, method.as_str(), response.status());
            record_gateway_request_duration(
                state.as_ref(),
                &route,
                method.as_str(),
                response.status(),
                started_at.elapsed(),
            );
            emit_access_log(
                state.as_ref(),
                Some(&route),
                method.as_str(),
                path.as_str(),
                &response,
                &headers,
            );
            response
        }
        None => {
            if let Some(router) = state.composed_router.clone() {
                return match router.oneshot(request).await {
                    Ok(response) => response,
                    Err(error) => problem_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "composed_api_host_error",
                        format!("composed API host failed: {error}"),
                    ),
                };
            }
            if let Some(surface) = declared_surface_for_path(&state.config, path.as_str()) {
                let response = mark_failure_class(
                    problem_response_with_request_headers(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "dependency_not_available",
                        format!(
                            "dependency surface `{}` is declared but not embedded in this gateway build",
                            surface.service_id
                        ),
                        &headers,
                    ),
                    "dependency_not_available",
                );
                record_unmatched_metrics(state.as_ref(), method.as_str(), &response);
                emit_access_log(
                    state.as_ref(),
                    None,
                    method.as_str(),
                    path.as_str(),
                    &response,
                    &headers,
                );
                return response;
            }
            let response = mark_failure_class(
                problem_response_with_request_headers(
                    StatusCode::NOT_FOUND,
                    "gateway_route_not_found",
                    "no SDKWork gateway route matched the request",
                    &headers,
                ),
                "gateway_route_not_found",
            );
            emit_access_log(
                state.as_ref(),
                None,
                method.as_str(),
                path.as_str(),
                &response,
                &headers,
            );
            response
        }
    }
}

/// Find the declared dependency surface whose API prefix matches the request
/// path, so requests to declared-but-not-embedded surfaces fail with an honest
/// 503 instead of a generic 404.
fn declared_surface_for_path<'a>(
    config: &'a GatewayRuntimeConfig,
    path: &str,
) -> Option<&'a sdkwork_api_cloud_gateway_config::DependencyApiSurfaceConfig> {
    config
        .dependency_surfaces
        .iter()
        .filter(|surface| {
            let prefix = surface.api_prefix.trim_end_matches('/');
            path == prefix || path.starts_with(format!("{prefix}/").as_str())
        })
        .max_by_key(|surface| surface.api_prefix.len())
}

fn record_unmatched_metrics(state: &GatewayState, method: &str, response: &Response) {
    state.metrics.record_http_request(
        state.config.service.name.as_str(),
        method,
        "unmatched",
        "none",
        response.status(),
    );
}

fn policy_rejection_response(
    state: &GatewayState,
    rejection: policy::PolicyRejection,
    method: &str,
    path: &str,
    headers: &HeaderMap,
) -> Response {
    state.metrics.record_policy_rejection(
        state.config.service.name.as_str(),
        rejection.kind,
        rejection.scope.as_str(),
        rejection.status,
    );
    emit_audit_log(
        state,
        GatewayAuditLogEmitInput {
            category: "policy",
            action: "reject",
            reason: rejection.kind,
            scope: Some(rejection.scope.as_str()),
            status: rejection.status,
            method,
            path,
            headers,
        },
    );

    let mut response = problem_response_with_request_headers(
        rejection.status,
        rejection.title,
        rejection.detail.as_str(),
        headers,
    );
    rejection.apply_headers(response.headers_mut());
    mark_failure_class(response, rejection.title)
}

fn apply_policy_dry_run_events(
    state: &GatewayState,
    decision: &policy::PolicyDecision,
    method: &str,
    path: &str,
    headers: &HeaderMap,
    response: &mut Response,
) {
    let dry_run_events = decision.dry_run_events();
    if dry_run_events.is_empty() {
        return;
    }

    let status = response.status();
    for event in dry_run_events {
        state.metrics.record_policy_dry_run(
            state.config.service.name.as_str(),
            event.kind,
            event.scope.as_str(),
            status,
        );
        emit_audit_log(
            state,
            GatewayAuditLogEmitInput {
                category: "policy",
                action: "dry-run",
                reason: event.kind,
                scope: Some(event.scope.as_str()),
                status,
                method,
                path,
                headers,
            },
        );
    }

    let first = &dry_run_events[0];
    response.headers_mut().insert(
        "x-sdkwork-policy-action",
        HeaderValue::from_static("dry-run"),
    );
    response.headers_mut().insert(
        "x-sdkwork-policy-kind",
        HeaderValue::from_static(first.kind),
    );
    if let Ok(scope) = HeaderValue::from_str(first.scope.as_str()) {
        response
            .headers_mut()
            .insert("x-sdkwork-policy-rule", scope);
    }
}

async fn route_dependency_request(
    state: Arc<GatewayState>,
    route: RouteDescriptor,
    request: Request<Body>,
    policy_decision: policy::PolicyDecision,
) -> Response {
    let request_method = request.method().as_str().to_string();
    let request_path = request.uri().path().to_string();
    let request_headers = request.headers().clone();
    let request = match enforce_request_body_limit(
        request,
        policy_decision.max_body_bytes(state.config.request.max_body_bytes),
    )
    .await
    {
        Ok(request) => request,
        Err(rejection) => {
            if let Some(scope) = policy_decision.max_body_bytes_scope() {
                state.metrics.record_policy_rejection(
                    state.config.service.name.as_str(),
                    rejection.title,
                    scope,
                    rejection.status,
                );
                emit_audit_log(
                    state.as_ref(),
                    GatewayAuditLogEmitInput {
                        category: "policy",
                        action: "reject",
                        reason: rejection.title,
                        scope: Some(scope),
                        status: rejection.status,
                        method: request_method.as_str(),
                        path: request_path.as_str(),
                        headers: &request_headers,
                    },
                );
            }
            let title = rejection.title;
            return mark_failure_class(rejection.into_response(), title);
        }
    };

    if let Some(router) = state.composed_router.clone() {
        return match router.oneshot(request).await {
            Ok(response) => response,
            Err(error) => problem_response(
                StatusCode::BAD_GATEWAY,
                "composed_dependency_error",
                format!("composed dependency host failed: {error}"),
            ),
        };
    }

    if let Some(router) = state
        .embedded_routers
        .get(route.service_id.as_str())
        .cloned()
    {
        return match router.oneshot(request).await {
            Ok(response) => response,
            Err(error) => problem_response(
                StatusCode::BAD_GATEWAY,
                "embedded_dependency_error",
                format!("embedded dependency `{}` failed: {error}", route.service_id),
            ),
        };
    }

    problem_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "embedded_dependency_not_available",
        format!("dependency `{}` has no embedded router", route.service_id),
    )
}

fn mark_failure_class(mut response: Response, failure_class: &'static str) -> Response {
    response
        .extensions_mut()
        .insert(GatewayFailureClass(failure_class));
    response
}

fn failure_class_for_response(response: &Response) -> Option<&'static str> {
    response
        .extensions()
        .get::<GatewayFailureClass>()
        .map(|failure_class| failure_class.0)
}

struct RequestBodyRejection {
    status: StatusCode,
    title: &'static str,
    detail: String,
    request_id: String,
}

impl RequestBodyRejection {
    fn into_response(self) -> Response {
        problem_response_with_request_id(
            self.status,
            self.title,
            self.detail,
            Some(self.request_id.as_str()),
        )
    }
}

async fn enforce_request_body_limit(
    request: Request<Body>,
    max_body_bytes: usize,
) -> Result<Request<Body>, RequestBodyRejection> {
    let (parts, body) = request.into_parts();
    let request_id = request_id_for_headers(&parts.headers);
    let body_bytes = match to_bytes(body, max_body_bytes).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let (status, title, detail) = if is_length_limit_error(&error) {
                (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "request_body_too_large",
                    "request body exceeded configured gateway limit".to_string(),
                )
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    "invalid_body",
                    format!("request body could not be read: {error}"),
                )
            };

            return Err(RequestBodyRejection {
                status,
                title,
                detail,
                request_id,
            });
        }
    };

    Ok(Request::from_parts(parts, Body::from(body_bytes)))
}

fn request_id_for_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(new_request_id)
}

fn is_length_limit_error(error: &axum::Error) -> bool {
    let mut source = error.source();
    while let Some(error) = source {
        if error.is::<LengthLimitError>() {
            return true;
        }
        source = error.source();
    }
    false
}

fn validate_preflight(
    config: &GatewayRuntimeConfig,
    embedded_routers: &HashMap<String, Router>,
    has_composed_router: bool,
) -> Result<(), GatewayRuntimeError> {
    for surface in config.required_dependency_surfaces() {
        if !has_composed_router && !embedded_routers.contains_key(surface.service_id.as_str()) {
            return Err(GatewayRuntimeError::MissingEmbeddedRouter {
                service_id: surface.service_id.clone(),
            });
        }
    }

    Ok(())
}

fn registry_for_config(config: &GatewayRuntimeConfig) -> GatewayRouteRegistry {
    GatewayRouteRegistry::from_dependency_surfaces(config.dependency_surfaces.as_slice())
}

fn exact_registry_for_contributions(
    config: &GatewayRuntimeConfig,
    contributions: &[ApiAssemblyContribution],
) -> Result<GatewayRouteRegistry, GatewayRuntimeError> {
    let mut descriptors = Vec::new();
    for contribution in contributions {
        for route in contribution.route_manifest.routes() {
            let surface_name = api_surface_name(route.path).ok_or_else(|| {
                GatewayRuntimeError::ApiAssemblyComposition {
                    message: format!(
                        "{} route {} has no canonical API surface",
                        contribution.owner, route.path
                    ),
                }
            })?;
            let surface = config
                .dependency_surfaces
                .iter()
                .filter(|surface| surface.workspace == contribution.owner)
                .filter(|surface| normalized_surface_name(surface.surface.as_str()) == surface_name)
                .filter(|surface| route_path_belongs_to_prefix(route.path, &surface.api_prefix))
                .max_by_key(|surface| surface.api_prefix.len())
                .ok_or_else(|| GatewayRuntimeError::ApiAssemblyComposition {
                    message: format!(
                        "{} route {} is not declared by a matching dependency API surface",
                        contribution.owner, route.path
                    ),
                })?;
            descriptors.push(RouteDescriptor {
                service_id: surface.service_id.clone(),
                path_pattern: route.path.to_owned(),
                api_prefix: surface.api_prefix.clone(),
                surface: surface.surface.clone(),
                methods: vec![http_method_name(route.method).to_owned()],
                protocol: RouteProtocol::Http,
                visibility: route_visibility(surface_name),
                execution_target: RouteExecutionTarget::Dependency,
                sdk_target: Some(surface.sdk_family.clone()),
                requires_executable_dependency: true,
            });
        }
    }

    let wired_workspaces = contributions
        .iter()
        .map(|contribution| contribution.owner)
        .collect::<std::collections::HashSet<_>>();
    for surface in config.required_dependency_surfaces() {
        // Surfaces of workspaces that are not wired into this build remain
        // declared-but-unavailable (503 at request time); only wired
        // workspaces must prove executable route coverage here.
        if !wired_workspaces.contains(surface.workspace.as_str()) {
            continue;
        }
        if !descriptors
            .iter()
            .any(|route| route.service_id == surface.service_id)
        {
            return Err(GatewayRuntimeError::MissingEmbeddedRouter {
                service_id: surface.service_id.clone(),
            });
        }
    }

    GatewayRouteRegistry::try_from_exact_routes(descriptors).map_err(|error| {
        GatewayRuntimeError::RouteRegistry {
            message: error.to_string(),
        }
    })
}

fn api_surface_name(path: &str) -> Option<&'static str> {
    match sdkwork_web_contract::infer_api_surface_from_path(path) {
        ApiSurface::AppApi => Some("app"),
        ApiSurface::BackendApi => Some("backend"),
        ApiSurface::OpenApi => Some("open"),
        ApiSurface::InternalApi => Some("internal"),
        ApiSurface::GatewayApi => Some("gateway"),
        ApiSurface::Unknown => None,
    }
}

fn normalized_surface_name(surface: &str) -> &str {
    surface
        .trim()
        .strip_suffix("-api")
        .unwrap_or(surface.trim())
}

fn route_path_belongs_to_prefix(path: &str, prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches('/');
    path == prefix || path.starts_with(format!("{prefix}/").as_str())
}

fn http_method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Delete => "DELETE",
        HttpMethod::Get => "GET",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
    }
}

fn route_visibility(surface: &str) -> RouteVisibility {
    match surface {
        "app" => RouteVisibility::App,
        "backend" => RouteVisibility::Backend,
        "open" => RouteVisibility::Open,
        _ => RouteVisibility::Internal,
    }
}

fn metrics_route_path(path: &str) -> String {
    gateway_route_path(path, "/metrics")
}

fn gateway_route_path(path: &str, fallback: &str) -> String {
    sdkwork_api_cloud_gateway_config::normalize_gateway_route_path(path, fallback)
}

fn record_gateway_request(
    state: &GatewayState,
    route: &RouteDescriptor,
    method: &str,
    status: StatusCode,
) {
    state.metrics.record_http_request(
        state.config.service.name.as_str(),
        method,
        route_target_label(route.execution_target),
        route.service_id.as_str(),
        status,
    );
}

fn record_gateway_request_duration(
    state: &GatewayState,
    route: &RouteDescriptor,
    method: &str,
    status: StatusCode,
    duration: Duration,
) {
    state.metrics.record_http_request_duration(
        state.config.service.name.as_str(),
        method,
        route_target_label(route.execution_target),
        route.service_id.as_str(),
        status,
        duration,
    );
}

fn route_target_label(target: RouteExecutionTarget) -> &'static str {
    match target {
        RouteExecutionTarget::Gateway => "gateway",
        RouteExecutionTarget::Dependency => "dependency",
    }
}

fn security_audit_reason(event: &security::SecurityEvent) -> &'static str {
    match (event.kind, event.rule) {
        ("waf", "baseline-signature") => "waf_baseline_signature",
        _ => event.kind,
    }
}

fn should_sample_log(sample_rate_basis_points: u32, seed_parts: &[&str]) -> bool {
    if sample_rate_basis_points == 0 {
        return false;
    }
    if sample_rate_basis_points >= 10_000 {
        return true;
    }

    stable_log_sample_hash(seed_parts) % 10_000 < u64::from(sample_rate_basis_points)
}

fn stable_log_sample_hash(seed_parts: &[&str]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for part in seed_parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn emit_audit_log(state: &GatewayState, input: GatewayAuditLogEmitInput<'_>) {
    if !state.config.observability.audit_log_enabled {
        return;
    }

    let request_id = input
        .headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok());
    let traceparent = input
        .headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok());
    if !should_sample_log(
        state
            .config
            .observability
            .audit_log_sample_rate_basis_points,
        &[
            input.category,
            input.action,
            input.reason,
            input.scope.unwrap_or(""),
            request_id.unwrap_or(""),
            traceparent.unwrap_or(""),
            input.method,
            input.path,
        ],
    ) {
        return;
    }

    let (route_target, service_id) = audit_route_context(state, input.method, input.path);
    let event = build_gateway_audit_log_event(
        &state.config,
        GatewayAuditLogInput {
            category: input.category,
            action: input.action,
            reason: input.reason,
            scope: input.scope,
            status: input.status.as_u16(),
            method: input.method,
            route_target,
            service_id,
            request_id,
            traceparent,
        },
    );

    info!(
        target: "sdkwork_api_cloud_gateway::audit",
        ?event,
        "gateway audit"
    );
}

fn audit_route_context<'a>(
    state: &'a GatewayState,
    method: &str,
    path: &str,
) -> (&'static str, Option<&'a str>) {
    state
        .registry
        .resolve(method, path)
        .map(|route| {
            (
                route_target_label(route.execution_target),
                Some(route.service_id.as_str()),
            )
        })
        .unwrap_or(("unmatched", None))
}

fn emit_access_log(
    state: &GatewayState,
    route: Option<&RouteDescriptor>,
    method: &str,
    path: &str,
    response: &Response,
    headers: &HeaderMap,
) {
    if !state.config.observability.access_log_enabled {
        return;
    }

    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok());
    let traceparent = headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok());
    if !should_sample_log(
        state
            .config
            .observability
            .access_log_sample_rate_basis_points,
        &[
            request_id.unwrap_or(""),
            traceparent.unwrap_or(""),
            method,
            path,
        ],
    ) {
        return;
    }

    let header_pairs = headers
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|value| (name.as_str(), value)))
        .collect::<Vec<_>>();
    let event = build_gateway_access_log_event(
        &state.config,
        GatewayAccessLogInput {
            method,
            path,
            status: response.status().as_u16(),
            route_target: route
                .map(|route| route_target_label(route.execution_target))
                .unwrap_or("unmatched"),
            service_id: route.map(|route| route.service_id.as_str()),
            failure_class: failure_class_for_response(response),
            request_id,
            traceparent,
            headers: header_pairs,
        },
    );

    info!(
        target: "sdkwork_api_cloud_gateway::access",
        ?event,
        "gateway access"
    );
}

fn cors_layer(config: &GatewayRuntimeConfig) -> tower_http::cors::CorsLayer {
    let policy = web_framework::gateway_cors_policy(config);
    let mut layer = sdkwork_web_axum::cors_layer_from_policy(policy.clone());
    // Same-origin requests (Origin == Host) are always allowed for dependency
    // surfaces that declare `sameOriginAllowed`, on top of the canonical CORS
    // allowlist (development private-network semantics + the
    // SDKWORK_CORS_ALLOWED_ORIGINS allowlist). The predicate therefore accepts
    // the canonical policy origins first and only then widens to same-origin,
    // so first-party browser flows keep working without weakening the
    // configured cross-origin policy.
    if config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.same_origin_allowed)
    {
        let canonical_policy = policy;
        layer = layer.allow_origin(tower_http::cors::AllowOrigin::predicate(
            move |origin, request_parts| {
                let origin = origin.to_str().unwrap_or_default();
                canonical_policy.allows_origin_value(origin)
                    || request_parts
                        .headers
                        .get(header::HOST)
                        .and_then(|host| host.to_str().ok())
                        .map(|host| same_origin(origin, host))
                        .unwrap_or(false)
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

fn problem_response(
    status: StatusCode,
    title: impl Into<String>,
    detail: impl Into<String>,
) -> Response {
    problem_response_with_request_id(status, title, detail, None)
}

fn problem_response_with_request_headers(
    status: StatusCode,
    title: impl Into<String>,
    detail: impl Into<String>,
    headers: &HeaderMap,
) -> Response {
    let request_id = request_id_for_headers(headers);
    let trace_id = trace_id_for_headers(headers);
    problem_response_with_context(
        status,
        title,
        detail,
        Some(request_id.as_str()),
        trace_id.as_deref(),
    )
}

fn problem_response_with_request_id(
    status: StatusCode,
    title: impl Into<String>,
    detail: impl Into<String>,
    request_id: Option<&str>,
) -> Response {
    problem_response_with_context(status, title, detail, request_id, None)
}

fn problem_response_with_context(
    status: StatusCode,
    title: impl Into<String>,
    detail: impl Into<String>,
    request_id: Option<&str>,
    trace_id: Option<&str>,
) -> Response {
    let mut body = serde_json::Map::new();
    body.insert("type".to_string(), json!("about:blank"));
    body.insert("title".to_string(), json!(title.into()));
    body.insert("detail".to_string(), json!(detail.into()));
    body.insert("status".to_string(), json!(status.as_u16()));
    if let Some(trace_id) = trace_id {
        body.insert("traceId".to_string(), json!(trace_id));
    }
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/problem+json");
    if let Some(request_id) = request_id {
        builder = builder.header("x-request-id", request_id);
    }
    builder
        .body(Body::from(serde_json::Value::Object(body).to_string()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn trace_id_for_headers(headers: &HeaderMap) -> Option<String> {
    let traceparent = headers.get("traceparent")?.to_str().ok()?;
    let mut parts = traceparent.split('-');
    let _version = parts.next()?;
    let trace_id = parts.next()?;
    if trace_id.len() != 32 || trace_id.chars().all(|character| character == '0') {
        return None;
    }
    trace_id
        .chars()
        .all(|character| character.is_ascii_hexdigit())
        .then(|| trace_id.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_sampling_honors_disabled_and_full_rate() {
        assert!(!should_sample_log(0, &["request-1", "GET", "/healthz"]));
        assert!(should_sample_log(10_000, &["request-1", "GET", "/healthz"]));
        assert!(should_sample_log(20_000, &["request-1", "GET", "/healthz"]));
    }

    #[test]
    fn log_sampling_is_deterministic_for_partial_rate() {
        let first = should_sample_log(2_500, &["request-1", "POST", "/app/v3/api/auth"]);
        let second = should_sample_log(2_500, &["request-1", "POST", "/app/v3/api/auth"]);

        assert_eq!(first, second);
    }
}
