use std::path::PathBuf;
use std::sync::Arc;

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_lifecycle::{
    discover_database_modules, lifecycle_options_from_env, LifecycleOrchestrator,
};
use sdkwork_database_spi::traits::DatabaseModuleDescriptorProvider;
use sdkwork_database_spi::{DatabaseAssetProvider, DatabaseManifest, DefaultDatabaseModule};
use sdkwork_database_sqlx::{
    create_process_shared_pool_from_env, process_shared_database_pool, DatabasePool,
};

use crate::GatewayRuntimeError;

const PROCESS_DATABASE_SERVICE_CODE: &str = "CLOUD";
const DATABASE_BOOTSTRAP_APPLIED_BY: &str = "sdkwork-api-cloud-gateway";
const DEVELOPER_CATALOG_WORKSPACE: &str = "sdkwork-documents";

/// Creates the process-shared pool used by every selected owner assembly.
///
/// Module database lifecycle belongs to each `sdkwork-api-*-assembly`. The
/// platform host provides the pool and topology context only; it never imports
/// or invokes dependency-owned database hosts directly.
pub async fn bootstrap_embedded_databases(
    config: &GatewayRuntimeConfig,
) -> Result<Option<DatabasePool>, GatewayRuntimeError> {
    if config.dependency_surfaces.is_empty() {
        return Ok(None);
    }

    if config.database.auto_migrate && std::env::var_os("SDKWORK_DATABASE_AUTO_MIGRATE").is_none() {
        // SAFETY: gateway startup sets this before dependency workers spawn.
        unsafe { std::env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true") };
    }

    process_database_pool().await.map(Some)
}

/// Discovers database modules for every enabled dependency surface and runs
/// their lifecycle bootstrap on the process-shared pool.
///
/// Implements DATABASE_FRAMEWORK_SPEC §4.3 startup sequence steps 3-6 for
/// embedded surfaces: discovery (via `SDKWORK_<MODULE>_APP_ROOT`), policy
/// resolution from each module manifest plus the `SDKWORK_DATABASE_*` env
/// keys, then init / migrate / seed. Failures fail closed so the gateway
/// never serves traffic against a half-initialized schema. When
/// `force_migrate` is set (explicit `--migrate-databases` runs), migration
/// executes regardless of `SDKWORK_DATABASE_AUTO_MIGRATE`.
pub async fn discover_and_bootstrap_embedded_database_modules(
    config: &GatewayRuntimeConfig,
    pool: &DatabasePool,
    force_migrate: bool,
) -> Result<(), GatewayRuntimeError> {
    // Standalone container/install packages ship every dependency's database
    // module under SDKWORK_DATABASE_MODULES_ROOT. Provision them before surface
    // discovery so a fresh database reaches a complete schema regardless of
    // which surfaces the runtime configuration selects: declared sub-modules
    // first (their schemas are prerequisites of the owning module's later
    // migrations, e.g. the cloudrouter payment tables), then every root module.
    // Idempotent: installed modules return early and applied migrations are
    // recorded in ops_schema_migration_history.
    provision_packaged_database_modules(pool).await?;

    let surfaces = enabled_surface_app_roots(config);
    let app_roots: Vec<PathBuf> = surfaces.iter().map(|(_, root)| root.clone()).collect();
    let workspace_by_root: std::collections::HashMap<PathBuf, String> = surfaces
        .iter()
        .map(|(workspace, root)| (root.clone(), workspace.clone()))
        .collect();
    let (registry, report) = discover_database_modules(&app_roots).map_err(|error| {
        GatewayRuntimeError::EmbeddedDatabaseBootstrap {
            workspace: "(discovery)".to_owned(),
            module_id: "(discovery)".to_owned(),
            message: error.to_string(),
        }
    })?;
    for skipped in &report.skipped_without_database_assets {
        tracing::debug!(
            target: "sdkwork.api.cloud_gateway.database",
            app_root = %skipped.display(),
            "dependency surface ships no database assets; skipping bootstrap"
        );
    }

    let mut bootstrapped_workspaces = std::collections::HashSet::new();
    for (discovered, module) in report.registered.iter().zip(registry.modules().iter()) {
        let Some(workspace) = workspace_by_root.get(&discovered.app_root) else {
            continue;
        };
        bootstrap_discovered_module(workspace, module.clone(), pool, force_migrate).await?;
        bootstrapped_workspaces.insert(workspace.clone());
    }

    // The developer catalog is platform-wide content owned by the documents
    // module. It is initialized even when the deployment does not export an
    // explicit SDKWORK_DOCUMENTS_APP_ROOT, because packaged installs carry the
    // documents database assets under SDKWORK_DATABASE_MODULES_ROOT. When the
    // surface is already selected through its explicit root, the loop above has
    // already bootstrapped it.
    if !bootstrapped_workspaces.contains(DEVELOPER_CATALOG_WORKSPACE) {
        bootstrap_developer_catalog(pool, force_migrate).await?;
    }

    tracing::info!(
        target: "sdkwork.api.cloud_gateway.database",
        registered = report.registered.len(),
        skipped = report.skipped_without_database_assets.len(),
        force_migrate,
        "embedded database module discovery and bootstrap completed"
    );
    Ok(())
}

/// Database modules provisioned from a packaged standalone installation.
struct PackagedDatabaseModule {
    app_root: PathBuf,
    workspace: String,
    module: Arc<dyn sdkwork_database_spi::DatabaseModule>,
    manifest: DatabaseManifest,
}

/// Lists the packaged database application roots (`<root>/database/database.manifest.json`).
fn packaged_database_app_roots() -> Result<Vec<PathBuf>, GatewayRuntimeError> {
    let Some(root) = std::env::var_os("SDKWORK_DATABASE_MODULES_ROOT").filter(|value| !value.is_empty())
    else {
        return Ok(Vec::new());
    };
    let root = PathBuf::from(root);
    if !root.is_dir() {
        tracing::warn!(
            target: "sdkwork.api.cloud_gateway.database",
            root = %root.display(),
            "SDKWORK_DATABASE_MODULES_ROOT is not a directory; skipping packaged database provisioning"
        );
        return Ok(Vec::new());
    }
    let mut app_roots = Vec::new();
    let entries = std::fs::read_dir(&root).map_err(|error| {
        GatewayRuntimeError::EmbeddedDatabaseBootstrap {
            workspace: "(packaged)".to_owned(),
            module_id: "(packaged)".to_owned(),
            message: format!("cannot read {}: {error}", root.display()),
        }
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| GatewayRuntimeError::EmbeddedDatabaseBootstrap {
            workspace: "(packaged)".to_owned(),
            module_id: "(packaged)".to_owned(),
            message: format!("cannot read entry under {}: {error}", root.display()),
        })?;
        let path = entry.path();
        if path.join("database").join("database.manifest.json").is_file() {
            app_roots.push(path);
        }
    }
    app_roots.sort();
    Ok(app_roots)
}

/// Lifecycle-safe module id guard (mirrors the packaged module directory contract).
fn valid_packaged_module_id(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn packaged_module_error(
    workspace: &str,
    module_id: &str,
    message: impl Into<String>,
) -> GatewayRuntimeError {
    GatewayRuntimeError::EmbeddedDatabaseBootstrap {
        workspace: workspace.to_owned(),
        module_id: module_id.to_owned(),
        message: message.into(),
    }
}

/// Provisions every database module shipped inside a packaged standalone tree.
///
/// Standalone container images and server packages carry the dependency-owned
/// database assets under `SDKWORK_DATABASE_MODULES_ROOT` instead of the source
/// checkout layout the dependency surfaces resolve. Without this pass a fresh
/// database only gets the surfaces the runtime configuration selected, so
/// embedded app routers fail their schema preflight (memory, models, ...) and
/// modules whose later migrations depend on a declared sub-module baseline
/// (cloudrouter payment tables) fail with "relation does not exist".
///
/// Declared sub-modules are provisioned first, then every root module; both
/// phases force migrations so a fresh install reaches `schema_current` without
/// a separate release job. Re-runs are cheap: installed modules skip their
/// baseline and applied migrations are skipped by the history table.
async fn provision_packaged_database_modules(
    pool: &DatabasePool,
) -> Result<(), GatewayRuntimeError> {
    let mut modules: Vec<PackagedDatabaseModule> = Vec::new();
    for app_root in packaged_database_app_roots()? {
        let workspace = app_root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("(packaged)")
            .to_owned();
        let module = DefaultDatabaseModule::from_app_root(&app_root)
            .map_err(|error| packaged_module_error(&workspace, "(packaged)", error.to_string()))?;
        let manifest = DatabaseManifest::from_file(module.manifest_path())
            .map_err(|error| packaged_module_error(&workspace, &module.descriptor().module_id, error.to_string()))?;
        modules.push(PackagedDatabaseModule {
            app_root,
            workspace,
            module: Arc::new(module),
            manifest,
        });
    }
    if modules.is_empty() {
        return Ok(());
    }

    // Pass 1 and 2 run in dependency-tolerant rounds: packaged baselines can
    // reference tables owned by a sibling module that appears later in the
    // manifest order (for example appbase `edu-data` referencing the `base-data`
    // country table, or cloudrouter migrations referencing a sub-module
    // baseline). A round retries only the modules whose previous attempt failed;
    // a round without progress is a hard failure.
    let mut root_modules: Vec<(String, Arc<dyn sdkwork_database_spi::DatabaseModule>)> =
        Vec::with_capacity(modules.len());
    let mut submodule_modules: Vec<(String, Arc<dyn sdkwork_database_spi::DatabaseModule>)> =
        Vec::new();
    for packaged in &modules {
        for submodule_id in &packaged.manifest.modules {
            if !valid_packaged_module_id(submodule_id) {
                return Err(packaged_module_error(
                    &packaged.workspace,
                    submodule_id,
                    "invalid database sub-module id",
                ));
            }
            let submodule_root = packaged
                .app_root
                .join("database")
                .join("modules")
                .join(submodule_id);
            if !submodule_root.join("database.manifest.json").is_file() {
                continue;
            }
            let label = format!("{}/{}", packaged.workspace, submodule_id);
            let submodule = DefaultDatabaseModule::from_module_root(&submodule_root)
                .map_err(|error| packaged_module_error(&label, submodule_id, error.to_string()))?;
            let submodule_manifest = DatabaseManifest::from_file(submodule.manifest_path())
                .map_err(|error| packaged_module_error(&label, submodule_id, error.to_string()))?;
            if submodule_manifest.module_id != *submodule_id {
                return Err(packaged_module_error(
                    &label,
                    submodule_id,
                    format!(
                        "sub-module directory declares moduleId {}",
                        submodule_manifest.module_id
                    ),
                ));
            }
            submodule_modules.push((label, Arc::new(submodule)));
        }
        root_modules.push((
            packaged.workspace.clone(),
            packaged.module.clone(),
        ));
    }

    // Dependency ordering across packaged modules is not guaranteed by the
    // manifests: an owning module's baseline can be a prerequisite of a
    // declared sub-module baseline (appbase `base-data` -> `edu-data`), while a
    // sub-module baseline can be a prerequisite of the owner's later migrations
    // (cloudrouter payment-control-plane -> migration 0043). Provision in three
    // dependency-tolerant stages: root baselines, sub-modules, root migrations.
    bootstrap_packaged_modules_until_stable(
        root_modules.clone(),
        pool,
        "root baselines",
        ProvisionStage::InitOnly,
    )
    .await?;
    bootstrap_packaged_modules_until_stable(
        submodule_modules,
        pool,
        "sub-modules",
        ProvisionStage::MigrateOnly,
    )
    .await?;
    let provisioned = root_modules.len();
    bootstrap_packaged_modules_until_stable(
        root_modules,
        pool,
        "root modules",
        ProvisionStage::MigrateOnly,
    )
    .await?;
    tracing::info!(
        target: "sdkwork.api.cloud_gateway.database",
        modules = provisioned,
        "packaged database module provisioning completed"
    );
    Ok(())
}

/// Provisioning stage for a packaged module.
#[derive(Clone, Copy)]
enum ProvisionStage {
    /// Apply the module baseline only (used to make root tables available to
    /// declared sub-module baselines before any migration runs).
    InitOnly,
    /// Baseline plus pending migrations, without applying seeds. Seeds stay
    /// owned by the surface/bootstrap paths so each declared seed runs once
    /// through its own locale/profile policy instead of on every packaged
    /// module (course/document catalogs are large and slow).
    MigrateOnly,
}

async fn init_packaged_module(
    label: &str,
    module: Arc<dyn sdkwork_database_spi::DatabaseModule>,
    pool: &DatabasePool,
) -> Result<(), GatewayRuntimeError> {
    let manifest = DatabaseManifest::from_file(module.manifest_path()).map_err(|error| {
        packaged_module_error(label, "(packaged)", error.to_string())
    })?;
    let descriptor = module.descriptor();
    let _options = lifecycle_options_from_env(&descriptor.service_code, &manifest);
    LifecycleOrchestrator::new(pool.clone(), module)
        .with_applied_by(DATABASE_BOOTSTRAP_APPLIED_BY)
        .init()
        .await
        .map_err(|error| {
            packaged_module_error(label, &descriptor.module_id, format!("init failed: {error}"))
        })
}

/// Applies the baseline (if needed) and pending migrations without seeding.
async fn migrate_packaged_module(
    label: &str,
    module: Arc<dyn sdkwork_database_spi::DatabaseModule>,
    pool: &DatabasePool,
) -> Result<(), GatewayRuntimeError> {
    let manifest = DatabaseManifest::from_file(module.manifest_path()).map_err(|error| {
        packaged_module_error(label, "(packaged)", error.to_string())
    })?;
    let descriptor = module.descriptor();
    let _options = lifecycle_options_from_env(&descriptor.service_code, &manifest);
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module)
        .with_applied_by(DATABASE_BOOTSTRAP_APPLIED_BY);
    orchestrator.init().await.map_err(|error| {
        packaged_module_error(label, &descriptor.module_id, format!("init failed: {error}"))
    })?;
    orchestrator.migrate().await.map_err(|error| {
        packaged_module_error(label, &descriptor.module_id, format!("migrate failed: {error}"))
    })?;
    Ok(())
}

/// Bootstraps a set of packaged modules in dependency-tolerant rounds.
async fn bootstrap_packaged_modules_until_stable(
    mut pending: Vec<(String, Arc<dyn sdkwork_database_spi::DatabaseModule>)>,
    pool: &DatabasePool,
    phase: &'static str,
    stage: ProvisionStage,
) -> Result<(), GatewayRuntimeError> {
    const MAX_ROUNDS: usize = 8;
    let mut rounds = 0usize;
    while !pending.is_empty() {
        rounds += 1;
        let attempted = pending.len();
        let mut failed: Vec<(String, Arc<dyn sdkwork_database_spi::DatabaseModule>)> = Vec::new();
        let mut first_error: Option<String> = None;
        for (label, module) in pending {
            let result = match stage {
                ProvisionStage::InitOnly => {
                    init_packaged_module(&label, module.clone(), pool).await
                }
                ProvisionStage::MigrateOnly => {
                    migrate_packaged_module(&label, module.clone(), pool).await
                }
            };
            match result {
                Ok(()) => {}
                Err(error) => {
                    let message = error.to_string();
                    if first_error.is_none() {
                        first_error = Some(format!("{label}: {message}"));
                    }
                    failed.push((label, module));
                }
            }
        }
        if failed.is_empty() {
            return Ok(());
        }
        if failed.len() == attempted || rounds >= MAX_ROUNDS {
            let detail = first_error.unwrap_or_default();
            return Err(packaged_module_error(
                "(packaged)",
                "(packaged)",
                format!("{phase} provisioning failed after {rounds} round(s): {detail}"),
            ));
        }
        tracing::warn!(
            target: "sdkwork.api.cloud_gateway.database",
            phase,
            remaining = failed.len(),
            rounds,
            "packaged database modules retrying after dependency ordering"
        );
        pending = failed;
    }
    Ok(())
}

/// Initializes the developer catalog through the documents module lifecycle.
///
/// The catalog powers the API market and SDK market. It is idempotent: the
/// lifecycle records each seed file's checksum, so an unchanged catalog is
/// skipped and a changed catalog is upserted. Set
/// `SDKWORK_DEVELOPER_CATALOG_BOOTSTRAP=false` to opt out.
async fn bootstrap_developer_catalog(
    pool: &DatabasePool,
    force_migrate: bool,
) -> Result<(), GatewayRuntimeError> {
    if std::env::var("SDKWORK_DEVELOPER_CATALOG_BOOTSTRAP")
        .is_ok_and(|value| value.eq_ignore_ascii_case("false"))
    {
        tracing::info!(
            target: "sdkwork.api.cloud_gateway.database",
            "developer catalog bootstrap disabled by SDKWORK_DEVELOPER_CATALOG_BOOTSTRAP"
        );
        return Ok(());
    }
    let Some(app_root) = resolve_developer_catalog_app_root() else {
        tracing::debug!(
            target: "sdkwork.api.cloud_gateway.database",
            "developer catalog database assets not found; skipping catalog bootstrap"
        );
        return Ok(());
    };
    let (registry, report) =
        discover_database_modules(std::slice::from_ref(&app_root)).map_err(|error| {
            GatewayRuntimeError::EmbeddedDatabaseBootstrap {
                workspace: DEVELOPER_CATALOG_WORKSPACE.to_owned(),
                module_id: "(discovery)".to_owned(),
                message: error.to_string(),
            }
        })?;
    if report.registered.is_empty() {
        tracing::warn!(
            target: "sdkwork.api.cloud_gateway.database",
            app_root = %app_root.display(),
            "developer catalog app root ships no database manifest; catalog data will be missing"
        );
        return Ok(());
    }

    for module in registry.modules() {
        bootstrap_discovered_module(
            DEVELOPER_CATALOG_WORKSPACE,
            module.clone(),
            pool,
            force_migrate,
        )
        .await?;
    }
    tracing::info!(
        target: "sdkwork.api.cloud_gateway.database",
        app_root = %app_root.display(),
        "developer catalog database bootstrap completed"
    );
    Ok(())
}

async fn bootstrap_discovered_module(
    workspace: &str,
    module: std::sync::Arc<dyn sdkwork_database_spi::DatabaseModule>,
    pool: &DatabasePool,
    force_migrate: bool,
) -> Result<(), GatewayRuntimeError> {
    let descriptor = module.descriptor();
    let module_id = descriptor.module_id.clone();
    let manifest = DatabaseManifest::from_file(module.manifest_path()).map_err(|error| {
        GatewayRuntimeError::EmbeddedDatabaseBootstrap {
            workspace: workspace.to_owned(),
            module_id: module_id.clone(),
            message: error.to_string(),
        }
    })?;
    let options = lifecycle_options_from_env(&descriptor.service_code, &manifest);
    let orchestrator = LifecycleOrchestrator::new(pool.clone(), module)
        .with_applied_by(DATABASE_BOOTSTRAP_APPLIED_BY);

    orchestrator
        .init()
        .await
        .map_err(|error| GatewayRuntimeError::EmbeddedDatabaseBootstrap {
            workspace: workspace.to_owned(),
            module_id: module_id.clone(),
            message: format!("init failed: {error}"),
        })?;

    let mut applied_migrations = 0usize;
    if force_migrate || options.auto_migrate {
        applied_migrations = orchestrator.migrate().await.map_err(|error| {
            GatewayRuntimeError::EmbeddedDatabaseBootstrap {
                workspace: workspace.to_owned(),
                module_id: module_id.clone(),
                message: format!("migrate failed: {error}"),
            }
        })?;
    }

    let mut applied_seeds = 0usize;
    if options.seed_on_boot {
        applied_seeds = orchestrator
            .seed(&options.seed_locale, &options.seed_profile)
            .await
            .map_err(|error| GatewayRuntimeError::EmbeddedDatabaseBootstrap {
                workspace: workspace.to_owned(),
                module_id: module_id.clone(),
                message: format!("seed failed: {error}"),
            })?;
    }

    tracing::info!(
        target: "sdkwork.api.cloud_gateway.database",
        workspace,
        module_id = %module_id,
        applied_migrations,
        applied_seeds,
        auto_migrate = options.auto_migrate || force_migrate,
        "embedded database module bootstrap completed"
    );
    Ok(())
}

/// Maps enabled dependency surfaces to their application roots via
/// `SDKWORK_<MODULE>_APP_ROOT` (workspace `sdkwork-course` →
/// `SDKWORK_COURSE_APP_ROOT`). Surfaces without the env var are skipped:
/// standalone deployments own their roots through the module repositories.
fn enabled_surface_app_roots(config: &GatewayRuntimeConfig) -> Vec<(String, PathBuf)> {
    let mut surfaces = Vec::new();
    for surface in &config.dependency_surfaces {
        let Some(env_key) = surface_app_root_env_key(&surface.workspace) else {
            continue;
        };
        let Some(value) = std::env::var_os(&env_key) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        surfaces.push((surface.workspace.clone(), PathBuf::from(value)));
    }
    surfaces.sort();
    surfaces.dedup_by(|a, b| a.1 == b.1);
    surfaces
}

/// Resolves the documents application root for the developer catalog.
///
/// Packaged installs place module assets under `database-modules/`; source
/// checkouts keep each module in a sibling directory. The explicit
/// `SDKWORK_DOCUMENTS_APP_ROOT` always wins.
fn resolve_developer_catalog_app_root() -> Option<PathBuf> {
    if let Some(env_key) = surface_app_root_env_key(DEVELOPER_CATALOG_WORKSPACE) {
        if let Some(value) = std::env::var_os(&env_key) {
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }

    let mut candidates = Vec::new();
    for root in database_module_roots() {
        candidates.push(root.join(DEVELOPER_CATALOG_WORKSPACE));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(DEVELOPER_CATALOG_WORKSPACE));
        if let Some(parent) = current_dir.parent() {
            candidates.push(parent.join(DEVELOPER_CATALOG_WORKSPACE));
        }
    }
    resolve_surface_app_root_from_candidates(candidates)
}

/// Candidate module roots, in priority order: the packaged asset root first,
/// then the install root, then the executable directory and the working
/// directory. `resolve_surface_app_root_from_candidates` keeps only roots that
/// actually ship `database/database.manifest.json`.
fn database_module_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(configured) = std::env::var_os("SDKWORK_DATABASE_MODULES_ROOT") {
        if !configured.is_empty() {
            roots.push(PathBuf::from(configured));
        }
    }
    if let Some(app_root) = std::env::var_os("SDKWORK_APP_ROOT") {
        if !app_root.is_empty() {
            roots.push(PathBuf::from(app_root).join("database-modules"));
        }
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(binary_dir) = executable.parent() {
            roots.push(binary_dir.join("database-modules"));
            if let Some(install_root) = binary_dir.parent() {
                roots.push(install_root.join("database-modules"));
            }
        }
    }
    if let Ok(current_dir) = std::env::current_dir() {
        roots.push(current_dir.join("database-modules"));
        if let Some(parent) = current_dir.parent() {
            roots.push(parent.join("database-modules"));
        }
    }
    roots
}

