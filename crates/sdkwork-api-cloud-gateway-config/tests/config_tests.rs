use std::{collections::HashMap, fs, path::PathBuf, process::Command};

use sdkwork_api_cloud_gateway_config::{
    DependencyRuntimeMode, GatewayMode, GatewayPolicyRuleConfig, GatewayRuntimeConfig,
    APPBASE_APP_API_BASE_URL_ENV, SDKWORK_IM_BACKEND_HTTP_SERVICE_ID,
    SDKWORK_IM_IM_HTTP_SERVICE_ID,
};

#[test]
fn default_config_declares_gateway_identity_and_appbase_dependency() {
    let config = GatewayRuntimeConfig::default();

    assert_eq!(config.service.name, "sdkwork-api-cloud-gateway");
    assert_eq!(config.server.bind, "127.0.0.1:3900");
    assert_eq!(config.mode, GatewayMode::Split);
    assert!(config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.service_id == "sdkwork-iam-app-api"
            && surface.api_prefix == "/app/v3/api"
            && surface.cargo_feature.as_deref() == Some("foundation-appbase")
            && surface.cargo_dependency.as_deref() == Some("sdkwork_routes_iam_app_api")
            && surface.executable_export.as_deref()
                == Some("sdkwork_routes_iam_app_api::build_sdkwork_iam_app_api_router")));
}

#[test]
fn parses_toml_and_applies_environment_overrides() {
    let toml = r#"
        [service]
        name = "sdkwork-api-cloud-gateway"
        environment = "test"

        [server]
        bind = "127.0.0.1:3901"

        mode = "split"

        [[upstreams]]
        serviceId = "sdkwork-iam-app-api"
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
        matchServiceId = "sdkwork-iam-app-api"
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
        "SDKWORK_API_CLOUD_GATEWAY_BIND".to_string(),
        "127.0.0.1:4999".to_string(),
    );
    env.insert(
        "SDKWORK_APPBASE_APP_API_BASE_URL".to_string(),
        "http://127.0.0.1:4888".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_MAX_BODY_BYTES".to_string(),
        "2048".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_TIMEOUT_MS".to_string(),
        "2500".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_READINESS_CHECK_UPSTREAMS".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_READINESS_TIMEOUT_MS".to_string(),
        "1250".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_METRICS_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS".to_string(),
        "500".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_AUDIT_LOG_SAMPLE_RATE_BASIS_POINTS".to_string(),
        "750".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_SECURITY_HEADERS_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_ALLOWED_HOSTS".to_string(),
        "gw.example.com,api.example.com".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_RATE_LIMIT_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_RETRY_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_RETRY_MAX_ATTEMPTS".to_string(),
        "2".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_ENABLED".to_string(),
        "false".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_FAILURE_THRESHOLD".to_string(),
        "7".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_OPEN_SECONDS".to_string(),
        "15".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_CIRCUIT_BREAKER_HALF_OPEN_MAX_ATTEMPTS".to_string(),
        "3".to_string(),
    );

    let config = config
        .with_env_overrides(&env)
        .expect("env overrides apply");

    assert_eq!(config.server.bind, "127.0.0.1:4999");
    assert_eq!(
        config
            .upstream("sdkwork-iam-app-api")
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
            .upstream("sdkwork-iam-app-api")
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
        "configs/sdkwork-api-cloud-gateway.development.toml.example",
        "configs/sdkwork-api-cloud-gateway.test.toml.example",
        "configs/sdkwork-api-cloud-gateway.production.toml.example",
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
        matchServiceId = "sdkwork-iam-app-api"
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
            match_service_id: Some("sdkwork-iam-app-api".to_string()),
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
        "SDKWORK_API_CLOUD_GATEWAY_POLICY_ENABLED".to_string(),
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
        "SDKWORK_API_CLOUD_GATEWAY_POLICY_DRY_RUN".to_string(),
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
        "SDKWORK_API_CLOUD_GATEWAY_DIAGNOSTICS_ENABLED".to_string(),
        "true".to_string(),
    );
    env.insert(
        "SDKWORK_API_CLOUD_GATEWAY_DIAGNOSTICS_PATH".to_string(),
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
        "SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS".to_string(),
        "10001".to_string(),
    );

    let error = GatewayRuntimeConfig::default()
        .with_env_overrides(&env)
        .expect_err("invalid env sample rate should be rejected")
        .to_string();

    assert!(error.contains("SDKWORK_API_CLOUD_GATEWAY_ACCESS_LOG_SAMPLE_RATE_BASIS_POINTS"));
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
              "serviceId": "sdkwork-iam-app-api",
              "workspace": "sdkwork-appbase",
              "sdkFamily": "sdkwork-iam-app-sdk",
              "apiAuthority": "sdkwork-iam-app-api",
              "surface": "app",
              "apiPrefix": "/app/v3/api",
              "runtimeMode": "split-or-embedded",
              "requiredBaseUrlKey": "SDKWORK_APPBASE_APP_API_BASE_URL",
              "cargoFeature": "foundation-appbase",
              "cargoDependency": "sdkwork_routes_iam_app_api",
              "executableExport": "sdkwork_routes_iam_app_api::build_sdkwork_iam_app_api_router"
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
        Some("sdkwork_routes_iam_app_api")
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
    assert_eq!(notary.runtime_mode, DependencyRuntimeMode::SplitOrEmbedded);
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
fn application_manifest_declares_collapsed_embedded_assembly_metadata() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    for (service_id, workspace, cargo_feature, cargo_dependency, executable_export) in [
        (
            "sdkwork-agent-app-api",
            "sdkwork-kernel",
            "foundation-kernel",
            "sdkwork-agents-gateway-assembly",
            "sdkwork_agents_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-agent-backend-api",
            "sdkwork-kernel",
            "foundation-kernel",
            "sdkwork-agents-gateway-assembly",
            "sdkwork_agents_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-agent-open-api",
            "sdkwork-kernel",
            "foundation-kernel",
            "sdkwork-agents-gateway-assembly",
            "sdkwork_agents_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-deploy-app-api",
            "sdkwork-deployments",
            "foundation-deployments",
            "sdkwork-deployments-gateway-assembly",
            "sdkwork_deployments_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-deploy-backend-api",
            "sdkwork-deployments",
            "foundation-deployments",
            "sdkwork-deployments-gateway-assembly",
            "sdkwork_deployments_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-dezhou-app-api",
            "sdkwork-dezhou",
            "foundation-dezhou",
            "sdkwork-dezhou-gateway-assembly",
            "sdkwork_dezhou_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-dezhou-backend-api",
            "sdkwork-dezhou",
            "foundation-dezhou",
            "sdkwork-dezhou-gateway-assembly",
            "sdkwork_dezhou_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-documents-app-api",
            "sdkwork-documents",
            "foundation-documents",
            "sdkwork-documents-gateway-assembly",
            "sdkwork_documents_gateway_assembly::assemble_application_business_router_from_env",
        ),
        (
            "sdkwork-documents-backend-api",
            "sdkwork-documents",
            "foundation-documents",
            "sdkwork-documents-gateway-assembly",
            "sdkwork_documents_gateway_assembly::assemble_application_business_router_from_env",
        ),
        (
            "sdkwork-documents-open-api",
            "sdkwork-documents",
            "foundation-documents",
            "sdkwork-documents-gateway-assembly",
            "sdkwork_documents_gateway_assembly::assemble_application_business_router_from_env",
        ),
        (
            "sdkwork-modelkit-app-api",
            "sdkwork-modelkit",
            "foundation-modelkit",
            "sdkwork-modelkit-gateway-assembly",
            "sdkwork_modelkit_gateway_assembly::assemble_application_business_router_from_env",
        ),
        (
            "sdkwork-prompts-app-api",
            "sdkwork-prompts",
            "foundation-prompts",
            "sdkwork-prompts-gateway-assembly",
            "sdkwork_prompts_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-prompts-backend-api",
            "sdkwork-prompts",
            "foundation-prompts",
            "sdkwork-prompts-gateway-assembly",
            "sdkwork_prompts_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-prompts-open-api",
            "sdkwork-prompts",
            "foundation-prompts",
            "sdkwork-prompts-gateway-assembly",
            "sdkwork_prompts_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-web-app-api",
            "sdkwork-web-server",
            "foundation-web-server",
            "sdkwork-web-server-gateway-assembly",
            "sdkwork_web_server_gateway_assembly::assemble_application_business_router",
        ),
        (
            "sdkwork-web-backend-api",
            "sdkwork-web-server",
            "foundation-web-server",
            "sdkwork-web-server-gateway-assembly",
            "sdkwork_web_server_gateway_assembly::assemble_application_business_router",
        ),
    ] {
        let surface = config
            .dependency_surfaces
            .iter()
            .find(|surface| surface.service_id == service_id)
            .unwrap_or_else(|| panic!("manifest declares {service_id} surface"));

        assert_eq!(surface.workspace, workspace);
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::SplitOrEmbedded);
        assert_eq!(surface.cargo_feature.as_deref(), Some(cargo_feature));
        assert_eq!(surface.cargo_dependency.as_deref(), Some(cargo_dependency));
        assert_eq!(
            surface.executable_export.as_deref(),
            Some(executable_export)
        );
    }
}

