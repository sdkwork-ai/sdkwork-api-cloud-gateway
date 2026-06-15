use std::{
    collections::HashMap,
    error::Error as StdError,
    fmt, io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{
    body::{to_bytes, Body},
    extract::{
        ws::{
            rejection::WebSocketUpgradeRejection, CloseFrame as AxumCloseFrame,
            Message as AxumWsMessage, WebSocket, WebSocketUpgrade,
        },
        State,
    },
    http::{header, HeaderMap, HeaderValue, Request, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use http_body_util::LengthLimitError;
use reqwest::Client;
#[cfg(feature = "foundation-appbase")]
use sdkwork_api_gateway_config::APPBASE_APP_API_SERVICE_ID;
use sdkwork_api_gateway_config::{
    GatewayMode, GatewayResilienceConfig, GatewayRuntimeConfig, ServiceUpstreamConfig,
};
use sdkwork_api_gateway_observability::{
    build_gateway_access_log_event, build_gateway_audit_log_event, describe_gateway_startup,
    GatewayAccessLogInput, GatewayAuditLogInput,
};
use sdkwork_api_gateway_registry::{GatewayRouteRegistry, RouteDescriptor, RouteExecutionTarget};
#[cfg(feature = "foundation-appbase")]
use sdkwork_router_iam_app_api::SdkworkAppbaseLocalIamRuntimeContext;
use serde_json::json;
use tokio::{net::TcpStream, select};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        protocol::{
            frame::coding::CloseCode as TungsteniteCloseCode, CloseFrame as TungsteniteCloseFrame,
        },
        Message as TungsteniteMessage,
    },
    MaybeTlsStream, WebSocketStream,
};
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::{
    diagnostics, health, metrics::GatewayMetrics, openapi, policy, resilience, security, traffic,
    GatewayRuntimeError,
};

static REQUEST_ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    client: Client,
}

#[derive(Clone)]
pub(crate) struct GatewayState {
    pub(crate) config: GatewayRuntimeConfig,
    pub(crate) registry: GatewayRouteRegistry,
    pub(crate) embedded_routers: HashMap<String, Router>,
    pub(crate) client: Client,
    pub(crate) metrics: GatewayMetrics,
    pub(crate) resilience: resilience::GatewayResilienceState,
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
        validate_preflight(&config, &embedded_routers)?;
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(GatewayRuntimeError::HttpClient)?;

        Ok(Self {
            config,
            registry,
            embedded_routers,
            client,
        })
    }

    pub fn router(self) -> Router {
        let summary = describe_gateway_startup(&self.config, &self.registry);
        info!(
            service_name = %summary.service_name,
            bind = %summary.bind,
            mode = ?summary.mode,
            upstream_count = summary.upstream_count,
            route_count = summary.route_count,
            "starting sdkwork api gateway"
        );

        let cors = cors_layer(&self.config);
        let metrics_enabled = self.config.observability.metrics_enabled;
        let metrics_path = metrics_route_path(self.config.observability.metrics_path.as_str());
        let diagnostics_enabled = self.config.diagnostics.enabled;
        let diagnostics_path = gateway_route_path(
            self.config.diagnostics.path.as_str(),
            "/internal/gateway/diagnostics",
        );
        let state = Arc::new(GatewayState {
            config: self.config,
            registry: self.registry,
            embedded_routers: self.embedded_routers,
            client: self.client,
            metrics: GatewayMetrics::default(),
            resilience: resilience::GatewayResilienceState::default(),
            traffic: traffic::GatewayTrafficState::default(),
        });

        let mut router = Router::new()
            .route("/healthz", get(health::healthz))
            .route("/readyz", get(health::readyz))
            .route("/openapi/index.json", get(openapi::openapi_index))
            .route("/openapi.json", get(openapi::openapi_document))
            .fallback(gateway_fallback);
        if metrics_enabled {
            router = router.route(metrics_path.as_str(), get(metrics_endpoint));
        }
        if diagnostics_enabled {
            router = router.route(
                diagnostics_path.as_str(),
                get(diagnostics::diagnostics_endpoint),
            );
        }

        router
            .layer(cors)
            .layer(middleware::from_fn_with_state(
                state.clone(),
                traffic_middleware,
            ))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                security_middleware,
            ))
            .with_state(state)
    }
}

