use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_sqlx::{
    create_process_shared_pool_from_env, process_shared_database_pool, DatabasePool,
};

use crate::GatewayRuntimeError;

const PROCESS_DATABASE_SERVICE_CODE: &str = "CLOUD";

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

/// Runs the explicit installer lifecycle through the same owner assembly
/// entrypoints used by standalone and cloud runtime composition.
pub async fn migrate_embedded_databases(
    config: &GatewayRuntimeConfig,
) -> Result<Option<DatabasePool>, GatewayRuntimeError> {
    let Some(pool) = bootstrap_embedded_databases(config).await? else {
        return Ok(None);
    };

    let bootstrap = crate::embedded_dependency_routes::bootstrap_embedded_dependency_contributions(
        config, &pool,
    )
    .await?;
    tracing::info!(
        assembly_count = bootstrap.contributions.len(),
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
