use std::path::PathBuf;

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_lifecycle::{
    discover_database_modules, lifecycle_options_from_env, LifecycleOrchestrator,
};
use sdkwork_database_spi::DatabaseManifest;
use sdkwork_database_sqlx::{
    create_process_shared_pool_from_env, process_shared_database_pool, DatabasePool,
};

use crate::GatewayRuntimeError;

const PROCESS_DATABASE_SERVICE_CODE: &str = "CLOUD";
const DATABASE_BOOTSTRAP_APPLIED_BY: &str = "sdkwork-api-cloud-gateway";

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
    let surfaces = enabled_surface_app_roots(config);
    if surfaces.is_empty() {
        return Ok(());
    }
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

    for (discovered, module) in report.registered.iter().zip(registry.modules().iter()) {
        let Some(workspace) = workspace_by_root.get(&discovered.app_root) else {
            continue;
        };
        bootstrap_discovered_module(workspace, module.clone(), pool, force_migrate).await?;
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
/// `SDKWORK_COURSE_APP_ROOT`). When that explicit variable is absent, packaged
/// installs resolve the module from `SDKWORK_DATABASE_MODULES_ROOT`, the
/// install root's `database-modules/` directory, or a sibling checkout. This is
/// what lets a packaged gateway initialize every embedded module from the
/// assets it ships, instead of silently skipping them when only
/// `SDKWORK_DATABASE_MODULES_ROOT` is set.
fn enabled_surface_app_roots(config: &GatewayRuntimeConfig) -> Vec<(String, PathBuf)> {
    let mut surfaces = Vec::new();
    for surface in &config.dependency_surfaces {
        let Some(root) = resolve_surface_app_root(&surface.workspace) else {
            continue;
        };
        surfaces.push((surface.workspace.clone(), root));
    }
    surfaces.sort();
    surfaces.dedup_by(|a, b| a.1 == b.1);
    surfaces
}

fn resolve_surface_app_root(workspace: &str) -> Option<PathBuf> {
    if let Some(env_key) = surface_app_root_env_key(workspace) {
        if let Some(value) = std::env::var_os(&env_key) {
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }

    let mut candidates = Vec::new();
    for root in database_module_roots() {
        candidates.push(root.join(workspace));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(workspace));
        if let Some(parent) = current_dir.parent() {
            candidates.push(parent.join(workspace));
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
