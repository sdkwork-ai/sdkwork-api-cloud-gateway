//! Gateway-issued open-api API keys (`iam_gateway_api_key`) and the
//! vendor-wire credential bridge.
//!
//! Cloud Router issues OpenAI-compatible gateway keys (`sk-`/`sp-` prefixed)
//! and stores only their HMAC-SHA256 hash in `iam_gateway_api_key.key_hash`
//! (peppered with `SDKWORK_CLOUDROUTER_API_KEY_PEPPER`). The platform gateway
//! composes that surface into the single Web Framework pipeline
//! (`APPLICATION_GATEWAY_SPEC.md` §2.3), so the framework's open-api API-key
//! resolver is the layer that must recognise the credential — `API_SPEC.md`
//! §827 requires the server to resolve tenant/organization/user from the
//! validated API-key record rather than from a client-supplied header.
//!
//! This module owns the two deterministic, security-critical pieces of that
//! path:
//!
//! * [`gateway_api_key_hash`] — the hash parity contract with Cloud Router.
//!   If this drifts, every `sk-` credential fails closed, so it is pinned by a
//!   known-answer test against an independently computed HMAC vector.
//! * [`bridge_vendor_authorization`] — the vendor wire bridge. The published
//!   contract of the vendor-compatible surface is `Authorization: Bearer
//!   <key>` (OpenAI/Anthropic wire, `API_SPEC.md` §4.5.2), while the framework
//!   profile that serves it reads `X-API-Key`. The bridge renames that one
//!   header for the vendor prefixes and leaves every other header untouched.
//! * [`GatewayApiKeyResolver`] — the credential authority for both vendor
//!   credential kinds: an API-key-prefixed value resolves against
//!   `iam_gateway_api_key`, and a signed-in user's login token resolves through
//!   the IAM session auth-token channel. The dependency's own handler then picks
//!   the subject's default upstream account group for the request's modality, so
//!   a logged-in user needs no separately issued key to use the relay.
//!
//! Both functions are pure and side-effect free so they can be unit tested
//! without a database or a listener; the resolver wiring that consumes them
//! lives next to the framework composition.

use axum::http::{header, HeaderMap, HeaderValue};
use sdkwork_web_core::{
    default_open_api_bearer_classifier, DynOpenApiBearerCredentialClassifier,
    OpenApiCredentialKind, WebFrameworkErrorKind,
};

/// Environment key that carries Cloud Router's API-key hashing pepper.
pub const API_KEY_PEPPER_ENV: &str = "SDKWORK_CLOUDROUTER_API_KEY_PEPPER";

/// Lowercase header name the canonical `api-key` open-api profile reads.
const API_KEY_HEADER: &str = "x-api-key";

/// Minimum pepper length accepted by Cloud Router's own configuration
/// (`ApiKeySecurityConfig`); a shorter value can never have produced a stored
/// hash, so treating it as unset keeps the resolver fail-closed.
const MIN_PEPPER_LEN: usize = 16;

/// Hashes a presented gateway API key exactly as Cloud Router does.
///
/// Cloud Router computes `hex(HMAC-SHA256(pepper, secret))`
/// (`HmacSha256ApiKeySecretHasher`) and compares it to `key_hash`. The gateway
/// must reproduce that byte-for-byte, so this delegates to the shared
/// `sdkwork_utils_rust::hmac_sha256` implementation instead of growing a second
/// one.
#[must_use]
pub fn gateway_api_key_hash(secret: &str, pepper: &str) -> String {
    sdkwork_utils_rust::hmac_sha256(secret.as_bytes(), pepper.as_bytes())
}

/// Reads the pepper from the process environment.
///
/// Returns `None` when the variable is absent, blank, or implausibly short; the
/// caller must then skip the gateway-key lookup and fall through to the
/// platform resolver rather than guessing.
#[must_use]
pub fn configured_api_key_pepper() -> Option<String> {
    let value = std::env::var(API_KEY_PEPPER_ENV).ok()?;
    let trimmed = value.trim();
    if trimmed.len() < MIN_PEPPER_LEN {
        return None;
    }
    Some(trimmed.to_owned())
}

