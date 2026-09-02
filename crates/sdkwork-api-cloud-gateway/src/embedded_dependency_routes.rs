//! Host-neutral owner API modules for the platform cloud gateway.
//!
//! Every embedded dependency is installed through its own `WebModule`
//! (API_ASSEMBLY_SPEC §4.1.1): the dependency owns its complete route,
//! manifest, and OpenAPI definition end to end, and the gateway only composes
//! the resulting modules through `ApiModuleRegistry`. No dependency is reached
//! over HTTP, so the composition never loops back to this gateway's listener.

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_web_bootstrap::WebModule;

use crate::GatewayRuntimeError;

pub struct EmbeddedDependencyBootstrap {
    /// Dependency-owned Web Modules (API_ASSEMBLY_SPEC §4.1.1). The platform
    /// gateway installs these through `ApiModuleRegistry::add_modules`;
    /// duplicate registrations are ignored so route composition stays free.
    pub modules: Vec<WebModule>,
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
    let mut modules: Vec<WebModule> = Vec::new();
    #[allow(unused_mut)]
    let mut runtime = EmbeddedDependencyRuntime::default();

    #[cfg(feature = "foundation-appbase")]
    if workspace_surface_enabled(config, "sdkwork-iam") {
        let module = sdkwork_api_iam_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-iam", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-order")]
    if workspace_surface_enabled(config, "sdkwork-order") {
        let module = sdkwork_api_order_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-order", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-membership")]
    if workspace_surface_enabled(config, "sdkwork-membership") {
        let module = sdkwork_api_membership_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-membership", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-skills")]
    if workspace_surface_enabled(config, "sdkwork-skills") {
        let module = sdkwork_api_skills_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-skills", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-prompts")]
    if workspace_surface_enabled(config, "sdkwork-prompts") {
        let module = sdkwork_api_prompts_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-prompts", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-documents")]
    if workspace_surface_enabled(config, "sdkwork-documents") {
        let module = sdkwork_api_documents_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-documents", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-course")]
    if workspace_surface_enabled(config, "sdkwork-course") {
        let module = sdkwork_api_course_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-course", message))?;
        modules.push(module);
    }

    // The capture below is only written when `foundation-image` compiles and
    // only read when both surfaces are enabled, so cfg-reduced builds can hit
    // unused_mut/unused_variables — suppress those on the declaration.
    #[cfg(feature = "foundation-cloudrouter")]
    #[allow(unused_mut, unused_variables)]
    let mut cloudrouter_open_surface: Option<axum::Router> = None;

    #[cfg(feature = "foundation-cloudrouter")]
    if workspace_surface_enabled(config, "sdkwork-cloudrouter") {
        // The module owns its OpenAPI enrichment now
        // (API_ASSEMBLY_SPEC §4.1.1): the gateway installs the dependency's
        // complete definition instead of restamping it per host.
        let module = sdkwork_api_cloudrouter_assembly::web_module_with_context(
            sdkwork_api_cloudrouter_assembly::ApiAssemblyContext::cloud_gateway(),
        )
        .await
        .map_err(|message| workspace_build_error(config, "sdkwork-cloudrouter", message))?;
        // APPLICATION_GATEWAY_SPEC §2.3: the image generation module is also
        // embedded in this process, so it must reach CloudRouter through the
        // in-process dispatch adapter (built below) instead of an HTTP base
        // URL that would loop back into a listener. The contribution router is
        // CloudRouter's surface dispatcher: a dispatched open-api request runs
        // the full edge-runtime gateway pipeline (API-key authentication and
        // billing) exactly as an external HTTP request would.
        #[cfg(feature = "foundation-image")]
        if workspace_surface_enabled(config, "sdkwork-image") {
            cloudrouter_open_surface = module
                .contributions()
                .first()
                .map(|contribution| contribution.router.clone());
        }
        modules.push(module);
    }

    #[cfg(feature = "foundation-image")]
    if workspace_surface_enabled(config, "sdkwork-image") {
        // APPLICATION_GATEWAY_SPEC §2.3: when CloudRouter is composed
        // in-process, inject the embedded dispatch adapter so generation never
        // issues an HTTP request back into a listener. Without the embedded
        // CloudRouter surface (standalone CloudRouter deployments) the factory
        // falls back to the env-driven HTTP provider.
        #[cfg(not(all(feature = "foundation-image", feature = "foundation-cloudrouter")))]
        let embedded_provider: Option<
            std::sync::Arc<dyn sdkwork_image_generation_provider_spi::ImageGenerationProvider>,
        > = None;
        #[cfg(all(feature = "foundation-image", feature = "foundation-cloudrouter"))]
        let embedded_provider: Option<
            std::sync::Arc<dyn sdkwork_image_generation_provider_spi::ImageGenerationProvider>,
        > = cloudrouter_open_surface
            .filter(|_| workspace_surface_enabled(config, "sdkwork-cloudrouter"))
            .map(|router| {
                std::sync::Arc::new(
                    sdkwork_image_generation_provider_adapter::CloudRouterEmbeddedImageGenerationAdapter::from_env(
                        router,
                    ),
                ) as std::sync::Arc<dyn sdkwork_image_generation_provider_spi::ImageGenerationProvider>
            });
        let (module, background_processor) =
            sdkwork_api_image_assembly::web_module_with_pool_retaining_background_with_provider(
                pool.clone(),
                embedded_provider,
            )
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-image", message))?;
        modules.push(module);
        runtime.retain_background_task(background_processor);
    }

    #[cfg(feature = "foundation-community")]
    if workspace_surface_enabled(config, "sdkwork-community") {
        let module = sdkwork_api_community_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-community", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-dezhou")]
    if workspace_surface_enabled(config, "sdkwork-dezhou") {
        let module = sdkwork_api_dezhou_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-dezhou", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-doudizhu")]
    if workspace_surface_enabled(config, "sdkwork-doudizhu") {
        let module = sdkwork_api_doudizhu_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-doudizhu", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-mahjong")]
    if workspace_surface_enabled(config, "sdkwork-mahjong") {
        let module = sdkwork_api_mahjong_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-mahjong", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-xiangqi")]
    if workspace_surface_enabled(config, "sdkwork-xiangqi") {
        let module = sdkwork_api_xiangqi_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-xiangqi", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-notary")]
    if workspace_surface_enabled(config, "sdkwork-notary") {
        let module = sdkwork_api_notary_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-notary", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-news")]
    if workspace_surface_enabled(config, "sdkwork-news") {
        let module = sdkwork_api_news_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-news", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-assets")]
    if workspace_surface_enabled(config, "sdkwork-assets") {
        let module = sdkwork_api_assets_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-assets", message))?;
        modules.push(module);
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
        let module = sdkwork_api_drive_assembly::web_module_with_postgres_pool(postgres_pool)
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-drive", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-payment")]
    if workspace_surface_enabled(config, "sdkwork-payment") {
        let module = sdkwork_api_payment_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-payment", message))?;
        modules.push(module);
    }

    #[cfg(all(feature = "foundation-im", not(feature = "foundation-im-realtime")))]
    if workspace_surface_enabled(config, "sdkwork-im") {
        // IM owns process-wide shared database pools (sqlx host + r2d2) installed
        // exactly once per process; the assembly reuses that singleton.
        let (module, im_runtime) =
            sdkwork_api_im_assembly::web_module_with_realtime_bootstrap(None)
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
        modules.push(module);
        runtime.retain_im_runtime(im_runtime);
    }

    #[cfg(all(feature = "foundation-im", feature = "foundation-im-realtime"))]
    if workspace_surface_enabled(config, "sdkwork-im") {
        // IM owns process-wide shared database pools (sqlx host + r2d2) installed
        // exactly once per process; the assembly reuses that singleton. When
        // realtime hosting is enabled (ADR-20260809-platform-gateway-realtime-hosting),
        // the session-gateway plane is embedded first so the assembly wires the
        // real plane (WebSocket upgrade + declared TCP/UDP/QUIC link binds)
        // instead of the in-memory default plane.
        let (module, im_runtime) = if config.realtime.enabled {
            let (module, im_runtime, plane) = bootstrap_im_realtime_contribution(config).await?;
            runtime.retain_realtime_plane(plane);
            (module, im_runtime)
        } else {
            sdkwork_api_im_assembly::web_module_with_realtime_bootstrap(None)
                .await
                .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?
        };
        modules.push(module);
        runtime.retain_im_runtime(im_runtime);
    }

    #[cfg(feature = "foundation-deployments")]
    if workspace_surface_enabled(config, "sdkwork-deployments") {
        let module = sdkwork_api_deployments_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-deployments", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-inventory")]
    if workspace_surface_enabled(config, "sdkwork-inventory") {
        let module = sdkwork_api_inventory_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-inventory", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-invoice")]
    if workspace_surface_enabled(config, "sdkwork-invoice") {
        let module = sdkwork_api_invoice_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-invoice", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-portal")]
    if workspace_surface_enabled(config, "sdkwork-portal") {
        let module = sdkwork_api_portal_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-portal", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-shop")]
    if workspace_surface_enabled(config, "sdkwork-shop") {
        let module = sdkwork_api_shop_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-shop", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-voice")]
    if workspace_surface_enabled(config, "sdkwork-voice") {
        let module = sdkwork_api_voice_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-voice", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-appstore")]
    if workspace_surface_enabled(config, "sdkwork-appstore") {
        let module = sdkwork_api_appstore_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-appstore", message))?;
        modules.push(module);
    }

    // Container runtime never boots sdkwork-webserver: reverse proxy is host
    // nginx (`deployments/webserver/` / `docker/nginx/`), and the Web Server
    // product runs as its own containers when needed.
    #[cfg(feature = "foundation-webserver")]
    if workspace_surface_enabled(config, "sdkwork-webserver")
        && !container_runtime_skips_webserver()
    {
        let module = sdkwork_api_webserver_assembly::web_module_with_context(
            sdkwork_api_webserver_assembly::ApiAssemblyContext::cloud_gateway(),
        )
        .await
        .map_err(|message| workspace_build_error(config, "sdkwork-webserver", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-rtc")]
    if workspace_surface_enabled(config, "sdkwork-rtc") {
        let module = sdkwork_api_rtc_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-rtc", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-mail")]
    if workspace_surface_enabled(config, "sdkwork-mail") {
        let module = sdkwork_api_mail_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-mail", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-mcp")]
    if workspace_surface_enabled(config, "sdkwork-mcp") {
        let module = sdkwork_api_mcp_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-mcp", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-browser")]
    if workspace_surface_enabled(config, "sdkwork-browser") {
        let module = sdkwork_api_browser_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-browser", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-llm")]
    if workspace_surface_enabled(config, "sdkwork-llm") {
        let module = sdkwork_api_llm_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-llm", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-agents")]
    if workspace_surface_enabled(config, "sdkwork-agents") {
        let module = sdkwork_api_agents_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-agents", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-generations")]
    if workspace_surface_enabled(config, "sdkwork-generations") {
        let module = sdkwork_api_generations_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-generations", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-aiot")]
    if workspace_surface_enabled(config, "sdkwork-aiot") {
        let module = sdkwork_api_aiot_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-aiot", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-promotion")]
    if workspace_surface_enabled(config, "sdkwork-promotion") {
        let module = sdkwork_api_promotion_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-promotion", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-knowledgebase")]
    if workspace_surface_enabled(config, "sdkwork-knowledgebase") {
        let knowledgebase_environment = std::env::var("SDKWORK_KNOWLEDGEBASE_ENVIRONMENT")
            .unwrap_or_else(|_| config.service.environment.clone());
        let module = sdkwork_api_knowledgebase_assembly::web_module_with_pool_for_environment(
            pool.clone(),
            &knowledgebase_environment,
        )
        .await
        .map_err(|message| workspace_build_error(config, "sdkwork-knowledgebase", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-customerservice")]
    if workspace_surface_enabled(config, "sdkwork-customerservice") {
        let module = sdkwork_api_customerservice_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-customerservice", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-manager")]
    if workspace_surface_enabled(config, "sdkwork-manager") {
        let module = sdkwork_api_manager_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-manager", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-cms")]
    if workspace_surface_enabled(config, "sdkwork-cms") {
        let module = sdkwork_api_cms_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-cms", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-forum")]
    if workspace_surface_enabled(config, "sdkwork-forum") {
        let module = sdkwork_api_forum_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-forum", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-modelkit")]
    if workspace_surface_enabled(config, "sdkwork-modelkit") {
        let module = sdkwork_api_modelkit_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-modelkit", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-account")]
    if workspace_surface_enabled(config, "sdkwork-account") {
        let module = sdkwork_api_account_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-account", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-catalog")]
    if workspace_surface_enabled(config, "sdkwork-catalog") {
        let module = sdkwork_api_catalog_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-catalog", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-gameengine")]
    if workspace_surface_enabled(config, "sdkwork-gameengine") {
        let module = sdkwork_api_gameengine_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-gameengine", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-local-router")]
    if workspace_surface_enabled(config, "sdkwork-local-router") {
        // The bundle carries the owner runtime handle; its background tasks are
        // detached process-lifetime workers, so the gateway installs only the
        // module and never binds the local-router listener.
        let module = sdkwork_api_local_router_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-local-router", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-appbase-data")]
    if workspace_surface_enabled(config, "sdkwork-appbase") {
        let module = sdkwork_api_appbase_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-appbase", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-birdcoder")]
    if workspace_surface_enabled(config, "sdkwork-birdcoder") {
        let module = sdkwork_api_birdcoder_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-birdcoder", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-canvas")]
    if workspace_surface_enabled(config, "sdkwork-canvas") {
        let module = sdkwork_api_canvas_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-canvas", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-company")]
    if workspace_surface_enabled(config, "sdkwork-company") {
        let module = sdkwork_api_company_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-company", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-feeds")]
    if workspace_surface_enabled(config, "sdkwork-feeds") {
        let module = sdkwork_api_feeds_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-feeds", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-github")]
    if workspace_surface_enabled(config, "sdkwork-github") {
        let module = sdkwork_api_github_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-github", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-log")]
    if workspace_surface_enabled(config, "sdkwork-log") {
        let module = sdkwork_api_log_assembly::web_module_with_pool(pool.clone())
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-log", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-messaging")]
    if workspace_surface_enabled(config, "sdkwork-messaging") {
        let module = sdkwork_api_messaging_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-messaging", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-notes")]
    if workspace_surface_enabled(config, "sdkwork-notes") {
        let module = sdkwork_api_notes_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-notes", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-search")]
    if workspace_surface_enabled(config, "sdkwork-search") {
        let module = sdkwork_api_search_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-search", message))?;
        modules.push(module);
    }

    #[cfg(feature = "foundation-settings")]
    if workspace_surface_enabled(config, "sdkwork-settings") {
        let module = sdkwork_api_settings_assembly::web_module()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-settings", message))?;
        modules.push(module);
    }

    Ok(EmbeddedDependencyBootstrap { modules, runtime })
}

