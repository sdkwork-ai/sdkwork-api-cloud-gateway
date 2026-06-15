use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use axum::{
    body::Body,
    extract::{ws::WebSocketUpgrade, Query, State},
    http::{header, Request, Response, StatusCode},
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use sdkwork_api_gateway::{
    build_embedded_sdkwork_appbase_app_api_router_with_runtime_context,
    build_sdkwork_api_gateway_api_server_router, build_sdkwork_api_gateway_router,
    build_sdkwork_api_gateway_router_with_embedded_routers, GatewayRuntime,
    SdkworkAppbaseLocalIamRuntimeContext,
};
use sdkwork_api_gateway_config::{
    DependencyApiSurfaceConfig, DependencyRuntimeMode, GatewayMode, GatewayRuntimeConfig,
    RateLimitRuleConfig, ServiceUpstreamConfig,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame},
        Message,
    },
};
use tower::ServiceExt;

async fn appbase_echo(Query(query): Query<HashMap<String, String>>, body: String) -> String {
    format!(
        "trace={};body={}",
        query.get("trace").cloned().unwrap_or_default(),
        body
    )
}

async fn dependency_echo(Query(query): Query<HashMap<String, String>>, body: String) -> String {
    format!(
        "dependency={};body={}",
        query.get("dependency").cloned().unwrap_or_default(),
        body
    )
}

async fn forwarded_echo(headers: axum::http::HeaderMap) -> String {
    let forwarded_host = headers
        .get("x-forwarded-host")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let forwarded_proto = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    format!("host={forwarded_host};proto={forwarded_proto}")
}

async fn request_id_echo(headers: axum::http::HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

async fn traceparent_echo(headers: axum::http::HeaderMap) -> String {
    headers
        .get("traceparent")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

async fn slow_echo() -> &'static str {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    "slow"
}

async fn flaky_retry_echo(State(attempts): State<Arc<AtomicUsize>>) -> (StatusCode, String) {
    let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
    if attempt == 1 {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("attempt={attempt}"),
        );
    }
    (StatusCode::OK, format!("attempt={attempt}"))
}

async fn failing_circuit_echo(State(attempts): State<Arc<AtomicUsize>>) -> (StatusCode, String) {
    let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
    (
        StatusCode::SERVICE_UNAVAILABLE,
        format!("failing-attempt={attempt}"),
    )
}

async fn health_echo() -> &'static str {
    "healthy"
}

async fn delayed_stream() -> Response<Body> {
    let stream = futures_util::stream::unfold(0_u8, |state| async move {
        match state {
            0 => Some((Ok::<_, std::io::Error>(Bytes::from_static(b"first")), 1)),
            1 => {
                tokio::time::sleep(Duration::from_millis(1_000)).await;
                Some((Ok::<_, std::io::Error>(Bytes::from_static(b"second")), 2))
            }
            _ => None,
        }
    });

    Response::builder()
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from_stream(stream))
        .expect("stream response builds")
}

async fn websocket_echo(ws: WebSocketUpgrade) -> axum::response::Response {
    ws.on_upgrade(|mut socket| async move {
        while let Some(Ok(message)) = socket.recv().await {
            if socket.send(message).await.is_err() {
                break;
            }
        }
    })
}

async fn spawn_appbase_echo_upstream() -> String {
    let router = Router::new().route(
        "/app/v3/api/auth/echo",
        post(appbase_echo).delete(appbase_echo),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test upstream server");
    });
    format!("http://{address}")
}

async fn spawn_forwarded_echo_upstream() -> String {
    let router = Router::new()
        .route("/app/v3/api/auth/forwarded", post(forwarded_echo))
        .route("/app/v3/api/auth/request-id", post(request_id_echo))
        .route("/app/v3/api/auth/traceparent", post(traceparent_echo))
        .route("/app/v3/api/auth/slow", post(slow_echo))
        .route("/app/v3/api/auth/stream", get(delayed_stream))
        .route("/healthz", get(health_echo));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind forwarded test upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test forwarded upstream server");
    });
    format!("http://{address}")
}

async fn spawn_flaky_retry_upstream(attempts: Arc<AtomicUsize>) -> String {
    let router = Router::new()
        .route("/app/v3/api/auth/flaky", post(flaky_retry_echo))
        .with_state(attempts);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind flaky retry upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test flaky retry upstream server");
    });
    format!("http://{address}")
}

async fn spawn_failing_circuit_upstream(attempts: Arc<AtomicUsize>) -> String {
    let router = Router::new()
        .route("/app/v3/api/auth/failing", post(failing_circuit_echo))
        .with_state(attempts);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind failing circuit upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test failing circuit upstream server");
    });
    format!("http://{address}")
}

async fn spawn_dependency_echo_upstream() -> String {
    let router = Router::new().route("/custom/v1/echo", post(dependency_echo));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test dependency upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test dependency upstream server");
    });
    format!("http://{address}")
}

async fn spawn_foundation_echo_upstream(api_prefix: &str) -> String {
    let prefix = api_prefix.trim_end_matches('/').to_string();
    let probe_path = format!("{}/probe", prefix);
    let wildcard_path = format!("{}/{{*rest}}", prefix);
    let router = Router::new()
        .route(probe_path.as_str(), post(dependency_echo))
        .route(wildcard_path.as_str(), post(dependency_echo));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind foundation test upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test foundation upstream server");
    });
    format!("http://{address}")
}

async fn spawn_foundation_route_echo_upstream(route_path: &str) -> String {
    let router = Router::new().route(route_path, post(dependency_echo));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind foundation route test upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test foundation route upstream server");
    });
    format!("http://{address}")
}

async fn spawn_foundation_multi_route_echo_upstream(route_paths: &[&str]) -> String {
    let mut router = Router::new();
    for route_path in route_paths {
        router = router.route(route_path, post(dependency_echo));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind foundation multi-route test upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test foundation multi-route upstream server");
    });
    format!("http://{address}")
}

async fn spawn_websocket_echo_upstream() -> String {
    let router = Router::new().route("/custom/v1/ws", get(websocket_echo));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind websocket test upstream");
    let address = listener.local_addr().expect("test upstream local address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test websocket upstream server");
    });
    format!("http://{address}")
}

