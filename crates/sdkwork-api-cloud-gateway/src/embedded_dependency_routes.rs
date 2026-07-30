//! Embedded dependency routers for platform collapsed ingress (§5.7.2).
//! Each registered router is business-only; host routes own `/healthz`, `/livez`, `/readyz`.

use std::collections::HashMap;

use axum::Router;
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;

use crate::GatewayRuntimeError;

pub async fn bootstrap_embedded_dependency_routers(
    config: &GatewayRuntimeConfig,
) -> Result<HashMap<String, Router>, GatewayRuntimeError> {
    #[allow(unused_mut)]
    let mut routers = HashMap::new();
    let _ = config;

    #[cfg(feature = "foundation-appbase")]
    {
        use sdkwork_api_cloud_gateway_config::{
            APPBASE_APP_API_SERVICE_ID, APPBASE_BACKEND_API_SERVICE_ID, APPBASE_OPEN_API_SERVICE_ID,
        };

        if dependency_surface_enabled(config, APPBASE_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, APPBASE_BACKEND_API_SERVICE_ID)
            || dependency_surface_enabled(config, APPBASE_OPEN_API_SERVICE_ID)
        {
            let iam_router = bootstrap_embedded_iam_router(APPBASE_APP_API_SERVICE_ID).await?;
            if dependency_surface_enabled(config, APPBASE_APP_API_SERVICE_ID) {
                routers.insert(APPBASE_APP_API_SERVICE_ID.to_owned(), iam_router.clone());
            }
            if dependency_surface_enabled(config, APPBASE_BACKEND_API_SERVICE_ID) {
                routers.insert(
                    APPBASE_BACKEND_API_SERVICE_ID.to_owned(),
                    iam_router.clone(),
                );
            }
            if dependency_surface_enabled(config, APPBASE_OPEN_API_SERVICE_ID) {
                routers.insert(APPBASE_OPEN_API_SERVICE_ID.to_owned(), iam_router);
            }
        }
    }

    #[cfg(feature = "foundation-drive")]
    if workspace_surface_enabled(config, "sdkwork-drive") {
        let assembly = sdkwork_api_drive_assembly::assemble_business_routes_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-drive", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-drive", assembly.router);
    }

    #[cfg(feature = "foundation-membership")]
    if workspace_surface_enabled(config, "sdkwork-membership") {
        let assembly = sdkwork_api_membership_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-membership", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-membership", assembly.router);
    }

    #[cfg(feature = "foundation-order")]
    if workspace_surface_enabled(config, "sdkwork-order") {
        let assembly = sdkwork_api_order_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-order", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-order", assembly.router);
    }

    #[cfg(feature = "foundation-promotion")]
    if workspace_surface_enabled(config, "sdkwork-promotion") {
        let assembly = sdkwork_api_promotion_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-promotion", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-promotion", assembly.router);
    }

    #[cfg(feature = "foundation-payment")]
    if workspace_surface_enabled(config, "sdkwork-payment") {
        let assembly = sdkwork_api_payment_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-payment", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-payment", assembly.router);
    }

    #[cfg(feature = "foundation-course")]
    {
        use sdkwork_api_cloud_gateway_config::{
            COURSE_APP_API_SERVICE_ID, COURSE_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, COURSE_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, COURSE_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_course_assembly::assemble_api_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: COURSE_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let business_router = assembly.router;
            if dependency_surface_enabled(config, COURSE_APP_API_SERVICE_ID) {
                routers.insert(
                    COURSE_APP_API_SERVICE_ID.to_owned(),
                    business_router.clone(),
                );
            }
            if dependency_surface_enabled(config, COURSE_BACKEND_API_SERVICE_ID) {
                routers.insert(COURSE_BACKEND_API_SERVICE_ID.to_owned(), business_router);
            }
        }
    }

    #[cfg(feature = "foundation-voice")]
    {
        use sdkwork_api_cloud_gateway_config::{
            VOICE_APP_API_SERVICE_ID, VOICE_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, VOICE_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, VOICE_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_voice_assembly::assemble_api_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: VOICE_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let voice_router = assembly.router;
            if dependency_surface_enabled(config, VOICE_APP_API_SERVICE_ID) {
                routers.insert(VOICE_APP_API_SERVICE_ID.to_owned(), voice_router.clone());
            }
            if dependency_surface_enabled(config, VOICE_BACKEND_API_SERVICE_ID) {
                routers.insert(VOICE_BACKEND_API_SERVICE_ID.to_owned(), voice_router);
            }
        }
    }

    #[cfg(feature = "foundation-image")]
    {
        use sdkwork_api_cloud_gateway_config::IMAGE_APP_API_SERVICE_ID;
        if dependency_surface_enabled(config, IMAGE_APP_API_SERVICE_ID) {
            let assembly = sdkwork_api_image_assembly::assemble_api_router_from_env()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: IMAGE_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let image_router = assembly.router;
            if dependency_surface_enabled(config, IMAGE_APP_API_SERVICE_ID) {
                routers.insert(IMAGE_APP_API_SERVICE_ID.to_owned(), image_router);
            }
        }
    }

    #[cfg(feature = "foundation-cms")]
    {
        use sdkwork_api_cloud_gateway_config::{
            CMS_APP_API_SERVICE_ID, CMS_BACKEND_API_SERVICE_ID, CMS_OPEN_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, CMS_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, CMS_BACKEND_API_SERVICE_ID)
            || dependency_surface_enabled(config, CMS_OPEN_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_cms_assembly::assemble_api_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: CMS_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let cms_router = assembly.router;
            if dependency_surface_enabled(config, CMS_APP_API_SERVICE_ID) {
                routers.insert(CMS_APP_API_SERVICE_ID.to_owned(), cms_router.clone());
            }
            if dependency_surface_enabled(config, CMS_BACKEND_API_SERVICE_ID) {
                routers.insert(CMS_BACKEND_API_SERVICE_ID.to_owned(), cms_router.clone());
            }
            if dependency_surface_enabled(config, CMS_OPEN_API_SERVICE_ID) {
                routers.insert(CMS_OPEN_API_SERVICE_ID.to_owned(), cms_router);
            }
        }
    }

    #[cfg(feature = "foundation-forum")]
    {
        use sdkwork_api_cloud_gateway_config::{
            FORUM_APP_API_SERVICE_ID, FORUM_BACKEND_API_SERVICE_ID, FORUM_OPEN_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, FORUM_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, FORUM_BACKEND_API_SERVICE_ID)
            || dependency_surface_enabled(config, FORUM_OPEN_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_forum_assembly::assemble_api_router().await;
            let forum_router = assembly.router;
            if dependency_surface_enabled(config, FORUM_APP_API_SERVICE_ID) {
                routers.insert(FORUM_APP_API_SERVICE_ID.to_owned(), forum_router.clone());
            }
            if dependency_surface_enabled(config, FORUM_BACKEND_API_SERVICE_ID) {
                routers.insert(
                    FORUM_BACKEND_API_SERVICE_ID.to_owned(),
                    forum_router.clone(),
                );
            }
            if dependency_surface_enabled(config, FORUM_OPEN_API_SERVICE_ID) {
                routers.insert(FORUM_OPEN_API_SERVICE_ID.to_owned(), forum_router);
            }
        }
    }

    #[cfg(feature = "foundation-community")]
    {
        use sdkwork_api_cloud_gateway_config::{
            COMMUNITY_APP_API_SERVICE_ID, COMMUNITY_BACKEND_API_SERVICE_ID,
            COMMUNITY_OPEN_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, COMMUNITY_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, COMMUNITY_BACKEND_API_SERVICE_ID)
            || dependency_surface_enabled(config, COMMUNITY_OPEN_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_community_assembly::assemble_api_router();
            let community_router = assembly.router;
            if dependency_surface_enabled(config, COMMUNITY_APP_API_SERVICE_ID) {
                routers.insert(
                    COMMUNITY_APP_API_SERVICE_ID.to_owned(),
                    community_router.clone(),
                );
            }
            if dependency_surface_enabled(config, COMMUNITY_BACKEND_API_SERVICE_ID) {
                routers.insert(
                    COMMUNITY_BACKEND_API_SERVICE_ID.to_owned(),
                    community_router.clone(),
                );
            }
            if dependency_surface_enabled(config, COMMUNITY_OPEN_API_SERVICE_ID) {
                routers.insert(COMMUNITY_OPEN_API_SERVICE_ID.to_owned(), community_router);
            }
        }
    }

    #[cfg(feature = "foundation-clawrouter")]
    if workspace_surface_enabled(config, "sdkwork-clawrouter") {
        let assembly = sdkwork_api_clawrouter_assembly::assemble_api_router(
            sdkwork_api_clawrouter_assembly::ApiAssemblyContext::cloud_gateway(),
        )
            .await
            .map_err(|error| {
                workspace_build_error(config, "sdkwork-clawrouter", format!("{error:#}"))
            })?;
        insert_workspace_router(config, &mut routers, "sdkwork-clawrouter", assembly.router);
    }

    #[cfg(feature = "foundation-mcp")]
    {
        use sdkwork_api_cloud_gateway_config::{
            MCP_APP_API_SERVICE_ID, MCP_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, MCP_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, MCP_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_mcp_assembly::assemble_api_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: MCP_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let mcp_router = assembly.router;
            if dependency_surface_enabled(config, MCP_APP_API_SERVICE_ID) {
                routers.insert(MCP_APP_API_SERVICE_ID.to_owned(), mcp_router.clone());
            }
            if dependency_surface_enabled(config, MCP_BACKEND_API_SERVICE_ID) {
                routers.insert(MCP_BACKEND_API_SERVICE_ID.to_owned(), mcp_router);
            }
        }
    }

    #[cfg(feature = "foundation-im")]
    if workspace_surface_enabled(config, "sdkwork-im") {
        let assembly = sdkwork_api_im_assembly::assemble_api_router()
            .await
            .map_err(|message| workspace_build_error(config, "sdkwork-im", message))?;
        insert_workspace_router(config, &mut routers, "sdkwork-im", assembly.router);
    }

    #[cfg(feature = "foundation-rtc")]
    {
        use sdkwork_api_cloud_gateway_config::{
            RTC_APP_API_SERVICE_ID, RTC_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, RTC_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, RTC_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_rtc_assembly::assemble_api_router()
                .await
                .map_err(|error| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: RTC_APP_API_SERVICE_ID.to_owned(),
                    message: error.to_string(),
                })?;
            let rtc_router = assembly.router;
            if dependency_surface_enabled(config, RTC_APP_API_SERVICE_ID) {
                routers.insert(RTC_APP_API_SERVICE_ID.to_owned(), rtc_router.clone());
            }
            if dependency_surface_enabled(config, RTC_BACKEND_API_SERVICE_ID) {
                routers.insert(RTC_BACKEND_API_SERVICE_ID.to_owned(), rtc_router);
            }
        }
    }

    #[cfg(feature = "foundation-notary")]
    {
        use sdkwork_api_cloud_gateway_config::{
            NOTARY_APP_API_SERVICE_ID, NOTARY_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, NOTARY_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, NOTARY_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_notary_assembly::assemble_api_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: NOTARY_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let notary_router = assembly.router;
            if dependency_surface_enabled(config, NOTARY_APP_API_SERVICE_ID) {
                routers.insert(NOTARY_APP_API_SERVICE_ID.to_owned(), notary_router.clone());
            }
            if dependency_surface_enabled(config, NOTARY_BACKEND_API_SERVICE_ID) {
                routers.insert(NOTARY_BACKEND_API_SERVICE_ID.to_owned(), notary_router);
            }
        }
    }

    #[cfg(feature = "foundation-mail")]
    {
        use sdkwork_api_cloud_gateway_config::{
            MAIL_APP_API_SERVICE_ID, MAIL_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, MAIL_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, MAIL_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_mail_assembly::assemble_api_router()
                .await
                .map_err(|error| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: MAIL_APP_API_SERVICE_ID.to_owned(),
                    message: error.to_string(),
                })?;
            let mail_router = assembly.router;
            if dependency_surface_enabled(config, MAIL_APP_API_SERVICE_ID) {
                routers.insert(MAIL_APP_API_SERVICE_ID.to_owned(), mail_router.clone());
            }
            if dependency_surface_enabled(config, MAIL_BACKEND_API_SERVICE_ID) {
                routers.insert(MAIL_BACKEND_API_SERVICE_ID.to_owned(), mail_router);
            }
        }
    }

    #[cfg(feature = "foundation-aiot")]
    {
        use sdkwork_api_cloud_gateway_config::{
            AIOT_APP_API_SERVICE_ID, AIOT_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, AIOT_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, AIOT_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_aiot_assembly::assemble_api_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: AIOT_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let aiot_router = assembly.router;
            if dependency_surface_enabled(config, AIOT_APP_API_SERVICE_ID) {
                routers.insert(AIOT_APP_API_SERVICE_ID.to_owned(), aiot_router.clone());
            }
            if dependency_surface_enabled(config, AIOT_BACKEND_API_SERVICE_ID) {
                routers.insert(AIOT_BACKEND_API_SERVICE_ID.to_owned(), aiot_router);
            }
        }
    }

    #[cfg(feature = "foundation-appstore")]
    {
        use sdkwork_api_cloud_gateway_config::{
            SDKWORK_APPSTORE_APP_API_SERVICE_ID, SDKWORK_APPSTORE_BACKEND_API_SERVICE_ID,
            SDKWORK_APPSTORE_OPEN_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, SDKWORK_APPSTORE_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, SDKWORK_APPSTORE_BACKEND_API_SERVICE_ID)
            || dependency_surface_enabled(config, SDKWORK_APPSTORE_OPEN_API_SERVICE_ID)
        {
            let assembly = sdkwork_api_appstore_assembly::assemble_api_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: SDKWORK_APPSTORE_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let appstore_router = assembly.router;
            if dependency_surface_enabled(config, SDKWORK_APPSTORE_APP_API_SERVICE_ID) {
                routers.insert(
                    SDKWORK_APPSTORE_APP_API_SERVICE_ID.to_owned(),
                    appstore_router.clone(),
                );
            }
            if dependency_surface_enabled(config, SDKWORK_APPSTORE_BACKEND_API_SERVICE_ID) {
                routers.insert(
                    SDKWORK_APPSTORE_BACKEND_API_SERVICE_ID.to_owned(),
                    appstore_router.clone(),
                );
            }
            if dependency_surface_enabled(config, SDKWORK_APPSTORE_OPEN_API_SERVICE_ID) {
                routers.insert(
                    SDKWORK_APPSTORE_OPEN_API_SERVICE_ID.to_owned(),
                    appstore_router,
                );
            }
        }
    }

    #[cfg(feature = "foundation-agents")]
    if workspace_surface_enabled(config, "sdkwork-agents") {
        let assembly = sdkwork_api_agents_assembly::assemble_api_router()
            .await
            .map_err(|error| {
                workspace_build_error(config, "sdkwork-agents", format!("{error:#}"))
            })?;
        insert_workspace_router(config, &mut routers, "sdkwork-agents", assembly.router);
    }

    #[cfg(feature = "foundation-account")]
    if workspace_surface_enabled(config, "sdkwork-account") {
        let assembly = sdkwork_api_account_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-account", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-account", assembly.router);
    }

    #[cfg(feature = "foundation-catalog")]
    if workspace_surface_enabled(config, "sdkwork-catalog") {
        let assembly = sdkwork_api_catalog_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-catalog", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-catalog", assembly.router);
    }

    #[cfg(feature = "foundation-customerservice")]
    if workspace_surface_enabled(config, "sdkwork-customerservice") {
        let assembly = sdkwork_api_customerservice_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-customerservice", error))?;
        insert_workspace_router(
            config,
            &mut routers,
            "sdkwork-customerservice",
            assembly.router,
        );
    }

    #[cfg(feature = "foundation-gameengine")]
    if workspace_surface_enabled(config, "sdkwork-gameengine") {
        let assembly = sdkwork_api_gameengine_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-gameengine", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-gameengine", assembly.router);
    }

    #[cfg(feature = "foundation-inventory")]
    if workspace_surface_enabled(config, "sdkwork-inventory") {
        let assembly = sdkwork_api_inventory_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-inventory", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-inventory", assembly.router);
    }

    #[cfg(feature = "foundation-invoice")]
    if workspace_surface_enabled(config, "sdkwork-invoice") {
        let assembly = sdkwork_api_invoice_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-invoice", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-invoice", assembly.router);
    }

    #[cfg(feature = "foundation-local-router")]
    if workspace_surface_enabled(config, "sdkwork-local-router") {
        let assembly = sdkwork_api_local_router_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-local-router", error))?;
        insert_workspace_router(
            config,
            &mut routers,
            "sdkwork-local-router",
            assembly.router,
        );
    }

    #[cfg(feature = "foundation-manager")]
    if workspace_surface_enabled(config, "sdkwork-manager") {
        let assembly = sdkwork_api_manager_assembly::assemble_business_routes_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-manager", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-manager", assembly.router);
    }

    #[cfg(feature = "foundation-news")]
    if workspace_surface_enabled(config, "sdkwork-news") {
        let assembly = sdkwork_api_news_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-news", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-news", assembly.router);
    }

    #[cfg(feature = "foundation-portal")]
    if workspace_surface_enabled(config, "sdkwork-portal") {
        let assembly = sdkwork_api_portal_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-portal", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-portal", assembly.router);
    }

    #[cfg(feature = "foundation-shop")]
    if workspace_surface_enabled(config, "sdkwork-shop") {
        let assembly = sdkwork_api_shop_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-shop", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-shop", assembly.router);
    }

    #[cfg(feature = "foundation-skills")]
    if workspace_surface_enabled(config, "sdkwork-skills") {
        let assembly = sdkwork_api_skills_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-skills", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-skills", assembly.router);
    }

    #[cfg(feature = "foundation-deployments")]
    if workspace_surface_enabled(config, "sdkwork-deployments") {
        let assembly = sdkwork_api_deployments_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-deployments", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-deployments", assembly.router);
    }

    #[cfg(feature = "foundation-dezhou")]
    if workspace_surface_enabled(config, "sdkwork-dezhou") {
        let assembly = sdkwork_api_dezhou_assembly::assemble_business_routes()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-dezhou", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-dezhou", assembly.router);
    }

    #[cfg(feature = "foundation-documents")]
    if workspace_surface_enabled(config, "sdkwork-documents") {
        let assembly = sdkwork_api_documents_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-documents", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-documents", assembly.router);
    }

    #[cfg(feature = "foundation-modelkit")]
    if workspace_surface_enabled(config, "sdkwork-modelkit") {
        let assembly = sdkwork_api_modelkit_assembly::assemble_api_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-modelkit", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-modelkit", assembly.router);
    }

    #[cfg(feature = "foundation-prompts")]
    if workspace_surface_enabled(config, "sdkwork-prompts") {
        let assembly = sdkwork_api_prompts_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-prompts", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-prompts", assembly.router);
    }

    #[cfg(feature = "foundation-web-server")]
    if workspace_surface_enabled(config, "sdkwork-web-server") {
        let assembly = sdkwork_api_web_server_assembly::assemble_api_router(
            sdkwork_api_web_server_assembly::ApiAssemblyContext::cloud_gateway(),
        )
        .await
        .map_err(|error| workspace_build_error(config, "sdkwork-web-server", error.to_string()))?;
        insert_workspace_router(config, &mut routers, "sdkwork-web-server", assembly.router);
    }

    #[cfg(feature = "foundation-browser")]
    if workspace_surface_enabled(config, "sdkwork-browser") {
        let assembly = sdkwork_api_browser_assembly::assemble_api_router().map_err(|error| {
            workspace_build_error(config, "sdkwork-browser", format!("{error:#}"))
        })?;
        insert_workspace_router(config, &mut routers, "sdkwork-browser", assembly.router);
    }

    #[cfg(feature = "foundation-knowledgebase")]
    if workspace_surface_enabled(config, "sdkwork-knowledgebase") {
        let assembly =
            sdkwork_api_knowledgebase_assembly::assemble_business_routes_from_environment()
                .await
                .map_err(|error| {
                    workspace_build_error(config, "sdkwork-knowledgebase", format!("{error:#}"))
                })?;
        insert_workspace_router(
            config,
            &mut routers,
            "sdkwork-knowledgebase",
            assembly.router,
        );
    }

    #[cfg(feature = "foundation-llm")]
    if workspace_surface_enabled(config, "sdkwork-llm") {
        let assembly = sdkwork_api_llm_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-llm", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-llm", assembly.router);
    }

    #[cfg(feature = "foundation-mahjong")]
    if workspace_surface_enabled(config, "sdkwork-mahjong") {
        let assembly = sdkwork_api_mahjong_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-mahjong", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-mahjong", assembly.router);
    }

    #[cfg(feature = "foundation-doudizhu")]
    if workspace_surface_enabled(config, "sdkwork-doudizhu") {
        let assembly = sdkwork_api_doudizhu_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-doudizhu", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-doudizhu", assembly.router);
    }

    #[cfg(feature = "foundation-xiangqi")]
    if workspace_surface_enabled(config, "sdkwork-xiangqi") {
        let assembly = sdkwork_api_xiangqi_assembly::assemble_api_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-xiangqi", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-xiangqi", assembly.router);
    }

    Ok(routers)
}

