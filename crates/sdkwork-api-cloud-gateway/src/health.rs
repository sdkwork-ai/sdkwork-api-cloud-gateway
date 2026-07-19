use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use sdkwork_database_sqlx::{
    process_shared_database_pool, process_shared_database_pool_enabled, PoolHealth,
};
use serde_json::json;

use crate::runtime::GatewayState;

pub(crate) async fn healthz() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "sdkwork-api-cloud-gateway"
    }))
}

pub(crate) async fn livez() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "sdkwork-api-cloud-gateway"
    }))
}

pub(crate) async fn readyz(State(state): State<Arc<GatewayState>>) -> Response {
    let pool_required = process_shared_database_pool_enabled();
    let pool_health = match process_shared_database_pool() {
        Some(pool) => Some(pool.health_check().await),
        None => None,
    };
    let status = readiness_status(pool_required, pool_health.as_ref());
    let database = database_readiness_payload(pool_required, pool_health.as_ref());

    (
        status,
        Json(json!({
            "status": if status == StatusCode::OK { "ok" } else { "unavailable" },
            "service": state.config.service.name,
            "mode": state.config.mode,
            "routeCount": state.registry.routes().len(),
            "dependencies": state.config.dependency_surfaces,
            "database": database
        })),
    )
        .into_response()
}

fn readiness_status(pool_required: bool, pool_health: Option<&PoolHealth>) -> StatusCode {
    if !pool_required || pool_health.is_some_and(PoolHealth::is_usable) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

fn database_readiness_payload(
    pool_required: bool,
    pool_health: Option<&PoolHealth>,
) -> serde_json::Value {
    match (pool_required, pool_health) {
        (false, _) => json!({ "status": "not-required" }),
        (true, None) => json!({ "status": "not-installed" }),
        (true, Some(health)) => json!({
            "status": health.status.label(),
            "engine": health.engine,
            "poolSize": health.pool_size,
            "idleConnections": health.idle_connections,
            "maxConnections": health.max_connections,
            "minConnections": health.min_connections,
            "latencyMs": health.latency_ms
        }),
    }
}

#[cfg(test)]
mod tests {
    use sdkwork_database_sqlx::{PoolHealth, PoolHealthStatus};

    use super::*;

    fn pool_health(status: PoolHealthStatus) -> PoolHealth {
        PoolHealth {
            status,
            engine: "postgres".to_owned(),
            pool_size: 1,
            idle_connections: 1,
            latency_ms: 1,
            max_connections: 4,
            min_connections: 0,
        }
    }

    #[test]
    fn readiness_fails_when_required_process_pool_is_not_installed() {
        assert_eq!(
            StatusCode::SERVICE_UNAVAILABLE,
            readiness_status(true, None)
        );
    }

    #[test]
    fn readiness_fails_when_required_process_pool_is_unhealthy() {
        let health = pool_health(PoolHealthStatus::Unhealthy("connection failed".to_owned()));
        assert_eq!(
            StatusCode::SERVICE_UNAVAILABLE,
            readiness_status(true, Some(&health))
        );
    }

    #[test]
    fn readiness_accepts_usable_or_non_required_process_pool() {
        let health = pool_health(PoolHealthStatus::Healthy);
        assert_eq!(StatusCode::OK, readiness_status(true, Some(&health)));
        assert_eq!(StatusCode::OK, readiness_status(false, None));
    }
}
