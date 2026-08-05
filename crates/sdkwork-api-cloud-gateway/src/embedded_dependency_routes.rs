//! Host-neutral owner API contributions for the platform cloud gateway.

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_web_bootstrap::ApiAssemblyContribution;

use crate::GatewayRuntimeError;

pub struct EmbeddedDependencyBootstrap {
    pub contributions: Vec<ApiAssemblyContribution>,
    pub runtime: EmbeddedDependencyRuntime,
}

#[derive(Default)]
pub struct EmbeddedDependencyRuntime {
    background_tasks: Vec<tokio::task::JoinHandle<()>>,
    #[cfg(feature = "foundation-im")]
    im_runtime: Option<sdkwork_api_im_assembly::ApiAssemblyRuntime>,
}

impl EmbeddedDependencyRuntime {
    #[cfg(feature = "foundation-image")]
    fn retain_background_task(&mut self, task: Option<tokio::task::JoinHandle<()>>) {
        if let Some(task) = task {
            self.background_tasks.push(task);
        }
    }

    #[cfg(feature = "foundation-im")]
    fn retain_im_runtime(&mut self, runtime: sdkwork_api_im_assembly::ApiAssemblyRuntime) {
        self.im_runtime = Some(runtime);
    }
}

impl Drop for EmbeddedDependencyRuntime {
    fn drop(&mut self) {
        for task in &self.background_tasks {
            task.abort();
        }
    }
}

