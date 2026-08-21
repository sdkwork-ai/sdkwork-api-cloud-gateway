//! Host-neutral owner API contributions for the platform cloud gateway.

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_web_bootstrap::ApiAssemblyContribution;
#[cfg(any(feature = "foundation-cloudrouter", feature = "foundation-webserver"))]
use sdkwork_web_contract::enrich_owned_openapi_document;

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
    #[cfg(feature = "foundation-im-realtime")]
    realtime_plane: Option<session_gateway::GatewayEmbeddedRealtimePlane>,
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

    #[cfg(feature = "foundation-im-realtime")]
    fn retain_realtime_plane(&mut self, plane: session_gateway::GatewayEmbeddedRealtimePlane) {
        self.realtime_plane = Some(plane);
    }

    /// Extracts the embedded realtime plane so a gateway main can drain it
    /// after the HTTP listener's graceful shutdown instead of aborting tasks.
    #[cfg(feature = "foundation-im-realtime")]
    pub fn take_realtime_plane(&mut self) -> Option<session_gateway::GatewayEmbeddedRealtimePlane> {
        self.realtime_plane.take()
    }
}

impl Drop for EmbeddedDependencyRuntime {
    fn drop(&mut self) {
        for task in &self.background_tasks {
            task.abort();
        }
        #[cfg(feature = "foundation-im-realtime")]
        if let Some(plane) = &self.realtime_plane {
            for handle in &plane.link_transport_handles {
                handle.abort();
            }
            if let Some(handle) = &plane.maintenance_handle {
                handle.abort();
            }
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

    #[cfg(feature = "foundation-assets")]
    if workspace_surface_enabled(config, "sdkwork-assets") {
        let contribution = sdkwork_api_assets_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-assets", message))?;
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

    #[cfg(all(feature = "foundation-im", not(feature = "foundation-im-realtime")))]
    if workspace_surface_enabled(config, "sdkwork-im") {
        // IM owns process-wide shared database pools (sqlx host + r2d2) installed
        // exactly once per process; the assembly reuses that singleton.
        let contribution = sdkwork_api_im_assembly::assemble_api_router()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
        contributions.push(contribution.contribution);
        runtime.retain_im_runtime(contribution.runtime);
    }

    #[cfg(all(feature = "foundation-im", feature = "foundation-im-realtime"))]
    if workspace_surface_enabled(config, "sdkwork-im") {
        // IM owns process-wide shared database pools (sqlx host + r2d2) installed
        // exactly once per process; the assembly reuses that singleton. When
        // realtime hosting is enabled (ADR-20260809-platform-gateway-realtime-hosting),
        // the session-gateway plane is embedded first so the assembly wires the
        // real plane (WebSocket upgrade + declared TCP/UDP/QUIC link binds)
        // instead of the in-memory default plane.
        let assembly = if config.realtime.enabled {
            let (assembly, plane) = bootstrap_im_realtime_contribution(config).await?;
            runtime.retain_realtime_plane(plane);
            assembly
        } else {
            sdkwork_api_im_assembly::assemble_api_router()
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?
        };
        contributions.push(assembly.contribution);
        runtime.retain_im_runtime(assembly.runtime);
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
        let openapi = enrich_owned_openapi_document(
            assembly.openapi,
            "sdkwork-cloudrouter",
            assembly.route_manifest.routes(),
        )
        .map_err(|error| {
            workspace_build_error(config, "sdkwork-cloudrouter", format!("{error:?}"))
        })?;
        contributions.push(ApiAssemblyContribution {
            owner: "sdkwork-cloudrouter",
            router: assembly.router,
            route_manifest: assembly.route_manifest,
            openapi,
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

    #[cfg(feature = "foundation-webserver")]
    if workspace_surface_enabled(config, "sdkwork-webserver") {
        let assembly = sdkwork_api_webserver_assembly::assemble_api_router(
            sdkwork_api_webserver_assembly::ApiAssemblyContext::cloud_gateway(),
        )
        .await
        .map_err(|message| {
            workspace_build_error(config, "sdkwork-webserver", message.to_string())
        })?;
        contributions.push(assembly.into_contribution());
    }

    #[cfg(feature = "foundation-rtc")]
    if workspace_surface_enabled(config, "sdkwork-rtc") {
        let contribution = sdkwork_api_rtc_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-rtc", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-mail")]
    if workspace_surface_enabled(config, "sdkwork-mail") {
        let contribution = sdkwork_api_mail_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-mail", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-mcp")]
    if workspace_surface_enabled(config, "sdkwork-mcp") {
        let contribution = sdkwork_api_mcp_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-mcp", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-browser")]
    if workspace_surface_enabled(config, "sdkwork-browser") {
        let contribution =
            sdkwork_api_browser_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-browser", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-llm")]
    if workspace_surface_enabled(config, "sdkwork-llm") {
        let contribution = sdkwork_api_llm_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-llm", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-agents")]
    if workspace_surface_enabled(config, "sdkwork-agents") {
        let contribution = sdkwork_api_agents_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-agents", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-generations")]
    if workspace_surface_enabled(config, "sdkwork-generations") {
        let contribution =
            sdkwork_api_generations_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-generations", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-aiot")]
    if workspace_surface_enabled(config, "sdkwork-aiot") {
        let contribution = sdkwork_api_aiot_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-aiot", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-promotion")]
    if workspace_surface_enabled(config, "sdkwork-promotion") {
        let contribution =
            sdkwork_api_promotion_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-promotion", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-knowledgebase")]
    if workspace_surface_enabled(config, "sdkwork-knowledgebase") {
        let knowledgebase_environment = std::env::var("SDKWORK_KNOWLEDGEBASE_ENVIRONMENT")
            .unwrap_or_else(|_| config.service.environment.clone());
        let contribution =
            sdkwork_api_knowledgebase_assembly::assemble_api_router_with_pool_for_environment(
                pool.clone(),
                &knowledgebase_environment,
            )
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-knowledgebase", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-customerservice")]
    if workspace_surface_enabled(config, "sdkwork-customerservice") {
        let contribution =
            sdkwork_api_customerservice_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| {
                    workspace_build_error(config, "sdkwork-customerservice", message)
                })?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-manager")]
    if workspace_surface_enabled(config, "sdkwork-manager") {
        let contribution =
            sdkwork_api_manager_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-manager", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-cms")]
    if workspace_surface_enabled(config, "sdkwork-cms") {
        let contribution = sdkwork_api_cms_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-cms", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-forum")]
    if workspace_surface_enabled(config, "sdkwork-forum") {
        let assembly = sdkwork_api_forum_assembly::assemble_api_router_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-forum", message))?;
        contributions.push(assembly.contribution);
    }

    #[cfg(feature = "foundation-modelkit")]
    if workspace_surface_enabled(config, "sdkwork-modelkit") {
        let contribution =
            sdkwork_api_modelkit_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-modelkit", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-account")]
    if workspace_surface_enabled(config, "sdkwork-account") {
        let contribution =
            sdkwork_api_account_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-account", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-catalog")]
    if workspace_surface_enabled(config, "sdkwork-catalog") {
        let contribution =
            sdkwork_api_catalog_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-catalog", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-gameengine")]
    if workspace_surface_enabled(config, "sdkwork-gameengine") {
        let contribution =
            sdkwork_api_gameengine_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-gameengine", message))?;
        contributions.push(contribution);
    }

    #[cfg(feature = "foundation-local-router")]
    if workspace_surface_enabled(config, "sdkwork-local-router") {
        // The bundle carries the owner runtime handle; its background tasks are
        // detached process-lifetime workers, so the gateway mounts only the
        // contribution and never binds the local-router listener.
        let assembly =
            sdkwork_api_local_router_assembly::assemble_api_router_with_pool(pool.clone())
                .await
                .map_err(|message| {
                    workspace_build_error(config, "sdkwork-local-router", message)
                })?;
        contributions.push(assembly.contribution);
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

/// Bootstraps the embedded IM realtime plane and the IM API assembly wired to
/// that plane (`ADR-20260809-platform-gateway-realtime-hosting`).
///
/// Order matters: the plane resolves membership-gated scope access from the
/// IM process-shared pools, so those pools must be installed first (idempotent;
/// the assembly reuses the same singleton pools).
#[cfg(feature = "foundation-im-realtime")]
async fn bootstrap_im_realtime_contribution(
    config: &GatewayRuntimeConfig,
) -> Result<
    (
        sdkwork_api_im_assembly::ApiAssembly,
        session_gateway::GatewayEmbeddedRealtimePlane,
    ),
    GatewayRuntimeError,
> {
    sdkwork_im_database_pool::bootstrap_im_process_database_pools_from_env()
        .await
        .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
    let plane = session_gateway::bootstrap_gateway_embedded_realtime_plane()
        .await
        .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
    let contribution = sdkwork_api_im_assembly::assemble_api_router_with_realtime_bootstrap(Some(
        &plane.bootstrap,
    ))
    .await
    .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
    Ok((contribution, plane))
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
        assert!(workspace_surface_enabled(&config, "sdkwork-assets"));
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
        assert!(workspace_surface_enabled(&config, "sdkwork-rtc"));
        assert!(workspace_surface_enabled(&config, "sdkwork-mail"));
        assert!(workspace_surface_enabled(&config, "sdkwork-mcp"));
        assert!(workspace_surface_enabled(&config, "sdkwork-browser"));
        assert!(workspace_surface_enabled(&config, "sdkwork-llm"));
        assert!(workspace_surface_enabled(&config, "sdkwork-agents"));
        assert!(workspace_surface_enabled(&config, "sdkwork-generations"));
        assert!(workspace_surface_enabled(&config, "sdkwork-aiot"));
        assert!(workspace_surface_enabled(&config, "sdkwork-promotion"));
        assert!(workspace_surface_enabled(&config, "sdkwork-knowledgebase"));
        assert!(workspace_surface_enabled(
            &config,
            "sdkwork-customerservice"
        ));
        assert!(workspace_surface_enabled(&config, "sdkwork-manager"));
        assert!(workspace_surface_enabled(&config, "sdkwork-cms"));
        assert!(workspace_surface_enabled(&config, "sdkwork-forum"));
        assert!(workspace_surface_enabled(&config, "sdkwork-modelkit"));
        assert!(workspace_surface_enabled(&config, "sdkwork-account"));
        assert!(workspace_surface_enabled(&config, "sdkwork-catalog"));
        assert!(workspace_surface_enabled(&config, "sdkwork-gameengine"));
        assert!(workspace_surface_enabled(&config, "sdkwork-local-router"));
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
