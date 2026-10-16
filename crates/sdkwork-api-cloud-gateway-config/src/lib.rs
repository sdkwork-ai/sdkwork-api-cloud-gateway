//! Typed configuration for the SDKWork API Gateway.

use std::{collections::HashMap, fs, path::Path};

use sdkwork_web_core::registered_client_origins::{
    is_registered_sdkwork_client_origin, merge_registered_sdkwork_client_origins,
};
use sdkwork_web_core::registered_console_hosts::{
    registered_console_hosts_from_lookup, RegisteredConsoleHosts,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_GATEWAY_SERVICE_NAME: &str = "sdkwork-api-cloud-gateway";
pub const DEFAULT_GATEWAY_BIND: &str = "127.0.0.1:3900";
pub const APPBASE_APP_API_SERVICE_ID: &str = "sdkwork-iam-app-api";
pub const APPBASE_APP_API_PREFIX: &str = "/app/v3/api";
pub const APPBASE_BACKEND_API_SERVICE_ID: &str = "sdkwork-iam-backend-api";
pub const APPBASE_BACKEND_API_PREFIX: &str = "/backend/v3/api/iam";
pub const APPBASE_OPEN_API_SERVICE_ID: &str = "sdkwork-iam-open-api";
pub const DRIVE_APP_API_SERVICE_ID: &str = "sdkwork-drive-app-api";
pub const DRIVE_APP_API_PREFIX: &str = "/app/v3/api/drive";
pub const ASSETS_APP_API_SERVICE_ID: &str = "sdkwork-assets-app-api";
pub const ASSETS_APP_API_PREFIX: &str = "/app/v3/api/assets";
pub const DRIVE_BACKEND_API_SERVICE_ID: &str = "sdkwork-drive-backend-api";
pub const DRIVE_OPEN_API_SERVICE_ID: &str = "sdkwork-drive-open-api";
pub const NOTARY_APP_API_SERVICE_ID: &str = "sdkwork-notary-app-api";
pub const NOTARY_APP_API_PREFIX: &str = "/app/v3/api/notary";
pub const NOTARY_BACKEND_API_SERVICE_ID: &str = "sdkwork-notary-backend-api";
pub const COMMERCE_APP_API_SERVICE_ID: &str = "sdkwork-commerce-app-api";
pub const COMMERCE_APP_API_PREFIX: &str = "/app/v3/api";
pub const COMMERCE_BACKEND_API_SERVICE_ID: &str = "sdkwork-commerce-backend-api";
pub const COMMERCE_BACKEND_API_PREFIX: &str = "/backend/v3/api";
pub const RTC_APP_API_SERVICE_ID: &str = "sdkwork-rtc-app-api";
pub const RTC_APP_API_PREFIX: &str = "/app/v3/api/rtc";
pub const RTC_BACKEND_API_SERVICE_ID: &str = "sdkwork-rtc-backend-api";
pub const RTC_BACKEND_API_PREFIX: &str = "/backend/v3/api/rtc";
pub const AGENT_APP_API_SERVICE_ID: &str = "sdkwork-agent-app-api";
pub const AGENT_APP_API_PREFIX: &str = "/app/v3/api/ai";
pub const AGENT_BACKEND_API_SERVICE_ID: &str = "sdkwork-agent-backend-api";
pub const AGENT_BACKEND_API_PREFIX: &str = "/backend/v3/api/ai";
pub const AGENT_OPEN_API_SERVICE_ID: &str = "sdkwork-agent-open-api";
pub const AGENT_OPEN_API_PREFIX: &str = "/agent/v3/api";
pub const AIOT_APP_API_SERVICE_ID: &str = "sdkwork-aiot-app-api";
pub const AIOT_APP_API_PREFIX: &str = "/app/v3/api/iot";
pub const AIOT_BACKEND_API_SERVICE_ID: &str = "sdkwork-aiot-backend-api";
pub const AIOT_BACKEND_API_PREFIX: &str = "/backend/v3/api/iot";
pub const MEMORY_APP_API_SERVICE_ID: &str = "sdkwork-memory-app-api";
pub const MEMORY_BACKEND_API_SERVICE_ID: &str = "sdkwork-memory-backend-api";
pub const MEMORY_OPEN_API_SERVICE_ID: &str = "sdkwork-memory-open-api";
pub const KNOWLEDGEBASE_APP_API_SERVICE_ID: &str = "sdkwork-knowledgebase-app-api";
pub const KNOWLEDGEBASE_BACKEND_API_SERVICE_ID: &str = "sdkwork-knowledgebase-backend-api";
pub const KNOWLEDGEBASE_OPEN_API_SERVICE_ID: &str = "sdkwork-knowledgebase-open-api";
pub const NOTES_APP_API_SERVICE_ID: &str = "sdkwork-notes-app-api";
pub const NOTES_BACKEND_API_SERVICE_ID: &str = "sdkwork-notes-backend-api";
pub const NOTES_OPEN_API_SERVICE_ID: &str = "sdkwork-notes-open-api";
pub const GENERATIONS_APP_API_SERVICE_ID: &str = "sdkwork-generations-app-api";
pub const GENERATIONS_APP_API_PREFIX: &str = "/app/v3/api/generations";
pub const GENERATIONS_BACKEND_API_SERVICE_ID: &str = "sdkwork-generations-backend-api";
pub const GENERATIONS_BACKEND_API_PREFIX: &str = "/backend/v3/api/generations";
pub const COMMUNITY_APP_API_SERVICE_ID: &str = "sdkwork-community-app-api";
pub const COMMUNITY_BACKEND_API_SERVICE_ID: &str = "sdkwork-community-backend-api";
pub const COMMUNITY_OPEN_API_SERVICE_ID: &str = "sdkwork-community-open-api";
pub const SEARCH_APP_API_SERVICE_ID: &str = "sdkwork-search-app-api";
pub const SEARCH_BACKEND_API_SERVICE_ID: &str = "sdkwork-search-backend-api";
pub const VOICE_APP_API_SERVICE_ID: &str = "sdkwork-voice-app-api";
pub const VOICE_BACKEND_API_SERVICE_ID: &str = "sdkwork-voice-backend-api";
pub const IMAGE_APP_API_SERVICE_ID: &str = "sdkwork-image-app-api";
pub const COURSE_APP_API_SERVICE_ID: &str = "sdkwork-course-app-api";
pub const COURSE_BACKEND_API_SERVICE_ID: &str = "sdkwork-course-backend-api";
pub const CMS_APP_API_SERVICE_ID: &str = "sdkwork-cms-app-api";
pub const CMS_APP_API_PREFIX: &str = "/app/v3/api/cms";
pub const CMS_BACKEND_API_SERVICE_ID: &str = "sdkwork-cms-backend-api";
pub const CMS_BACKEND_API_PREFIX: &str = "/backend/v3/api/cms";
pub const CMS_OPEN_API_SERVICE_ID: &str = "sdkwork-cms-open-api";
pub const CMS_OPEN_API_PREFIX: &str = "/cms/v3/api";
pub const FORUM_APP_API_SERVICE_ID: &str = "sdkwork-forum-app-api";
pub const FORUM_APP_API_PREFIX: &str = "/app/v3/api/forum";
pub const FORUM_BACKEND_API_SERVICE_ID: &str = "sdkwork-forum-backend-api";
pub const FORUM_BACKEND_API_PREFIX: &str = "/backend/v3/api/forum";
pub const FORUM_OPEN_API_SERVICE_ID: &str = "sdkwork-forum-open-api";
pub const FORUM_OPEN_API_PREFIX: &str = "/forum/v3/api";
pub const SDKWORK_APPSTORE_APP_API_SERVICE_ID: &str = "sdkwork-appstore-app-api";
pub const SDKWORK_APPSTORE_BACKEND_API_SERVICE_ID: &str = "sdkwork-appstore-backend-api";
pub const SDKWORK_APPSTORE_OPEN_API_SERVICE_ID: &str = "sdkwork-appstore-open-api";
pub const MAIL_APP_API_SERVICE_ID: &str = "sdkwork-mail-app-api";
pub const MAIL_BACKEND_API_SERVICE_ID: &str = "sdkwork-mail-backend-api";
pub const MCP_APP_API_SERVICE_ID: &str = "sdkwork-mcp-app-api";
pub const MCP_BACKEND_API_SERVICE_ID: &str = "sdkwork-mcp-backend-api";
pub const DEFAULT_MAX_REQUEST_BODY_BYTES: usize = 32 * 1024 * 1024;

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
    pub realtime: GatewayRealtimeConfig,
    #[serde(default)]
    pub database: GatewayDatabaseConfig,
    #[serde(default)]
    pub mode: GatewayMode,
    #[serde(default = "default_dependency_surfaces")]
    pub dependency_surfaces: Vec<DependencyApiSurfaceConfig>,
    #[serde(default)]
    pub request: GatewayRequestConfig,
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

/// Platform gateway realtime hosting toggle
/// (`ADR-20260809-platform-gateway-realtime-hosting`): when enabled together
/// with the `foundation-im-realtime` Cargo feature and an enabled `sdkwork-im`
/// surface, the gateway embeds the IM session-gateway realtime plane and
/// terminates its WebSocket upgrade plus declared TCP/UDP/QUIC link binds.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRealtimeConfig {
    #[serde(default)]
    pub enabled: bool,
}

