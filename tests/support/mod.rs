#![allow(dead_code)] // Shared local-server fixture; each crate uses a subset.
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::IntoResponse,
    routing::get,
};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{net::TcpListener, sync::Notify, task::JoinHandle};
#[derive(Clone, Default)]
struct ServerState {
    entered: std::sync::Arc<Notify>,
    hits: std::sync::Arc<AtomicUsize>,
}
pub struct TestServer {
    pub base_url: String,
    pub entered: std::sync::Arc<Notify>,
    pub hits: std::sync::Arc<AtomicUsize>,
    task: JoinHandle<()>,
}
impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl TestServer {
    pub async fn start() -> Self {
        let state = ServerState::default();
        let app = axum::Router::new()
            .route("/health", get(|| async { axum::Json(serde_json::json!({"status":"ok"})) }))
            .route("/echo", get(echo).post(echo))
            .route(
                "/not-found",
                get(|| async { (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({"error":"not found"}))) }),
            )
            .route(
                "/slow",
                get(|State(state): State<ServerState>| async move {
                    state.entered.notify_one();
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    axum::Json(serde_json::json!({"status":"late"}))
                }),
            )
            .route(
                "/large",
                get(|| async { axum::Json(serde_json::json!({"data":"x".repeat(131072)})) }),
            )
            .route(
                "/redirect",
                get(|| async { (StatusCode::TEMPORARY_REDIRECT, [("location", "/echo")]) }),
            )
            .route(
                "/cookie",
                get(|| async {
                    (
                        [("set-cookie", "session=private-cookie; Path=/")],
                        axum::Json(serde_json::json!({"status":"set"})),
                    )
                }),
            )
            .route(
                "/secrets",
                get(|| async {
                    axum::Json(serde_json::json!({"password":"response-secret", "echo":"response-secret", "ok":true}))
                }),
            )
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local test server");
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            base_url,
            entered: state.entered,
            hits: state.hits,
            task,
        }
    }
}
async fn echo(
    State(state): State<ServerState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    state.hits.fetch_add(1, Ordering::SeqCst);
    let headers: serde_json::Map<String, Value> = headers
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                serde_json::json!(v.to_str().unwrap_or_default()),
            )
        })
        .collect();
    axum::Json(
        serde_json::json!({"method":method.as_str(), "query":uri.query(), "headers":headers,"body":serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null)}),
    )
}
pub fn scenario(base_url: &str, path: &str) -> interl_core::ScenarioDocument {
    let mut document = interl_core::parse_scenario(
        include_str!("../../scenarios/examples/hello-api.json"),
        interl_core::ScenarioFormat::Json,
    )
    .unwrap();
    document.client_profiles[0].base_url = base_url.into();
    if let interl_core::WorkflowNode::Http { config, .. } = &mut document.workflow.nodes[1] {
        config.path = path.into();
    }
    document
}
