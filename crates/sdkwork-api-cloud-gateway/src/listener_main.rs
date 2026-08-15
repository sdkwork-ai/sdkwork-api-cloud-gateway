use std::{collections::HashMap, env, net::SocketAddr};

use anyhow::{anyhow, Context};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
#[cfg(not(all(feature = "foundation-appbase", feature = "foundation-im-realtime")))]
use sdkwork_api_cloud_gateway::build_sdkwork_api_cloud_gateway_router;
#[cfg(all(feature = "foundation-appbase", feature = "foundation-im-realtime"))]
use sdkwork_api_cloud_gateway::build_sdkwork_api_cloud_gateway_router_with_embedded_realtime;
use sdkwork_api_cloud_gateway::{
    embedded_database_bootstrap::migrate_embedded_databases, load_gateway_config,
};
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{
    filter::Filtered, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

const GATEWAY_RUNTIME_STACK_SIZE: usize = 16 * 1024 * 1024;
const MIGRATE_DATABASES_ARG: &str = "--migrate-databases";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessMode {
    Serve,
    MigrateDatabases,
}

fn main() -> anyhow::Result<()> {
    install_process_crypto_provider()?;
    let process_mode = process_mode_from_args(env::args().skip(1));
    if process_mode == ProcessMode::MigrateDatabases {
        // SAFETY: this happens before the Tokio runtime or any worker thread is
        // created. The explicit lifecycle command must apply pending modules
        // even when the service runtime keeps auto-migration disabled.
        unsafe { env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true") };
    }

    std::thread::Builder::new()
        .name("sdkwork-api-cloud-gateway-runtime".to_owned())
        .stack_size(GATEWAY_RUNTIME_STACK_SIZE)
        .spawn(move || run_gateway_process(process_mode))
        .context("failed to spawn the gateway runtime thread")?
        .join()
        .map_err(|_| anyhow!("gateway runtime thread panicked"))?
}

fn run_gateway_process(process_mode: ProcessMode) -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build the gateway Tokio runtime")?
        .block_on(run_gateway(process_mode))
}

async fn run_gateway(process_mode: ProcessMode) -> anyhow::Result<()> {
    sdkwork_database_sqlx::enable_process_shared_database_pool();

    let env = env::vars().collect::<HashMap<_, _>>();
    let config = load_gateway_config(env::args().skip(1), &env, ".")?;
    init_tracing(&config)?;
    if process_mode == ProcessMode::MigrateDatabases {
        migrate_embedded_databases(&config)
            .await?
            .ok_or_else(|| anyhow!("no embedded database modules are enabled"))?;
        info!("sdkwork api gateway database migration completed");
        return Ok(());
    }
    let bind: SocketAddr = config
        .server
        .bind
        .parse()
        .with_context(|| format!("invalid bind address `{}`", config.server.bind))?;

    // The realtime builder additionally returns shutdown handles so the
    // embedded IM realtime plane (WebSocket upgrade + TCP/UDP/QUIC link
    // listeners) drains its routes before the process exits.
    #[cfg(all(feature = "foundation-appbase", feature = "foundation-im-realtime"))]
    let (app, shutdown) =
        build_sdkwork_api_cloud_gateway_router_with_embedded_realtime(config).await?;
    #[cfg(not(all(feature = "foundation-appbase", feature = "foundation-im-realtime")))]
    let app = build_sdkwork_api_cloud_gateway_router(config).await?;

    let listener = TcpListener::bind(bind).await?;
    let local_addr = listener.local_addr()?;

    info!(%local_addr, "sdkwork api gateway listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    #[cfg(all(feature = "foundation-appbase", feature = "foundation-im-realtime"))]
    shutdown.shutdown().await;

    Ok(())
}

fn process_mode_from_args<I, S>(args: I) -> ProcessMode
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if args
        .into_iter()
        .any(|argument| argument.as_ref() == MIGRATE_DATABASES_ARG)
    {
        ProcessMode::MigrateDatabases
    } else {
        ProcessMode::Serve
    }
}

fn install_process_crypto_provider() -> anyhow::Result<()> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to install the process-level Rustls crypto provider"))
}

/// Install the tracing subscriber, optionally exporting request spans to the
/// configured OpenTelemetry OTLP endpoint.
fn init_tracing(config: &GatewayRuntimeConfig) -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sdkwork_api_cloud_gateway=info,tower_http=info"));

    if !config.observability.otel.enabled {
        fmt().with_env_filter(filter).init();
        return Ok(());
    }

    let service_name = config
        .observability
        .otel
        .service_name
        .clone()
        .unwrap_or_else(|| config.service.name.clone());
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.observability.otel.endpoint.clone())
        .build()
        .with_context(|| {
            format!(
                "failed to build OTLP span exporter for {}",
                config.observability.otel.endpoint
            )
        })?;
    let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();
    let tracer = provider.tracer(service_name.clone());
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    tracing_subscriber::registry()
        .with(Filtered::new(fmt::layer(), filter))
        .with(otel_layer)
        .init();
    info!(
        endpoint = %config.observability.otel.endpoint,
        service_name = %service_name,
        "OpenTelemetry span export enabled"
    );
    Ok(())
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

    #[test]
    fn recognizes_explicit_database_migration_mode() {
        assert_eq!(
            process_mode_from_args(["--config", "gateway.toml", MIGRATE_DATABASES_ARG]),
            ProcessMode::MigrateDatabases
        );
        assert_eq!(
            process_mode_from_args(["--config=gateway.toml"]),
            ProcessMode::Serve
        );
    }
}
