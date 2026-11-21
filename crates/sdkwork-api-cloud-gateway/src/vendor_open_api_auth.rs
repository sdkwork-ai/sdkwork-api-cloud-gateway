//! Gateway-issued open-api API keys (`iam_gateway_api_key`) and the
//! vendor-wire credential bridge.
//!
//! Cloud Router issues OpenAI-compatible gateway keys (`sk-`/`sp-` prefixed)
//! and stores only their HMAC-SHA256 hash in `iam_gateway_api_key.key_hash`
//! (peppered with `SDKWORK_CLOUDROUTER_API_KEY_PEPPER`). The platform gateway
//! composes that surface into the single Web Framework pipeline
//! (`APPLICATION_GATEWAY_SPEC.md` §2.3), so the framework's open-api API-key
//! resolver is the layer that must recognise the credential —`API_SPEC.md`
//! §827 requires the server to resolve tenant/organization/user from the
//! validated API-key record rather than from a client-supplied header.
//!
//! This module owns the two deterministic, security-critical pieces of that
//! path:
//!
//! * [`gateway_api_key_hash`] —the hash parity contract with Cloud Router.
//!   If this drifts, every `sk-` credential fails closed, so it is pinned by a
//!   known-answer test against an independently computed HMAC vector.
//! * [`bridge_vendor_authorization`] —the vendor wire bridge. The published
//!   contract of the vendor-compatible surface is `Authorization: Bearer
//!   <key>` (OpenAI/Anthropic wire, `API_SPEC.md` §4.5.2), while the framework
//!   profile that serves it reads `X-API-Key`. The bridge renames that one
//!   header for the vendor prefixes and leaves every other header untouched.
//! * [`GatewayApiKeyResolver`] —the credential authority for both vendor
//!   credential kinds: an API-key-prefixed value resolves against
//!   `iam_gateway_api_key`, and a signed-in user's login token resolves through
//!   the IAM session auth-token channel. The dependency's own handler then picks
//!   the subject's default upstream account group for the request's modality, so
//!   a logged-in user needs no separately issued key to use the relay.
//!
//! Both functions are pure and side-effect free so they can be unit tested
//! without a database or a listener; the resolver wiring that consumes them
//! lives next to the framework composition.

use axum::http::{header, HeaderMap, HeaderName, HeaderValue};

/// Environment key that carries Cloud Router's API-key hashing pepper.
pub const API_KEY_PEPPER_ENV: &str = "SDKWORK_CLOUDROUTER_API_KEY_PEPPER";

/// Lowercase header name the canonical `api-key` open-api profile reads.
const API_KEY_HEADER: &str = "x-api-key";

/// Lowercase header name of the second half of the dual-token credential pair.
const ACCESS_TOKEN_HEADER: &str = "access-token";

/// Lowercase header name the Google-compatible vendor wire publishes.
const GOOGLE_API_KEY_HEADER: &str = "x-goog-api-key";

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
    /// The request carries the dual-token pair (`Authorization` plus
    /// `Access-Token`) that the vendor profile also accepts; both headers were
    /// left untouched, because renaming the bearer half would turn a complete
    /// credential pair into the mixed profile the framework must reject.
    DualTokenPreserved,
}

/// Presents a vendor credential as the canonical `X-API-Key` credential.
///
/// OpenAI-compatible clients (and Cloud Router's own documentation) authenticate
/// with `Authorization: Bearer <key>`, Google-compatible clients with
/// `X-Goog-Api-Key`; the `api-key-or-dual-token` profile that serves those
/// operations reads `X-API-Key` for its API-key alternative and the
/// `Authorization` + `Access-Token` pair for its session alternative. Rewriting
/// the header keeps each published single-credential wire working without
/// relaxing the framework profile.
///
/// Only a `Bearer` scheme is bridged: any other scheme is left for the
/// framework to reject as the credential-profile violation it is. The vendor
/// header is removed when bridged, because a request carrying both headers is
/// exactly the contamination the guard must reject. A request that also carries
/// `Access-Token` is a dual-token caller and is left alone: splitting its pair
/// would invalidate a credential the profile accepts.
pub fn bridge_vendor_authorization(headers: &mut HeaderMap) -> VendorCredentialBridge {
    if headers.contains_key(API_KEY_HEADER) {
        return VendorCredentialBridge::AlreadyCanonical;
    }
    if headers.contains_key(ACCESS_TOKEN_HEADER) {
        return VendorCredentialBridge::DualTokenPreserved;
    }
    if let Some(value) = bearer_credential(headers.get(header::AUTHORIZATION)) {
        headers.remove(header::AUTHORIZATION);
        headers.insert(HeaderName::from_static(API_KEY_HEADER), value);
        return VendorCredentialBridge::Bridged;
    }
    // The Google-compatible wire publishes `X-Goog-Api-Key` instead of a bearer;
    // the framework profile does not know that header, so a request carrying only
    // it would be classified as credential-less. Present it canonically too.
    if let Some(value) = headers.remove(GOOGLE_API_KEY_HEADER) {
        headers.insert(HeaderName::from_static(API_KEY_HEADER), value);
        return VendorCredentialBridge::Bridged;
    }
    VendorCredentialBridge::NoCredential
}