async fn spawn_all_foundation_upstreams() -> HashMap<String, String> {
    let mut env = HashMap::new();
    
    let appbase_app = spawn_appbase_echo_upstream().await;
    env.insert("SDKWORK_APPBASE_APP_API_BASE_URL".to_string(), appbase_app);
    
    let appbase_backend = spawn_foundation_echo_upstream("/backend/v3/api/iam").await;
    env.insert("SDKWORK_APPBASE_BACKEND_API_BASE_URL".to_string(), appbase_backend);
    
    let drive_app = spawn_foundation_echo_upstream("/app/v3/api/drive").await;
    env.insert("SDKWORK_DRIVE_APP_API_BASE_URL".to_string(), drive_app);
    
    let drive_backend = spawn_foundation_echo_upstream("/backend/v3/api/drive").await;
    env.insert("SDKWORK_DRIVE_BACKEND_API_BASE_URL".to_string(), drive_backend);
    
    let drive_open = spawn_foundation_echo_upstream("/open/v3/api/drive").await;
    env.insert("SDKWORK_DRIVE_OPEN_API_BASE_URL".to_string(), drive_open);
    
    let drive_admin = spawn_foundation_echo_upstream("/admin/v3/api/drive").await;
    env.insert("SDKWORK_DRIVE_ADMIN_STORAGE_API_BASE_URL".to_string(), drive_admin);
    
    let notary_app = spawn_foundation_echo_upstream("/app/v3/api/notary").await;
    env.insert("SDKWORK_NOTARY_APP_API_BASE_URL".to_string(), notary_app);
    
    let notary_backend = spawn_foundation_echo_upstream("/backend/v3/api/notary").await;
    env.insert("SDKWORK_NOTARY_BACKEND_API_BASE_URL".to_string(), notary_backend);
    
    let rtc_app = spawn_foundation_echo_upstream("/app/v3/api/rtc").await;
    env.insert("SDKWORK_RTC_APP_API_BASE_URL".to_string(), rtc_app);
    
    let rtc_backend = spawn_foundation_echo_upstream("/backend/v3/api/rtc").await;
    env.insert("SDKWORK_RTC_BACKEND_API_BASE_URL".to_string(), rtc_backend);
    
    let agent_app = spawn_foundation_echo_upstream("/app/v3/api/ai").await;
    env.insert("SDKWORK_AGENT_APP_API_BASE_URL".to_string(), agent_app);
    
    let agent_backend = spawn_foundation_echo_upstream("/backend/v3/api/ai").await;
    env.insert("SDKWORK_AGENT_BACKEND_API_BASE_URL".to_string(), agent_backend);
    
    let agent_open = spawn_foundation_echo_upstream("/agent/v3/api").await;
    env.insert("SDKWORK_AGENT_OPEN_API_BASE_URL".to_string(), agent_open);
    
    let aiot_app = spawn_foundation_echo_upstream("/app/v3/api/iot").await;
    env.insert("SDKWORK_AIOT_APP_API_BASE_URL".to_string(), aiot_app);
    
    let aiot_backend = spawn_foundation_echo_upstream("/backend/v3/api/iot").await;
    env.insert("SDKWORK_AIOT_BACKEND_API_BASE_URL".to_string(), aiot_backend);
    
    let memory_app = spawn_foundation_echo_upstream("/app/v3/api/memory").await;
    env.insert("SDKWORK_MEMORY_APP_API_BASE_URL".to_string(), memory_app);
    
    let memory_backend = spawn_foundation_echo_upstream("/backend/v3/api/memory").await;
    env.insert("SDKWORK_MEMORY_BACKEND_API_BASE_URL".to_string(), memory_backend);
    
    let memory_open = spawn_foundation_echo_upstream("/mem/v3/api/memory").await;
    env.insert("SDKWORK_MEMORY_OPEN_API_BASE_URL".to_string(), memory_open);
    
    let knowledgebase_app = spawn_foundation_echo_upstream("/app/v3/api/knowledge").await;
    env.insert("SDKWORK_KNOWLEDGEBASE_APP_API_BASE_URL".to_string(), knowledgebase_app);
    
    let knowledgebase_backend = spawn_foundation_echo_upstream("/backend/v3/api/knowledge").await;
    env.insert("SDKWORK_KNOWLEDGEBASE_BACKEND_API_BASE_URL".to_string(), knowledgebase_backend);
    
    let knowledgebase_open = spawn_foundation_echo_upstream("/knowledge/v3/api").await;
    env.insert("SDKWORK_KNOWLEDGEBASE_OPEN_API_BASE_URL".to_string(), knowledgebase_open);
    
    let news_app = spawn_foundation_echo_upstream("/app/v3/api/news").await;
    env.insert("SDKWORK_NEWS_APP_API_BASE_URL".to_string(), news_app);
    
    let news_backend = spawn_foundation_echo_upstream("/backend/v3/api/news").await;
    env.insert("SDKWORK_NEWS_BACKEND_API_BASE_URL".to_string(), news_backend);
    
    let news_open = spawn_foundation_echo_upstream("/open/v3/api/news").await;
    env.insert("SDKWORK_NEWS_OPEN_API_BASE_URL".to_string(), news_open);
    
    let notes_app = spawn_foundation_echo_upstream("/app/v3/api/notes").await;
    env.insert("SDKWORK_NOTES_APP_API_BASE_URL".to_string(), notes_app);
    
    let notes_backend = spawn_foundation_echo_upstream("/backend/v3/api/notes").await;
    env.insert("SDKWORK_NOTES_BACKEND_API_BASE_URL".to_string(), notes_backend);
    
    let notes_open = spawn_foundation_echo_upstream("/notes/v3/api").await;
    env.insert("SDKWORK_NOTES_OPEN_API_BASE_URL".to_string(), notes_open);
    
    let music_app = spawn_foundation_echo_upstream("/app/v3/api/music").await;
    env.insert("SDKWORK_MUSIC_APP_API_BASE_URL".to_string(), music_app);
    
    let music_backend = spawn_foundation_echo_upstream("/backend/v3/api/music").await;
    env.insert("SDKWORK_MUSIC_BACKEND_API_BASE_URL".to_string(), music_backend);
    
    let generations_app = spawn_foundation_echo_upstream("/app/v3/api/generations").await;
    env.insert("SDKWORK_GENERATIONS_APP_API_BASE_URL".to_string(), generations_app);
    
    let generations_backend = spawn_foundation_echo_upstream("/backend/v3/api/generations").await;
    env.insert("SDKWORK_GENERATIONS_BACKEND_API_BASE_URL".to_string(), generations_backend);
    
    let community_app = spawn_foundation_echo_upstream("/app/v3/api/community").await;
    env.insert("SDKWORK_COMMUNITY_APP_API_BASE_URL".to_string(), community_app);
    
    let community_backend = spawn_foundation_echo_upstream("/backend/v3/api/community").await;
    env.insert("SDKWORK_COMMUNITY_BACKEND_API_BASE_URL".to_string(), community_backend);
    
    let community_open = spawn_foundation_echo_upstream("/community/v3/api").await;
    env.insert("SDKWORK_COMMUNITY_OPEN_API_BASE_URL".to_string(), community_open);
    
    let search_app = spawn_foundation_echo_upstream("/app/v3/api/search").await;
    env.insert("SDKWORK_SEARCH_APP_API_BASE_URL".to_string(), search_app);
    
    let search_backend = spawn_foundation_echo_upstream("/backend/v3/api/search").await;
    env.insert("SDKWORK_SEARCH_BACKEND_API_BASE_URL".to_string(), search_backend);
    
    let voice_app = spawn_foundation_echo_upstream("/app/v3/api/voice").await;
    env.insert("SDKWORK_VOICE_APP_API_BASE_URL".to_string(), voice_app);
    
    let voice_backend = spawn_foundation_echo_upstream("/backend/v3/api/voice").await;
    env.insert("SDKWORK_VOICE_BACKEND_API_BASE_URL".to_string(), voice_backend);
    
    let image_app = spawn_foundation_echo_upstream("/app/v3/api/image").await;
    env.insert("SDKWORK_IMAGE_APP_API_BASE_URL".to_string(), image_app);
    
    let image_backend = spawn_foundation_echo_upstream("/backend/v3/api/image").await;
    env.insert("SDKWORK_IMAGE_BACKEND_API_BASE_URL".to_string(), image_backend);
    
    let image_open = spawn_foundation_echo_upstream("/image/v3/api").await;
    env.insert("SDKWORK_IMAGE_OPEN_API_BASE_URL".to_string(), image_open);
    
    let comments_app = spawn_foundation_multi_route_echo_upstream(&[
        "/app/v3/api/comments/probe",
        "/app/v3/api/comments/{*rest}",
        "/app/v3/api/engagement/probe",
        "/app/v3/api/engagement/{*rest}",
    ])
    .await;
    env.insert("SDKWORK_COMMENTS_APP_API_BASE_URL".to_string(), comments_app);
    
    let comments_backend = spawn_foundation_echo_upstream("/backend/v3/api/comments").await;
    env.insert("SDKWORK_COMMENTS_BACKEND_API_BASE_URL".to_string(), comments_backend);
    
    let course_app = spawn_foundation_multi_route_echo_upstream(&[
        "/app/v3/api/courses/probe",
        "/app/v3/api/courses/{*rest}",
        "/app/v3/api/course_applications/probe",
        "/app/v3/api/course_applications/{*rest}",
    ])
    .await;
    env.insert("SDKWORK_COURSE_APP_API_BASE_URL".to_string(), course_app);
    
    let course_backend = spawn_foundation_multi_route_echo_upstream(&[
        "/backend/v3/api/courses/probe",
        "/backend/v3/api/courses/{*rest}",
        "/backend/v3/api/course_applications/probe",
        "/backend/v3/api/course_applications/{*rest}",
        "/backend/v3/api/course_lessons/probe",
        "/backend/v3/api/course_lessons/{*rest}",
        "/backend/v3/api/course_sections/probe",
        "/backend/v3/api/course_sections/{*rest}",
    ])
    .await;
    env.insert("SDKWORK_COURSE_BACKEND_API_BASE_URL".to_string(), course_backend);
    
    let messaging_app = spawn_foundation_echo_upstream("/app/v3/api/messaging").await;
    env.insert("SDKWORK_MESSAGING_APP_API_BASE_URL".to_string(), messaging_app);
    
    let messaging_backend = spawn_foundation_echo_upstream("/backend/v3/api/messaging").await;
    env.insert("SDKWORK_MESSAGING_BACKEND_API_BASE_URL".to_string(), messaging_backend);
    
    let commerce_app = spawn_foundation_echo_upstream("/app/v3/api").await;
    env.insert("SDKWORK_COMMERCE_APP_API_BASE_URL".to_string(), commerce_app);
    
    let commerce_backend = spawn_foundation_echo_upstream("/backend/v3/api").await;
    env.insert("SDKWORK_COMMERCE_BACKEND_API_BASE_URL".to_string(), commerce_backend);
    
    let cms_app = spawn_foundation_echo_upstream("/app/v3/api/cms").await;
    env.insert("SDKWORK_CMS_APP_API_BASE_URL".to_string(), cms_app);
    
    let cms_backend = spawn_foundation_echo_upstream("/backend/v3/api/cms").await;
    env.insert("SDKWORK_CMS_BACKEND_API_BASE_URL".to_string(), cms_backend);
    
    let cms_open = spawn_foundation_echo_upstream("/cms/v3/api").await;
    env.insert("SDKWORK_CMS_OPEN_API_BASE_URL".to_string(), cms_open);
    
    let forum_app = spawn_foundation_echo_upstream("/app/v3/api/forum").await;
    env.insert("SDKWORK_FORUM_APP_API_BASE_URL".to_string(), forum_app);
    
    let forum_backend = spawn_foundation_echo_upstream("/backend/v3/api/forum").await;
    env.insert("SDKWORK_FORUM_BACKEND_API_BASE_URL".to_string(), forum_backend);
    
    let forum_open = spawn_foundation_echo_upstream("/forum/v3/api").await;
    env.insert("SDKWORK_FORUM_OPEN_API_BASE_URL".to_string(), forum_open);
    
    let craw_chat_im = spawn_foundation_echo_upstream("/im/v3/api").await;
    env.insert("CRAW_CHAT_IM_HTTP_BASE_URL".to_string(), craw_chat_im);
    
    let craw_chat_backend = spawn_foundation_echo_upstream("/backend/v3/api/control").await;
    env.insert("CRAW_CHAT_BACKEND_HTTP_BASE_URL".to_string(), craw_chat_backend);
    
    let claw_gateway = spawn_foundation_echo_upstream("/v1").await;
    env.insert("SDKWORK_CLAW_GATEWAY_BASE_URL".to_string(), claw_gateway);
    
    let appstore_app = spawn_foundation_multi_route_echo_upstream(&[
        "/app/v3/api/catalog/probe",
        "/app/v3/api/catalog/{*rest}",
        "/app/v3/api/listings/probe",
        "/app/v3/api/listings/{*rest}",
        "/app/v3/api/publishers/probe",
        "/app/v3/api/publishers/{*rest}",
        "/app/v3/api/releases/probe",
        "/app/v3/api/releases/{*rest}",
        "/app/v3/api/compliance/probe",
        "/app/v3/api/compliance/{*rest}",
        "/app/v3/api/library/probe",
        "/app/v3/api/library/{*rest}",
        "/app/v3/api/wishlist/probe",
        "/app/v3/api/wishlist/{*rest}",
        "/app/v3/api/downloadGrants/probe",
        "/app/v3/api/downloadGrants/{*rest}",
    ])
    .await;
    env.insert("SDKWORK_APPSTORE_APP_API_BASE_URL".to_string(), appstore_app);
    
    let appstore_backend = spawn_foundation_multi_route_echo_upstream(&[
        "/backend/v3/api/moderation/probe",
        "/backend/v3/api/moderation/{*rest}",
        "/backend/v3/api/catalog/probe",
        "/backend/v3/api/catalog/{*rest}",
        "/backend/v3/api/listings/probe",
        "/backend/v3/api/listings/{*rest}",
        "/backend/v3/api/publishers/probe",
        "/backend/v3/api/publishers/{*rest}",
        "/backend/v3/api/market/probe",
        "/backend/v3/api/market/{*rest}",
        "/backend/v3/api/metrics/probe",
        "/backend/v3/api/metrics/{*rest}",
    ])
    .await;
    env.insert("SDKWORK_APPSTORE_BACKEND_API_BASE_URL".to_string(), appstore_backend);
    
    let appstore_open = spawn_foundation_multi_route_echo_upstream(&[
        "/store/v3/api/releases/probe",
        "/store/v3/api/releases/{*rest}",
        "/store/v3/api/catalog/probe",
        "/store/v3/api/catalog/{*rest}",
        "/store/v3/api/listings/probe",
        "/store/v3/api/listings/{*rest}",
        "/store/v3/api/automation/probe",
        "/store/v3/api/automation/{*rest}",
    ])
    .await;
    env.insert("SDKWORK_APPSTORE_OPEN_API_BASE_URL".to_string(), appstore_open);
    
    env
}

