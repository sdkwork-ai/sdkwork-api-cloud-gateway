use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode},
    response::Response,
};
use sdkwork_api_cloud_gateway_config::GatewaySecurityConfig;

/// WAF JSON body inspection budget. Bodies larger than this are skipped by
/// the WAF stage and left to the request body limit enforcement.
const WAF_JSON_BODY_LIMIT: usize = 1024 * 1024;

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
    validate_csrf(config, request)?;
    validate_credential_headers_on_anonymous_routes(config, request)?;
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
    framework_handles_baseline: bool,
    mut response: Response,
) -> Response {
    if !config.secure_headers_enabled {
        return response;
    }

    let headers = response.headers_mut();
    if !framework_handles_baseline {
        insert_if_absent(headers, "x-content-type-options", "nosniff");
        insert_if_absent(headers, "x-frame-options", "DENY");
        insert_if_absent(headers, "referrer-policy", "no-referrer");
        insert_if_absent(
            headers,
            "permissions-policy",
            "camera=(), microphone=(), geolocation=()",
        );
        insert_if_absent(
            headers,
            "strict-transport-security",
            "max-age=31536000; includeSubDomains",
        );
    }
    insert_if_absent(headers, "x-permitted-cross-domain-policies", "none");
    insert_if_absent(
        headers,
        "content-security-policy",
        "default-src 'none'; frame-ancestors 'none'",
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

/// Same-origin CSRF guard for state-changing requests carrying a session
/// cookie (config `csrfGuardEnabled`).
///
/// Browsers attach cookies automatically; the guard requires an explicit
/// `Origin` that matches the request `Host` (or a modern `Sec-Fetch-Site`
/// assertion of same-origin / none). Non-browser clients without cookies are
/// not subject to CSRF and pass through.
fn validate_csrf(
    config: &GatewaySecurityConfig,
    request: &Request<Body>,
) -> Result<(), SecurityRejection> {
    if !config.csrf_guard_enabled {
        return Ok(());
    }
    if !is_state_changing_method(request.method()) {
        return Ok(());
    }
    if !request.headers().contains_key(header::COOKIE) {
        return Ok(());
    }
    if sec_fetch_site_allows(request.headers()) {
        return Ok(());
    }
    let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(csrf_rejection(
            "csrf_origin_required",
            "state-changing request with a session cookie must declare an Origin header",
        ));
    };
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !origin_host_matches(origin, host) {
        return Err(csrf_rejection(
            "csrf_origin_mismatch",
            "Origin does not match the gateway Host for a state-changing request",
        ));
    }
    Ok(())
}

/// Reject credential headers on anonymous auth entry points when
/// `rejectCredentialHeadersOnAnonymousAuthRoutes` is enabled. Anonymous
/// routes must not carry session/API credentials; forwarding them risks
/// credential leakage to downstream services and silent auth confusion.
fn validate_credential_headers_on_anonymous_routes(
    config: &GatewaySecurityConfig,
    request: &Request<Body>,
) -> Result<(), SecurityRejection> {
    if !config.reject_credential_headers_on_anonymous_auth_routes {
        return Ok(());
    }
    if !is_anonymous_auth_route(request.uri().path()) {
        return Ok(());
    }
    let credential_headers = [
        header::AUTHORIZATION,
        HeaderName::from_static("access-token"),
        header::COOKIE,
        HeaderName::from_static("x-api-key"),
    ];
    if let Some(name) = credential_headers
        .iter()
        .find(|name| request.headers().contains_key(*name))
    {
        return Err(SecurityRejection {
            status: StatusCode::BAD_REQUEST,
            title: "credential_headers_not_allowed_on_anonymous_route",
            detail: format!("anonymous auth route must not carry credential header `{name}`"),
            headers: Vec::new(),
            events: Vec::new(),
        });
    }
    Ok(())
}

fn csrf_rejection(title: &'static str, detail: &str) -> SecurityRejection {
    SecurityRejection {
        status: StatusCode::FORBIDDEN,
        title,
        detail: detail.to_string(),
        headers: Vec::new(),
        events: Vec::new(),
    }
}

fn is_state_changing_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

fn sec_fetch_site_allows(headers: &HeaderMap) -> bool {
    headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "same-origin" | "none"
            )
        })
        .unwrap_or(false)
}

fn origin_host_matches(origin: &str, host: &str) -> bool {
    let origin_host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
        .and_then(|authority| authority.split_once('/').map(|(host, _)| host))
        .unwrap_or(origin);
    origin_host.eq_ignore_ascii_case(host.trim())
}

/// Anonymous auth entry points where credential headers are rejected.
/// Matches surface-agnostic `/auth`, `/oauth`, and account-recovery paths.
fn is_anonymous_auth_route(path: &str) -> bool {
    const ANONYMOUS_SEGMENTS: &[&str] = &[
        "/auth/",
        "/oauth/",
        "/login",
        "/register",
        "/signup",
        "/signin",
        "/verify",
        "/verify-email",
        "/verify_phone",
        "/forgot-password",
        "/reset-password",
        "/refresh-token",
        "/password-reset",
        "/magic-link",
    ];
    ANONYMOUS_SEGMENTS
        .iter()
        .any(|segment| path.contains(segment))
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

/// Inspect JSON request bodies against the WAF baseline signature when
/// `waf.inspectJsonBody` is enabled (config `waf.inspectJsonBody`).
///
/// The body is buffered up to [`WAF_JSON_BODY_LIMIT`] and recursively scanned
/// for signature matches. In blocking mode the request is rejected; in
/// monitor mode a WAF event is returned so the middleware can record it.
pub(crate) async fn inspect_json_body(
    config: &GatewaySecurityConfig,
    request: Request<Body>,
) -> Result<(Request<Body>, Vec<SecurityEvent>), SecurityRejection> {
    if !config.waf.enabled || !config.waf.inspect_json_body {
        return Ok((request, Vec::new()));
    }
    if matches!(
        request.method(),
        &Method::GET | &Method::HEAD | &Method::OPTIONS
    ) {
        return Ok((request, Vec::new()));
    }
    let is_json = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|content_type| {
            content_type
                .split(';')
                .next()
                .map(str::trim)
                .is_some_and(|media| media.eq_ignore_ascii_case("application/json"))
        })
        .unwrap_or(false);
    if !is_json {
        return Ok((request, Vec::new()));
    }

    let (parts, body) = request.into_parts();
    let bytes = match to_bytes(body, WAF_JSON_BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(error) => {
            // Bodies beyond the WAF budget are handled by the request body
            // limit enforcement downstream; do not fail the request here.
            let _ = error;
            return Ok((Request::from_parts(parts, Body::empty()), Vec::new()));
        }
    };
    let matched = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map(|value| json_value_matches(&value))
        .unwrap_or(false);
    let request = Request::from_parts(parts, Body::from(bytes));
    if !matched {
        return Ok((request, Vec::new()));
    }

    if is_waf_blocking_mode(config.waf.mode.as_str()) {
        return Err(SecurityRejection {
            status: StatusCode::FORBIDDEN,
            title: "waf_request_blocked",
            detail: "request body matched a gateway WAF baseline signature".to_string(),
            headers: waf_headers("block"),
            events: vec![waf_event("block")],
        });
    }

    Ok((request, vec![waf_event("monitor")]))
}

fn json_value_matches(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => waf_value_matches(text.as_str()),
        serde_json::Value::Array(items) => items.iter().any(json_value_matches),
        serde_json::Value::Object(map) => map.values().any(json_value_matches),
        _ => false,
    }
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