#[test]
fn application_manifest_declares_claw_router_foundation_split_dependency_surfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    let (service_id, workspace, sdk_family, api_prefix, env_key, default_base_url) = (
        "sdkwork-iam-backend-api",
        "sdkwork-appbase",
        "sdkwork-iam-backend-sdk",
        "/backend/v3/api/iam",
        "SDKWORK_APPBASE_BACKEND_API_BASE_URL",
        "http://127.0.0.1:3801",
    );
    let surface = config
        .dependency_surfaces
        .iter()
        .find(|surface| surface.service_id == service_id)
        .unwrap_or_else(|| panic!("manifest declares {service_id} surface"));
    assert_eq!(surface.workspace, workspace);
    assert_eq!(surface.sdk_family, sdk_family);
    assert_eq!(surface.api_authority, service_id);
    assert_eq!(surface.api_prefix, api_prefix);
    assert_eq!(surface.runtime_mode, DependencyRuntimeMode::SplitOrEmbedded);
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

#[test]
fn application_manifest_declares_sdkwork_im_foundation_split_dependency_surfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    for (service_id, workspace, sdk_family, surface_kind, api_prefix, env_key, default_base_url) in [
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
        assert_eq!(surface.runtime_mode, DependencyRuntimeMode::SplitOrEmbedded);
        assert_eq!(surface.required_base_url_key.as_deref(), Some(env_key));

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
        let expected_runtime_mode = if matches!(
            workspace,
            "sdkwork-drive"
                | "sdkwork-memory"
                | "sdkwork-knowledgebase"
                | "sdkwork-notes"
                | "sdkwork-generations"
        ) {
            DependencyRuntimeMode::Split
        } else {
            DependencyRuntimeMode::SplitOrEmbedded
        };
        assert_eq!(surface.runtime_mode, expected_runtime_mode);
        assert_eq!(surface.required_base_url_key.as_deref(), Some(env_key));

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
        let expected_runtime_mode = if workspace == "sdkwork-search" {
            DependencyRuntimeMode::Split
        } else {
            DependencyRuntimeMode::SplitOrEmbedded
        };
        assert_eq!(surface.runtime_mode, expected_runtime_mode);
        assert_eq!(surface.required_base_url_key.as_deref(), Some(env_key));

        assert_eq!(
            config
                .upstream(service_id)
                .unwrap_or_else(|| panic!("{service_id} upstream is derived"))
                .base_url,
            default_base_url
        );
    }

    for service_id in ["sdkwork-course-app-api", "sdkwork-course-backend-api"] {
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
fn application_manifest_declares_extended_platform_dependency_upstreams() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config = GatewayRuntimeConfig::from_application_root(root).expect("manifest parses");

    for (service_id, env_key, default_base_url) in [
        (
            "sdkwork-cms-app-api",
            "SDKWORK_CMS_APP_API_BASE_URL",
            "http://127.0.0.1:19882",
        ),
        (
            "sdkwork-cms-backend-api",
            "SDKWORK_CMS_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19883",
        ),
        (
            "sdkwork-cms-open-api",
            "SDKWORK_CMS_OPEN_API_BASE_URL",
            "http://127.0.0.1:19884",
        ),
        (
            "sdkwork-forum-app-api",
            "SDKWORK_FORUM_APP_API_BASE_URL",
            "http://127.0.0.1:19885",
        ),
        (
            "sdkwork-forum-backend-api",
            "SDKWORK_FORUM_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19886",
        ),
        (
            "sdkwork-forum-open-api",
            "SDKWORK_FORUM_OPEN_API_BASE_URL",
            "http://127.0.0.1:19887",
        ),
        (
            "sdkwork-im-im-http",
            "SDKWORK_IM_IM_HTTP_BASE_URL",
            "http://127.0.0.1:19880",
        ),
        (
            "sdkwork-im-backend-http",
            "SDKWORK_IM_BACKEND_HTTP_BASE_URL",
            "http://127.0.0.1:19881",
        ),
        (
            "sdkwork-claw-gateway",
            "SDKWORK_CLAW_GATEWAY_BASE_URL",
            "http://127.0.0.1:18190",
        ),
        (
            "sdkwork-appstore-app-api",
            "SDKWORK_APPSTORE_APP_API_BASE_URL",
            "http://127.0.0.1:19980",
        ),
        (
            "sdkwork-appstore-backend-api",
            "SDKWORK_APPSTORE_BACKEND_API_BASE_URL",
            "http://127.0.0.1:19981",
        ),
        (
            "sdkwork-appstore-open-api",
            "SDKWORK_APPSTORE_OPEN_API_BASE_URL",
            "http://127.0.0.1:19982",
        ),
    ] {
        assert!(
            config
                .dependency_surfaces
                .iter()
                .any(|surface| surface.service_id == service_id),
            "manifest declares {service_id} dependency surface"
        );
        assert_eq!(
            config
                .upstream(service_id)
                .unwrap_or_else(|| panic!("{service_id} upstream is derived"))
                .base_url,
            default_base_url
        );
        assert!(
            config
                .dependency_surfaces
                .iter()
                .filter(|surface| surface.service_id == service_id)
                .all(|surface| surface.required_base_url_key.as_deref() == Some(env_key)),
            "{service_id} surfaces share env key {env_key}"
        );
    }
}