fn workspace_surface_enabled(config: &GatewayRuntimeConfig, workspace: &str) -> bool {
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.workspace == workspace)
}

/// Only compiled when the webserver surface is: the sole call site sits behind
/// `#[cfg(feature = "foundation-webserver")]`, and container image profiles
/// deliberately omit that feature (container runtime never boots sdkwork-webserver).
#[cfg(feature = "foundation-webserver")]
fn container_runtime_skips_webserver() -> bool {
    match std::env::var("SDKWORK_API_CLOUD_GATEWAY_RUNTIME_TARGET") {
        Ok(value) => value.eq_ignore_ascii_case("container"),
        Err(_) => false,
    }
}

/// Bootstraps the embedded IM realtime plane and the IM Web Module wired to that
/// plane (`ADR-20260809-platform-gateway-realtime-hosting`).
///
/// Order matters: the plane resolves membership-gated scope access from the IM
/// process-shared pools, so those pools must be installed first (idempotent;
/// the module reuses the same singleton pools).
///
/// The returned IM runtime is handed back to the host because IM background
/// workers must be aborted on gateway shutdown.
#[cfg(feature = "foundation-im-realtime")]
async fn bootstrap_im_realtime_contribution(
    config: &GatewayRuntimeConfig,
) -> Result<
    (
        WebModule,
        sdkwork_api_im_assembly::ApiAssemblyRuntime,
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
    let (module, im_runtime) =
        sdkwork_api_im_assembly::web_module_with_realtime_bootstrap(Some(&plane.bootstrap))
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
    Ok((module, im_runtime, plane))
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
        assert!(workspace_surface_enabled(&config, "sdkwork-appbase"));
        assert!(workspace_surface_enabled(&config, "sdkwork-birdcoder"));
        assert!(workspace_surface_enabled(&config, "sdkwork-canvas"));
        assert!(workspace_surface_enabled(&config, "sdkwork-company"));
        assert!(workspace_surface_enabled(&config, "sdkwork-feeds"));
        assert!(workspace_surface_enabled(&config, "sdkwork-github"));
        assert!(workspace_surface_enabled(&config, "sdkwork-log"));
        assert!(workspace_surface_enabled(&config, "sdkwork-messaging"));
        assert!(workspace_surface_enabled(&config, "sdkwork-notes"));
        assert!(workspace_surface_enabled(&config, "sdkwork-search"));
        assert!(workspace_surface_enabled(&config, "sdkwork-settings"));
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
            .expect("assemble embedded IAM and Community web modules");
        let community = bootstrap
            .modules
            .iter()
            .find(|module| module.owner() == "sdkwork-community")
            .expect("Community module is selected");
        let contribution = community
            .contributions()
            .first()
            .expect("Community module carries its route contribution");

        assert_eq!(contribution.route_manifest.routes().len(), 26);
        contribution
            .readiness_check
            .check()
            .await
            .expect("Community module uses the healthy process pool");

        drop(bootstrap);
        pool.close().await;
    }
}
