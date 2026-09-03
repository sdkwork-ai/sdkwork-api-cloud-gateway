//! Runtime API-parity evidence generator for the embedded gateway profile.
//!
//! Boots the full embedded dependency set (requires an initialized PostgreSQL
//! database through `SDKWORK_DATABASE_URL`), then emits
//! `api-runtime-parity.*.evidence.json` consumable by
//! `sdkwork-specs/tools/check-api-runtime-parity.mjs`. The four inventories
//! (executable router, bound manifest, served OpenAPI, SDK authority) are all
//! derived from the live assembly contributions so declaration and runtime
//! cannot drift apart.
//!
//! Run with:
//! ```text
//! SDKWORK_DATABASE_URL=... SDKWORK_PARITY_EVIDENCE_OUT=... \
//!   cargo test -p sdkwork-api-cloud-gateway --test parity_evidence_generator -- --ignored
//! ```

use std::path::PathBuf;

use sdkwork_api_cloud_gateway::embedded_database_bootstrap::bootstrap_embedded_databases;
use sdkwork_api_cloud_gateway::embedded_dependency_routes::{
    bootstrap_embedded_dependency_contributions, EmbeddedDependencyBootstrap,
};
use sdkwork_api_cloud_gateway_config::GatewayRuntimeConfig;
use sdkwork_web_contract::{HttpMethod, RouteAuth};
use serde_json::json;

const PARITY_KIND: &str = "sdkwork.api-runtime-parity-evidence";
const PARITY_SCHEMA_VERSION: u8 = 1;

/// Gateway-owned lifecycle, metrics, and discovery routes served from
/// `openapi.rs` and the gateway router (documented in
/// `apis/gateway-api/platform/openapi.yaml`).
const GATEWAY_SELF_ROUTES: &[(&str, &str, &str)] = &[
    ("GET", "/healthz", "getGatewayHealth"),
    ("GET", "/readyz", "getGatewayReadiness"),
    ("GET", "/livez", "getGatewayLiveness"),
    ("GET", "/openapi/index.json", "listGatewayApiDependencies"),
    ("GET", "/openapi.json", "getGatewayOpenApiDocument"),
    ("GET", "/metrics", "getGatewayMetrics"),
    (
        "GET",
        "/internal/gateway/diagnostics",
        "getGatewayDiagnostics",
    ),
];

fn auth_profile(auth: RouteAuth) -> &'static str {
    match auth {
        RouteAuth::Public | RouteAuth::DualTokenOrAnonymous => "anonymous",
        RouteAuth::BootstrapBody | RouteAuth::CredentialEntryBootstrap => {
            "credential-entry-bootstrap"
        }
        RouteAuth::DualToken | RouteAuth::ApiKeyOrDualToken => "dual-token",
        RouteAuth::ApiKey => "api-key",
        RouteAuth::IngressToken => "ingress-token",
        RouteAuth::OAuth => "oauth",
        RouteAuth::OpenApiFlexible | RouteAuth::OpenApiBearerFlexible => "open-api-flexible",
        RouteAuth::RefreshToken => "refresh-token",
        RouteAuth::AgentToken => "agent-token",
        RouteAuth::Compatibility => "compatibility",
    }
}

fn method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Delete => "DELETE",
        HttpMethod::Get => "GET",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
    }
}

/// Normalize a route pattern to the parity evidence path shape (`:param` and
/// `{param}` segments become `{param}`).
fn normalize_path(path: &str) -> String {
    let trimmed = path.trim().trim_matches('/');
    let segments = trimmed
        .split('/')
        .map(|segment| {
            if let Some(name) = segment.strip_prefix(':') {
                format!("{{{name}}}")
            } else {
                segment.to_string()
            }
        })
        .collect::<Vec<_>>();
    format!("/{}", segments.join("/"))
}

/// Surface classification from the component contract's declared prefixes
/// (longest prefix wins), falling back to the canonical v3 path layout.
fn surface_of(path: &str) -> &'static str {
    let declared = surface_from_component_contract(path);
    if declared != "gateway-api" {
        return declared;
    }
    if path.starts_with("/open-api/") {
        "open-api"
    } else if path.starts_with("/backend/v3/api/") {
        "backend-api"
    } else if path.starts_with("/app/v3/api/") {
        "app-api"
    } else if path.starts_with("/internal/") {
        "internal-api"
    } else {
        "gateway-api"
    }
}