/// Outcome of inspecting a request for a vendor-compatible credential.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorCredentialBridge {
    /// No `Authorization: Bearer` credential was present; nothing changed.
    NoCredential,
    /// The bearer credential was moved to `X-API-Key`.
    Bridged,
    /// An `X-API-Key` credential was already present; the vendor header was
    /// left untouched so the framework's contamination guard can decide.
    AlreadyCanonical,
}

/// Presents a vendor `Authorization: Bearer <key>` credential as the canonical
/// `X-API-Key` credential.
///
/// OpenAI-compatible clients (and Cloud Router's own documentation) authenticate
/// with `Authorization: Bearer <key>`; the `api-key-or-dual-token` profile that
/// serves those operations reads `X-API-Key`. Rewriting the header keeps the
/// published wire working without relaxing the framework profile.
///
/// Only a `Bearer` scheme is bridged: any other scheme is left for the
/// framework to reject as the credential-profile violation it is. The
/// `Authorization` header is removed when bridged, because a request carrying
/// both headers is exactly the contamination the guard must reject.
pub fn bridge_vendor_authorization(headers: &mut HeaderMap) -> VendorCredentialBridge {
    if headers.contains_key(API_KEY_HEADER) {
        return VendorCredentialBridge::AlreadyCanonical;
    }
    let Some(raw) = headers.get(header::AUTHORIZATION) else {
        return VendorCredentialBridge::NoCredential;
    };
    let Ok(raw) = raw.to_str() else {
        return VendorCredentialBridge::NoCredential;
    };
    let Some((scheme, credential)) = raw.split_once(' ') else {
        return VendorCredentialBridge::NoCredential;
    };
    if !scheme.eq_ignore_ascii_case("bearer") {
        return VendorCredentialBridge::NoCredential;
    }
    let credential = credential.trim();
    if credential.is_empty() {
        return VendorCredentialBridge::NoCredential;
    }
    let Ok(value) = HeaderValue::from_str(credential) else {
        return VendorCredentialBridge::NoCredential;
    };
    headers.remove(header::AUTHORIZATION);
    headers.insert(header::HeaderName::from_static(API_KEY_HEADER), value);
    VendorCredentialBridge::Bridged
}

