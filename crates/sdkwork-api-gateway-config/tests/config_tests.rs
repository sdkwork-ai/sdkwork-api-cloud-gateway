use std::{collections::HashMap, fs, path::PathBuf, process::Command};

use sdkwork_api_gateway_config::{
    DependencyRuntimeMode, GatewayMode, GatewayPolicyRuleConfig, GatewayRuntimeConfig,
    APPBASE_APP_API_BASE_URL_ENV,
};

#[test]
fn default_config_declares_gateway_identity_and_appbase_dependency() {
    let config = GatewayRuntimeConfig::default();

    assert_eq!(config.service.name, "sdkwork-api-gateway");
    assert_eq!(config.server.bind, "127.0.0.1:3900");
    assert_eq!(config.mode, GatewayMode::Split);
    assert!(config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.service_id == "sdkwork-appbase-app-api"
            && surface.api_prefix == "/app/v3/api"
            && surface.cargo_feature.as_deref() == Some("foundation-appbase")
            && surface.cargo_dependency.as_deref() == Some("sdkwork_iam_http")
            && surface.executable_export.as_deref()
                == Some("sdkwork_iam_http::build_sdkwork_appbase_app_api_router")));
}

#[test]
fn parses_toml_and_applies_environment_overrides() {
    let toml = r#"
        [service]
        name = "sdkwork-api-gateway"
        environment = "test"

        [server]
        bind = "127.0.0.1:3901"

        mode = "split"

        [[upstreams]]
        serviceId = "sdkwork-appbase-app-api"
        baseUrl = "http://127.0.0.1:3800"
        required = true
        timeoutMs = 1500
        readinessPath = "/internal/readyz"

        [cors]
        allowAnyOrigin = false
        allowedOrigins = ["https://console.example.com"]

        [request]
        maxBodyBytes = 1048576
        timeoutMs = 5000

        [readiness]
        checkUpstreams = true
        timeoutMs = 750

        [observability]
        accessLogEnabled = true
        auditLogEnabled = true
        accessLogSampleRateBasisPoints = 2500
        auditLogSampleRateBasisPoints = 10000
        metricsEnabled = true
        metricsPath = "/internal/metrics"
        tracingEnabled = true
        redactHeaders = ["authorization", "access-token", "cookie", "x-api-key"]

        [security]
        allowedHosts = ["api.example.com"]
        trustedProxyCidrs = ["127.0.0.1/32"]
        secureHeadersEnabled = true
        maxHeaderBytes = 32768
        maxHeaderCount = 64
        rejectCredentialHeadersOnAnonymousAuthRoutes = true
        csrfGuardEnabled = false

        [security.waf]
        enabled = true
        mode = "monitor"
        inspectQuery = true
        inspectHeaders = true
        inspectJsonBody = false

        [traffic]
        rateLimitEnabled = true
        compatXRateLimitHeaders = true
        globalMaxInFlight = 1024

        [[traffic.rateLimits]]
        id = "auth-ip"
        matchPathPrefix = "/app/v3/api/auth"
        key = "client-ip"
        algorithm = "token-bucket"
        capacity = 60
        refillPerSecond = 1
        burst = 20
        rejectStatus = 429

        [[traffic.concurrencyLimits]]
        id = "appbase"
        matchServiceId = "sdkwork-appbase-app-api"
        maxInFlight = 256

        [resilience]
        retryEnabled = true
        maxAttempts = 3
        retryStatuses = [502, 503, 504]
        retryBackoffMs = 25
        circuitBreakerEnabled = true
        failureThreshold = 4
        openSeconds = 45
        halfOpenMaxAttempts = 2
    "#;

    let config = GatewayRuntimeConfig::from_toml_str(toml).expect("config parses");
    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_GATEWAY_BIND".to_string(),
        "127.0.0.1:4999".to_string(),
    );
    env.insert(
        "SDKWORK_APPBASE_APP_API_BASE_URL".to_string(),
        "http://127.0.0.1:4888".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_MAX_BODY_BYTES".to_string(),
        "2048".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_TIMEOUT_MS".to_string(),
        "2500".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_READINESS_CHECK_UPSTREAMS".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_READINESS_TIMEOUT_MS".to_string(),
        "1250".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_METRICS_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_AUDIT_LOG_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS".to_string(),
        "500".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_AUDIT_LOG_SAMPLE_RATE_BASIS_POINTS".to_string(),
        "750".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_SECURITY_HEADERS_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_ALLOWED_HOSTS".to_string(),
        "gw.example.com,api.example.com".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_RATE_LIMIT_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_RETRY_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_RETRY_MAX_ATTEMPTS".to_string(),
        "2".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_FAILURE_THRESHOLD".to_string(),
        "7".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_OPEN_SECONDS".to_string(),
        "15".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_CIRCUIT_BREAKER_HALF_OPEN_MAX_ATTEMPTS".to_string(),
        "3".to_string(),
    );

    let config = config
        .with_env_overrides(&env)
        .expect("env overrides apply");

    assert_eq!(config.server.bind, "127.0.0.1:4999");
    assert_eq!(
        config
            .upstream("sdkwork-appbase-app-api")
            .expect("appbase upstream")
            .base_url,
        "http://127.0.0.1:4888"
    );
    assert_eq!(
        config.cors.allowed_origins,
        vec!["https://console.example.com".to_string()]
    );
    assert_eq!(config.request.max_body_bytes, 2048);
    assert_eq!(config.request.timeout_ms, 2500);
    assert!(!config.readiness.check_upstreams);
    assert_eq!(config.readiness.timeout_ms, 1250);
    assert_eq!(
        config
            .upstream("sdkwork-appbase-app-api")
            .expect("appbase upstream")
            .readiness_path,
        "/internal/readyz"
    );
    assert!(config.observability.access_log_enabled);
    assert!(!config.observability.audit_log_enabled);
    assert_eq!(
        config.observability.access_log_sample_rate_basis_points,
        500
    );
    assert_eq!(config.observability.audit_log_sample_rate_basis_points, 750);
    assert!(!config.observability.metrics_enabled);
    assert_eq!(config.observability.metrics_path, "/internal/metrics");
    assert!(config.observability.tracing_enabled);
    assert_eq!(
        config.observability.redact_headers,
        vec![
            "authorization".to_string(),
            "access-token".to_string(),
            "cookie".to_string(),
            "x-api-key".to_string()
        ]
    );
    assert!(!config.security.secure_headers_enabled);
    assert_eq!(
        config.security.allowed_hosts,
        vec!["gw.example.com".to_string(), "api.example.com".to_string()]
    );
    assert_eq!(config.security.trusted_proxy_cidrs, vec!["127.0.0.1/32"]);
    assert_eq!(config.security.max_header_bytes, 32768);
    assert_eq!(config.security.max_header_count, 64);
    assert!(
        config
            .security
            .reject_credential_headers_on_anonymous_auth_routes
    );
    assert!(config.security.waf.enabled);
    assert_eq!(config.security.waf.mode, "monitor");
    assert!(!config.traffic.rate_limit_enabled);
    assert!(config.traffic.compat_x_rate_limit_headers);
    assert_eq!(config.traffic.global_max_in_flight, Some(1024));
    assert_eq!(config.traffic.rate_limits.len(), 1);
    assert_eq!(config.traffic.rate_limits[0].id, "auth-ip");
    assert_eq!(config.traffic.rate_limits[0].key, "client-ip");
    assert_eq!(config.traffic.concurrency_limits.len(), 1);
    assert_eq!(config.traffic.concurrency_limits[0].max_in_flight, 256);
    assert!(!config.resilience.retry_enabled);
    assert_eq!(config.resilience.max_attempts, 2);
    assert_eq!(config.resilience.retry_statuses, vec![502, 503, 504]);
    assert_eq!(config.resilience.retry_backoff_ms, 25);
    let resilience = serde_json::to_value(&config.resilience).expect("resilience serializes");
    assert_eq!(resilience["circuitBreakerEnabled"], false);
    assert_eq!(resilience["failureThreshold"], 7);
    assert_eq!(resilience["openSeconds"], 15);
    assert_eq!(resilience["halfOpenMaxAttempts"], 3);
}