fn custom_dependency_surface() -> DependencyApiSurfaceConfig {
    DependencyApiSurfaceConfig {
        service_id: "custom-open-api".to_string(),
        workspace: "custom-service".to_string(),
        sdk_family: "sdkwork-custom-open-sdk".to_string(),
        api_authority: "custom-open-api".to_string(),
        surface: "open".to_string(),
        api_prefix: "/custom/v1".to_string(),
        runtime_mode: DependencyRuntimeMode::SplitOrEmbedded,
        same_origin_allowed: true,
        executable_export: Some("custom_service::build_router".to_string()),
        cargo_feature: None,
        cargo_dependency: None,
        coverage: "custom-open-api-routes".to_string(),
        required_base_url_key: Some("CUSTOM_OPEN_API_BASE_URL".to_string()),
    }
}

fn embedded_config_without_upstreams() -> GatewayRuntimeConfig {
    let mut config = GatewayRuntimeConfig {
        mode: GatewayMode::Embedded,
        ..GatewayRuntimeConfig::default()
    };
    config.upstreams.clear();
    config
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf8 body")
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    serde_json::from_slice(bytes.as_ref()).expect("json response")
}

#[tokio::test]
async fn preflight_fails_when_split_dependency_upstream_is_missing() {
    let mut config = GatewayRuntimeConfig::default();
    config.upstreams.clear();

    let error = GatewayRuntime::new(config)
        .expect_err("missing required split upstream should fail")
        .to_string();

    assert!(error.contains("sdkwork-appbase-app-api"));
    assert!(error.contains("upstream"));
}

#[tokio::test]
async fn gateway_exposes_health_ready_and_openapi_index() {
    let app = build_sdkwork_api_gateway_router(GatewayRuntimeConfig::default())
        .expect("default config builds");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(response_text(response).await.contains("\"status\":\"ok\""));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let readiness = response_json(response).await;
    assert_eq!(readiness["service"], "sdkwork-api-gateway");
    assert_eq!(readiness["routeCount"], 4);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/openapi/index.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(response_text(response)
        .await
        .contains("sdkwork-appbase-app-api"));
}

#[tokio::test]
async fn strict_cors_allows_configured_origin_and_rejects_others() {
    let mut config = GatewayRuntimeConfig::default();
    config.cors.allow_any_origin = false;
    config.cors.allowed_origins = vec!["https://console.example.com".to_string()];
    let app = build_sdkwork_api_gateway_router(config).expect("router builds");

    let allowed_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/healthz")
                .header(header::ORIGIN, "https://console.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed_response.status(), 200);
    assert_eq!(
        allowed_response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("https://console.example.com")
    );

    let blocked_response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/healthz")
                .header(header::ORIGIN, "https://evil.example.com")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(blocked_response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_none());
}

#[tokio::test]
async fn unknown_routes_return_problem_json() {
    let app = build_sdkwork_api_gateway_router(GatewayRuntimeConfig::default())
        .expect("default config builds");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "gateway_route_not_found");
    assert_eq!(problem["status"], 404);
}