/// The credential of one vendor `Authorization` header, when it is a usable
/// `Bearer` value.
fn bearer_credential(raw: Option<&HeaderValue>) -> Option<HeaderValue> {
    let (scheme, credential) = raw?.to_str().ok()?.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let credential = credential.trim();
    if credential.is_empty() {
        return None;
    }
    HeaderValue::from_str(credential).ok()
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
/// reroute the platform's own open-api authentication. The login-token
/// alternative of that same channel lives in the IAM resolver
/// (`IamDatabaseWebRequestContextResolver::resolve_api_key`), so every
/// composition that mounts it answers the same request the same way; this
/// wrapper only contributes the gateway-issued key store.
#[derive(Clone)]
pub struct GatewayApiKeyResolver<R> {
    inner: R,
    lookup: GatewayApiKeyLookup,
    app_id: std::sync::Arc<str>,
    environment: sdkwork_web_core::WebEnvironment,
    deployment_mode: sdkwork_web_core::WebDeploymentMode,
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
        }
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
        // Every other credential on this channel belongs to the platform
        // resolver: an API key against `iam_api_key`, an unprefixed login
        // `authToken` against the IAM session, or the development claim-string
        // form. Keeping one owner for that split is what makes a standalone
        // application gateway and this platform edge answer identically.
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

    #[test]
    fn bridge_presents_the_google_api_key_header() {
        // Google-compatible clients publish `X-Goog-Api-Key`; the framework
        // profile does not know that header, so without the bridge the request
        // would classify as credential-less and 401 for both credential kinds.
        let mut map = headers(&[("x-goog-api-key", "google-wire-credential")]);
        assert_eq!(
            VendorCredentialBridge::Bridged,
            bridge_vendor_authorization(&mut map)
        );
        assert_eq!(
            "google-wire-credential",
            map.get(API_KEY_HEADER)
                .expect("bridged")
                .to_str()
                .expect("ascii")
        );
        assert!(!map.contains_key(GOOGLE_API_KEY_HEADER));
    }

    #[test]
    fn bridge_preserves_a_dual_token_pair() {
        // The same vendor profile also accepts the dual-token pair. Renaming the
        // bearer half would leave X-API-Key next to Access-Token — the mixed
        // profile the framework rejects — so the pair must reach it intact.
        let mut map = headers(&[
            ("authorization", "Bearer session-auth-token"),
            ("access-token", "session-access-token"),
        ]);
        assert_eq!(
            VendorCredentialBridge::DualTokenPreserved,
            bridge_vendor_authorization(&mut map)
        );
        assert_eq!(
            "Bearer session-auth-token",
            map.get(header::AUTHORIZATION)
                .expect("pair preserved")
                .to_str()
                .expect("ascii")
        );
        assert!(map.contains_key("access-token"));
        assert!(!map.contains_key(API_KEY_HEADER));
    }

    /// Records which credential channel the wrapper delegated to.
    #[derive(Clone, Default)]
    struct ScriptedInner {
        calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl ScriptedInner {
        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("calls").clone()
        }
    }

    fn scripted_principal() -> sdkwork_web_core::WebRequestPrincipal {
        sdkwork_web_core::WebRequestPrincipal::builder()
            .tenant_id("100001")
            .user_id("30")
            .app_id("appbase-api-key")
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
            Ok(scripted_principal())
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
    async fn api_key_channel_delegates_every_credential_shape() {
        use sdkwork_web_core::WebRequestContextResolver as _;

        // The wrapper owns only the gateway-issued key store. Both credential
        // kinds — and the development claim-string form — reach the platform
        // resolver unchanged, which is where the API-key/login-token split lives
        // so that every composition mounting that resolver behaves the same.
        for credential in [
            "sk-live-123",
            "login-token-without-an-api-key-prefix",
            "api_key_id=key-1;tenant_id=100001",
        ] {
            let inner = ScriptedInner::default();
            let resolver = resolver_over(inner.clone());
            let principal = resolver
                .resolve_api_key(credential)
                .await
                .expect("the platform resolver answers");
            assert_eq!("appbase-api-key", principal.app_id());
            assert_eq!(vec![format!("api-key:{credential}")], inner.calls());
        }
    }
}
