use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, HeaderValue, Request, StatusCode},
    response::Response,
};
use sdkwork_api_gateway_config::GatewaySecurityConfig;

pub(crate) struct SecurityRejection {
    pub(crate) status: StatusCode,
    pub(crate) title: &'static str,
    pub(crate) detail: String,
    headers: Vec<(HeaderName, HeaderValue)>,
    events: Vec<SecurityEvent>,
}

pub(crate) struct SecurityInspection {
    headers: Vec<(HeaderName, HeaderValue)>,
    events: Vec<SecurityEvent>,
}

#[derive(Clone, Copy)]
pub(crate) struct SecurityEvent {
    pub(crate) kind: &'static str,
    pub(crate) action: &'static str,
    pub(crate) rule: &'static str,
}

pub(crate) fn validate_request(
    config: &GatewaySecurityConfig,
    request: &Request<Body>,
) -> Result<SecurityInspection, SecurityRejection> {
    validate_header_limits(config, request.headers())?;
    validate_allowed_host(config, request.headers())?;
    let mut inspection = SecurityInspection {
        headers: Vec::new(),
        events: Vec::new(),
    };
    if inspect_waf(config, request)? {
        inspection.headers.extend(waf_headers("monitor"));
        inspection.events.push(waf_event("monitor"));
    }
    Ok(inspection)
}

impl SecurityInspection {
    pub(crate) fn apply_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in &self.headers {
            headers.insert(name.clone(), value.clone());
        }
    }

    pub(crate) fn events(&self) -> &[SecurityEvent] {
        self.events.as_slice()
    }
}

impl SecurityRejection {
    pub(crate) fn apply_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in &self.headers {
            headers.insert(name.clone(), value.clone());
        }
    }

    pub(crate) fn events(&self) -> &[SecurityEvent] {
        self.events.as_slice()
    }
}

pub(crate) fn apply_secure_response_headers(
    config: &GatewaySecurityConfig,
    mut response: Response,
) -> Response {
    if !config.secure_headers_enabled {
        return response;
    }

    let headers = response.headers_mut();
    insert_if_absent(headers, "x-content-type-options", "nosniff");
    insert_if_absent(headers, "x-frame-options", "DENY");
    insert_if_absent(headers, "referrer-policy", "no-referrer");
    insert_if_absent(
        headers,
        "permissions-policy",
        "camera=(), microphone=(), geolocation=()",
    );
    insert_if_absent(headers, "x-permitted-cross-domain-policies", "none");
    insert_if_absent(
        headers,
        "content-security-policy",
        "default-src 'none'; frame-ancestors 'none'",
    );
    insert_if_absent(
        headers,
        "strict-transport-security",
        "max-age=31536000; includeSubDomains",
    );
    response
}

fn validate_allowed_host(
    config: &GatewaySecurityConfig,
    headers: &HeaderMap,
) -> Result<(), SecurityRejection> {
    if config.allowed_hosts.is_empty() {
        return Ok(());
    }

    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(SecurityRejection {
            status: StatusCode::MISDIRECTED_REQUEST,
            title: "host_not_allowed",
            detail: "request host is required by gateway host allowlist".to_string(),
            headers: Vec::new(),
            events: Vec::new(),
        });
    };

    let host = normalize_host(host);
    let allowed = config
        .allowed_hosts
        .iter()
        .any(|allowed_host| normalize_host(allowed_host) == host);
    if allowed {
        return Ok(());
    }

    Err(SecurityRejection {
        status: StatusCode::MISDIRECTED_REQUEST,
        title: "host_not_allowed",
        detail: "request host is not allowed by gateway host allowlist".to_string(),
        headers: Vec::new(),
        events: Vec::new(),
    })
}

fn validate_header_limits(
    config: &GatewaySecurityConfig,
    headers: &HeaderMap,
) -> Result<(), SecurityRejection> {
    if headers.len() > config.max_header_count {
        return Err(SecurityRejection {
            status: StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            title: "request_headers_too_large",
            detail: "request header count exceeded configured gateway limit".to_string(),
            headers: Vec::new(),
            events: Vec::new(),
        });
    }

    let header_bytes = headers
        .iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
        .sum::<usize>();
    if header_bytes > config.max_header_bytes {
        return Err(SecurityRejection {
            status: StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            title: "request_headers_too_large",
            detail: "request header bytes exceeded configured gateway limit".to_string(),
            headers: Vec::new(),
            events: Vec::new(),
        });
    }

    Ok(())
}

fn inspect_waf(
    config: &GatewaySecurityConfig,
    request: &Request<Body>,
) -> Result<bool, SecurityRejection> {
    if !config.waf.enabled {
        return Ok(false);
    }

    let matched_query = if config.waf.inspect_query {
        request.uri().query().is_some_and(waf_value_matches)
    } else {
        false
    };
    let matched_headers = if config.waf.inspect_headers {
        request
            .headers()
            .iter()
            .any(|(_name, value)| value.to_str().is_ok_and(waf_value_matches))
    } else {
        false
    };

    if !(matched_query || matched_headers) {
        return Ok(false);
    }

    if is_waf_blocking_mode(config.waf.mode.as_str()) {
        return Err(SecurityRejection {
            status: StatusCode::FORBIDDEN,
            title: "waf_request_blocked",
            detail: "request matched a gateway WAF baseline signature".to_string(),
            headers: waf_headers("block"),
            events: vec![waf_event("block")],
        });
    }

    Ok(true)
}

fn waf_event(action: &'static str) -> SecurityEvent {
    SecurityEvent {
        kind: "waf",
        action,
        rule: "baseline-signature",
    }
}

fn waf_headers(action: &'static str) -> Vec<(HeaderName, HeaderValue)> {
    vec![
        (
            HeaderName::from_static("x-sdkwork-waf-action"),
            HeaderValue::from_static(action),
        ),
        (
            HeaderName::from_static("x-sdkwork-waf-rule"),
            HeaderValue::from_static("baseline-signature"),
        ),
    ]
}

fn is_waf_blocking_mode(mode: &str) -> bool {
    matches!(
        mode.trim().to_ascii_lowercase().as_str(),
        "enforce" | "block" | "blocking" | "prevent"
    )
}

fn waf_value_matches(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    let normalized = value.replace('+', " ");
    normalized.contains("<script")
        || normalized.contains("%3cscript")
        || normalized.contains("javascript:")
        || normalized.contains("union select")
        || (normalized.contains(" or ") && normalized.contains("1=1"))
        || normalized.contains("' or 1=1")
        || normalized.contains("\" or 1=1")
        || normalized.contains("../")
        || normalized.contains("%2e%2e%2f")
        || normalized.contains("/etc/passwd")
}

fn insert_if_absent(headers: &mut HeaderMap, name: &'static str, value: &'static str) {
    if headers.contains_key(name) {
        return;
    }
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_static(value),
    );
}

fn normalize_host(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if let Some(end) = value
        .strip_prefix('[')
        .and_then(|rest| rest.find(']').map(|index| index + 1))
    {
        return value[..=end].to_string();
    }
    value
        .split_once(':')
        .map(|(host, _)| host)
        .unwrap_or(value.as_str())
        .to_string()
}