#[test]
fn application_manifest_declares_cms_and_forum_env_bindings() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = root.join("sdkwork.app.config.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap())
            .expect("manifest parses");
    let variables = manifest["envBindings"]["variables"]
        .as_array()
        .expect("application manifest declares envBindings.variables");
    for env_key in [
        "SDKWORK_CMS_APP_API_BASE_URL",
        "SDKWORK_CMS_BACKEND_API_BASE_URL",
        "SDKWORK_CMS_OPEN_API_BASE_URL",
        "SDKWORK_FORUM_APP_API_BASE_URL",
        "SDKWORK_FORUM_BACKEND_API_BASE_URL",
        "SDKWORK_FORUM_OPEN_API_BASE_URL",
        "SDKWORK_IM_IM_HTTP_BASE_URL",
        "SDKWORK_IM_BACKEND_HTTP_BASE_URL",
    ] {
        assert!(
            variables
                .iter()
                .any(|entry| entry["name"].as_str() == Some(env_key)),
            "application manifest must declare env binding {env_key}"
        );
    }
}

#[test]
fn legacy_im_upstream_env_aliases_apply_when_canonical_keys_are_missing() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut env = HashMap::new();
    env.insert(
        "CRAW_CHAT_IM_HTTP_BASE_URL".to_string(),
        "http://127.0.0.1:29980".to_string(),
    );
    env.insert(
        "CRAW_CHAT_BACKEND_HTTP_BASE_URL".to_string(),
        "http://127.0.0.1:29981".to_string(),
    );

    let config = GatewayRuntimeConfig::from_application_root(root)
        .expect("manifest parses")
        .with_env_overrides(&env)
        .expect("legacy IM env overrides apply");

    assert_eq!(
        config
            .upstream(SDKWORK_IM_IM_HTTP_SERVICE_ID)
            .expect("im http upstream")
            .base_url,
        "http://127.0.0.1:29980"
    );
    assert_eq!(
        config
            .upstream(SDKWORK_IM_BACKEND_HTTP_SERVICE_ID)
            .expect("im backend upstream")
            .base_url,
        "http://127.0.0.1:29981"
    );
}

#[test]
fn application_manifest_env_bindings_cover_all_dependency_base_url_keys() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = root.join("sdkwork.app.config.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap())
            .expect("manifest parses");
    let env_names: std::collections::HashSet<String> = manifest["envBindings"]["variables"]
        .as_array()
        .expect("application manifest declares envBindings.variables")
        .iter()
        .filter_map(|entry| entry["name"].as_str().map(str::to_string))
        .collect();

    for dependency in manifest["sdkDependencies"]
        .as_array()
        .expect("application manifest declares sdkDependencies")
    {
        let Some(env_key) = dependency["requiredBaseUrlKey"].as_str() else {
            continue;
        };
        let service_id = dependency["serviceId"]
            .as_str()
            .unwrap_or("unknown-service-id");
        assert!(
            env_names.contains(env_key),
            "application manifest envBindings must declare requiredBaseUrlKey {env_key} for service {service_id}"
        );
    }
}

