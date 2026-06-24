use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_api_cloud_gateway_observability::{
    build_gateway_access_log_event, build_gateway_audit_log_event, GatewayAccessLogInput,
    GatewayAuditLogInput,
};

#[test]
fn access_log_event_redacts_configured_headers_and_keeps_trace_context() {
    let mut config = GatewayRuntimeConfig::default();
    config.observability.redact_headers = vec![
        "authorization".to_string(),
        "cookie".to_string(),
        "x-api-key".to_string(),
    ];

    let event = build_gateway_access_log_event(
        &config,
        GatewayAccessLogInput {
            method: "POST",
            path: "/app/v3/api/auth/sessions",
            status: 201,
            route_target: "dependency",
            service_id: Some("sdkwork-appbase-app-api"),
            failure_class: Some("upstream_timeout"),
            request_id: Some("request-1"),
            traceparent: Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"),
            headers: vec![
                ("authorization", "Bearer secret"),
                ("content-type", "application/json"),
                ("x-api-key", "secret-key"),
            ],
        },
    );

    assert_eq!(event.service_name, "sdkwork-api-cloud-gateway");
    assert_eq!(event.method, "POST");
    assert_eq!(event.status, 201);
    assert_eq!(event.service_id.as_deref(), Some("sdkwork-appbase-app-api"));
    assert_eq!(event.failure_class.as_deref(), Some("upstream_timeout"));
    assert_eq!(event.request_id.as_deref(), Some("request-1"));
    assert_eq!(
        event.traceparent.as_deref(),
        Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01")
    );
    assert_eq!(
        event.headers.get("authorization").map(String::as_str),
        Some("[redacted]")
    );
    assert_eq!(
        event.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        event.headers.get("x-api-key").map(String::as_str),
        Some("[redacted]")
    );
}

#[test]
fn audit_log_event_uses_bounded_security_and_traffic_fields() {
    let config = GatewayRuntimeConfig::default();

    let event = build_gateway_audit_log_event(
        &config,
        GatewayAuditLogInput {
            category: "security",
            action: "block",
            reason: "waf_baseline_signature",
            scope: Some("baseline-signature"),
            status: 403,
            method: "POST",
            route_target: "dependency",
            service_id: Some("sdkwork-appbase-app-api"),
            request_id: Some("request-2"),
            traceparent: Some("00-4bf92f3577b34da6a3ce929d0e0e4736-1111111111111111-01"),
        },
    );

    assert_eq!(event.service_name, "sdkwork-api-cloud-gateway");
    assert_eq!(event.environment, "development");
    assert_eq!(event.category, "security");
    assert_eq!(event.action, "block");
    assert_eq!(event.reason, "waf_baseline_signature");
    assert_eq!(event.scope.as_deref(), Some("baseline-signature"));
    assert_eq!(event.status, 403);
    assert_eq!(event.method, "POST");
    assert_eq!(event.route_target, "dependency");
    assert_eq!(event.service_id.as_deref(), Some("sdkwork-appbase-app-api"));
    assert_eq!(event.request_id.as_deref(), Some("request-2"));
}