impl fmt::Debug for GatewayRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewayRuntime")
            .field("service", &self.config.service.name)
            .field("mode", &self.config.mode)
            .field("upstreams", &self.config.upstreams.len())
            .field(
                "embedded_routers",
                &self.embedded_routers.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

pub fn build_sdkwork_api_gateway_router(
    config: GatewayRuntimeConfig,
) -> Result<Router, GatewayRuntimeError> {
    GatewayRuntime::new(config).map(GatewayRuntime::router)
}

#[cfg(feature = "foundation-appbase")]
pub fn build_sdkwork_api_gateway_api_server_router(
    config: GatewayRuntimeConfig,
) -> Result<Router, GatewayRuntimeError> {
    match config.mode {
        GatewayMode::Split => build_sdkwork_api_gateway_router(config),
        GatewayMode::Embedded => build_sdkwork_api_gateway_router_with_embedded_routers(
            config,
            [(
                APPBASE_APP_API_SERVICE_ID,
                build_embedded_sdkwork_appbase_app_api_router(),
            )],
        ),
    }
}

#[cfg(not(feature = "foundation-appbase"))]
pub fn build_sdkwork_api_gateway_api_server_router(
    config: GatewayRuntimeConfig,
) -> Result<Router, GatewayRuntimeError> {
    build_sdkwork_api_gateway_router(config)
}

pub fn build_sdkwork_api_gateway_router_with_registry(
    config: GatewayRuntimeConfig,
    registry: GatewayRouteRegistry,
) -> Result<Router, GatewayRuntimeError> {
    GatewayRuntime::with_registry(config, registry).map(GatewayRuntime::router)
}

pub fn build_sdkwork_api_gateway_router_with_embedded_routers<I, K>(
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
pub fn build_sdkwork_api_gateway_router_with_embedded_appbase(
    mut config: GatewayRuntimeConfig,
) -> Result<Router, GatewayRuntimeError> {
    config.mode = GatewayMode::Embedded;
    build_sdkwork_api_gateway_router_with_embedded_routers(
        config,
        [(
            APPBASE_APP_API_SERVICE_ID,
            build_embedded_sdkwork_appbase_app_api_router(),
        )],
    )
}

#[cfg(feature = "foundation-appbase")]
pub fn build_embedded_sdkwork_appbase_app_api_router() -> Router {
    sdkwork_router_iam_app_api::build_sdkwork_appbase_app_api_router()
}

#[cfg(feature = "foundation-appbase")]
pub fn build_embedded_sdkwork_appbase_app_api_router_with_runtime_context(
    runtime_context: SdkworkAppbaseLocalIamRuntimeContext,
) -> Router {
    sdkwork_router_iam_app_api::build_sdkwork_appbase_app_api_router_with_runtime_context(runtime_context)
}

async fn metrics_endpoint(State(state): State<Arc<GatewayState>>) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(
            state.metrics.render(state.config.service.name.as_str()),
        ))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
            return security::apply_secure_response_headers(&state.config.security, response);
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
    let mut response = next.run(request).await;
    inspection.apply_headers(response.headers_mut());
    for event in inspection.events() {
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
    security::apply_secure_response_headers(&state.config.security, response)
}

async fn traffic_middleware(
    State(state): State<Arc<GatewayState>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let service_id = state
        .registry
        .resolve(method.as_str(), path.as_str())
        .map(|route| route.service_id.clone());
    let permit = match state.traffic.acquire(
        &state.config.traffic,
        path.as_str(),
        service_id.as_deref(),
        request.headers(),
    ) {
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
            return security::apply_secure_response_headers(&state.config.security, response);
        }
    };

    let mut response = next.run(request).await;
    permit.apply_headers(response.headers_mut());
    response
}

