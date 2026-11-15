//! Admin-managed dynamic CORS policy for the platform edge.
//!
//! The static configuration remains the security floor: exact origins, the
//! registered console-host pattern, the registered sub-domain rule and the
//! `https://*.example.com` / `https://**.example.com` allowlist entries all
//! come from configuration and cannot be removed at runtime. On top of that
//! floor this module loads the admin-managed row of `framework_cors_policy`
//! (`tenant_id` + `environment`) into an immutable in-memory snapshot and
//! hot-swaps it on a fixed interval.
//!
//! Why a scanned snapshot instead of a per-request query:
//!
//! - the request path performs one read-lock and one `Arc` clone, so a CORS
//!   decision never waits on PostgreSQL;
//! - every process and replica runs the scan independently, so a
//!   multi-process or multi-node deployment converges without a restart,
//!   without a deploy and without a shared pub/sub channel;
//! - a scan that finds the same `version` keeps the current snapshot, so the
//!   steady state does no allocation beyond the version query itself;
//! - a missing table or a database outage keeps the last good snapshot (the
//!   static floor on the first load), so CORS never fails open.
//!
//! `version` is the coarse change signal: the admin upsert increments it. A
//! row whose `version` is unchanged is not re-merged, which keeps the scan
//! cheap while still letting an operator force a refresh by bumping the row.
//!
//! Scope: the platform edge serves one tenant scope per deployment (the
//! `cors.dynamic.tenantId` configuration value, default `0`), because the
//! preflight answer is produced before request-context resolution. Tenant
//! scoping inside the framework pipeline remains available through the
//! request-context resolver and is unaffected by this overlay.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, RwLock,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_web_core::cors_policy::{CorsPolicyContext, DynamicCorsPolicySource};
use sdkwork_web_core::{CorsPolicy, WebFrameworkError};
use sqlx::PgPool;
use tracing::{info, warn};

/// Keep in sync with `framework_cors_policy` (web-store-sqlx migration 004 +
/// 014). The gateway owns the platform-scoped read (periodic scan) and the
/// console-facing upsert for this row; the table schema stays owned by the web
/// framework, so any change there must be mirrored here.
///
/// The statements are `&'static str` and bind every value as a parameter, so
/// sqlx's dynamic-SQL audit is satisfied and no caller value reaches the
/// statement text.
const LOAD_POLICY_ROW_SQL: &str =
    "SELECT version, allow_all_origins, allowed_origins, allow_credentials \
     FROM framework_cors_policy WHERE tenant_id = $1 \
     AND (environment = $2 OR environment = $3 OR environment = $4) \
     ORDER BY version DESC LIMIT 1";

/// Mirrors the framework control-plane repository upsert: the first write
/// inserts `version = 1`, every later write increments it, and that version is
/// the change signal every process's scan observes.
const UPSERT_POLICY_ROW_SQL: &str = "INSERT INTO framework_cors_policy \
     (tenant_id, environment, allow_all_origins, allowed_origins, allow_credentials, version) \
     VALUES ($1, $2, $3, $4, $5, 1) \
     ON CONFLICT(tenant_id, environment) DO UPDATE SET \
       allow_all_origins = excluded.allow_all_origins, \
       allowed_origins = excluded.allowed_origins, \
       allow_credentials = excluded.allow_credentials, \
       version = framework_cors_policy.version + 1 \
     RETURNING version, allow_all_origins, allowed_origins, allow_credentials";

/// Deployment scope of the admin-managed policy row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CorsPolicyScope {
    pub(crate) tenant_id: String,
    pub(crate) environment: String,
    /// Alternate labels the same deployment may have written, because the
    /// framework pipeline uses `prod`/`test`/`dev` while an operator or an
    /// older admin surface may write `production`/`staging`/`demo`.
    pub(crate) environment_aliases: Vec<String>,
    pub(crate) refresh_interval: Duration,
    pub(crate) production_like: bool,
}

impl CorsPolicyScope {
    pub(crate) fn from_config(config: &GatewayRuntimeConfig) -> Self {
        let environment = config.service.environment.trim().to_ascii_lowercase();
        let primary = policy_environment_label(environment.as_str()).to_owned();
        let mut environment_aliases = vec![primary.clone()];
        if environment != primary && !environment.is_empty() {
            environment_aliases.push(environment.clone());
        }
        Self {
            tenant_id: config.cors.dynamic.tenant_id.trim().to_owned(),
            environment: primary,
            environment_aliases,
            refresh_interval: Duration::from_secs(config.cors.dynamic.refresh_interval_seconds),
            production_like: matches!(
                environment.as_str(),
                "production" | "prod" | "staging" | "stage" | "live"
            ),
        }
    }

