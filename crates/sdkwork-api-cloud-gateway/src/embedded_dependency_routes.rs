//! Embedded dependency routers for platform collapsed ingress (§5.7.2).
//! Each registered router is business-only; host routes own `/healthz`, `/livez`, `/readyz`.

use std::collections::HashMap;

use axum::Router;
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;

use crate::GatewayRuntimeError;

pub async fn bootstrap_embedded_dependency_routers(
    config: &GatewayRuntimeConfig,
) -> Result<HashMap<String, Router>, GatewayRuntimeError> {
    let mut routers = HashMap::new();

    #[cfg(feature = "foundation-appbase")]
    {
        use sdkwork_api_cloud_gateway_config::{
            APPBASE_APP_API_SERVICE_ID, APPBASE_BACKEND_API_SERVICE_ID,
        };

        if dependency_surface_enabled(config, APPBASE_APP_API_SERVICE_ID) {
            let router = crate::runtime::build_embedded_sdkwork_iam_app_api_router().await?;
            routers.insert(APPBASE_APP_API_SERVICE_ID.to_owned(), router);
        }
        if dependency_surface_enabled(config, APPBASE_BACKEND_API_SERVICE_ID) {
            let router = crate::runtime::build_embedded_sdkwork_iam_backend_api_router().await?;
            routers.insert(APPBASE_BACKEND_API_SERVICE_ID.to_owned(), router);
        }
    }

    #[cfg(feature = "foundation-drive")]
    if workspace_surface_enabled(config, "sdkwork-drive") {
        let assembly =
            sdkwork_drive_gateway_assembly::assemble_application_business_router_from_env()
                .await
                .map_err(|error| workspace_build_error(config, "sdkwork-drive", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-drive", assembly.router);
    }

    #[cfg(feature = "foundation-membership")]
    if workspace_surface_enabled(config, "sdkwork-membership") {
        let assembly =
            sdkwork_membership_gateway_assembly::assemble_application_business_router_from_env()
                .await
                .map_err(|error| workspace_build_error(config, "sdkwork-membership", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-membership", assembly.router);
    }

    #[cfg(feature = "foundation-order")]
    if workspace_surface_enabled(config, "sdkwork-order") {
        let assembly = sdkwork_order_gateway_assembly::assemble_application_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-order", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-order", assembly.router);
    }

    #[cfg(feature = "foundation-promotion")]
    if workspace_surface_enabled(config, "sdkwork-promotion") {
        let assembly = sdkwork_promotion_gateway_assembly::assemble_application_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-promotion", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-promotion", assembly.router);
    }

    #[cfg(feature = "foundation-payment")]
    if workspace_surface_enabled(config, "sdkwork-payment") {
        let assembly = sdkwork_payment_gateway_assembly::assemble_application_router_from_env()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-payment", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-payment", assembly.router);
    }

    #[cfg(feature = "foundation-audio")]
    if workspace_surface_enabled(config, "sdkwork-audio") {
        let assembly = sdkwork_audio_gateway_assembly::assemble_application_router();
        insert_workspace_router(config, &mut routers, "sdkwork-audio", assembly.router);
    }

    #[cfg(feature = "foundation-course")]
    {
        use sdkwork_api_cloud_gateway_config::{
            COURSE_APP_API_SERVICE_ID, COURSE_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, COURSE_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, COURSE_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_course_gateway_assembly::assemble_application_business_router()
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
            let assembly = sdkwork_voice_gateway_assembly::assemble_application_router()
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

    #[cfg(feature = "foundation-music")]
    {
        use sdkwork_api_cloud_gateway_config::{
            MUSIC_APP_API_SERVICE_ID, MUSIC_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, MUSIC_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, MUSIC_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_music_gateway_assembly::assemble_application_router();
            let music_router = assembly.router;
            if dependency_surface_enabled(config, MUSIC_APP_API_SERVICE_ID) {
                routers.insert(MUSIC_APP_API_SERVICE_ID.to_owned(), music_router.clone());
            }
            if dependency_surface_enabled(config, MUSIC_BACKEND_API_SERVICE_ID) {
                routers.insert(MUSIC_BACKEND_API_SERVICE_ID.to_owned(), music_router);
            }
        }
    }

    #[cfg(feature = "foundation-messaging")]
    {
        use sdkwork_api_cloud_gateway_config::{
            MESSAGING_APP_API_SERVICE_ID, MESSAGING_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, MESSAGING_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, MESSAGING_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_messaging_gateway_assembly::assemble_application_router();
            let messaging_router = assembly.router;
            if dependency_surface_enabled(config, MESSAGING_APP_API_SERVICE_ID) {
                routers.insert(
                    MESSAGING_APP_API_SERVICE_ID.to_owned(),
                    messaging_router.clone(),
                );
            }
            if dependency_surface_enabled(config, MESSAGING_BACKEND_API_SERVICE_ID) {
                routers.insert(
                    MESSAGING_BACKEND_API_SERVICE_ID.to_owned(),
                    messaging_router,
                );
            }
        }
    }

    #[cfg(feature = "foundation-image")]
    {
        use sdkwork_api_cloud_gateway_config::{
            IMAGE_APP_API_SERVICE_ID, IMAGE_BACKEND_API_SERVICE_ID, IMAGE_OPEN_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, IMAGE_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, IMAGE_BACKEND_API_SERVICE_ID)
            || dependency_surface_enabled(config, IMAGE_OPEN_API_SERVICE_ID)
        {
            let assembly = sdkwork_image_gateway_assembly::assemble_application_router_from_env()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: IMAGE_APP_API_SERVICE_ID.to_owned(),
                    message,
                })?;
            let image_router = assembly.router;
            if dependency_surface_enabled(config, IMAGE_APP_API_SERVICE_ID) {
                routers.insert(IMAGE_APP_API_SERVICE_ID.to_owned(), image_router.clone());
            }
            if dependency_surface_enabled(config, IMAGE_BACKEND_API_SERVICE_ID) {
                routers.insert(
                    IMAGE_BACKEND_API_SERVICE_ID.to_owned(),
                    image_router.clone(),
                );
            }
            if dependency_surface_enabled(config, IMAGE_OPEN_API_SERVICE_ID) {
                routers.insert(IMAGE_OPEN_API_SERVICE_ID.to_owned(), image_router);
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
            let assembly = sdkwork_cms_gateway_assembly::assemble_application_router();
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
            let assembly = sdkwork_forum_gateway_assembly::assemble_application_router();
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
            let assembly = sdkwork_community_gateway_assembly::assemble_application_router();
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

    #[cfg(feature = "foundation-comments")]
    {
        use sdkwork_api_cloud_gateway_config::{
            COMMENTS_APP_API_SERVICE_ID, COMMENTS_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, COMMENTS_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, COMMENTS_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_comments_gateway_assembly::assemble_application_router();
            let comments_router = assembly.router;
            if dependency_surface_enabled(config, COMMENTS_APP_API_SERVICE_ID) {
                routers.insert(
                    COMMENTS_APP_API_SERVICE_ID.to_owned(),
                    comments_router.clone(),
                );
            }
            if dependency_surface_enabled(config, COMMENTS_BACKEND_API_SERVICE_ID) {
                routers.insert(COMMENTS_BACKEND_API_SERVICE_ID.to_owned(), comments_router);
            }
        }
    }

    #[cfg(feature = "foundation-clawrouter")]
    {
        use sdkwork_api_cloud_gateway_config::SDKWORK_CLAW_GATEWAY_SERVICE_ID;
        if dependency_surface_enabled(config, SDKWORK_CLAW_GATEWAY_SERVICE_ID) {
            let assembly = sdkwork_clawrouter_gateway_assembly::assemble_application_router();
            routers.insert(SDKWORK_CLAW_GATEWAY_SERVICE_ID.to_owned(), assembly.router);
        }
    }

    #[cfg(feature = "foundation-mcp")]
    {
        use sdkwork_api_cloud_gateway_config::{
            MCP_APP_API_SERVICE_ID, MCP_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, MCP_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, MCP_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_mcp_gateway_assembly::assemble_application_router()
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
    {
        use sdkwork_api_cloud_gateway_config::{
            SDKWORK_IM_BACKEND_HTTP_SERVICE_ID, SDKWORK_IM_IM_HTTP_SERVICE_ID,
        };
        if dependency_surface_enabled(config, SDKWORK_IM_IM_HTTP_SERVICE_ID)
            || dependency_surface_enabled(config, SDKWORK_IM_BACKEND_HTTP_SERVICE_ID)
        {
            let assembly = sdkwork_im_gateway_assembly::assemble_application_router()
                .await
                .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                    service_id: SDKWORK_IM_IM_HTTP_SERVICE_ID.to_owned(),
                    message,
                })?;
            let im_router = assembly.router;
            if dependency_surface_enabled(config, SDKWORK_IM_IM_HTTP_SERVICE_ID) {
                routers.insert(SDKWORK_IM_IM_HTTP_SERVICE_ID.to_owned(), im_router.clone());
            }
            if dependency_surface_enabled(config, SDKWORK_IM_BACKEND_HTTP_SERVICE_ID) {
                routers.insert(SDKWORK_IM_BACKEND_HTTP_SERVICE_ID.to_owned(), im_router);
            }
        }
    }

    #[cfg(feature = "foundation-rtc")]
    {
        use sdkwork_api_cloud_gateway_config::{
            RTC_APP_API_SERVICE_ID, RTC_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, RTC_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, RTC_BACKEND_API_SERVICE_ID)
        {
            let assembly = sdkwork_rtc_gateway_assembly::assemble_application_router()
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
            let assembly = sdkwork_notary_gateway_assembly::assemble_application_business_router()
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
            let assembly = sdkwork_mail_gateway_assembly::assemble_application_router()
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
            let assembly = sdkwork_aiot_gateway_assembly::assemble_application_router()
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
            let assembly = sdkwork_appstore_gateway_assembly::assemble_application_router()
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

    #[cfg(feature = "foundation-kernel")]
    if workspace_surface_enabled(config, "sdkwork-kernel") {
        let assembly = sdkwork_agents_gateway_assembly::assemble_application_router()
            .await
            .map_err(|error| {
                workspace_build_error(config, "sdkwork-kernel", format!("{error:#}"))
            })?;
        insert_workspace_router(config, &mut routers, "sdkwork-kernel", assembly.router);
    }

    #[cfg(feature = "foundation-deployments")]
    if workspace_surface_enabled(config, "sdkwork-deployments") {
        let assembly = sdkwork_deployments_gateway_assembly::assemble_application_business_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-deployments", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-deployments", assembly.router);
    }

    #[cfg(feature = "foundation-dezhou")]
    if workspace_surface_enabled(config, "sdkwork-dezhou") {
        let assembly = sdkwork_dezhou_gateway_assembly::assemble_application_business_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-dezhou", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-dezhou", assembly.router);
    }

    #[cfg(feature = "foundation-documents")]
    if workspace_surface_enabled(config, "sdkwork-documents") {
        let assembly =
            sdkwork_documents_gateway_assembly::assemble_application_business_router_from_env()
                .await
                .map_err(|error| workspace_build_error(config, "sdkwork-documents", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-documents", assembly.router);
    }

    #[cfg(feature = "foundation-modelkit")]
    if workspace_surface_enabled(config, "sdkwork-modelkit") {
        let assembly =
            sdkwork_modelkit_gateway_assembly::assemble_application_business_router_from_env()
                .await
                .map_err(|error| workspace_build_error(config, "sdkwork-modelkit", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-modelkit", assembly.router);
    }

    #[cfg(feature = "foundation-prompts")]
    if workspace_surface_enabled(config, "sdkwork-prompts") {
        let assembly = sdkwork_prompts_gateway_assembly::assemble_application_business_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-prompts", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-prompts", assembly.router);
    }

    #[cfg(feature = "foundation-web-server")]
    if workspace_surface_enabled(config, "sdkwork-web-server") {
        let assembly = sdkwork_web_server_gateway_assembly::assemble_application_business_router()
            .await
            .map_err(|error| workspace_build_error(config, "sdkwork-web-server", error))?;
        insert_workspace_router(config, &mut routers, "sdkwork-web-server", assembly.router);
    }

    Ok(routers)
}

#[cfg(any(
    feature = "foundation-drive",
    feature = "foundation-membership",
    feature = "foundation-order",
    feature = "foundation-promotion",
    feature = "foundation-payment",
    feature = "foundation-audio",
    feature = "foundation-kernel",
    feature = "foundation-deployments",
    feature = "foundation-dezhou",
    feature = "foundation-documents",
    feature = "foundation-modelkit",
    feature = "foundation-prompts",
    feature = "foundation-web-server"
))]
fn workspace_surface_enabled(config: &GatewayRuntimeConfig, workspace: &str) -> bool {
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.workspace == workspace)
}

#[cfg(any(
    test,
    feature = "foundation-drive",
    feature = "foundation-membership",
    feature = "foundation-order",
    feature = "foundation-promotion",
    feature = "foundation-payment",
    feature = "foundation-audio",
    feature = "foundation-kernel",
    feature = "foundation-deployments",
    feature = "foundation-dezhou",
    feature = "foundation-documents",
    feature = "foundation-modelkit",
    feature = "foundation-prompts",
    feature = "foundation-web-server"
))]
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

#[cfg(any(
    feature = "foundation-drive",
    feature = "foundation-membership",
    feature = "foundation-order",
    feature = "foundation-promotion",
    feature = "foundation-payment",
    feature = "foundation-audio",
    feature = "foundation-kernel",
    feature = "foundation-deployments",
    feature = "foundation-dezhou",
    feature = "foundation-documents",
    feature = "foundation-modelkit",
    feature = "foundation-prompts",
    feature = "foundation-web-server"
))]
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
            "sdkwork-audio",
            "sdkwork-kernel",
            "sdkwork-deployments",
            "sdkwork-dezhou",
            "sdkwork-documents",
            "sdkwork-modelkit",
            "sdkwork-prompts",
            "sdkwork-web-server",
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

        assert_eq!(expected_service_ids.len(), 23);
        assert_eq!(routers.len(), expected_service_ids.len());
        for service_id in expected_service_ids {
            assert!(
                routers.contains_key(service_id.as_str()),
                "embedded router registered for {service_id}"
            );
        }
    }
}