#[test]
fn parses_all_tracked_config_templates() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for relative_path in [
        "configs/sdkwork-api-gateway.development.toml.example",
        "configs/sdkwork-api-gateway.test.toml.example",
        "configs/sdkwork-api-gateway.production.toml.example",
    ] {
        let path = root.join(relative_path);
        GatewayRuntimeConfig::from_path(&path)
            .unwrap_or_else(|error| panic!("{relative_path} should parse: {error}"));
    }
}

#[test]
fn parses_policy_rules_for_method_body_and_timeout_governance() {
    let config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [policy]
        enabled = true

        [[policy.rules]]
        id = "appbase-auth-write-policy"
        matchServiceId = "sdkwork-appbase-app-api"
        matchPathPrefix = "/app/v3/api/auth"
        allowedMethods = ["POST"]
        maxBodyBytes = 4096
        timeoutMs = 250
        "#,
    )
    .expect("policy config parses");

    assert!(config.policy.enabled);
    assert_eq!(config.policy.rules.len(), 1);
    assert_eq!(
        config.policy.rules[0],
        GatewayPolicyRuleConfig {
            id: "appbase-auth-write-policy".to_string(),
            match_service_id: Some("sdkwork-appbase-app-api".to_string()),
            match_path_prefix: Some("/app/v3/api/auth".to_string()),
            allowed_methods: vec!["POST".to_string()],
            max_body_bytes: Some(4096),
            timeout_ms: Some(250),
        }
    );
}

#[test]
fn policy_enabled_can_be_overridden_from_environment() {
    let config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [policy]
        enabled = true
        "#,
    )
    .expect("policy config parses");
    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_GATEWAY_POLICY_ENABLED".to_string(),
        "false".to_string(),
    );

    let config = config
        .with_env_overrides(&env)
        .expect("policy env override applies");

    assert!(!config.policy.enabled);
}

#[test]
fn policy_dry_run_can_be_configured_and_overridden_from_environment() {
    let config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [policy]
        enabled = true
        dryRun = true
        "#,
    )
    .expect("policy config parses");
    assert!(config.policy.dry_run);

    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_GATEWAY_POLICY_DRY_RUN".to_string(),
        "false".to_string(),
    );
    let config = config
        .with_env_overrides(&env)
        .expect("policy dry-run env override applies");

    assert!(!config.policy.dry_run);
}