    /// Returns exactly three bound labels so the read stays a single statement
    /// without a PostgreSQL array bind.
    fn environment_candidates(&self) -> [String; 3] {
        let mut candidates = self.environment_aliases.clone();
        if !candidates.iter().any(|value| value == &self.environment) {
            candidates.insert(0, self.environment.clone());
        }
        while candidates.len() < 3 {
            candidates.push(self.environment.clone());
        }
        candidates.truncate(3);
        [
            candidates[0].clone(),
            candidates[1].clone(),
            candidates[2].clone(),
        ]
    }
}

/// Maps the lifecycle environment to the framework pipeline's environment
/// label (`CorsPolicyContext::environment_label`), so the overlay row the edge
/// reads is the same row the pipeline would resolve.
pub(crate) fn policy_environment_label(environment: &str) -> &'static str {
    match environment {
        "production" | "prod" | "staging" | "stage" | "live" => "prod",
        "test" | "testing" | "demo" => "test",
        _ => "dev",
    }
}

/// One `framework_cors_policy` row as stored by the framework control plane.
///
/// The gateway owns both reads (the periodic scan) and the operator-facing
/// upsert, so the table is accessed from this one module only. The upsert SQL
/// mirrors the framework control-plane repository (`version` starts at 1 and
/// increments on every write) without depending on that crate's implementation,
/// per the API assembly integration contract.
#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct CorsPolicyRow {
    pub(crate) version: i32,
    pub(crate) allow_all_origins: i32,
    pub(crate) allowed_origins: String,
    pub(crate) allow_credentials: i32,
}

/// Immutable, request-path-safe view of the effective CORS policy.
#[derive(Debug)]
pub(crate) struct CorsPolicySnapshot {
    policy: CorsPolicy,
    version: i64,
    dynamic_origin_count: usize,
    /// Whether the snapshot differs from the static floor. When it does not,
    /// the framework pipeline can keep using its own static policy and skip the
    /// per-request overlay clone entirely.
    has_overrides: bool,
}

impl CorsPolicySnapshot {
    fn static_only(base: CorsPolicy) -> Self {
        Self {
            policy: base,
            version: 0,
            dynamic_origin_count: 0,
            has_overrides: false,
        }
    }

    pub(crate) fn policy(&self) -> &CorsPolicy {
        &self.policy
    }

    pub(crate) fn version(&self) -> i64 {
        self.version
    }

    pub(crate) fn dynamic_origin_count(&self) -> usize {
        self.dynamic_origin_count
    }

    pub(crate) fn has_overrides(&self) -> bool {
        self.has_overrides
    }
}

/// Cumulative counters exposed through the diagnostics endpoint.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CorsRefreshMetrics {
    pub(crate) attempts: u64,
    pub(crate) failures: u64,
    pub(crate) applied_version: i64,
    pub(crate) dynamic_origins: usize,
}

/// Hot-swappable CORS policy shared by the edge layer, the response-header
/// writer and the embedded framework pipeline.
pub(crate) struct GatewayCorsPolicy {
    base: CorsPolicy,
    snapshot: RwLock<Arc<CorsPolicySnapshot>>,
    refresh_attempts: AtomicU64,
    refresh_failures: AtomicU64,
}

impl GatewayCorsPolicy {
    /// Builds the process-wide policy from the static configuration floor.
    pub(crate) fn new(base: CorsPolicy) -> Arc<Self> {
        Arc::new(Self {
            snapshot: RwLock::new(Arc::new(CorsPolicySnapshot::static_only(base.clone()))),
            base,
            refresh_attempts: AtomicU64::new(0),
            refresh_failures: AtomicU64::new(0),
        })
    }

    /// Returns the current snapshot. Callers hold the `Arc` for the duration of
    /// one decision; the write side never mutates a published snapshot.
    pub(crate) fn snapshot(&self) -> Arc<CorsPolicySnapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn allows_origin(&self, origin: &str) -> bool {
        self.snapshot().policy.allows_origin_value(origin)
    }