#[tokio::test]
async fn secure_response_headers_are_added_when_enabled() {
    let mut config = GatewayRuntimeConfig::default();
    config.security.secure_headers_enabled = true;
    let app = build_sdkwork_api_gateway_router(config).expect("router builds");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        response
            .headers()
            .get("x-frame-options")
            .and_then(|value| value.to_str().ok()),
        Some("DENY")
    );
    assert_eq!(
        response
            .headers()
            .get("referrer-policy")
            .and_then(|value| value.to_str().ok()),
        Some("no-referrer")
    );
}

#[tokio::test]
async fn host_allowlist_rejects_untrusted_hosts() {
    let mut config = GatewayRuntimeConfig::default();
    config.security.allowed_hosts = vec!["api.example.com".to_string()];
    let app = build_sdkwork_api_gateway_router(config).expect("router builds");

    let rejected = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header(header::HOST, "evil.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), 421);
    assert_eq!(
        rejected
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = response_json(rejected).await;
    assert_eq!(problem["title"], "host_not_allowed");

    let allowed = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header(header::HOST, "api.example.com:443")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), 200);
}

#[tokio::test]
async fn header_count_and_byte_limits_reject_oversized_requests() {
    let mut count_config = GatewayRuntimeConfig::default();
    count_config.security.max_header_count = 0;
    let count_app = build_sdkwork_api_gateway_router(count_config).expect("router builds");

    let count_response = count_app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-extra", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(count_response.status(), 431);
    let count_problem = response_json(count_response).await;
    assert_eq!(count_problem["title"], "request_headers_too_large");

    let mut bytes_config = GatewayRuntimeConfig::default();
    bytes_config.security.max_header_bytes = 8;
    let bytes_app = build_sdkwork_api_gateway_router(bytes_config).expect("router builds");

    let bytes_response = bytes_app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-large", "abcdef")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bytes_response.status(), 431);
    let bytes_problem = response_json(bytes_response).await;
    assert_eq!(bytes_problem["title"], "request_headers_too_large");
}

#[tokio::test]
async fn waf_monitor_mode_marks_suspicious_query_without_blocking() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.security.waf.enabled = true;
    config.security.waf.mode = "monitor".to_string();
    config.security.waf.inspect_query = true;
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=waf-monitor&q=%3Cscript%3Ealert(1)%3C/script%3E")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("x-sdkwork-waf-action")
            .and_then(|value| value.to_str().ok()),
        Some("monitor")
    );
    assert_eq!(
        response_text(response).await,
        "trace=waf-monitor;body=hello"
    );
}

#[tokio::test]
async fn waf_enforce_mode_blocks_suspicious_query_with_problem_json() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.security.waf.enabled = true;
    config.security.waf.mode = "enforce".to_string();
    config.security.waf.inspect_query = true;
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=waf-block&q=%3Cscript%3Ealert(1)%3C/script%3E")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    assert_eq!(
        response
            .headers()
            .get("x-sdkwork-waf-action")
            .and_then(|value| value.to_str().ok()),
        Some("block")
    );
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "waf_request_blocked");
}

#[tokio::test]
async fn waf_enforce_mode_blocks_suspicious_headers_when_enabled() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.security.waf.enabled = true;
    config.security.waf.mode = "block".to_string();
    config.security.waf.inspect_headers = true;
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/request-id")
                .header("x-search", "' OR 1=1 --")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "waf_request_blocked");
}

#[tokio::test]
async fn embedded_mode_delegates_appbase_dependency_routes_to_supplied_router() {
    let config = embedded_config_without_upstreams();

    let appbase_router = Router::new().route(
        "/app/v3/api/auth/ping",
        get(|| async { "embedded-appbase" }),
    );

    let app = build_sdkwork_api_gateway_router_with_embedded_routers(
        config,
        [("sdkwork-appbase-app-api", appbase_router)],
    )
    .expect("embedded router satisfies dependency");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/app/v3/api/auth/ping")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response_text(response).await, "embedded-appbase");
}

#[tokio::test]
async fn split_mode_proxies_dependency_route_to_configured_upstream() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=proxy")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response_text(response).await, "trace=proxy;body=hello");
}

#[tokio::test]
async fn metrics_endpoint_is_hidden_by_default_and_exposed_when_enabled() {
    let appbase_router = Router::new().route(
        "/app/v3/api/auth/ping",
        get(|| async { "embedded-appbase" }),
    );
    let hidden_app = build_sdkwork_api_gateway_router_with_embedded_routers(
        embedded_config_without_upstreams(),
        [("sdkwork-appbase-app-api", appbase_router.clone())],
    )
    .expect("embedded router builds");

    let hidden_response = hidden_app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hidden_response.status(), 404);

    let mut config = embedded_config_without_upstreams();
    config.observability.metrics_enabled = true;
    config.observability.metrics_path = "/internal/metrics".to_string();
    let metrics_app = build_sdkwork_api_gateway_router_with_embedded_routers(
        config,
        [("sdkwork-appbase-app-api", appbase_router)],
    )
    .expect("embedded metrics router builds");

    let metrics_response = metrics_app
        .oneshot(
            Request::builder()
                .uri("/internal/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(metrics_response.status(), 200);
    assert!(metrics_response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .starts_with("text/plain"));
    let metrics = response_text(metrics_response).await;
    assert!(metrics.contains("# HELP sdkwork_api_gateway_http_requests_total"));
    assert!(metrics.contains("sdkwork_api_gateway_build_info"));
}

#[tokio::test]
async fn diagnostics_endpoint_is_hidden_by_default() {
    let app = build_sdkwork_api_gateway_router(GatewayRuntimeConfig::default())
        .expect("default router builds");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/internal/gateway/diagnostics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn diagnostics_endpoint_returns_safe_policy_and_route_inventory_when_enabled() {
    let config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [service]
        name = "sdkwork-api-gateway"
        environment = "test"

        [[upstreams]]
        serviceId = "sdkwork-appbase-app-api"
        baseUrl = "http://127.0.0.1:4800"

        [diagnostics]
        enabled = true
        path = "internal/gateway/inventory"
        includePolicyInventory = true
        includeRouteInventory = true

        [policy]
        enabled = true
        dryRun = true

        [[policy.rules]]
        id = "auth-post-only"
        matchServiceId = "sdkwork-appbase-app-api"
        matchPathPrefix = "/app/v3/api/auth"
        allowedMethods = ["POST"]
        maxBodyBytes = 4096
        timeoutMs = 250
        "#,
    )
    .expect("diagnostics config parses");

    let app = build_sdkwork_api_gateway_router(config).expect("diagnostics router builds");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/internal/gateway/inventory")
                .header("authorization", "Bearer secret-token")
                .header(
                    "traceparent",
                    "00-abcdefabcdefabcdefabcdefabcdefab-abcdefabcdefabcd-01",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );

    let body = response_text(response).await;
    assert!(!body.contains("http://127.0.0.1:4800"));
    assert!(!body.contains("secret-token"));
    assert!(!body.contains("traceparent"));
    assert!(!body.contains("abcdefabcdef"));

    let diagnostics: serde_json::Value =
        serde_json::from_str(body.as_str()).expect("diagnostics json parses");
    assert_eq!(diagnostics["service"]["name"], "sdkwork-api-gateway");
    assert_eq!(diagnostics["service"]["environment"], "test");
    assert_eq!(diagnostics["service"]["mode"], "split");
    assert_eq!(diagnostics["dependencySurfaces"]["count"], 1);
    assert_eq!(diagnostics["routes"]["count"], 4);
    let auth_route = diagnostics["routes"]["items"]
        .as_array()
        .expect("route items array")
        .iter()
        .find(|route| route["pathPattern"] == "/app/v3/api/auth/{*path}")
        .expect("auth route inventory item");
    assert_eq!(auth_route["pathPattern"], "/app/v3/api/auth/{*path}");
    assert_eq!(auth_route["executionTarget"], "dependency");
    assert_eq!(diagnostics["policy"]["enabled"], true);
    assert_eq!(diagnostics["policy"]["dryRun"], true);
    assert_eq!(diagnostics["policy"]["ruleCount"], 1);
    assert_eq!(diagnostics["policy"]["rules"][0]["id"], "auth-post-only");
    assert_eq!(
        diagnostics["policy"]["rules"][0]["matchServiceId"],
        "sdkwork-appbase-app-api"
    );
    assert_eq!(
        diagnostics["policy"]["rules"][0]["matchPathPrefix"],
        "/app/v3/api/auth"
    );
    assert_eq!(
        diagnostics["policy"]["rules"][0]["allowedMethods"][0],
        "POST"
    );
    assert_eq!(diagnostics["policy"]["rules"][0]["hasMaxBodyBytes"], true);
    assert_eq!(diagnostics["policy"]["rules"][0]["hasTimeoutMs"], true);
    assert!(diagnostics.get("upstreams").is_none());
}

#[tokio::test]
async fn split_mode_metrics_count_proxied_dependency_requests() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.observability.metrics_enabled = true;
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=metrics")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let metrics_response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(metrics_response.status(), 200);
    let metrics = response_text(metrics_response).await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_http_requests_total{service=\"sdkwork-api-gateway\",method=\"POST\",route_target=\"dependency\",service_id=\"sdkwork-appbase-app-api\",status=\"200\"} 1"
    ));
    assert!(metrics.contains(
        "sdkwork_api_gateway_upstream_requests_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",status=\"200\"} 1"
    ));
    assert!(metrics.contains(
        "sdkwork_api_gateway_http_request_duration_seconds_bucket{service=\"sdkwork-api-gateway\",method=\"POST\",route_target=\"dependency\",service_id=\"sdkwork-appbase-app-api\",status=\"200\",le=\"+Inf\"} 1"
    ));
    assert!(metrics.contains(
        "sdkwork_api_gateway_http_request_duration_seconds_count{service=\"sdkwork-api-gateway\",method=\"POST\",route_target=\"dependency\",service_id=\"sdkwork-appbase-app-api\",status=\"200\"} 1"
    ));
    assert!(metrics.contains(
        "sdkwork_api_gateway_http_request_duration_seconds_sum{service=\"sdkwork-api-gateway\",method=\"POST\",route_target=\"dependency\",service_id=\"sdkwork-appbase-app-api\",status=\"200\"}"
    ));
}

