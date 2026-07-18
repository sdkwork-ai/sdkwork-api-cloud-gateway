//! Runtime router builders for the SDKWork API Gateway.

mod diagnostics;
mod embedded_dependency_routes;
mod error;
mod health;
#[cfg(feature = "foundation-appbase")]
mod iam_application_bootstrap;
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
pub use runtime::{
    build_embedded_sdkwork_iam_app_api_router, build_embedded_sdkwork_iam_backend_api_router,
    build_sdkwork_api_cloud_gateway_router_with_embedded_appbase,
};
pub use runtime::{
    build_sdkwork_api_cloud_gateway_router,
    build_sdkwork_api_cloud_gateway_router_with_embedded_routers,
    build_sdkwork_api_cloud_gateway_router_with_registry, GatewayRuntime,
};
pub use service_config_loader::{config_path_from_args, load_gateway_config};
pub use web_framework::{
    finalize_gateway_router, finalize_gateway_router_async, parse_web_framework_toggle,
    web_framework_enabled,
};