#[test]
fn parses_diagnostics_config_and_environment_overrides() {
    let config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [diagnostics]
        enabled = false
        path = "/internal/gateway/diagnostics"
        includePolicyInventory = true
        includeRouteInventory = true
        "#,
    )
    .expect("diagnostics config parses");
    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_GATEWAY_DIAGNOSTICS_ENABLED".to_string(),
        "true".to_string(),
    );
    env.insert(
        "SDKWORK_API_GATEWAY_DIAGNOSTICS_PATH".to_string(),
        "internal/gateway/inventory".to_string(),
    );

    let config = config
        .with_env_overrides(&env)
        .expect("diagnostics env overrides apply");

    assert!(config.diagnostics.enabled);
    assert_eq!(config.diagnostics.path, "internal/gateway/inventory");
    assert!(config.diagnostics.include_policy_inventory);
    assert!(config.diagnostics.include_route_inventory);
}

#[test]
fn rejects_sample_rates_above_basis_point_range() {
    let error = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        accessLogSampleRateBasisPoints = 10001
        "#,
    )
    .expect_err("sample rate above 10000 basis points should be rejected")
    .to_string();

    assert!(error.contains("accessLogSampleRateBasisPoints"));
    assert!(error.contains("10000"));
}

#[test]
fn rejects_invalid_policy_rule_identity_and_body_limit() {
    let empty_id_error = GatewayRuntimeConfig::from_toml_str(
        r#"
        [policy]
        enabled = true

        [[policy.rules]]
        id = ""
        maxBodyBytes = 1024
        "#,
    )
    .expect_err("policy rule id should be stable and non-empty")
    .to_string();
    assert!(empty_id_error.contains("policy.rules[].id"));

    let zero_body_error = GatewayRuntimeConfig::from_toml_str(
        r#"
        [policy]
        enabled = true

        [[policy.rules]]
        id = "zero-body"
        maxBodyBytes = 0
        "#,
    )
    .expect_err("zero maxBodyBytes should be rejected")
    .to_string();
    assert!(zero_body_error.contains("policy.rules[].maxBodyBytes"));
}

#[test]
fn rejects_invalid_traffic_rule_identity() {
    let error = GatewayRuntimeConfig::from_toml_str(
        r#"
        [traffic]
        rateLimitEnabled = true

        [[traffic.rateLimits]]
        id = ""
        capacity = 10
        "#,
    )
    .expect_err("traffic rule id should be stable and non-empty")
    .to_string();

    assert!(error.contains("traffic.rateLimits[].id"));
}

#[test]
fn env_overrides_are_range_validated() {
    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS".to_string(),
        "10001".to_string(),
    );

    let error = GatewayRuntimeConfig::default()
        .with_env_overrides(&env)
        .expect_err("invalid env sample rate should be rejected")
        .to_string();

    assert!(error.contains("SDKWORK_API_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS"));
    assert!(error.contains("10000"));
}

#[test]
fn sdkwork_manifest_drives_runtime_identity_bind_and_dependency_surfaces() {
    let manifest = r#"
        {
          "schemaVersion": 2,
          "appId": "custom-gateway",
          "name": "Custom Gateway",
          "backend": {
            "appId": "custom-gateway",
            "serviceId": "custom-gateway",
            "defaultBind": "127.0.0.1:4900"
          },
          "sdkDependencies": [
            {
              "serviceId": "sdkwork-appbase-app-api",
              "workspace": "sdkwork-appbase",
              "sdkFamily": "sdkwork-appbase-app-sdk",
              "apiAuthority": "sdkwork-appbase-app-api",
              "surface": "app",
              "apiPrefix": "/app/v3/api",
              "runtimeMode": "split-or-embedded",
              "requiredBaseUrlKey": "SDKWORK_APPBASE_APP_API_BASE_URL",
              "cargoFeature": "foundation-appbase",
              "cargoDependency": "sdkwork_iam_http",
              "executableExport": "sdkwork_iam_http::build_sdkwork_appbase_app_api_router"
            }
          ]
        }
    "#;

    let config =
        GatewayRuntimeConfig::from_sdkwork_manifest_str(manifest).expect("manifest parses");

    assert_eq!(config.service.name, "custom-gateway");
    assert_eq!(config.server.bind, "127.0.0.1:4900");
    assert_eq!(config.dependency_surfaces.len(), 1);
    let surface = &config.dependency_surfaces[0];
    assert_eq!(surface.runtime_mode, DependencyRuntimeMode::SplitOrEmbedded);
    assert_eq!(
        surface.required_base_url_key.as_deref(),
        Some(APPBASE_APP_API_BASE_URL_ENV)
    );
    assert_eq!(surface.cargo_feature.as_deref(), Some("foundation-appbase"));
    assert_eq!(
        surface.cargo_dependency.as_deref(),
        Some("sdkwork_iam_http")
    );
}

