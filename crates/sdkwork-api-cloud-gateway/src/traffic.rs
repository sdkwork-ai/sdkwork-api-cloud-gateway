use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use sdkwork_api_cloud_gateway_config::{
    ConcurrencyLimitConfig, GatewayTrafficConfig, RateLimitRuleConfig,
};
use sdkwork_utils_rust::trusted_proxy::TrustedProxyConfig;

#[derive(Clone, Default)]
pub(crate) struct GatewayTrafficState {
    inner: Arc<Mutex<TrafficState>>,
}

impl GatewayTrafficState {
    pub(crate) fn with_trusted_proxies(trusted_proxy_cidrs: &[String]) -> Self {
        let trusted_proxies =
            TrustedProxyConfig::from_cidrs(trusted_proxy_cidrs.iter().map(String::as_str));
        Self {
            inner: Arc::new(Mutex::new(TrafficState {
                trusted_proxies,
                ..TrafficState::default()
            })),
        }
    }
}

#[derive(Default)]
struct TrafficState {
    global_in_flight: usize,
    concurrency_by_rule: HashMap<String, usize>,
    buckets: HashMap<String, TokenBucket>,
    trusted_proxies: TrustedProxyConfig,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
    last_seen: Instant,
}

/// Buckets idle longer than this are swept when the bucket table is full.
const BUCKET_IDLE_TTL: Duration = Duration::from_secs(600);
/// Hard cap on rate-limit bucket identities. Beyond it, new identities share
/// one conservative "saturated" bucket so memory stays bounded under spoofed
/// identity floods (each bucket is ~100 bytes, worst case ~10 MB).
const MAX_RATE_LIMIT_BUCKETS: usize = 100_000;

pub(crate) struct TrafficPermit {
    state: GatewayTrafficState,
    global_acquired: bool,
    concurrency_keys: Vec<String>,
    headers: Vec<(HeaderName, HeaderValue)>,
}

pub(crate) struct TrafficRejection {
    pub(crate) status: StatusCode,
    pub(crate) title: &'static str,
    pub(crate) detail: String,
    pub(crate) kind: &'static str,
    pub(crate) scope: String,
    headers: Vec<(HeaderName, HeaderValue)>,
}

struct RateLimitDecision {
    allowed: bool,
    limit: u64,
    remaining: u64,
    reset_seconds: u64,
    policy: String,
}

impl GatewayTrafficState {
    pub(crate) fn acquire(
        &self,
        config: &GatewayTrafficConfig,
        path: &str,
        service_id: Option<&str>,
        headers: &HeaderMap,
    ) -> Result<TrafficPermit, TrafficRejection> {
        let mut permit = self.acquire_concurrency(config, path, service_id)?;

        if config.rate_limit_enabled {
            for rule in config
                .rate_limits
                .iter()
                .filter(|rule| rate_limit_matches(rule, path, service_id))
            {
                let decision = self.check_rate_limit(rule, headers);
                let rate_headers =
                    rate_limit_headers(&decision, config.compat_x_rate_limit_headers);
                if decision.allowed {
                    permit.headers.extend(rate_headers);
                    continue;
                }

                drop(permit);
                let mut headers = rate_headers;
                headers.push((header::RETRY_AFTER, header_value(decision.reset_seconds)));
                return Err(TrafficRejection {
                    status: StatusCode::from_u16(rule.reject_status)
                        .unwrap_or(StatusCode::TOO_MANY_REQUESTS),
                    title: "rate_limit_exceeded",
                    detail: format!("gateway rate limit `{}` was exceeded", rule.id),
                    kind: "rate_limit",
                    scope: rule.id.clone(),
                    headers,
                });
            }
        }

        Ok(permit)
    }