/// Whether `path` belongs to a vendor-compatible open-api prefix.
///
/// Prefixes come from the composed dependency surfaces, so a prefix only ever
/// matches a surface this build actually serves. Matching is segment-aware:
/// `/v1` covers `/v1/chat/completions` but never `/v10/...`.
#[must_use]
pub fn is_vendor_open_api_path(path: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|prefix| {
        let prefix = prefix.trim_end_matches('/');
        prefix.is_empty()
            || path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Dependency surfaces of this build that speak a vendor-compatible open-api wire.
///
/// Only Cloud Router's open-api prefixes are bridged: its published contract is
/// the upstream wire (`API_SPEC.md` §4.5.2), while SDKWork-owned open-api
/// surfaces keep their canonical `X-API-Key` contract untouched.
#[must_use]
pub fn vendor_open_api_prefixes(
    config: &sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig,
) -> Vec<String> {
    config
        .dependency_surfaces
        .iter()
        .filter(|surface| surface.workspace == "sdkwork-cloudrouter")
        .filter(|surface| {
            matches!(
                surface.surface.trim().to_ascii_lowercase().as_str(),
                "open" | "open-api" | "gateway-api"
            )
        })
        .map(|surface| surface.api_prefix.clone())
        .collect()
}

fn web_environment(value: &str) -> sdkwork_web_core::WebEnvironment {
    match value.trim().to_ascii_lowercase().as_str() {
        "prod" | "production" => sdkwork_web_core::WebEnvironment::Prod,
        "dev" | "development" | "local" => sdkwork_web_core::WebEnvironment::Dev,
        _ => sdkwork_web_core::WebEnvironment::Test,
    }
}

fn web_deployment_mode(
    environment: &sdkwork_web_core::WebEnvironment,
) -> sdkwork_web_core::WebDeploymentMode {
    match environment {
        sdkwork_web_core::WebEnvironment::Prod => sdkwork_web_core::WebDeploymentMode::Saas,
        _ => sdkwork_web_core::WebDeploymentMode::Local,
    }
}

/// Resolves a presented gateway API key against `iam_gateway_api_key`.
///
/// Disabled (returns `Ok(None)`, i.e. fail-open to the platform resolver) when
/// the deployment has no PostgreSQL pool or the pepper is not configured: a
/// gateway that cannot reason about these keys must not invent a principal.
#[derive(Clone)]
pub struct GatewayApiKeyLookup {
    pool: Option<std::sync::Arc<sqlx::PgPool>>,
    pepper: Option<std::sync::Arc<str>>,
}

impl GatewayApiKeyLookup {
    #[must_use]
    pub fn new(
        pool: Option<std::sync::Arc<sqlx::PgPool>>,
        pepper: Option<std::sync::Arc<str>>,
    ) -> Self {
        Self { pool, pepper }
    }

    /// Validates one credential and projects the framework principal.
    pub async fn resolve(
        &self,
        secret: &str,
        app_id: &str,
        environment: sdkwork_web_core::WebEnvironment,
        deployment_mode: sdkwork_web_core::WebDeploymentMode,
    ) -> Result<Option<sdkwork_web_core::WebRequestPrincipal>, sdkwork_web_core::WebFrameworkError>
    {
        let (Some(pool), Some(pepper)) = (self.pool.as_deref(), self.pepper.as_deref()) else {
            return Ok(None);
        };
        let secret = secret.trim();
        if secret.is_empty() {
            return Ok(None);
        }
        let key_hash = gateway_api_key_hash(secret, pepper);
        let row = sqlx::query(
            "SELECT id, tenant_id, organization_id, user_id, name \
             FROM iam_gateway_api_key \
             WHERE key_hash = $1 \
               AND status = 1 \
               AND deleted_at IS NULL \
               AND revoked_at IS NULL \
               AND (expire_at IS NULL OR expire_at::timestamptz > now()) \
             ORDER BY id DESC \
             LIMIT 1",
        )
        .bind(&key_hash)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            sdkwork_web_core::WebFrameworkError::dependency_unavailable(format!(
                "gateway api key lookup failed: {error}"
            ))
        })?;
        let Some(row) = row else {
            return Ok(None);
        };
        use sqlx::Row as _;
        let api_key_id: i64 = row.try_get("id").unwrap_or_default();
        let tenant_id: i64 = row.try_get("tenant_id").unwrap_or_default();
        let organization_id: i64 = row.try_get("organization_id").unwrap_or_default();
        let user_id: i64 = row.try_get("user_id").unwrap_or_default();
        let display_name: Option<String> = row.try_get::<Option<String>, _>("name").unwrap_or(None);
        if tenant_id <= 0 {
            // A gateway key without a tenant binding cannot produce an isolated
            // context; fall through instead of fabricating one.
            return Ok(None);
        }
        let organization_scope = if organization_id > 0 {
            Some(organization_id.to_string())
        } else {
            None
        };
        let principal = sdkwork_web_core::WebRequestPrincipal::builder()
            .tenant_id(tenant_id.to_string())
            .organization_id(organization_scope)
            .user_id(user_id.to_string())
            .app_id(app_id.to_owned())
            .environment(environment)
            .deployment_mode(deployment_mode)
            .auth_level(sdkwork_web_core::WebAuthLevel::ApiKey)
            .subject_type(sdkwork_web_core::WebSubjectType::ApiKey)
            .api_key_id(Some(api_key_id.to_string()))
            .display_name(display_name)
            .build();
        Ok(Some(principal))
    }
}

/// Framework resolver that adds gateway API keys to the platform API-key channel.
///
/// Every other credential channel delegates unchanged, so this cannot weaken or
/// reroute the platform's own open-api authentication.
#[derive(Clone)]
pub struct GatewayApiKeyResolver<R> {
    inner: R,
    lookup: GatewayApiKeyLookup,
    app_id: std::sync::Arc<str>,
    environment: sdkwork_web_core::WebEnvironment,
    deployment_mode: sdkwork_web_core::WebDeploymentMode,
    bearer_classifier: DynOpenApiBearerCredentialClassifier,
}

