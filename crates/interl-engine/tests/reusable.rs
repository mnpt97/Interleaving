#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{parse_document, ScenarioDocument, ScenarioFormat};
use interl_engine::*;
use serde_json::{json, Value};
use std::{ffi::OsString, sync::Mutex, time::Duration};
const SOURCE: &str = include_str!("../../../scenarios/examples/reusable-auth.interl.json");

/// Keep the living example executable in both file formats against the local fixture.
#[tokio::test]
async fn feature_showcase_runs_in_json_and_yaml_with_identical_safe_outcomes() {
    let server = support::TestServer::start().await;
    let json_source = include_str!("../../../scenarios/examples/feature-showcase.interl.json");
    let yaml_source = include_str!("../../../scenarios/examples/feature-showcase.interl.yaml");
    let json_document = parse_document(json_source, ScenarioFormat::Json).unwrap();
    let yaml_document = parse_document(yaml_source, ScenarioFormat::Yaml).unwrap();
    assert_eq!(json_document, yaml_document, "showcase formats diverged");

    for (source, format, minimum) in [
        (json_source, ScenarioFormat::Json, 1),
        (yaml_source, ScenarioFormat::Yaml, 1),
        (json_source, ScenarioFormat::Json, 4),
        (yaml_source, ScenarioFormat::Yaml, 4),
    ] {
        // Override both the profile default and the explicit per-node URL.
        let source = source.replace("http://127.0.0.1:8080", &server.base_url);
        let document = parse_document(&source, format).unwrap();
        let mut value = serde_json::to_value(document).unwrap();
        // Keep automated execution quick while exercising the real waiting endpoint.
        value["environment"]["variables"]["waitMs"] = json!(20);
        value["groups"][0]["lifecycle"]["iteration"]["inputs"]["minimum"]["value"] = json!(minimum);
        let document = parse(&value);
        let report = collect(
            PreparedRun::new_with_secret_provider(
                &document,
                RunOptions::default(),
                &Provider::default(),
            )
            .unwrap()
            .start(CancellationToken::new()),
        )
        .await;
        assert_eq!(report.status, RunStatus::Passed, "{:?}", report.error);
        assert_eq!(response_statuses(&report), vec![200; 12]);
        for id in [
            "wait-before-inspection",
            "wait-for-health",
            "wait-before-finishing",
        ] {
            let sent = report
                .events
                .iter()
                .find(|event| {
                    event.node_id.as_deref() == Some(id)
                        && matches!(event.event, EventKind::RequestSent { .. })
                })
                .unwrap();
            let received = report
                .events
                .iter()
                .find(|event| {
                    event.node_id.as_deref() == Some(id)
                        && matches!(event.event, EventKind::ResponseReceived { .. })
                })
                .unwrap();
            assert!(received.elapsed_ns - sent.elapsed_ns >= 20_000_000);
        }
        // Captured graph labels/attachment sides drive the independent Run canvas.
        for workflow in report.graph["workflows"].as_array().unwrap() {
            for node in workflow["nodes"].as_array().unwrap() {
                assert!(node["name"].is_string());
            }
            for edge in workflow["edges"].as_array().unwrap() {
                assert!(edge["sourceHandle"].is_string());
                assert!(edge["targetHandle"].is_string());
                if edge["id"] == "route-to-fallback" {
                    assert_eq!(edge["sourceHandle"], "bottom");
                }
                if edge["id"] == "route-to-accepted" {
                    assert_eq!(edge["sourceHandle"], "top");
                }
            }
        }
        let executed: Vec<_> = report
            .events
            .iter()
            .filter_map(|event| {
                matches!(event.event, EventKind::StepStarted { .. })
                    .then_some(event.node_id.as_deref())
                    .flatten()
            })
            .collect();
        assert_eq!(executed.contains(&"accepted"), minimum == 1);
        assert_eq!(executed.contains(&"fallback"), minimum == 4);
        assert_eq!(
            executed
                .iter()
                .filter(|node| **node == "extract-health")
                .count(),
            1
        );
        assert!(executed.contains(&"cleanup-health"));
        safe(&report);
    }
}
#[derive(Default)]
struct Provider {
    calls: Mutex<Vec<String>>,
}
impl SecretProvider for Provider {
    fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
        self.calls.lock().unwrap().push(name.into());
        match name {
            "INTERL_ALICE_USERNAME" | "INTERL_USERNAME" => Ok("alice".into()),
            "INTERL_ALICE_PASSWORD" | "INTERL_PASSWORD" => Ok("alice-password".into()),
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

#[tokio::test]
async fn editor_auth_composite_retains_bearer_and_cookie_sessions_without_leaking_values() {
    let server = support::TestServer::start().await;
    for cookie in [false, true] {
        let mut value: Value =
            serde_json::from_str(include_str!("fixtures/editor-auth.json")).unwrap();
        value["clientProfiles"][0]["baseUrl"] = json!(server.base_url);
        if cookie {
            let nodes = value["workflows"][1]["nodes"].as_array_mut().unwrap();
            nodes.retain(|node| !matches!(node["type"].as_str(), Some("extract" | "setSession")));
            value["workflows"][1]["layout"]["positions"]
                .as_object_mut()
                .unwrap()
                .remove("extract");
            value["workflows"][1]["layout"]["positions"]
                .as_object_mut()
                .unwrap()
                .remove("session");
            value["workflows"][1]["edges"] = json!([{"id":"edge-1","from":"start","to":"login"},{"id":"edge-5","from":"login","to":"end"}]);
        }
        let path = if cookie {
            "/auth/cookie-me"
        } else {
            "/auth/me"
        };
        value["workflows"][0]["nodes"]
            .as_array_mut()
            .unwrap()
            .insert(
                2,
                json!({"id":"protected","type":"http","config":{"method":"GET","path":path}}),
            );
        value["workflows"][0]["edges"][1]["to"] = json!("protected");
        value["workflows"][0]["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"edge-3","from":"protected","to":"end"}));
        let provider = Provider::default();
        let document = parse(&value);
        let saved = interl_core::serialize_document(&document).unwrap();
        let reopened = parse_document(&saved, ScenarioFormat::Json).unwrap();
        let report = collect(
            PreparedRun::new_with_secret_provider(&reopened, RunOptions::default(), &provider)
                .unwrap()
                .start(CancellationToken::new()),
        )
        .await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(response_statuses(&report), [200, 200]);
        assert_eq!(provider.calls.lock().unwrap().len(), 2);
        assert!(!serde_json::to_string(&report).unwrap().contains("alice"));
        safe(&report);
    }
}

fn native_document(base: &str) -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("fixtures/editor-auth-native.json")).unwrap();
    value["clientProfiles"][0]["baseUrl"] = json!(base);
    value
}
fn linear_nodes(value: &mut Value, nodes: Vec<Value>) {
    value["workflows"][0]["edges"] = json!(nodes
        .windows(2)
        .enumerate()
        .map(|(i, p)| json!({"id":format!("e{i}"),"from":p[0]["id"],"to":p[1]["id"]}))
        .collect::<Vec<_>>());
    value["workflows"][0]["nodes"] = json!(nodes);
    value["workflows"][0]
        .as_object_mut()
        .unwrap()
        .remove("layout");
}
fn request(id: &str, path: &str) -> Value {
    json!({"id":id,"type":"http","config":{"method":"GET","path":path}})
}
#[tokio::test]
async fn native_editor_auth_roundtrips_json_yaml_and_inherits_bearer_or_cookie() {
    let server = support::TestServer::start().await;
    for cookie in [false, true] {
        let mut value = native_document(&server.base_url);
        let mut login = value["workflows"][0]["nodes"][1].clone();
        if cookie {
            login["config"]["mode"] = json!("cookie");

            for field in ["tokenExpression", "headerName", "headerPrefix"] {
                login["config"].as_object_mut().unwrap().remove(field);
            }
        }
        linear_nodes(
            &mut value,
            vec![
                json!({"id":"start","type":"start"}),
                request("anonymous", "/auth/me"),
                login,
                request(
                    "protected",
                    if cookie {
                        "/auth/cookie-me"
                    } else {
                        "/auth/me"
                    },
                ),
                json!({"id":"end","type":"end"}),
            ],
        );
        let document = parse(&value);
        let saved = interl_core::serialize_document(&document).unwrap();
        let reopened = parse_document(&saved, ScenarioFormat::Json).unwrap();
        let yaml = include_str!("fixtures/editor-auth-native.yaml");
        let yaml_document = parse_document(yaml, ScenarioFormat::Yaml).unwrap();
        assert_eq!(
            yaml_document,
            parse(&native_document("http://localhost:8080"))
        );
        let report = collect(
            PreparedRun::new_with_secret_provider(
                &reopened,
                Default::default(),
                &Provider::default(),
            )
            .unwrap()
            .start(CancellationToken::new()),
        )
        .await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(response_statuses(&report), [401, 200, 200]);
        assert_eq!(
            report.graph["workflows"][0]["nodes"][2]["type"],
            "authentication"
        );
        safe(&report);
    }
}
#[tokio::test]
async fn native_failed_auth_stops_and_cleanup_cannot_reuse_prior_session() {
    let server = support::TestServer::start().await;
    for path in [
        "/auth/login-missing-token",
        "/auth/login-malformed",
        "/not-found",
        "/auth/login",
    ] {
        let mut value = native_document(&server.base_url);
        let first = value["workflows"][0]["nodes"][1].clone();
        let mut failed = first.clone();
        failed["id"] = json!("second");
        failed["config"]["path"] = json!(path);
        if path == "/auth/login" {
            failed["config"]["username"]["name"] = json!("INTERL_PASSWORD");
        }
        linear_nodes(
            &mut value,
            vec![
                json!({"id":"start","type":"start"}),
                first,
                failed,
                request("never", "/echo"),
                json!({"id":"end","type":"end"}),
            ],
        );
        cleanup(&mut value, "/auth/cookie-me");
        let report = collect(
            PreparedRun::new_with_secret_provider(
                &parse(&value),
                Default::default(),
                &Provider::default(),
            )
            .unwrap()
            .start(CancellationToken::new()),
        )
        .await;
        assert_eq!(report.status, RunStatus::Error);
        assert_eq!(report.summary.requests, 3);
        assert_eq!(response_statuses(&report).last(), Some(&401));
        assert!(!report
            .events
            .iter()
            .any(|e| e.node_id.as_deref() == Some("never")));
        safe(&report);
    }
}
#[tokio::test]
async fn native_auth_replaces_all_headers_and_cookies_with_node_owned_credentials() {
    use axum::{
        extract::State,
        http::HeaderMap,
        routing::{get, post},
        Json,
    };
    use std::sync::Arc;
    let observations = Arc::new(Mutex::new(Vec::<Value>::new()));
    let app = axum::Router::new()
        .route("/v1/login", post(|State(seen): State<Arc<Mutex<Vec<Value>>>>, headers: HeaderMap, Json(body): Json<Value>| async move {
            seen.lock().unwrap().push(json!({"login":body["username"], "authorization":headers.get("authorization").and_then(|h|h.to_str().ok()), "cookie":headers.get("cookie").and_then(|h|h.to_str().ok()), "stale":headers.get("x-stale").and_then(|h|h.to_str().ok())}));
            let user = body["username"].as_str().unwrap();
            ([("set-cookie", format!("{user}_only=private-{user}; Path=/"))], Json(json!({"accessToken":format!("token-{user}")})))
        }))
        .route("/v1/check", get(|State(seen): State<Arc<Mutex<Vec<Value>>>>, headers: HeaderMap| async move {
            seen.lock().unwrap().push(json!({"authorization":headers.get("authorization").and_then(|h|h.to_str().ok()), "cookie":headers.get("cookie").and_then(|h|h.to_str().ok()), "stale":headers.get("x-stale").and_then(|h|h.to_str().ok())}));
            Json(json!({"ok":true}))
        })).with_state(observations.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    struct Accounts;
    impl SecretProvider for Accounts {
        fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
            Ok(match name {
                "INTERL_USERNAME" => "alice",
                "INTERL_PASSWORD" => "alice-password",
                "BOB_USERNAME" => "bob",
                "BOB_PASSWORD" => "bob-password",
                _ => return Err(SecretLookupError::Missing),
            }
            .into())
        }
    }
    let mut value = native_document(&base);
    value["clientProfiles"][0]["headers"] =
        json!({"Authorization":"stale-static", "Cookie":"stale=static"});
    let mut first = value["workflows"][0]["nodes"][1].clone();
    first["config"]["path"] = json!("/login");
    let mut second = first.clone();
    second["id"] = json!("second");
    second["config"]["mode"] = json!("cookie");

    for field in ["tokenExpression", "headerName", "headerPrefix"] {
        second["config"].as_object_mut().unwrap().remove(field);
    }
    second["config"]["username"]["name"] = json!("BOB_USERNAME");
    second["config"]["password"]["name"] = json!("BOB_PASSWORD");
    let stale = json!({"id":"stale","type":"setSession","config":{"setHeaders":{"X-Stale":{"source":"literal","value":"old"}}}});
    linear_nodes(
        &mut value,
        vec![
            json!({"id":"start","type":"start"}),
            stale,
            first,
            request("alice-check", "/check"),
            second,
            request("bob-check", "/check"),
            json!({"id":"end","type":"end"}),
        ],
    );
    let report = collect(
        PreparedRun::new_with_secret_provider(&parse(&value), Default::default(), &Accounts)
            .unwrap()
            .start(CancellationToken::new()),
    )
    .await;
    task.abort();
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(response_statuses(&report), [200, 200, 200, 200]);
    let seen = observations.lock().unwrap();
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[0]["login"], "alice");
    assert!(seen[0]["authorization"].is_null());
    assert!(seen[0]["cookie"].is_null());
    assert!(seen[0]["stale"].is_null());
    assert_eq!(seen[1]["authorization"], "Bearer token-alice");
    assert!(seen[1]["cookie"].as_str().unwrap().contains("alice_only"));
    assert_eq!(seen[2]["login"], "bob");
    assert!(seen[2]["authorization"].is_null());
    assert!(seen[2]["cookie"].is_null());
    assert!(seen[3]["authorization"].is_null());
    assert_eq!(seen[3]["cookie"], "bob_only=private-bob");
    let serialized = serde_json::to_string(&report).unwrap();
    for secret in [
        "alice-password",
        "bob-password",
        "token-alice",
        "private-alice",
        "private-bob",
    ] {
        assert!(!serialized.contains(secret));
    }
    safe(&report);
}

