use std::net::{IpAddr, SocketAddr};

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
        .chain(config.own_hosts.iter())
        .any(|allowed_host| normalize_host(allowed_host) == host);
    if allowed {
        return Ok(());
    }

    // Deployments that must stay reachable directly through the host IP (LAN
    // smoke checks, load-balancer health probes) accept raw IP literal Host
    // headers explicitly via SDKWORK_API_CLOUD_GATEWAY_SECURITY_ALLOW_IP_HOSTS;
    // registered domain names keep matching the allowlist as before. Hosts that
    // name this process's own listener are accepted independently of that
    // switch (`record_own_hosts`).
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

/// Records this process's own published hosts as accepted `Host` values.
///
/// The gateway is a composition root: embedded consumers resolve peer surfaces
/// through the HTTP service registry and call this process back over the
/// advertised private endpoint, so that request's `Host` is an IP literal
/// (`172.18.0.5:8080`) that the operator allowlist and the production
/// `allowIpHosts=false` posture reject. Those hosts name the socket the process
/// itself owns, so accepting them restores in-process traffic without widening
/// the caller policy that `allowed_hosts` enforces.
pub(crate) fn record_own_hosts(
    config: &mut GatewaySecurityConfig,
    local_addr: SocketAddr,
    advertised_origin: &str,
) {
    for host in own_hosts_for(local_addr, advertised_origin) {
        if !config.own_hosts.contains(&host) {
            config.own_hosts.push(host);
        }
    }
}

/// Hosts that unambiguously name this process's own listener: the advertised
/// registry origin, the concrete bind address and both loopback literals.
fn own_hosts_for(local_addr: SocketAddr, advertised_origin: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    if let Some(host) = origin_host(advertised_origin) {
        hosts.push(host);
    }
    match local_addr.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => hosts.push(ip.to_string()),
        IpAddr::V6(ip) if !ip.is_unspecified() => hosts.push(format!("[{ip}]")),
        _ => {}
    }
    hosts.push("127.0.0.1".to_string());
    hosts.push("[::1]".to_string());
    hosts.sort();
    hosts.dedup();
    hosts
}

/// Normalized host of an absolute `http(s)` origin, a bare `host:port`
/// authority, or a bracketed IPv6 literal.
fn origin_host(origin: &str) -> Option<String> {
    let trimmed = origin.trim();
    let authority = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed)
        .split('/')
        .next()
        .unwrap_or_default()
        .trim();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_userinfo, host)| host);
    let host = normalize_host(authority);
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
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

/// Reject caller-attached credential headers on anonymous auth entry points
/// when `rejectCredentialHeadersOnAnonymousAuthRoutes` is enabled. Anonymous
/// routes cannot act on a credential, so forwarding one risks credential
/// leakage to downstream services and silent auth confusion.
///
/// `Cookie` is intentionally NOT in this set. A same-origin browser `fetch`
/// always carries every cookie stored for the origin, and the generated SDKs
/// never suppress it, so rejecting `Cookie` here turns a normal browser
/// device/QR login (`POST /app/v3/api/oauth/device_authorizations`) into a
/// `400` for any visitor who happens to hold an unrelated session cookie —
/// including the desktop/web clients that call this gateway. Cookies are
/// ambient browser state, not a credential the calling code attaches; the
/// cookie-bearing state-changing path stays governed by `validate_csrf`.
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

/// Model-inference surfaces carry the caller's prompt, not request parameters.
///
/// Their JSON strings are conversation content: a body containing `../`,
/// `/etc/passwd`, `union select` or `<script` is normally the *subject* of the
/// conversation — a code review, an incident write-up, a security question —
/// not an attack on this gateway. Signature-matching prompt bodies therefore
/// rejects ordinary chat, which is what production reports as
/// `waf_baseline_signature` 403 on `POST /v1/chat/completions`.
///
/// Body inspection stops at the door for these routes. Header, query and URI
/// inspection still apply, and auth, body-size limits, routing and billing are
/// untouched: the gateway still never forwards a prompt to an upstream it may
/// not use.
fn is_prompt_inference_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    matches!(
        path,
        "/v1/chat/completions"
            | "/v1/completions"
            | "/v1/responses"
            | "/v1/embeddings"
            | "/v1/images/generations"
            | "/v1/audio/speech"
            | "/anthropic/v1/messages"
    ) || path.ends_with(":generateContent")
        || path.ends_with(":streamGenerateContent")
}

