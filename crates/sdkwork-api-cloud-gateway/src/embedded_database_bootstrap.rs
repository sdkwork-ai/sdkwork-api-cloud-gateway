use std::{future::Future, pin::Pin};

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_sqlx::{create_pool_from_env, process_shared_database_pool, DatabasePool};

use crate::GatewayRuntimeError;

const PROCESS_DATABASE_SERVICE_CODE: &str = "CLAW";

const DATABASE_WORKSPACES: &[&str] = &[
    "sdkwork-iam",
    "sdkwork-drive",
    "sdkwork-membership",
    "sdkwork-notary",
    "sdkwork-voice",
    "sdkwork-rtc",
    "sdkwork-clawrouter",
    "sdkwork-appstore",
    "sdkwork-course",
    "sdkwork-cms",
    "sdkwork-forum",
    "sdkwork-im",
    "sdkwork-mail",
    "sdkwork-mcp",
    "sdkwork-dezhou",
    "sdkwork-music",
    "sdkwork-image",
    "sdkwork-aiot",
    "sdkwork-community",
    "sdkwork-kernel",
    "sdkwork-order",
    "sdkwork-promotion",
    "sdkwork-payment",
    "sdkwork-documents",
    "sdkwork-prompts",
    "sdkwork-deployments",
    "sdkwork-web-server",
    "sdkwork-modelkit",
    "sdkwork-comments",
    "sdkwork-messaging",
    "sdkwork-audio",
];