#[test]
fn component_spec_dependencies_align_with_application_manifest_base_url_keys() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("sdkwork.app.config.json")).expect("manifest reads"),
    )
    .expect("manifest parses");
    let spec: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("specs/component.spec.json")).expect("component spec reads"),
    )
    .expect("component spec parses");

    let mut manifest_keys = HashMap::new();
    for dependency in manifest["sdkDependencies"]
        .as_array()
        .expect("application manifest declares sdkDependencies")
    {
        let service_id = dependency["serviceId"]
            .as_str()
            .expect("sdkDependencies entry declares serviceId")
            .to_string();
        let env_key = dependency["requiredBaseUrlKey"]
            .as_str()
            .expect("sdkDependencies entry declares requiredBaseUrlKey")
            .to_string();
        if let Some(existing) = manifest_keys.insert(service_id.clone(), env_key.clone()) {
            assert_eq!(
                existing, env_key,
                "manifest sdkDependencies must use one requiredBaseUrlKey for {service_id}"
            );
        }
    }

    for dependency in spec["dependencies"]
        .as_array()
        .expect("component spec declares dependencies")
    {
        let service_id = dependency["serviceId"]
            .as_str()
            .expect("component spec dependency declares serviceId");
        let env_key = dependency["requiredBaseUrlKey"]
            .as_str()
            .expect("component spec dependency declares requiredBaseUrlKey");
        assert_eq!(
            manifest_keys.get(service_id).map(String::as_str),
            Some(env_key),
            "component spec and manifest must agree on requiredBaseUrlKey for {service_id}"
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
    assert_eq!(
        spec["kind"].as_str(),
        Some("sdkwork.component.spec"),
        "component spec must declare the SDKWork component spec kind"
    );
    assert_eq!(
        spec["component"]["name"].as_str(),
        Some("sdkwork-api-cloud-gateway"),
        "component spec must declare standard component.name"
    );
    assert_eq!(
        spec["component"]["root"].as_str(),
        Some("."),
        "component spec must declare repository-root component.root"
    );
    assert_eq!(
        spec["component"]["domain"].as_str(),
        Some("platform"),
        "component spec must declare canonical platform domain"
    );
    assert_eq!(
        spec["component"]["capability"].as_str(),
        Some("api-gateway"),
        "component spec must declare api-gateway capability"
    );
    let languages = spec["component"]["languages"]
        .as_array()
        .expect("component spec declares component.languages");
    for language in ["rust", "javascript", "node"] {
        assert!(
            languages
                .iter()
                .any(|candidate| candidate.as_str() == Some(language)),
            "component spec must declare authored language {language}"
        );
    }
    let manifests = spec["component"]["manifests"]
        .as_array()
        .expect("component spec declares component.manifests");
    assert!(
        manifests
            .iter()
            .any(|entry| entry.as_str() == Some("Cargo.lock")),
        "component manifests must list Cargo.lock for deployable Rust backend"
    );
    assert!(
        root.join("Cargo.lock").is_file(),
        "Cargo.lock must exist at repository root per DEPENDENCY_MANAGEMENT_SPEC"
    );
    let canonical_specs = spec["canonicalSpecs"]
        .as_array()
        .expect("component spec declares canonicalSpecs");
    for required_spec in [
        "CODE_STYLE_SPEC.md",
        "NAMING_SPEC.md",
        "RUST_CODE_SPEC.md",
        "TYPESCRIPT_CODE_SPEC.md",
        "COMPONENT_SPEC.md",
        "GITHUB_WORKFLOW_SPEC.md",
        "WEB_FRAMEWORK_SPEC.md",
        "SECURITY_SPEC.md",
        "OBSERVABILITY_SPEC.md",
        "APP_MANIFEST_SPEC.md",
        "ENVIRONMENT_SPEC.md",
    ] {
        assert!(
            canonical_specs
                .iter()
                .any(|entry| entry["file"].as_str() == Some(required_spec)
                    && entry["path"]
                        .as_str()
                        .is_some_and(|path| path.ends_with(required_spec))),
            "component spec must link canonical spec {required_spec}"
        );
    }
    assert!(
        spec["contracts"]["publicExports"]
            .as_array()
            .expect("contracts.publicExports is declared")
            .iter()
            .any(|export| export.as_str() == Some("sdkwork-api-cloud-gateway")),
        "component spec must declare public crate export"
    );
    assert!(
        spec["contracts"]["dependencyApiExports"]
            .as_array()
            .is_some_and(|exports| exports.is_empty()),
        "gateway must explicitly default to no dependency API re-export"
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
    for required_command in [
        "pnpm check",
        "pnpm verify",
        "pnpm workflow:validate",
        "pnpm gateway:matrix:standalone",
        "pnpm gateway:package:standalone",
        "pnpm gateway:validate:standalone",
    ] {
        assert!(
            verification_commands.iter().any(|command| command
                .as_str()
                .is_some_and(|value| value == required_command)),
            "component spec must declare release verification command {required_command}"
        );
    }
    assert!(
        verification_commands
            .iter()
            .any(|command| command.as_str() == Some("cargo fmt -- --check")),
        "component spec must verify rustfmt for gateway workspace members"
    );
    assert!(
        !verification_commands.iter().any(|command| command
            .as_str()
            .is_some_and(|value| value == "cargo fmt --all -- --check")),
        "component spec must not use cargo fmt --all; optional foundation path deps resolve sibling workspaces"
    );

    let runtime_exports = spec["entrypoints"]
        .as_array()
        .expect("component spec declares entrypoints")
        .iter()
        .find(|entrypoint| {
            entrypoint["kind"].as_str() == Some("library")
                && entrypoint["path"].as_str() == Some("crates/sdkwork-api-cloud-gateway")
        })
        .and_then(|entrypoint| entrypoint["exports"].as_array())
        .expect("component spec declares gateway library exports");
    assert!(
        runtime_exports
            .iter()
            .any(|export| export.as_str() == Some("build_sdkwork_api_cloud_gateway_router")),
        "component spec must declare the canonical standalone-gateway router builder"
    );
    for required_export in [
        "web_framework_enabled",
        "parse_web_framework_toggle",
        "finalize_gateway_router",
        "finalize_gateway_router_async",
        "split_proxy_call_chain",
    ] {
        assert!(
            runtime_exports
                .iter()
                .any(|export| export.as_str() == Some(required_export)),
            "component spec must declare web-framework export {required_export}"
        );
    }
    assert!(
        runtime_exports
            .iter()
            .all(|export| export.as_str() != Some("build_sdkwork_api_cloud_gateway_service_router")),
        "component spec must not preserve the legacy service router alias"
    );
    assert_eq!(
        spec["runtimeBehavior"]["generatedRequestIdFormat"].as_str(),
        Some("uuid-v4"),
        "component spec must declare framework UUID request identity"
    );
    assert_eq!(
        spec["runtimeBehavior"]["webFramework"]["toggleEnv"].as_str(),
        Some("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED"),
        "component spec must declare web-framework toggle env"
    );
    assert_eq!(
        spec["runtimeBehavior"]["webFramework"]["splitMode"]["interceptorCount"].as_u64(),
        Some(19),
        "component spec must document split-mode framework interceptor count"
    );
    assert_eq!(
        spec["runtimeBehavior"]["security"]["secureHeaders"]["splitModeBaselineAuthority"].as_str(),
        Some("sdkwork-web-framework HeaderSecurity stage"),
        "component spec must declare framework ownership of baseline secure headers in split mode"
    );
    assert_eq!(
        spec["runtimeBehavior"]["webFramework"]["disabledMode"]["corsAuthority"].as_str(),
        Some("tower-http CorsLayer"),
        "component spec must document legacy cors authority when web framework is disabled"
    );
    assert_eq!(
        spec["runtimeBehavior"]["webFramework"]["disabledMode"]["secureHeadersAuthority"]
            .as_str(),
        Some("gateway security middleware"),
        "component spec must document gateway secure header authority when web framework is disabled"
    );
    let web_framework = spec["gatewayIntegration"]["webFramework"]
        .as_object()
        .expect("gatewayIntegration.webFramework is declared");
    for dependency in [
        "sdkwork-web-contract",
        "sdkwork-web-core",
        "sdkwork-web-axum",
    ] {
        assert!(
            web_framework["cargoDependencies"]
                .as_array()
                .expect("webFramework cargoDependencies")
                .iter()
                .any(|entry| entry.as_str() == Some(dependency)),
            "gatewayIntegration.webFramework must declare {dependency}"
        );
    }
    let shared_utilities = spec["gatewayIntegration"]["sharedUtilities"]
        .as_object()
        .expect("gatewayIntegration.sharedUtilities is declared");
    assert!(
        shared_utilities["cargoDependencies"]
            .as_array()
            .expect("sharedUtilities cargoDependencies")
            .iter()
            .any(|entry| entry.as_str() == Some("sdkwork-utils-rust")),
        "gatewayIntegration.sharedUtilities must declare sdkwork-utils-rust"
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
                .find(|package| package["name"].as_str() == Some("sdkwork-api-cloud-gateway"))
        })
        .expect("cargo metadata includes sdkwork-api-cloud-gateway");

    let runtime_dependencies = runtime_package["dependencies"]
        .as_array()
        .expect("runtime package declares dependencies");
    for web_dependency in [
        "sdkwork-web-contract",
        "sdkwork-web-core",
        "sdkwork-web-axum",
    ] {
        assert!(
            runtime_dependencies
                .iter()
                .any(|dependency| dependency["name"].as_str() == Some(web_dependency)),
            "runtime package must depend on {web_dependency}"
        );
    }
    assert!(
        runtime_dependencies
            .iter()
            .any(|dependency| dependency["name"].as_str() == Some("sdkwork-utils-rust")),
        "runtime package must depend on sdkwork-utils-rust for shared utility helpers"
    );

    let dependencies = spec["dependencies"]
        .as_array()
        .expect("component spec declares dependencies");
    let mut seen_service_ids = std::collections::HashSet::new();
    for dependency in dependencies {
        let service_id = dependency["serviceId"]
            .as_str()
            .expect("dependency declares serviceId");
        assert!(
            seen_service_ids.insert(service_id.to_owned()),
            "component spec dependencies must not duplicate serviceId {service_id}"
        );
    }
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
            .find(|dependency| {
                dependency["name"].as_str() == Some(cargo_dependency)
                    || dependency["rename"].as_str() == Some(cargo_dependency)
            })
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
fn sdkwork_workspace_dependencies_are_referenced_by_member_manifests() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace_toml_path = root.join("Cargo.toml");
    let workspace_toml = fs::read_to_string(&workspace_toml_path).unwrap_or_else(|error| {
        panic!(
            "failed to read workspace manifest {}: {error}",
            workspace_toml_path.display()
        )
    });
    let workspace: toml::Value = toml::from_str(&workspace_toml).expect("workspace TOML parses");

    let workspace_dependencies = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(toml::Value::as_table)
        .expect("workspace dependencies are declared");
    let members = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .expect("workspace members are declared");

    let mut member_dependency_tables = Vec::new();
    for member in members {
        let member = member.as_str().expect("workspace member is a string");
        let member_toml_path = root.join(member).join("Cargo.toml");
        let member_toml = fs::read_to_string(&member_toml_path).unwrap_or_else(|error| {
            panic!(
                "failed to read member manifest {}: {error}",
                member_toml_path.display()
            )
        });
        let member_manifest: toml::Value =
            toml::from_str(&member_toml).expect("member manifest TOML parses");
        member_dependency_tables.push(member_manifest);
    }

    for (dependency_name, dependency_config) in workspace_dependencies {
        let is_sdkwork_sibling = dependency_config
            .get("path")
            .and_then(toml::Value::as_str)
            .is_some_and(|path| path.starts_with("../sdkwork-"));
        if !is_sdkwork_sibling {
            continue;
        }

        let referenced = member_dependency_tables.iter().any(|manifest| {
            ["dependencies", "dev-dependencies", "build-dependencies"]
                .iter()
                .filter_map(|table| manifest.get(*table).and_then(toml::Value::as_table))
                .any(|dependencies| {
                    dependencies
                        .get(dependency_name)
                        .and_then(toml::Value::as_table)
                        .and_then(|dependency| dependency.get("workspace"))
                        .and_then(toml::Value::as_bool)
                        == Some(true)
                })
        });

        assert!(
            referenced,
            "SDKWork workspace dependency {dependency_name} must be referenced by a member manifest or removed"
        );
    }
}

#[test]
fn gateway_runtime_source_is_split_by_endpoint_responsibility() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let gateway_src = root.join("crates/sdkwork-api-cloud-gateway/src");

    for module in [
        "health.rs",
        "openapi.rs",
        "diagnostics.rs",
        "web_framework.rs",
    ] {
        assert!(
            gateway_src.join(module).is_file(),
            "gateway endpoint responsibility must live in {module}"
        );
    }

    let gateway_tests = root.join("crates/sdkwork-api-cloud-gateway/tests");
    assert!(
        gateway_tests.join("web_framework_tests.rs").is_file(),
        "gateway must keep dedicated web-framework integration tests"
    );

    let lib_source = fs::read_to_string(gateway_src.join("lib.rs")).expect("lib.rs readable");
    assert!(
        lib_source.contains("pub mod web_framework"),
        "gateway library must declare web_framework module"
    );

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

#[test]
fn superpowers_docs_are_marked_as_historical_execution_records() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme_path = root.join("docs/superpowers/README.md");
    let readme = fs::read_to_string(&readme_path).unwrap_or_else(|error| {
        panic!(
            "failed to read superpowers docs README {}: {error}",
            readme_path.display()
        )
    });

    for required_phrase in [
        "historical execution records",
        "not the current source of truth",
        "../../README.md",
        "../../AGENTS.md",
        "../../specs/component.spec.json",
        "../../../sdkwork-specs/README.md",
    ] {
        assert!(
            readme.contains(required_phrase),
            "superpowers docs README must clarify {required_phrase}"
        );
    }
}

