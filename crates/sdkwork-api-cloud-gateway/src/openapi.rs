use std::sync::Arc;

use axum::{extract::State, Json};
use serde_json::json;

use crate::runtime::GatewayState;

pub(crate) async fn openapi_index(
    State(state): State<Arc<GatewayState>>,
) -> Json<serde_json::Value> {
    Json(json!({
        "service": state.config.service.name,
        "mode": state.config.mode,
        "routes": state.registry.routes(),
        "dependencies": state.config.dependency_surfaces
    }))
}

pub(crate) async fn openapi_document(
    State(state): State<Arc<GatewayState>>,
) -> Json<serde_json::Value> {
    Json(json!({
        "openapi": "3.1.0",
        "info": {
            "title": "SDKWork API Gateway",
            "version": env!("CARGO_PKG_VERSION")
        },
        "paths": {
            "/healthz": {
                "get": {
                    "operationId": "getGatewayHealth",
                    "x-sdkwork-request-context": "WebRequestContext",
                    "x-sdkwork-api-surface": "gateway-api",
                    "x-sdkwork-auth-mode": "anonymous",
                    "security": [],
                    "responses": {
                        "200": { "description": "Gateway health" }
                    }
                }
            },
            "/readyz": {
                "get": {
                    "operationId": "getGatewayReadiness",
                    "x-sdkwork-request-context": "WebRequestContext",
                    "x-sdkwork-api-surface": "gateway-api",
                    "x-sdkwork-auth-mode": "anonymous",
                    "security": [],
                    "responses": {
                        "200": { "description": "Gateway readiness" }
                    }
                }
            },
            "/openapi/index.json": {
                "get": {
                    "operationId": "listGatewayApiDependencies",
                    "x-sdkwork-request-context": "WebRequestContext",
                    "x-sdkwork-api-surface": "gateway-api",
                    "x-sdkwork-auth-mode": "anonymous",
                    "security": [],
                    "responses": {
                        "200": { "description": "Gateway route and dependency index" }
                    }
                }
            },
            "/openapi.json": {
                "get": {
                    "operationId": "getGatewayOpenApiDocument",
                    "x-sdkwork-request-context": "WebRequestContext",
                    "x-sdkwork-api-surface": "gateway-api",
                    "x-sdkwork-auth-mode": "anonymous",
                    "security": [],
                    "responses": {
                        "200": { "description": "Gateway OpenAPI document" }
                    }
                }
            }
        },
        "x-sdkwork-gateway": {
            "mode": state.config.mode,
            "routes": state.registry.routes(),
            "dependencies": state.config.dependency_surfaces
        }
    }))
}