/// Inspect JSON request bodies against the WAF baseline signature when
/// `waf.inspectJsonBody` is enabled (config `waf.inspectJsonBody`).
///
/// Bodies within [`WAF_JSON_BODY_LIMIT`] are buffered and recursively scanned
/// for signature matches. A larger body is skipped by the WAF stage and left to
/// the request body limit downstream, but it is **never truncated**: the handler
/// must receive exactly the bytes the client sent (media generation payloads
/// carry inline images well past one megabyte). In blocking mode a matched
/// request is rejected; in monitor mode a WAF event is returned so the
/// middleware can record it.
///
/// `max_body_bytes` is the gateway request cap (`request.maxBodyBytes`). It
/// bounds the buffer used for a streamed body with no declared length; a body
/// beyond it is rejected here with the same problem the downstream body-limit
/// enforcement would produce, because consuming it would leave the handler with
/// an empty body.
pub(crate) async fn inspect_json_body(
    config: &GatewaySecurityConfig,
    request: Request<Body>,
    max_body_bytes: usize,
) -> Result<(Request<Body>, Vec<SecurityEvent>), SecurityRejection> {
    if !config.waf.enabled || !config.waf.inspect_json_body {
        return Ok((request, Vec::new()));
    }
    // A chat prompt is data the caller wants analysed, not input to this
    // service; scanning it for attack signatures blocks legitimate traffic.
    if is_prompt_inference_path(request.uri().path()) {
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

    // A declared length beyond the inspection budget is skipped without reading
    // the stream, so oversized payloads cost no WAF buffer at all.
    let declared_length = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<usize>().ok());
    if declared_length.is_some_and(|length| length > WAF_JSON_BODY_LIMIT) {
        return Ok((request, Vec::new()));
    }

    let (parts, body) = request.into_parts();
    let bytes = match to_bytes(body, max_body_bytes).await {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = error;
            return Err(SecurityRejection {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                title: "request_body_too_large",
                detail: "request body exceeded configured gateway limit".to_string(),
                headers: Vec::new(),
                events: Vec::new(),
            });
        }
    };
    // Streamed bodies (chunked, or a client that understated Content-Length) can
    // only be measured by buffering; anything past the inspection budget is
    // forwarded uninspected rather than dropped.
    let matched = bytes.len() <= WAF_JSON_BODY_LIMIT
        && serde_json::from_slice::<serde_json::Value>(&bytes)
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
                "expected anonymous route `{path}` to be subject to the credential-header rule"
            );
        }
    }

    fn anonymous_route_request(
        header_name: HeaderName,
        header_value: &'static str,
    ) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri("/app/v3/api/oauth/device_authorizations")
            .header(header_name, header_value)
            .body(Body::empty())
            .expect("anonymous device authorization request")
    }

    #[test]
    fn anonymous_device_flow_routes_reject_caller_attached_credentials() {
        let config = GatewaySecurityConfig {
            reject_credential_headers_on_anonymous_auth_routes: true,
            ..GatewaySecurityConfig::default()
        };

        for (name, value) in [
            (header::AUTHORIZATION, "Bearer stale-session-token"),
            (
                HeaderName::from_static("access-token"),
                "stale-access-token",
            ),
            (HeaderName::from_static("x-api-key"), "stale-api-key"),
        ] {
            let request = anonymous_route_request(name.clone(), value);
            let rejection = validate_credential_headers_on_anonymous_routes(&config, &request)
                .expect_err("caller-attached credential must be rejected on an anonymous route");
            assert_eq!(rejection.status, StatusCode::BAD_REQUEST);
            assert_eq!(
                rejection.title,
                "credential_headers_not_allowed_on_anonymous_route"
            );
            assert!(
                rejection.detail.contains(&name.to_string()),
                "detail `{}` must name header `{name}`",
                rejection.detail
            );
        }
    }

    #[test]
    fn anonymous_device_flow_routes_tolerate_browser_cookies() {
        let config = GatewaySecurityConfig {
            reject_credential_headers_on_anonymous_auth_routes: true,
            ..GatewaySecurityConfig::default()
        };

        // A browser attaches every cookie for the origin to a same-origin
        // device/QR-login call, even when that cookie has nothing to do with
        // the anonymous flow. The route must stay reachable.
        let request = anonymous_route_request(header::COOKIE, "sdkwork_session=unrelated");
        validate_credential_headers_on_anonymous_routes(&config, &request)
            .expect("browser-attached cookie must not block an anonymous device route");

        // Tolerating the ambient cookie never masks an explicit credential.
        let mixed = Request::builder()
            .method(Method::POST)
            .uri("/app/v3/api/oauth/device_authorizations")
            .header(header::COOKIE, "sdkwork_session=unrelated")
            .header(
                HeaderName::from_static("access-token"),
                "stale-access-token",
            )
            .body(Body::empty())
            .expect("mixed anonymous device authorization request");
        let mixed_rejection = validate_credential_headers_on_anonymous_routes(&config, &mixed)
            .expect_err("explicit credential must still be rejected alongside a cookie");
        assert_eq!(
            mixed_rejection.title,
            "credential_headers_not_allowed_on_anonymous_route"
        );
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

    #[test]
    fn own_published_host_is_accepted_while_foreign_ip_literals_stay_rejected() {
        let host_header = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, value.parse().expect("host header"));
            headers
        };
        let mut config = GatewaySecurityConfig {
            allowed_hosts: vec!["api.noaper.com".to_string()],
            ..GatewaySecurityConfig::default()
        };
        record_own_hosts(
            &mut config,
            SocketAddr::from(([0, 0, 0, 0], 8080)),
            "http://172.18.0.5:8080",
        );

        assert!(validate_allowed_host(&config, &host_header("api.noaper.com:443")).is_ok());
        assert!(
            validate_allowed_host(&config, &host_header("172.18.0.5:8080")).is_ok(),
            "the advertised registry endpoint must be accepted as a Host value"
        );
        assert!(
            validate_allowed_host(&config, &host_header("172.18.0.9:8080")).is_err(),
            "a foreign IP literal must stay rejected"
        );
        assert!(validate_allowed_host(&config, &host_header("evil.example.com")).is_err());
    }

    #[test]
    fn own_hosts_cover_advertised_origin_bind_address_and_loopback() {
        let hosts = own_hosts_for(
            SocketAddr::from(([127, 0, 0, 1], 8080)),
            "http://10.0.0.5:8080",
        );
        assert!(hosts.contains(&"10.0.0.5".to_string()));
        assert!(hosts.contains(&"127.0.0.1".to_string()));
        assert!(hosts.contains(&"[::1]".to_string()));
        assert!(!hosts.iter().any(|host| host.contains("http")));
    }

    #[test]
    fn origin_host_normalizes_scheme_path_userinfo_and_port() {
        assert_eq!(
            origin_host("http://172.18.0.5:8080").as_deref(),
            Some("172.18.0.5")
        );
        assert_eq!(
            origin_host("https://Gateway.Example.com:443/base").as_deref(),
            Some("gateway.example.com")
        );
        assert_eq!(origin_host("[fd00::5]:8080").as_deref(), Some("[fd00::5]"));
        assert_eq!(origin_host("10.0.0.5").as_deref(), Some("10.0.0.5"));
        assert_eq!(origin_host("   "), None);
    }

    fn body_inspection_config(mode: &str) -> GatewaySecurityConfig {
        GatewaySecurityConfig {
            waf: sdkwork_api_cloud_gateway_config::GatewayWafConfig {
                enabled: true,
                mode: mode.to_string(),
                inspect_json_body: true,
                ..sdkwork_api_cloud_gateway_config::GatewayWafConfig::default()
            },
            ..GatewaySecurityConfig::default()
        }
    }

    fn json_post(body: String) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri("/app/v3/api/generations/videos/text_to_video")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .expect("json request")
    }

    #[tokio::test]
    async fn oversized_json_body_is_forwarded_intact_instead_of_truncated() {
        let config = body_inspection_config("enforce");
        // Media payloads carry inline images far past the WAF inspection budget.
        let payload = format!(
            "{{\"prompt\":\"{}\"}}",
            "a".repeat(WAF_JSON_BODY_LIMIT + 4096)
        );
        let request = json_post(payload.clone());

        let (request, events) = inspect_json_body(&config, request, 32 * 1024 * 1024)
            .await
            .expect("a body inside the gateway cap is not rejected");
        assert!(events.is_empty(), "over-budget bodies are not inspected");

        let (_, body) = request.into_parts();
        let forwarded = to_bytes(body, 64 * 1024 * 1024).await.expect("body");
        assert_eq!(
            forwarded.len(),
            payload.len(),
            "the handler must receive every byte the client sent"
        );
    }

    #[tokio::test]
    async fn declared_oversized_content_length_skips_inspection_without_reading() {
        let config = body_inspection_config("enforce");
        let payload = format!("{{\"image\":\"{}\"}}", "b".repeat(2048));
        let request = Request::builder()
            .method(Method::POST)
            .uri("/app/v3/api/generations/images/text_to_image")
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::CONTENT_LENGTH,
                (WAF_JSON_BODY_LIMIT + 1).to_string(),
            )
            .body(Body::from(payload.clone()))
            .expect("json request");

        let (request, events) = inspect_json_body(&config, request, 32 * 1024 * 1024)
            .await
            .expect("declared oversized body passes through");
        assert!(events.is_empty());

        let (_, body) = request.into_parts();
        let forwarded = to_bytes(body, 64 * 1024 * 1024).await.expect("body");
        assert_eq!(forwarded.len(), payload.len());
    }

    #[tokio::test]
    async fn streamed_body_beyond_the_gateway_cap_is_rejected_not_emptied() {
        let config = body_inspection_config("enforce");
        // No Content-Length: the WAF stage has to buffer to meter the body.
        let request = json_post(format!("{{\"prompt\":\"{}\"}}", "c".repeat(4096)));

        let rejection = inspect_json_body(&config, request, 1024)
            .await
            .expect_err("body above the gateway cap is rejected");
        assert_eq!(rejection.status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(rejection.title, "request_body_too_large");
    }

    #[tokio::test]
    async fn inspectable_json_body_still_blocks_a_baseline_signature() {
        let config = body_inspection_config("enforce");
        let request = json_post("{\"prompt\":\"../../etc/passwd\"}".to_string());

        let rejection = inspect_json_body(&config, request, 32 * 1024 * 1024)
            .await
            .expect_err("signature match is blocked");
        assert_eq!(rejection.status, StatusCode::FORBIDDEN);
        assert_eq!(rejection.title, "waf_request_blocked");
    }

    /// Production regression: `POST /v1/chat/completions` was answered with a
    /// `waf_baseline_signature` 403 whenever the conversation mentioned a file
    /// path, because the prompt body was scanned as if it were request input.
    #[tokio::test]
    async fn chat_prompt_body_is_not_signature_inspected() {
        let config = body_inspection_config("enforce");
        let payload = "{\"model\":\"deepseek/deepseek-flash\",\"messages\":[{\"role\":\"user\",\
                       \"content\":\"why does ../../etc/passwd 404 and what is ' or 1=1?\"}]}"
            .to_string();
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/chat/completions")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.clone()))
            .expect("json request");

        let (request, events) = inspect_json_body(&config, request, 32 * 1024 * 1024)
            .await
            .expect("a prompt body is conversation content, not a WAF target");
        assert!(events.is_empty(), "prompt bodies emit no WAF event");

        let (_, body) = request.into_parts();
        let forwarded = to_bytes(body, 64 * 1024 * 1024).await.expect("body");
        assert_eq!(
            String::from_utf8_lossy(&forwarded),
            payload,
            "the handler receives exactly the prompt the caller sent"
        );
    }

    #[tokio::test]
    async fn non_prompt_json_body_still_reaches_the_signature_matcher() {
        let config = body_inspection_config("enforce");
        let request = Request::builder()
            .method(Method::POST)
            .uri("/backend/v3/api/iam/users")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{\"name\":\"../../etc/passwd\"}"))
            .expect("json request");

        let rejection = inspect_json_body(&config, request, 32 * 1024 * 1024)
            .await
            .expect_err("management bodies stay inside the WAF");
        assert_eq!(rejection.status, StatusCode::FORBIDDEN);
        assert_eq!(rejection.title, "waf_request_blocked");
    }
}
