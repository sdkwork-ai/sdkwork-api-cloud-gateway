use std::{future::Future, pin::Pin};

use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_database_sqlx::{create_pool_from_env, process_shared_database_pool, DatabasePool};

use crate::GatewayRuntimeError;

const PROCESS_DATABASE_SERVICE_CODE: &str = "CLOUD";

const DATABASE_WORKSPACES: &[&str] = &[
    "sdkwork-iam",
    "sdkwork-drive",
    "sdkwork-membership",
    "sdkwork-notary",
    "sdkwork-voice",
    "sdkwork-rtc",
    "sdkwork-cloudrouter",
    "sdkwork-appstore",
    "sdkwork-course",
    "sdkwork-cms",
    "sdkwork-forum",
    "sdkwork-im",
    "sdkwork-mail",
    "sdkwork-mcp",
    "sdkwork-dezhou",
    "sdkwork-image",
    "sdkwork-aiot",
    "sdkwork-community",
    "sdkwork-agents",
    "sdkwork-order",
    "sdkwork-promotion",
    "sdkwork-payment",
    "sdkwork-documents",
    "sdkwork-prompts",
    "sdkwork-deployments",
    "sdkwork-webserver",
    "sdkwork-browser",
    "sdkwork-knowledgebase",
    "sdkwork-llm",
    "sdkwork-mahjong",
    "sdkwork-doudizhu",
    "sdkwork-xiangqi",
    "sdkwork-modelkit",
    "sdkwork-account",
    "sdkwork-catalog",
    "sdkwork-customerservice",
    "sdkwork-gameengine",
    "sdkwork-inventory",
    "sdkwork-invoice",
    "sdkwork-skills",
    "sdkwork-local-router",
    "sdkwork-manager",
    "sdkwork-news",
    "sdkwork-portal",
    "sdkwork-shop",
];

