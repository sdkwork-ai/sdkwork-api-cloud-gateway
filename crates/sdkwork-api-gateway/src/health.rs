use std::{sync::Arc, time::Duration};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use sdkwork_api_gateway_config::ServiceUpstreamConfig;
use serde_json::json;

use crate::runtime::GatewayState;

pub(crate) async fn healthz() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "sdkwork-api-gateway"
    }))
}

pub(crate) async fn readyz(State(state): State<Arc<GatewayState>>) -> Response {
    let upstream_checks = if state.config.readiness.check_upstreams {
        check_required_upstreams(&state).await
    } else {
        Vec::new()
    };
    let degraded = upstream_checks
        .iter()
        .any(|check| check["status"] == "unavailable");
    let status = if degraded {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    } else {
        axum::http::StatusCode::OK
    };

    (
        status,
        Json(json!({
            "status": if degraded { "degraded" } else { "ok" },
            "service": state.config.service.name,
            "mode": state.config.mode,
            "routeCount": state.registry.routes().len(),
            "upstreams": state.config.upstreams,
            "upstreamChecks": upstream_checks,
            "dependencies": state.config.dependency_surfaces
        })),
    )
        .into_response()
}

async fn check_required_upstreams(state: &GatewayState) -> Vec<serde_json::Value> {
    let upstreams = state
        .config
        .upstreams
        .iter()
        .filter(|upstream| upstream.required)
        .cloned()
        .collect::<Vec<_>>();
    let mut checks = Vec::with_capacity(upstreams.len());

    for upstream in upstreams {
        let url = upstream_readiness_url(&upstream);
        let result = state
            .client
            .get(url.as_str())
            .timeout(Duration::from_millis(state.config.readiness.timeout_ms))
            .send()
            .await;

        let check = match result {
            Ok(response) if response.status().is_success() => json!({
                "serviceId": upstream.service_id,
                "status": "ok",
                "statusCode": response.status().as_u16(),
                "url": url
            }),
            Ok(response) => json!({
                "serviceId": upstream.service_id,
                "status": "unavailable",
                "statusCode": response.status().as_u16(),
                "url": url
            }),
            Err(error) => json!({
                "serviceId": upstream.service_id,
                "status": "unavailable",
                "error": error.to_string(),
                "url": url
            }),
        };
        checks.push(check);
    }

    checks
}

fn upstream_readiness_url(upstream: &ServiceUpstreamConfig) -> String {
    let path = upstream.readiness_path.trim();
    let path = if path.is_empty() {
        "/"
    } else if path.starts_with('/') {
        path
    } else {
        return format!(
            "{}/{}",
            upstream.base_url.trim_end_matches('/'),
            path.trim_start_matches('/')
        );
    };

    format!("{}{}", upstream.base_url.trim_end_matches('/'), path)
}
