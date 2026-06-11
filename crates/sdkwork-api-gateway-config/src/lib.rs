//! Typed configuration for the SDKWork API Gateway.

use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_GATEWAY_SERVICE_NAME: &str = "sdkwork-api-gateway";
pub const DEFAULT_GATEWAY_BIND: &str = "127.0.0.1:3900";
pub const APPBASE_APP_API_SERVICE_ID: &str = "sdkwork-appbase-app-api";
pub const APPBASE_APP_API_PREFIX: &str = "/app/v3/api";
pub const APPBASE_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:3800";
pub const APPBASE_APP_API_BASE_URL_ENV: &str = "SDKWORK_APPBASE_APP_API_BASE_URL";
pub const APPBASE_BACKEND_API_SERVICE_ID: &str = "sdkwork-appbase-backend-api";
pub const APPBASE_BACKEND_API_PREFIX: &str = "/backend/v3/api/iam";
pub const APPBASE_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:3801";
pub const APPBASE_BACKEND_API_BASE_URL_ENV: &str = "SDKWORK_APPBASE_BACKEND_API_BASE_URL";
pub const DRIVE_APP_API_SERVICE_ID: &str = "sdkwork-drive-app-api";
pub const DRIVE_APP_API_PREFIX: &str = "/app/v3/api/drive";
pub const DRIVE_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18080";
pub const DRIVE_APP_API_BASE_URL_ENV: &str = "SDKWORK_DRIVE_APP_API_BASE_URL";
pub const DRIVE_BACKEND_API_SERVICE_ID: &str = "sdkwork-drive-backend-api";
pub const DRIVE_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18081";
pub const DRIVE_OPEN_API_SERVICE_ID: &str = "sdkwork-drive-open-api";
pub const DRIVE_OPEN_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18082";
pub const DRIVE_ADMIN_STORAGE_API_SERVICE_ID: &str = "sdkwork-drive-admin-storage-api";
pub const DRIVE_ADMIN_STORAGE_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18083";
pub const NOTARY_APP_API_SERVICE_ID: &str = "sdkwork-notary-app-api";
pub const NOTARY_APP_API_PREFIX: &str = "/app/v3/api/notary";
pub const NOTARY_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18092";
pub const NOTARY_APP_API_BASE_URL_ENV: &str = "SDKWORK_NOTARY_APP_API_BASE_URL";
pub const NOTARY_BACKEND_API_SERVICE_ID: &str = "sdkwork-notary-backend-api";
pub const NOTARY_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18093";
pub const COMMERCE_APP_API_SERVICE_ID: &str = "sdkwork-commerce-app-api";
pub const COMMERCE_APP_API_PREFIX: &str = "/app/v3/api";
pub const COMMERCE_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18180";
pub const COMMERCE_APP_API_BASE_URL_ENV: &str = "SDKWORK_COMMERCE_APP_API_BASE_URL";
pub const COMMERCE_BACKEND_API_SERVICE_ID: &str = "sdkwork-commerce-backend-api";
pub const COMMERCE_BACKEND_API_PREFIX: &str = "/backend/v3/api";
pub const COMMERCE_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18181";
pub const COMMERCE_BACKEND_API_BASE_URL_ENV: &str = "SDKWORK_COMMERCE_BACKEND_API_BASE_URL";
pub const RTC_APP_API_SERVICE_ID: &str = "sdkwork-rtc-app-api";
pub const RTC_APP_API_PREFIX: &str = "/app/v3/api/rtc";
pub const RTC_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18280";
pub const RTC_APP_API_BASE_URL_ENV: &str = "SDKWORK_RTC_APP_API_BASE_URL";
pub const RTC_BACKEND_API_SERVICE_ID: &str = "sdkwork-rtc-backend-api";
pub const RTC_BACKEND_API_PREFIX: &str = "/backend/v3/api/rtc";
pub const RTC_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18281";
pub const RTC_BACKEND_API_BASE_URL_ENV: &str = "SDKWORK_RTC_BACKEND_API_BASE_URL";
pub const AGENT_APP_API_SERVICE_ID: &str = "sdkwork-agent-app-api";
pub const AGENT_APP_API_PREFIX: &str = "/app/v3/api/ai";
pub const AGENT_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18380";
pub const AGENT_APP_API_BASE_URL_ENV: &str = "SDKWORK_AGENT_APP_API_BASE_URL";
pub const AGENT_BACKEND_API_SERVICE_ID: &str = "sdkwork-agent-backend-api";
pub const AGENT_BACKEND_API_PREFIX: &str = "/backend/v3/api/ai";
pub const AGENT_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18381";
pub const AGENT_BACKEND_API_BASE_URL_ENV: &str = "SDKWORK_AGENT_BACKEND_API_BASE_URL";
pub const AGENT_OPEN_API_SERVICE_ID: &str = "sdkwork-agent-open-api";
pub const AGENT_OPEN_API_PREFIX: &str = "/agent/v3/api";
pub const AGENT_OPEN_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18382";
pub const AGENT_OPEN_API_BASE_URL_ENV: &str = "SDKWORK_AGENT_OPEN_API_BASE_URL";
pub const AIOT_APP_API_SERVICE_ID: &str = "sdkwork-aiot-app-api";
pub const AIOT_APP_API_PREFIX: &str = "/app/v3/api/iot";
pub const AIOT_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18480";
pub const AIOT_APP_API_BASE_URL_ENV: &str = "SDKWORK_AIOT_APP_API_BASE_URL";
pub const AIOT_BACKEND_API_SERVICE_ID: &str = "sdkwork-aiot-backend-api";
pub const AIOT_BACKEND_API_PREFIX: &str = "/backend/v3/api/iot";
pub const AIOT_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18481";
pub const AIOT_BACKEND_API_BASE_URL_ENV: &str = "SDKWORK_AIOT_BACKEND_API_BASE_URL";
pub const MEMORY_APP_API_SERVICE_ID: &str = "sdkwork-memory-app-api";
pub const MEMORY_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18580";
pub const MEMORY_BACKEND_API_SERVICE_ID: &str = "sdkwork-memory-backend-api";
pub const MEMORY_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18581";
pub const MEMORY_OPEN_API_SERVICE_ID: &str = "sdkwork-memory-open-api";
pub const MEMORY_OPEN_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18582";
pub const KNOWLEDGEBASE_APP_API_SERVICE_ID: &str = "sdkwork-knowledgebase-app-api";
pub const KNOWLEDGEBASE_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18680";
pub const KNOWLEDGEBASE_BACKEND_API_SERVICE_ID: &str = "sdkwork-knowledgebase-backend-api";
pub const KNOWLEDGEBASE_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18681";
pub const NEWS_APP_API_SERVICE_ID: &str = "sdkwork-news-app-api";
pub const NEWS_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18780";
pub const NEWS_BACKEND_API_SERVICE_ID: &str = "sdkwork-news-backend-api";
pub const NEWS_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18781";
pub const NEWS_OPEN_API_SERVICE_ID: &str = "sdkwork-news-open-api";
pub const NEWS_OPEN_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18782";
pub const NOTES_APP_API_SERVICE_ID: &str = "sdkwork-notes-app-api";
pub const NOTES_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18880";
pub const NOTES_BACKEND_API_SERVICE_ID: &str = "sdkwork-notes-backend-api";
pub const NOTES_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18881";
pub const NOTES_OPEN_API_SERVICE_ID: &str = "sdkwork-notes-open-api";
pub const NOTES_OPEN_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18882";
pub const MUSIC_APP_API_SERVICE_ID: &str = "sdkwork-music-app-api";
pub const MUSIC_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18980";
pub const MUSIC_BACKEND_API_SERVICE_ID: &str = "sdkwork-music-backend-api";
pub const MUSIC_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:18981";
pub const GENERATIONS_APP_API_SERVICE_ID: &str = "sdkwork-generations-app-api";
pub const GENERATIONS_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19080";
pub const GENERATIONS_BACKEND_API_SERVICE_ID: &str = "sdkwork-generations-backend-api";
pub const GENERATIONS_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19081";
pub const COMMUNITY_APP_API_SERVICE_ID: &str = "sdkwork-community-app-api";
pub const COMMUNITY_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19180";
pub const COMMUNITY_BACKEND_API_SERVICE_ID: &str = "sdkwork-community-backend-api";
pub const COMMUNITY_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19181";
pub const COMMUNITY_OPEN_API_SERVICE_ID: &str = "sdkwork-community-open-api";
pub const COMMUNITY_OPEN_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19182";
pub const SEARCH_APP_API_SERVICE_ID: &str = "sdkwork-search-app-api";
pub const SEARCH_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19280";
pub const SEARCH_BACKEND_API_SERVICE_ID: &str = "sdkwork-search-backend-api";
pub const SEARCH_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19281";
pub const VOICE_APP_API_SERVICE_ID: &str = "sdkwork-voice-app-api";
pub const VOICE_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19380";
pub const VOICE_BACKEND_API_SERVICE_ID: &str = "sdkwork-voice-backend-api";
pub const VOICE_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19381";
pub const IMAGE_APP_API_SERVICE_ID: &str = "sdkwork-image-app-api";
pub const IMAGE_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19480";
pub const IMAGE_BACKEND_API_SERVICE_ID: &str = "sdkwork-image-backend-api";
pub const IMAGE_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19481";
pub const IMAGE_OPEN_API_SERVICE_ID: &str = "sdkwork-image-open-api";
pub const IMAGE_OPEN_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19482";
pub const COMMENTS_APP_API_SERVICE_ID: &str = "sdkwork-comments-app-api";
pub const COMMENTS_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19580";
pub const COMMENTS_BACKEND_API_SERVICE_ID: &str = "sdkwork-comments-backend-api";
pub const COMMENTS_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19581";
pub const COURSE_APP_API_SERVICE_ID: &str = "sdkwork-course-app-api";
pub const COURSE_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19680";
pub const COURSE_BACKEND_API_SERVICE_ID: &str = "sdkwork-course-backend-api";
pub const COURSE_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19681";
pub const MESSAGING_APP_API_SERVICE_ID: &str = "sdkwork-messaging-app-api";
pub const MESSAGING_APP_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19780";
pub const MESSAGING_BACKEND_API_SERVICE_ID: &str = "sdkwork-messaging-backend-api";
pub const MESSAGING_BACKEND_API_DEFAULT_BASE_URL: &str = "http://127.0.0.1:19781";
pub const DEFAULT_MAX_REQUEST_BODY_BYTES: usize = 32 * 1024 * 1024;
pub const DEFAULT_UPSTREAM_READINESS_PATH: &str = "/healthz";