    fn acquire_concurrency(
        &self,
        config: &GatewayTrafficConfig,
        path: &str,
        service_id: Option<&str>,
    ) -> Result<TrafficPermit, TrafficRejection> {
        let mut state = self.inner.lock().expect("gateway traffic mutex poisoned");
        let mut permit = TrafficPermit {
            state: self.clone(),
            global_acquired: false,
            concurrency_keys: Vec::new(),
            headers: Vec::new(),
        };

        if let Some(max_in_flight) = config.global_max_in_flight {
            if state.global_in_flight >= max_in_flight {
                return Err(concurrency_rejection("global"));
            }
            state.global_in_flight += 1;
            permit.global_acquired = true;
        }

        for rule in config
            .concurrency_limits
            .iter()
            .filter(|rule| concurrency_limit_matches(rule, path, service_id))
        {
            let current = state
                .concurrency_by_rule
                .get(rule.id.as_str())
                .copied()
                .unwrap_or(0);
            if current >= rule.max_in_flight {
                drop(state);
                drop(permit);
                return Err(concurrency_rejection(rule.id.as_str()));
            }

            state
                .concurrency_by_rule
                .insert(rule.id.clone(), current + 1);
            permit.concurrency_keys.push(rule.id.clone());
        }

        drop(state);
        Ok(permit)
    }

    fn check_rate_limit(
        &self,
        rule: &RateLimitRuleConfig,
        headers: &HeaderMap,
    ) -> RateLimitDecision {
        let now = Instant::now();
        let limit = rule.capacity.or(rule.limit).or(rule.burst).unwrap_or(60);
        let capacity = rule.burst.unwrap_or(limit).max(limit);
        let refill_per_second = refill_per_second(rule, limit);
        let mut state = self.inner.lock().expect("gateway traffic mutex poisoned");
        let identity = rate_limit_identity(rule, headers, &state.trusted_proxies);
        let bucket_key = if state.buckets.len() >= MAX_RATE_LIMIT_BUCKETS {
            state
                .buckets
                .retain(|_, bucket| now.duration_since(bucket.last_seen) < BUCKET_IDLE_TTL);
            if state.buckets.len() >= MAX_RATE_LIMIT_BUCKETS {
                format!("{}:saturated", rule.id)
            } else {
                format!("{}:{}", rule.id, identity)
            }
        } else {
            format!("{}:{}", rule.id, identity)
        };
        let bucket = state
            .buckets
            .entry(bucket_key)
            .or_insert_with(|| TokenBucket {
                tokens: capacity as f64,
                last_refill: now,
                last_seen: now,
            });
        bucket.last_seen = now;

        if refill_per_second > 0 {
            let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
            bucket.tokens =
                (bucket.tokens + elapsed * refill_per_second as f64).min(capacity as f64);
        }
        bucket.last_refill = now;

        let allowed = bucket.tokens >= 1.0;
        if allowed {
            bucket.tokens -= 1.0;
        }
        let remaining = bucket.tokens.floor().max(0.0) as u64;
        let reset_seconds = reset_seconds(bucket.tokens, refill_per_second);

        RateLimitDecision {
            allowed,
            limit,
            remaining,
            reset_seconds,
            policy: format!(
                "{}; limit={}; window={}; algorithm=\"{}\"",
                rule.id,
                limit,
                rule.window_seconds.unwrap_or(1),
                rule.algorithm
            ),
        }
    }
}

impl TrafficPermit {
    pub(crate) fn apply_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in &self.headers {
            headers.insert(name.clone(), value.clone());
        }
    }
}

impl Drop for TrafficPermit {
    fn drop(&mut self) {
        if !self.global_acquired && self.concurrency_keys.is_empty() {
            return;
        }

        let mut state = self
            .state
            .inner
            .lock()
            .expect("gateway traffic mutex poisoned");
        if self.global_acquired && state.global_in_flight > 0 {
            state.global_in_flight -= 1;
        }
        for key in &self.concurrency_keys {
            if let Some(current) = state.concurrency_by_rule.get_mut(key) {
                *current = current.saturating_sub(1);
            }
        }
    }
}

impl TrafficRejection {
    pub(crate) fn apply_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in &self.headers {
            headers.insert(name.clone(), value.clone());
        }
    }
}

fn rate_limit_matches(rule: &RateLimitRuleConfig, path: &str, service_id: Option<&str>) -> bool {
    rule_matches(
        rule.match_path_prefix.as_deref(),
        rule.match_service_id.as_deref(),
        path,
        service_id,
    )
}

fn concurrency_limit_matches(
    rule: &ConcurrencyLimitConfig,
    path: &str,
    service_id: Option<&str>,
) -> bool {
    rule_matches(
        rule.match_path_prefix.as_deref(),
        rule.match_service_id.as_deref(),
        path,
        service_id,
    )
}

