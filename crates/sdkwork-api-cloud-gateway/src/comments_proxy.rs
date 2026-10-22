use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderName, HeaderValue, Request, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;

const COMMENTS_RUNTIME_DEFAULT: &str = "http://127.0.0.1:3901";
const COMMENTS_PATH_PREFIXES: [&str; 2] = ["/app/v3/api/comments", "/app/v3/api/engagement"];

fn comments_runtime_base() -> String {
    std::env::var("SDKWORK_COMMENTS_RUNTIME_URL")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| COMMENTS_RUNTIME_DEFAULT.to_owned())
}

pub(crate) fn is_comments_path(path: &str) -> bool {
    COMMENTS_PATH_PREFIXES
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
}

pub(crate) async fn proxy_comments_request(request: Request<Body>) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), 4 * 1024 * 1024).await {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("comments request body is invalid: {error}"),
            )
                .into_response()
        }
    };
    let target = format!("{}{uri}", comments_runtime_base());
    let client = reqwest::Client::new();
    let mut upstream = client.request(
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET),
        &target,
    );
    for (name, value) in headers.iter() {
        if name == header::HOST || name == header::CONTENT_LENGTH {
            continue;
        }
        if let Ok(value) = value.to_str() {
            upstream = upstream.header(name.as_str(), value);
        }
    }
    if !body.is_empty() {
        upstream = upstream.body(Bytes::from(body));
    }
    match upstream.send().await {
        Ok(response) => {
            let status = response.status();
            let headers = response.headers().clone();
            match response.bytes().await {
                Ok(bytes) => {
                    let mut out = Response::new(Body::from(bytes));
                    *out.status_mut() = status;
                    for (name, value) in headers.iter() {
                        if name == header::TRANSFER_ENCODING || name == header::CONTENT_LENGTH {
                            continue;
                        }
                        if let (Ok(name), Ok(value)) = (
                            HeaderName::from_bytes(name.as_str().as_bytes()),
                            HeaderValue::from_bytes(value.as_bytes()),
                        ) {
                            out.headers_mut().append(name, value);
                        }
                    }
                    out
                }
                Err(error) => (
                    StatusCode::BAD_GATEWAY,
                    format!("comments runtime response failed: {error}"),
                )
                    .into_response(),
            }
        }
        Err(error) => (
            StatusCode::BAD_GATEWAY,
            format!("comments runtime is unavailable: {error}"),
        )
            .into_response(),
    }
}