async fn gateway_fallback(
    State(state): State<Arc<GatewayState>>,
    websocket_upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
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

            if let Ok(websocket_upgrade) = websocket_upgrade {
                return route_websocket_dependency_request(
                    state,
                    route,
                    headers,
                    uri,
                    websocket_upgrade,
                )
                .await;
            }

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

async fn route_websocket_dependency_request(
    state: Arc<GatewayState>,
    route: RouteDescriptor,
    headers: HeaderMap,
    uri: Uri,
    websocket_upgrade: WebSocketUpgrade,
) -> Response {
    if state
        .embedded_routers
        .contains_key(route.service_id.as_str())
    {
        return problem_response(
            StatusCode::NOT_IMPLEMENTED,
            "embedded_websocket_dependency_not_supported",
            format!(
                "embedded websocket delegation for dependency `{}` is not supported by this gateway runtime",
                route.service_id
            ),
        );
    }

    let Some(upstream) = state.config.upstream(route.service_id.as_str()) else {
        return problem_response(
            StatusCode::BAD_GATEWAY,
            "dependency_not_available",
            format!(
                "dependency `{}` has no upstream or embedded router",
                route.service_id
            ),
        );
    };

    let upstream_socket = match connect_websocket_upstream(upstream, &headers, &uri).await {
        Ok(upstream_socket) => upstream_socket,
        Err(error) => {
            return problem_response(
                StatusCode::BAD_GATEWAY,
                "websocket_upstream_request_failed",
                format!(
                    "websocket upstream `{}` failed: {error}",
                    upstream.service_id
                ),
            );
        }
    };

    websocket_upgrade.on_upgrade(move |client_socket| async move {
        bridge_websockets(client_socket, upstream_socket).await;
    })
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

    if let Some(upstream) = state.config.upstream(route.service_id.as_str()) {
        let circuit_permit = match state
            .resilience
            .before_upstream_request(&state.config.resilience, upstream.service_id.as_str())
        {
            Ok(permit) => permit,
            Err(rejection) => {
                state.metrics.record_resilience_event(
                    state.config.service.name.as_str(),
                    upstream.service_id.as_str(),
                    rejection.event,
                );
                let mut response = problem_response_with_request_headers(
                    rejection.status,
                    rejection.title,
                    rejection.detail.as_str(),
                    request.headers(),
                );
                rejection.apply_headers(response.headers_mut());
                return mark_failure_class(response, rejection.title);
            }
        };
        if let Some(event) = circuit_permit.event() {
            state.metrics.record_resilience_event(
                state.config.service.name.as_str(),
                upstream.service_id.as_str(),
                event,
            );
        }

        let gateway_timeout_ms = policy_decision.timeout_ms(state.config.request.timeout_ms);
        let mut response = if let Some(timeout_duration) =
            gateway_request_timeout_duration(gateway_timeout_ms)
        {
            match tokio::time::timeout(
                timeout_duration,
                proxy_to_upstream(
                    &state.client,
                    upstream,
                    request,
                    &state.metrics,
                    state.config.service.name.as_str(),
                    &state.config.resilience,
                ),
            )
            .await
            {
                Ok(response) => response,
                Err(_) => {
                    state.metrics.record_resilience_event(
                        state.config.service.name.as_str(),
                        upstream.service_id.as_str(),
                        "gateway_request_timeout",
                    );
                    state.metrics.record_upstream_request(
                        state.config.service.name.as_str(),
                        upstream.service_id.as_str(),
                        StatusCode::GATEWAY_TIMEOUT,
                    );
                    mark_failure_class(
                        problem_response_with_request_headers(
                            StatusCode::GATEWAY_TIMEOUT,
                            "gateway_request_timeout",
                            format!(
                                "gateway request to dependency `{}` exceeded configured timeout of {} ms",
                                upstream.service_id, gateway_timeout_ms
                            ),
                            &request_headers,
                        ),
                        "gateway_request_timeout",
                    )
                }
            }
        } else {
            proxy_to_upstream(
                &state.client,
                upstream,
                request,
                &state.metrics,
                state.config.service.name.as_str(),
                &state.config.resilience,
            )
            .await
        };
        if let Some(event) = state.resilience.record_upstream_result(
            &state.config.resilience,
            upstream.service_id.as_str(),
            response.status(),
        ) {
            state.metrics.record_resilience_event(
                state.config.service.name.as_str(),
                upstream.service_id.as_str(),
                event,
            );
        }
        circuit_permit.apply_headers(response.headers_mut());
        return response;
    }

    problem_response(
        StatusCode::BAD_GATEWAY,
        "dependency_not_available",
        format!(
            "dependency `{}` has no upstream or embedded router",
            route.service_id
        ),
    )
}

fn gateway_request_timeout_duration(timeout_ms: u64) -> Option<Duration> {
    (timeout_ms > 0).then(|| Duration::from_millis(timeout_ms))
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

async fn proxy_to_upstream(
    client: &Client,
    upstream: &ServiceUpstreamConfig,
    request: Request<Body>,
    metrics: &GatewayMetrics,
    service_name: &str,
    resilience: &GatewayResilienceConfig,
) -> Response {
    let (parts, body) = request.into_parts();
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|path| path.as_str())
        .unwrap_or("/");
    let url = format!(
        "{}{}",
        upstream.base_url.trim_end_matches('/'),
        path_and_query
    );
    let method = match reqwest::Method::from_bytes(parts.method.as_str().as_bytes()) {
        Ok(method) => method,
        Err(error) => {
            return mark_failure_class(
                problem_response_with_request_id(
                    StatusCode::BAD_REQUEST,
                    "invalid_method",
                    format!("request method is not supported by proxy client: {error}"),
                    None,
                ),
                "invalid_method",
            );
        }
    };
    let body_bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return mark_failure_class(
                problem_response_with_request_id(
                    StatusCode::BAD_REQUEST,
                    "invalid_body",
                    format!("request body could not be read: {error}"),
                    None,
                ),
                "invalid_body",
            );
        }
    };

    let request_id = request_id_for_headers(&parts.headers);
    let max_attempts = retry_max_attempts(resilience);
    let mut attempt = 1_usize;

    loop {
        let mut builder = client
            .request(method.clone(), url.clone())
            .timeout(Duration::from_millis(upstream.timeout_ms))
            .body(body_bytes.clone());
        for (name, value) in parts.headers.iter() {
            if is_hop_by_hop_header(name.as_str()) {
                continue;
            }
            if let Ok(value) = value.to_str() {
                builder = builder.header(name.as_str(), value);
            }
        }
        builder = builder.header("x-request-id", request_id.as_str());
        builder = apply_forwarded_headers(builder, &parts.headers, parts.uri.scheme_str());

        let response = match builder.send().await {
            Ok(response) => response,
            Err(error) => {
                let (status, title) = if error.is_timeout() {
                    (StatusCode::GATEWAY_TIMEOUT, "upstream_timeout")
                } else {
                    (StatusCode::BAD_GATEWAY, "upstream_request_failed")
                };
                if error.is_timeout() {
                    metrics.record_resilience_event(
                        service_name,
                        upstream.service_id.as_str(),
                        "upstream_timeout",
                    );
                }
                metrics.record_upstream_request(service_name, upstream.service_id.as_str(), status);
                if should_retry_status(resilience, status.as_u16(), attempt, max_attempts) {
                    metrics.record_resilience_event(
                        service_name,
                        upstream.service_id.as_str(),
                        "retry_attempt",
                    );
                    retry_backoff(resilience, attempt).await;
                    attempt += 1;
                    continue;
                }
                if should_record_retry_exhausted(resilience, status.as_u16(), attempt, max_attempts)
                {
                    metrics.record_resilience_event(
                        service_name,
                        upstream.service_id.as_str(),
                        "retry_exhausted",
                    );
                }
                return mark_failure_class(
                    problem_response_with_request_id(
                        status,
                        title,
                        format!("upstream `{}` request failed: {error}", upstream.service_id),
                        Some(request_id.as_str()),
                    ),
                    title,
                );
            }
        };

        let status = response.status().as_u16();
        if let Ok(status_code) = StatusCode::from_u16(status) {
            metrics.record_upstream_request(
                service_name,
                upstream.service_id.as_str(),
                status_code,
            );
        }
        if should_retry_status(resilience, status, attempt, max_attempts) {
            metrics.record_resilience_event(
                service_name,
                upstream.service_id.as_str(),
                "retry_attempt",
            );
            retry_backoff(resilience, attempt).await;
            attempt += 1;
            continue;
        }
        let retry_exhausted =
            should_record_retry_exhausted(resilience, status, attempt, max_attempts);
        if retry_exhausted {
            metrics.record_resilience_event(
                service_name,
                upstream.service_id.as_str(),
                "retry_exhausted",
            );
        }

        let headers = response.headers().clone();
        let body = Body::from_stream(
            response
                .bytes_stream()
                .map_err(|error| io::Error::other(error.to_string())),
        );

        let mut builder = Response::builder()
            .status(status)
            .header("x-request-id", request_id.as_str());
        if attempt > 1 {
            builder = builder.header("x-sdkwork-gateway-retry-attempts", attempt.to_string());
        }
        for (name, value) in headers.iter() {
            if is_hop_by_hop_header(name.as_str()) {
                continue;
            }
            if let Ok(value) = value.to_str() {
                builder = builder.header(name.as_str(), value);
            }
        }
        return builder
            .body(body)
            .map(|response| {
                if retry_exhausted {
                    mark_failure_class(response, "retry_exhausted")
                } else {
                    response
                }
            })
            .unwrap_or_else(|error| {
                mark_failure_class(
                    problem_response_with_request_id(
                        StatusCode::BAD_GATEWAY,
                        "upstream_response_invalid",
                        format!(
                            "upstream `{}` response was invalid: {error}",
                            upstream.service_id
                        ),
                        Some(request_id.as_str()),
                    ),
                    "upstream_response_invalid",
                )
            });
    }
}

