use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode},
    response::Response,
};
use sdkwork_api_cloud_gateway_config::GatewaySecurityConfig;
use sdkwork_web_core::registered_client_origins::is_registered_sdkwork_client_origin;

/// WAF JSON body inspection budget. Bodies larger than this are skipped by
/// the WAF stage and left to the request body limit enforcement.
const WAF_JSON_BODY_LIMIT: usize = 1024 * 1024;

#[derive(Debug)]
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

#[derive(Clone, Copy, Debug)]
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

    // Deployments that must stay reachable directly through the host IP (LAN
    // smoke checks, load-balancer health probes) accept raw IP literal Host
    // headers explicitly via SDKWORK_API_CLOUD_GATEWAY_SECURITY_ALLOW_IP_HOSTS;
    // registered domain names keep matching the allowlist as before.
    if config.allow_ip_hosts && is_ip_literal_host(&host) {
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

/// Reports whether the normalized host (ports already stripped) is a raw IP
/// literal. IPv6 keeps its bracket form after `normalize_host`, so the
/// brackets are removed before parsing.
fn is_ip_literal_host(host: &str) -> bool {
    let candidate = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);
    candidate.parse::<std::net::IpAddr>().is_ok()
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
    if is_registered_sdkwork_client_origin(origin) {
        return Ok(());
    }
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
/// Keep this list intentionally narrow: many `/app/v3/api/auth/*` and
/// `/app/v3/api/oauth/*` routes use the IAM app-api credential bootstrap
/// flow and legitimately carry `access-token`.
fn is_anonymous_auth_route(path: &str) -> bool {
    const ANONYMOUS_EXACT_PATHS: &[&str] = &["/app/v3/api/oauth/device_authorizations"];
    const ANONYMOUS_PATH_PREFIXES: &[&str] = &[
        "/app/v3/api/oauth/device_authorizations/",
        "/oauth/device_authorizations/",
    ];
    if ANONYMOUS_EXACT_PATHS
        .iter()
        .any(|candidate| path.eq_ignore_ascii_case(candidate))
    {
        return true;
    }
    ANONYMOUS_PATH_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        && (path.contains("/session_exchanges") || path.ends_with("/token"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_auth_session_routes_are_not_treated_as_anonymous() {
        for path in [
            "/app/v3/api/auth/sessions",
            "/app/v3/api/auth/sessions/login_context_selection",
            "/app/v3/api/auth/sessions/organization_selection",
            "/app/v3/api/auth/sessions/refresh",
            "/app/v3/api/oauth/sessions",
            "/app/v3/api/oauth/sessions/external",
            "/app/v3/api/oauth/callbacks/wechat",
        ] {
            assert!(
                !is_anonymous_auth_route(path),
                "expected bootstrap route `{path}` to allow credential headers"
            );
        }
    }

    #[test]
    fn anonymous_device_flow_routes_still_reject_credential_headers() {
        for path in [
            "/app/v3/api/oauth/device_authorizations",
            "/app/v3/api/oauth/device_authorizations/abc/session_exchanges",
        ] {
            assert!(
                is_anonymous_auth_route(path),
                "expected anonymous route `{path}` to reject credential headers"
            );
        }
    }

    #[test]
    fn csrf_guard_allows_registered_desktop_client_origins() {
        let config = GatewaySecurityConfig {
            csrf_guard_enabled: true,
            ..GatewaySecurityConfig::default()
        };
        let request = Request::builder()
            .method(Method::POST)
            .uri("/app/v3/api/auth/sessions")
            .header(header::HOST, "api-dev.birdcoder.com")
            .header(header::ORIGIN, "app://dsh")
            .header(header::COOKIE, "session=abc")
            .body(Body::empty())
            .expect("csrf request");

        validate_csrf(&config, &request).expect("registered desktop origin");
    }

    #[test]
    fn ip_literal_hosts_are_rejected_by_default_and_allowed_when_enabled() {
        let host_header = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, value.parse().expect("host header"));
            headers
        };
        let mut config = GatewaySecurityConfig {
            allowed_hosts: vec!["api-test.sdkwork.com".to_string()],
            ..GatewaySecurityConfig::default()
        };

        // Default posture: raw IP literals stay rejected while the registered
        // domain keeps matching the allowlist.
        for host in ["192.168.8.24:3911", "[fd00::5]:3911"] {
            assert!(
                validate_allowed_host(&config, &host_header(host)).is_err(),
                "IP literal `{host}` must stay rejected while allow_ip_hosts is off"
            );
        }
        assert!(validate_allowed_host(&config, &host_header("api-test.sdkwork.com:3911")).is_ok());

        // Explicit deploy-time switch: IPv4 and IPv6 literals pass, unknown
        // domains stay rejected.
        config.allow_ip_hosts = true;
        for host in [
            "192.168.8.24",
            "192.168.8.24:3911",
            "[fd00::5]:3911",
            "127.0.0.1",
        ] {
            assert!(
                validate_allowed_host(&config, &host_header(host)).is_ok(),
                "IP literal `{host}` must be allowed with allow_ip_hosts on"
            );
        }
        assert!(
            validate_allowed_host(&config, &host_header("evil.example.com")).is_err(),
            "non-IP host outside the allowlist must stay rejected"
        );
    }

    #[test]
    fn ip_literal_host_detection_covers_v4_v6_and_names() {
        assert!(is_ip_literal_host("192.168.8.24"));
        assert!(is_ip_literal_host("127.0.0.1"));
        assert!(is_ip_literal_host("[::1]"));
        assert!(is_ip_literal_host("[fd00::5]"));
        assert!(!is_ip_literal_host("api-test.sdkwork.com"));
        assert!(!is_ip_literal_host("localhost"));
        assert!(!is_ip_literal_host("999.1.1.1"));
    }
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