#[derive(Debug, Error)]
pub enum GatewayConfigError {
    #[error("failed to read gateway config `{path}`: {source}")]
    ReadFile {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse gateway config: {0}")]
    ParseToml(#[from] toml::de::Error),
    #[error("failed to parse sdkwork app manifest: {0}")]
    ParseManifest(#[from] serde_json::Error),
    #[error("invalid gateway mode `{0}`")]
    InvalidMode(String),
    #[error("invalid boolean value for `{key}`: `{value}`")]
    InvalidBoolean { key: String, value: String },
    #[error("invalid integer value for `{key}`: `{value}`")]
    InvalidInteger { key: String, value: String },
    #[error("invalid gateway config `{key}`: {reason}")]
    InvalidValue { key: String, reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRuntimeConfig {
    #[serde(default)]
    pub service: GatewayServiceConfig,
    #[serde(default)]
    pub server: GatewayServerConfig,
    #[serde(default)]
    pub mode: GatewayMode,
    #[serde(default = "default_upstreams")]
    pub upstreams: Vec<ServiceUpstreamConfig>,
    #[serde(default = "default_dependency_surfaces")]
    pub dependency_surfaces: Vec<DependencyApiSurfaceConfig>,
    #[serde(default)]
    pub request: GatewayRequestConfig,
    #[serde(default)]
    pub readiness: GatewayReadinessConfig,
    #[serde(default)]
    pub observability: GatewayObservabilityConfig,
    #[serde(default)]
    pub security: GatewaySecurityConfig,
    #[serde(default)]
    pub traffic: GatewayTrafficConfig,
    #[serde(default)]
    pub policy: GatewayPolicyConfig,
    #[serde(default)]
    pub diagnostics: GatewayDiagnosticsConfig,
    #[serde(default)]
    pub resilience: GatewayResilienceConfig,
    #[serde(default)]
    pub cors: CorsConfig,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayServiceConfig {
    #[serde(default = "default_service_name")]
    pub name: String,
    #[serde(default = "default_environment")]
    pub environment: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GatewayMode {
    #[default]
    Split,
    Embedded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUpstreamConfig {
    pub service_id: String,
    pub base_url: String,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_readiness_path")]
    pub readiness_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyApiSurfaceConfig {
    pub service_id: String,
    pub workspace: String,
    pub sdk_family: String,
    pub api_authority: String,
    pub surface: String,
    pub api_prefix: String,
    #[serde(default)]
    pub runtime_mode: DependencyRuntimeMode,
    #[serde(default = "default_true")]
    pub same_origin_allowed: bool,
    #[serde(default)]
    pub executable_export: Option<String>,
    #[serde(default)]
    pub cargo_feature: Option<String>,
    #[serde(default)]
    pub cargo_dependency: Option<String>,
    #[serde(default)]
    pub coverage: String,
    #[serde(default)]
    pub required_base_url_key: Option<String>,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyRuntimeMode {
    Split,
    Embedded,
    #[default]
    SplitOrEmbedded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorsConfig {
    #[serde(default = "default_true")]
    pub allow_any_origin: bool,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRequestConfig {
    #[serde(default = "default_max_request_body_bytes")]
    pub max_body_bytes: usize,
    #[serde(default = "default_request_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayReadinessConfig {
    #[serde(default)]
    pub check_upstreams: bool,
    #[serde(default = "default_readiness_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayObservabilityConfig {
    #[serde(default = "default_true")]
    pub access_log_enabled: bool,
    #[serde(default = "default_true")]
    pub audit_log_enabled: bool,
    #[serde(default = "default_log_sample_rate_basis_points")]
    pub access_log_sample_rate_basis_points: u32,
    #[serde(default = "default_log_sample_rate_basis_points")]
    pub audit_log_sample_rate_basis_points: u32,
    #[serde(default)]
    pub metrics_enabled: bool,
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,
    #[serde(default)]
    pub tracing_enabled: bool,
    #[serde(default = "default_redact_headers")]
    pub redact_headers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewaySecurityConfig {
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default)]
    pub trusted_proxy_cidrs: Vec<String>,
    #[serde(default = "default_true")]
    pub secure_headers_enabled: bool,
    #[serde(default = "default_max_header_bytes")]
    pub max_header_bytes: usize,
    #[serde(default = "default_max_header_count")]
    pub max_header_count: usize,
    #[serde(default = "default_true")]
    pub reject_credential_headers_on_anonymous_auth_routes: bool,
    #[serde(default)]
    pub csrf_guard_enabled: bool,
    #[serde(default)]
    pub waf: GatewayWafConfig,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayWafConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_waf_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub inspect_query: bool,
    #[serde(default = "default_true")]
    pub inspect_headers: bool,
    #[serde(default)]
    pub inspect_json_body: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayTrafficConfig {
    #[serde(default)]
    pub rate_limit_enabled: bool,
    #[serde(default = "default_true")]
    pub compat_x_rate_limit_headers: bool,
    #[serde(default)]
    pub global_max_in_flight: Option<usize>,
    #[serde(default)]
    pub rate_limits: Vec<RateLimitRuleConfig>,
    #[serde(default)]
    pub concurrency_limits: Vec<ConcurrencyLimitConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitRuleConfig {
    pub id: String,
    #[serde(default)]
    pub match_path_prefix: Option<String>,
    #[serde(default)]
    pub match_service_id: Option<String>,
    #[serde(default = "default_rate_limit_key")]
    pub key: String,
    #[serde(default = "default_rate_limit_algorithm")]
    pub algorithm: String,
    #[serde(default)]
    pub capacity: Option<u64>,
    #[serde(default)]
    pub refill_per_second: Option<u64>,
    #[serde(default)]
    pub burst: Option<u64>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub window_seconds: Option<u64>,
    #[serde(default = "default_rate_limit_reject_status")]
    pub reject_status: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrencyLimitConfig {
    pub id: String,
    #[serde(default)]
    pub match_path_prefix: Option<String>,
    #[serde(default)]
    pub match_service_id: Option<String>,
    pub max_in_flight: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayPolicyConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub rules: Vec<GatewayPolicyRuleConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayPolicyRuleConfig {
    pub id: String,
    #[serde(default)]
    pub match_service_id: Option<String>,
    #[serde(default)]
    pub match_path_prefix: Option<String>,
    #[serde(default)]
    pub allowed_methods: Vec<String>,
    #[serde(default)]
    pub max_body_bytes: Option<usize>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayDiagnosticsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_diagnostics_path")]
    pub path: String,
    #[serde(default = "default_true")]
    pub include_policy_inventory: bool,
    #[serde(default = "default_true")]
    pub include_route_inventory: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayResilienceConfig {
    #[serde(default)]
    pub retry_enabled: bool,
    #[serde(default = "default_retry_max_attempts")]
    pub max_attempts: usize,
    #[serde(default = "default_retry_statuses")]
    pub retry_statuses: Vec<u16>,
    #[serde(default)]
    pub retry_backoff_ms: u64,
    #[serde(default)]
    pub circuit_breaker_enabled: bool,
    #[serde(default = "default_circuit_breaker_failure_threshold")]
    pub failure_threshold: usize,
    #[serde(default = "default_circuit_breaker_open_seconds")]
    pub open_seconds: u64,
    #[serde(default = "default_circuit_breaker_half_open_max_attempts")]
    pub half_open_max_attempts: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGatewayRuntimeConfig {
    service: Option<GatewayServiceConfig>,
    server: Option<RawGatewayServerConfig>,
    mode: Option<GatewayMode>,
    upstreams: Option<Vec<ServiceUpstreamConfig>>,
    dependency_surfaces: Option<Vec<DependencyApiSurfaceConfig>>,
    request: Option<GatewayRequestConfig>,
    readiness: Option<GatewayReadinessConfig>,
    observability: Option<GatewayObservabilityConfig>,
    security: Option<GatewaySecurityConfig>,
    traffic: Option<GatewayTrafficConfig>,
    policy: Option<GatewayPolicyConfig>,
    diagnostics: Option<GatewayDiagnosticsConfig>,
    resilience: Option<GatewayResilienceConfig>,
    cors: Option<CorsConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGatewayServerConfig {
    bind: Option<String>,
    mode: Option<GatewayMode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkAppManifest {
    app_id: Option<String>,
    backend: Option<SdkworkManifestBackend>,
    sdk_dependencies: Option<Vec<SdkworkManifestDependency>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkManifestBackend {
    app_id: Option<String>,
    service_id: Option<String>,
    default_bind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkManifestDependency {
    service_id: String,
    workspace: String,
    sdk_family: String,
    api_authority: String,
    surface: String,
    api_prefix: String,
    #[serde(default)]
    runtime_mode: DependencyRuntimeMode,
    #[serde(default = "default_true")]
    same_origin_allowed: bool,
    #[serde(default)]
    executable_export: Option<String>,
    #[serde(default)]
    cargo_feature: Option<String>,
    #[serde(default)]
    cargo_dependency: Option<String>,
    #[serde(default)]
    coverage: Option<String>,
    #[serde(default)]
    required_base_url_key: Option<String>,
}

impl Default for GatewayRuntimeConfig {
    fn default() -> Self {
        Self {
            service: GatewayServiceConfig::default(),
            server: GatewayServerConfig::default(),
            mode: GatewayMode::default(),
            upstreams: default_upstreams(),
            dependency_surfaces: default_dependency_surfaces(),
            request: GatewayRequestConfig::default(),
            readiness: GatewayReadinessConfig::default(),
            observability: GatewayObservabilityConfig::default(),
            security: GatewaySecurityConfig::default(),
            traffic: GatewayTrafficConfig::default(),
            policy: GatewayPolicyConfig::default(),
            diagnostics: GatewayDiagnosticsConfig::default(),
            resilience: GatewayResilienceConfig::default(),
            cors: CorsConfig::default(),
        }
    }
}

impl GatewayRuntimeConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, GatewayConfigError> {
        let raw = toml::from_str::<RawGatewayRuntimeConfig>(input)?;
        let mut config = Self::default();

        if let Some(service) = raw.service {
            config.service = service;
        }
        if let Some(server) = raw.server {
            if let Some(bind) = server.bind {
                config.server.bind = bind;
            }
            if let Some(mode) = server.mode {
                config.mode = mode;
            }
        }
        if let Some(mode) = raw.mode {
            config.mode = mode;
        }
        if let Some(upstreams) = raw.upstreams {
            config.upstreams = upstreams;
        }
        if let Some(dependency_surfaces) = raw.dependency_surfaces {
            config.dependency_surfaces = dependency_surfaces;
        }
        if let Some(request) = raw.request {
            config.request = request;
        }
        if let Some(readiness) = raw.readiness {
            config.readiness = readiness;
        }
        if let Some(observability) = raw.observability {
            config.observability = observability;
        }
        if let Some(security) = raw.security {
            config.security = security;
        }
        if let Some(traffic) = raw.traffic {
            config.traffic = traffic;
        }
        if let Some(policy) = raw.policy {
            config.policy = policy;
        }
        if let Some(diagnostics) = raw.diagnostics {
            config.diagnostics = diagnostics;
        }
        if let Some(resilience) = raw.resilience {
            config.resilience = resilience;
        }
        if let Some(cors) = raw.cors {
            config.cors = cors;
        }

        config.validate()?;
        Ok(config)
    }

    pub fn from_sdkwork_manifest_str(input: &str) -> Result<Self, GatewayConfigError> {
        let manifest = serde_json::from_str::<SdkworkAppManifest>(input)?;
        let config = Self::from_sdkwork_manifest(manifest);
        config.validate()?;
        Ok(config)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, GatewayConfigError> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|source| GatewayConfigError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        if is_json_path(path) {
            Self::from_sdkwork_manifest_str(input.as_str())
        } else {
            Self::from_toml_str(input.as_str())
        }
    }

    pub fn from_sdkwork_manifest_path(path: impl AsRef<Path>) -> Result<Self, GatewayConfigError> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|source| GatewayConfigError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_sdkwork_manifest_str(input.as_str())
    }

    pub fn from_application_root(path: impl AsRef<Path>) -> Result<Self, GatewayConfigError> {
        Self::from_sdkwork_manifest_path(path.as_ref().join("sdkwork.app.config.json"))
    }

    pub fn with_env_overrides(
        mut self,
        env: &HashMap<String, String>,
    ) -> Result<Self, GatewayConfigError> {
        if let Some(bind) = env.get("SDKWORK_API_GATEWAY_BIND") {
            self.server.bind = bind.clone();
        }
        if let Some(mode) = env.get("SDKWORK_API_GATEWAY_MODE") {
            self.mode = GatewayMode::parse(mode)?;
        }
        if let Some(max_body_bytes) = env.get("SDKWORK_API_GATEWAY_MAX_BODY_BYTES") {
            self.request.max_body_bytes =
                parse_usize_env("SDKWORK_API_GATEWAY_MAX_BODY_BYTES", max_body_bytes)?;
        }
        if let Some(timeout_ms) = env.get("SDKWORK_API_GATEWAY_TIMEOUT_MS") {
            self.request.timeout_ms = parse_u64_env("SDKWORK_API_GATEWAY_TIMEOUT_MS", timeout_ms)?;
        }
        if let Some(check_upstreams) = env.get("SDKWORK_API_GATEWAY_READINESS_CHECK_UPSTREAMS") {
            self.readiness.check_upstreams = parse_bool_env(
                "SDKWORK_API_GATEWAY_READINESS_CHECK_UPSTREAMS",
                check_upstreams,
            )?;
        }
        if let Some(timeout_ms) = env.get("SDKWORK_API_GATEWAY_READINESS_TIMEOUT_MS") {
            self.readiness.timeout_ms =
                parse_u64_env("SDKWORK_API_GATEWAY_READINESS_TIMEOUT_MS", timeout_ms)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_ACCESS_LOG_ENABLED") {
            self.observability.access_log_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_ACCESS_LOG_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_AUDIT_LOG_ENABLED") {
            self.observability.audit_log_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_AUDIT_LOG_ENABLED", enabled)?;
        }
        if let Some(sample_rate) =
            env.get("SDKWORK_API_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS")
        {
            self.observability.access_log_sample_rate_basis_points = parse_sample_rate_env(
                "SDKWORK_API_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS",
                sample_rate,
            )?;
        }
        if let Some(sample_rate) = env.get("SDKWORK_API_GATEWAY_AUDIT_LOG_SAMPLE_RATE_BASIS_POINTS")
        {
            self.observability.audit_log_sample_rate_basis_points = parse_sample_rate_env(
                "SDKWORK_API_GATEWAY_AUDIT_LOG_SAMPLE_RATE_BASIS_POINTS",
                sample_rate,
            )?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_METRICS_ENABLED") {
            self.observability.metrics_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_METRICS_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_TRACING_ENABLED") {
            self.observability.tracing_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_TRACING_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_SECURITY_HEADERS_ENABLED") {
            self.security.secure_headers_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_SECURITY_HEADERS_ENABLED", enabled)?;
        }
        if let Some(allowed_hosts) = env.get("SDKWORK_API_GATEWAY_ALLOWED_HOSTS") {
            self.security.allowed_hosts = parse_csv_env(allowed_hosts);
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_RATE_LIMIT_ENABLED") {
            self.traffic.rate_limit_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_RATE_LIMIT_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_POLICY_ENABLED") {
            self.policy.enabled = parse_bool_env("SDKWORK_API_GATEWAY_POLICY_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_POLICY_DRY_RUN") {
            self.policy.dry_run = parse_bool_env("SDKWORK_API_GATEWAY_POLICY_DRY_RUN", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_DIAGNOSTICS_ENABLED") {
            self.diagnostics.enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_DIAGNOSTICS_ENABLED", enabled)?;
        }
        if let Some(path) = env.get("SDKWORK_API_GATEWAY_DIAGNOSTICS_PATH") {
            self.diagnostics.path = path.clone();
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_RETRY_ENABLED") {
            self.resilience.retry_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_RETRY_ENABLED", enabled)?;
        }
        if let Some(max_attempts) = env.get("SDKWORK_API_GATEWAY_RETRY_MAX_ATTEMPTS") {
            self.resilience.max_attempts =
                parse_usize_env("SDKWORK_API_GATEWAY_RETRY_MAX_ATTEMPTS", max_attempts)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_ENABLED") {
            self.resilience.circuit_breaker_enabled =
                parse_bool_env("SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_ENABLED", enabled)?;
        }
        if let Some(failure_threshold) =
            env.get("SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_FAILURE_THRESHOLD")
        {
            self.resilience.failure_threshold = parse_usize_env(
                "SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_FAILURE_THRESHOLD",
                failure_threshold,
            )?;
        }
        if let Some(open_seconds) = env.get("SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_OPEN_SECONDS") {
            self.resilience.open_seconds = parse_u64_env(
                "SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_OPEN_SECONDS",
                open_seconds,
            )?;
        }
        if let Some(half_open_max_attempts) =
            env.get("SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_HALF_OPEN_MAX_ATTEMPTS")
        {
            self.resilience.half_open_max_attempts = parse_usize_env(
                "SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_HALF_OPEN_MAX_ATTEMPTS",
                half_open_max_attempts,
            )?;
        }

        let surfaces = self.dependency_surfaces.clone();
        for surface in surfaces {
            if let Some(key) = surface.required_base_url_key.as_deref() {
                if let Some(base_url) = env.get(key).filter(|value| !value.trim().is_empty()) {
                    self.set_upstream_base_url(surface.service_id.as_str(), base_url);
                }
            }
        }

        if let Some(base_url) = env
            .get(APPBASE_APP_API_BASE_URL_ENV)
            .filter(|value| !value.trim().is_empty())
        {
            self.set_upstream_base_url(APPBASE_APP_API_SERVICE_ID, base_url);
        }

        self.validate()?;
        Ok(self)
    }

    pub fn upstream(&self, service_id: &str) -> Option<&ServiceUpstreamConfig> {
        self.upstreams
            .iter()
            .find(|upstream| upstream.service_id == service_id)
    }

    pub fn required_dependency_surfaces(
        &self,
    ) -> impl Iterator<Item = &DependencyApiSurfaceConfig> {
        self.dependency_surfaces.iter()
    }

    pub fn validate(&self) -> Result<(), GatewayConfigError> {
        validate_sample_rate(
            "observability.accessLogSampleRateBasisPoints",
            self.observability.access_log_sample_rate_basis_points,
        )?;
        validate_sample_rate(
            "observability.auditLogSampleRateBasisPoints",
            self.observability.audit_log_sample_rate_basis_points,
        )?;
        validate_positive_usize("request.maxBodyBytes", self.request.max_body_bytes)?;

        for upstream in &self.upstreams {
            validate_non_empty("upstreams[].serviceId", upstream.service_id.as_str())?;
            validate_non_empty("upstreams[].baseUrl", upstream.base_url.as_str())?;
        }
        for surface in &self.dependency_surfaces {
            validate_non_empty(
                "dependencySurfaces[].serviceId",
                surface.service_id.as_str(),
            )?;
            validate_non_empty(
                "dependencySurfaces[].apiPrefix",
                surface.api_prefix.as_str(),
            )?;
        }
        for rule in &self.traffic.rate_limits {
            validate_non_empty("traffic.rateLimits[].id", rule.id.as_str())?;
        }
        for rule in &self.traffic.concurrency_limits {
            validate_non_empty("traffic.concurrencyLimits[].id", rule.id.as_str())?;
            validate_positive_usize(
                "traffic.concurrencyLimits[].maxInFlight",
                rule.max_in_flight,
            )?;
        }
        for rule in &self.policy.rules {
            validate_non_empty("policy.rules[].id", rule.id.as_str())?;
            if let Some(max_body_bytes) = rule.max_body_bytes {
                validate_positive_usize("policy.rules[].maxBodyBytes", max_body_bytes)?;
            }
        }

        Ok(())
    }

    fn set_upstream_base_url(&mut self, service_id: &str, base_url: &str) {
        if let Some(upstream) = self
            .upstreams
            .iter_mut()
            .find(|upstream| upstream.service_id == service_id)
        {
            upstream.base_url = trim_trailing_slash(base_url);
            return;
        }

        self.upstreams
            .push(ServiceUpstreamConfig::new(service_id, base_url));
    }

    fn from_sdkwork_manifest(manifest: SdkworkAppManifest) -> Self {
        let mut config = Self::default();
        let backend = manifest.backend;
        let service_name = backend
            .as_ref()
            .and_then(|backend| backend.service_id.clone())
            .or_else(|| backend.as_ref().and_then(|backend| backend.app_id.clone()))
            .or(manifest.app_id);
        if let Some(service_name) = service_name.filter(|value| !value.trim().is_empty()) {
            config.service.name = service_name;
        }
        if let Some(default_bind) = backend
            .and_then(|backend| backend.default_bind)
            .filter(|value| !value.trim().is_empty())
        {
            config.server.bind = default_bind;
        }
        if let Some(dependencies) = manifest.sdk_dependencies {
            config.dependency_surfaces = dependencies
                .into_iter()
                .map(DependencyApiSurfaceConfig::from_manifest_dependency)
                .collect();
            let mut upstreams = Vec::new();
            for surface in &config.dependency_surfaces {
                let Some(upstream) = default_upstream_for_dependency_surface(surface) else {
                    continue;
                };
                if upstreams.iter().any(|candidate: &ServiceUpstreamConfig| {
                    candidate.service_id == upstream.service_id
                }) {
                    continue;
                }
                upstreams.push(upstream);
            }
            config.upstreams = upstreams;
        }

        config
    }
}

impl Default for GatewayServiceConfig {
    fn default() -> Self {
        Self {
            name: default_service_name(),
            environment: default_environment(),
        }
    }
}

impl Default for GatewayServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
        }
    }
}

impl GatewayMode {
    pub fn parse(value: &str) -> Result<Self, GatewayConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "split" => Ok(Self::Split),
            "embedded" | "all-in-one" | "all_in_one" => Ok(Self::Embedded),
            other => Err(GatewayConfigError::InvalidMode(other.to_string())),
        }
    }
}

impl ServiceUpstreamConfig {
    pub fn new(service_id: impl Into<String>, base_url: impl AsRef<str>) -> Self {
        Self {
            service_id: service_id.into(),
            base_url: trim_trailing_slash(base_url.as_ref()),
            required: true,
            timeout_ms: default_timeout_ms(),
            readiness_path: default_readiness_path(),
        }
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_any_origin: true,
            allowed_origins: Vec::new(),
        }
    }
}

impl Default for GatewayRequestConfig {
    fn default() -> Self {
        Self {
            max_body_bytes: default_max_request_body_bytes(),
            timeout_ms: default_request_timeout_ms(),
        }
    }
}

impl Default for GatewayReadinessConfig {
    fn default() -> Self {
        Self {
            check_upstreams: false,
            timeout_ms: default_readiness_timeout_ms(),
        }
    }
}

impl Default for GatewayObservabilityConfig {
    fn default() -> Self {
        Self {
            access_log_enabled: true,
            audit_log_enabled: true,
            access_log_sample_rate_basis_points: default_log_sample_rate_basis_points(),
            audit_log_sample_rate_basis_points: default_log_sample_rate_basis_points(),
            metrics_enabled: false,
            metrics_path: default_metrics_path(),
            tracing_enabled: false,
            redact_headers: default_redact_headers(),
        }
    }
}

impl Default for GatewaySecurityConfig {
    fn default() -> Self {
        Self {
            allowed_hosts: Vec::new(),
            trusted_proxy_cidrs: Vec::new(),
            secure_headers_enabled: true,
            max_header_bytes: default_max_header_bytes(),
            max_header_count: default_max_header_count(),
            reject_credential_headers_on_anonymous_auth_routes: true,
            csrf_guard_enabled: false,
            waf: GatewayWafConfig::default(),
        }
    }
}

impl Default for GatewayWafConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: default_waf_mode(),
            inspect_query: true,
            inspect_headers: true,
            inspect_json_body: false,
        }
    }
}

impl Default for GatewayTrafficConfig {
    fn default() -> Self {
        Self {
            rate_limit_enabled: false,
            compat_x_rate_limit_headers: true,
            global_max_in_flight: None,
            rate_limits: Vec::new(),
            concurrency_limits: Vec::new(),
        }
    }
}

impl Default for GatewayPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dry_run: false,
            rules: Vec::new(),
        }
    }
}

impl Default for GatewayDiagnosticsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: default_diagnostics_path(),
            include_policy_inventory: true,
            include_route_inventory: true,
        }
    }
}

impl Default for GatewayResilienceConfig {
    fn default() -> Self {
        Self {
            retry_enabled: false,
            max_attempts: default_retry_max_attempts(),
            retry_statuses: default_retry_statuses(),
            retry_backoff_ms: 0,
            circuit_breaker_enabled: false,
            failure_threshold: default_circuit_breaker_failure_threshold(),
            open_seconds: default_circuit_breaker_open_seconds(),
            half_open_max_attempts: default_circuit_breaker_half_open_max_attempts(),
        }
    }
}

impl DependencyApiSurfaceConfig {
    fn from_manifest_dependency(dependency: SdkworkManifestDependency) -> Self {
        let executable_export = dependency
            .executable_export
            .or_else(|| default_executable_export_for_service(dependency.service_id.as_str()));
        let coverage = dependency
            .coverage
            .unwrap_or_else(|| default_coverage_for_service(dependency.service_id.as_str()));

        Self {
            service_id: dependency.service_id,
            workspace: dependency.workspace,
            sdk_family: dependency.sdk_family,
            api_authority: dependency.api_authority,
            surface: dependency.surface,
            api_prefix: dependency.api_prefix,
            runtime_mode: dependency.runtime_mode,
            same_origin_allowed: dependency.same_origin_allowed,
            executable_export,
            cargo_feature: dependency.cargo_feature,
            cargo_dependency: dependency.cargo_dependency,
            coverage,
            required_base_url_key: dependency.required_base_url_key,
        }
    }
}

fn default_upstreams() -> Vec<ServiceUpstreamConfig> {
    vec![ServiceUpstreamConfig::new(
        APPBASE_APP_API_SERVICE_ID,
        APPBASE_APP_API_DEFAULT_BASE_URL,
    )]
}

fn default_dependency_surfaces() -> Vec<DependencyApiSurfaceConfig> {
    vec![DependencyApiSurfaceConfig {
        service_id: APPBASE_APP_API_SERVICE_ID.to_string(),
        workspace: "sdkwork-appbase".to_string(),
        sdk_family: "sdkwork-appbase-app-sdk".to_string(),
        api_authority: "sdkwork-appbase-app-api".to_string(),
        surface: "app".to_string(),
        api_prefix: APPBASE_APP_API_PREFIX.to_string(),
        runtime_mode: DependencyRuntimeMode::SplitOrEmbedded,
        same_origin_allowed: true,
        executable_export: Some(
            "sdkwork_iam_http::build_sdkwork_appbase_app_api_router".to_string(),
        ),
        cargo_feature: Some("foundation-appbase".to_string()),
        cargo_dependency: Some("sdkwork_iam_http".to_string()),
        coverage: "appbase-iam-routes".to_string(),
        required_base_url_key: Some(APPBASE_APP_API_BASE_URL_ENV.to_string()),
    }]
}

fn default_upstream_for_dependency_surface(
    surface: &DependencyApiSurfaceConfig,
) -> Option<ServiceUpstreamConfig> {
    let base_url = match surface.service_id.as_str() {
        APPBASE_APP_API_SERVICE_ID => APPBASE_APP_API_DEFAULT_BASE_URL,
        DRIVE_APP_API_SERVICE_ID => DRIVE_APP_API_DEFAULT_BASE_URL,
        DRIVE_BACKEND_API_SERVICE_ID => DRIVE_BACKEND_API_DEFAULT_BASE_URL,
        DRIVE_OPEN_API_SERVICE_ID => DRIVE_OPEN_API_DEFAULT_BASE_URL,
        DRIVE_ADMIN_STORAGE_API_SERVICE_ID => DRIVE_ADMIN_STORAGE_API_DEFAULT_BASE_URL,
        NOTARY_APP_API_SERVICE_ID => NOTARY_APP_API_DEFAULT_BASE_URL,
        NOTARY_BACKEND_API_SERVICE_ID => NOTARY_BACKEND_API_DEFAULT_BASE_URL,
        APPBASE_BACKEND_API_SERVICE_ID => APPBASE_BACKEND_API_DEFAULT_BASE_URL,
        COMMERCE_APP_API_SERVICE_ID => COMMERCE_APP_API_DEFAULT_BASE_URL,
        COMMERCE_BACKEND_API_SERVICE_ID => COMMERCE_BACKEND_API_DEFAULT_BASE_URL,
        RTC_APP_API_SERVICE_ID => RTC_APP_API_DEFAULT_BASE_URL,
        RTC_BACKEND_API_SERVICE_ID => RTC_BACKEND_API_DEFAULT_BASE_URL,
        AGENT_APP_API_SERVICE_ID => AGENT_APP_API_DEFAULT_BASE_URL,
        AGENT_BACKEND_API_SERVICE_ID => AGENT_BACKEND_API_DEFAULT_BASE_URL,
        AGENT_OPEN_API_SERVICE_ID => AGENT_OPEN_API_DEFAULT_BASE_URL,
        AIOT_APP_API_SERVICE_ID => AIOT_APP_API_DEFAULT_BASE_URL,
        AIOT_BACKEND_API_SERVICE_ID => AIOT_BACKEND_API_DEFAULT_BASE_URL,
        MEMORY_APP_API_SERVICE_ID => MEMORY_APP_API_DEFAULT_BASE_URL,
        MEMORY_BACKEND_API_SERVICE_ID => MEMORY_BACKEND_API_DEFAULT_BASE_URL,
        MEMORY_OPEN_API_SERVICE_ID => MEMORY_OPEN_API_DEFAULT_BASE_URL,
        KNOWLEDGEBASE_APP_API_SERVICE_ID => KNOWLEDGEBASE_APP_API_DEFAULT_BASE_URL,
        KNOWLEDGEBASE_BACKEND_API_SERVICE_ID => KNOWLEDGEBASE_BACKEND_API_DEFAULT_BASE_URL,
        NEWS_APP_API_SERVICE_ID => NEWS_APP_API_DEFAULT_BASE_URL,
        NEWS_BACKEND_API_SERVICE_ID => NEWS_BACKEND_API_DEFAULT_BASE_URL,
        NEWS_OPEN_API_SERVICE_ID => NEWS_OPEN_API_DEFAULT_BASE_URL,
        NOTES_APP_API_SERVICE_ID => NOTES_APP_API_DEFAULT_BASE_URL,
        NOTES_BACKEND_API_SERVICE_ID => NOTES_BACKEND_API_DEFAULT_BASE_URL,
        NOTES_OPEN_API_SERVICE_ID => NOTES_OPEN_API_DEFAULT_BASE_URL,
        MUSIC_APP_API_SERVICE_ID => MUSIC_APP_API_DEFAULT_BASE_URL,
        MUSIC_BACKEND_API_SERVICE_ID => MUSIC_BACKEND_API_DEFAULT_BASE_URL,
        GENERATIONS_APP_API_SERVICE_ID => GENERATIONS_APP_API_DEFAULT_BASE_URL,
        GENERATIONS_BACKEND_API_SERVICE_ID => GENERATIONS_BACKEND_API_DEFAULT_BASE_URL,
        COMMUNITY_APP_API_SERVICE_ID => COMMUNITY_APP_API_DEFAULT_BASE_URL,
        COMMUNITY_BACKEND_API_SERVICE_ID => COMMUNITY_BACKEND_API_DEFAULT_BASE_URL,
        COMMUNITY_OPEN_API_SERVICE_ID => COMMUNITY_OPEN_API_DEFAULT_BASE_URL,
        SEARCH_APP_API_SERVICE_ID => SEARCH_APP_API_DEFAULT_BASE_URL,
        SEARCH_BACKEND_API_SERVICE_ID => SEARCH_BACKEND_API_DEFAULT_BASE_URL,
        VOICE_APP_API_SERVICE_ID => VOICE_APP_API_DEFAULT_BASE_URL,
        VOICE_BACKEND_API_SERVICE_ID => VOICE_BACKEND_API_DEFAULT_BASE_URL,
        IMAGE_APP_API_SERVICE_ID => IMAGE_APP_API_DEFAULT_BASE_URL,
        IMAGE_BACKEND_API_SERVICE_ID => IMAGE_BACKEND_API_DEFAULT_BASE_URL,
        IMAGE_OPEN_API_SERVICE_ID => IMAGE_OPEN_API_DEFAULT_BASE_URL,
        COMMENTS_APP_API_SERVICE_ID => COMMENTS_APP_API_DEFAULT_BASE_URL,
        COMMENTS_BACKEND_API_SERVICE_ID => COMMENTS_BACKEND_API_DEFAULT_BASE_URL,
        COURSE_APP_API_SERVICE_ID => COURSE_APP_API_DEFAULT_BASE_URL,
        COURSE_BACKEND_API_SERVICE_ID => COURSE_BACKEND_API_DEFAULT_BASE_URL,
        MESSAGING_APP_API_SERVICE_ID => MESSAGING_APP_API_DEFAULT_BASE_URL,
        MESSAGING_BACKEND_API_SERVICE_ID => MESSAGING_BACKEND_API_DEFAULT_BASE_URL,
        _ => return None,
    };

    Some(ServiceUpstreamConfig::new(
        surface.service_id.as_str(),
        base_url,
    ))
}

fn default_executable_export_for_service(service_id: &str) -> Option<String> {
    (service_id == APPBASE_APP_API_SERVICE_ID)
        .then(|| "sdkwork_iam_http::build_sdkwork_appbase_app_api_router".to_string())
}

fn default_coverage_for_service(service_id: &str) -> String {
    if service_id == APPBASE_APP_API_SERVICE_ID {
        return "appbase-iam-routes".to_string();
    }
    String::new()
}

fn default_service_name() -> String {
    DEFAULT_GATEWAY_SERVICE_NAME.to_string()
}

fn default_environment() -> String {
    "development".to_string()
}

fn default_bind() -> String {
    DEFAULT_GATEWAY_BIND.to_string()
}

fn default_timeout_ms() -> u64 {
    30_000
}

fn default_readiness_timeout_ms() -> u64 {
    1_000
}

fn default_max_request_body_bytes() -> usize {
    DEFAULT_MAX_REQUEST_BODY_BYTES
}

fn default_request_timeout_ms() -> u64 {
    0
}

fn default_readiness_path() -> String {
    DEFAULT_UPSTREAM_READINESS_PATH.to_string()
}

fn default_metrics_path() -> String {
    "/metrics".to_string()
}

fn default_diagnostics_path() -> String {
    "/internal/gateway/diagnostics".to_string()
}

fn default_log_sample_rate_basis_points() -> u32 {
    10_000
}

fn default_redact_headers() -> Vec<String> {
    [
        "authorization",
        "access-token",
        "cookie",
        "x-api-key",
        "set-cookie",
    ]
    .into_iter()
    .map(ToOwned::to_owned)
    .collect()
}

fn default_max_header_bytes() -> usize {
    64 * 1024
}

fn default_max_header_count() -> usize {
    128
}

fn default_waf_mode() -> String {
    "monitor".to_string()
}

fn default_rate_limit_key() -> String {
    "global".to_string()
}

fn default_rate_limit_algorithm() -> String {
    "token-bucket".to_string()
}

fn default_rate_limit_reject_status() -> u16 {
    429
}

fn default_retry_max_attempts() -> usize {
    1
}

fn default_retry_statuses() -> Vec<u16> {
    vec![502, 503, 504]
}

fn default_circuit_breaker_failure_threshold() -> usize {
    5
}

fn default_circuit_breaker_open_seconds() -> u64 {
    30
}

fn default_circuit_breaker_half_open_max_attempts() -> usize {
    1
}

fn default_true() -> bool {
    true
}

fn trim_trailing_slash(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn is_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}

fn parse_bool_env(key: &str, value: &str) -> Result<bool, GatewayConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" | "on" => Ok(true),
        "false" | "0" | "no" | "n" | "off" => Ok(false),
        _ => Err(GatewayConfigError::InvalidBoolean {
            key: key.to_string(),
            value: value.to_string(),
        }),
    }
}

fn parse_u64_env(key: &str, value: &str) -> Result<u64, GatewayConfigError> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| GatewayConfigError::InvalidInteger {
            key: key.to_string(),
            value: value.to_string(),
        })
}

fn parse_u32_env(key: &str, value: &str) -> Result<u32, GatewayConfigError> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| GatewayConfigError::InvalidInteger {
            key: key.to_string(),
            value: value.to_string(),
        })
}

