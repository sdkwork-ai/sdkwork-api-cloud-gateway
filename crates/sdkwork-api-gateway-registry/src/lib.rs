//! Route registry for the SDKWork API Gateway.

use sdkwork_api_gateway_config::{
    DependencyApiSurfaceConfig, APPBASE_APP_API_PREFIX, APPBASE_APP_API_SERVICE_ID,
    APPBASE_BACKEND_API_PREFIX, APPBASE_BACKEND_API_SERVICE_ID,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayRouteRegistry {
    routes: Vec<RouteDescriptor>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDescriptor {
    pub service_id: String,
    pub path_pattern: String,
    pub api_prefix: String,
    pub surface: String,
    pub methods: Vec<String>,
    pub protocol: RouteProtocol,
    pub visibility: RouteVisibility,
    pub execution_target: RouteExecutionTarget,
    pub sdk_target: Option<String>,
    pub requires_executable_dependency: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteProtocol {
    Http,
    WebSocket,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteVisibility {
    App,
    Backend,
    Open,
    Internal,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RouteExecutionTarget {
    Gateway,
    Dependency,
}

impl GatewayRouteRegistry {
    pub fn new(routes: Vec<RouteDescriptor>) -> Self {
        Self { routes }
    }

    pub fn sdkwork_default() -> Self {
        Self::new(vec![
            appbase_route("/app/v3/api/auth/{*path}"),
            appbase_route("/app/v3/api/iam/{*path}"),
            appbase_route("/app/v3/api/oauth/{*path}"),
            appbase_route("/app/v3/api/system/iam/{*path}"),
        ])
    }

    pub fn from_dependency_surfaces(surfaces: &[DependencyApiSurfaceConfig]) -> Self {
        let mut routes = Vec::new();
        for surface in surfaces {
            if surface.service_id == APPBASE_APP_API_SERVICE_ID
                && surface.api_prefix == APPBASE_APP_API_PREFIX
            {
                routes.extend(Self::sdkwork_default().routes);
            } else if surface.service_id == APPBASE_BACKEND_API_SERVICE_ID
                && surface.api_prefix == APPBASE_BACKEND_API_PREFIX
            {
                routes.push(appbase_backend_route());
            } else {
                routes.push(dependency_surface_route(surface));
            }
        }
        deduplicate_routes(&mut routes);
        sort_routes_by_specificity(&mut routes);
        Self::new(routes)
    }

    pub fn routes(&self) -> &[RouteDescriptor] {
        self.routes.as_slice()
    }

    pub fn resolve(&self, method: &str, path: &str) -> Option<&RouteDescriptor> {
        let method = method.to_ascii_uppercase();
        self.routes
            .iter()
            .find(|route| route.matches_method(method.as_str()) && route.matches_path(path))
    }
}

impl Default for GatewayRouteRegistry {
    fn default() -> Self {
        Self::sdkwork_default()
    }
}

impl RouteDescriptor {
    pub fn matches_method(&self, method: &str) -> bool {
        self.methods
            .iter()
            .any(|candidate| candidate == "*" || candidate.eq_ignore_ascii_case(method))
    }

    pub fn matches_path(&self, path: &str) -> bool {
        if let Some(prefix) = self.path_pattern.strip_suffix("/{*path}") {
            return path == prefix || path.starts_with(format!("{prefix}/").as_str());
        }

        self.path_pattern == path
    }
}

fn appbase_route(path_pattern: &str) -> RouteDescriptor {
    RouteDescriptor {
        service_id: APPBASE_APP_API_SERVICE_ID.to_string(),
        path_pattern: path_pattern.to_string(),
        api_prefix: APPBASE_APP_API_PREFIX.to_string(),
        surface: "app".to_string(),
        methods: vec!["*".to_string()],
        protocol: RouteProtocol::Http,
        visibility: RouteVisibility::App,
        execution_target: RouteExecutionTarget::Dependency,
        sdk_target: Some("sdkwork-appbase-app-sdk".to_string()),
        requires_executable_dependency: true,
    }
}

fn appbase_backend_route() -> RouteDescriptor {
    RouteDescriptor {
        service_id: APPBASE_BACKEND_API_SERVICE_ID.to_string(),
        path_pattern: "/backend/v3/api/iam/{*path}".to_string(),
        api_prefix: APPBASE_BACKEND_API_PREFIX.to_string(),
        surface: "backend".to_string(),
        methods: vec!["*".to_string()],
        protocol: RouteProtocol::Http,
        visibility: RouteVisibility::Backend,
        execution_target: RouteExecutionTarget::Dependency,
        sdk_target: Some("sdkwork-appbase-backend-sdk".to_string()),
        requires_executable_dependency: false,
    }
}

fn dependency_surface_route(surface: &DependencyApiSurfaceConfig) -> RouteDescriptor {
    RouteDescriptor {
        service_id: surface.service_id.clone(),
        path_pattern: format!("{}/{{*path}}", surface.api_prefix.trim_end_matches('/')),
        api_prefix: surface.api_prefix.clone(),
        surface: surface.surface.clone(),
        methods: vec!["*".to_string()],
        protocol: RouteProtocol::Http,
        visibility: visibility_from_surface(surface.surface.as_str()),
        execution_target: RouteExecutionTarget::Dependency,
        sdk_target: Some(surface.sdk_family.clone()),
        requires_executable_dependency: surface.executable_export.is_some(),
    }
}

fn visibility_from_surface(surface: &str) -> RouteVisibility {
    match surface.trim().to_ascii_lowercase().as_str() {
        "app" => RouteVisibility::App,
        "backend" => RouteVisibility::Backend,
        "open" => RouteVisibility::Open,
        _ => RouteVisibility::Internal,
    }
}

fn deduplicate_routes(routes: &mut Vec<RouteDescriptor>) {
    let mut unique = Vec::new();
    for route in routes.drain(..) {
        if unique.iter().any(|candidate: &RouteDescriptor| {
            candidate.service_id == route.service_id && candidate.path_pattern == route.path_pattern
        }) {
            continue;
        }
        unique.push(route);
    }
    *routes = unique;
}

fn sort_routes_by_specificity(routes: &mut [RouteDescriptor]) {
    routes.sort_by(|left, right| {
        route_specificity(right)
            .cmp(&route_specificity(left))
            .then_with(|| left.path_pattern.cmp(&right.path_pattern))
            .then_with(|| left.service_id.cmp(&right.service_id))
    });
}

fn route_specificity(route: &RouteDescriptor) -> (usize, usize, usize) {
    let wildcard_rank = usize::from(!route.path_pattern.ends_with("/{*path}"));
    let prefix = route
        .path_pattern
        .strip_suffix("/{*path}")
        .unwrap_or(route.path_pattern.as_str());
    let segment_count = prefix
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count();
    (prefix.len(), segment_count, wildcard_rank)
}
