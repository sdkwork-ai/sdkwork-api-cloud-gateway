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
const HELP_ARGS: [&str; 2] = ["-h", "--help"];
const VERSION_ARGS: [&str; 2] = ["-V", "--version"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessMode {
    Serve,
    MigrateDatabases,
    Help,
    Version,
}

fn main() -> anyhow::Result<()> {
    let process_mode = process_mode_from_args(env::args().skip(1));
    match process_mode {
        ProcessMode::Help => {
            print_help();
            return Ok(());
        }
        ProcessMode::Version => {
            println!("sdkwork-api-cloud-gateway {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        ProcessMode::Serve | ProcessMode::MigrateDatabases => {}
    }

    install_process_crypto_provider()?;
    if process_mode == ProcessMode::MigrateDatabases {
        // SAFETY: this happens before the Tokio runtime or any worker thread is
        // created. The explicit lifecycle command must apply pending modules
        // even when the service runtime keeps auto-migration disabled.
        unsafe { env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true") };
        // Mark the process as the explicit lifecycle command so dependency-owned
        // production/staging guards (Cloud Router, Models, ...) admit this
        // operator path while plain runtime processes stay fail-closed.
        unsafe { env::set_var("SDKWORK_DATABASE_LIFECYCLE_COMMAND", "migrate") };
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
    publish_embedded_cloudrouter_ingress(&config);

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
    let mut mode = ProcessMode::Serve;
    for argument in args {
        let argument = argument.as_ref();
        if HELP_ARGS.contains(&argument) {
            return ProcessMode::Help;
        }
        if VERSION_ARGS.contains(&argument) {
            return ProcessMode::Version;
        }
        if argument == MIGRATE_DATABASES_ARG {
            mode = ProcessMode::MigrateDatabases;
        }
    }
    mode
}

fn print_help() {
    println!(
        "sdkwork-api-cloud-gateway {}\n\
         \n\
         Usage: sdkwork-api-cloud-gateway [OPTIONS]\n\
         \n\
         Options:\n\
           --config <PATH>       Load the gateway runtime TOML file\n\
           --migrate-databases   Run owner assembly database lifecycles and exit\n\
           -h, --help            Print help without initializing the runtime\n\
           -V, --version         Print version without initializing the runtime",
        env!("CARGO_PKG_VERSION")
    );
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

/// Publishes this process' ingress bind as the Cloud Router open-api ingress for
/// embedded consumers that reach the surface through the generated Rust SDK.
///
/// The SDK resolver (`sdkwork_agents_tool_cloudrouter::cloudrouter_base_url`)
/// maps `SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND` to a
/// loopback URL and falls back to the Cloud Router standalone default
/// `http://127.0.0.1:3900` when it is unset. The standalone Cloud Router
/// topology files declare the variable (`0.0.0.0:3900`, `127.0.0.1:3905`, ...),
/// so the split deployment works, but the collapsed platform gateway topology
/// never did — and the gateway's own bind is profile-dependent (`0.0.0.0:8080`
/// in container profiles, `3900`/`3906`/`3907` elsewhere). Every SDK call from
/// an embedded surface therefore dialled a port this process does not own and
/// failed with a transport connect error.
///
/// Explicit overrides still win: a split deployment keeps its own
/// `SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND` or
/// `SDKWORK_AGENTS_CLOUDROUTER_BASE_URL` and this default is skipped.
#[cfg(feature = "foundation-cloudrouter")]
fn publish_embedded_cloudrouter_ingress(config: &GatewayRuntimeConfig) {
    let composed_in_process = config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.workspace == "sdkwork-cloudrouter");
    if !composed_in_process {
        return;
    }
    let Some(port) = bind_port(&config.server.bind) else {
        return;
    };
    if env::var_os("SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND").is_none() {
        // SAFETY: this runs on the gateway runtime thread before the HTTP
        // listener, the embedded dependency assemblies, and any request worker
        // read the process environment.
        unsafe {
            env::set_var(
                "SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND",
                &config.server.bind,
            )
        };
    }
    // The generations media adapter carries the same standalone-port default
    // (`GENERATIONS_MEDIA_GATEWAY_BASE_URL`), and the AIoT intelligence bridge
    // requires `SDKWORK_CLOUDROUTER_APPLICATION_OPEN_HTTP_URL`; both must point
    // at this process' open-api listener in the collapsed topology.
    if env::var_os("GENERATIONS_MEDIA_GATEWAY_BASE_URL").is_none() {
        // SAFETY: same single-threaded bootstrap window as above.
        unsafe {
            env::set_var(
                "GENERATIONS_MEDIA_GATEWAY_BASE_URL",
                format!("http://127.0.0.1:{port}"),
            )
        };
    }
    if env::var_os("SDKWORK_CLOUDROUTER_APPLICATION_OPEN_HTTP_URL").is_none() {
        // SAFETY: same single-threaded bootstrap window as above.
        unsafe {
            env::set_var(
                "SDKWORK_CLOUDROUTER_APPLICATION_OPEN_HTTP_URL",
                format!("http://127.0.0.1:{port}"),
            )
        };
    }
    info!(
        bind = %config.server.bind,
        cloudrouter_base_url = %format!("http://127.0.0.1:{port}"),
        "embedded cloud router open-api ingress published for in-process SDK consumers"
    );
}

/// Compiles the publisher away when the Cloud Router surface is not linked in:
/// nothing composes the surface, so no SDK consumer needs its ingress.
#[cfg(not(feature = "foundation-cloudrouter"))]
fn publish_embedded_cloudrouter_ingress(_config: &GatewayRuntimeConfig) {}

/// Extracts the TCP port of an `address:port` bind string.
#[cfg(feature = "foundation-cloudrouter")]
fn bind_port(bind: &str) -> Option<&str> {
    let (_, port) = bind.rsplit_once(':')?;
    if port.is_empty() || !port.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    Some(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes env-mutating tests: `std::env` is process-global, so
    /// parallel tests would race on the same variables.
    #[cfg(feature = "foundation-cloudrouter")]
    fn env_test_guard() -> &'static std::sync::Mutex<()> {
        static GUARD: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        GUARD.get_or_init(|| std::sync::Mutex::new(()))
    }

    #[cfg(feature = "foundation-cloudrouter")]
    fn embedded_cloudrouter_test_config() -> GatewayRuntimeConfig {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        GatewayRuntimeConfig::from_application_root(root)
            .expect("gateway application manifest parses")
    }

    /// Restores the three published variables so other tests never observe a
    /// leaked default.
    #[cfg(feature = "foundation-cloudrouter")]
    struct PublishedEnvGuard {
        previous: [(String, Option<String>); 3],
    }

    #[cfg(feature = "foundation-cloudrouter")]
    impl PublishedEnvGuard {
        const KEYS: [&str; 3] = [
            "SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND",
            "GENERATIONS_MEDIA_GATEWAY_BASE_URL",
            "SDKWORK_CLOUDROUTER_APPLICATION_OPEN_HTTP_URL",
        ];

        fn capture_and_clear() -> Self {
            let mut previous: [(String, Option<String>); 3] =
                std::array::from_fn(|_| (String::new(), None));
            for (slot, key) in Self::KEYS.iter().enumerate() {
                previous[slot] = (key.to_string(), std::env::var(key).ok());
                // SAFETY: test-only, serialized by env_test_guard.
                unsafe { std::env::remove_var(key) };
            }
            Self { previous }
        }
    }

    #[cfg(feature = "foundation-cloudrouter")]
    impl Drop for PublishedEnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.previous {
                // SAFETY: test-only, serialized by env_test_guard.
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    #[cfg(feature = "foundation-cloudrouter")]
    #[test]
    fn publishes_loopback_ingress_for_embedded_cloudrouter_consumers() {
        let _guard = env_test_guard().lock().unwrap();
        let _env = PublishedEnvGuard::capture_and_clear();
        let config = embedded_cloudrouter_test_config();
        assert!(
            config
                .dependency_surfaces
                .iter()
                .any(|surface| surface.workspace == "sdkwork-cloudrouter"),
            "test application manifest must compose the cloudrouter surface"
        );

        publish_embedded_cloudrouter_ingress(&config);

        let port = bind_port(&config.server.bind).expect("test bind carries a port");
        assert_eq!(
            std::env::var("SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND")
                .expect("ingress bind published"),
            config.server.bind
        );
        for key in [
            "GENERATIONS_MEDIA_GATEWAY_BASE_URL",
            "SDKWORK_CLOUDROUTER_APPLICATION_OPEN_HTTP_URL",
        ] {
            assert_eq!(
                std::env::var(key).expect(key),
                format!("http://127.0.0.1:{port}"),
                "{key} must target this process' own listener"
            );
        }
    }

    #[cfg(feature = "foundation-cloudrouter")]
    #[test]
    fn explicit_ingress_overrides_win_over_published_defaults() {
        let _guard = env_test_guard().lock().unwrap();
        let _env = PublishedEnvGuard::capture_and_clear();
        let config = embedded_cloudrouter_test_config();

        // Split deployment: CloudRouter runs as its own process elsewhere.
        // SAFETY: test-only, serialized by env_test_guard.
        unsafe {
            std::env::set_var(
                "SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND",
                "cloudrouter.internal:443",
            )
        };
        publish_embedded_cloudrouter_ingress(&config);

        assert_eq!(
            std::env::var("SDKWORK_CLOUDROUTER_ROUTER_APPLICATION_PUBLIC_INGRESS_BIND")
                .expect("explicit override preserved"),
            "cloudrouter.internal:443"
        );
    }

    #[cfg(feature = "foundation-cloudrouter")]
    #[test]
    fn extracts_the_loopback_port_from_every_bind_form() {
        assert_eq!(bind_port("0.0.0.0:8080"), Some("8080"));
        assert_eq!(bind_port("127.0.0.1:3900"), Some("3900"));
        assert_eq!(bind_port("[::]:3907"), Some("3907"));
        assert_eq!(bind_port("gateway:3906"), Some("3906"));
        assert_eq!(bind_port("no-port"), None);
        assert_eq!(bind_port("host:"), None);
        assert_eq!(bind_port("host:39x0"), None);
    }

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

    #[test]
    fn recognizes_side_effect_free_process_information_modes() {
        assert_eq!(process_mode_from_args(["--help"]), ProcessMode::Help);
        assert_eq!(process_mode_from_args(["-h"]), ProcessMode::Help);
        assert_eq!(process_mode_from_args(["--version"]), ProcessMode::Version);
        assert_eq!(process_mode_from_args(["-V"]), ProcessMode::Version);
        assert_eq!(
            process_mode_from_args([MIGRATE_DATABASES_ARG, "--help"]),
            ProcessMode::Help
        );
    }
}