fn parse_sample_rate_env(key: &str, value: &str) -> Result<u32, GatewayConfigError> {
    let parsed = parse_u32_env(key, value)?;
    validate_sample_rate(key, parsed)?;
    Ok(parsed)
}

fn parse_usize_env(key: &str, value: &str) -> Result<usize, GatewayConfigError> {
    value
        .trim()
        .parse::<usize>()
        .map_err(|_| GatewayConfigError::InvalidInteger {
            key: key.to_string(),
            value: value.to_string(),
        })
}

fn parse_csv_env(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn validate_sample_rate(key: &str, value: u32) -> Result<(), GatewayConfigError> {
    if value <= 10_000 {
        return Ok(());
    }

    Err(invalid_value(
        key,
        "must be between 0 and 10000 basis points",
    ))
}

fn validate_positive_usize(key: &str, value: usize) -> Result<(), GatewayConfigError> {
    if value > 0 {
        return Ok(());
    }

    Err(invalid_value(key, "must be greater than 0"))
}

fn validate_non_empty(key: &str, value: &str) -> Result<(), GatewayConfigError> {
    if !value.trim().is_empty() {
        return Ok(());
    }

    Err(invalid_value(key, "must be non-empty"))
}

fn invalid_value(key: &str, reason: &str) -> GatewayConfigError {
    GatewayConfigError::InvalidValue {
        key: key.to_string(),
        reason: reason.to_string(),
    }
}