    pub(crate) fn metrics(&self) -> CorsRefreshMetrics {
        let snapshot = self.snapshot();
        CorsRefreshMetrics {
            attempts: self.refresh_attempts.load(Ordering::Relaxed),
            failures: self.refresh_failures.load(Ordering::Relaxed),
            applied_version: snapshot.version(),
            dynamic_origins: snapshot.dynamic_origin_count(),
        }
    }

    /// Loads the admin-managed row and publishes a new snapshot when the row
    /// version changed. Returns whether a new snapshot was published.
    pub(crate) async fn refresh_from_db(
        &self,
        pool: &PgPool,
        scope: &CorsPolicyScope,
    ) -> Result<bool, String> {
        self.refresh_attempts.fetch_add(1, Ordering::Relaxed);
        let row = match load_row(pool, scope).await {
            Ok(row) => row,
            Err(error) => {
                self.refresh_failures.fetch_add(1, Ordering::Relaxed);
                return Err(error);
            }
        };
        let current_version = self.snapshot().version();
        let next_version = row.as_ref().map_or(0, |row| i64::from(row.version));
        if row.is_some() && next_version == current_version {
            return Ok(false);
        }
        let snapshot = build_snapshot(&self.base, row.as_ref(), scope);
        let mut guard = self
            .snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Arc::new(snapshot);
        Ok(true)
    }

    /// Starts the process-lifetime periodic scan.
    ///
    /// Ownership: the spawned task belongs to the process runtime and exits
    /// with it; it holds its own `Arc` of the policy and the shared pool, so no
    /// shutdown signal is required. `interval` is bounded by configuration
    /// validation (2s..=3600s), and each process starts at a slightly different
    /// offset so a fleet restart does not stampede the database.
    pub(crate) fn spawn_refresher(self: &Arc<Self>, pool: PgPool, scope: CorsPolicyScope) {
        let policy = Arc::clone(self);
        let interval = scope.refresh_interval;
        tokio::spawn(async move {
            let jitter = fleet_jitter(interval);
            if !jitter.is_zero() {
                tokio::time::sleep(jitter).await;
            }
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                match policy.refresh_from_db(&pool, &scope).await {
                    Ok(true) => {
                        let snapshot = policy.snapshot();
                        info!(
                            target: "sdkwork.api.cloud_gateway",
                            version = snapshot.version(),
                            dynamic_origins = snapshot.dynamic_origin_count(),
                            tenant_id = %scope.tenant_id,
                            environment = %scope.environment,
                            "applied admin-managed CORS policy"
                        );
                    }
                    Ok(false) => {}
                    Err(error) => {
                        warn!(
                            target: "sdkwork.api.cloud_gateway",
                            %error,
                            tenant_id = %scope.tenant_id,
                            environment = %scope.environment,
                            "admin-managed CORS policy refresh failed; keeping the last good snapshot"
                        );
                    }
                }
            }
        });
    }
}

#[async_trait]
impl DynamicCorsPolicySource for GatewayCorsPolicy {
    async fn resolve(
        &self,
        _ctx: &CorsPolicyContext,
    ) -> Result<Option<CorsPolicy>, WebFrameworkError> {
        let snapshot = self.snapshot();
        if !snapshot.has_overrides() {
            // The static pipeline policy already equals this snapshot; return
            // `None` so the pipeline keeps its zero-copy reference.
            return Ok(None);
        }
        Ok(Some(snapshot.policy().clone()))
    }
}

pub(crate) async fn load_row(
    pool: &PgPool,
    scope: &CorsPolicyScope,
) -> Result<Option<CorsPolicyRow>, String> {
    let [primary, alias, fallback] = scope.environment_candidates();
    sqlx::query_as::<_, CorsPolicyRow>(LOAD_POLICY_ROW_SQL)
        .bind(scope.tenant_id.as_str())
        .bind(primary)
        .bind(alias)
        .bind(fallback)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("framework_cors_policy read failed: {error}"))
}

/// Upserts the platform-scoped row and returns the stored record (including the
/// incremented version), mirroring the framework control-plane repository.
pub(crate) async fn upsert_row(
    pool: &PgPool,
    scope: &CorsPolicyScope,
    allow_all_origins: bool,
    allowed_origins: &[String],
    allow_credentials: bool,
) -> Result<CorsPolicyRow, String> {
    let origins_json = serde_json::to_string(allowed_origins)
        .map_err(|error| format!("allowed origins payload is invalid: {error}"))?;
    sqlx::query_as::<_, CorsPolicyRow>(UPSERT_POLICY_ROW_SQL)
        .bind(scope.tenant_id.as_str())
        .bind(scope.environment.as_str())
        .bind(i64::from(allow_all_origins))
        .bind(origins_json)
        .bind(i64::from(allow_credentials))
        .fetch_one(pool)
        .await
        .map_err(|error| format!("framework_cors_policy upsert failed: {error}"))
}

