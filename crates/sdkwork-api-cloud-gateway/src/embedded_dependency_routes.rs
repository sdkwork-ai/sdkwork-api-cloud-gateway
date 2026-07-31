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
}

impl EmbeddedDependencyRuntime {
    #[cfg(feature = "foundation-image")]
    fn retain_background_task(&mut self, task: Option<tokio::task::JoinHandle<()>>) {
        if let Some(task) = task {
            self.background_tasks.push(task);
        }
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
        assert!(!workspace_surface_enabled(&config, "unknown-workspace"));
    }
}
