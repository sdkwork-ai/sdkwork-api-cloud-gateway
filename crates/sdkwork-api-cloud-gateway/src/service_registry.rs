//! Database-backed HTTP service registration for the cloud gateway process.
//!
//! `DISCOVERY_SPEC.md` makes the registry the HTTP base-URL authority for
//! split/cloud deployments. This module is the gateway's server half:
//!
//! - on startup it registers **this process and port** for every HTTP surface
//!   the gateway composes (its own service name plus each embedded dependency
//!   surface and its short SDK alias);
//! - the upsert is idempotent, so a restart or replica race never duplicates a
//!   row (`uk_discovery_service_instance_identity`);
//! - a heartbeat loop renews every lease well before it expires, re-registering
//!   if a lease was lost;
//! - shutdown deregisters best-effort before the listener drains.
//!
//! Registration and resolution are **fail-open for this process**: a database
//! outage logs a warning and leaves the gateway serving with the authored
//! topology/default base URLs. Losing the registry must never take the gateway
//! down (`RPC_RESILIENCE_SPEC.md` degradation posture).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_discovery_client::{
    HttpInstanceRegistration, HttpRegistry, HttpRegistryConfig, HttpRegistryResolver,
    RegisteredHttpInstance, RegistryIdentity,
};
use sdkwork_utils_rust::service_base_url::{bind_port, install_http_service_registry};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// Registry lease duration for one gateway instance.
const LEASE_TTL_SECONDS: u64 = 30;
/// Heartbeat cadence; comfortably shorter than the lease.
const HEARTBEAT_INTERVAL_SECONDS: u64 = 10;
/// Registry namespace used when the deployment does not declare one.
const DEFAULT_NAMESPACE: &str = "platform";
/// Toggle for the whole registration/resolution feature.
const ENABLED_ENV: &str = "SDKWORK_DISCOVERY_HTTP_REGISTRY_ENABLED";
/// Namespace override.
const NAMESPACE_ENV: &str = "SDKWORK_DISCOVERY_NAMESPACE";
/// Optional dedicated discovery database; defaults to the application database.
const ENV_DISCOVERY_DATABASE_URL: &str = "SDKWORK_DISCOVERY_DATABASE_URL";
/// Authored public origin of this gateway (`https://api.example.com`).
const PUBLIC_HTTP_URL_ENV: &str = "SDKWORK_API_CLOUD_GATEWAY_PUBLIC_HTTP_URL";
/// Generic authored public origin fallback.
const GENERIC_PUBLIC_HTTP_URL_ENV: &str = "SDKWORK_PUBLIC_BASE_URL";
/// Refresh interval override.
const REFRESH_INTERVAL_ENV: &str = "SDKWORK_DISCOVERY_HTTP_REGISTRY_REFRESH_MS";
/// Failure threshold override.
const FAILURE_THRESHOLD_ENV: &str = "SDKWORK_DISCOVERY_HTTP_REGISTRY_FAILURE_THRESHOLD";
/// Failure cooldown override.
const FAILURE_COOLDOWN_ENV: &str = "SDKWORK_DISCOVERY_HTTP_REGISTRY_FAILURE_COOLDOWN_MS";
/// Optional active health-probe path (`/healthz`); unset keeps failure-only detection.
const HEALTH_PROBE_PATH_ENV: &str = "SDKWORK_DISCOVERY_HTTP_REGISTRY_HEALTH_PATH";
/// Active health-probe timeout override.
const HEALTH_PROBE_TIMEOUT_ENV: &str = "SDKWORK_DISCOVERY_HTTP_REGISTRY_HEALTH_TIMEOUT_MS";

struct LeaseState {
    registration: HttpInstanceRegistration,
    lease: RegisteredHttpInstance,
}

/// The live registration of this gateway process.
pub struct ServiceRegistryRuntime {
    registry: HttpRegistry,
    leases: Arc<Mutex<Vec<LeaseState>>>,
    resolver_refresh: JoinHandle<()>,
    registration_heartbeat: JoinHandle<()>,
}