/// Decodes the stored JSON origin array. A corrupt payload yields an empty list
/// (fail-closed: the static floor still applies).
pub(crate) fn decode_allowed_origins(json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json).unwrap_or_default()
}

fn build_snapshot(
    base: &CorsPolicy,
    row: Option<&CorsPolicyRow>,
    scope: &CorsPolicyScope,
) -> CorsPolicySnapshot {
    let Some(row) = row else {
        return CorsPolicySnapshot::static_only(base.clone());
    };
    let mut policy = base.clone();
    let mut dynamic_origin_count = 0usize;
    let mut has_overrides = false;

    if row.allow_all_origins != 0 {
        if scope.production_like {
            warn!(
                target: "sdkwork.api.cloud_gateway",
                tenant_id = %scope.tenant_id,
                environment = %scope.environment,
                "ignoring allow_all_origins from the admin-managed CORS policy in a production-like environment; declare explicit origins or host wildcards"
            );
        } else {
            policy.allow_all_origins = true;
            has_overrides = true;
        }
    }

    let parsed =
        serde_json::from_str::<Vec<String>>(row.allowed_origins.as_str()).unwrap_or_else(|error| {
            warn!(
                target: "sdkwork.api.cloud_gateway",
                %error,
                tenant_id = %scope.tenant_id,
                environment = %scope.environment,
                "admin-managed CORS policy allowed_origins is not a JSON string array; ignoring it"
            );
            Vec::new()
        });
    for origin in parsed {
        let Some(origin) = normalize_dynamic_origin(origin.as_str()) else {
            warn!(
                target: "sdkwork.api.cloud_gateway",
                origin = %origin,
                "ignoring invalid admin-managed CORS origin"
            );
            continue;
        };
        if policy.allowed_origins.iter().any(|known| known == &origin) {
            continue;
        }
        policy.allowed_origins.push(origin);
        dynamic_origin_count += 1;
        has_overrides = true;
    }

    let dynamic_credentials = row.allow_credentials != 0;
    if dynamic_credentials != policy.allow_credentials {
        policy.allow_credentials = dynamic_credentials;
        has_overrides = true;
    }

    CorsPolicySnapshot {
        policy,
        version: i64::from(row.version),
        dynamic_origin_count,
        has_overrides,
    }
}

/// Accepts only an exact `http(s)` origin or a host-wildcard allowlist entry
/// (`https://*.example.com`, `https://**.example.com`). Everything else —
/// credentials, paths, queries, fragments, non-HTTP schemes, `*` — is rejected
/// so an admin write can never widen the policy to `null` or a script URL.
fn normalize_dynamic_origin(origin: &str) -> Option<String> {
    let origin = origin.trim();
    if origin.is_empty() || origin.contains('*') && !origin.contains("*.") {
        return None;
    }
    if sdkwork_web_core::cors_wildcard::is_host_wildcard_origin_pattern(origin) {
        return Some(origin.to_owned());
    }
    let authority = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))?;
    if authority.is_empty()
        || authority.contains(['/', '?', '#', '@', '*'])
        || origin != origin.trim()
    {
        return None;
    }
    Some(origin.to_owned())
}