impl<R> GatewayApiKeyResolver<R> {
    #[must_use]
    pub fn new(
        inner: R,
        lookup: GatewayApiKeyLookup,
        app_id: impl Into<std::sync::Arc<str>>,
        environment: sdkwork_web_core::WebEnvironment,
    ) -> Self {
        let deployment_mode = web_deployment_mode(&environment);
        Self {
            inner,
            lookup,
            app_id: app_id.into(),
            environment,
            deployment_mode,
            bearer_classifier: default_open_api_bearer_classifier(),
        }
    }

    /// Whether a presented credential is an SDKWork login auth token rather
    /// than an API key.
    ///
    /// The vendor-compatible wire (Cloud Router's `/v1` face) publishes one
    /// `Authorization: Bearer` credential and lets a signed-in user use the
    /// session's login token in place of an API key. The classifier is the same
    /// one Cloud Router's own handler applies — a value that does not carry an
    /// API-key prefix is a login token — so the gateway and the dependency agree
    /// on which channel a credential belongs to.
    fn is_login_token(&self, raw_credential: &str) -> bool {
        self.bearer_classifier.classify(raw_credential) == OpenApiCredentialKind::AuthToken
    }
}

#[async_trait::async_trait]
impl<R: sdkwork_web_core::WebRequestContextResolver> sdkwork_web_core::WebRequestContextResolver
    for GatewayApiKeyResolver<R>
{
    fn resolver_production_profile(&self) -> sdkwork_web_core::ResolverProductionProfile {
        self.inner.resolver_production_profile()
    }

    fn jwt_production_claim_policy(&self) -> Option<sdkwork_web_core::JwtProductionClaimPolicy> {
        self.inner.jwt_production_claim_policy()
    }

    fn uses_default_api_key_lookup(&self) -> bool {
        false
    }

    fn uses_default_oauth_token_lookup(&self) -> bool {
        self.inner.uses_default_oauth_token_lookup()
    }

    async fn resolve_api_key(
        &self,
        raw_api_key: &str,
    ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError> {
        if let Some(principal) = self
            .lookup
            .resolve(
                raw_api_key,
                self.app_id.as_ref(),
                self.environment.clone(),
                self.deployment_mode.clone(),
            )
            .await?
        {
            return Ok(principal);
        }
        // Vendor login-token channel: a signed-in SDKWork user authenticates the
        // vendor-compatible open-api with the session's login token instead of an
        // API key, so a credential without an API-key prefix resolves through the
        // session's auth-token lookup. An API-key-prefixed value never enters this
        // channel. A credential the session store does not know — and a
        // composition whose resolver wires no auth-token channel at all — falls
        // through to the platform API-key channel, which stays authoritative for
        // unprefixed key records and the development claim-string form.
        if self.is_login_token(raw_api_key) {
            match self.inner.resolve_bearer_auth_token(raw_api_key).await {
                Ok(principal) => return Ok(principal),
                Err(error)
                    if matches!(
                        error.kind,
                        WebFrameworkErrorKind::InvalidCredentials
                            | WebFrameworkErrorKind::DependencyUnavailable
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        self.inner.resolve_api_key(raw_api_key).await
    }

    async fn resolve_dual_token(
        &self,
        raw_auth_token: &str,
        raw_access_token: &str,
    ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError> {
        self.inner
            .resolve_dual_token(raw_auth_token, raw_access_token)
            .await
    }

    async fn resolve_access_token(
        &self,
        raw_access_token: &str,
    ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError> {
        self.inner.resolve_access_token(raw_access_token).await
    }

    async fn resolve_oauth_bearer(
        &self,
        raw_bearer_token: &str,
    ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError> {
        self.inner.resolve_oauth_bearer(raw_bearer_token).await
    }

    async fn resolve_bearer_auth_token(
        &self,
        raw_bearer_token: &str,
    ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError> {
        self.inner.resolve_bearer_auth_token(raw_bearer_token).await
    }

    async fn resolve_compatibility(
        &self,
        external_protocol_id: &str,
        credentials: &[sdkwork_web_core::CompatibilityCredential],
    ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError> {
        self.inner
            .resolve_compatibility(external_protocol_id, credentials)
            .await
    }
}

/// Wraps the platform resolver with the gateway-key channel.
///
/// The wrapper is always installed so the wiring is uniform; it only resolves a
/// credential when a PostgreSQL pool and the hashing pepper are both present.
#[must_use]
pub fn wrap_gateway_api_key_resolver<R>(
    config: &sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig,
    pool: Option<std::sync::Arc<sqlx::PgPool>>,
    inner: R,
) -> GatewayApiKeyResolver<R>
where
    R: sdkwork_web_core::WebRequestContextResolver,
{
    let environment = web_environment(config.service.environment.as_str());
    let pepper = configured_api_key_pepper().map(std::sync::Arc::from);
    GatewayApiKeyResolver::new(
        inner,
        GatewayApiKeyLookup::new(pool, pepper),
        APP_ID,
        environment,
    )
}

/// Stable application id attached to gateway-key principals.
const APP_ID: &str = "sdkwork-api-cloud-gateway";

#[cfg(test)]
mod tests {
    use super::*;

    const PEPPER: &str = "0123456789abcdef0123456789abcdef";

    fn headers(entries: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in entries {
            map.insert(
                header::HeaderName::from_bytes(name.as_bytes()).expect("header name"),
                HeaderValue::from_str(value).expect("header value"),
            );
        }
        map
    }

    #[test]
    fn gateway_key_hash_matches_cloud_router_hmac_sha256() {
        // Independently computed: HMAC-SHA256(key = pepper, message = secret).
        assert_eq!(
            "2a4016392fece2abff3028d97426d3618f4464fabd19776c73ae38c0fc7880dd",
            gateway_api_key_hash("sk-diagnostic-key", PEPPER)
        );
        assert_eq!(64, gateway_api_key_hash("sk-any", PEPPER).len());
    }

    #[test]
    fn gateway_key_hash_is_keyed_by_the_pepper() {
        assert_ne!(
            gateway_api_key_hash("sk-same", "0123456789abcdef0123456789abcdef"),
            gateway_api_key_hash("sk-same", "fedcba9876543210fedcba9876543210")
        );
    }

    #[test]
    fn bridge_moves_bearer_credential_to_api_key_header() {
        let mut map = headers(&[("authorization", "Bearer sk-live-123")]);
        assert_eq!(
            VendorCredentialBridge::Bridged,
            bridge_vendor_authorization(&mut map)
        );
        assert_eq!(
            "sk-live-123",
            map.get("x-api-key")
                .expect("bridged")
                .to_str()
                .expect("ascii")
        );
        assert!(!map.contains_key(header::AUTHORIZATION));
    }

    #[test]
    fn bridge_matches_the_scheme_case_insensitively() {
        let mut map = headers(&[("authorization", "bEaReR   sk-live-123")]);
        assert_eq!(
            VendorCredentialBridge::Bridged,
            bridge_vendor_authorization(&mut map)
        );
        assert_eq!(
            "sk-live-123",
            map.get("x-api-key")
                .expect("bridged")
                .to_str()
                .expect("ascii")
        );
    }

    #[test]
    fn bridge_leaves_non_bearer_schemes_for_the_guard() {
        // mem0-style `Authorization: Token <key>` is not this surface's wire;
        // the framework must report it rather than silently accept it.
        let mut map = headers(&[("authorization", "Token sk-live-123")]);
        assert_eq!(
            VendorCredentialBridge::NoCredential,
            bridge_vendor_authorization(&mut map)
        );
        assert!(map.contains_key(header::AUTHORIZATION));
        assert!(!map.contains_key("x-api-key"));
    }

    #[test]
    fn bridge_is_a_no_op_when_an_api_key_is_already_present() {
        let mut map = headers(&[
            ("x-api-key", "sk-from-client"),
            ("authorization", "Bearer sk-live-123"),
        ]);
        assert_eq!(
            VendorCredentialBridge::AlreadyCanonical,
            bridge_vendor_authorization(&mut map)
        );
        assert_eq!(
            "sk-from-client",
            map.get("x-api-key")
                .expect("untouched")
                .to_str()
                .expect("ascii")
        );
        assert!(map.contains_key(header::AUTHORIZATION));
    }

    #[test]
    fn bridge_ignores_empty_or_malformed_credentials() {
        let mut map = headers(&[("authorization", "Bearer   ")]);
        assert_eq!(
            VendorCredentialBridge::NoCredential,
            bridge_vendor_authorization(&mut map)
        );
        let mut map = headers(&[("authorization", "Bearer")]);
        assert_eq!(
            VendorCredentialBridge::NoCredential,
            bridge_vendor_authorization(&mut map)
        );
    }

    #[tokio::test]
    async fn resolver_without_gateway_keys_delegates_to_the_platform_resolver() {
        use sdkwork_web_core::WebRequestContextResolver as _;

        let resolver = GatewayApiKeyResolver::new(
            sdkwork_web_core::DefaultWebRequestContextResolver::default(),
            GatewayApiKeyLookup::new(None, None),
            "appbase",
            sdkwork_web_core::WebEnvironment::Dev,
        );
        let principal = resolver
            .resolve_api_key(
                "api_key_id=key-1;tenant_id=100001;user_id=30;app_id=appbase;environment=dev;deployment_mode=saas",
            )
            .await
            .expect("falls through to the platform resolver");
        assert_eq!("100001", principal.tenant_id());
        assert_eq!(Some("key-1"), principal.api_key_id());
        // The wrapper never claims the dev claim-string channel; the platform
        // resolver stays authoritative for it.
        assert!(!resolver.uses_default_api_key_lookup());
    }

    #[test]
    fn vendor_prefix_matching_is_segment_aware() {
        let prefixes = vec!["/v1".to_owned(), "/anthropic/v1".to_owned()];
        assert!(is_vendor_open_api_path("/v1", &prefixes));
        assert!(is_vendor_open_api_path("/v1/chat/completions", &prefixes));
        assert!(is_vendor_open_api_path("/anthropic/v1/messages", &prefixes));
        assert!(!is_vendor_open_api_path("/v10/models", &prefixes));
        assert!(!is_vendor_open_api_path("/v1beta/models", &prefixes));
        assert!(!is_vendor_open_api_path("/app/v3/api/iam", &prefixes));
    }

    /// The credential channels one scripted inner resolver reports.
    #[derive(Clone, Default)]
    struct ScriptedInner {
        calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        /// What the auth-token channel answers; `None` answers a login-token principal.
        bearer: Option<sdkwork_web_core::WebFrameworkErrorKind>,
    }

    impl ScriptedInner {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    fn scripted_principal(channel: &str) -> sdkwork_web_core::WebRequestPrincipal {
        sdkwork_web_core::WebRequestPrincipal::builder()
            .tenant_id("100001")
            .user_id("30")
            .app_id(format!("appbase-{channel}"))
            .environment(sdkwork_web_core::WebEnvironment::Prod)
            .deployment_mode(sdkwork_web_core::WebDeploymentMode::Saas)
            .auth_level(sdkwork_web_core::WebAuthLevel::Password)
            .subject_type(sdkwork_web_core::WebSubjectType::User)
            .build()
    }

    #[async_trait::async_trait]
    impl sdkwork_web_core::WebRequestContextResolver for ScriptedInner {
        async fn resolve_api_key(
            &self,
            raw_api_key: &str,
        ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError>
        {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("api-key:{raw_api_key}"));
            Ok(scripted_principal("api-key"))
        }

        async fn resolve_dual_token(
            &self,
            _raw_auth_token: &str,
            _raw_access_token: &str,
        ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError>
        {
            Err(sdkwork_web_core::WebFrameworkError::invalid_credentials(
                "unused",
            ))
        }

        async fn resolve_access_token(
            &self,
            _raw_access_token: &str,
        ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError>
        {
            Err(sdkwork_web_core::WebFrameworkError::invalid_credentials(
                "unused",
            ))
        }

        async fn resolve_bearer_auth_token(
            &self,
            raw_bearer_token: &str,
        ) -> Result<sdkwork_web_core::WebRequestPrincipal, sdkwork_web_core::WebFrameworkError>
        {
            self.calls
                .lock()
                .expect("calls")
                .push(format!("auth-token:{raw_bearer_token}"));
            match self.bearer.clone() {
                None => Ok(scripted_principal("auth-token")),
                Some(kind) => Err(sdkwork_web_core::WebFrameworkError {
                    kind,
                    message: "scripted auth-token failure".to_owned(),
                    retry_after_seconds: None,
                    auth_profile: None,
                    failed_stage: None,
                    reason: None,
                }),
            }
        }
    }

    fn resolver_over(inner: ScriptedInner) -> GatewayApiKeyResolver<ScriptedInner> {
        GatewayApiKeyResolver::new(
            inner,
            GatewayApiKeyLookup::new(None, None),
            "appbase",
            sdkwork_web_core::WebEnvironment::Prod,
        )
    }

    #[tokio::test]
    async fn login_token_resolves_through_the_auth_token_channel() {
        use sdkwork_web_core::WebRequestContextResolver as _;

        let inner = ScriptedInner::default();
        let resolver = resolver_over(inner.clone());
        let principal = resolver
            .resolve_api_key("login-token-without-an-api-key-prefix")
            .await
            .expect("signed-in user resolves as a session");
        assert_eq!("appbase-auth-token", principal.app_id());
        // The API-key channel must not be consulted for a login token: it would
        // only report the token as an invalid key.
        assert_eq!(
            vec!["auth-token:login-token-without-an-api-key-prefix".to_owned()],
            inner.calls()
        );
    }

    #[tokio::test]
    async fn prefixed_api_key_never_enters_the_auth_token_channel() {
        use sdkwork_web_core::WebRequestContextResolver as _;

        let inner = ScriptedInner::default();
        let resolver = resolver_over(inner.clone());
        let principal = resolver
            .resolve_api_key("sk-live-123")
            .await
            .expect("api key resolves on the platform channel");
        assert_eq!("appbase-api-key", principal.app_id());
        assert_eq!(vec!["api-key:sk-live-123".to_owned()], inner.calls());
    }

    #[tokio::test]
    async fn unknown_login_token_falls_back_to_the_api_key_channel() {
        use sdkwork_web_core::WebRequestContextResolver as _;

        // A composition whose resolver wires no auth-token lookup, or a token the
        // session store does not know, keeps the unprefixed api-key channel
        // authoritative instead of failing the request on an unavailable channel.
        for kind in [
            sdkwork_web_core::WebFrameworkErrorKind::InvalidCredentials,
            sdkwork_web_core::WebFrameworkErrorKind::DependencyUnavailable,
        ] {
            let inner = ScriptedInner {
                bearer: Some(kind.clone()),
                ..ScriptedInner::default()
            };
            let resolver = resolver_over(inner.clone());
            let principal = resolver
                .resolve_api_key("api_key_id=key-1;tenant_id=100001")
                .await
                .expect("falls through to the api-key channel");
            assert_eq!("appbase-api-key", principal.app_id());
            assert_eq!(
                vec![
                    "auth-token:api_key_id=key-1;tenant_id=100001".to_owned(),
                    "api-key:api_key_id=key-1;tenant_id=100001".to_owned(),
                ],
                inner.calls(),
                "{kind:?} must fall through, not fail the request"
            );
        }
    }

    #[tokio::test]
    async fn expired_login_token_failure_is_not_reinterpreted_as_an_api_key() {
        use sdkwork_web_core::WebRequestContextResolver as _;

        let inner = ScriptedInner {
            bearer: Some(sdkwork_web_core::WebFrameworkErrorKind::ExpiredCredentials),
            ..ScriptedInner::default()
        };
        let resolver = resolver_over(inner.clone());
        let error = resolver
            .resolve_api_key("expired-login-token")
            .await
            .expect_err("an expired session is reported as such");
        assert_eq!(
            sdkwork_web_core::WebFrameworkErrorKind::ExpiredCredentials,
            error.kind
        );
        assert_eq!(
            vec!["auth-token:expired-login-token".to_owned()],
            inner.calls()
        );
    }
}