impl ServiceRegistryRuntime {
    /// Registers every composed HTTP surface and installs the registry-backed
    /// base-URL resolver.
    ///
    /// Returns `None` when the feature is disabled, the process has no
    /// PostgreSQL pool, or the registry is unreachable. Every one of those is a
    /// degradation, not a startup failure.
    pub async fn start(
        config: &GatewayRuntimeConfig,
        local_addr: std::net::SocketAddr,
    ) -> Option<Self> {
        if !registry_enabled() {
            info!("HTTP service registry disabled by configuration");
            return None;
        }
        let Some(database_url) = registry_database_url() else {
            warn!(
                "HTTP service registry skipped: neither {ENV_DISCOVERY_DATABASE_URL} nor \
                 SDKWORK_DATABASE_URL is configured"
            );
            return None;
        };

        let identity =
            RegistryIdentity::new(registry_namespace(), config.service.environment.clone());
        let registry = match tokio::time::timeout(
            Duration::from_secs(5),
            HttpRegistry::connect(&database_url, identity, 2),
        )
        .await
        {
            Ok(Ok(registry)) => registry,
            Ok(Err(error)) => {
                warn!(
                    %error,
                    "HTTP service registry database is unavailable; continuing with topology base URLs"
                );
                return None;
            }
            Err(_) => {
                warn!(
                    "HTTP service registry database connection timed out; continuing with topology base URLs"
                );
                return None;
            }
        };
        match tokio::time::timeout(Duration::from_secs(5), registry.ensure_schema()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!(
                    %error,
                    "HTTP service registry table is unavailable; continuing with topology base URLs"
                );
                return None;
            }
            Err(_) => {
                warn!(
                    "HTTP service registry schema check timed out; continuing with topology base URLs"
                );
                return None;
            }
        }

        let endpoint = public_base_url(local_addr.port());
        let instance_id = instance_id(config, local_addr.port());
        let version = env!("CARGO_PKG_VERSION");
        let mut registrations = Vec::new();
        for (service_name, api_prefix) in service_registrations(config) {
            let mut registration = HttpInstanceRegistration::serving(
                service_name,
                instance_id.clone(),
                endpoint.clone(),
                version,
                LEASE_TTL_SECONDS,
            );
            registration.metadata = registration_metadata(config, api_prefix.as_deref());
            registrations.push(registration);
        }

        let mut leases = Vec::new();
        let mut failures = 0_usize;
        for registration in registrations {
            match tokio::time::timeout(Duration::from_secs(2), registry.register(&registration))
                .await
            {
                Ok(Ok(lease)) => {
                    failures = 0;
                    leases.push(LeaseState {
                        registration,
                        lease,
                    });
                }
                Ok(Err(error)) => {
                    failures += 1;
                    warn!(
                        service = %registration.service_name,
                        %error,
                        "HTTP service registry registration failed"
                    );
                    if failures >= 3 {
                        warn!("HTTP service registry registration failed repeatedly; stopping");
                        break;
                    }
                }
                Err(_) => {
                    failures += 1;
                    warn!(
                        service = %registration.service_name,
                        "HTTP service registry registration timed out"
                    );
                    if failures >= 3 {
                        warn!("HTTP service registry registration timed out repeatedly; stopping");
                        break;
                    }
                }
            }
        }
        if leases.is_empty() {
            warn!(
                failures,
                "HTTP service registry registered no surface; continuing with topology base URLs"
            );
            return None;
        }
        let registered_count = leases.len();
        let registered_services = leases
            .iter()
            .map(|state| state.registration.service_name.as_str())
            .collect::<Vec<_>>()
            .join(",");

        let resolver = HttpRegistryResolver::new(registry.clone(), resolver_config());
        match tokio::time::timeout(Duration::from_secs(5), resolver.refresh_once()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!(
                %error,
                "initial HTTP service registry snapshot failed; callers degrade until refresh succeeds"
            ),
            Err(_) => warn!(
                "initial HTTP service registry snapshot timed out; callers degrade until refresh succeeds"
            ),
        }
        if !install_http_service_registry(resolver.clone()) {
            warn!("an HTTP service registry was already installed; keeping the first view");
        }
        let resolver_refresh = resolver.start();