#[test]
fn local_specs_readme_documents_component_contract_boundary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme_path = root.join("specs/README.md");
    let readme = fs::read_to_string(&readme_path).unwrap_or_else(|error| {
        panic!(
            "failed to read local specs README {}: {error}",
            readme_path.display()
        )
    });

    for required_phrase in [
        "Purpose:",
        "Owner:",
        "Allowed content:",
        "Forbidden content:",
        "Related specs:",
        "Verification:",
        "specs/component.spec.json",
        "machine-readable component contract",
        "../sdkwork-specs/COMPONENT_SPEC.md",
        "../sdkwork-specs/DOCUMENTATION_SPEC.md",
        "../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md",
        "Do not copy root SDKWork standards",
        "Do not add a separate gateway catalog",
    ] {
        assert!(
            readme.contains(required_phrase),
            "specs/README.md must document local specs boundary phrase {required_phrase}"
        );
    }
}

#[test]
fn sdkwork_workspace_readmes_follow_repository_workspace_standard() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    for relative_path in [
        ".sdkwork/README.md",
        ".sdkwork/skills/README.md",
        ".sdkwork/plugins/README.md",
    ] {
        let readme_path = root.join(relative_path);
        let readme = fs::read_to_string(&readme_path).unwrap_or_else(|error| {
            panic!(
                "failed to read SDKWork workspace README {}: {error}",
                readme_path.display()
            )
        });

        for required_phrase in [
            "Purpose:",
            "Owner:",
            "Allowed content:",
            "Forbidden content:",
            "Related specs:",
            "Verification:",
            "SDKWORK_WORKSPACE_SPEC.md",
        ] {
            assert!(
                readme.contains(required_phrase),
                "{relative_path} must document SDKWork workspace phrase {required_phrase}"
            );
        }
    }

    let workspace_readme_path = root.join(".sdkwork/README.md");
    let workspace_readme = fs::read_to_string(&workspace_readme_path).unwrap_or_else(|error| {
        panic!(
            "failed to read SDKWork workspace README {}: {error}",
            workspace_readme_path.display()
        )
    });
    for required_phrase in [
        "AGENTS.md",
        "../sdkwork-specs/SOUL.md",
        "source-controlled SDKWork workspace metadata",
        "not runtime state",
    ] {
        assert!(
            workspace_readme.contains(required_phrase),
            ".sdkwork/README.md must document workspace boundary phrase {required_phrase}"
        );
    }

    let gitignore_path = root.join(".sdkwork/.gitignore");
    let gitignore = fs::read_to_string(&gitignore_path).unwrap_or_else(|error| {
        panic!(
            "failed to read SDKWork workspace gitignore {}: {error}",
            gitignore_path.display()
        )
    });
    for ignored_path in ["local/", "tmp/", "cache/", "secrets/", "state/", "*.local"] {
        assert!(
            gitignore.contains(ignored_path),
            ".sdkwork/.gitignore must ignore {ignored_path}"
        );
    }
}

