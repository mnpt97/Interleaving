#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{parse_document, ScenarioDocument, ScenarioFormat};
use interl_engine::*;
use serde_json::{json, Value};
use std::{ffi::OsString, sync::Mutex, time::Duration};
const SOURCE: &str = include_str!("../../../scenarios/examples/reusable-auth.interl.json");
#[derive(Default)]
struct Provider {
    calls: Mutex<Vec<String>>,
}
impl SecretProvider for Provider {
    fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
        self.calls.lock().unwrap().push(name.into());
        match name {
            "INTERL_ALICE_USERNAME" => Ok("alice".into()),
            "INTERL_ALICE_PASSWORD" => Ok("alice-password".into()),
            _ => Err(SecretLookupError::Missing),
        }
    }
}
fn document(base: &str) -> Value {
    serde_json::from_str(&SOURCE.replace("http://127.0.0.1:8080", base)).unwrap()
}
fn parse(v: &Value) -> ScenarioDocument {
    parse_document(&v.to_string(), ScenarioFormat::Json).unwrap()
}
async fn collect(mut handle: RunHandle) -> RunReport {
    let mut events = Vec::new();
    while let Some(e) = handle.events.recv().await {
        events.push(e);
    }
    let report = handle.task.await.unwrap();
    assert_eq!(
        serde_json::to_value(&events).unwrap(),
        serde_json::to_value(&report.events).unwrap()
    );
    report
}
fn safe(r: &RunReport) {
    let s = serde_json::to_string(r).unwrap();
    for secret in ["alice-password", "fake-token-", "fake-cookie-"] {
        assert!(!s.contains(secret), "secret leaked");
    }
    assert_eq!(r.report_version, 3);
    assert!(matches!(
        r.events.last().unwrap().event,
        EventKind::RunFinished { .. }
    ));
    for (i, e) in r.events.iter().enumerate() {
        assert_eq!(e.sequence, i as u64 + 1);
    }
}
fn response_statuses(report: &RunReport) -> Vec<u16> {
    report
        .events
        .iter()
        .filter_map(|e| match e.event {
            EventKind::ResponseReceived { status, .. } => Some(status),
            _ => None,
        })
        .collect()
}
fn cleanup(v: &mut Value, path: &str) {
    v["workflows"].as_array_mut().unwrap().push(json!({"id":"cleanup","name":"Cleanup","inputs":{},"outputs":{},"nodes":[{"id":"start","type":"start"},{"id":"request","type":"http","config":{"method":"GET","path":path}},{"id":"end","type":"end"}],"edges":[{"id":"a","from":"start","to":"request"},{"id":"b","from":"request","to":"end"}]}));
    v["groups"][0]["lifecycle"]["cleanup"] = json!({"workflowId":"cleanup","inputs":{}});
}
#[tokio::test]
async fn initialize_call_and_iteration_share_auth_but_not_local_ids() {
    let server = support::TestServer::start().await;
    let provider = Provider::default();
    let report = collect(
        PreparedRun::new_with_secret_provider(
            &parse(&document(&server.base_url)),
            RunOptions::default(),
            &provider,
        )
        .unwrap()
        .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(report.summary.responses, 2);
    assert_eq!(response_statuses(&report), [200, 200]);
    assert_eq!(
        report.snapshot["workflows"][1]["inputs"]["token"]["type"],
        "string"
    );
    assert_eq!(
        report.snapshot["groups"][0]["lifecycle"]["iteration"]["inputs"]["token"]["source"],
        "lifecycle"
    );
    match &report.events[0].event {
        EventKind::RunStarted { graph, .. } => assert_eq!(graph, &report.graph),
        _ => panic!("run must start with a graph capture"),
    }
    safe(&report);
    assert_eq!(provider.calls.lock().unwrap().len(), 2);
    let starts = report
        .events
        .iter()
        .filter(|e| matches!(e.event, EventKind::WorkflowStarted { .. }))
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 3);
    assert_eq!(starts[0].context.phase.as_deref(), Some("initialize"));
    assert!(starts[2].context.parent_invocation_id.is_some());
    assert_ne!(
        starts[0].context.invocation_id,
        starts[2].context.invocation_id
    );
    assert!(report.events.iter().any(
        |e| matches!(e.event, EventKind::EdgeTraversed { .. }) && e.context.edge_key.is_some()
    ));
}
#[tokio::test]
async fn repeated_calls_have_distinct_frames_and_same_cookie_session() {
    let server = support::TestServer::start().await;
    let mut v = document(&server.base_url);
    v["workflows"][2]["nodes"][1]["config"]["path"] = json!("/auth/cookie-me");
    v["workflows"][1]["nodes"].as_array_mut().unwrap().insert(
        3,
        json!({"id":"second","type":"call","config":{"workflowId":"inspect-account","inputs":{}}}),
    );
    v["workflows"][1]["edges"] = json!([{"id":"a","from":"start","to":"apply-token"},{"id":"b","from":"apply-token","to":"inspect"},{"id":"c","from":"inspect","to":"second"},{"id":"d","from":"second","to":"end"}]);
    let report = collect(
        PreparedRun::new_with_secret_provider(
            &parse(&v),
            RunOptions::default(),
            &Provider::default(),
        )
        .unwrap()
        .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(report.summary.responses, 3);
    assert_eq!(response_statuses(&report), [200, 200, 200]);
    let ids = report
        .events
        .iter()
        .filter(|e| {
            matches!(e.event, EventKind::WorkflowStarted { .. })
                && e.context.workflow_key.as_deref() == Some("wf:2")
        })
        .map(|e| e.context.invocation_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    safe(&report);
}
#[tokio::test]
async fn initialize_failure_stops_iteration_and_cleanup_preserves_primary_error() {
    let server = support::TestServer::start().await;
    let mut v = document(&server.base_url);
    v["workflows"][0]["nodes"][1]["config"]["path"] = json!("/auth/login-missing-token");
    cleanup(&mut v, "/auth/cookie-me");
    let report = collect(
        PreparedRun::new_with_secret_provider(
            &parse(&v),
            RunOptions::default(),
            &Provider::default(),
        )
        .unwrap()
        .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.cleanup.status, "completed");
    assert_eq!(
        report.error.as_ref().unwrap().kind,
        "extractionMissingProperty"
    );
    assert_eq!(report.summary.responses, 2);
    assert_eq!(response_statuses(&report), [200, 200]);
    assert!(!report
        .events
        .iter()
        .any(|e| e.context.phase.as_deref() == Some("iteration")));
    safe(&report);
}
#[tokio::test]
async fn cleanup_deadline_bounds_entire_workflow_and_changes_success_to_error() {
    let server = support::TestServer::start().await;
    let mut v = document(&server.base_url);
    cleanup(&mut v, "/slow");
    v["groups"][0]["lifecycle"]["cleanupTimeoutMs"] = json!(10);
    let report = tokio::time::timeout(
        Duration::from_secs(5),
        collect(
            PreparedRun::new_with_secret_provider(
                &parse(&v),
                RunOptions::default(),
                &Provider::default(),
            )
            .unwrap()
            .start(CancellationToken::new()),
        ),
    )
    .await
    .unwrap();
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.cleanup.status, "error");
    assert_eq!(report.error.as_ref().unwrap().kind, "cleanupTimeout");
    safe(&report);
}
#[tokio::test]
async fn user_cancel_attempts_cleanup_owner_close_skips_it() {
    for owner in [false, true] {
        let server = support::TestServer::start().await;
        let mut v = document(&server.base_url);
        v["workflows"][0]["nodes"][1]["config"]["path"] = json!("/auth/login-slow");
        cleanup(&mut v, "/health");
        let control = RunControl::new();
        let mut handle = PreparedRun::new_with_secret_provider(
            &parse(&v),
            RunOptions::default(),
            &Provider::default(),
        )
        .unwrap()
        .start_control(control.clone());
        let mut events = Vec::new();
        while let Some(e) = handle.events.recv().await {
            let sent = matches!(e.event, EventKind::RequestSent { .. });
            events.push(e);
            if sent {
                break;
            }
        }
        if owner {
            control.stop(StopCause::OwnerClosed);
        } else {
            control.cancel();
        }
        while let Some(e) = handle.events.recv().await {
            events.push(e);
        }
        let report = handle.task.await.unwrap();
        assert_eq!(report.status, RunStatus::Cancelled);
        assert_eq!(
            report.cleanup.status,
            if owner { "skipped" } else { "completed" }
        );
        assert_eq!(report.summary.requests, if owner { 1 } else { 2 });
        safe(&report);
    }
}
#[test]
fn rejected_program_never_reads_environment() {
    let mut v = document("http://127.0.0.1:1");
    v["groups"][0]["lifecycle"]["iteration"]["inputs"] = json!({});
    let provider = Provider::default();
    assert!(
        PreparedRun::new_with_secret_provider(&parse(&v), RunOptions::default(), &provider)
            .is_err()
    );
    assert!(provider.calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn aggregate_scalar_budget_counts_paused_parent_and_child_inputs() {
    let server = support::TestServer::start().await;
    let mut v = document(&server.base_url);
    v["groups"][0]["lifecycle"] = json!({"iteration":{"workflowId":"main","inputs":{}}});
    let mut parent_inputs = serde_json::Map::new();
    let mut child_inputs = serde_json::Map::new();
    let mut mappings = serde_json::Map::new();
    let mut fields = serde_json::Map::new();
    for i in 0..7 {
        let name = format!("v{i}");
        parent_inputs.insert(
            name.clone(),
            json!({"type":"string","default":"x".repeat(8192)}),
        );
        child_inputs.insert(name.clone(), json!({"type":"string"}));
        mappings.insert(name.clone(), json!({"source":"input","name":name}));
        fields.insert(name.clone(), json!({"source":"input","name":name}));
    }
    v["workflows"] = json!([
        {"id":"main","name":"Main","inputs":parent_inputs,"outputs":{},"nodes":[{"id":"start","type":"start"},{"id":"child","type":"call","config":{"workflowId":"child","inputs":mappings}},{"id":"end","type":"end"}],"edges":[{"id":"a","from":"start","to":"child"},{"id":"b","from":"child","to":"end"}]},
        {"id":"child","name":"Child","inputs":child_inputs,"outputs":{},"nodes":[{"id":"start","type":"start"},{"id":"http","type":"http","config":{"method":"POST","path":"/echo","body":{"kind":"jsonTemplate","fields":fields}}},{"id":"end","type":"end"}],"edges":[{"id":"a","from":"start","to":"http"},{"id":"b","from":"http","to":"end"}]}
    ]);
    cleanup(&mut v, "/health");
    let provider = Provider::default();
    let report = collect(
        PreparedRun::new_with_secret_provider(&parse(&v), Default::default(), &provider)
            .unwrap()
            .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.error.as_ref().unwrap().kind, "valueLimit");
    assert_eq!(report.summary.requests, 0);
    assert_eq!(report.cleanup.status, "skipped");
    assert!(provider.calls.lock().unwrap().is_empty());
    safe(&report);
}
#[tokio::test]
async fn retained_response_budget_is_shared_across_all_nested_frames() {
    let server = support::TestServer::start().await;
    let mut v = document(&server.base_url);
    v["groups"][0]["lifecycle"] = json!({"iteration":{"workflowId":"w0","inputs":{}}});
    v["workflows"] = json!([]);
    for i in 0..8 {
        let mut nodes = vec![
            json!({"id":"start","type":"start"}),
            json!({"id":"login","type":"http","config":{"method":"POST","path":"/auth/login-large","body":{"kind":"jsonTemplate","fields":{"username":{"source":"identity","field":"username"},"password":{"source":"identity","field":"password"}}}}}),
        ];
        if i < 7 {
            nodes.push(json!({"id":"child","type":"call","config":{"workflowId":format!("w{}",i+1),"inputs":{}}}));
        }
        nodes.push(json!({"id":"extract","type":"extract","config":{"sourceNodeId":"login","expression":"response.body.accessToken","output":"token"}}));
        nodes.push(json!({"id":"end","type":"end"}));
        let edges = nodes
            .windows(2)
            .enumerate()
            .map(|(j, pair)| json!({"id":format!("e{j}"),"from":pair[0]["id"],"to":pair[1]["id"]}))
            .collect::<Vec<_>>();
        v["workflows"].as_array_mut().unwrap().push(json!({"id":format!("w{i}"),"name":"Nested","inputs":{},"outputs":{},"nodes":nodes,"edges":edges}));
    }
    let report = collect(
        PreparedRun::new_with_secret_provider(
            &parse(&v),
            RunOptions {
                max_response_bytes: 200000,
                ..Default::default()
            },
            &Provider::default(),
        )
        .unwrap()
        .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.error.as_ref().unwrap().kind, "valueLimit");
    assert_eq!(report.summary.requests, 8);
    safe(&report);
}
#[tokio::test]
async fn closing_receiver_interrupts_cleanup_and_keeps_terminal_report() {
    let server = support::TestServer::start().await;
    let mut v = document(&server.base_url);
    cleanup(&mut v, "/slow");
    let mut handle =
        PreparedRun::new_with_secret_provider(&parse(&v), Default::default(), &Provider::default())
            .unwrap()
            .start(CancellationToken::new());
    while let Some(e) = handle.events.recv().await {
        if e.context.phase.as_deref() == Some("cleanup")
            && matches!(e.event, EventKind::RequestSent { .. })
        {
            break;
        }
    }
    drop(handle.events);
    let report = tokio::time::timeout(Duration::from_secs(1), handle.task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.cleanup.status, "cancelled");
    assert!(report.delivery_dropped_events > 0);
    safe(&report);
}
#[tokio::test]
async fn graph_keys_do_not_collapse_when_display_labels_are_redacted() {
    let server = support::TestServer::start().await;
    let mut v = document(&server.base_url);
    v["workflows"][0]["name"] = json!("alice");
    v["workflows"][1]["name"] = json!("alice");
    let report = collect(
        PreparedRun::new_with_secret_provider(&parse(&v), Default::default(), &Provider::default())
            .unwrap()
            .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(
        report.graph["workflows"][0]["name"],
        report.graph["workflows"][1]["name"]
    );
    assert_ne!(
        report.graph["workflows"][0]["key"],
        report.graph["workflows"][1]["key"]
    );
    safe(&report);
}

#[tokio::test]
async fn scalar_ports_response_metadata_and_call_outputs_keep_json_types() {
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = axum::Router::new()
        .route(
            "/source",
            axum::routing::get(|| async { axum::http::StatusCode::OK }),
        )
        .route(
            "/typed",
            axum::routing::post(|axum::Json(body): axum::Json<Value>| async move {
                if body == json!({"count":3,"ready":true,"previous":200,"answer":200}) {
                    axum::http::StatusCode::OK
                } else {
                    axum::http::StatusCode::BAD_REQUEST
                }
            }),
        );
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let mut v = document(&base);
    v["groups"][0]["lifecycle"] = json!({"iteration":{"workflowId":"main","inputs":{}}});
    let root_fields = json!({"count":{"source":"input","name":"count"},"ready":{"source":"input","name":"ready"},"previous":{"source":"response","nodeId":"source","field":"status"},"answer":{"source":"callOutput","nodeId":"child","name":"code"}});
    let child_fields = json!({"count":{"source":"input","name":"count"},"ready":{"source":"input","name":"ready"},"previous":{"source":"input","name":"previous"},"answer":{"source":"literal","value":200}});
    v["workflows"] = json!([
        {"id":"main","name":"Main","inputs":{"count":{"type":"integer","default":3},"ready":{"type":"boolean","default":true}},"outputs":{},"nodes":[{"id":"start","type":"start"},{"id":"source","type":"http","config":{"method":"GET","path":"/source"}},{"id":"child","type":"call","config":{"workflowId":"child","inputs":{"count":{"source":"input","name":"count"},"ready":{"source":"input","name":"ready"},"previous":{"source":"response","nodeId":"source","field":"status"}}}},{"id":"check","type":"http","config":{"method":"POST","path":"/typed","body":{"kind":"jsonTemplate","fields":root_fields}}},{"id":"end","type":"end"}],"edges":[{"id":"a","from":"start","to":"source"},{"id":"b","from":"source","to":"child"},{"id":"c","from":"child","to":"check"},{"id":"d","from":"check","to":"end"}]},
        {"id":"child","name":"Child","inputs":{"count":{"type":"integer"},"ready":{"type":"boolean"},"previous":{"type":"integer"}},"outputs":{"code":{"type":"integer","value":{"source":"response","nodeId":"request","field":"status"}}},"nodes":[{"id":"start","type":"start"},{"id":"request","type":"http","config":{"method":"POST","path":"/typed","body":{"kind":"jsonTemplate","fields":child_fields}}},{"id":"end","type":"end"}],"edges":[{"id":"a","from":"start","to":"request"},{"id":"b","from":"request","to":"end"}]}
    ]);
    let provider = Provider::default();
    let report = collect(
        PreparedRun::new_with_secret_provider(&parse(&v), Default::default(), &provider)
            .unwrap()
            .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(response_statuses(&report), [200, 200, 200]);
    assert!(provider.calls.lock().unwrap().is_empty());
    safe(&report);
}