        let leases = Arc::new(Mutex::new(leases));
        let heartbeat_leases = Arc::clone(&leases);
        let heartbeat_registry = registry.clone();
        let registration_heartbeat = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(HEARTBEAT_INTERVAL_SECONDS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut cycles = 0_u32;
            loop {
                ticker.tick().await;
                {
                    let mut guard = heartbeat_leases.lock().await;
                    for state in guard.iter_mut() {
                        renew_lease(&heartbeat_registry, state).await;
                    }
                }
                cycles = cycles.wrapping_add(1);
                // Producer-side hygiene when no discovery control plane reaps
                // expired rows: every 6th heartbeat (about one minute).
                if cycles % 6 == 0 {
                    match tokio::time::timeout(
                        Duration::from_secs(5),
                        heartbeat_registry.reap_expired(1_000),
                    )
                    .await
                    {
                        Ok(Ok(0)) => {}
                        Ok(Ok(reaped)) => {
                            info!(reaped, "HTTP service registry expired rows reaped")
                        }
                        Ok(Err(error)) => {
                            warn!(%error, "HTTP service registry expiry sweep failed")
                        }
                        Err(_) => warn!("HTTP service registry expiry sweep timed out"),
                    }
                }
            }
        });

        info!(
            endpoint = %endpoint,
            instance_id = %instance_id,
            services = %registered_services,
            instances = registered_count,
            "HTTP service registry registered this gateway instance"
        );

        Some(Self {
            registry,
            leases,
            resolver_refresh,
            registration_heartbeat,
        })
    }

    /// Deregisters every lease and stops both background loops.
    ///
    /// Called after the HTTP server has drained. Errors are logged, never
    /// propagated: shutdown must not fail because the registry is down.
    pub async fn shutdown(self) {
        self.resolver_refresh.abort();
        self.registration_heartbeat.abort();
        let leases = self.leases.lock().await;
        for state in leases.iter() {
            if let Err(error) = self
                .registry
                .deregister(
                    &state.registration.service_name,
                    &state.registration.instance_id,
                )
                .await
            {
                warn!(
                    service = %state.registration.service_name,
                    %error,
                    "HTTP service registry deregistration failed"
                );
            }
        }
        info!(
            instances = leases.len(),
            "HTTP service registry deregistered this gateway instance"
        );
    }
}

/// Renews one lease with a bounded timeout, re-registering if it was lost.
///
/// The timeout matters for shutdown: a hung database must not hold the leases
/// mutex forever while the process is trying to deregister and exit.
async fn renew_lease(registry: &HttpRegistry, state: &mut LeaseState) {
    match tokio::time::timeout(
        Duration::from_secs(3),
        registry.heartbeat(&state.lease.lease_id, LEASE_TTL_SECONDS),
    )
    .await
    {
        Ok(Ok(Some(updated))) => state.lease = updated,
        Ok(Ok(None)) => {
            warn!(
                service = %state.registration.service_name,
                instance_id = %state.registration.instance_id,
                "HTTP service registry lease expired; re-registering"
            );
            match tokio::time::timeout(
                Duration::from_secs(3),
                registry.register(&state.registration),
            )
            .await
            {
                Ok(Ok(updated)) => state.lease = updated,
                Ok(Err(error)) => warn!(
                    service = %state.registration.service_name,
                    %error,
                    "HTTP service registry re-registration failed"
                ),
                Err(_) => warn!(
                    service = %state.registration.service_name,
                    "HTTP service registry re-registration timed out"
                ),
            }
        }
        Ok(Err(error)) => warn!(
            service = %state.registration.service_name,
            %error,
            "HTTP service registry heartbeat failed"
        ),
        Err(_) => warn!(
            service = %state.registration.service_name,
            "HTTP service registry heartbeat timed out"
        ),
    }
}

fn registry_enabled() -> bool {
    match std::env::var(ENABLED_ENV) {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => true,
    }
}

fn registry_namespace() -> String {
    std::env::var(NAMESPACE_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_NAMESPACE.to_owned())
}

/// Registry connection string: a dedicated discovery database wins, otherwise
/// the application database hosts the registry table.
fn registry_database_url() -> Option<String> {
    [ENV_DISCOVERY_DATABASE_URL, "SDKWORK_DATABASE_URL"]
        .into_iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
}

fn resolver_config() -> HttpRegistryConfig {
    let defaults = HttpRegistryConfig::default();
    HttpRegistryConfig {
        refresh_interval_ms: env_u64(REFRESH_INTERVAL_ENV)
            .unwrap_or(defaults.refresh_interval_ms)
            .max(250),
        failure_threshold: env_u64(FAILURE_THRESHOLD_ENV)
            .map(|value| value.clamp(1, 100) as u32)
            .unwrap_or(defaults.failure_threshold),
        failure_cooldown_ms: env_u64(FAILURE_COOLDOWN_ENV)
            .unwrap_or(defaults.failure_cooldown_ms)
            .max(1_000),
        health_probe_path: std::env::var(HEALTH_PROBE_PATH_ENV)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .or(defaults.health_probe_path),
        health_probe_timeout_ms: env_u64(HEALTH_PROBE_TIMEOUT_ENV)
            .unwrap_or(defaults.health_probe_timeout_ms)
            .max(250),
    }
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
}

