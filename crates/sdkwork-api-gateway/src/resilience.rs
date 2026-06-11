use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use sdkwork_api_gateway_config::GatewayResilienceConfig;

#[derive(Clone, Default)]
pub(crate) struct GatewayResilienceState {
    inner: Arc<Mutex<ResilienceState>>,
}

#[derive(Default)]
struct ResilienceState {
    circuit_breakers: HashMap<String, CircuitBreaker>,
}

struct CircuitBreaker {
    consecutive_failures: usize,
    state: CircuitBreakerState,
}

#[derive(Clone, Copy)]
enum CircuitBreakerState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen { in_flight: usize },
}

pub(crate) struct CircuitBreakerRejection {
    pub(crate) status: StatusCode,
    pub(crate) title: &'static str,
    pub(crate) detail: String,
    pub(crate) event: &'static str,
    headers: Vec<(HeaderName, HeaderValue)>,
}

#[derive(Default)]
pub(crate) struct CircuitBreakerPermit {
    headers: Vec<(HeaderName, HeaderValue)>,
    event: Option<&'static str>,
}

impl GatewayResilienceState {
    pub(crate) fn before_upstream_request(
        &self,
        config: &GatewayResilienceConfig,
        service_id: &str,
    ) -> Result<CircuitBreakerPermit, CircuitBreakerRejection> {
        if !config.circuit_breaker_enabled {
            return Ok(CircuitBreakerPermit::default());
        }

        let now = Instant::now();
        let mut state = self
            .inner
            .lock()
            .expect("gateway resilience mutex poisoned");
        let breaker = state
            .circuit_breakers
            .entry(service_id.to_string())
            .or_default();

        match breaker.state {
            CircuitBreakerState::Closed => Ok(CircuitBreakerPermit::default()),
            CircuitBreakerState::Open { opened_at } => {
                let open_for = Duration::from_secs(config.open_seconds);
                let elapsed = now.duration_since(opened_at);
                if elapsed >= open_for {
                    breaker.state = CircuitBreakerState::HalfOpen { in_flight: 1 };
                    return Ok(CircuitBreakerPermit::half_open());
                }

                Err(circuit_breaker_rejection(
                    service_id,
                    open_for.saturating_sub(elapsed).as_secs().max(1),
                ))
            }
            CircuitBreakerState::HalfOpen { in_flight } => {
                let max_attempts = config.half_open_max_attempts.max(1);
                if in_flight >= max_attempts {
                    return Err(circuit_breaker_rejection(service_id, 1));
                }

                breaker.state = CircuitBreakerState::HalfOpen {
                    in_flight: in_flight + 1,
                };
                Ok(CircuitBreakerPermit::half_open())
            }
        }
    }

    pub(crate) fn record_upstream_result(
        &self,
        config: &GatewayResilienceConfig,
        service_id: &str,
        status: StatusCode,
    ) -> Option<&'static str> {
        if !config.circuit_breaker_enabled {
            return None;
        }

        let mut state = self
            .inner
            .lock()
            .expect("gateway resilience mutex poisoned");
        let breaker = state
            .circuit_breakers
            .entry(service_id.to_string())
            .or_default();

        if is_circuit_breaker_failure(status) {
            return breaker.record_failure(config.failure_threshold.max(1), Instant::now());
        } else {
            breaker.record_success();
        }
        None
    }
}

impl CircuitBreakerPermit {
    fn half_open() -> Self {
        Self {
            headers: vec![(
                HeaderName::from_static("x-sdkwork-circuit-state"),
                HeaderValue::from_static("half-open"),
            )],
            event: Some("circuit_half_open_probe"),
        }
    }

    pub(crate) fn apply_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in &self.headers {
            headers.insert(name.clone(), value.clone());
        }
    }

    pub(crate) fn event(&self) -> Option<&'static str> {
        self.event
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            state: CircuitBreakerState::Closed,
        }
    }
}

impl CircuitBreaker {
    fn record_failure(&mut self, failure_threshold: usize, now: Instant) -> Option<&'static str> {
        match self.state {
            CircuitBreakerState::Closed => {
                self.consecutive_failures += 1;
                if self.consecutive_failures >= failure_threshold {
                    self.open(now);
                    return Some("circuit_open");
                }
            }
            CircuitBreakerState::Open { .. } => {}
            CircuitBreakerState::HalfOpen { .. } => {
                self.open(now);
                return Some("circuit_open");
            }
        }
        None
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CircuitBreakerState::Closed;
    }

    fn open(&mut self, now: Instant) {
        self.consecutive_failures = 0;
        self.state = CircuitBreakerState::Open { opened_at: now };
    }
}

impl CircuitBreakerRejection {
    pub(crate) fn apply_headers(&self, headers: &mut HeaderMap) {
        for (name, value) in &self.headers {
            headers.insert(name.clone(), value.clone());
        }
    }
}

fn circuit_breaker_rejection(
    service_id: &str,
    retry_after_seconds: u64,
) -> CircuitBreakerRejection {
    CircuitBreakerRejection {
        status: StatusCode::SERVICE_UNAVAILABLE,
        title: "circuit_breaker_open",
        detail: format!("gateway circuit breaker for dependency `{service_id}` is open"),
        event: "circuit_short_circuit",
        headers: vec![
            (
                HeaderName::from_static("x-sdkwork-circuit-state"),
                HeaderValue::from_static("open"),
            ),
            (header::RETRY_AFTER, header_value(retry_after_seconds)),
        ],
    }
}

fn is_circuit_breaker_failure(status: StatusCode) -> bool {
    status.is_server_error()
}

fn header_value(value: impl ToString) -> HeaderValue {
    HeaderValue::from_str(value.to_string().as_str()).expect("gateway resilience header is valid")
}