/// Embedded database lifecycle defaults applied before any module bootstrap.
///
/// `autoMigrate` defaults to `true` so a simple configuration initializes
/// every embedded database module on first start (fresh install or version
/// upgrade). Operators who manage schema lifecycle explicitly set it to
/// `false`; `SDKWORK_DATABASE_AUTO_MIGRATE` still overrides this value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayDatabaseConfig {
    #[serde(default = "default_true")]
    pub auto_migrate: bool,
}

impl Default for GatewayDatabaseConfig {
    fn default() -> Self {
        Self { auto_migrate: true }
    }
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GatewayMode {
    #[default]
    Embedded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DependencyRuntimeMode {
    #[default]
    Embedded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorsConfig {
    #[serde(default = "default_true")]
    pub allow_any_origin: bool,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Registered console host pattern
    /// (`<scheme>://<label><suffix>.<registered base domain>`).
    ///
    /// Every module browser console is served from its own cloud sub-domain and
    /// calls this edge cross-origin, and origin matching has no sub-domain
    /// wildcard, so a deployment either enumerates thousands of exact origins or
    /// declares the closed pattern. `None` keeps exact-origin-only matching.
    #[serde(default)]
    pub registered_console_hosts: Option<RegisteredConsoleHosts>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRequestConfig {
    #[serde(default = "default_max_request_body_bytes")]
    pub max_body_bytes: usize,
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
    /// OpenTelemetry OTLP export for spans.
    #[serde(default)]
    pub otel: GatewayOtelConfig,
}

/// OpenTelemetry OTLP export configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayOtelConfig {
    /// Export request spans to the OTLP endpoint when enabled.
    #[serde(default)]
    pub enabled: bool,
    /// OTLP exporter endpoint (gRPC by default; OTLP/gRPC is assumed).
    #[serde(default = "default_otel_endpoint")]
    pub endpoint: String,
    /// Override for the OpenTelemetry service name.
    #[serde(default)]
    pub service_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewaySecurityConfig {
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Accept requests whose Host header is a raw IP literal (IPv4/IPv6) even
    /// when `allowed_hosts` is non-empty. Disabled by default: production
    /// keeps the exact domain allowlist. Standalone container deployments
    /// that must stay reachable directly through the host IP (operator
    /// smoke checks, LAN clients, load-balancer health probes) enable it at
    /// deploy time via `SDKWORK_API_CLOUD_GATEWAY_SECURITY_ALLOW_IP_HOSTS`.
    #[serde(default)]
    pub allow_ip_hosts: bool,
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
    /// Rate-limit backend: `memory` (in-process token buckets) or `redis`
    /// (atomic cluster-wide buckets via Lua).
    #[serde(default = "default_rate_limit_backend")]
    pub rate_limit_backend: String,
    /// Redis connection URL used when `rateLimitBackend = "redis"`.
    #[serde(default)]
    pub redis_url: Option<String>,
    /// Key namespace prefix for Redis rate-limit buckets.
    #[serde(default = "default_redis_key_prefix")]
    pub redis_key_prefix: String,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGatewayRuntimeConfig {
    service: Option<GatewayServiceConfig>,
    server: Option<RawGatewayServerConfig>,
    realtime: Option<GatewayRealtimeConfig>,
    database: Option<GatewayDatabaseConfig>,
    mode: Option<GatewayMode>,
    dependency_surfaces: Option<Vec<DependencyApiSurfaceConfig>>,
    request: Option<GatewayRequestConfig>,
    observability: Option<GatewayObservabilityConfig>,
    security: Option<GatewaySecurityConfig>,
    traffic: Option<GatewayTrafficConfig>,
    policy: Option<GatewayPolicyConfig>,
    diagnostics: Option<GatewayDiagnosticsConfig>,
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
    app: Option<SdkworkManifestApp>,
    backend: Option<SdkworkManifestBackend>,
    sdk_dependencies: Option<Vec<SdkworkManifestDependency>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkManifestApp {
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkManifestBackend {
    app_id: Option<String>,
    service_id: Option<String>,
    default_bind: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    #[serde(default, alias = "embeddedExecutableExport")]
    executable_export: Option<String>,
    #[serde(default)]
    cargo_feature: Option<String>,
    #[serde(default)]
    cargo_dependency: Option<String>,
    #[serde(default)]
    coverage: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkComponentSpec {
    #[serde(default)]
    api_surfaces: Vec<SdkworkComponentApiSurface>,
    #[serde(default)]
    dependencies: Vec<SdkworkComponentDependency>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkComponentApiSurface {
    surface: String,
    prefix: String,
    ownership: String,
    dependency_service_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkworkComponentDependency {
    service_id: String,
    workspace: String,
    sdk_family: String,
    api_authority: String,
    #[serde(default)]
    runtime_modes: Vec<DependencyRuntimeMode>,
    #[serde(default, alias = "embeddedExecutableExport")]
    executable_export: Option<String>,
    #[serde(default)]
    cargo_feature: Option<String>,
    #[serde(default)]
    cargo_dependency: Option<String>,
    #[serde(default)]
    coverage: Option<String>,
}

impl Default for GatewayRuntimeConfig {
    fn default() -> Self {
        Self {
            service: GatewayServiceConfig::default(),
            server: GatewayServerConfig::default(),
            realtime: GatewayRealtimeConfig::default(),
            database: GatewayDatabaseConfig::default(),
            mode: GatewayMode::default(),
            dependency_surfaces: default_dependency_surfaces(),
            request: GatewayRequestConfig::default(),
            observability: GatewayObservabilityConfig::default(),
            security: GatewaySecurityConfig::default(),
            traffic: GatewayTrafficConfig::default(),
            policy: GatewayPolicyConfig::default(),
            diagnostics: GatewayDiagnosticsConfig::default(),
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
        if let Some(realtime) = raw.realtime {
            config.realtime = realtime;
        }
        if let Some(database) = raw.database {
            config.database = database;
        }
        if let Some(mode) = raw.mode {
            config.mode = mode;
        }
        if let Some(dependency_surfaces) = raw.dependency_surfaces {
            config.dependency_surfaces = dependency_surfaces;
        }
        if let Some(request) = raw.request {
            config.request = request;
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
        if let Some(cors) = raw.cors {
            config.cors = cors;
        }

        config.finalize_registered_client_cors();
        config.validate()?;
        Ok(config)
    }

    pub fn from_sdkwork_manifest_str(input: &str) -> Result<Self, GatewayConfigError> {
        let manifest = serde_json::from_str::<SdkworkAppManifest>(input)?;
        let mut config = Self::from_sdkwork_manifest(manifest);
        config.finalize_registered_client_cors();
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

    pub fn from_component_spec_str(input: &str) -> Result<Self, GatewayConfigError> {
        let component = serde_json::from_str::<SdkworkComponentSpec>(input)?;
        let config = Self {
            dependency_surfaces: dependency_surfaces_from_component(component)?,
            ..Self::default()
        };
        config.validate()?;
        Ok(config)
    }

    pub fn from_component_spec_path(path: impl AsRef<Path>) -> Result<Self, GatewayConfigError> {
        let path = path.as_ref();
        let input = fs::read_to_string(path).map_err(|source| GatewayConfigError::ReadFile {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_component_spec_str(input.as_str())
    }

    pub fn from_application_root(path: impl AsRef<Path>) -> Result<Self, GatewayConfigError> {
        let application_root = path.as_ref();
        let config =
            Self::from_sdkwork_manifest_path(application_root.join("sdkwork.app.config.json"))?;
        config.with_application_component_surfaces(application_root)
    }

    /// Synchronize executable dependency surfaces from the application component contract.
    ///
    /// Runtime TOML files own operational settings such as bind, security, traffic, and
    /// observability. The component spec joins dependency API prefixes with Cargo integration
    /// evidence, so an explicit runtime profile cannot replace that contract with a partial list.
    pub fn with_application_component_surfaces(
        mut self,
        application_root: impl AsRef<Path>,
    ) -> Result<Self, GatewayConfigError> {
        let component = Self::from_component_spec_path(
            application_root.as_ref().join("specs/component.spec.json"),
        )?;
        self.dependency_surfaces = component.dependency_surfaces;
        self.finalize_registered_client_cors();
        self.validate()?;
        Ok(self)
    }

    /// Compatibility alias for callers compiled against the earlier method name.
    pub fn with_application_manifest_surfaces(
        self,
        application_root: impl AsRef<Path>,
    ) -> Result<Self, GatewayConfigError> {
        self.with_application_component_surfaces(application_root)
    }

    pub fn with_env_overrides(
        mut self,
        env: &HashMap<String, String>,
    ) -> Result<Self, GatewayConfigError> {
        if let Some(environment) = env.get("SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT") {
            self.service.environment = normalize_environment(environment)?;
        }
        if let Some(bind) = env.get("SDKWORK_API_CLOUD_GATEWAY_BIND") {
            self.server.bind = bind.clone();
        }
        if let Some(mode) = env.get("SDKWORK_API_CLOUD_GATEWAY_MODE") {
            self.mode = GatewayMode::parse(mode)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_REALTIME_ENABLED") {
            self.realtime.enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_REALTIME_ENABLED", enabled)?;
        }
        if let Some(max_body_bytes) = env.get("SDKWORK_API_CLOUD_GATEWAY_MAX_BODY_BYTES") {
            self.request.max_body_bytes =
                parse_usize_env("SDKWORK_API_CLOUD_GATEWAY_MAX_BODY_BYTES", max_body_bytes)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_ENABLED") {
            self.observability.access_log_enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_ENABLED") {
            self.observability.audit_log_enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_ENABLED", enabled)?;
        }
        if let Some(sample_rate) =
            env.get("SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS")
        {
            self.observability.access_log_sample_rate_basis_points = parse_sample_rate_env(
                "SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS",
                sample_rate,
            )?;
        }
        if let Some(sample_rate) =
            env.get("SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_SAMPLE_RATE_BASIS_POINTS")
        {
            self.observability.audit_log_sample_rate_basis_points = parse_sample_rate_env(
                "SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_SAMPLE_RATE_BASIS_POINTS",
                sample_rate,
            )?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_METRICS_ENABLED") {
            self.observability.metrics_enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_METRICS_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_TRACING_ENABLED") {
            self.observability.tracing_enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_TRACING_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_SECURITY_HEADERS_ENABLED") {
            self.security.secure_headers_enabled = parse_bool_env(
                "SDKWORK_API_CLOUD_GATEWAY_SECURITY_HEADERS_ENABLED",
                enabled,
            )?;
        }
        if let Some(allowed_hosts) = env.get("SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS") {
            self.security.allowed_hosts = parse_csv_env(allowed_hosts);
        }
        if let Some(allow_ip_hosts) = env.get("SDKWORK_API_CLOUD_GATEWAY_SECURITY_ALLOW_IP_HOSTS") {
            self.security.allow_ip_hosts = parse_bool_env(
                "SDKWORK_API_CLOUD_GATEWAY_SECURITY_ALLOW_IP_HOSTS",
                allow_ip_hosts,
            )?;
        }
        if let Some(allow_any_origin) = env.get("SDKWORK_API_CLOUD_GATEWAY_CORS_ALLOW_ANY_ORIGIN") {
            self.cors.allow_any_origin = parse_bool_env(
                "SDKWORK_API_CLOUD_GATEWAY_CORS_ALLOW_ANY_ORIGIN",
                allow_any_origin,
            )?;
        }
        if let Some(allowed_origins) = env.get("SDKWORK_CORS_ALLOWED_ORIGINS") {
            self.cors.allowed_origins = parse_csv_env(allowed_origins);
        }
        // Registered console host pattern: a half-configured or invalid pattern
        // must fail loudly at startup rather than silently rejecting every
        // module console preflight. Resolved through the injected environment so
        // the pattern obeys the same override contract as every other key.
        match registered_console_hosts_from_lookup(|key| env.get(key).cloned()) {
            Ok(Some(hosts)) => self.cors.registered_console_hosts = Some(hosts),
            Ok(None) => {}
            Err(reason) => {
                return Err(invalid_value("cors.registeredConsoleHosts", reason));
            }
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_ENABLED") {
            self.traffic.rate_limit_enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_POLICY_ENABLED") {
            self.policy.enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_POLICY_ENABLED", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_POLICY_DRY_RUN") {
            self.policy.dry_run =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_POLICY_DRY_RUN", enabled)?;
        }
        if let Some(enabled) = env.get("SDKWORK_API_CLOUD_GATEWAY_DIAGNOSTICS_ENABLED") {
            self.diagnostics.enabled =
                parse_bool_env("SDKWORK_API_CLOUD_GATEWAY_DIAGNOSTICS_ENABLED", enabled)?;
        }
        if let Some(path) = env.get("SDKWORK_API_CLOUD_GATEWAY_DIAGNOSTICS_PATH") {
            self.diagnostics.path = path.clone();
        }
        self.finalize_registered_client_cors();
        self.validate()?;
        Ok(self)
    }

    /// Ensures registered SDKWork desktop and mini program client origins are
    /// always present on the configured CORS allowlist.
    pub fn finalize_registered_client_cors(&mut self) {
        if self.cors.allow_any_origin {
            return;
        }
        merge_registered_sdkwork_client_origins(&mut self.cors.allowed_origins);
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

        // The registered console host pattern is validated in every environment,
        // production included: it is a closed registry, so an invalid pattern is
        // a configuration defect rather than a posture choice.
        if let Some(hosts) = &self.cors.registered_console_hosts {
            hosts
                .validate()
                .map_err(|reason| invalid_value("cors.registeredConsoleHosts", reason))?;
        }

        if is_production_like_environment(self.service.environment.as_str())
            && self
                .dependency_surfaces
                .iter()
                .any(|surface| surface.surface.eq_ignore_ascii_case("app"))
        {
            if self.cors.allow_any_origin {
                return Err(invalid_value(
                    "cors.allowAnyOrigin",
                    "must be false for a production-like gateway that exposes app-api",
                ));
            }
            if self.cors.registered_console_hosts.is_none() {
                if self.cors.allowed_origins.is_empty() {
                    return Err(invalid_value(
                        "cors.allowedOrigins",
                        "must contain at least one exact HTTP(S) origin, or configure cors.registeredConsoleHosts, for a production-like gateway that exposes app-api",
                    ));
                }
                if !self
                    .cors
                    .allowed_origins
                    .iter()
                    .any(|origin| is_exact_http_origin(origin))
                {
                    return Err(invalid_value(
                        "cors.allowedOrigins",
                        "must contain at least one exact HTTP(S) origin, or configure cors.registeredConsoleHosts, for a production-like gateway that exposes app-api",
                    ));
                }
            }
            for origin in &self.cors.allowed_origins {
                validate_exact_allowed_origin("cors.allowedOrigins[]", origin)?;
            }
        }

        for surface in &self.dependency_surfaces {
            validate_non_empty(
                "dependencySurfaces[].serviceId",
                surface.service_id.as_str(),
            )?;
            validate_non_empty("dependencySurfaces[].workspace", surface.workspace.as_str())?;
            validate_non_empty(
                "dependencySurfaces[].sdkFamily",
                surface.sdk_family.as_str(),
            )?;
            validate_non_empty(
                "dependencySurfaces[].apiAuthority",
                surface.api_authority.as_str(),
            )?;
            validate_non_empty("dependencySurfaces[].surface", surface.surface.as_str())?;
            validate_non_empty(
                "dependencySurfaces[].apiPrefix",
                surface.api_prefix.as_str(),
            )?;
            validate_non_empty(
                "dependencySurfaces[].executableExport",
                surface.executable_export.as_deref().unwrap_or_default(),
            )?;
            validate_non_empty(
                "dependencySurfaces[].cargoFeature",
                surface.cargo_feature.as_deref().unwrap_or_default(),
            )?;
            validate_non_empty(
                "dependencySurfaces[].cargoDependency",
                surface.cargo_dependency.as_deref().unwrap_or_default(),
            )?;
            validate_non_empty("dependencySurfaces[].coverage", surface.coverage.as_str())?;
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

        match self.traffic.rate_limit_backend.as_str() {
            "memory" => {}
            "redis" => {
                if self.traffic.redis_url.as_deref().is_none_or(str::is_empty) {
                    return Err(GatewayConfigError::InvalidValue {
                        key: "traffic.redisUrl".to_string(),
                        reason: "required when traffic.rateLimitBackend = \"redis\"".to_string(),
                    });
                }
            }
            other => {
                return Err(GatewayConfigError::InvalidValue {
                    key: "traffic.rateLimitBackend".to_string(),
                    reason: format!("must be \"memory\" or \"redis\", found {other:?}"),
                });
            }
        }

        Ok(())
    }

    fn from_sdkwork_manifest(manifest: SdkworkAppManifest) -> Self {
        let mut config = Self::default();
        let backend = manifest.backend;
        let service_name = backend
            .as_ref()
            .and_then(|backend| backend.service_id.clone())
            .or_else(|| backend.as_ref().and_then(|backend| backend.app_id.clone()))
            .or_else(|| manifest.app.and_then(|app| app.key))
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
        }

        config
    }
}

fn dependency_surfaces_from_component(
    component: SdkworkComponentSpec,
) -> Result<Vec<DependencyApiSurfaceConfig>, GatewayConfigError> {
    let mut dependencies = HashMap::new();
    for dependency in component.dependencies {
        let service_id = dependency.service_id.clone();
        if dependencies
            .insert(service_id.clone(), dependency)
            .is_some()
        {
            return Err(invalid_value(
                "specs/component.spec.json#dependencies[].serviceId",
                format!("duplicate dependency service id `{service_id}`"),
            ));
        }
    }

    component
        .api_surfaces
        .into_iter()
        .filter(|surface| surface.ownership.eq_ignore_ascii_case("dependency"))
        .map(|surface| {
            let service_id = surface
                .dependency_service_id
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    invalid_value(
                        "specs/component.spec.json#apiSurfaces[].dependencyServiceId",
                        format!(
                            "dependency surface `{}` must name its dependency service",
                            surface.prefix
                        ),
                    )
                })?;
            let dependency = dependencies.get(service_id).ok_or_else(|| {
                invalid_value(
                    "specs/component.spec.json#apiSurfaces[].dependencyServiceId",
                    format!(
                        "dependency surface `{}` references undeclared service `{service_id}`",
                        surface.prefix
                    ),
                )
            })?;
            Ok(DependencyApiSurfaceConfig::from_component_surface(
                surface, dependency,
            ))
        })
        .collect()
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
            "embedded" | "all-in-one" | "all_in_one" => Ok(Self::Embedded),
            other => Err(GatewayConfigError::InvalidMode(other.to_string())),
        }
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_any_origin: true,
            allowed_origins: Vec::new(),
            registered_console_hosts: None,
        }
    }
}

impl Default for GatewayRequestConfig {
    fn default() -> Self {
        Self {
            max_body_bytes: default_max_request_body_bytes(),
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
            otel: GatewayOtelConfig::default(),
        }
    }
}

impl Default for GatewaySecurityConfig {
    fn default() -> Self {
        Self {
            allowed_hosts: Vec::new(),
            allow_ip_hosts: false,
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
            rate_limit_backend: default_rate_limit_backend(),
            redis_url: None,
            redis_key_prefix: default_redis_key_prefix(),
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
        }
    }

    fn from_component_surface(
        surface: SdkworkComponentApiSurface,
        dependency: &SdkworkComponentDependency,
    ) -> Self {
        Self {
            service_id: dependency.service_id.clone(),
            workspace: dependency.workspace.clone(),
            sdk_family: dependency.sdk_family.clone(),
            api_authority: dependency.api_authority.clone(),
            surface: surface.surface,
            api_prefix: surface.prefix,
            runtime_mode: dependency
                .runtime_modes
                .first()
                .copied()
                .unwrap_or_default(),
            same_origin_allowed: true,
            executable_export: dependency.executable_export.clone(),
            cargo_feature: dependency.cargo_feature.clone(),
            cargo_dependency: dependency.cargo_dependency.clone(),
            coverage: dependency.coverage.clone().unwrap_or_default(),
        }
    }
}

fn default_dependency_surfaces() -> Vec<DependencyApiSurfaceConfig> {
    vec![DependencyApiSurfaceConfig {
        service_id: APPBASE_APP_API_SERVICE_ID.to_string(),
        workspace: "sdkwork-iam".to_string(),
        sdk_family: "sdkwork-iam-app-sdk".to_string(),
        api_authority: "sdkwork-iam-app-api".to_string(),
        surface: "app".to_string(),
        api_prefix: APPBASE_APP_API_PREFIX.to_string(),
        runtime_mode: DependencyRuntimeMode::Embedded,
        same_origin_allowed: true,
        executable_export: Some("sdkwork_api_iam_assembly::assemble_business_router".to_string()),
        cargo_feature: Some("foundation-appbase".to_string()),
        cargo_dependency: Some("sdkwork-api-iam-assembly".to_string()),
        coverage: "appbase-iam-routes".to_string(),
    }]
}

fn default_executable_export_for_service(service_id: &str) -> Option<String> {
    (service_id == APPBASE_APP_API_SERVICE_ID)
        .then(|| "sdkwork_api_iam_assembly::assemble_business_router".to_string())
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

fn default_max_request_body_bytes() -> usize {
    DEFAULT_MAX_REQUEST_BODY_BYTES
}

fn default_otel_endpoint() -> String {
    "http://localhost:4317".to_string()
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

fn default_rate_limit_backend() -> String {
    "memory".to_string()
}

fn default_redis_key_prefix() -> String {
    "sdkwork:gateway:ratelimit".to_string()
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

fn default_true() -> bool {
    true
}

fn is_json_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}

/// Normalizes gateway route paths using shared SDKWork path helpers.
pub fn normalize_gateway_route_path(path: &str, fallback: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return fallback.to_owned();
    }
    let normalized = sdkwork_utils_rust::path::normalize_path(trimmed);
    if normalized.is_empty() {
        fallback.to_owned()
    } else if normalized.starts_with('/') {
        normalized
    } else {
        format!("/{normalized}")
    }
}

fn parse_bool_env(key: &str, value: &str) -> Result<bool, GatewayConfigError> {
    sdkwork_utils_rust::boolean::parse_bool(value).ok_or_else(|| {
        GatewayConfigError::InvalidBoolean {
            key: key.to_string(),
            value: value.to_string(),
        }
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

fn is_production_like_environment(environment: &str) -> bool {
    matches!(
        environment.trim().to_ascii_lowercase().as_str(),
        "prod" | "production" | "stage" | "staging" | "live"
    )
}

fn normalize_environment(value: &str) -> Result<String, GatewayConfigError> {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "development" | "test" | "staging" | "production" | "demo"
    ) {
        return Ok(normalized);
    }
    Err(GatewayConfigError::InvalidValue {
        key: "SDKWORK_API_CLOUD_GATEWAY_ENVIRONMENT".to_owned(),
        reason: format!("unsupported lifecycle environment `{value}`"),
    })
}

fn is_exact_http_origin(origin: &str) -> bool {
    validate_exact_http_origin("cors.allowedOrigins[]", origin).is_ok()
}

/// A first-party client shell origin: either a generic `app://` / `tauri://`
/// authority or one of the canonical registered SDKWork client origins the
/// shared registry publishes (`CORS_SPEC.md` section 3 fixes that set; it
/// includes the `dsh-app://app` desktop host, whose scheme is not `app://`).
fn is_exact_desktop_origin(origin: &str) -> bool {
    let trimmed = origin.trim();
    if is_registered_sdkwork_client_origin(trimmed) {
        return true;
    }
    let Some(authority) = trimmed
        .strip_prefix("app://")
        .or_else(|| trimmed.strip_prefix("tauri://"))
    else {
        return false;
    };
    !(authority.is_empty()
        || authority.contains(['/', '?', '#', '*'])
        || authority.contains('@')
        || trimmed != origin)
}

fn validate_exact_http_origin(key: &str, origin: &str) -> Result<(), GatewayConfigError> {
    let trimmed = origin.trim();
    let Some(authority) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    else {
        return Err(invalid_value(key, "must use the http or https scheme"));
    };
    if authority.is_empty()
        || authority.contains(['/', '?', '#', '*'])
        || authority.contains('@')
        || trimmed != origin
    {
        return Err(invalid_value(
            key,
            "must be an exact origin without credentials, wildcard, path, query, or fragment",
        ));
    }
    Ok(())
}

fn validate_exact_allowed_origin(key: &str, origin: &str) -> Result<(), GatewayConfigError> {
    if is_exact_http_origin(origin) || is_exact_desktop_origin(origin) {
        return Ok(());
    }
    Err(invalid_value(
        key,
        "must be an exact HTTP(S) origin or a registered desktop origin (app:// or tauri://) without credentials, wildcard, path, query, or fragment",
    ))
}

fn invalid_value(key: &str, reason: impl Into<String>) -> GatewayConfigError {
    GatewayConfigError::InvalidValue {
        key: key.to_string(),
        reason: reason.into(),
    }
}

impl Default for GatewayOtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_otel_endpoint(),
            service_name: None,
        }
    }
}
