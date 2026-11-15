//! Console-facing CORS settings API for the platform edge.
//!
//! `GET`/`PATCH /backend/v3/api/system/cors/settings` lets an operator manage
//! the platform's browser origin allowlist from `/admin/settings` instead of
//! editing configuration or redeploying. The row it writes is the same
//! `framework_cors_policy` row the edge's periodic scan reads
//! ([`crate::cors_dynamic`]), so a save is visible within one refresh interval
//! on every process and node of the deployment.
//!
//! Authorization matches the other `/backend/v3/api/system/*/settings`
//! endpoints: the composed pipeline authenticates the console's dual token and
//! resolves the tenant, and this module refuses any tenant other than the
//! configured platform scope. That keeps the platform-scoped CORS row out of
//! reach of an ordinary tenant administrator without introducing a new
//! permission that deployments do not yet grant.
//!
//! Entries accept exact `http(s)` origins and host wildcards:
//!
//! - `https://mall.sdkwork.com` — one exact origin;
//! - `https://*.sdkwork.com` — one sub-domain label;
//! - `https://**.sdkwork.com` — one or more labels (second, third, fourth and
//!   deeper levels), evaluated by `sdkwork_web_core::cors_wildcard`.

use axum::{
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_utils_rust::SdkWorkApiResponse;
use sdkwork_web_contract::{HttpMethod, HttpRoute};
use sdkwork_web_core::{problem_response, WebFrameworkError, WebRequestContext};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use crate::cors_dynamic::{self, CorsPolicyScope};

/// Collection path of the platform CORS settings document.
pub(crate) const CORS_SETTINGS_PATH: &str = "/backend/v3/api/system/cors/settings";

/// Maximum number of allowlist entries an operator can declare. Guards the
/// database row and the per-request matcher against accidental floods.
const MAX_ALLOWED_ORIGINS: usize = 512;
const MAX_ORIGIN_LENGTH: usize = 2048;

/// Route manifest of the platform CORS settings surface.
pub(crate) const CORS_SETTINGS_ROUTES: &[HttpRoute] = &[
    HttpRoute::dual_token(
        HttpMethod::Get,
        CORS_SETTINGS_PATH,
        "system",
        "system.cors.settings.retrieve",
    ),
    HttpRoute::dual_token(
        HttpMethod::Patch,
        CORS_SETTINGS_PATH,
        "system",
        "system.cors.settings.update",
    ),
];

#[derive(Clone)]
struct CorsAdminState {
    pool: PgPool,
    scope: CorsPolicyScope,
}

/// Builds the platform CORS settings router.
pub(crate) fn cors_admin_router(pool: PgPool, config: &GatewayRuntimeConfig) -> Router {
    let state = CorsAdminState {
        pool,
        scope: CorsPolicyScope::from_config(config),
    };
    Router::new()
        .route(
            CORS_SETTINGS_PATH,
            get(fetch_cors_settings).patch(update_cors_settings),
        )
        .with_state(state)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CorsSettingsItem {
    allow_all_origins: bool,
    allowed_origins: Vec<String>,
    allow_credentials: bool,
    /// API_SPEC §13: int64 wire values are decimal strings.
    #[serde(with = "sdkwork_utils_rust::serde_int64")]
    version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCorsSettingsRequest {
    #[serde(default)]
    allow_all_origins: bool,
    #[serde(default)]
    allowed_origins: Vec<String>,
    #[serde(default = "default_true")]
    allow_credentials: bool,
}

fn default_true() -> bool {
    true
}

async fn fetch_cors_settings(
    ctx: WebRequestContext,
    State(state): State<CorsAdminState>,
) -> Response {
    if let Err(response) = authorize(&ctx, &state) {
        return response;
    }
    match cors_dynamic::load_row(&state.pool, &state.scope).await {
        Ok(Some(row)) => success_response(
            &ctx,
            json!({
                "item": CorsSettingsItem {
                    allow_all_origins: row.allow_all_origins != 0,
                    allowed_origins: cors_dynamic::decode_allowed_origins(
                        row.allowed_origins.as_str(),
                    ),
                    allow_credentials: row.allow_credentials != 0,
                    version: i64::from(row.version),
                }
            }),
        ),
        Ok(None) => success_response(
            &ctx,
            json!({
                "item": CorsSettingsItem {
                    allow_all_origins: false,
                    allowed_origins: Vec::new(),
                    allow_credentials: true,
                    version: 0,
                }
            }),
        ),
        Err(error) => {
            tracing::error!(
                target: "sdkwork.api.cloud_gateway",
                %error,
                "platform CORS settings read failed"
            );
            problem_for(
                &ctx,
                WebFrameworkError::dependency_unavailable(
                    "the CORS policy store is temporarily unavailable",
                ),
            )
        }
    }
}

async fn update_cors_settings(
    ctx: WebRequestContext,
    State(state): State<CorsAdminState>,
    Json(body): Json<UpdateCorsSettingsRequest>,
) -> Response {
    if let Err(response) = authorize(&ctx, &state) {
        return response;
    }
    let fields = match normalize_settings(body, state.scope.production_like) {
        Ok(fields) => fields,
        Err(message) => return problem_for(&ctx, WebFrameworkError::bad_request(message)),
    };
    match cors_dynamic::upsert_row(
        &state.pool,
        &state.scope,
        fields.allow_all_origins,
        fields.allowed_origins.as_slice(),
        fields.allow_credentials,
    )
    .await
    {
        Ok(row) => success_response(
            &ctx,
            json!({
                "item": CorsSettingsItem {
                    allow_all_origins: row.allow_all_origins != 0,
                    allowed_origins: cors_dynamic::decode_allowed_origins(
                        row.allowed_origins.as_str(),
                    ),
                    allow_credentials: row.allow_credentials != 0,
                    version: i64::from(row.version),
                }
            }),
        ),
        Err(error) => {
            tracing::error!(
                target: "sdkwork.api.cloud_gateway",
                %error,
                "platform CORS settings write failed"
            );
            problem_for(
                &ctx,
                WebFrameworkError::dependency_unavailable(
                    "the CORS policy store is temporarily unavailable",
                ),
            )
        }
    }
}

/// Restricts the platform-scoped row to the configured platform tenant.
fn authorize(ctx: &WebRequestContext, state: &CorsAdminState) -> Result<(), Response> {
    let tenant = match ctx.require_tenant_id() {
        Ok(tenant) => tenant,
        Err(error) => return Err(problem_for(ctx, error)),
    };
    if tenant != state.scope.tenant_id {
        return Err(problem_for(
            ctx,
            WebFrameworkError::forbidden(
                "the platform CORS policy is scoped to the platform tenant",
            ),
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct NormalizedSettings {
    allow_all_origins: bool,
    allowed_origins: Vec<String>,
    allow_credentials: bool,
}

fn normalize_settings(
    body: UpdateCorsSettingsRequest,
    production_like: bool,
) -> Result<NormalizedSettings, String> {
    if body.allow_all_origins && production_like {
        return Err(
            "allowAllOrigins is not permitted in a production-like environment; declare exact origins or host wildcards such as https://**.sdkwork.com"
                .to_owned(),
        );
    }
    if body.allowed_origins.len() > MAX_ALLOWED_ORIGINS {
        return Err(format!(
            "allowedOrigins must contain at most {MAX_ALLOWED_ORIGINS} entries"
        ));
    }
    let mut normalized: Vec<String> = Vec::with_capacity(body.allowed_origins.len());
    for origin in body.allowed_origins {
        let origin = normalize_origin(origin.as_str())?;
        if !normalized.iter().any(|known| known == &origin) {
            normalized.push(origin);
        }
    }
    if !body.allow_all_origins && normalized.is_empty() {
        return Err(
            "allowedOrigins must contain at least one exact origin or host wildcard; this settings row only extends the static CORS floor"
                .to_owned(),
        );
    }
    Ok(NormalizedSettings {
        allow_all_origins: body.allow_all_origins,
        allowed_origins: normalized,
        allow_credentials: body.allow_credentials,
    })
}

fn normalize_origin(origin: &str) -> Result<String, String> {
    if origin.is_empty() || origin.trim() != origin {
        return Err("an allowed origin must be non-empty and trimmed".to_owned());
    }
    if origin.len() > MAX_ORIGIN_LENGTH {
        return Err(format!(
            "allowed origin `{origin}` exceeds {MAX_ORIGIN_LENGTH} characters"
        ));
    }
    if sdkwork_web_core::cors_wildcard::is_host_wildcard_origin_pattern(origin) {
        return Ok(origin.to_owned());
    }
    if origin.contains('*') {
        return Err(format!(
            "allowed origin `{origin}` is not a supported host wildcard; use https://*.example.com or https://**.example.com"
        ));
    }
    let Some(authority) = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
    else {
        return Err(format!(
            "allowed origin `{origin}` must use the http or https scheme"
        ));
    };
    if authority.is_empty()
        || authority.contains(['/', '?', '#', '@'])
        || authority.chars().any(char::is_whitespace)
    {
        return Err(format!(
            "allowed origin `{origin}` must be an exact origin without credentials, path, query or fragment"
        ));
    }
    Ok(origin.to_owned())
}

fn success_response(ctx: &WebRequestContext, data: serde_json::Value) -> Response {
    let trace_id = ctx.resolved_trace_id();
    let envelope = SdkWorkApiResponse::success(data, trace_id.clone());
    let mut response = (StatusCode::OK, Json(envelope)).into_response();
    if let Ok(value) = HeaderValue::from_str(trace_id.as_str()) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-sdkwork-trace-id"), value);
    }
    response
}

fn problem_for(ctx: &WebRequestContext, error: WebFrameworkError) -> Response {
    problem_response(&error, ctx.problem_correlation())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(origins: &[&str], allow_all: bool) -> UpdateCorsSettingsRequest {
        UpdateCorsSettingsRequest {
            allow_all_origins: allow_all,
            allowed_origins: origins.iter().map(|value| (*value).to_owned()).collect(),
            allow_credentials: true,
        }
    }

    #[test]
    fn accepts_exact_origins_and_host_wildcards_at_every_depth() {
        let normalized = normalize_settings(
            body(
                &[
                    "https://platform.birdcoder.com",
                    "https://*.sdkwork.com",
                    "https://**.birdcoder.com",
                ],
                false,
            ),
            true,
        )
        .expect("valid settings");
        assert_eq!(3, normalized.allowed_origins.len());
        assert!(normalized.allow_credentials);
    }

    #[test]
    fn rejects_unsafe_origins() {
        for origin in [
            "*",
            "https://*",
            "https://**.com",
            "https://evil.example.com/path",
            "https://user@evil.example.com",
            "javascript:alert(1)",
            "https://mall .sdkwork.com",
        ] {
            let error = normalize_settings(body(&[origin], false), false)
                .expect_err("origin must be rejected");
            assert!(
                error.contains(origin) || error.contains("allowedOrigins"),
                "unexpected message `{error}` for `{origin}`"
            );
        }
    }

    #[test]
    fn rejects_allow_all_origins_in_production_like_environments() {
        let error = normalize_settings(body(&["https://mall.sdkwork.com"], true), true)
            .expect_err("production must not allow all origins");
        assert!(error.contains("allowAllOrigins"));
        normalize_settings(body(&["https://mall.sdkwork.com"], true), false)
            .expect("development may allow all origins");
    }

    #[test]
    fn requires_at_least_one_origin_when_allow_all_is_off() {
        let error = normalize_settings(body(&[], false), true).expect_err("empty allowlist");
        assert!(error.contains("allowedOrigins"));
    }

    #[test]
    fn deduplicates_entries_in_order() {
        let normalized = normalize_settings(
            body(
                &[
                    "https://mall.sdkwork.com",
                    "https://mall.sdkwork.com",
                    "https://**.sdkwork.com",
                ],
                false,
            ),
            true,
        )
        .expect("valid settings");
        assert_eq!(
            vec![
                "https://mall.sdkwork.com".to_owned(),
                "https://**.sdkwork.com".to_owned()
            ],
            normalized.allowed_origins
        );
    }
}