#[test]
fn application_manifest_declares_drive_and_notary_split_dependency_surfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    let drive = config
        .dependency_surfaces
        .iter()
        .find(|surface| surface.service_id == "sdkwork-drive-app-api")
        .expect("manifest declares Drive app-api surface");
    assert_eq!(drive.workspace, "sdkwork-drive");
    assert_eq!(drive.sdk_family, "sdkwork-drive-app-sdk");
    assert_eq!(drive.api_authority, "sdkwork-drive-app-api");
    assert_eq!(drive.surface, "app");
    assert_eq!(drive.api_prefix, "/app/v3/api/drive");
    assert_eq!(drive.runtime_mode, DependencyRuntimeMode::Split);
    assert_eq!(
        drive.required_base_url_key.as_deref(),
        Some("SDKWORK_DRIVE_APP_API_BASE_URL")
    );
    assert!(drive.executable_export.is_none());
    assert!(drive.cargo_feature.is_none());
    assert!(drive.cargo_dependency.is_none());

    let notary = config
        .dependency_surfaces
        .iter()
        .find(|surface| surface.service_id == "sdkwork-notary-app-api")
        .expect("manifest declares Notary app-api surface");
    assert_eq!(notary.workspace, "sdkwork-notary");
    assert_eq!(notary.sdk_family, "sdkwork-notary-app-sdk");
    assert_eq!(notary.api_authority, "sdkwork-notary-app-api");
    assert_eq!(notary.surface, "app");
    assert_eq!(notary.api_prefix, "/app/v3/api/notary");
    assert_eq!(notary.runtime_mode, DependencyRuntimeMode::Split);
    assert_eq!(
        notary.required_base_url_key.as_deref(),
        Some("SDKWORK_NOTARY_APP_API_BASE_URL")
    );
    assert!(notary.executable_export.is_none());
    assert!(notary.cargo_feature.is_none());
    assert!(notary.cargo_dependency.is_none());

    assert_eq!(
        config
            .upstream("sdkwork-drive-app-api")
            .expect("Drive upstream is derived")
            .base_url,
        "http://127.0.0.1:18080"
    );
    assert_eq!(
        config
            .upstream("sdkwork-notary-app-api")
            .expect("Notary upstream is derived")
            .base_url,
        "http://127.0.0.1:18092"
    );
}

#[test]
fn application_manifest_declares_claw_router_foundation_split_dependency_surfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    for (service_id, workspace, sdk_family, api_prefix, env_key, default_base_url) in [
        (
            "sdkwork-appbase-backend-api",
            "sdkwork-appbase",
            "sdkwork-appbase-backend-sdk",
            "/backend/v3/api/iam",
            "SDKWORK_APPBASE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:3801",
        ),
        (
            "sdkwork-commerce-app-api",
            "sdkwork-commerce",
            "sdkwork-commerce-app-sdk",
            "/app/v3/api",
            "SDKWORK_COMMERCE_APP_API_BASE_URL",
            "http://127.0.0.1:18180",
        ),
        (
            "sdkwork-commerce-backend-api",
            "sdkwork-commerce",
            "sdkwork-commerce-backend-sdk",
            "/backend/v3/api",
            "SDKWORK_COMMERCE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18181",
        ),
    ] {
        let surface = config
            .dependency_surfaces
            .iter()
            .find(|surface| surface.service_id == service_id)
            .unwrap_or_else(|| panic!("manifest declares {service_id} surface"));
        assert_eq!(surface.workspace, workspace);
        assert_eq!(surface.sdk_family, sdk_family);
        assert_eq!(surface.api_authority, service_id);
        assert_eq!(surface.api_prefix, api_prefix);
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::Split);
        assert_eq!(surface.required_base_url_key.as_deref(), Some(env_key));
        assert!(surface.executable_export.is_none());
        assert!(surface.cargo_feature.is_none());
        assert!(surface.cargo_dependency.is_none());

        assert_eq!(
            config
                .upstream(service_id)
                .unwrap_or_else(|| panic!("{service_id} upstream is derived"))
                .base_url,
            default_base_url
        );
    }
}

#[test]
fn application_manifest_declares_craw_chat_foundation_split_dependency_surfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    for (service_id, workspace, sdk_family, surface_kind, api_prefix, env_key, default_base_url) in [
        (
            "sdkwork-rtc-app-api",
            "sdkwork-rtc",
            "sdkwork-rtc-app-sdk",
            "app",
            "/app/v3/api/rtc",
            "SDKWORK_RTC_APP_API_BASE_URL",
            "http://127.0.0.1:18280",
        ),
        (
            "sdkwork-rtc-backend-api",
            "sdkwork-rtc",
            "sdkwork-rtc-backend-sdk",
            "backend",
            "/backend/v3/api/rtc",
            "SDKWORK_RTC_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18281",
        ),
        (
            "sdkwork-agent-app-api",
            "sdkwork-kernel",
            "sdkwork-agent-app-sdk",
            "app",
            "/app/v3/api/ai",
            "SDKWORK_AGENT_APP_API_BASE_URL",
            "http://127.0.0.1:18380",
        ),
        (
            "sdkwork-agent-backend-api",
            "sdkwork-kernel",
            "sdkwork-agent-backend-sdk",
            "backend",
            "/backend/v3/api/ai",
            "SDKWORK_AGENT_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18381",
        ),
        (
            "sdkwork-agent-open-api",
            "sdkwork-kernel",
            "sdkwork-agent-sdk",
            "open",
            "/agent/v3/api",
            "SDKWORK_AGENT_OPEN_API_BASE_URL",
            "http://127.0.0.1:18382",
        ),
        (
            "sdkwork-aiot-app-api",
            "sdkwork-aiot",
            "sdkwork-aiot-app-sdk",
            "app",
            "/app/v3/api/iot",
            "SDKWORK_AIOT_APP_API_BASE_URL",
            "http://127.0.0.1:18480",
        ),
        (
            "sdkwork-aiot-backend-api",
            "sdkwork-aiot",
            "sdkwork-aiot-backend-sdk",
            "backend",
            "/backend/v3/api/iot",
            "SDKWORK_AIOT_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18481",
        ),
    ] {
        let surface = config
            .dependency_surfaces
            .iter()
            .find(|surface| surface.service_id == service_id)
            .unwrap_or_else(|| panic!("manifest declares {service_id} surface"));
        assert_eq!(surface.workspace, workspace);
        assert_eq!(surface.sdk_family, sdk_family);
        assert_eq!(surface.api_authority, service_id);
        assert_eq!(surface.surface, surface_kind);
        assert_eq!(surface.api_prefix, api_prefix);
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::Split);
        assert_eq!(surface.required_base_url_key.as_deref(), Some(env_key));
        assert!(surface.executable_export.is_none());
        assert!(surface.cargo_feature.is_none());
        assert!(surface.cargo_dependency.is_none());

        assert_eq!(
            config
                .upstream(service_id)
                .unwrap_or_else(|| panic!("{service_id} upstream is derived"))
                .base_url,
            default_base_url
        );
    }
}

