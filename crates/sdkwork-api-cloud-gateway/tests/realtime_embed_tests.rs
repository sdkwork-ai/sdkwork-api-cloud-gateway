//! Platform gateway realtime hosting evidence
//! (`ADR-20260809-platform-gateway-realtime-hosting`): the gateway process
//! embeds the IM session-gateway realtime plane and terminates the WebSocket
//! upgrade on its single HTTP listener plus declared client link transports.
//!
//! The full embedded test requires an explicit PostgreSQL integration profile
//! (same convention as `embedded_dependency_routes` integration tests); it is
//! `#[ignore]`d for default runs. The unit tests run everywhere.

#![cfg(all(feature = "foundation-appbase", feature = "foundation-im-realtime"))]

use std::{
    env,
    net::{SocketAddr, TcpListener as StdTcpListener},
    path::Path,
    time::Duration,
};

use sdkwork_api_cloud_gateway::{
    build_sdkwork_api_cloud_gateway_router_with_embedded_realtime, GatewayShutdown,
};
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use tokio::net::TcpListener;

/// Reserves a free loopback port by binding and releasing it.
fn ephemeral_port() -> u16 {
    let listener = StdTcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("local address").port()
}

#[test]
fn empty_gateway_shutdown_handle_drains_without_error() {
    // The shutdown path must be safe when no realtime plane was retained.
    let shutdown = GatewayShutdown::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build test runtime");
    runtime.block_on(async move {
        shutdown.shutdown().await;
    });
}

#[tokio::test]
#[ignore = "requires an explicit, initialized PostgreSQL integration profile and IM realtime env"]
async fn embedded_realtime_plane_serves_websocket_and_tcp_link_transport() {
    env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "false");
    env::set_var("SDKWORK_DATABASE_SEED_ON_BOOT", "false");
    env::set_var("SDKWORK_IM_ENVIRONMENT", "development");
    // Reserve the r2d2 budget the IM process pools need before any pool
    // creation call in this process.
    if env::var_os("SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT").is_none() {
        env::set_var("SDKWORK_DATABASE_TEMPORARY_DRIVER_POOL_COUNT", "10");
    }

    // Declare a TCP client link transport; the embedded plane must bind it.
    let tcp_port = ephemeral_port();
    env::set_var(
        "SDKWORK_IM_REALTIME_TCP_BIND_ADDR",
        format!("127.0.0.1:{tcp_port}"),
    );
    env::remove_var("SDKWORK_IM_REALTIME_CLUSTER_BUS_URL");
    env::remove_var("SDKWORK_IM_REALTIME_ROUTE_STORE_URL");

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut config = GatewayRuntimeConfig::from_application_root(root)
        .expect("gateway application manifest parses");
    config.realtime.enabled = true;
    config
        .dependency_surfaces
        .retain(|surface| matches!(surface.workspace.as_str(), "sdkwork-iam" | "sdkwork-im"));

    let (router, shutdown) = build_sdkwork_api_cloud_gateway_router_with_embedded_realtime(config)
        .await
        .expect("build gateway with embedded IM realtime plane");

    let http_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway HTTP listener");
    let http_addr: SocketAddr = http_listener.local_addr().expect("HTTP local address");
    let server = tokio::spawn(async move {
        axum::serve(http_listener, router)
            .with_graceful_shutdown(std::future::pending::<()>())
            .await
            .expect("gateway HTTP server");
    });

    // WebSocket upgrade completes a handshake through the single HTTP
    // listener at the declared realtime path.
    let ws_url = format!("ws://{http_addr}/im/v3/api/realtime/ws");
    let (stream, response) = tokio_tungstenite::connect_async(ws_url)
        .await
        .expect("websocket upgrade completes a handshake");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::SWITCHING_PROTOCOLS,
        "the realtime plane must serve the websocket upgrade, not a gateway fallback"
    );
    drop(stream);

    // The declared TCP link transport listener accepts connections.
    tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::TcpStream::connect(("127.0.0.1", tcp_port)),
    )
    .await
    .expect("TCP link transport listener accepts connections")
    .expect("TCP connect succeeds");

    server.abort();
    let _ = server.await;
    // Drains the plane: marks the node draining, fences and releases routes,
    // shuts down link transports and the cluster subscriber.
    shutdown.shutdown().await;
}
