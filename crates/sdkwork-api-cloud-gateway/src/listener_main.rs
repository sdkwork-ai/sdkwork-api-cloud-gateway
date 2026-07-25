use std::{collections::HashMap, env, net::SocketAddr};

use anyhow::{anyhow, Context};
use sdkwork_api_cloud_gateway::build_sdkwork_api_cloud_gateway_router;
use sdkwork_api_cloud_gateway::load_gateway_config;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

const GATEWAY_RUNTIME_STACK_SIZE: usize = 16 * 1024 * 1024;

fn main() -> anyhow::Result<()> {
    install_process_crypto_provider()?;

    std::thread::Builder::new()
        .name("sdkwork-api-cloud-gateway-runtime".to_owned())
        .stack_size(GATEWAY_RUNTIME_STACK_SIZE)
        .spawn(run_gateway_process)
        .context("failed to spawn the gateway runtime thread")?
        .join()
        .map_err(|_| anyhow!("gateway runtime thread panicked"))?
}

fn run_gateway_process() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build the gateway Tokio runtime")?
        .block_on(run_gateway())
}

async fn run_gateway() -> anyhow::Result<()> {
    init_tracing();
    sdkwork_database_sqlx::enable_process_shared_database_pool();

    let env = env::vars().collect::<HashMap<_, _>>();
    let config = load_gateway_config(env::args().skip(1), &env, ".")?;
    let bind: SocketAddr = config
        .server
        .bind
        .parse()
        .with_context(|| format!("invalid bind address `{}`", config.server.bind))?;
    let app = build_sdkwork_api_cloud_gateway_router(config).await?;
    let listener = TcpListener::bind(bind).await?;
    let local_addr = listener.local_addr()?;

    info!(%local_addr, "sdkwork api gateway listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn install_process_crypto_provider() -> anyhow::Result<()> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to install the process-level Rustls crypto provider"))
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sdkwork_api_cloud_gateway=info,tower_http=info"));
    fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install terminate signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_a_process_crypto_provider_before_router_construction() {
        install_process_crypto_provider().expect("crypto provider should install");
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}
