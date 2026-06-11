use std::{collections::HashMap, path::PathBuf};

use sdkwork_api_gateway_api_server::load_gateway_config;
use sdkwork_api_gateway_config::GatewayMode;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn loads_sdkwork_manifest_from_application_root_when_no_config_path_is_provided() {
    let config =
        load_gateway_config(Vec::<String>::new(), &HashMap::new(), workspace_root()).unwrap();

    assert_eq!(config.service.name, "sdkwork-api-gateway");
    assert_eq!(config.server.bind, "127.0.0.1:3900");
    assert!(config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.service_id == "sdkwork-appbase-app-api"));
}

#[test]
fn explicit_config_path_and_environment_overrides_win_over_manifest_defaults() {
    let root = workspace_root();
    let config_path = root.join("configs/sdkwork-api-gateway.test.toml.example");
    let mut env = HashMap::new();
    env.insert(
        "SDKWORK_API_GATEWAY_BIND".to_string(),
        "127.0.0.1:4998".to_string(),
    );
    env.insert("SDKWORK_API_GATEWAY_MODE".to_string(), "split".to_string());
    env.insert(
        "SDKWORK_APPBASE_APP_API_BASE_URL".to_string(),
        "http://127.0.0.1:4888".to_string(),
    );

    let config = load_gateway_config(
        vec!["--config".to_string(), config_path.display().to_string()],
        &env,
        root,
    )
    .unwrap();

    assert_eq!(config.server.bind, "127.0.0.1:4998");
    assert_eq!(config.mode, GatewayMode::Split);
    assert_eq!(
        config
            .upstream("sdkwork-appbase-app-api")
            .expect("env-created appbase upstream")
            .base_url,
        "http://127.0.0.1:4888"
    );
}
