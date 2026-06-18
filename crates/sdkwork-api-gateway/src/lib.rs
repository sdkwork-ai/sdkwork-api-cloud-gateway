//! Runtime router builders for the SDKWork API Gateway.

mod diagnostics;
mod error;
mod health;
mod metrics;
mod openapi;
mod policy;
mod resilience;
mod runtime;
mod security;
mod traffic;

pub use error::GatewayRuntimeError;
#[cfg(feature = "foundation-appbase")]
pub use runtime::{
    build_embedded_sdkwork_appbase_app_api_router,
    build_embedded_sdkwork_appbase_app_api_router_with_runtime_context,
    build_embedded_sdkwork_appbase_backend_api_router,
    build_sdkwork_api_gateway_router_with_embedded_appbase,
};
pub use runtime::{
    build_sdkwork_api_gateway_api_server_router, build_sdkwork_api_gateway_router,
    build_sdkwork_api_gateway_router_with_embedded_routers,
    build_sdkwork_api_gateway_router_with_registry, GatewayRuntime,
};
#[cfg(feature = "foundation-appbase")]
pub use sdkwork_router_iam_app_api::SdkworkAppbaseLocalIamRuntimeContext;
