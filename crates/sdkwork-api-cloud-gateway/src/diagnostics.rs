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

    let cors_metrics = state.cors.metrics();
    let cors_snapshot = state.cors.snapshot();
    snapshot.insert(
        "cors".to_string(),
        json!({
            "dynamicEnabled": config.cors.dynamic.enabled,
            "refreshIntervalSeconds": config.cors.dynamic.refresh_interval_seconds,
            "tenantId": config.cors.dynamic.tenant_id,
            "appliedVersion": cors_metrics.applied_version,
            "dynamicOriginCount": cors_metrics.dynamic_origins,
            "refreshAttempts": cors_metrics.attempts,
            "refreshFailures": cors_metrics.failures,
            "staticOriginCount": cors_snapshot.policy().allowed_origins.len()
                .saturating_sub(cors_metrics.dynamic_origins),
            "registeredConsoleHosts": cors_snapshot.policy().registered_console_hosts.is_some(),
            "registeredSubdomainHosts": cors_snapshot.policy().registered_subdomain_hosts.is_some(),
        }),
    );

    serde_json::Value::Object(snapshot)
}