#[cfg(feature = "foundation-appbase")]
pub(crate) async fn bootstrap_embedded_iam_router(
    service_id: &str,
) -> Result<Router, GatewayRuntimeError> {
    crate::runtime::ensure_embedded_iam_router_prerequisites(service_id).await?;
    let (assembly, host) = sdkwork_api_iam_assembly::bootstrap_iam_for_application()
        .await
        .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
            service_id: service_id.to_owned(),
            message,
        })?;
    let resolver = sdkwork_iam_web_adapter::IamWebRequestContextResolver::from_database_pool(Some(
        host.pool().clone(),
    ));
    Ok(
        sdkwork_iam_web_adapter::wrap_router_with_iam_owner_web_framework(
            assembly.router,
            resolver,
            assembly.route_manifest,
        ),
    )
}

#[allow(dead_code)]
fn workspace_surface_enabled(config: &GatewayRuntimeConfig, workspace: &str) -> bool {
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.workspace == workspace)
}

#[allow(dead_code)]
fn insert_workspace_router(
    config: &GatewayRuntimeConfig,
    routers: &mut HashMap<String, Router>,
    workspace: &str,
    router: Router,
) {
    for surface in config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.workspace == workspace)
    {
        routers.insert(surface.service_id.clone(), router.clone());
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn dependency_surface_enabled(config: &GatewayRuntimeConfig, service_id: &str) -> bool {
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.service_id == service_id)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };

    use axum::Router;
    use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;

    use super::insert_workspace_router;

    #[test]
    fn collapsed_workspace_router_registration_covers_every_manifest_surface() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = GatewayRuntimeConfig::from_application_root(root)
            .expect("gateway application manifest parses");
        let workspaces = [
            "sdkwork-membership",
            "sdkwork-order",
            "sdkwork-promotion",
            "sdkwork-payment",
            "sdkwork-agents",
            "sdkwork-account",
            "sdkwork-catalog",
            "sdkwork-customerservice",
            "sdkwork-gameengine",
            "sdkwork-inventory",
            "sdkwork-invoice",
            "sdkwork-local-router",
            "sdkwork-manager",
            "sdkwork-news",
            "sdkwork-portal",
            "sdkwork-shop",
            "sdkwork-skills",
            "sdkwork-deployments",
            "sdkwork-dezhou",
            "sdkwork-documents",
            "sdkwork-modelkit",
            "sdkwork-prompts",
            "sdkwork-web-server",
            "sdkwork-browser",
            "sdkwork-knowledgebase",
            "sdkwork-llm",
            "sdkwork-mahjong",
            "sdkwork-doudizhu",
            "sdkwork-xiangqi",
        ];
        let expected_service_ids = config
            .dependency_surfaces
            .iter()
            .filter(|surface| workspaces.contains(&surface.workspace.as_str()))
            .map(|surface| surface.service_id.clone())
            .collect::<HashSet<_>>();
        let mut routers = HashMap::new();

        for workspace in workspaces {
            insert_workspace_router(&config, &mut routers, workspace, Router::new());
        }

        assert_eq!(routers.len(), expected_service_ids.len());
        for service_id in expected_service_ids {
            assert!(
                routers.contains_key(service_id.as_str()),
                "embedded router registered for {service_id}"
            );
        }
    }
}