fn resolve_surface_app_root_from_candidates(
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    use sdkwork_database_lifecycle::app_root_has_database_assets;

    let mut seen = std::collections::HashSet::new();
    for candidate in candidates {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        if app_root_has_database_assets(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn surface_app_root_env_key(workspace: &str) -> Option<String> {
    let module = workspace.strip_prefix("sdkwork-")?;
    if module.is_empty() || !module.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
        return None;
    }
    Some(format!(
        "SDKWORK_{}_APP_ROOT",
        module.to_uppercase().replace('-', "_")
    ))
}

/// Runs the explicit installer lifecycle through the same owner assembly
/// entrypoints used by standalone and cloud runtime composition.
pub async fn migrate_embedded_databases(
    config: &GatewayRuntimeConfig,
) -> Result<Option<DatabasePool>, GatewayRuntimeError> {
    let Some(pool) = bootstrap_embedded_databases(config).await? else {
        return Ok(None);
    };

    // Explicit migrate runs always migrate every discovered module,
    // regardless of SDKWORK_DATABASE_AUTO_MIGRATE: this mode IS the explicit
    // migrate command (container entrypoint migrate-on-start, installer runs).
    discover_and_bootstrap_embedded_database_modules(config, &pool, true).await?;

    let bootstrap = crate::embedded_dependency_routes::bootstrap_embedded_dependency_contributions(
        config, &pool,
    )
    .await?;
    tracing::info!(
        module_count = bootstrap.modules.len(),
        "owner API assembly database lifecycle completed"
    );
    drop(bootstrap);

    Ok(Some(pool))
}

async fn process_database_pool() -> Result<DatabasePool, GatewayRuntimeError> {
    if let Some(pool) = process_shared_database_pool() {
        return Ok(pool);
    }

    create_process_shared_pool_from_env(PROCESS_DATABASE_SERVICE_CODE)
        .await
        .map_err(|error| GatewayRuntimeError::DatabasePoolBootstrap {
            message: error.to_string(),
        })?
        .ok_or_else(|| GatewayRuntimeError::DatabasePoolBootstrap {
            message: "missing SDKWORK_DATABASE_* configuration".to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::{resolve_surface_app_root_from_candidates, surface_app_root_env_key};

    #[test]
    fn workspace_to_app_root_env_key_uses_upper_snake() {
        assert_eq!(
            surface_app_root_env_key("sdkwork-course").as_deref(),
            Some("SDKWORK_COURSE_APP_ROOT")
        );
        assert_eq!(
            surface_app_root_env_key("sdkwork-local-router").as_deref(),
            Some("SDKWORK_LOCAL_ROUTER_APP_ROOT")
        );
    }

    #[test]
    fn workspace_to_app_root_env_key_rejects_non_module_names() {
        assert_eq!(surface_app_root_env_key("sdkwork-"), None);
        assert_eq!(surface_app_root_env_key("gateway-core"), None);
        assert_eq!(surface_app_root_env_key("sdkwork-Course"), None);
    }

    #[test]
    fn component_spec_declares_the_documents_database_surface() {
        let component_spec = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../specs/component.spec.json");
        let config =
            sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig::from_component_spec_path(
                component_spec,
            )
            .expect("gateway component spec must parse");
        assert!(
            config
                .dependency_surfaces
                .iter()
                .any(|surface| surface.workspace == "sdkwork-documents"),
            "the documents dependency surface must be selected so gateway startup bootstraps its catalog"
        );
    }

    #[test]
    fn resolves_the_first_candidate_that_ships_database_assets() {
        let base = std::env::temp_dir().join("sdkwork-gateway-db-root-resolution");
        let _ = std::fs::remove_dir_all(&base);
        let packaged = base.join("packaged").join("sdkwork-documents");
        std::fs::create_dir_all(packaged.join("database")).expect("create database dir");
        std::fs::write(
            packaged.join("database").join("database.manifest.json"),
            "{}",
        )
        .expect("write manifest");
        let empty = base.join("empty").join("sdkwork-documents");
        std::fs::create_dir_all(&empty).expect("create empty dir");

        let resolved =
            resolve_surface_app_root_from_candidates(vec![empty.clone(), packaged.clone()]);

        assert_eq!(resolved, Some(packaged));
        let _ = std::fs::remove_dir_all(&base);
    }
}
