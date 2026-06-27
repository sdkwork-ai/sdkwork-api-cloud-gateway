//! Embedded dependency routers for platform collapsed ingress (§5.7.2).
//! Each registered router is business-only; host routes own `/healthz`, `/livez`, `/readyz`.

use std::collections::HashMap;

use axum::Router;
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;

use crate::GatewayRuntimeError;

pub async fn bootstrap_embedded_dependency_routers(
    config: &GatewayRuntimeConfig,
) -> Result<HashMap<String, Router>, GatewayRuntimeError> {
    let mut routers = HashMap::new();

    #[cfg(feature = "foundation-appbase")]
    {
        use sdkwork_api_cloud_gateway_config::APPBASE_APP_API_SERVICE_ID;

        if dependency_surface_enabled(config, APPBASE_APP_API_SERVICE_ID) {
            let router = crate::runtime::build_embedded_sdkwork_iam_app_api_router().await?;
            routers.insert(APPBASE_APP_API_SERVICE_ID.to_owned(), router);
        }
    }

    #[cfg(feature = "foundation-course")]
    {
        use sdkwork_api_cloud_gateway_config::{
            COURSE_APP_API_SERVICE_ID, COURSE_BACKEND_API_SERVICE_ID,
        };
        if dependency_surface_enabled(config, COURSE_APP_API_SERVICE_ID)
            || dependency_surface_enabled(config, COURSE_BACKEND_API_SERVICE_ID)
        {
            let assembly =
                sdkwork_course_gateway_assembly::assemble_application_business_router()
                    .await
                    .map_err(|message| GatewayRuntimeError::EmbeddedRouterBuild {
                        service_id: COURSE_APP_API_SERVICE_ID.to_owned(),
                        message,
                    })?;
            let business_router = assembly.router;
            if dependency_surface_enabled(config, COURSE_APP_API_SERVICE_ID) {
                routers.insert(COURSE_APP_API_SERVICE_ID.to_owned(), business_router.clone());
            }
            if dependency_surface_enabled(config, COURSE_BACKEND_API_SERVICE_ID) {
                routers.insert(COURSE_BACKEND_API_SERVICE_ID.to_owned(), business_router);
            }
        }
    }

    Ok(routers)
}

fn dependency_surface_enabled(config: &GatewayRuntimeConfig, service_id: &str) -> bool {
    config
        .dependency_surfaces
        .iter()
        .any(|surface| surface.service_id == service_id)
}