async fn connect_websocket_upstream(
    upstream: &ServiceUpstreamConfig,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, tokio_tungstenite::tungstenite::Error> {
    let upstream_url = websocket_upstream_url(upstream.base_url.as_str(), uri);
    let mut request = upstream_url.into_client_request()?;
    copy_websocket_forward_headers(headers, request.headers_mut());

    let (upstream_socket, _) = connect_async(request).await?;
    Ok(upstream_socket)
}

async fn bridge_websockets(
    client_socket: WebSocket,
    upstream_socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
) {
    let (mut client_sender, mut client_receiver) = client_socket.split();
    let (mut upstream_sender, mut upstream_receiver) = upstream_socket.split();

    loop {
        select! {
            message = client_receiver.next() => {
                let Some(Ok(message)) = message else {
                    break;
                };
                let close_frame = match &message {
                    AxumWsMessage::Close(frame) => frame.clone(),
                    _ => None,
                };
                if let Some(message) = axum_to_tungstenite_message(message) {
                    if upstream_sender.send(message).await.is_err() {
                        break;
                    }
                }
                if close_frame.is_some() {
                    let _ = client_sender.close().await;
                    break;
                }
            },
            message = upstream_receiver.next() => {
                let Some(Ok(message)) = message else {
                    break;
                };
                let close_frame = match &message {
                    TungsteniteMessage::Close(frame) => frame.clone(),
                    _ => None,
                };
                if let Some(message) = tungstenite_to_axum_message(message) {
                    if client_sender.send(message).await.is_err() {
                        break;
                    }
                }
                if close_frame.is_some() {
                    break;
                }
            },
        }
    }
}

fn axum_to_tungstenite_message(message: AxumWsMessage) -> Option<TungsteniteMessage> {
    match message {
        AxumWsMessage::Text(text) => Some(TungsteniteMessage::Text(text.to_string().into())),
        AxumWsMessage::Binary(bytes) => Some(TungsteniteMessage::Binary(bytes)),
        AxumWsMessage::Ping(bytes) => Some(TungsteniteMessage::Ping(bytes)),
        AxumWsMessage::Pong(bytes) => Some(TungsteniteMessage::Pong(bytes)),
        AxumWsMessage::Close(frame) => Some(TungsteniteMessage::Close(
            frame.map(axum_close_frame_to_tungstenite),
        )),
    }
}

fn tungstenite_to_axum_message(message: TungsteniteMessage) -> Option<AxumWsMessage> {
    match message {
        TungsteniteMessage::Text(text) => Some(AxumWsMessage::Text(text.to_string().into())),
        TungsteniteMessage::Binary(bytes) => Some(AxumWsMessage::Binary(bytes)),
        TungsteniteMessage::Ping(bytes) => Some(AxumWsMessage::Ping(bytes)),
        TungsteniteMessage::Pong(bytes) => Some(AxumWsMessage::Pong(bytes)),
        TungsteniteMessage::Close(frame) => Some(AxumWsMessage::Close(
            frame.map(tungstenite_close_frame_to_axum),
        )),
        TungsteniteMessage::Frame(_) => None,
    }
}

fn axum_close_frame_to_tungstenite(frame: AxumCloseFrame) -> TungsteniteCloseFrame {
    TungsteniteCloseFrame {
        code: TungsteniteCloseCode::from(frame.code),
        reason: frame.reason.to_string().into(),
    }
}

fn tungstenite_close_frame_to_axum(frame: TungsteniteCloseFrame) -> AxumCloseFrame {
    AxumCloseFrame {
        code: frame.code.into(),
        reason: frame.reason.to_string().into(),
    }
}

fn websocket_upstream_url(base_url: &str, uri: &Uri) -> String {
    let websocket_base_url = if let Some(rest) = base_url.strip_prefix("https://") {
        format!("wss://{}", rest.trim_end_matches('/'))
    } else if let Some(rest) = base_url.strip_prefix("http://") {
        format!("ws://{}", rest.trim_end_matches('/'))
    } else {
        base_url.trim_end_matches('/').to_string()
    };
    let path_and_query = uri
        .path_and_query()
        .map(|value| value.as_str())
        .unwrap_or("/");
    format!("{websocket_base_url}{path_and_query}")
}

fn copy_websocket_forward_headers(headers: &HeaderMap, upstream_headers: &mut HeaderMap) {
    for (name, value) in headers.iter() {
        if is_websocket_client_managed_header(name.as_str()) {
            continue;
        }
        upstream_headers.insert(name.clone(), value.clone());
    }
    if let Ok(request_id) = HeaderValue::from_str(request_id_for_headers(headers).as_str()) {
        upstream_headers.insert("x-request-id", request_id);
    }
    apply_forwarded_headers_to_header_map(upstream_headers, headers, None);
}

fn request_id_for_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(generate_request_id)
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

fn generate_request_id() -> String {
    let next = REQUEST_ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("sdkwork-api-gateway-{next}")
}

fn apply_forwarded_headers(
    builder: reqwest::RequestBuilder,
    headers: &HeaderMap,
    scheme: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut builder = builder;
    if let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    {
        builder = builder.header("x-forwarded-host", host);
    }
    builder.header("x-forwarded-proto", scheme.unwrap_or("http"))
}

fn apply_forwarded_headers_to_header_map(
    target: &mut HeaderMap,
    source: &HeaderMap,
    scheme: Option<&str>,
) {
    if let Some(host) = source
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| HeaderValue::from_str(value).ok())
    {
        target.insert("x-forwarded-host", host);
    }
    target.insert(
        "x-forwarded-proto",
        HeaderValue::from_str(scheme.unwrap_or("http")).expect("static scheme header is valid"),
    );
}