#[tokio::test]
async fn metrics_count_waf_monitor_and_block_events() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut monitor_config = GatewayRuntimeConfig::default();
    monitor_config.observability.metrics_enabled = true;
    monitor_config.security.waf.enabled = true;
    monitor_config.security.waf.mode = "monitor".to_string();
    monitor_config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url.clone();

    let monitor_app = build_sdkwork_api_gateway_router(monitor_config).expect("router builds");
    let monitor_response = monitor_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=waf-metrics&q=%3Cscript%3E")
                .body(Body::from("monitor"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(monitor_response.status(), 200);

    let monitor_metrics = response_text(
        monitor_app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert!(monitor_metrics.contains(
        "sdkwork_api_gateway_security_events_total{service=\"sdkwork-api-gateway\",kind=\"waf\",action=\"monitor\",rule=\"baseline-signature\"} 1"
    ));

    let mut block_config = GatewayRuntimeConfig::default();
    block_config.observability.metrics_enabled = true;
    block_config.security.waf.enabled = true;
    block_config.security.waf.mode = "block".to_string();
    block_config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let block_app = build_sdkwork_api_gateway_router(block_config).expect("router builds");
    let block_response = block_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=waf-metrics&q=%3Cscript%3E")
                .body(Body::from("block"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(block_response.status(), 403);

    let block_metrics = response_text(
        block_app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert!(block_metrics.contains(
        "sdkwork_api_gateway_security_events_total{service=\"sdkwork-api-gateway\",kind=\"waf\",action=\"block\",rule=\"baseline-signature\"} 1"
    ));
}

#[tokio::test]
async fn metrics_count_rate_limit_rejections() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.observability.metrics_enabled = true;
    config.traffic.rate_limit_enabled = true;
    config.traffic.rate_limits.push(RateLimitRuleConfig {
        id: "auth-global".to_string(),
        match_path_prefix: Some("/app/v3/api/auth".to_string()),
        match_service_id: None,
        key: "global".to_string(),
        algorithm: "token-bucket".to_string(),
        capacity: Some(1),
        refill_per_second: Some(0),
        burst: Some(1),
        limit: None,
        window_seconds: None,
        reject_status: 429,
    });
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let allowed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=rate-limit-metrics")
                .body(Body::from("first"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), 200);

    let rejected = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=rate-limit-metrics")
                .body(Body::from("second"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), 429);

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_traffic_rejections_total{service=\"sdkwork-api-gateway\",kind=\"rate_limit\",scope=\"auth-global\",status=\"429\"} 1"
    ));
}

#[tokio::test]
async fn route_prefix_rate_limit_rejects_with_standard_headers() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.traffic.rate_limit_enabled = true;
    config.traffic.rate_limits.push(RateLimitRuleConfig {
        id: "auth-global".to_string(),
        match_path_prefix: Some("/app/v3/api/auth".to_string()),
        match_service_id: None,
        key: "global".to_string(),
        algorithm: "token-bucket".to_string(),
        capacity: Some(1),
        refill_per_second: Some(0),
        burst: Some(1),
        limit: None,
        window_seconds: None,
        reject_status: 429,
    });
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let allowed = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=rate-limit")
                .body(Body::from("first"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), 200);

    let rejected = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=rate-limit")
                .body(Body::from("second"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(rejected.status(), 429);
    assert_eq!(
        rejected
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
    assert!(rejected
        .headers()
        .get("ratelimit")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .contains("remaining=0"));
    assert!(rejected
        .headers()
        .get("ratelimit-policy")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .contains("auth-global"));
    let problem = response_json(rejected).await;
    assert_eq!(problem["title"], "rate_limit_exceeded");
}

#[tokio::test]
async fn global_concurrency_limit_rejects_requests_over_capacity() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.traffic.global_max_in_flight = Some(1);
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway");
    let address = listener.local_addr().expect("gateway local address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("gateway concurrency server");
    });
    let first_client = reqwest::Client::new();
    let second_client = reqwest::Client::new();
    let first = tokio::spawn(async move {
        first_client
            .post(format!("http://{address}/app/v3/api/auth/slow"))
            .send()
            .await
    });
    tokio::time::sleep(Duration::from_millis(10)).await;

    let rejected = tokio::time::timeout(
        Duration::from_secs(2),
        second_client
            .post(format!("http://{address}/app/v3/api/auth/request-id"))
            .send(),
    )
    .await
    .expect("second request completes")
    .unwrap();
    assert_eq!(rejected.status().as_u16(), 503);
    let problem = rejected
        .json::<serde_json::Value>()
        .await
        .expect("problem json");
    assert_eq!(problem["title"], "concurrency_limit_exceeded");

    let first_response = tokio::time::timeout(Duration::from_secs(2), first)
        .await
        .expect("first request completes")
        .expect("first request task joins")
        .unwrap();
    assert_eq!(first_response.status().as_u16(), 200);
}

#[tokio::test]
async fn split_mode_adds_forwarded_headers_to_upstream_requests() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/forwarded")
                .header("host", "api.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response_text(response).await,
        "host=api.example.com;proto=http"
    );
}

#[tokio::test]
async fn split_mode_generates_request_id_when_missing_and_forwards_it_upstream() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/request-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let request_id = response_text(response).await;
    assert!(request_id.starts_with("sdkwork-api-gateway-"));
    assert!(request_id.len() > "sdkwork-api-gateway-".len());
}

#[tokio::test]
async fn split_mode_forwards_traceparent_to_upstream_requests() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let traceparent = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/traceparent")
                .header("traceparent", traceparent)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response_text(response).await, traceparent);
}

#[tokio::test]
async fn problem_responses_include_trace_id_when_traceparent_is_present() {
    let app = build_sdkwork_api_gateway_router(GatewayRuntimeConfig::default())
        .expect("default config builds");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/missing")
                .header(
                    "traceparent",
                    "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "gateway_route_not_found");
    assert_eq!(problem["traceId"], "4bf92f3577b34da6a3ce929d0e0e4736");
}

#[tokio::test]
async fn split_mode_returns_request_id_on_upstream_connection_failure_problem() {
    let mut config = GatewayRuntimeConfig::default();
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = "http://127.0.0.1:9".to_string();

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo")
                .header("x-request-id", "request-failure-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 502);
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("request-failure-1")
    );
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "upstream_request_failed");
}