fn fleet_jitter(interval: Duration) -> Duration {
    let spread = interval.as_millis() / 4;
    if spread == 0 {
        return Duration::ZERO;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u128::from(elapsed.subsec_nanos()))
        .unwrap_or(0);
    Duration::from_millis(u64::try_from(nanos % spread).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdkwork_web_core::RegisteredSubdomainHosts;

    fn base_policy() -> CorsPolicy {
        CorsPolicy {
            allowed_origins: vec!["https://platform.birdcoder.com".to_owned()],
            registered_subdomain_hosts: Some(RegisteredSubdomainHosts::new(
                vec!["sdkwork.com".to_owned()],
                vec!["https".to_owned()],
            )),
            ..CorsPolicy::default()
        }
    }

    fn scope(production_like: bool) -> CorsPolicyScope {
        CorsPolicyScope {
            tenant_id: "0".to_owned(),
            environment: "prod".to_owned(),
            environment_aliases: vec!["prod".to_owned(), "production".to_owned()],
            refresh_interval: Duration::from_secs(10),
            production_like,
        }
    }

    #[test]
    fn scope_accepts_the_pipeline_label_and_the_lifecycle_name() {
        let mut config = GatewayRuntimeConfig::default();
        config.service.environment = "production".to_owned();
        let scope = CorsPolicyScope::from_config(&config);
        assert_eq!("prod", scope.environment);
        let candidates = scope.environment_candidates();
        assert!(candidates.contains(&"prod".to_owned()));
        assert!(candidates.contains(&"production".to_owned()));
        assert!(scope.production_like);

        let mut demo = GatewayRuntimeConfig::default();
        demo.service.environment = "demo".to_owned();
        let demo_scope = CorsPolicyScope::from_config(&demo);
        assert_eq!("test", demo_scope.environment);
        assert!(!demo_scope.production_like);
    }

    fn row(version: i32, origins: &[&str], allow_all: bool, credentials: bool) -> CorsPolicyRow {
        CorsPolicyRow {
            version,
            allow_all_origins: i32::from(allow_all),
            allowed_origins: serde_json::to_string(
                &origins
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>(),
            )
            .expect("json"),
            allow_credentials: i32::from(credentials),
        }
    }

    #[test]
    fn static_floor_snapshot_has_no_overrides() {
        let snapshot = build_snapshot(&base_policy(), None, &scope(true));
        assert!(!snapshot.has_overrides());
        assert_eq!(0, snapshot.version());
        assert!(snapshot
            .policy()
            .allows_origin_value("https://a.b.sdkwork.com"));
    }

    #[test]
    fn admin_origins_extend_the_static_floor_at_every_depth() {
        let row = row(
            7,
            &[
                "https://mall.sdkwork.com",
                "https://**.birdcoder.com",
                "https://platform.birdcoder.com",
                "javascript:alert(1)",
                "https://user@evil.example.com",
                "https://evil.example.com/path",
                "null",
                "*",
            ],
            false,
            true,
        );
        let snapshot = build_snapshot(&base_policy(), Some(&row), &scope(true));
        assert!(snapshot.has_overrides());
        assert_eq!(7, snapshot.version());
        assert_eq!(2, snapshot.dynamic_origin_count());
        let policy = snapshot.policy();
        for origin in [
            "https://mall.sdkwork.com",
            "https://a.b.sdkwork.com",
            "https://x.y.z.sdkwork.com",
            "https://a.b.c.birdcoder.com",
            "https://platform.birdcoder.com",
        ] {
            assert!(
                policy.allows_origin_value(origin),
                "expected `{origin}` to be allowed"
            );
        }
        for origin in [
            "https://evil.example.com",
            "https://notbirdcoder.com",
            "https://birdcoder.com.attacker.example",
        ] {
            assert!(
                !policy.allows_origin_value(origin),
                "expected `{origin}` to stay rejected"
            );
        }
    }

    #[test]
    fn production_ignores_allow_all_origins_while_development_honors_it() {
        let row = row(2, &[], true, true);
        let production = build_snapshot(&base_policy(), Some(&row), &scope(true));
        assert!(!production.policy().allow_all_origins);

        let development = build_snapshot(&base_policy(), Some(&row), &scope(false));
        assert!(development.policy().allow_all_origins);
        assert!(development.has_overrides());
    }

    #[test]
    fn malformed_allowed_origins_payload_keeps_the_static_floor() {
        let mut row = row(3, &[], false, true);
        row.allowed_origins = "not-json".to_owned();
        let snapshot = build_snapshot(&base_policy(), Some(&row), &scope(true));
        assert!(!snapshot.has_overrides());
        assert!(snapshot
            .policy()
            .allows_origin_value("https://a.b.sdkwork.com"));
    }

    #[test]
    fn environment_labels_match_the_framework_pipeline() {
        assert_eq!("prod", policy_environment_label("production"));
        assert_eq!("prod", policy_environment_label("staging"));
        assert_eq!("test", policy_environment_label("demo"));
        assert_eq!("test", policy_environment_label("test"));
        assert_eq!("dev", policy_environment_label("development"));
        assert_eq!("dev", policy_environment_label("local"));
    }
}
