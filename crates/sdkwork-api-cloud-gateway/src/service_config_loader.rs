use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::Context;
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;

pub fn load_gateway_config<I, S>(
    args: I,
    env: &HashMap<String, String>,
    application_root: impl AsRef<Path>,
) -> anyhow::Result<GatewayRuntimeConfig>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let application_root = application_root.as_ref();
    let config = if let Some(path) = config_path_from_args(args).or_else(|| {
        env.get("SDKWORK_API_CLOUD_GATEWAY_CONFIG")
            .map(PathBuf::from)
    }) {
        GatewayRuntimeConfig::from_path(resolve_config_path(application_root, path))
            .context("failed to load explicit gateway config")?
    } else if application_root.join("sdkwork.app.config.json").exists() {
        GatewayRuntimeConfig::from_application_root(application_root)
            .context("failed to load sdkwork app manifest")?
    } else {
        GatewayRuntimeConfig::default()
    };

    config
        .with_env_overrides(env)
        .context("failed to apply gateway environment overrides")
}

pub fn config_path_from_args<I, S>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            return args.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn resolve_config_path(application_root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        application_root.join(path)
    }
}
