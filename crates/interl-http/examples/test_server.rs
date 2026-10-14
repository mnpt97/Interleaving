//! Local-only manual M2 verification server. Never used by production execution.
use axum::{
    body::Bytes,
    http::{HeaderMap, Method},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/echo", get(echo).post(echo))
        .route(
            "/not-found",
            get(|| async {
                (
                    axum::http::StatusCode::NOT_FOUND,
                    Json(json!({"error":"not found"})),
                )
            }),
        )
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                Json(json!({"status":"late"}))
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("port 8080 must be available");
    println!("M2 verification server: http://127.0.0.1:8080 (Ctrl-C to stop)");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serve local requests");
}
async fn echo(method: Method, headers: HeaderMap, body: Bytes) -> Json<Value> {
    let headers: serde_json::Map<String, Value> = headers
        .iter()
        .map(|(key, value)| (key.to_string(), json!(value.to_str().unwrap_or_default())))
        .collect();
    Json(
        json!({"method":method.as_str(),"headers":headers,"body":serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null)}),
    )
}
