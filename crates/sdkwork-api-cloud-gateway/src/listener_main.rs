use std::{collections::HashMap, env, net::SocketAddr};

use anyhow::Context;
use sdkwork_api_cloud_gateway::build_sdkwork_api_cloud_gateway_router;
use sdkwork_api_cloud_gateway::load_gateway_config;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
