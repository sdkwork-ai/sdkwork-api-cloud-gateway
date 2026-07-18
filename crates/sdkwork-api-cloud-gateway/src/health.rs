use std::sync::Arc;

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
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
    (
        axum::http::StatusCode::OK,
        Json(json!({
            "status": "ok",
            "service": state.config.service.name,
            "mode": state.config.mode,
            "routeCount": state.registry.routes().len(),
            "dependencies": state.config.dependency_surfaces
        })),
    )
        .into_response()
}