#[test]
fn root_readme_documents_standard_workspace_layout() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let readme_path = root.join("README.md");
    let readme = fs::read_to_string(&readme_path).unwrap_or_else(|error| {
        panic!(
            "failed to read root README {}: {error}",
            readme_path.display()
        )
    });

    for required_phrase in [
        "## Root Layout",
        "../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md",
        "full SDKWork project-root directory dictionary",
        "`apis/`",
        "`sdks/`",
        "authored API contract inputs",
        "SDK family workspace",
        "`.sdkwork/plugins/`",
        "Package-local Rust unit and integration tests remain under each crate's `tests/` directory",
    ] {
        assert!(
            readme.contains(required_phrase),
            "root README must document workspace layout phrase {required_phrase}"
        );
    }
}

#[test]
fn agent_entrypoints_follow_sdkwork_standard() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let agents_path = root.join("AGENTS.md");
    let agents = fs::read_to_string(&agents_path).unwrap_or_else(|error| {
        panic!(
            "failed to read agent guide {}: {error}",
            agents_path.display()
        )
    });

    assert!(
        agents.starts_with("# Repository Guidelines"),
        "AGENTS.md must use the SDKWork standard title"
    );
    for required_section in [
        "## SDKWORK Soul",
        "## SDKWORK Standards",
        "## Application Identity",
        "## Local Dictionary Structure",
        "## Spec Resolution Order",
        "## Required Specs By Task Type",
        "## Code Style Rules",
        "## Build, Test, and Verification",
        "## Agent Execution Rules",
        "## Human Review Rules",
    ] {
        assert!(
            agents.contains(required_section),
            "AGENTS.md must include standard section {required_section}"
        );
    }
    for required_reference in [
        "../sdkwork-specs/README.md",
        "../sdkwork-specs/SOUL.md",
        "../sdkwork-specs/AGENTS_SPEC.md",
        "../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md",
    ] {
        assert!(
            agents.contains(required_reference),
            "AGENTS.md must cite {required_reference}"
        );
    }
    assert!(
        agents.contains("cargo fmt -- --check"),
        "AGENTS.md must document workspace-scoped rustfmt verification"
    );
    assert!(
        !agents.contains("cargo fmt --all -- --check"),
        "AGENTS.md must not document cargo fmt --all for this gateway workspace"
    );

    for shim in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let shim_path = root.join(shim);
        let content = fs::read_to_string(&shim_path)
            .unwrap_or_else(|error| panic!("failed to read shim {}: {error}", shim_path.display()));
        assert!(
            content.contains("AGENTS.md"),
            "{shim} must point to same-root AGENTS.md"
        );
        assert!(
            content.contains("../sdkwork-specs/README.md"),
            "{shim} must cite the relative SDKWork standards entrypoint"
        );
        assert!(
            content.contains("../sdkwork-specs/SOUL.md"),
            "{shim} must cite the SDKWork soul path"
        );
        assert!(
            content.contains("../sdkwork-specs/AGENTS_SPEC.md"),
            "{shim} must cite the agent standard path"
        );
        assert!(
            content.contains("Do not duplicate or override SDKWork rules here."),
            "{shim} must stay a compatibility shim instead of a second rule body"
        );
    }
}

