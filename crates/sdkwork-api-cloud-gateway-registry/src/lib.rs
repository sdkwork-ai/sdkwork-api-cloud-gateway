//! Route registry for the SDKWork API Gateway.

use std::collections::BTreeMap;

use sdkwork_api_cloud_gateway_config::{
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

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RouteRegistryError {
    #[error("route `{service_id}` must declare an absolute path template")]
    InvalidPath { service_id: String },
    #[error("exact route `{service_id}` cannot use catch-all template `{path_pattern}`")]
    CatchAllNotAllowed {
        service_id: String,
        path_pattern: String,
    },
    #[error("route `{service_id}` must declare at least one concrete HTTP method")]
    InvalidMethods { service_id: String },
    #[error(
        "route collision for {method} {normalized_path}: `{existing_service_id}` conflicts with `{service_id}`"
    )]
    RouteCollision {
        method: String,
        normalized_path: String,
        existing_service_id: String,
        service_id: String,
    },
}

impl GatewayRouteRegistry {
    pub fn new(routes: Vec<RouteDescriptor>) -> Self {
        let mut routes = routes;
        deduplicate_routes(&mut routes);
        sort_routes_by_specificity(&mut routes);
        Self { routes }
    }

    pub fn try_from_exact_routes(
        mut routes: Vec<RouteDescriptor>,
    ) -> Result<Self, RouteRegistryError> {
        validate_exact_routes(&routes)?;
        sort_routes_by_specificity(&mut routes);
        Ok(Self { routes })
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

        match_template_path(self.path_pattern.as_str(), path)
    }
}

fn match_template_path(template: &str, path: &str) -> bool {
    let template_segments = path_segments(template);
    let path_segments = path_segments(path);
    template_segments.len() == path_segments.len()
        && template_segments
            .iter()
            .zip(path_segments)
            .all(|(template_segment, path_segment)| {
                is_path_parameter(template_segment) || *template_segment == path_segment
            })
}

fn path_segments(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn is_path_parameter(segment: &str) -> bool {
    segment.len() > 2 && segment.starts_with('{') && segment.ends_with('}')
}

fn normalized_path_template(path: &str) -> String {
    let segments = path_segments(path)
        .into_iter()
        .map(|segment| {
            if is_path_parameter(segment) {
                "{param}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>();
    format!("/{}", segments.join("/"))
}

fn validate_exact_routes(routes: &[RouteDescriptor]) -> Result<(), RouteRegistryError> {
    let mut owners = BTreeMap::<(String, String), &str>::new();
    for route in routes {
        if !route.path_pattern.starts_with('/') {
            return Err(RouteRegistryError::InvalidPath {
                service_id: route.service_id.clone(),
            });
        }
        if route.path_pattern.contains("{*") {
            return Err(RouteRegistryError::CatchAllNotAllowed {
                service_id: route.service_id.clone(),
                path_pattern: route.path_pattern.clone(),
            });
        }
        if route.methods.is_empty() || route.methods.iter().any(|method| method == "*") {
            return Err(RouteRegistryError::InvalidMethods {
                service_id: route.service_id.clone(),
            });
        }

        let normalized_path = normalized_path_template(route.path_pattern.as_str());
        for method in &route.methods {
            let method = method.to_ascii_uppercase();
            let identity = (method.clone(), normalized_path.clone());
            if let Some(existing_service_id) = owners.insert(identity, route.service_id.as_str()) {
                return Err(RouteRegistryError::RouteCollision {
                    method,
                    normalized_path,
                    existing_service_id: existing_service_id.to_owned(),
                    service_id: route.service_id.clone(),
                });
            }
        }
    }
    Ok(())
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
        sdk_target: Some("sdkwork-iam-app-sdk".to_string()),
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
        sdk_target: Some("sdkwork-iam-backend-sdk".to_string()),
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