fn rule_matches(
    match_path_prefix: Option<&str>,
    match_service_id: Option<&str>,
    path: &str,
    service_id: Option<&str>,
) -> bool {
    let path_matches = match_path_prefix
        .map(|prefix| {
            path == prefix
                || path.starts_with(format!("{}/", prefix.trim_end_matches('/')).as_str())
        })
        .unwrap_or(true);
    let service_matches = match_service_id
        .map(|expected| service_id == Some(expected))
        .unwrap_or(true);
    path_matches && service_matches
}

fn rate_limit_identity(
    rule: &RateLimitRuleConfig,
    headers: &HeaderMap,
    trusted_proxies: &TrustedProxyConfig,
) -> String {
    match rule.key.trim().to_ascii_lowercase().as_str() {
        "client-ip" => client_ip_for_rate_limit(headers, trusted_proxies),
        "host" => headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unknown")
            .to_string(),
        _ => "global".to_string(),
    }
}

/// Resolve the rate-limit client identity from `X-Forwarded-For`.
///
/// Forwarded headers are honoured only when trusted proxies are configured and
/// the last hop of the chain is inside a trusted CIDR. Without trusted proxies
/// the header is ignored entirely, so clients cannot spoof their identity by
/// sending a synthetic `X-Forwarded-For`.
fn client_ip_for_rate_limit(headers: &HeaderMap, trusted: &TrustedProxyConfig) -> String {
    if trusted.is_empty() {
        return "unknown".to_string();
    }
    let Some(raw) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    else {
        return "unknown".to_string();
    };
    let chain = raw
        .split(',')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let Some(last) = chain.last() else {
        return "unknown".to_string();
    };
    let Ok(last_ip) = last.parse::<IpAddr>() else {
        return "unknown".to_string();
    };
    if !trusted.is_trusted(&last_ip) {
        return "unknown".to_string();
    }
    chain
        .first()
        .and_then(|entry| entry.parse::<IpAddr>().ok())
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn refill_per_second(rule: &RateLimitRuleConfig, limit: u64) -> u64 {
    rule.refill_per_second.unwrap_or_else(|| {
        rule.window_seconds
            .filter(|window| *window > 0)
            .map(|window| limit.div_ceil(window))
            .unwrap_or(limit)
    })
}

fn reset_seconds(tokens: f64, refill_per_second: u64) -> u64 {
    if refill_per_second == 0 {
        return 1;
    }
    if tokens >= 1.0 {
        return 0;
    }
    ((1.0 - tokens) / refill_per_second as f64).ceil().max(1.0) as u64
}

fn rate_limit_headers(
    decision: &RateLimitDecision,
    compat_x_rate_limit_headers: bool,
) -> Vec<(HeaderName, HeaderValue)> {
    let mut headers = vec![
        (
            HeaderName::from_static("ratelimit"),
            header_value(format!(
                "limit={}, remaining={}, reset={}",
                decision.limit, decision.remaining, decision.reset_seconds
            )),
        ),
        (
            HeaderName::from_static("ratelimit-policy"),
            header_value(decision.policy.as_str()),
        ),
    ];
    if compat_x_rate_limit_headers {
        headers.extend([
            (
                HeaderName::from_static("x-ratelimit-limit"),
                header_value(decision.limit),
            ),
            (
                HeaderName::from_static("x-ratelimit-remaining"),
                header_value(decision.remaining),
            ),
            (
                HeaderName::from_static("x-ratelimit-reset"),
                header_value(decision.reset_seconds),
            ),
        ]);
    }
    headers
}

fn concurrency_rejection(scope: &str) -> TrafficRejection {
    TrafficRejection {
        status: StatusCode::SERVICE_UNAVAILABLE,
        title: "concurrency_limit_exceeded",
        detail: format!("gateway concurrency limit `{scope}` was exceeded"),
        kind: "concurrency",
        scope: scope.to_string(),
        headers: vec![(header::RETRY_AFTER, HeaderValue::from_static("1"))],
    }
}

fn header_value(value: impl ToString) -> HeaderValue {
    HeaderValue::from_str(value.to_string().as_str()).expect("gateway traffic header is valid")
}