#[test]
fn gateway_owned_api_contract_is_authored_under_apis_directory() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let contract_path = root.join("apis/gateway-api/platform/openapi.yaml");
    let contract = fs::read_to_string(&contract_path).unwrap_or_else(|error| {
        panic!(
            "gateway-owned API contract must exist at {}: {error}",
            contract_path.display()
        )
    });
    let document: serde_json::Value =
        serde_json::from_str(&contract).expect("gateway-api openapi contract parses as JSON");
    for operation_id in [
        "getGatewayHealth",
        "getGatewayReadiness",
        "listGatewayApiDependencies",
        "getGatewayOpenApiDocument",
    ] {
        let paths = document["paths"]
            .as_object()
            .expect("gateway-api contract declares paths");
        assert!(
            paths.values().any(|path_item| {
                path_item
                    .as_object()
                    .and_then(|methods| methods.get("get"))
                    .and_then(|operation| operation["operationId"].as_str())
                    == Some(operation_id)
            }),
            "gateway-api contract must declare operationId {operation_id}"
        );
    }
}

#[test]
fn package_dev_entrypoint_is_declared_in_component_spec() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let package_json_path = root.join("package.json");
    let package_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&package_json_path).unwrap_or_else(|error| {
            panic!(
                "failed to read package manifest {}: {error}",
                package_json_path.display()
            )
        }),
    )
    .expect("package.json parses");
    assert_eq!(
        package_json["scripts"]["dev"].as_str(),
        Some("node scripts/dev/run-sdkwork-api-cloud-gateway-dev.mjs"),
        "package.json must keep pnpm dev as the standard independent startup entrypoint"
    );
    let dev_runner_path = root.join("scripts/dev/run-sdkwork-api-cloud-gateway-dev.mjs");
    let dev_runner = fs::read_to_string(&dev_runner_path).unwrap_or_else(|error| {
        panic!(
            "failed to read dev runner {}: {error}",
            dev_runner_path.display()
        )
    });
    assert!(
        dev_runner.contains(
            "SDKWORK_WEB_SNOWFLAKE_NODE_ID: process.env.SDKWORK_WEB_SNOWFLAKE_NODE_ID ?? '0'"
        ),
        "standalone development must supply the web-server Snowflake node id while preserving an explicit override"
    );
    assert!(
        dev_runner.contains("process.env.SDKWORK_AGENT_RUNTIME_DATABASE_ENGINE ?? 'postgres'"),
        "standalone development must keep agents kernel runtime persistence on the initialized PostgreSQL profile"
    );
    assert!(
        dev_runner.contains("SDKWORK_DATABASE_SCHEMA_FALLBACK_PUBLIC: 'false'"),
        "standalone development must disable public-schema fallback for application-owned tables"
    );
    assert!(
        dev_runner.contains("PGOPTIONS: `-c search_path=${profile.config.database.schema}`"),
        "standalone development must pin every embedded PostgreSQL connection to the initialized application schema"
    );
    for service in [
        "IAM",
        "AGENTS",
        "AGENT_RUNTIME",
        "DEPLOY",
        "DEZHOU",
        "DOCUMENTS",
        "MODELKIT",
        "PROMPTS",
        "WEB",
    ] {
        assert!(
            dev_runner.contains(&format!("  '{service}',")),
            "standalone development must declare the {service} database service override"
        );
    }
    assert!(
        dev_runner.contains("childEnv[`SDKWORK_${service}_DATABASE_URL`]"),
        "standalone development must supply service-scoped database URLs"
    );
    assert!(
        dev_runner.contains("childEnv[`SDKWORK_${service}_DATABASE_SCHEMA`]"),
        "standalone development must supply service-scoped database schemas"
    );

    let spec_path = root.join("specs/component.spec.json");
    let spec: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&spec_path).unwrap_or_else(|error| {
            panic!(
                "failed to read component spec {}: {error}",
                spec_path.display()
            )
        }))
        .expect("component spec parses");
    let runtime_entrypoints = spec["contracts"]["runtimeEntrypoints"]
        .as_array()
        .expect("component spec declares runtime entrypoints");
    assert!(
        runtime_entrypoints
            .iter()
            .any(|entrypoint| entrypoint.as_str() == Some("package.json#scripts.dev")),
        "component spec must declare package.json#scripts.dev as the pnpm dev startup entrypoint"
    );
    for required_entrypoint in [
        "sdkwork_api_cloud_gateway::web_framework::finalize_gateway_router",
        "sdkwork_api_cloud_gateway::web_framework::finalize_gateway_router_async",
        "sdkwork_api_cloud_gateway::web_framework::split_proxy_call_chain",
        "sdkwork_api_cloud_gateway::web_framework::web_framework_enabled",
        "sdkwork_api_cloud_gateway::web_framework::parse_web_framework_toggle",
    ] {
        assert!(
            runtime_entrypoints
                .iter()
                .any(|entrypoint| entrypoint.as_str() == Some(required_entrypoint)),
            "component spec must declare web-framework runtime entrypoint {required_entrypoint}"
        );
    }
}