#[tokio::test]
async fn split_mode_honors_per_upstream_timeout() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    let upstream = config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream");
    upstream.base_url = upstream_base_url;
    upstream.timeout_ms = 10;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/slow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 504);
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    assert!(request_id.starts_with("sdkwork-api-gateway-"));
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "upstream_timeout");
}

#[tokio::test]
async fn split_mode_honors_gateway_request_timeout_before_upstream_timeout() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.request.timeout_ms = 10;
    let upstream = config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream");
    upstream.base_url = upstream_base_url;
    upstream.timeout_ms = 1_000;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/slow")
                .header("x-request-id", "gateway-timeout-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 504);
    assert_eq!(
        response
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
        Some("gateway-timeout-1")
    );
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "gateway_request_timeout");
    assert_eq!(problem["status"], 504);
}

#[tokio::test]
async fn metrics_count_gateway_request_timeouts() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.observability.metrics_enabled = true;
    config.request.timeout_ms = 10;
    let upstream = config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream");
    upstream.base_url = upstream_base_url;
    upstream.timeout_ms = 1_000;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/slow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 504);

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_resilience_events_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",event=\"gateway_request_timeout\"} 1"
    ));
}

#[tokio::test]
async fn metrics_count_upstream_timeouts() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.observability.metrics_enabled = true;
    let upstream = config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream");
    upstream.base_url = upstream_base_url;
    upstream.timeout_ms = 10;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/slow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 504);
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "upstream_timeout");

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_resilience_events_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",event=\"upstream_timeout\"} 1"
    ));
}

#[tokio::test]
async fn metrics_count_retry_attempts_and_exhaustion() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let upstream_base_url = spawn_failing_circuit_upstream(attempts.clone()).await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        metricsEnabled = true

        [resilience]
        retryEnabled = true
        maxAttempts = 2
        retryStatuses = [503]
        retryBackoffMs = 0
        "#,
    )
    .expect("config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/failing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert_eq!(response_text(response).await, "failing-attempt=2");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_resilience_events_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",event=\"retry_attempt\"} 1"
    ));
    assert!(metrics.contains(
        "sdkwork_api_gateway_resilience_events_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",event=\"retry_exhausted\"} 1"
    ));
}

#[tokio::test]
async fn split_mode_retries_retryable_upstream_statuses_when_enabled() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let upstream_base_url = spawn_flaky_retry_upstream(attempts.clone()).await;
    let mut config = GatewayRuntimeConfig::default();
    config.resilience.retry_enabled = true;
    config.resilience.max_attempts = 2;
    config.resilience.retry_statuses = vec![503];
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response_text(response).await, "attempt=2");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn split_mode_opens_circuit_after_repeated_upstream_failures() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let upstream_base_url = spawn_failing_circuit_upstream(attempts.clone()).await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [resilience]
        circuitBreakerEnabled = true
        failureThreshold = 2
        openSeconds = 30
        halfOpenMaxAttempts = 1
        "#,
    )
    .expect("config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    for expected_attempt in 1..=2 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/app/v3/api/auth/failing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 503);
        assert_eq!(
            response_text(response).await,
            format!("failing-attempt={expected_attempt}")
        );
    }

    let rejected = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/failing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(rejected.status(), 503);
    assert_eq!(
        rejected
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/problem+json")
    );
    assert_eq!(
        rejected
            .headers()
            .get("x-sdkwork-circuit-state")
            .and_then(|value| value.to_str().ok()),
        Some("open")
    );
    let problem = response_json(rejected).await;
    assert_eq!(problem["title"], "circuit_breaker_open");
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn split_mode_allows_half_open_probe_and_closes_circuit_after_success() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let upstream_base_url = spawn_flaky_retry_upstream(attempts.clone()).await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [resilience]
        circuitBreakerEnabled = true
        failureThreshold = 1
        openSeconds = 0
        halfOpenMaxAttempts = 1
        "#,
    )
    .expect("config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), 503);
    assert_eq!(response_text(first).await, "attempt=1");

    let half_open = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(half_open.status(), 200);
    assert_eq!(
        half_open
            .headers()
            .get("x-sdkwork-circuit-state")
            .and_then(|value| value.to_str().ok()),
        Some("half-open")
    );
    assert_eq!(response_text(half_open).await, "attempt=2");

    let closed = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(closed.status(), 200);
    assert!(closed.headers().get("x-sdkwork-circuit-state").is_none());
    assert_eq!(response_text(closed).await, "attempt=3");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn metrics_count_circuit_breaker_short_circuit_and_half_open_probe() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let upstream_base_url = spawn_flaky_retry_upstream(attempts.clone()).await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        metricsEnabled = true

        [resilience]
        circuitBreakerEnabled = true
        failureThreshold = 1
        openSeconds = 30
        halfOpenMaxAttempts = 1
        "#,
    )
    .expect("config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("split router builds");
    let failing = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failing.status(), 503);

    let short_circuited = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(short_circuited.status(), 503);
    assert_eq!(
        short_circuited
            .headers()
            .get("x-sdkwork-circuit-state")
            .and_then(|value| value.to_str().ok()),
        Some("open")
    );

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_resilience_events_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",event=\"circuit_open\"} 1"
    ));
    assert!(metrics.contains(
        "sdkwork_api_gateway_resilience_events_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",event=\"circuit_short_circuit\"} 1"
    ));

    let attempts = Arc::new(AtomicUsize::new(0));
    let upstream_base_url = spawn_flaky_retry_upstream(attempts.clone()).await;
    let mut half_open_config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        metricsEnabled = true

        [resilience]
        circuitBreakerEnabled = true
        failureThreshold = 1
        openSeconds = 0
        halfOpenMaxAttempts = 1
        "#,
    )
    .expect("config parses");
    half_open_config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let half_open_app =
        build_sdkwork_api_gateway_router(half_open_config).expect("split router builds");
    let failing = half_open_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failing.status(), 503);

    let probe = half_open_app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/flaky")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(probe.status(), 200);
    assert_eq!(
        probe
            .headers()
            .get("x-sdkwork-circuit-state")
            .and_then(|value| value.to_str().ok()),
        Some("half-open")
    );

    let metrics = response_text(
        half_open_app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_resilience_events_total{service=\"sdkwork-api-gateway\",service_id=\"sdkwork-appbase-app-api\",event=\"circuit_half_open_probe\"} 1"
    ));
}

#[tokio::test]
async fn readyz_can_check_required_upstream_health() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.readiness.check_upstreams = true;
    let upstream = config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream");
    upstream.base_url = upstream_base_url;
    upstream.readiness_path = "/healthz".to_string();

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let readiness = response_json(response).await;
    assert_eq!(readiness["status"], "ok");
    assert_eq!(readiness["upstreamChecks"][0]["status"], "ok");
}

#[tokio::test]
async fn readyz_returns_unavailable_when_required_upstream_health_fails() {
    let mut config = GatewayRuntimeConfig::default();
    config.readiness.check_upstreams = true;
    let upstream = config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream");
    upstream.base_url = "http://127.0.0.1:9".to_string();
    upstream.readiness_path = "/healthz".to_string();

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 503);
    let readiness = response_json(response).await;
    assert_eq!(readiness["status"], "degraded");
    assert_eq!(readiness["upstreamChecks"][0]["status"], "unavailable");
}

#[tokio::test]
async fn split_mode_rejects_request_bodies_over_configured_limit() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.request.max_body_bytes = 4;
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 413);
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "request_body_too_large");
}

