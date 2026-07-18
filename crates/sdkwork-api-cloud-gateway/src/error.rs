use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayRuntimeError {
    #[error("required dependency `{service_id}` has no embedded router")]
    MissingEmbeddedRouter { service_id: String },
    #[error("failed to build embedded router for `{service_id}`: {message}")]
    EmbeddedRouterBuild { service_id: String, message: String },
}