#[test]
fn application_manifest_declares_web_framework_toggle_env_binding() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = root.join("sdkwork.app.config.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).unwrap_or_else(|error| {
            panic!(
                "failed to read application manifest {}: {error}",
                manifest_path.display()
            )
        }))
        .expect("application manifest parses");
    let variables = manifest["envBindings"]["variables"]
        .as_array()
        .expect("application manifest declares envBindings.variables");
    assert!(
        variables.iter().any(|entry| {
            entry["name"].as_str() == Some("SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED")
        }),
        "application manifest must declare SDKWORK_API_CLOUD_GATEWAY_WEB_FRAMEWORK_ENABLED env binding"
    );
}

#[test]
fn development_template_defaults_to_embedded_single_port_mode() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join("configs/sdkwork-api-cloud-gateway.development.toml.example");
    let config = GatewayRuntimeConfig::from_path(&path)
        .unwrap_or_else(|error| panic!("development config should parse: {error}"));

    assert_eq!(config.mode, GatewayMode::Embedded);
    assert!(
        config.upstreams.is_empty(),
        "default development config should not require split upstream ports"
    );
    assert_eq!(config.server.bind, "127.0.0.1:3900");
    assert_eq!(config.dependency_surfaces.len(), 17);
    for service_id in [
        "sdkwork-iam-app-api",
        "sdkwork-agent-app-api",
        "sdkwork-agent-backend-api",
        "sdkwork-agent-open-api",
        "sdkwork-deploy-app-api",
        "sdkwork-deploy-backend-api",
        "sdkwork-dezhou-app-api",
        "sdkwork-dezhou-backend-api",
        "sdkwork-documents-app-api",
        "sdkwork-documents-backend-api",
        "sdkwork-documents-open-api",
        "sdkwork-modelkit-app-api",
        "sdkwork-prompts-app-api",
        "sdkwork-prompts-backend-api",
        "sdkwork-prompts-open-api",
        "sdkwork-web-app-api",
        "sdkwork-web-backend-api",
    ] {
        assert!(
            config
                .dependency_surfaces
                .iter()
                .any(|surface| surface.service_id == service_id),
            "development config declares embedded surface {service_id}"
        );
    }
}

#[test]
fn workspace_crates_have_module_readmes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace_toml_path = root.join("Cargo.toml");
    let workspace_toml = fs::read_to_string(&workspace_toml_path).unwrap_or_else(|error| {
        panic!(
            "failed to read workspace manifest {}: {error}",
            workspace_toml_path.display()
        )
    });
    let workspace: toml::Value = toml::from_str(&workspace_toml).expect("workspace TOML parses");
    let members = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .expect("workspace members are declared");

    for member in members {
        let member = member.as_str().expect("workspace member is a string");
        let readme_path = root.join(member).join("README.md");
        let readme = fs::read_to_string(&readme_path).unwrap_or_else(|error| {
            panic!(
                "workspace crate {member} must have README.md at {}: {error}",
                readme_path.display()
            )
        });

        for required_phrase in [
            "Domain:",
            "Capability:",
            "Package type:",
            "Owner:",
            "Status:",
            "## Public API",
            "## Required SDK Surface",
            "## Configuration",
            "## SaaS/Private/Local Behavior",
            "## Security",
            "## Extension Points",
            "## Verification",
        ] {
            assert!(
                readme.contains(required_phrase),
                "workspace crate {member} README must include {required_phrase}"
            );
        }
    }
}