#[test]
fn application_manifest_declares_workspace_foundation_split_dependency_surfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    for (
        service_id,
        workspace,
        sdk_family,
        api_authority,
        surface_kind,
        api_prefix,
        env_key,
        default_base_url,
    ) in [
        (
            "sdkwork-drive-backend-api",
            "sdkwork-drive",
            "sdkwork-drive-backend-sdk",
            "sdkwork-drive.backend",
            "backend",
            "/backend/v3/api/drive",
            "SDKWORK_DRIVE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18081",
        ),
        (
            "sdkwork-drive-open-api",
            "sdkwork-drive",
            "sdkwork-drive-sdk",
            "sdkwork-drive.open",
            "open",
            "/open/v3/api/drive",
            "SDKWORK_DRIVE_OPEN_API_BASE_URL",
            "http://127.0.0.1:18082",
        ),
        (
            "sdkwork-drive-admin-storage-api",
            "sdkwork-drive",
            "sdkwork-drive-admin-storage-sdk",
            "sdkwork-drive.admin.storage",
            "admin",
            "/admin/v3/api/drive",
            "SDKWORK_DRIVE_ADMIN_STORAGE_API_BASE_URL",
            "http://127.0.0.1:18083",
        ),
        (
            "sdkwork-notary-backend-api",
            "sdkwork-notary",
            "sdkwork-notary-backend-sdk",
            "sdkwork-notary.backend",
            "backend",
            "/backend/v3/api/notary",
            "SDKWORK_NOTARY_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18093",
        ),
        (
            "sdkwork-memory-app-api",
            "sdkwork-memory",
            "sdkwork-memory-app-sdk",
            "sdkwork-memory.app",
            "app",
            "/app/v3/api/memory",
            "SDKWORK_MEMORY_APP_API_BASE_URL",
            "http://127.0.0.1:18580",
        ),
        (
            "sdkwork-memory-backend-api",
            "sdkwork-memory",
            "sdkwork-memory-backend-sdk",
            "sdkwork-memory.backend",
            "backend",
            "/backend/v3/api/memory",
            "SDKWORK_MEMORY_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18581",
        ),
        (
            "sdkwork-memory-open-api",
            "sdkwork-memory",
            "sdkwork-memory-sdk",
            "sdkwork-memory-open-api",
            "open",
            "/mem/v3/api/memory",
            "SDKWORK_MEMORY_OPEN_API_BASE_URL",
            "http://127.0.0.1:18582",
        ),
        (
            "sdkwork-knowledgebase-app-api",
            "sdkwork-knowledgebase",
            "sdkwork-knowledgebase-app-sdk",
            "sdkwork-knowledgebase.app",
            "app",
            "/app/v3/api/knowledge",
            "SDKWORK_KNOWLEDGEBASE_APP_API_BASE_URL",
            "http://127.0.0.1:18680",
        ),
        (
            "sdkwork-knowledgebase-backend-api",
            "sdkwork-knowledgebase",
            "sdkwork-knowledgebase-backend-sdk",
            "sdkwork-knowledgebase.backend",
            "backend",
            "/backend/v3/api/knowledge",
            "SDKWORK_KNOWLEDGEBASE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18681",
        ),
        (
            "sdkwork-knowledgebase-open-api",
            "sdkwork-knowledgebase",
            "sdkwork-knowledgebase-sdk",
            "sdkwork-knowledgebase.open",
            "open",
            "/knowledge/v3/api",
            "SDKWORK_KNOWLEDGEBASE_OPEN_API_BASE_URL",
            "http://127.0.0.1:18682",
        ),
        (
            "sdkwork-news-app-api",
            "sdkwork-news",
            "sdkwork-news-app-sdk",
            "sdkwork-news.app",
            "app",
            "/app/v3/api/news",
            "SDKWORK_NEWS_APP_API_BASE_URL",
            "http://127.0.0.1:18780",
        ),
        (
            "sdkwork-news-backend-api",
            "sdkwork-news",
            "sdkwork-news-backend-sdk",
            "sdkwork-news.backend",
            "backend",
            "/backend/v3/api/news",
            "SDKWORK_NEWS_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18781",
        ),
        (
            "sdkwork-news-open-api",
            "sdkwork-news",
            "sdkwork-news-sdk",
            "sdkwork-news.open",
            "open",
            "/open/v3/api/news",
            "SDKWORK_NEWS_OPEN_API_BASE_URL",
            "http://127.0.0.1:18782",
        ),
        (
            "sdkwork-notes-app-api",
            "sdkwork-notes",
            "sdkwork-notes-app-sdk",
            "sdkwork-notes-app-api",
            "app",
            "/app/v3/api/notes",
            "SDKWORK_NOTES_APP_API_BASE_URL",
            "http://127.0.0.1:18880",
        ),
        (
            "sdkwork-notes-backend-api",
            "sdkwork-notes",
            "sdkwork-notes-backend-sdk",
            "sdkwork-notes-backend-api",
            "backend",
            "/backend/v3/api/notes",
            "SDKWORK_NOTES_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18881",
        ),
        (
            "sdkwork-notes-open-api",
            "sdkwork-notes",
            "sdkwork-notes-sdk",
            "sdkwork-notes-open-api",
            "open",
            "/notes/v3/api",
            "SDKWORK_NOTES_OPEN_API_BASE_URL",
            "http://127.0.0.1:18882",
        ),
        (
            "sdkwork-music-app-api",
            "sdkwork-music",
            "sdkwork-music-app-sdk",
            "sdkwork-music-app-api",
            "app",
            "/app/v3/api/music",
            "SDKWORK_MUSIC_APP_API_BASE_URL",
            "http://127.0.0.1:18980",
        ),
        (
            "sdkwork-music-backend-api",
            "sdkwork-music",
            "sdkwork-music-backend-sdk",
            "sdkwork-music-backend-api",
            "backend",
            "/backend/v3/api/music",
            "SDKWORK_MUSIC_BACKEND_API_BASE_URL",
            "http://127.0.0.1:18981",
        ),
        (
            "sdkwork-generations-app-api",
            "sdkwork-generations",
            "sdkwork-generations-app-sdk",
            "sdkwork-generations-app-api",
            "app",
            "/app/v3/api/generations",
            "SDKWORK_GENERATIONS_APP_API_BASE_URL",
            "http://127.0.0.1:19080",
        ),
        (
            "sdkwork-generations-backend-api",
            "sdkwork-generations",
            "sdkwork-generations-backend-sdk",
            "sdkwork-generations-backend-api",
            "backend",
            "/backend/v3/api/generations",
            "SDKWORK_GENERATIONS_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19081",
        ),
        (
            "sdkwork-community-app-api",
            "sdkwork-community",
            "sdkwork-community-app-sdk",
            "sdkwork-community.app",
            "app",
            "/app/v3/api/community",
            "SDKWORK_COMMUNITY_APP_API_BASE_URL",
            "http://127.0.0.1:19180",
        ),
        (
            "sdkwork-community-backend-api",
            "sdkwork-community",
            "sdkwork-community-backend-sdk",
            "sdkwork-community.backend",
            "backend",
            "/backend/v3/api/community",
            "SDKWORK_COMMUNITY_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19181",
        ),
        (
            "sdkwork-community-open-api",
            "sdkwork-community",
            "sdkwork-community-sdk",
            "sdkwork-community.open",
            "open",
            "/community/v3/api",
            "SDKWORK_COMMUNITY_OPEN_API_BASE_URL",
            "http://127.0.0.1:19182",
        ),
    ] {
        let surface = config
            .dependency_surfaces
            .iter()
            .find(|surface| surface.service_id == service_id)
            .unwrap_or_else(|| panic!("manifest declares {service_id} surface"));
        assert_eq!(surface.workspace, workspace);
        assert_eq!(surface.sdk_family, sdk_family);
        assert_eq!(surface.api_authority, api_authority);
        assert_eq!(surface.surface, surface_kind);
        assert_eq!(surface.api_prefix, api_prefix);
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::Split);
        assert_eq!(surface.required_base_url_key.as_deref(), Some(env_key));
        assert!(surface.executable_export.is_none());
        assert!(surface.cargo_feature.is_none());
        assert!(surface.cargo_dependency.is_none());

        assert_eq!(
            config
                .upstream(service_id)
                .unwrap_or_else(|| panic!("{service_id} upstream is derived"))
                .base_url,
            default_base_url
        );
    }
}