fn validate_preflight(
    config: &GatewayRuntimeConfig,
    embedded_routers: &HashMap<String, Router>,
) -> Result<(), GatewayRuntimeError> {
    for surface in config.required_dependency_surfaces() {
        if config.upstream(surface.service_id.as_str()).is_some() {
            continue;
        }

        match config.mode {
            GatewayMode::Split => {
                return Err(GatewayRuntimeError::MissingDependencyUpstream {
                    service_id: surface.service_id.clone(),
                });
            }
            GatewayMode::Embedded => {
                if !embedded_routers.contains_key(surface.service_id.as_str()) {
                    return Err(GatewayRuntimeError::MissingEmbeddedRouter {
                        service_id: surface.service_id.clone(),
                    });
                }
            }
        }
    }

    Ok(())
}

fn registry_for_config(config: &GatewayRuntimeConfig) -> GatewayRouteRegistry {
    GatewayRouteRegistry::from_dependency_surfaces(config.dependency_surfaces.as_slice())
}

fn metrics_route_path(path: &str) -> String {
    gateway_route_path(path, "/metrics")
}

fn gateway_route_path(path: &str, fallback: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return fallback.to_string();
    }
    if path.starts_with('/') {
        return path.to_string();
    }
    format!("/{path}")
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

fn retry_max_attempts(resilience: &GatewayResilienceConfig) -> usize {
    if resilience.retry_enabled {
        resilience.max_attempts.max(1)
    } else {
        1
    }
}

