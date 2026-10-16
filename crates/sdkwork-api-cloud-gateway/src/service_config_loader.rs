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
    let config = if let Some(path) = config_path_from_args(args)
        .or_else(|| {
            env.get("SDKWORK_API_GATEWAY_CONFIG_FILE")
                .or_else(|| env.get("SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE"))
                .or_else(|| env.get("SDKWORK_API_CLOUD_GATEWAY_CONFIG"))
                .map(PathBuf::from)
        })
        .filter(|path| usable_config_candidate(application_root, path))
        .or_else(default_installed_config_path)
    {
        let config = GatewayRuntimeConfig::from_path(resolve_config_path(application_root, path))
            .context("failed to load explicit gateway config")?;
        if application_root.join("sdkwork.app.config.json").exists() {
            config
                .with_application_component_surfaces(application_root)
                .context("failed to synchronize gateway dependency surfaces from component spec")?
        } else {
            config
        }
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

/// A configured path is usable only when it resolves to a regular file.
///
/// Docker creates a directory at a bind-mount target when the host source does
/// not exist. The shipped `docker-compose.yml` mounts the operator config this
/// way, so a customer running `docker compose up` on a fresh checkout without
/// `docker/config/config.toml` would otherwise fail startup with an
/// unreadable-configuration error. Treat a directory as "no explicit config"
/// and fall through to the image-baked installed path instead.
fn usable_config_candidate(application_root: &Path, path: &Path) -> bool {
    resolve_config_path(application_root, path.to_path_buf()).is_file()
}

/// Installed Linux/container discovery order (APPLICATION_DEPLOY_LAYOUT_SPEC):
/// `topology.applicationCode = api-gateway` (lowercase kebab-case); the
/// canonical path is `/etc/sdkwork/api-gateway/config.toml`. The legacy
/// underscore paths below remain only as fallbacks for pre-migration
/// installs and are never written by new installers.
fn default_installed_config_path() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "/etc/sdkwork/api-gateway/config.toml",
        "/etc/sdkwork/api-gateway/sdkwork-api-cloud-gateway.toml",
        "/etc/sdkwork/api_gateway/config.toml",
        "/etc/sdkwork/api_gateway/sdkwork-api-cloud-gateway.toml",
    ];
    CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs, process,
        sync::atomic::{AtomicU32, Ordering},
    };

    /// Self-cleaning unique temporary directory: the crate has no tempfile
    /// dependency and the test only needs an isolated application root.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sdkwork-gateway-config-loader-{label}-{}-{unique}",
                process::id()
            ));
            fs::create_dir_all(&path).expect("temporary application root");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn configured_config_directory_falls_back_instead_of_failing() {
        let root = TempRoot::new("dir-mount");
        // Reproduce Docker's bind-mount behaviour: the mount target exists as a
        // directory because the host source file is absent.
        let mount_target = root.path().join("config.toml");
        fs::create_dir_all(&mount_target).expect("mount target directory");

        let mut env = HashMap::new();
        env.insert(
            "SDKWORK_API_CLOUD_GATEWAY_CONFIG_FILE".to_string(),
            mount_target.to_string_lossy().into_owned(),
        );

        // Must not error the way `GatewayRuntimeConfig::from_path` would on a
        // directory; the loader falls through to the installed/default config.
        let config = load_gateway_config(Vec::<String>::new(), &env, root.path())
            .expect("a directory mount target must not fail startup");
        drop(config);
    }

    #[test]
    fn usable_config_candidate_accepts_files_and_rejects_directories() {
        let root = TempRoot::new("candidate");
        let directory = root.path().join("as-directory");
        let file = root.path().join("as-file.toml");
        fs::create_dir_all(&directory).expect("directory candidate");
        fs::write(&file, "").expect("file candidate");

        assert!(usable_config_candidate(
            root.path(),
            Path::new("as-file.toml")
        ));
        assert!(!usable_config_candidate(
            root.path(),
            Path::new("as-directory")
        ));
        assert!(!usable_config_candidate(
            root.path(),
            Path::new("missing.toml")
        ));
    }
}