/// Absolute public origin of this gateway for registry consumers.
///
/// Precedence: the gateway's authored public URL, a generic public URL, then a
/// container/host identity derived origin. The derived form is intentionally
/// last: it is reachable on the compose network, while an authored domain is
/// required for consumers outside the deployment.
fn public_base_url(port: u16) -> String {
    for key in [PUBLIC_HTTP_URL_ENV, GENERIC_PUBLIC_HTTP_URL_ENV] {
        if let Some(value) = std::env::var(key)
            .ok()
            .map(|value| value.trim().trim_end_matches('/').to_owned())
            .filter(|value| !value.is_empty())
        {
            if value.starts_with("http://") || value.starts_with("https://") {
                return value;
            }
            warn!(key, "ignoring non-http(s) public base URL");
        }
    }
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "127.0.0.1".to_owned());
    format!("http://{}:{}", host.trim(), port)
}

fn instance_id(config: &GatewayRuntimeConfig, port: u16) -> String {
    let profile_id = std::env::var("SDKWORK_API_CLOUD_GATEWAY_PROFILE_ID")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            let profile = std::env::var("SDKWORK_API_CLOUD_GATEWAY_DEPLOYMENT_PROFILE")
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "standalone".to_owned());
            format!("{profile}.{}", config.service.environment)
        });
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_else(|| "gateway".to_owned());
    format!("{profile_id}:{}:{port}", host.trim())
}

/// `(service_name, api_prefix)` pairs this gateway should advertise.
fn service_registrations(config: &GatewayRuntimeConfig) -> Vec<(String, Option<String>)> {
    let mut entries = vec![
        ("sdkwork-api-cloud-gateway".to_owned(), None),
        ("api-gateway".to_owned(), None),
    ];
    for surface in &config.dependency_surfaces {
        let prefix = Some(surface.api_prefix.clone());
        entries.push((surface.workspace.clone(), prefix.clone()));
        if let Some(short) = surface.workspace.strip_prefix("sdkwork-") {
            entries.push((short.to_owned(), prefix.clone()));
        }
        if !surface.service_id.trim().is_empty() {
            entries.push((surface.service_id.clone(), prefix));
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.dedup_by(|left, right| left.0 == right.0);
    entries
}

fn registration_metadata(
    config: &GatewayRuntimeConfig,
    api_prefix: Option<&str>,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "deployment_profile".to_owned(),
        std::env::var("SDKWORK_DEPLOYMENT_PROFILE")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "standalone".to_owned()),
    );
    metadata.insert(
        "runtime_target".to_owned(),
        std::env::var("SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "server".to_owned()),
    );
    metadata.insert(
        "gateway_environment".to_owned(),
        config.service.environment.clone(),
    );
    if let Some(prefix) = api_prefix.filter(|value| !value.is_empty()) {
        metadata.insert("api_prefix".to_owned(), prefix.to_owned());
    }
    if let Some(port) = bind_port(&config.server.bind) {
        metadata.insert("bind_port".to_owned(), port.to_string());
    }
    metadata
}

/// Exposed for tests: the service aliases and their stable order.
#[cfg(test)]
pub(crate) fn service_names_for_test(config: &GatewayRuntimeConfig) -> Vec<String> {
    service_registrations(config)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_api_cloud_gateway_config::{
        DependencyApiSurfaceConfig, DependencyRuntimeMode, GatewayRuntimeConfig,
    };

    fn surface(workspace: &str, service_id: &str, api_prefix: &str) -> DependencyApiSurfaceConfig {
        DependencyApiSurfaceConfig {
            service_id: service_id.to_owned(),
            workspace: workspace.to_owned(),
            sdk_family: "sdkwork-test".to_owned(),
            api_authority: "test".to_owned(),
            surface: "app".to_owned(),
            api_prefix: api_prefix.to_owned(),
            runtime_mode: DependencyRuntimeMode::Embedded,
            same_origin_allowed: true,
            executable_export: None,
            cargo_feature: None,
            cargo_dependency: None,
            coverage: "test".to_owned(),
        }
    }

    #[test]
    fn registers_the_gateway_and_every_surface_with_its_sdk_alias_once() {
        let mut config = GatewayRuntimeConfig::default();
        config.dependency_surfaces = vec![
            surface("sdkwork-cloudrouter", "sdkwork-cloudrouter-open-api", "/v1"),
            surface(
                "sdkwork-agents",
                "sdkwork-agents-app-api",
                "/app/v3/api/ai/agents",
            ),
        ];

        let names = service_names_for_test(&config);

        for expected in [
            "sdkwork-api-cloud-gateway",
            "api-gateway",
            "sdkwork-cloudrouter",
            "cloudrouter",
            "sdkwork-cloudrouter-open-api",
            "sdkwork-agents",
            "agents",
            "sdkwork-agents-app-api",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "missing registry service `{expected}`: {names:?}"
            );
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names, unique, "service registrations must be unique");
    }
}