#[tokio::test]
async fn node_target_overrides_preserve_prefix_and_transport_session() {
    let server = support::TestServer::start().await;
    let other = support::TestServer::start().await;
    let mut value = native_document(&server.base_url);
    let mut login = value["workflows"][0]["nodes"][1].clone();
    login["config"]["baseUrl"] = json!(format!("{}/auth", server.base_url));
    login["config"]["path"] = json!("/login");
    let mut inherited = request("inherited", "/auth/me");
    inherited["config"]["baseUrl"] = Value::Null;
    inherited["config"]
        .as_object_mut()
        .unwrap()
        .remove("baseUrl");
    let mut overridden = request("overridden", "/me");
    overridden["config"]["baseUrl"] = json!(format!("{}/auth/", other.base_url));
    linear_nodes(
        &mut value,
        vec![
            json!({"id":"start","type":"start"}),
            login,
            inherited,
            overridden,
            json!({"id":"end","type":"end"}),
        ],
    );
    let report = collect(
        PreparedRun::new_with_secret_provider(
            &parse(&value),
            Default::default(),
            &Provider::default(),
        )
        .unwrap()
        .start(CancellationToken::new()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(response_statuses(&report), [200, 200, 401]);
    safe(&report);
}

#[tokio::test]
async fn cookie_auth_accepts_api_scoped_cookies_and_rejects_empty_sessions() {
    use axum::{
        http::HeaderMap,
        routing::{get, post},
        Json,
    };
    let app = axum::Router::new()
        .route(
            "/login",
            post(|| async {
                (
                    [("set-cookie", "scoped=private-scoped-cookie; Path=/api")],
                    Json(json!({"ok":true})),
                )
            }),
        )
        .route("/no-cookie", post(|| async { Json(json!({"ok":true})) }))
        .route(
            "/api/check",
            get(|headers: HeaderMap| async move {
                if headers.get("cookie").and_then(|h| h.to_str().ok())
                    == Some("scoped=private-scoped-cookie")
                {
                    axum::http::StatusCode::OK
                } else {
                    axum::http::StatusCode::UNAUTHORIZED
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    for path in ["/login", "/no-cookie"] {
        let mut value = native_document(&base);
        let mut login = value["workflows"][0]["nodes"][1].clone();
        login["config"]["mode"] = json!("cookie");

        for field in ["tokenExpression", "headerName", "headerPrefix"] {
            login["config"].as_object_mut().unwrap().remove(field);
        }
        login["config"]["path"] = json!(path);
        linear_nodes(
            &mut value,
            vec![
                json!({"id":"start","type":"start"}),
                login,
                request("protected", "/api/check"),
                json!({"id":"end","type":"end"}),
            ],
        );
        let report = collect(
            PreparedRun::new_with_secret_provider(
                &parse(&value),
                Default::default(),
                &Provider::default(),
            )
            .unwrap()
            .start(CancellationToken::new()),
        )
        .await;
        if path == "/login" {
            assert_eq!(report.status, RunStatus::Passed);
            assert_eq!(response_statuses(&report), [200, 200]);
        } else {
            assert_eq!(report.status, RunStatus::Error);
            assert_eq!(report.summary.requests, 1);
            assert_eq!(report.error.as_ref().unwrap().kind, "authenticationFailed");
        }
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains("private-scoped-cookie"));
        safe(&report);
    }
    task.abort();
}