#[tokio::test]
async fn policy_method_allowlist_rejects_disallowed_dependency_methods_and_records_metrics() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        metricsEnabled = true

        [policy]
        enabled = true

        [[policy.rules]]
        id = "auth-post-only"
        matchServiceId = "sdkwork-appbase-app-api"
        matchPathPrefix = "/app/v3/api/auth"
        allowedMethods = ["POST"]
        "#,
    )
    .expect("policy config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/app/v3/api/auth/echo")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 405);
    assert_eq!(
        response
            .headers()
            .get(header::ALLOW)
            .and_then(|value| value.to_str().ok()),
        Some("POST")
    );
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "method_not_allowed");

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_policy_rejections_total{service=\"sdkwork-api-gateway\",kind=\"method_not_allowed\",scope=\"auth-post-only\",status=\"405\"} 1"
    ));
}

#[tokio::test]
async fn policy_dry_run_allows_disallowed_method_and_records_observation_metrics() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        metricsEnabled = true

        [policy]
        enabled = true
        dryRun = true

        [[policy.rules]]
        id = "auth-post-only"
        matchServiceId = "sdkwork-appbase-app-api"
        matchPathPrefix = "/app/v3/api/auth"
        allowedMethods = ["POST"]
        "#,
    )
    .expect("policy config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/app/v3/api/auth/echo?trace=dry-run")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("x-sdkwork-policy-action")
            .and_then(|value| value.to_str().ok()),
        Some("dry-run")
    );
    assert_eq!(
        response
            .headers()
            .get("x-sdkwork-policy-rule")
            .and_then(|value| value.to_str().ok()),
        Some("auth-post-only")
    );
    assert_eq!(response_text(response).await, "trace=dry-run;body=hello");

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_policy_dry_run_total{service=\"sdkwork-api-gateway\",kind=\"method_not_allowed\",scope=\"auth-post-only\",status=\"200\"} 1"
    ));
    assert!(!metrics.contains("sdkwork_api_gateway_policy_rejections_total{service=\"sdkwork-api-gateway\",kind=\"method_not_allowed\",scope=\"auth-post-only\""));
}

#[tokio::test]
async fn policy_body_limit_overrides_global_limit_and_records_metrics() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        metricsEnabled = true

        [request]
        maxBodyBytes = 1048576

        [policy]
        enabled = true

        [[policy.rules]]
        id = "auth-small-body"
        matchServiceId = "sdkwork-appbase-app-api"
        matchPathPrefix = "/app/v3/api/auth"
        maxBodyBytes = 4
        "#,
    )
    .expect("policy config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 413);
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "request_body_too_large");

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_policy_rejections_total{service=\"sdkwork-api-gateway\",kind=\"request_body_too_large\",scope=\"auth-small-body\",status=\"413\"} 1"
    ));
}

#[tokio::test]
async fn policy_dry_run_allows_policy_body_limit_overflow_and_records_observation_metrics() {
    let upstream_base_url = spawn_appbase_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [observability]
        metricsEnabled = true

        [request]
        maxBodyBytes = 1048576

        [policy]
        enabled = true
        dryRun = true

        [[policy.rules]]
        id = "auth-small-body"
        matchServiceId = "sdkwork-appbase-app-api"
        matchPathPrefix = "/app/v3/api/auth"
        maxBodyBytes = 4
        "#,
    )
    .expect("policy config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=body-dry-run")
                .header(header::CONTENT_LENGTH, "5")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response
            .headers()
            .get("x-sdkwork-policy-action")
            .and_then(|value| value.to_str().ok()),
        Some("dry-run")
    );
    assert_eq!(
        response
            .headers()
            .get("x-sdkwork-policy-kind")
            .and_then(|value| value.to_str().ok()),
        Some("request_body_too_large")
    );
    assert_eq!(
        response_text(response).await,
        "trace=body-dry-run;body=hello"
    );

    let metrics = response_text(
        app.oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
    )
    .await;
    assert!(metrics.contains(
        "sdkwork_api_gateway_policy_dry_run_total{service=\"sdkwork-api-gateway\",kind=\"request_body_too_large\",scope=\"auth-small-body\",status=\"200\"} 1"
    ));
    assert!(!metrics.contains("sdkwork_api_gateway_policy_rejections_total{service=\"sdkwork-api-gateway\",kind=\"request_body_too_large\",scope=\"auth-small-body\""));
}

#[tokio::test]
async fn policy_timeout_overrides_global_timeout_for_dependency_route() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::from_toml_str(
        r#"
        [request]
        timeoutMs = 0

        [policy]
        enabled = true

        [[policy.rules]]
        id = "auth-fast-timeout"
        matchServiceId = "sdkwork-appbase-app-api"
        matchPathPrefix = "/app/v3/api/auth/slow"
        timeoutMs = 5
        "#,
    )
    .expect("policy config parses");
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/slow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 504);
    let problem = response_json(response).await;
    assert_eq!(problem["title"], "gateway_request_timeout");
}

#[tokio::test]
async fn split_mode_returns_upstream_response_before_stream_body_completes() {
    let upstream_base_url = spawn_forwarded_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config
        .upstreams
        .iter_mut()
        .find(|upstream| upstream.service_id == "sdkwork-appbase-app-api")
        .expect("default appbase upstream")
        .base_url = upstream_base_url;

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = tokio::time::timeout(
        Duration::from_millis(200),
        app.oneshot(
            Request::builder()
                .method("GET")
                .uri("/app/v3/api/auth/stream")
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .expect("proxy response should return before upstream stream body finishes")
    .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response_text(response).await, "firstsecond");
}

#[tokio::test]
async fn split_mode_proxies_configured_dependency_prefix_to_upstream() {
    let upstream_base_url = spawn_dependency_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.dependency_surfaces.push(custom_dependency_surface());
    config.upstreams.push(ServiceUpstreamConfig::new(
        "custom-open-api",
        upstream_base_url,
    ));

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/custom/v1/echo?dependency=configured")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response_text(response).await,
        "dependency=configured;body=hello"
    );
}

