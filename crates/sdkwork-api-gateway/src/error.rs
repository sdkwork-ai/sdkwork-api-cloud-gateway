use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayRuntimeError {
    #[error("required dependency `{service_id}` has no upstream in split mode")]
    MissingDependencyUpstream { service_id: String },
    #[error("required dependency `{service_id}` has no embedded router in embedded mode")]
    MissingEmbeddedRouter { service_id: String },
    #[error("failed to build HTTP client: {0}")]
    HttpClient(#[from] reqwest::Error),
}
