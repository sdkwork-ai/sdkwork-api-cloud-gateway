use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use sdkwork_api_cloud_gateway_config::{GatewayPolicyConfig, GatewayPolicyRuleConfig};
use sdkwork_api_cloud_gateway_registry::RouteDescriptor;

#[derive(Clone, Debug, Default)]
pub(crate) struct PolicyDecision {
    max_body_bytes: Option<PolicyLimit<usize>>,
    dry_run_events: Vec<PolicyDryRunEvent>,
}

#[derive(Clone, Debug)]
struct PolicyLimit<T> {
    value: T,
    scope: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PolicyDryRunEvent {
    pub(crate) kind: &'static str,
    pub(crate) scope: String,
}

pub(crate) struct PolicyRejection {
    pub(crate) status: StatusCode,
    pub(crate) title: &'static str,
    pub(crate) detail: String,
    pub(crate) kind: &'static str,
    pub(crate) scope: String,
    headers: Vec<(HeaderName, HeaderValue)>,
}

pub(crate) fn evaluate_request(
    config: &GatewayPolicyConfig,
    route: &RouteDescriptor,
    method: &str,
    path: &str,
    headers: &HeaderMap,
) -> Result<PolicyDecision, PolicyRejection> {
    if !config.enabled {
        return Ok(PolicyDecision::default());
    }

    let matched_rules = config
        .rules
        .iter()
        .filter(|rule| rule_matches(rule, route, path))
        .collect::<Vec<_>>();

    let mut dry_run_events = Vec::new();
    for rule in &matched_rules {
        if rule.allowed_methods.is_empty() || method_allowed(rule, method) {
            continue;
        }

        if config.dry_run {
            dry_run_events.push(PolicyDryRunEvent {
                kind: "method_not_allowed",
                scope: rule.id.clone(),
            });
            continue;
        }

        let allow = allowed_methods_header(rule.allowed_methods.as_slice());
        return Err(PolicyRejection {
            status: StatusCode::METHOD_NOT_ALLOWED,
            title: "method_not_allowed",
            detail: format!(
                "gateway policy `{}` does not allow method `{method}`",
                rule.id
            ),
            kind: "method_not_allowed",
            scope: rule.id.clone(),
            headers: vec![(header::ALLOW, allow)],
        });
    }

    let max_body_bytes = matched_rules
        .iter()
        .filter_map(|rule| rule.max_body_bytes.map(|value| (value, rule.id.as_str())))
        .min_by_key(|(value, _scope)| *value)
        .map(|(value, scope)| PolicyLimit {
            value,
            scope: scope.to_string(),
        });
    if config.dry_run {
        if let Some(limit) = max_body_bytes.as_ref() {
            if content_length_exceeds(headers, limit.value) {
                dry_run_events.push(PolicyDryRunEvent {
                    kind: "request_body_too_large",
                    scope: limit.scope.clone(),
                });
            }
        }

        return Ok(PolicyDecision {
            max_body_bytes: None,
            dry_run_events,
        });
    }

    Ok(PolicyDecision {
        max_body_bytes,
        dry_run_events,
    })
}

impl PolicyDecision {
    pub(crate) fn max_body_bytes(&self, fallback: usize) -> usize {
        self.max_body_bytes
            .as_ref()
            .map(|limit| limit.value)
            .unwrap_or(fallback)
    }

    pub(crate) fn max_body_bytes_scope(&self) -> Option<&str> {
        self.max_body_bytes
            .as_ref()
            .map(|limit| limit.scope.as_str())
    }

    pub(crate) fn dry_run_events(&self) -> &[PolicyDryRunEvent] {
        self.dry_run_events.as_slice()
    }
}

impl PolicyRejection {
    pub(crate) fn apply_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in &self.headers {
            headers.insert(name.clone(), value.clone());
        }
    }
}

fn rule_matches(rule: &GatewayPolicyRuleConfig, route: &RouteDescriptor, path: &str) -> bool {
    let service_matches = rule
        .match_service_id
        .as_deref()
        .map(|expected| expected == route.service_id.as_str())
        .unwrap_or(true);
    let path_matches = rule
        .match_path_prefix
        .as_deref()
        .map(|prefix| {
            path == prefix
                || path.starts_with(format!("{}/", prefix.trim_end_matches('/')).as_str())
        })
        .unwrap_or(true);

    service_matches && path_matches
}

fn method_allowed(rule: &GatewayPolicyRuleConfig, method: &str) -> bool {
    rule.allowed_methods
        .iter()
        .any(|candidate| candidate == "*" || candidate.eq_ignore_ascii_case(method))
}

fn allowed_methods_header(methods: &[String]) -> HeaderValue {
    let value = methods
        .iter()
        .map(|method| method.trim().to_ascii_uppercase())
        .filter(|method| !method.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    HeaderValue::from_str(value.as_str()).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn content_length_exceeds(headers: &HeaderMap, max_body_bytes: usize) -> bool {
    headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|content_length| content_length > max_body_bytes as u64)
        .unwrap_or(false)
}
