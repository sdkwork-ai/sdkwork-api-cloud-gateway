//! Observability helpers for the SDKWork API Gateway.

use std::collections::BTreeMap;

use sdkwork_api_gateway_config::{GatewayMode, GatewayRuntimeConfig};
use sdkwork_api_gateway_registry::GatewayRouteRegistry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayStartupSummary {
    pub service_name: String,
    pub environment: String,
    pub bind: String,
    pub mode: GatewayMode,
    pub upstream_count: usize,
    pub route_count: usize,
    pub dependency_services: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayAccessLogEvent {
    pub service_name: String,
    pub environment: String,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub route_target: String,
    pub service_id: Option<String>,
    pub failure_class: Option<String>,
    pub request_id: Option<String>,
    pub traceparent: Option<String>,
    pub headers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayAuditLogEvent {
    pub service_name: String,
    pub environment: String,
    pub category: String,
    pub action: String,
    pub reason: String,
    pub scope: Option<String>,
    pub status: u16,
    pub method: String,
    pub route_target: String,
    pub service_id: Option<String>,
    pub request_id: Option<String>,
    pub traceparent: Option<String>,
}

pub struct GatewayAccessLogInput<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub status: u16,
    pub route_target: &'a str,
    pub service_id: Option<&'a str>,
    pub failure_class: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub traceparent: Option<&'a str>,
    pub headers: Vec<(&'a str, &'a str)>,
}

pub struct GatewayAuditLogInput<'a> {
    pub category: &'a str,
    pub action: &'a str,
    pub reason: &'a str,
    pub scope: Option<&'a str>,
    pub status: u16,
    pub method: &'a str,
    pub route_target: &'a str,
    pub service_id: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub traceparent: Option<&'a str>,
}

pub fn describe_gateway_startup(
    config: &GatewayRuntimeConfig,
    registry: &GatewayRouteRegistry,
) -> GatewayStartupSummary {
    let mut dependency_services = config
        .dependency_surfaces
        .iter()
        .map(|surface| surface.service_id.clone())
        .collect::<Vec<_>>();
    dependency_services.sort();
    dependency_services.dedup();

    GatewayStartupSummary {
        service_name: config.service.name.clone(),
        environment: config.service.environment.clone(),
        bind: config.server.bind.clone(),
        mode: config.mode,
        upstream_count: config.upstreams.len(),
        route_count: registry.routes().len(),
        dependency_services,
    }
}

pub fn build_gateway_access_log_event(
    config: &GatewayRuntimeConfig,
    input: GatewayAccessLogInput<'_>,
) -> GatewayAccessLogEvent {
    let redact_headers = config
        .observability
        .redact_headers
        .iter()
        .map(|header| header.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let headers = input
        .headers
        .into_iter()
        .map(|(name, value)| {
            let name = name.to_ascii_lowercase();
            let value = if redact_headers.iter().any(|header| header == &name) {
                "[redacted]".to_string()
            } else {
                value.to_string()
            };
            (name, value)
        })
        .collect::<BTreeMap<_, _>>();

    GatewayAccessLogEvent {
        service_name: config.service.name.clone(),
        environment: config.service.environment.clone(),
        method: input.method.to_ascii_uppercase(),
        path: input.path.to_string(),
        status: input.status,
        route_target: input.route_target.to_string(),
        service_id: input.service_id.map(ToOwned::to_owned),
        failure_class: input.failure_class.map(ToOwned::to_owned),
        request_id: input.request_id.map(ToOwned::to_owned),
        traceparent: input.traceparent.map(ToOwned::to_owned),
        headers,
    }
}

pub fn build_gateway_audit_log_event(
    config: &GatewayRuntimeConfig,
    input: GatewayAuditLogInput<'_>,
) -> GatewayAuditLogEvent {
    GatewayAuditLogEvent {
        service_name: config.service.name.clone(),
        environment: config.service.environment.clone(),
        category: input.category.to_string(),
        action: input.action.to_string(),
        reason: input.reason.to_string(),
        scope: input.scope.map(ToOwned::to_owned),
        status: input.status,
        method: input.method.to_ascii_uppercase(),
        route_target: input.route_target.to_string(),
        service_id: input.service_id.map(ToOwned::to_owned),
        request_id: input.request_id.map(ToOwned::to_owned),
        traceparent: input.traceparent.map(ToOwned::to_owned),
    }
}