pub async fn bootstrap_embedded_databases(
    config: &GatewayRuntimeConfig,
) -> Result<(), GatewayRuntimeError> {
    if !config
        .dependency_surfaces
        .iter()
        .any(|surface| DATABASE_WORKSPACES.contains(&surface.workspace.as_str()))
    {
        return Ok(());
    }

    let pool = process_database_pool().await?;

    #[cfg(feature = "foundation-appbase")]
    if workspace_enabled(config, "sdkwork-iam") {
        run_module(
            "sdkwork-iam",
            "iam",
            sdkwork_iam_database_host::bootstrap_iam_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-drive")]
    if workspace_enabled(config, "sdkwork-drive") {
        run_module(
            "sdkwork-drive",
            "drive",
            sdkwork_drive_database_host::bootstrap_drive_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-membership")]
    if workspace_enabled(config, "sdkwork-membership") {
        run_module(
            "sdkwork-membership",
            "membership",
            sdkwork_membership_database_host::bootstrap_membership_database_with_pool(&pool),
        )
        .await?;
    }

    #[cfg(feature = "foundation-notary")]
    if workspace_enabled(config, "sdkwork-notary") {
        run_module(
            "sdkwork-notary",
            "notary",
            sdkwork_notary_database_host::bootstrap_notary_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-voice")]
    if workspace_enabled(config, "sdkwork-voice") {
        run_module(
            "sdkwork-voice",
            "voice",
            sdkwork_voice_database_host::bootstrap_voice_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-rtc")]
    if workspace_enabled(config, "sdkwork-rtc") {
        run_module(
            "sdkwork-rtc",
            "rtc",
            sdkwork_rtc_database_host::bootstrap_rtc_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-clawrouter")]
    if workspace_enabled(config, "sdkwork-clawrouter") {
        run_module(
            "sdkwork-clawrouter",
            "clawrouter",
            sdkwork_clawrouter_database_host::bootstrap_claw_router_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-appstore")]
    if workspace_enabled(config, "sdkwork-appstore") {
        run_module(
            "sdkwork-appstore",
            "appstore",
            sdkwork_appstore_database_host::bootstrap_appstore_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-course")]
    if workspace_enabled(config, "sdkwork-course") {
        run_module(
            "sdkwork-course",
            "course",
            sdkwork_course_database_host::bootstrap_course_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-cms")]
    if workspace_enabled(config, "sdkwork-cms") {
        run_module(
            "sdkwork-cms",
            "cms",
            sdkwork_cms_database_host::bootstrap_cms_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-forum")]
    if workspace_enabled(config, "sdkwork-forum") {
        run_module(
            "sdkwork-forum",
            "forum",
            sdkwork_forum_database_host::bootstrap_forum_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-im")]
    if workspace_enabled(config, "sdkwork-im") {
        run_module(
            "sdkwork-im",
            "im",
            sdkwork_im_database_host::bootstrap_im_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-mail")]
    if workspace_enabled(config, "sdkwork-mail") {
        run_module(
            "sdkwork-mail",
            "mail",
            sdkwork_mail_database_host::bootstrap_mail_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-mcp")]
    if workspace_enabled(config, "sdkwork-mcp") {
        run_module(
            "sdkwork-mcp",
            "mcp",
            sdkwork_mcp_database_host::bootstrap_mcp_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-dezhou")]
    if workspace_enabled(config, "sdkwork-dezhou") {
        run_module(
            "sdkwork-dezhou",
            "dezhou",
            sdkwork_dezhou_database_host::bootstrap_dezhou_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-music")]
    if workspace_enabled(config, "sdkwork-music") {
        run_module(
            "sdkwork-music",
            "music",
            sdkwork_music_database_host::bootstrap_music_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-image")]
    if workspace_enabled(config, "sdkwork-image") {
        run_module(
            "sdkwork-image",
            "image",
            sdkwork_image_database_host::bootstrap_image_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-aiot")]
    if workspace_enabled(config, "sdkwork-aiot") {
        run_module(
            "sdkwork-aiot",
            "aiot",
            sdkwork_aiot_database_host::bootstrap_aiot_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-community")]
    if workspace_enabled(config, "sdkwork-community") {
        run_module(
            "sdkwork-community",
            "community",
            sdkwork_community_database_host::bootstrap_community_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-kernel")]
    if workspace_enabled(config, "sdkwork-kernel") {
        run_module(
            "sdkwork-kernel",
            "agents",
            sdkwork_agents_database_host::bootstrap_agents_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-order")]
    if workspace_enabled(config, "sdkwork-order") {
        run_module(
            "sdkwork-order",
            "order",
            sdkwork_order_database_host::bootstrap_order_database_from_env(),
        )
        .await?;
    }

    #[cfg(feature = "foundation-promotion")]
    if workspace_enabled(config, "sdkwork-promotion") {
        run_module(
            "sdkwork-promotion",
            "promotion",
            sdkwork_promotion_database_host::bootstrap_promotion_database_from_env(),
        )
        .await?;
    }

    #[cfg(feature = "foundation-payment")]
    if workspace_enabled(config, "sdkwork-payment") {
        run_module(
            "sdkwork-payment",
            "payment",
            sdkwork_payment_database_host::bootstrap_payment_database_with_pool(&pool),
        )
        .await?;
    }

    #[cfg(feature = "foundation-documents")]
    if workspace_enabled(config, "sdkwork-documents") {
        run_module(
            "sdkwork-documents",
            "documents",
            sdkwork_documents_database_host::bootstrap_documents_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-prompts")]
    if workspace_enabled(config, "sdkwork-prompts") {
        run_module(
            "sdkwork-prompts",
            "prompts",
            sdkwork_prompts_database_host::bootstrap_prompts_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-deployments")]
    if workspace_enabled(config, "sdkwork-deployments") {
        run_module(
            "sdkwork-deployments",
            "deploy",
            sdkwork_deploy_database_host::bootstrap_deploy_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-web-server")]
    if workspace_enabled(config, "sdkwork-web-server") {
        run_module(
            "sdkwork-web-server",
            "web",
            sdkwork_webserver_database_host::bootstrap_web_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-modelkit")]
    if workspace_enabled(config, "sdkwork-modelkit") {
        run_module(
            "sdkwork-modelkit",
            "modelkit",
            sdkwork_modelkit_database_host::bootstrap_modelkit_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-comments")]
    if workspace_enabled(config, "sdkwork-comments") {
        run_module(
            "sdkwork-comments",
            "comments",
            sdkwork_comments_database_host::bootstrap_comments_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-messaging")]
    if workspace_enabled(config, "sdkwork-messaging") {
        run_module(
            "sdkwork-messaging",
            "messaging",
            sdkwork_messaging_database_host::bootstrap_messaging_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-audio")]
    if workspace_enabled(config, "sdkwork-audio") {
        run_module(
            "sdkwork-audio",
            "audio",
            sdkwork_audio_database_host::bootstrap_audio_database(pool.clone()),
        )
        .await?;
    }

    Ok(())
}

async fn process_database_pool() -> Result<DatabasePool, GatewayRuntimeError> {
    if let Some(pool) = process_shared_database_pool() {
        return Ok(pool);
    }

    create_pool_from_env(PROCESS_DATABASE_SERVICE_CODE)
        .await
        .map_err(|error| GatewayRuntimeError::DatabasePoolBootstrap {
            message: error.to_string(),
        })?
        .ok_or_else(|| GatewayRuntimeError::DatabasePoolBootstrap {
            message: format!(
                "missing SDKWORK_{PROCESS_DATABASE_SERVICE_CODE}_DATABASE_* configuration"
            ),
        })
}

fn run_module<'a, T, F>(
    workspace: &'a str,
    module_id: &'a str,
    bootstrap: F,
) -> Pin<Box<dyn Future<Output = Result<(), GatewayRuntimeError>> + 'a>>
where
    F: Future<Output = Result<T, String>> + 'a,
    T: 'a,
{
    Box::pin(async move {
        bootstrap
            .await
            .map_err(|message| GatewayRuntimeError::EmbeddedDatabaseBootstrap {
                workspace: workspace.to_owned(),
                module_id: module_id.to_owned(),
                message,
            })?;
        tracing::info!(workspace, module_id, "embedded database module initialized");
        Ok(())
    })
}

fn workspace_enabled(config: &GatewayRuntimeConfig, workspace: &str) -> bool {
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.workspace == workspace)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;

    use super::{workspace_enabled, DATABASE_WORKSPACES};

    #[test]
    fn database_bootstrap_convention_covers_declared_dependency_workspaces_in_order() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = GatewayRuntimeConfig::from_application_root(root)
            .expect("gateway application manifest parses");
        let declared = config
            .dependency_surfaces
            .iter()
            .map(|surface| surface.workspace.as_str())
            .collect::<HashSet<_>>();

        assert_eq!(DATABASE_WORKSPACES.len(), 31);
        assert_eq!(
            DATABASE_WORKSPACES.iter().copied().collect::<HashSet<_>>(),
            declared
        );
        for workspace in DATABASE_WORKSPACES {
            assert!(workspace_enabled(&config, workspace));
        }
    }
}
