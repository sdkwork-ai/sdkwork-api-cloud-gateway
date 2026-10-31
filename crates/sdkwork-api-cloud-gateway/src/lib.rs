//! Runtime router builders for the SDKWork API Gateway.

mod comments_proxy;
/// Installer-time workspace PostgreSQL provisioning (role/database/schema/extensions).
pub mod database_provisioning;
/// Embedded dependency database lifecycle shared by platform runtime consumers.
pub mod dependency_error_observer;
mod diagnostics;
pub mod embedded_database_bootstrap;
/// Embedded dependency API contributions assembled from sibling workspaces.
pub mod embedded_dependency_routes;
mod error;
mod health;
mod metrics;
mod openapi;
mod policy;
mod runtime;
mod security;
mod service_config_loader;
mod traffic;
pub mod web_framework;

pub use error::GatewayRuntimeError;
#[cfg(feature = "foundation-appbase")]
pub use runtime::build_sdkwork_api_cloud_gateway_router_with_embedded_appbase;
#[cfg(all(feature = "foundation-appbase", feature = "foundation-im-realtime"))]
pub use runtime::build_sdkwork_api_cloud_gateway_router_with_embedded_realtime;
pub use runtime::{
    build_sdkwork_api_cloud_gateway_router,
    build_sdkwork_api_cloud_gateway_router_with_embedded_routers,
    build_sdkwork_api_cloud_gateway_router_with_registry, GatewayRuntime, GatewayShutdown,
};
pub use service_config_loader::{config_path_from_args, load_gateway_config};
pub use web_framework::{
    finalize_gateway_router, finalize_gateway_router_async, parse_web_framework_toggle,
    web_framework_enabled,
};
