use std::path::PathBuf;

use sdkwork_iam_embedded_application_bootstrap::{
    ensure_tenant_application_from_app_root_with_env_and_fallback,
    resolve_application_app_root_with_fallback, resolve_bootstrap_environment,
};

pub async fn ensure_embedded_iam_tenant_application_bootstrap() -> Result<(), String> {
    let app_root = resolve_embedded_iam_app_root();
    sdkwork_iam_database_host::unified_postgres_env::apply_workspace_postgres_env(&app_root);
    sdkwork_iam_database_host::bootstrap_iam_database_from_env()
        .await
        .map_err(|error| format!("failed to bootstrap IAM database lifecycle: {error}"))?;
    ensure_tenant_application_from_app_root_with_env_and_fallback(
        resolve_bootstrap_environment().as_str(),
        app_root,
        None,
        &[],
    )
    .await
}

fn resolve_embedded_iam_app_root() -> PathBuf {
    resolve_application_app_root_with_fallback(
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_iam_app_root_prefers_sdkwork_app_root_env() {
        let temp = std::env::temp_dir().join(format!(
            "api-cloud-gateway-app-root-test-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&temp);
        let previous = std::env::var("SDKWORK_APP_ROOT").ok();
        unsafe {
            std::env::set_var("SDKWORK_APP_ROOT", temp.as_os_str());
        }
        assert_eq!(temp, resolve_embedded_iam_app_root());
        unsafe {
            match previous {
                Some(value) => std::env::set_var("SDKWORK_APP_ROOT", value),
                None => std::env::remove_var("SDKWORK_APP_ROOT"),
            }
        }
        let _ = std::fs::remove_dir_all(temp);
    }
}