#[test]
fn application_manifest_declares_materialized_content_foundation_split_dependency_surfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    for (
        service_id,
        workspace,
        sdk_family,
        api_authority,
        surface_kind,
        api_prefix,
        env_key,
        default_base_url,
    ) in [
        (
            "sdkwork-search-app-api",
            "sdkwork-search",
            "sdkwork-search-app-sdk",
            "sdkwork-search-app-api",
            "app",
            "/app/v3/api/search",
            "SDKWORK_SEARCH_APP_API_BASE_URL",
            "http://127.0.0.1:19280",
        ),
        (
            "sdkwork-search-backend-api",
            "sdkwork-search",
            "sdkwork-search-backend-sdk",
            "sdkwork-search-backend-api",
            "backend",
            "/backend/v3/api/search",
            "SDKWORK_SEARCH_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19281",
        ),
        (
            "sdkwork-voice-app-api",
            "sdkwork-voice",
            "sdkwork-voice-app-sdk",
            "sdkwork-voice-app-api",
            "app",
            "/app/v3/api/voice",
            "SDKWORK_VOICE_APP_API_BASE_URL",
            "http://127.0.0.1:19380",
        ),
        (
            "sdkwork-voice-backend-api",
            "sdkwork-voice",
            "sdkwork-voice-backend-sdk",
            "sdkwork-voice-backend-api",
            "backend",
            "/backend/v3/api/voice",
            "SDKWORK_VOICE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19381",
        ),
        (
            "sdkwork-image-app-api",
            "sdkwork-image",
            "sdkwork-image-app-sdk",
            "sdkwork-image-app-api",
            "app",
            "/app/v3/api/image",
            "SDKWORK_IMAGE_APP_API_BASE_URL",
            "http://127.0.0.1:19480",
        ),
        (
            "sdkwork-image-backend-api",
            "sdkwork-image",
            "sdkwork-image-backend-sdk",
            "sdkwork-image-backend-api",
            "backend",
            "/backend/v3/api/image",
            "SDKWORK_IMAGE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19481",
        ),
        (
            "sdkwork-image-open-api",
            "sdkwork-image",
            "sdkwork-image-sdk",
            "sdkwork-image-open-api",
            "open",
            "/image/v3/api",
            "SDKWORK_IMAGE_OPEN_API_BASE_URL",
            "http://127.0.0.1:19482",
        ),
        (
            "sdkwork-comments-app-api",
            "sdkwork-comments",
            "sdkwork-comments-app-sdk",
            "sdkwork-comments-app-api",
            "app",
            "/app/v3/api/comments",
            "SDKWORK_COMMENTS_APP_API_BASE_URL",
            "http://127.0.0.1:19580",
        ),
        (
            "sdkwork-comments-app-api",
            "sdkwork-comments",
            "sdkwork-comments-app-sdk",
            "sdkwork-comments-app-api",
            "app",
            "/app/v3/api/engagement",
            "SDKWORK_COMMENTS_APP_API_BASE_URL",
            "http://127.0.0.1:19580",
        ),
        (
            "sdkwork-comments-backend-api",
            "sdkwork-comments",
            "sdkwork-comments-backend-sdk",
            "sdkwork-comments-backend-api",
            "backend",
            "/backend/v3/api/comments",
            "SDKWORK_COMMENTS_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19581",
        ),
        (
            "sdkwork-comments-backend-api",
            "sdkwork-comments",
            "sdkwork-comments-backend-sdk",
            "sdkwork-comments-backend-api",
            "backend",
            "/backend/v3/api/engagement",
            "SDKWORK_COMMENTS_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19581",
        ),
        (
            "sdkwork-course-app-api",
            "sdkwork-course",
            "sdkwork-course-app-sdk",
            "sdkwork-course-app-api",
            "app",
            "/app/v3/api/courses",
            "SDKWORK_COURSE_APP_API_BASE_URL",
            "http://127.0.0.1:19680",
        ),
        (
            "sdkwork-course-app-api",
            "sdkwork-course",
            "sdkwork-course-app-sdk",
            "sdkwork-course-app-api",
            "app",
            "/app/v3/api/course_applications",
            "SDKWORK_COURSE_APP_API_BASE_URL",
            "http://127.0.0.1:19680",
        ),
        (
            "sdkwork-course-backend-api",
            "sdkwork-course",
            "sdkwork-course-backend-sdk",
            "sdkwork-course-backend-api",
            "backend",
            "/backend/v3/api/courses",
            "SDKWORK_COURSE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19681",
        ),
        (
            "sdkwork-course-backend-api",
            "sdkwork-course",
            "sdkwork-course-backend-sdk",
            "sdkwork-course-backend-api",
            "backend",
            "/backend/v3/api/course_applications",
            "SDKWORK_COURSE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19681",
        ),
        (
            "sdkwork-course-backend-api",
            "sdkwork-course",
            "sdkwork-course-backend-sdk",
            "sdkwork-course-backend-api",
            "backend",
            "/backend/v3/api/course_lessons",
            "SDKWORK_COURSE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19681",
        ),
        (
            "sdkwork-course-backend-api",
            "sdkwork-course",
            "sdkwork-course-backend-sdk",
            "sdkwork-course-backend-api",
            "backend",
            "/backend/v3/api/course_sections",
            "SDKWORK_COURSE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19681",
        ),
        (
            "sdkwork-messaging-app-api",
            "sdkwork-messaging",
            "sdkwork-messaging-app-sdk",
            "sdkwork-messaging-app-api",
            "app",
            "/app/v3/api/messaging",
            "SDKWORK_MESSAGING_APP_API_BASE_URL",
            "http://127.0.0.1:19780",
        ),
        (
            "sdkwork-messaging-backend-api",
            "sdkwork-messaging",
            "sdkwork-messaging-backend-sdk",
            "sdkwork-messaging-backend-api",
            "backend",
            "/backend/v3/api/messaging",
            "SDKWORK_MESSAGING_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19781",
        ),
    ] {
        let surface = config
            .dependency_surfaces
            .iter()
            .find(|surface| surface.service_id == service_id && surface.api_prefix == api_prefix)
            .unwrap_or_else(|| panic!("manifest declares {service_id} surface for {api_prefix}"));
        assert_eq!(surface.workspace, workspace);
        assert_eq!(surface.sdk_family, sdk_family);
        assert_eq!(surface.api_authority, api_authority);
        assert_eq!(surface.surface, surface_kind);
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::Split);
        assert_eq!(surface.required_base_url_key.as_deref(), Some(env_key));
        assert!(surface.executable_export.is_none());
        assert!(surface.cargo_feature.is_none());
        assert!(surface.cargo_dependency.is_none());

        assert_eq!(
            config
                .upstream(service_id)
                .unwrap_or_else(|| panic!("{service_id} upstream is derived"))
                .base_url,
            default_base_url
        );
    }

    for service_id in [
        "sdkwork-comments-app-api",
        "sdkwork-comments-backend-api",
        "sdkwork-course-app-api",
        "sdkwork-course-backend-api",
    ] {
        assert_eq!(
            config
                .upstreams
                .iter()
                .filter(|upstream| upstream.service_id == service_id)
                .count(),
            1,
            "{service_id} has one derived upstream even when multiple prefixes share the same SDK family"
        );
    }
}

