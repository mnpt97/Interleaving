//! Shared loopback-only waiting endpoint for manual visualization and HTTP fixtures.
use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

/// Maximum simulated wait; the caller's HTTP deadline may stop waiting sooner.
pub const MAX_TIMEOUT_MS: u64 = 60_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WaitRequest {
    timeout_ms: u64,
}

/// Provide the same bounded POST endpoint in the manual server and automated fixture.
pub fn router() -> Router {
    Router::new()
        .route("/long-poll", post(wait))
        .layer(DefaultBodyLimit::max(1024))
}

/// Wait asynchronously so other requests can proceed; return no request headers or credentials.
async fn wait(headers: HeaderMap, body: Bytes) -> Response {
    let is_json = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
        });
    if !is_json {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"error":"Use Content-Type: application/json."})),
        )
            .into_response();
    }
    let request = match serde_json::from_slice::<WaitRequest>(&body) {
        Ok(request) if request.timeout_ms <= MAX_TIMEOUT_MS => request,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"timeoutMs must be an integer from 0 to 60000."})),
            )
                .into_response();
        }
    };
    tokio::time::sleep(Duration::from_millis(request.timeout_ms)).await;
    Json(json!({"status":"complete", "timeoutMs":request.timeout_ms})).into_response()
}