pub async fn bootstrap_embedded_databases(
    config: &GatewayRuntimeConfig,
) -> Result<Option<DatabasePool>, GatewayRuntimeError> {
    if !config
        .dependency_surfaces
        .iter()
        .any(|surface| DATABASE_WORKSPACES.contains(&surface.workspace.as_str()))
    {
        return Ok(None);
    }

    // Lifecycle defaults: `[database] autoMigrate = true` (the gateway
    // default) makes a simple configuration initialize every embedded
    // database module on first start — fresh install or version upgrade.
    // The process environment still wins when it sets
    // SDKWORK_DATABASE_AUTO_MIGRATE explicitly.
    if config.database.auto_migrate && std::env::var_os("SDKWORK_DATABASE_AUTO_MIGRATE").is_none() {
        // SAFETY: single-threaded startup before any worker thread spawns.
        unsafe { std::env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true") };
    }

    #[allow(unused_variables)]
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
            sdkwork_membership_database_host::bootstrap_membership_database_host_with_pool(&pool),
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

    #[cfg(feature = "foundation-cloudrouter")]
    if workspace_enabled(config, "sdkwork-cloudrouter") {
        run_module(
            "sdkwork-cloudrouter",
            "cloudrouter",
            sdkwork_cloudrouter_database_host::bootstrap_cloud_router_database(pool.clone()),
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

    #[cfg(feature = "foundation-agents")]
    if workspace_enabled(config, "sdkwork-agents") {
        run_module(
            "sdkwork-agents",
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
    if workspace_enabled(config, "sdkwork-webserver") {
        run_module(
            "sdkwork-webserver",
            "web",
            sdkwork_webserver_database_host::bootstrap_web_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-browser")]
    if workspace_enabled(config, "sdkwork-browser") {
        run_module(
            "sdkwork-browser",
            "browser",
            sdkwork_browser_database_host::bootstrap_browser_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-knowledgebase")]
    if workspace_enabled(config, "sdkwork-knowledgebase") {
        run_module(
            "sdkwork-knowledgebase",
            "knowledgebase",
            sdkwork_knowledgebase_database_host::bootstrap_knowledgebase_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-llm")]
    if workspace_enabled(config, "sdkwork-llm") {
        run_module(
            "sdkwork-llm",
            "llm",
            sdkwork_llm_database_host::bootstrap_llm_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-mahjong")]
    if workspace_enabled(config, "sdkwork-mahjong") {
        run_module(
            "sdkwork-mahjong",
            "mahjong",
            sdkwork_mahjong_database_host::bootstrap_mahjong_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-doudizhu")]
    if workspace_enabled(config, "sdkwork-doudizhu") {
        run_module(
            "sdkwork-doudizhu",
            "doudizhu",
            sdkwork_doudizhu_database_host::bootstrap_doudizhu_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-xiangqi")]
    if workspace_enabled(config, "sdkwork-xiangqi") {
        run_module(
            "sdkwork-xiangqi",
            "xiangqi",
            sdkwork_xiangqi_database_host::bootstrap_xiangqi_database(pool.clone()),
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

    #[cfg(feature = "foundation-account")]
    if workspace_enabled(config, "sdkwork-account") {
        run_module(
            "sdkwork-account",
            "account",
            sdkwork_account_database_host::bootstrap_account_database_from_env(),
        )
        .await?;
    }

    #[cfg(feature = "foundation-catalog")]
    if workspace_enabled(config, "sdkwork-catalog") {
        run_module(
            "sdkwork-catalog",
            "catalog",
            sdkwork_catalog_database_host::bootstrap_catalog_database_from_env(),
        )
        .await?;
    }

    #[cfg(feature = "foundation-customerservice")]
    if workspace_enabled(config, "sdkwork-customerservice") {
        run_module(
            "sdkwork-customerservice",
            "customerservice",
            sdkwork_customerservice_database_host::bootstrap_customerservice_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-gameengine")]
    if workspace_enabled(config, "sdkwork-gameengine") {
        run_module(
            "sdkwork-gameengine",
            "games",
            sdkwork_gameengine_database_host::bootstrap_games_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-inventory")]
    if workspace_enabled(config, "sdkwork-inventory") {
        run_module(
            "sdkwork-inventory",
            "inventory",
            sdkwork_inventory_database_host::bootstrap_inventory_database_from_env(),
        )
        .await?;
    }

    #[cfg(feature = "foundation-invoice")]
    if workspace_enabled(config, "sdkwork-invoice") {
        run_module(
            "sdkwork-invoice",
            "invoice",
            sdkwork_invoice_database_host::bootstrap_invoice_database_from_env(),
        )
        .await?;
    }

    #[cfg(feature = "foundation-local-router")]
    if workspace_enabled(config, "sdkwork-local-router") {
        run_module(
            "sdkwork-local-router",
            "localrouter",
            sdkwork_local_router_database_host::bootstrap_local_router_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-manager")]
    if workspace_enabled(config, "sdkwork-manager") {
        run_module(
            "sdkwork-manager",
            "manager",
            sdkwork_manager_database_host::bootstrap_manager_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-news")]
    if workspace_enabled(config, "sdkwork-news") {
        run_module(
            "sdkwork-news",
            "news",
            sdkwork_news_database_host::bootstrap_news_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-portal")]
    if workspace_enabled(config, "sdkwork-portal") {
        run_module(
            "sdkwork-portal",
            "portal",
            sdkwork_portal_database_host::bootstrap_portal_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-shop")]
    if workspace_enabled(config, "sdkwork-shop") {
        run_module(
            "sdkwork-shop",
            "shop",
            sdkwork_shop_database_host::bootstrap_shop_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-skills")]
    if workspace_enabled(config, "sdkwork-skills") {
        run_module(
            "sdkwork-skills",
            "skills",
            sdkwork_skills_database_host::bootstrap_skills_database(pool.clone()),
        )
        .await?;
    }

    #[cfg(feature = "foundation-shop")]
    if workspace_enabled(config, "sdkwork-merchandise") {
        run_module(
            "sdkwork-merchandise",
            "merchandise",
            sdkwork_merchandise_database_host::bootstrap_merchandise_database(pool.clone()),
        )
        .await?;
    }

    Ok(Some(pool))
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
            message: "missing SDKWORK_DATABASE_* configuration".to_string(),
        })
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

        assert_eq!(
            DATABASE_WORKSPACES.iter().copied().collect::<HashSet<_>>(),
            declared
        );
        for workspace in DATABASE_WORKSPACES {
            assert!(workspace_enabled(&config, workspace));
        }
    }
}