#[tokio::test]
async fn split_mode_proxies_manifest_drive_and_notary_surfaces_to_gateway_upstreams() {
    let env = spawn_all_foundation_upstreams().await;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let config = GatewayRuntimeConfig::from_application_root(root)
        .expect("manifest config parses")
        .with_env_overrides(&env)
        .expect("foundation upstream env overrides apply");

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let drive_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/drive/probe?dependency=drive")
                .body(Body::from("hello-drive"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(drive_response.status(), 200);
    assert_eq!(
        response_text(drive_response).await,
        "dependency=drive;body=hello-drive"
    );

    let notary_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/notary/probe?dependency=notary")
                .body(Body::from("hello-notary"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(notary_response.status(), 200);
    assert_eq!(
        response_text(notary_response).await,
        "dependency=notary;body=hello-notary"
    );
}

#[tokio::test]
async fn split_mode_proxies_manifest_claw_router_foundation_surfaces_with_route_precedence() {
    let env = spawn_all_foundation_upstreams().await;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let config = GatewayRuntimeConfig::from_application_root(root)
        .expect("manifest config parses")
        .with_env_overrides(&env)
        .expect("foundation upstream env overrides apply");

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let appbase_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/echo?trace=appbase")
                .body(Body::from("hello-appbase"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(appbase_response.status(), 200);
    assert_eq!(
        response_text(appbase_response).await,
        "trace=appbase;body=hello-appbase"
    );

    let drive_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/drive/probe?dependency=drive")
                .body(Body::from("hello-drive"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(drive_response.status(), 200);
    assert_eq!(
        response_text(drive_response).await,
        "dependency=drive;body=hello-drive"
    );

    let appbase_backend_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/backend/v3/api/iam/probe?dependency=appbase-backend")
                .body(Body::from("hello-appbase-backend"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(appbase_backend_response.status(), 200);
    assert_eq!(
        response_text(appbase_backend_response).await,
        "dependency=appbase-backend;body=hello-appbase-backend"
    );

    let commerce_app_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/orders/probe?dependency=commerce-app")
                .body(Body::from("hello-commerce-app"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(commerce_app_response.status(), 200);
    assert_eq!(
        response_text(commerce_app_response).await,
        "dependency=commerce-app;body=hello-commerce-app"
    );

    let commerce_backend_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/backend/v3/api/orders/probe?dependency=commerce-backend")
                .body(Body::from("hello-commerce-backend"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(commerce_backend_response.status(), 200);
    assert_eq!(
        response_text(commerce_backend_response).await,
        "dependency=commerce-backend;body=hello-commerce-backend"
    );
}

#[tokio::test]
async fn split_mode_proxies_manifest_craw_chat_foundation_surfaces_with_route_precedence() {
    let env = spawn_all_foundation_upstreams().await;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let config = GatewayRuntimeConfig::from_application_root(root)
        .expect("manifest config parses")
        .with_env_overrides(&env)
        .expect("foundation upstream env overrides apply");

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    for (uri, dependency, body) in [
        (
            "/app/v3/api/rtc/probe?dependency=rtc-app",
            "rtc-app",
            "hello-rtc-app",
        ),
        (
            "/backend/v3/api/rtc/probe?dependency=rtc-backend",
            "rtc-backend",
            "hello-rtc-backend",
        ),
        (
            "/app/v3/api/ai/probe?dependency=agent-app",
            "agent-app",
            "hello-agent-app",
        ),
        (
            "/backend/v3/api/ai/probe?dependency=agent-backend",
            "agent-backend",
            "hello-agent-backend",
        ),
        (
            "/agent/v3/api/probe?dependency=agent-open",
            "agent-open",
            "hello-agent-open",
        ),
        (
            "/app/v3/api/iot/probe?dependency=aiot-app",
            "aiot-app",
            "hello-aiot-app",
        ),
        (
            "/backend/v3/api/iot/probe?dependency=aiot-backend",
            "aiot-backend",
            "hello-aiot-backend",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response_text(response).await,
            format!("dependency={dependency};body={body}")
        );
    }
}

#[tokio::test]
async fn split_mode_proxies_workspace_foundation_surfaces_with_route_precedence() {
    let env = spawn_all_foundation_upstreams().await;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let config = GatewayRuntimeConfig::from_application_root(root)
        .expect("manifest config parses")
        .with_env_overrides(&env)
        .expect("foundation upstream env overrides apply");

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    for (uri, dependency, body) in [
        (
            "/app/v3/api/memory/probe?dependency=memory-app",
            "memory-app",
            "hello-memory-app",
        ),
        (
            "/backend/v3/api/drive/probe?dependency=drive-backend",
            "drive-backend",
            "hello-drive-backend",
        ),
        (
            "/knowledge/v3/api/probe?dependency=knowledgebase-open",
            "knowledgebase-open",
            "hello-knowledgebase-open",
        ),
        (
            "/open/v3/api/news/probe?dependency=news-open",
            "news-open",
            "hello-news-open",
        ),
        (
            "/admin/v3/api/drive/probe?dependency=drive-admin",
            "drive-admin",
            "hello-drive-admin",
        ),
        (
            "/app/v3/api/community/probe?dependency=community-app",
            "community-app",
            "hello-community-app",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response_text(response).await,
            format!("dependency={dependency};body={body}")
        );
    }
}

#[tokio::test]
async fn split_mode_proxies_materialized_content_foundation_surfaces_with_route_precedence() {
    let env = spawn_all_foundation_upstreams().await;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");

    let config = GatewayRuntimeConfig::from_application_root(root)
        .expect("manifest config parses")
        .with_env_overrides(&env)
        .expect("materialized content foundation upstream env overrides apply");

    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    for (uri, dependency, body) in [
        (
            "/app/v3/api/search/probe?dependency=search-app",
            "search-app",
            "hello-search-app",
        ),
        (
            "/backend/v3/api/voice/probe?dependency=voice-backend",
            "voice-backend",
            "hello-voice-backend",
        ),
        (
            "/image/v3/api/probe?dependency=image-open",
            "image-open",
            "hello-image-open",
        ),
        (
            "/app/v3/api/comments/probe?dependency=comments-app",
            "comments-app",
            "hello-comments-app",
        ),
        (
            "/app/v3/api/engagement/probe?dependency=comments-engagement",
            "comments-engagement",
            "hello-comments-engagement",
        ),
        (
            "/backend/v3/api/courses/probe?dependency=course-backend",
            "course-backend",
            "hello-course-backend",
        ),
        (
            "/backend/v3/api/course_lessons/probe?dependency=course-lessons",
            "course-lessons",
            "hello-course-lessons",
        ),
        (
            "/app/v3/api/messaging/probe?dependency=messaging-app",
            "messaging-app",
            "hello-messaging-app",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response_text(response).await,
            format!("dependency={dependency};body={body}")
        );
    }
}

#[tokio::test]
async fn embedded_mode_delegates_configured_dependency_prefix_to_supplied_router() {
    let config = GatewayRuntimeConfig {
        mode: GatewayMode::Embedded,
        upstreams: Vec::new(),
        dependency_surfaces: vec![custom_dependency_surface()],
        ..GatewayRuntimeConfig::default()
    };

    let dependency_router = Router::new().route(
        "/custom/v1/ping",
        get(|| async { "embedded-custom-dependency" }),
    );

    let app = build_sdkwork_api_gateway_router_with_embedded_routers(
        config,
        [("custom-open-api", dependency_router)],
    )
    .expect("embedded dependency satisfies config");

    let response = app
        .oneshot(
            Request::builder()
                .uri("/custom/v1/ping")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(response_text(response).await, "embedded-custom-dependency");
}

#[tokio::test]
async fn split_mode_proxies_configured_websocket_dependency_to_upstream() {
    let upstream_base_url = spawn_websocket_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.dependency_surfaces.push(custom_dependency_surface());
    config.upstreams.push(ServiceUpstreamConfig::new(
        "custom-open-api",
        upstream_base_url,
    ));
    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway");
    let address = listener.local_addr().expect("gateway local address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("gateway websocket server");
    });

    let (mut websocket, _) = connect_async(format!("ws://{address}/custom/v1/ws"))
        .await
        .expect("connect through gateway websocket");
    websocket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            "hello-ws".into(),
        ))
        .await
        .expect("send websocket message");
    let message = websocket
        .next()
        .await
        .expect("websocket response")
        .expect("websocket response ok");

    assert_eq!(
        message,
        tokio_tungstenite::tungstenite::Message::Text("hello-ws".into())
    );
}

#[tokio::test]
async fn split_mode_preserves_websocket_close_frame_details() {
    let upstream_base_url = spawn_websocket_echo_upstream().await;
    let mut config = GatewayRuntimeConfig::default();
    config.dependency_surfaces.push(custom_dependency_surface());
    config.upstreams.push(ServiceUpstreamConfig::new(
        "custom-open-api",
        upstream_base_url,
    ));
    let app = build_sdkwork_api_gateway_router(config).expect("router builds");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind gateway");
    let address = listener.local_addr().expect("gateway local address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("gateway websocket server");
    });

    let (mut websocket, _) = connect_async(format!("ws://{address}/custom/v1/ws"))
        .await
        .expect("connect through gateway websocket");
    let close_frame = CloseFrame {
        code: CloseCode::Policy,
        reason: "policy-match".into(),
    };
    websocket
        .send(Message::Close(Some(close_frame.clone())))
        .await
        .expect("send websocket close frame");
    let message = websocket
        .next()
        .await
        .expect("websocket close response")
        .expect("websocket close response ok");

    assert_eq!(message, Message::Close(Some(close_frame)));
}

#[tokio::test]
async fn api_server_router_auto_mounts_appbase_when_running_embedded() {
    let config = embedded_config_without_upstreams();

    let app = build_sdkwork_api_gateway_api_server_router(config)
        .expect("api-server router mounts appbase");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/sessions")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), 404);
    assert_ne!(response.status(), 502);
}

#[tokio::test]
async fn embedded_appbase_runtime_context_builder_exports_executable_router() {
    let context = SdkworkAppbaseLocalIamRuntimeContext::new(
        "sdkwork-api-gateway",
        "tenant_gateway",
        "org_gateway",
    );
    let appbase_router =
        build_embedded_sdkwork_appbase_app_api_router_with_runtime_context(context);

    let config = embedded_config_without_upstreams();
    let app = build_sdkwork_api_gateway_router_with_embedded_routers(
        config,
        [("sdkwork-appbase-app-api", appbase_router)],
    )
    .expect("runtime context router satisfies dependency");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/app/v3/api/auth/sessions")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(response.status(), 404);
    assert_ne!(response.status(), 502);
}