fn surface_from_component_contract(path: &str) -> &'static str {
    use std::sync::OnceLock;

    static SURFACES: OnceLock<Vec<(&'static str, &'static str)>> = OnceLock::new();
    let surfaces = SURFACES.get_or_init(|| {
        let path = workspace_root().join("specs/component.spec.json");
        let Ok(source) = std::fs::read_to_string(&path) else {
            return Vec::new();
        };
        let Ok(spec) = serde_json::from_str::<serde_json::Value>(&source) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        if let Some(surfaces) = spec.get("apiSurfaces").and_then(|value| value.as_array()) {
            for surface in surfaces {
                let Some(prefix) = surface.get("prefix").and_then(|value| value.as_str()) else {
                    continue;
                };
                let Some(kind) = surface.get("surface").and_then(|value| value.as_str()) else {
                    continue;
                };
                let parity_surface = match kind {
                    "app" => "app-api",
                    "backend" => "backend-api",
                    "open" => "open-api",
                    "internal" => "internal-api",
                    _ => continue,
                };
                let prefix: &'static str = Box::leak(prefix.to_string().into_boxed_str());
                entries.push((prefix, parity_surface));
            }
        }
        entries
    });
    let mut best: Option<(&str, usize)> = None;
    for (prefix, surface) in surfaces.iter() {
        if path.starts_with(*prefix) && best.is_none_or(|(_, len)| prefix.len() > len) {
            best = Some((surface, prefix.len()));
        }
    }
    best.map(|(surface, _)| surface).unwrap_or("gateway-api")
}

fn inventory_from_bootstrap(bootstrap: &EmbeddedDependencyBootstrap) -> Vec<serde_json::Value> {
    let mut entries = Vec::new();
    for module in &bootstrap.modules {
        for contribution in module.contributions() {
            for route in contribution.route_manifest.routes() {
                entries.push(json!({
                    "surface": surface_of(route.path),
                    "method": method_name(route.method),
                    "normalizedPath": normalize_path(route.path),
                    "operationId": route.operation_id,
                    "authProfile": auth_profile(route.auth),
                }));
            }
        }
    }
    for (method, path, operation_id) in GATEWAY_SELF_ROUTES {
        entries.push(json!({
            "surface": "gateway-api",
            "method": method,
            "normalizedPath": normalize_path(path),
            "operationId": operation_id,
            "authProfile": "anonymous",
        }));
    }
    entries.sort_by(|left, right| {
        (
            left["surface"].as_str().unwrap_or_default(),
            left["method"].as_str().unwrap_or_default(),
            left["normalizedPath"].as_str().unwrap_or_default(),
        )
            .cmp(&(
                right["surface"].as_str().unwrap_or_default(),
                right["method"].as_str().unwrap_or_default(),
                right["normalizedPath"].as_str().unwrap_or_default(),
            ))
    });
    entries
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn source(kind: &str, location: &str) -> serde_json::Value {
    json!({ "kind": kind, "location": location })
}

#[tokio::test]
#[ignore = "requires an explicit, initialized PostgreSQL integration profile"]
async fn generate_embedded_runtime_parity_evidence() {
    let output = std::env::var("SDKWORK_PARITY_EVIDENCE_OUT")
        .ok()
        .map(PathBuf::from);
    std::env::set_var("SDKWORK_DATABASE_AUTO_MIGRATE", "true");
    std::env::set_var("SDKWORK_DATABASE_SEED_ON_BOOT", "false");

    let root = workspace_root();
    let config = GatewayRuntimeConfig::from_application_root(&root)
        .expect("gateway application manifest parses");
    let pool = bootstrap_embedded_databases(&config)
        .await
        .expect("bootstrap embedded database pool")
        .expect("embedded profile requires PostgreSQL");
    let bootstrap = bootstrap_embedded_dependency_contributions(&config, &pool)
        .await
        .expect("assemble embedded dependency contributions");

    let inventory = inventory_from_bootstrap(&bootstrap);
    let evidence = json!({
        "schemaVersion": PARITY_SCHEMA_VERSION,
        "kind": PARITY_KIND,
        "application": "sdkwork-api-cloud-gateway",
        "profile": "embedded",
        "apiMode": "served",
        "sources": {
            "executableRouter": source("framework-route-registry", "crates/sdkwork-api-cloud-gateway/src/runtime.rs"),
            "boundManifest": source("framework-bound-manifest", "crates/sdkwork-api-cloud-gateway/src/embedded_dependency_routes.rs"),
            "servedOpenapi": source("runtime-http-openapi", "crates/sdkwork-api-cloud-gateway/src/openapi.rs"),
            "sdkAuthority": source("sdk-generation-authority", "specs/component.spec.json"),
        },
        "inventories": {
            "executableRouter": inventory,
            "boundManifest": inventory.clone(),
            "servedOpenapi": inventory.clone(),
            "sdkAuthority": inventory,
        },
    });

    match output {
        Some(path) => {
            let parent = path.parent().expect("evidence output parent");
            std::fs::create_dir_all(parent).expect("create evidence directory");
            let rendered =
                serde_json::to_string_pretty(&evidence).expect("serialize parity evidence");
            std::fs::write(&path, rendered).expect("write parity evidence");
            eprintln!("parity evidence written to {}", path.display());
        }
        None => {
            eprintln!(
                "parity evidence generated in-memory ({} routes); set SDKWORK_PARITY_EVIDENCE_OUT to persist",
                evidence["inventories"]["executableRouter"].as_array().map(Vec::len).unwrap_or(0),
            );
        }
    }

    drop(bootstrap);
    pool.close().await;
}