pub async fn bootstrap_embedded_dependency_contributions(
    config: &GatewayRuntimeConfig,
    pool: &DatabasePool,
) -> Result<EmbeddedDependencyBootstrap, GatewayRuntimeError> {
    #[allow(unused_mut)]
    let mut contributions = Vec::new();
    #[allow(unused_mut)]
    let mut runtime = EmbeddedDependencyRuntime::default();

    #[cfg(feature = "foundation-appbase")]
    if workspace_surface_enabled(config, "sdkwork-iam") {
        let contribution =
            sdkwork_api_iam_assembly::assemble_owner_api_surfaces_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-iam", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-order")]
    if workspace_surface_enabled(config, "sdkwork-order") {
        let contribution = sdkwork_api_order_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-order", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-membership")]
    if workspace_surface_enabled(config, "sdkwork-membership") {
        let contribution =
            sdkwork_api_membership_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-membership", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-skills")]
    if workspace_surface_enabled(config, "sdkwork-skills") {
        let contribution = sdkwork_api_skills_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-skills", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-prompts")]
    if workspace_surface_enabled(config, "sdkwork-prompts") {
        let contribution =
            sdkwork_api_prompts_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-prompts", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-documents")]
    if workspace_surface_enabled(config, "sdkwork-documents") {
        let contribution =
            sdkwork_api_documents_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-documents", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-course")]
    if workspace_surface_enabled(config, "sdkwork-course") {
        let contribution = sdkwork_api_course_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-course", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-image")]
    if workspace_surface_enabled(config, "sdkwork-image") {
        let assembly = sdkwork_api_image_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-image", message))?;
        contributions.push(assembly.contribution);
        runtime.retain_background_task(assembly.background_processor);
    }

    #[cfg(feature = "foundation-community")]
    if workspace_surface_enabled(config, "sdkwork-community") {
        let contribution =
            sdkwork_api_community_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-community", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-dezhou")]
    if workspace_surface_enabled(config, "sdkwork-dezhou") {
        let contribution = sdkwork_api_dezhou_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-dezhou", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-doudizhu")]
    if workspace_surface_enabled(config, "sdkwork-doudizhu") {
        let contribution =
            sdkwork_api_doudizhu_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-doudizhu", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-mahjong")]
    if workspace_surface_enabled(config, "sdkwork-mahjong") {
        let contribution =
            sdkwork_api_mahjong_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-mahjong", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-xiangqi")]
    if workspace_surface_enabled(config, "sdkwork-xiangqi") {
        let contribution =
            sdkwork_api_xiangqi_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-xiangqi", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-notary")]
    if workspace_surface_enabled(config, "sdkwork-notary") {
        let contribution = sdkwork_api_notary_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-notary", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-news")]
    if workspace_surface_enabled(config, "sdkwork-news") {
        let contribution = sdkwork_api_news_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-news", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-drive")]
    if workspace_surface_enabled(config, "sdkwork-drive") {
        let postgres_pool = pool
            .as_postgres()
            .ok_or_else(|| {
                workspace_build_error(
                    config,
                    "sdkwork-drive",
                    "drive authoritative server requires a PostgreSQL pool".to_owned(),
                )
            })?
            .clone();
        let contribution = sdkwork_api_drive_assembly::assemble_api_router(postgres_pool)
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-drive", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-payment")]
    if workspace_surface_enabled(config, "sdkwork-payment") {
        let contribution =
            sdkwork_api_payment_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-payment", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-im")]
    if workspace_surface_enabled(config, "sdkwork-im") {
        // IM owns process-wide shared database pools (sqlx host + r2d2) installed
        // exactly once per process; the assembly reuses that singleton.
        let contribution = sdkwork_api_im_assembly::assemble_api_router()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
        contributions.push(contribution.contribution);
        runtime.retain_im_runtime(contribution.runtime);
    }

    #[cfg(feature = "foundation-deployments")]
    if workspace_surface_enabled(config, "sdkwork-deployments") {
        let assembly =
            sdkwork_api_deployments_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-deployments", message))?;
        contributions.push(assembly.contribution);
    }

    #[cfg(feature = "foundation-cloudrouter")]
    if workspace_surface_enabled(config, "sdkwork-cloudrouter") {
        let assembly = sdkwork_api_cloudrouter_assembly::assemble_api_router(
            sdkwork_api_cloudrouter_assembly::ApiAssemblyContext::cloud_gateway(),
        )
        .await
        .map_err(|error| {
            workspace_build_error(config, "sdkwork-cloudrouter", format!("{error:?}"))
        })?;
        contributions.push(ApiAssemblyContribution {
            owner: "sdkwork-cloudrouter",
            router: assembly.router,
            route_manifest: assembly.route_manifest,
            openapi: assembly.openapi,
            permission_catalog: assembly.permission_catalog,
            domain_context_injectors: assembly.domain_context_injectors,
            readiness_check: assembly.readiness_check,
        });
    }

    #[cfg(feature = "foundation-inventory")]
    if workspace_surface_enabled(config, "sdkwork-inventory") {
        let contribution =
            sdkwork_api_inventory_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-inventory", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-invoice")]
    if workspace_surface_enabled(config, "sdkwork-invoice") {
        let contribution =
            sdkwork_api_invoice_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-invoice", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-portal")]
    if workspace_surface_enabled(config, "sdkwork-portal") {
        let contribution = sdkwork_api_portal_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-portal", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-shop")]
    if workspace_surface_enabled(config, "sdkwork-shop") {
        let contribution = sdkwork_api_shop_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-shop", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-voice")]
    if workspace_surface_enabled(config, "sdkwork-voice") {
        let contribution =
            sdkwork_api_voice_assembly::assemble_contribution_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-voice", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-appstore")]
    if workspace_surface_enabled(config, "sdkwork-appstore") {
        let contribution =
            sdkwork_api_appstore_assembly::assemble_contribution_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-appstore", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-web-server")]
    if workspace_surface_enabled(config, "sdkwork-webserver") {
        let assembly = sdkwork_api_webserver_assembly::assemble_api_router(
            sdkwork_api_webserver_assembly::ApiAssemblyContext::cloud_gateway(),
        )
        .await
        .map_err(|message| {
            workspace_build_error(config, "sdkwork-webserver", message.to_string())
        })?;
        contributions.push(ApiAssemblyContribution {
            owner: "sdkwork-webserver",
            router: assembly.router,
            route_manifest: assembly.route_manifest,
            openapi: assembly.openapi,
            permission_catalog: assembly.permission_catalog,
            domain_context_injectors: assembly.domain_context_injectors,
            readiness_check: assembly.readiness_check,
        });
    }

    Ok(EmbeddedDependencyBootstrap {
        contributions,
        runtime,
    })
}

fn workspace_surface_enabled(config: &GatewayRuntimeConfig, workspace: &str) -> bool {
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.workspace == workspace)
}

