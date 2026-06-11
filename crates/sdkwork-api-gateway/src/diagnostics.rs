use std::sync::Arc;

use axum::{extract::State, Json};
use serde_json::json;

use crate::runtime::GatewayState;

pub(crate) async fn diagnostics_endpoint(
    State(state): State<Arc<GatewayState>>,
) -> Json<serde_json::Value> {
    Json(diagnostics_snapshot(state.as_ref()))
}

fn diagnostics_snapshot(state: &GatewayState) -> serde_json::Value {
    let config = &state.config;
    let mut snapshot = serde_json::Map::new();

    snapshot.insert(
        "service".to_string(),
        json!({
            "name": config.service.name,
            "environment": config.service.environment,
            "mode": config.mode,
        }),
    );
    snapshot.insert(
        "dependencySurfaces".to_string(),
        json!({
            "count": config.dependency_surfaces.len(),
        }),
    );

    let route_items = if config.diagnostics.include_route_inventory {
        state
            .registry
            .routes()
            .iter()
            .map(|route| {
                json!({
                    "serviceId": route.service_id,
                    "pathPattern": route.path_pattern,
                    "apiPrefix": route.api_prefix,
                    "surface": route.surface,
                    "executionTarget": route.execution_target,
                    "methods": route.methods,
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    snapshot.insert(
        "routes".to_string(),
        json!({
            "count": state.registry.routes().len(),
            "items": route_items,
        }),
    );

    let policy_rules = if config.diagnostics.include_policy_inventory {
        config
            .policy
            .rules
            .iter()
            .map(|rule| {
                json!({
                    "id": rule.id,
                    "matchServiceId": rule.match_service_id,
                    "matchPathPrefix": rule.match_path_prefix,
                    "allowedMethods": rule.allowed_methods,
                    "hasMaxBodyBytes": rule.max_body_bytes.is_some(),
                    "hasTimeoutMs": rule.timeout_ms.is_some(),
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    snapshot.insert(
        "policy".to_string(),
        json!({
            "enabled": config.policy.enabled,
            "dryRun": config.policy.dry_run,
            "ruleCount": config.policy.rules.len(),
            "rules": policy_rules,
        }),
    );

    serde_json::Value::Object(snapshot)
}