#[test]
fn component_spec_uses_cargo_metadata_as_gateway_integration_authority() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let spec_path = root.join("specs/component.spec.json");
    let spec: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&spec_path).unwrap_or_else(|error| {
            panic!(
                "failed to read component spec {}: {error}",
                spec_path.display()
            )
        }))
        .expect("component spec parses");

    assert_eq!(
        spec["gatewayIntegration"]["authority"].as_str(),
        Some("cargo-workspace")
    );
    assert_eq!(
        spec["gatewayIntegration"]["catalogPolicy"].as_str(),
        Some("no-dedicated-gateway-catalog")
    );

    let verification_commands = spec["verification"]["commands"]
        .as_array()
        .expect("component spec must declare verification.commands");
    assert!(
        verification_commands.iter().any(|command| command
            .as_str()
            .is_some_and(|value| value == "cargo metadata --no-deps --format-version 1")),
        "component spec must verify gateway Cargo metadata instead of a separate gateway catalog"
    );

    let runtime_exports = spec["entrypoints"]
        .as_array()
        .expect("component spec declares entrypoints")
        .iter()
        .find(|entrypoint| {
            entrypoint["kind"].as_str() == Some("library")
                && entrypoint["path"].as_str() == Some("crates/sdkwork-api-gateway")
        })
        .and_then(|entrypoint| entrypoint["exports"].as_array())
        .expect("component spec declares gateway library exports");
    assert!(
        runtime_exports
            .iter()
            .any(|export| export.as_str() == Some("build_sdkwork_api_gateway_api_server_router")),
        "component spec must declare the canonical api-server router builder"
    );
    assert!(
        runtime_exports
            .iter()
            .all(|export| export.as_str() != Some("build_sdkwork_api_gateway_service_router")),
        "component spec must not preserve the legacy service router alias"
    );

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let metadata_output = Command::new(cargo)
        .current_dir(&root)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .expect("cargo metadata command runs");
    assert!(
        metadata_output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata_output.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata_output.stdout).expect("cargo metadata parses");
    let runtime_package = metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"].as_str() == Some("sdkwork-api-gateway"))
        })
        .expect("cargo metadata includes sdkwork-api-gateway");

    let dependencies = spec["dependencies"]
        .as_array()
        .expect("component spec declares dependencies");
    for dependency in dependencies {
        let runtime_modes = dependency["runtimeModes"]
            .as_array()
            .expect("dependency declares runtimeModes");
        let split_only = runtime_modes.len() == 1 && runtime_modes[0].as_str() == Some("split");

        if split_only {
            assert!(
                dependency.get("cargoFeature").is_none(),
                "split-only dependency {} must not declare cargoFeature",
                dependency["serviceId"]
            );
            assert!(
                dependency.get("cargoDependency").is_none(),
                "split-only dependency {} must not declare cargoDependency",
                dependency["serviceId"]
            );
            assert!(
                dependency.get("embeddedExecutableExport").is_none(),
                "split-only dependency {} must not declare embeddedExecutableExport",
                dependency["serviceId"]
            );
            continue;
        }

        let Some(cargo_feature) = dependency["cargoFeature"].as_str() else {
            continue;
        };
        let cargo_dependency = dependency["cargoDependency"]
            .as_str()
            .expect("embedded dependency declares cargoDependency");
        let feature_members = runtime_package["features"][cargo_feature]
            .as_array()
            .unwrap_or_else(|| panic!("runtime package declares feature {cargo_feature}"));
        assert!(
            feature_members
                .iter()
                .any(|member| member.as_str() == Some(format!("dep:{cargo_dependency}").as_str())),
            "feature {cargo_feature} must activate dep:{cargo_dependency}"
        );
        let runtime_dependencies = runtime_package["dependencies"]
            .as_array()
            .expect("runtime package declares dependencies");
        let cargo_dependency_entry = runtime_dependencies
            .iter()
            .find(|dependency| dependency["name"].as_str() == Some(cargo_dependency))
            .unwrap_or_else(|| {
                panic!("runtime package dependency table includes {cargo_dependency}")
            });
        assert_eq!(
            cargo_dependency_entry["optional"].as_bool(),
            Some(true),
            "embedded foundation dependency {cargo_dependency} must be optional and feature-gated"
        );
    }
}

#[test]
fn gateway_runtime_source_is_split_by_endpoint_responsibility() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gateway_src = root.join("crates/sdkwork-api-gateway/src");

    for module in ["health.rs", "openapi.rs", "diagnostics.rs"] {
        assert!(
            gateway_src.join(module).is_file(),
            "gateway endpoint responsibility must live in {module}"
        );
    }

    let runtime_source = fs::read_to_string(gateway_src.join("runtime.rs"))
        .expect("runtime source should be readable");
    for forbidden_inline_endpoint in [
        "async fn healthz",
        "async fn readyz",
        "async fn openapi_index",
        "async fn openapi_document",
        "async fn diagnostics_endpoint",
    ] {
        assert!(
            !runtime_source.contains(forbidden_inline_endpoint),
            "runtime.rs must not keep inline endpoint handler {forbidden_inline_endpoint}"
        );
    }
}
