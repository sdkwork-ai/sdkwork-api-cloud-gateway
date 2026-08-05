use std::{collections::BTreeMap, sync::Mutex, time::Duration};

use axum::http::StatusCode;

const HTTP_REQUEST_DURATION_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Shard count for the metrics state. Each shard owns an independent mutex so
/// the per-request record path never contends on a single global lock under
/// high concurrency.
const METRICS_SHARD_COUNT: usize = 16;

/// Thread-affine shard selection: the same thread always lands on the same
/// shard, keeping lock handover rare without cross-shard coordination.
fn shard_index() -> usize {
    thread_local! {
        static SHARD: usize = {
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;
            let id = format!("{:?}", std::thread::current().id());
            for byte in id.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            (hash as usize) % METRICS_SHARD_COUNT
        };
    }
    SHARD.with(|shard| *shard)
}

#[derive(Clone)]
pub(crate) struct GatewayMetrics {
    inner: std::sync::Arc<Box<[Mutex<GatewayMetricsState>]>>,
}

impl Default for GatewayMetrics {
    fn default() -> Self {
        let shards = (0..METRICS_SHARD_COUNT)
            .map(|_| Mutex::new(GatewayMetricsState::default()))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            inner: std::sync::Arc::new(shards),
        }
    }
}

#[derive(Default)]
struct GatewayMetricsState {
    http_requests: BTreeMap<HttpRequestLabels, u64>,
    http_request_durations: BTreeMap<HttpRequestLabels, HistogramValue>,
    security_events: BTreeMap<SecurityEventLabels, u64>,
    traffic_rejections: BTreeMap<TrafficRejectionLabels, u64>,
    policy_rejections: BTreeMap<PolicyRejectionLabels, u64>,
    policy_dry_runs: BTreeMap<PolicyDryRunLabels, u64>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct HttpRequestLabels {
    service: String,
    method: String,
    route_target: String,
    service_id: String,
    status: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SecurityEventLabels {
    service: String,
    kind: String,
    action: String,
    rule: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TrafficRejectionLabels {
    service: String,
    kind: String,
    scope: String,
    status: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PolicyRejectionLabels {
    service: String,
    kind: String,
    scope: String,
    status: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PolicyDryRunLabels {
    service: String,
    kind: String,
    scope: String,
    status: String,
}

#[derive(Clone, Debug)]
struct HistogramValue {
    buckets: [u64; HTTP_REQUEST_DURATION_BUCKETS.len()],
    sum_seconds: f64,
    count: u64,
}

impl GatewayMetrics {
    pub(crate) fn record_http_request(
        &self,
        service: &str,
        method: &str,
        route_target: &str,
        service_id: &str,
        status: StatusCode,
    ) {
        let labels = HttpRequestLabels {
            service: service.to_string(),
            method: method.to_ascii_uppercase(),
            route_target: route_target.to_string(),
            service_id: service_id.to_string(),
            status: status.as_u16().to_string(),
        };
        let mut state = self.inner[shard_index()]
            .lock()
            .expect("gateway metrics shard poisoned");
        *state.http_requests.entry(labels).or_insert(0) += 1;
    }

    pub(crate) fn record_http_request_duration(
        &self,
        service: &str,
        method: &str,
        route_target: &str,
        service_id: &str,
        status: StatusCode,
        duration: Duration,
    ) {
        let labels = HttpRequestLabels {
            service: service.to_string(),
            method: method.to_ascii_uppercase(),
            route_target: route_target.to_string(),
            service_id: service_id.to_string(),
            status: status.as_u16().to_string(),
        };
        let mut state = self.inner[shard_index()]
            .lock()
            .expect("gateway metrics shard poisoned");
        let histogram = state.http_request_durations.entry(labels).or_default();
        histogram.record(duration.as_secs_f64());
    }

    pub(crate) fn record_security_event(
        &self,
        service: &str,
        kind: &str,
        action: &str,
        rule: &str,
    ) {
        let labels = SecurityEventLabels {
            service: service.to_string(),
            kind: kind.to_string(),
            action: action.to_string(),
            rule: rule.to_string(),
        };
        let mut state = self.inner[shard_index()]
            .lock()
            .expect("gateway metrics shard poisoned");
        *state.security_events.entry(labels).or_insert(0) += 1;
    }

    pub(crate) fn record_traffic_rejection(
        &self,
        service: &str,
        kind: &str,
        scope: &str,
        status: StatusCode,
    ) {
        let labels = TrafficRejectionLabels {
            service: service.to_string(),
            kind: kind.to_string(),
            scope: scope.to_string(),
            status: status.as_u16().to_string(),
        };
        let mut state = self.inner[shard_index()]
            .lock()
            .expect("gateway metrics shard poisoned");
        *state.traffic_rejections.entry(labels).or_insert(0) += 1;
    }

    pub(crate) fn record_policy_rejection(
        &self,
        service: &str,
        kind: &str,
        scope: &str,
        status: StatusCode,
    ) {
        let labels = PolicyRejectionLabels {
            service: service.to_string(),
            kind: kind.to_string(),
            scope: scope.to_string(),
            status: status.as_u16().to_string(),
        };
        let mut state = self.inner[shard_index()]
            .lock()
            .expect("gateway metrics shard poisoned");
        *state.policy_rejections.entry(labels).or_insert(0) += 1;
    }

    pub(crate) fn record_policy_dry_run(
        &self,
        service: &str,
        kind: &str,
        scope: &str,
        status: StatusCode,
    ) {
        let labels = PolicyDryRunLabels {
            service: service.to_string(),
            kind: kind.to_string(),
            scope: scope.to_string(),
            status: status.as_u16().to_string(),
        };
        let mut state = self.inner[shard_index()]
            .lock()
            .expect("gateway metrics shard poisoned");
        *state.policy_dry_runs.entry(labels).or_insert(0) += 1;
    }

    pub(crate) fn render(&self, service: &str) -> String {
        let mut combined = GatewayMetricsState::default();
        for shard in self.inner.iter() {
            let state = shard.lock().expect("gateway metrics shard poisoned");
            merge_metrics_state(&mut combined, &state);
        }
        let state = combined;
        let mut output = String::new();
        output.push_str("# HELP sdkwork_api_cloud_gateway_build_info Gateway build metadata.\n");
        output.push_str("# TYPE sdkwork_api_cloud_gateway_build_info gauge\n");
        output.push_str(&format!(
            "sdkwork_api_cloud_gateway_build_info{{service=\"{}\",version=\"{}\"}} 1\n",
            escape_label_value(service),
            escape_label_value(env!("CARGO_PKG_VERSION"))
        ));
        output.push_str(
            "# HELP sdkwork_api_cloud_gateway_http_requests_total Gateway HTTP requests by route target.\n",
        );
        output.push_str("# TYPE sdkwork_api_cloud_gateway_http_requests_total counter\n");
        for (labels, value) in &state.http_requests {
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_http_requests_total{{service=\"{}\",method=\"{}\",route_target=\"{}\",service_id=\"{}\",status=\"{}\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.method.as_str()),
                escape_label_value(labels.route_target.as_str()),
                escape_label_value(labels.service_id.as_str()),
                escape_label_value(labels.status.as_str()),
                value
            ));
        }
        output.push_str(
            "# HELP sdkwork_api_cloud_gateway_http_request_duration_seconds Gateway HTTP request duration to response headers.\n",
        );
        output
            .push_str("# TYPE sdkwork_api_cloud_gateway_http_request_duration_seconds histogram\n");
        for (labels, value) in &state.http_request_durations {
            for (index, bucket) in HTTP_REQUEST_DURATION_BUCKETS.iter().enumerate() {
                output.push_str(&format!(
                    "sdkwork_api_cloud_gateway_http_request_duration_seconds_bucket{{service=\"{}\",method=\"{}\",route_target=\"{}\",service_id=\"{}\",status=\"{}\",le=\"{}\"}} {}\n",
                    escape_label_value(labels.service.as_str()),
                    escape_label_value(labels.method.as_str()),
                    escape_label_value(labels.route_target.as_str()),
                    escape_label_value(labels.service_id.as_str()),
                    escape_label_value(labels.status.as_str()),
                    format_bucket(*bucket),
                    value.buckets[index]
                ));
            }
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_http_request_duration_seconds_bucket{{service=\"{}\",method=\"{}\",route_target=\"{}\",service_id=\"{}\",status=\"{}\",le=\"+Inf\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.method.as_str()),
                escape_label_value(labels.route_target.as_str()),
                escape_label_value(labels.service_id.as_str()),
                escape_label_value(labels.status.as_str()),
                value.count
            ));
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_http_request_duration_seconds_sum{{service=\"{}\",method=\"{}\",route_target=\"{}\",service_id=\"{}\",status=\"{}\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.method.as_str()),
                escape_label_value(labels.route_target.as_str()),
                escape_label_value(labels.service_id.as_str()),
                escape_label_value(labels.status.as_str()),
                value.sum_seconds
            ));
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_http_request_duration_seconds_count{{service=\"{}\",method=\"{}\",route_target=\"{}\",service_id=\"{}\",status=\"{}\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.method.as_str()),
                escape_label_value(labels.route_target.as_str()),
                escape_label_value(labels.service_id.as_str()),
                escape_label_value(labels.status.as_str()),
                value.count
            ));
        }
        output.push_str(
            "# HELP sdkwork_api_cloud_gateway_security_events_total Gateway security decisions by bounded rule.\n",
        );
        output.push_str("# TYPE sdkwork_api_cloud_gateway_security_events_total counter\n");
        for (labels, value) in &state.security_events {
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_security_events_total{{service=\"{}\",kind=\"{}\",action=\"{}\",rule=\"{}\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.kind.as_str()),
                escape_label_value(labels.action.as_str()),
                escape_label_value(labels.rule.as_str()),
                value
            ));
        }
        output.push_str(
            "# HELP sdkwork_api_cloud_gateway_traffic_rejections_total Gateway traffic-control rejections.\n",
        );
        output.push_str("# TYPE sdkwork_api_cloud_gateway_traffic_rejections_total counter\n");
        for (labels, value) in &state.traffic_rejections {
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_traffic_rejections_total{{service=\"{}\",kind=\"{}\",scope=\"{}\",status=\"{}\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.kind.as_str()),
                escape_label_value(labels.scope.as_str()),
                escape_label_value(labels.status.as_str()),
                value
            ));
        }
        output.push_str(
            "# HELP sdkwork_api_cloud_gateway_policy_rejections_total Gateway policy-governance rejections.\n",
        );
        output.push_str("# TYPE sdkwork_api_cloud_gateway_policy_rejections_total counter\n");
        for (labels, value) in &state.policy_rejections {
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_policy_rejections_total{{service=\"{}\",kind=\"{}\",scope=\"{}\",status=\"{}\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.kind.as_str()),
                escape_label_value(labels.scope.as_str()),
                escape_label_value(labels.status.as_str()),
                value
            ));
        }
        output.push_str(
            "# HELP sdkwork_api_cloud_gateway_policy_dry_run_total Gateway policy-governance dry-run observations.\n",
        );
        output.push_str("# TYPE sdkwork_api_cloud_gateway_policy_dry_run_total counter\n");
        for (labels, value) in &state.policy_dry_runs {
            output.push_str(&format!(
                "sdkwork_api_cloud_gateway_policy_dry_run_total{{service=\"{}\",kind=\"{}\",scope=\"{}\",status=\"{}\"}} {}\n",
                escape_label_value(labels.service.as_str()),
                escape_label_value(labels.kind.as_str()),
                escape_label_value(labels.scope.as_str()),
                escape_label_value(labels.status.as_str()),
                value
            ));
        }
        output
    }
}

fn merge_metrics_state(target: &mut GatewayMetricsState, source: &GatewayMetricsState) {
    for (labels, count) in &source.http_requests {
        *target.http_requests.entry(labels.clone()).or_insert(0) += count;
    }
    for (labels, value) in &source.http_request_durations {
        let entry = target
            .http_request_durations
            .entry(labels.clone())
            .or_default();
        for (index, bucket_count) in value.buckets.iter().enumerate() {
            entry.buckets[index] += bucket_count;
        }
        entry.sum_seconds += value.sum_seconds;
        entry.count += value.count;
    }
    for (labels, count) in &source.security_events {
        *target.security_events.entry(labels.clone()).or_insert(0) += count;
    }
    for (labels, count) in &source.traffic_rejections {
        *target.traffic_rejections.entry(labels.clone()).or_insert(0) += count;
    }
    for (labels, count) in &source.policy_rejections {
        *target.policy_rejections.entry(labels.clone()).or_insert(0) += count;
    }
    for (labels, count) in &source.policy_dry_runs {
        *target.policy_dry_runs.entry(labels.clone()).or_insert(0) += count;
    }
}

impl Default for HistogramValue {
    fn default() -> Self {
        Self {
            buckets: [0; HTTP_REQUEST_DURATION_BUCKETS.len()],
            sum_seconds: 0.0,
            count: 0,
        }
    }
}

impl HistogramValue {
    fn record(&mut self, duration_seconds: f64) {
        for (index, bucket) in HTTP_REQUEST_DURATION_BUCKETS.iter().enumerate() {
            if duration_seconds <= *bucket {
                self.buckets[index] += 1;
            }
        }
        self.sum_seconds += duration_seconds;
        self.count += 1;
    }
}

fn format_bucket(value: f64) -> String {
    if value.fract() == 0.0 {
        return format!("{value:.0}");
    }
    value.to_string()
}

pub(crate) fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
