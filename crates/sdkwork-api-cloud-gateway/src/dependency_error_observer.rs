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
    "type", "title", "status", "detail", "instance", "code", "traceId", "operationId", "i18nKey",
];

pub async fn observe_dependency_server_errors(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;

    if !response.status().is_server_error() {
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
            return Response::from_parts(parts, Body::empty());
        }
    };

    let body_text = String::from_utf8_lossy(&bytes).into_owned();
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(serde_json::Value::Object(envelope)) => {
            let fields = ENVELOPE_FIELDS
                .iter()
                .filter_map(|field| {
                    envelope
                        .get(*field)
                        .map(|value| match value.as_str() {
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