fn workspace_build_error(
    config: &GatewayRuntimeConfig,
    workspace: &str,
    message: String,
) -> GatewayRuntimeError {
    let service_id = config
        .dependency_surfaces
        .iter()
        .find(|surface| surface.workspace == workspace)
        .map(|surface| surface.service_id.clone())
        .unwrap_or_else(|| workspace.to_owned());
    GatewayRuntimeError::EmbeddedRouterBuild {
        service_id,
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;

    #[cfg(feature = "foundation-community")]
    use super::bootstrap_embedded_dependency_contributions;
    use super::workspace_surface_enabled;

    #[test]
    fn contribution_selection_uses_declared_workspace_ownership() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = GatewayRuntimeConfig::from_application_root(root)
            .expect("gateway application manifest parses");

        assert!(workspace_surface_enabled(&config, "sdkwork-iam"));
        assert!(workspace_surface_enabled(&config, "sdkwork-order"));
        assert!(workspace_surface_enabled(&config, "sdkwork-membership"));
        assert!(workspace_surface_enabled(&config, "sdkwork-skills"));
        assert!(workspace_surface_enabled(&config, "sdkwork-prompts"));
        assert!(workspace_surface_enabled(&config, "sdkwork-documents"));
        assert!(workspace_surface_enabled(&config, "sdkwork-course"));
        assert!(workspace_surface_enabled(&config, "sdkwork-image"));
        assert!(workspace_surface_enabled(&config, "sdkwork-community"));
        assert!(workspace_surface_enabled(&config, "sdkwork-dezhou"));
        assert!(workspace_surface_enabled(&config, "sdkwork-doudizhu"));
        assert!(workspace_surface_enabled(&config, "sdkwork-mahjong"));
        assert!(workspace_surface_enabled(&config, "sdkwork-xiangqi"));
        assert!(workspace_surface_enabled(&config, "sdkwork-notary"));
        assert!(workspace_surface_enabled(&config, "sdkwork-news"));
        assert!(workspace_surface_enabled(&config, "sdkwork-drive"));
        assert!(workspace_surface_enabled(&config, "sdkwork-payment"));
        assert!(workspace_surface_enabled(&config, "sdkwork-im"));
        assert!(workspace_surface_enabled(&config, "sdkwork-deployments"));
        assert!(workspace_surface_enabled(&config, "sdkwork-cloudrouter"));
        assert!(workspace_surface_enabled(&config, "sdkwork-inventory"));
        assert!(workspace_surface_enabled(&config, "sdkwork-invoice"));
        assert!(workspace_surface_enabled(&config, "sdkwork-portal"));
        assert!(workspace_surface_enabled(&config, "sdkwork-shop"));
        assert!(workspace_surface_enabled(&config, "sdkwork-voice"));
        assert!(workspace_surface_enabled(&config, "sdkwork-appstore"));
        assert!(!workspace_surface_enabled(&config, "unknown-workspace"));
    }

    #[cfg(feature = "foundation-community")]
    #[tokio::test]
    #[ignore = "requires an explicit, initialized PostgreSQL integration profile"]
    async fn community_contribution_uses_the_process_shared_pool() {
        std::env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "false");
        std::env::set_var("SDKWORK_DATABASE_SEED_ON_BOOT", "false");

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut config = GatewayRuntimeConfig::from_application_root(root)
            .expect("gateway application manifest parses");
        config.dependency_surfaces.retain(|surface| {
            matches!(
                surface.workspace.as_str(),
                "sdkwork-iam" | "sdkwork-community"
            )
        });

        let pool = crate::embedded_database_bootstrap::bootstrap_embedded_databases(&config)
            .await
            .expect("bootstrap process database pool")
            .expect("embedded profile requires PostgreSQL");
        let bootstrap = bootstrap_embedded_dependency_contributions(&config, &pool)
            .await
            .expect("assemble embedded IAM and Community contributions");
        let community = bootstrap
            .contributions
            .iter()
            .find(|contribution| contribution.owner == "sdkwork-community")
            .expect("Community contribution is selected");

        assert_eq!(community.route_manifest.routes().len(), 26);
        community
            .readiness_check
            .check()
            .await
            .expect("Community contribution uses the healthy process pool");

        drop(bootstrap);
        pool.close().await;
    }
}
