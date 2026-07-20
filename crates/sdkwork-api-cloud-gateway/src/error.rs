use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayRuntimeError {
    #[error("failed to create the process-shared database pool: {message}")]
    DatabasePoolBootstrap { message: String },
    #[error("failed to bootstrap database module `{module_id}` from `{workspace}`: {message}")]
    EmbeddedDatabaseBootstrap {
        workspace: String,
        module_id: String,
        message: String,
    },
    #[error("required dependency `{service_id}` has no embedded router")]
    MissingEmbeddedRouter { service_id: String },
    #[error("failed to build embedded router for `{service_id}`: {message}")]
    EmbeddedRouterBuild { service_id: String, message: String },
}
