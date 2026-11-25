//! Server-error observability for embedded dependency surfaces.
//!
//! The gateway access log records the response status but not the failure
//! reason. When an embedded dependency (e.g. the course app API) returns a
//! 5xx, the SDKWork problem-details envelope carries the machine-readable
//! cause (`code`, `detail`, `operationId`, `i18nKey`), but nothing logged it —
//! forcing log archaeology through container stdout. This middleware buffers
//! 5xx response bodies (capped) and emits a structured `ERROR` event with the
//! parsed envelope so on-call triage starts from the log line, not from
//! guesswork. Non-5xx responses pass through untouched and unbuffered.

use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

const MAX_BUFFERED_ERROR_BODY_BYTES: usize = 16 * 1024;
const LOG_TARGET: &str = "sdkwork.api.cloud_gateway.dependency_error";

/// Problem-details fields (RFC 9457 style) the gateway emits on failures.
const ENVELOPE_FIELDS: &[&str] = &[
    "type",
    "title",
    "status",
    "detail",
    "instance",
    "code",
    "traceId",
    "operationId",
    "i18nKey",
];

pub async fn observe_dependency_server_errors(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;

    if !response.status().is_server_error() {
        return response;
    }

    // Observation must not cost the caller its error detail. Decide before
    // polling: a body that cannot be proven to fit the observation budget cannot
    // be replayed once it has been read, so it is forwarded untouched instead of
    // being replaced with nothing.
    if !sdkwork_web_core::body_contract::may_buffer_whole(
        response.body(),
        MAX_BUFFERED_ERROR_BODY_BYTES,
    ) {
        tracing::error!(
            target: LOG_TARGET,
            method = %method,
            path = %path,
            status = response.status().as_u16(),
            "embedded dependency returned a server error; body exceeds the observation budget and is forwarded unread"
        );
        return response;
    }

    let status = response.status();
    let (parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_BUFFERED_ERROR_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::error!(
                target: LOG_TARGET,
                method = %method,
                path = %path,
                status = status.as_u16(),
                body_buffer_error = %error,
                "embedded dependency returned a server error; response body could not be buffered"
            );
            // Never hand the client an empty body where a real one was: state the
            // transport failure explicitly instead.
            return Response::from_parts(
                parts,
                Body::from(
                    r#"{"type":"about:blank","title":"Bad gateway","status":502,"detail":"the dependency response body could not be read"}"#,
                ),
            );
        }
    };

    let body_text = String::from_utf8_lossy(&bytes).into_owned();
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Object(envelope)) => {
            let fields = ENVELOPE_FIELDS
                .iter()
                .filter_map(|field| {
                    envelope.get(*field).map(|value| match value.as_str() {
                        Some(text) => format!("{field}={text}"),
                        None => format!("{field}={value}"),
                    })
                })
                .collect::<Vec<_>>()
                .join(" ");
            tracing::error!(
                target: LOG_TARGET,
                method = %method,
                path = %path,
                status = status.as_u16(),
                error_envelope = %fields,
                "embedded dependency returned a server error"
            );
        }
        _ => {
            tracing::error!(
                target: LOG_TARGET,
                method = %method,
                path = %path,
                status = status.as_u16(),
                body = %body_text,
                "embedded dependency returned a non-JSON server error"
            );
        }
    }

    Response::from_parts(parts, Body::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    async fn server_error_with_body(len: usize) -> Response {
        let mut body = b"{\"detail\":\"".to_vec();
        body.extend(std::iter::repeat(b'x').take(len));
        body.extend_from_slice(b"\"}");
        Response::builder()
            .status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
            .header(axum::http::header::CONTENT_TYPE, "application/problem+json")
            .body(Body::from(body))
            .expect("response")
    }

    async fn call_with_body(len: usize) -> (axum::http::StatusCode, Vec<u8>) {
        let app = Router::new()
            .route(
                "/boom",
                get(move || async move { server_error_with_body(len).await }),
            )
            .layer(axum::middleware::from_fn(observe_dependency_server_errors));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/boom")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("call");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        (status, bytes.to_vec())
    }

    /// The observer must never cost the caller its error detail: a 5xx body that
    /// cannot be buffered inside the observation budget is forwarded untouched.
    #[tokio::test]
    async fn oversized_server_error_body_reaches_the_client_unchanged() {
        let len = MAX_BUFFERED_ERROR_BODY_BYTES * 4;
        let (status, bytes) = call_with_body(len).await;

        assert_eq!(axum::http::StatusCode::INTERNAL_SERVER_ERROR, status);
        assert_eq!(
            b"{\"detail\":\"".len() + len + b"\"}".len(),
            bytes.len(),
            "an oversized 5xx body must not be truncated or emptied"
        );
        assert!(bytes.starts_with(b"{\"detail\":\""));
        assert!(bytes.ends_with(b"\"}"));
    }

    /// A body inside the budget is observed and still replayed byte for byte.
    #[tokio::test]
    async fn observed_server_error_body_is_replayed_unchanged() {
        let len = 32;
        let (status, bytes) = call_with_body(len).await;

        assert_eq!(axum::http::StatusCode::INTERNAL_SERVER_ERROR, status);
        let expected = format!("{{\"detail\":\"{}\"}}", "x".repeat(len));
        assert_eq!(expected.into_bytes(), bytes);
    }
}