fn should_retry_status(
    resilience: &GatewayResilienceConfig,
    status: u16,
    attempt: usize,
    max_attempts: usize,
) -> bool {
    resilience.retry_enabled
        && attempt < max_attempts
        && resilience.retry_statuses.contains(&status)
}

fn should_record_retry_exhausted(
    resilience: &GatewayResilienceConfig,
    status: u16,
    attempt: usize,
    max_attempts: usize,
) -> bool {
    resilience.retry_enabled
        && max_attempts > 1
        && attempt >= max_attempts
        && resilience.retry_statuses.contains(&status)
}

async fn retry_backoff(resilience: &GatewayResilienceConfig, attempt: usize) {
    if resilience.retry_backoff_ms == 0 {
        return;
    }

    tokio::time::sleep(Duration::from_millis(
        resilience.retry_backoff_ms * attempt as u64,
    ))
    .await;
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
        target: "sdkwork_api_gateway::audit",
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
        target: "sdkwork_api_gateway::access",
        ?event,
        "gateway access"
    );
}

fn cors_layer(config: &GatewayRuntimeConfig) -> CorsLayer {
    let layer = CorsLayer::new().allow_methods(Any).allow_headers(Any);
    if config.cors.allow_any_origin {
        return layer.allow_origin(Any);
    }

    let origins = config
        .cors
        .allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect::<Vec<_>>();
    layer.allow_origin(origins)
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

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    ) || name.eq_ignore_ascii_case(header::HOST.as_str())
}

fn is_websocket_client_managed_header(name: &str) -> bool {
    is_hop_by_hop_header(name)
        || matches!(
            name.to_ascii_lowercase().as_str(),
            "sec-websocket-key"
                | "sec-websocket-version"
                | "sec-websocket-protocol"
                | "sec-websocket-extensions"
        )
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
