#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{HttpMethod, ProgramBody as RequestBody, WorkflowNode};
use interl_engine::{CancellationToken, EventKind, PreparedRun, RunOptions, RunReport, RunStatus};
use serde_json::{json, Value};
async fn run(document: &interl_core::ScenarioDocument) -> RunReport {
    let mut handle = PreparedRun::new(document, RunOptions::default())
        .unwrap()
        .start(CancellationToken::new());
    let mut live = Vec::new();
    while let Some(event) = handle.events.recv().await {
        live.push(event);
    }
    let report = handle.task.await.unwrap();
    assert_eq!(
        serde_json::to_value(&live).unwrap(),
        serde_json::to_value(&report.events).unwrap()
    );
    report
}
#[tokio::test]
async fn responses_are_not_transport_failures_and_trace_is_ordered() {
    let server = support::TestServer::start().await;
    for (path, status) in [("/health", 200), ("/not-found", 404)] {
        let report = run(&support::scenario(&server.base_url, path)).await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(report.summary.requests, 1);
        assert_eq!(report.summary.responses, 1);
        assert_eq!(
            report.summary.http_error_responses,
            u32::from(status == 404)
        );
        assert!(report
            .events
            .iter()
            .any(|e| matches!(e.event, EventKind::ResponseReceived { status:s,.. } if s==status)));
        assert!(matches!(
            report.events.last().unwrap().event,
            EventKind::RunFinished {
                status: RunStatus::Passed
            }
        ));
        for (index, event) in report.events.iter().enumerate() {
            assert_eq!(event.sequence, index as u64 + 1);
            assert_eq!(event.run_id, report.run_id);
        }
        assert!(report
            .events
            .windows(2)
            .all(|p| p[0].elapsed_ns <= p[1].elapsed_ns));
        assert!(report.snapshot.get("layout").is_none());
    }
}
#[tokio::test]
async fn timeout_and_active_cancellation_stop_following_steps() {
    let server = support::TestServer::start().await;
    let mut document = support::scenario(&server.base_url, "/slow");
    if let WorkflowNode::Http { config, .. } = &mut document.workflows[0].nodes[1] {
        config.timeout_ms = Some(20);
    }
    let report = run(&document).await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.error.unwrap().kind, "timeout");
    // Consume the timeout request's readiness notification before the cancellation case.
    server.entered.notified().await;
    if let WorkflowNode::Http { config, .. } = &mut document.workflows[0].nodes[1] {
        config.timeout_ms = Some(1000);
    }
    let cancel = CancellationToken::new();
    let mut handle = PreparedRun::new(&document, RunOptions::default())
        .unwrap()
        .start(cancel.clone());
    let drain = tokio::spawn(async move {
        while handle.events.recv().await.is_some() {}
        handle.task.await.unwrap()
    });
    server.entered.notified().await;
    cancel.cancel();
    let report = drain.await.unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert!(report
        .events
        .iter()
        .any(|e| matches!(e.event, EventKind::RunCancelled {})));
    assert!(!report
        .events
        .iter()
        .any(|e| e.node_id.as_deref() == Some("end")));
}
#[tokio::test]
async fn redaction_covers_headers_json_queries_snapshot_and_response_echoes() {
    let server = support::TestServer::start().await;
    let mut doc = support::scenario(&server.base_url, "/echo?token=${apiToken}");
    doc.environment
        .variables
        .insert("apiToken".into(), json!("environment-secret"));
    doc.client_profiles[0]
        .headers
        .insert("Authorization".into(), "Bearer header-secret".into());
    if let WorkflowNode::Http { config, .. } = &mut doc.workflows[0].nodes[1] {
        config.method = HttpMethod::Post;
        config.body = Some(RequestBody::Json {
            value: json!({"password":"body-secret","echo":"body-secret","public":"visible"}),
        });
    }
    let report = run(&doc).await;
    let wire = serde_json::to_string(&report).unwrap();
    for secret in ["environment-secret", "header-secret", "body-secret"] {
        assert!(!wire.contains(secret), "leaked {secret}");
        assert!(
            !wire.contains(&interl_http::encode_component(secret)),
            "leaked encoded {secret}"
        );
    }
    assert!(wire.contains("visible"));
    assert!(wire.contains("REDACTED"));
    let report = run(&support::scenario(&server.base_url, "/secrets")).await;
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("response-secret"));
}
#[tokio::test]
async fn cancelled_full_channel_finishes_and_retains_critical_events() {
    let doc = support::scenario("http://127.0.0.1:1", "/health");
    let cancel = CancellationToken::new();
    let options = RunOptions {
        event_capacity: 1,
        ..Default::default()
    };
    let handle = PreparedRun::new(&doc, options)
        .unwrap()
        .start(cancel.clone());
    // Wait for the bounded channel to fill before cancelling, without draining it.
    tokio::task::yield_now().await;
    cancel.cancel();
    let report = tokio::time::timeout(std::time::Duration::from_secs(2), handle.task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert!(report.delivery_dropped_events > 0);
    assert!(matches!(
        report.events.last().unwrap().event,
        EventKind::RunFinished { .. }
    ));
}
#[tokio::test]
async fn report_limit_stops_execution_and_keeps_terminal_failure() {
    let server = support::TestServer::start().await;
    let mut doc = support::scenario(&server.base_url, "/echo");
    if let WorkflowNode::Http { config, .. } = &mut doc.workflows[0].nodes[1] {
        config.method = HttpMethod::Post;
        config.body = Some(RequestBody::Json {
            value: json!({"data":"x".repeat(40000)}),
        });
    }
    let options = RunOptions {
        max_report_bytes: 131072,
        max_preview_bytes: 65536,
        ..Default::default()
    };
    let mut handle = PreparedRun::new(&doc, options)
        .unwrap()
        .start(CancellationToken::new());
    while handle.events.recv().await.is_some() {}
    let report = handle.task.await.unwrap();
    assert!(serde_json::to_vec(&report).unwrap().len() <= 131072);
    assert!(report.events_truncated);
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.error.as_ref().unwrap().kind, "resourceLimit");
}
#[test]
fn rejects_multiple_users_and_missing_variables_before_execution() {
    let mut doc = support::scenario("http://127.0.0.1:1", "/health");
    doc.groups.push(doc.groups[0].clone());
    assert!(PreparedRun::new(&doc, RunOptions::default()).is_err());
    doc.groups.pop();
    if let WorkflowNode::Http { config, .. } = &mut doc.workflows[0].nodes[1] {
        config.path = "/${missing}".into();
    }
    assert!(PreparedRun::new(&doc, RunOptions::default()).is_err());
    assert_eq!(interl_engine::exit_code(RunStatus::Passed), 0);
    assert_eq!(interl_engine::exit_code(RunStatus::Failed), 1);
    assert_eq!(interl_engine::exit_code(RunStatus::Error), 3);
    assert_eq!(interl_engine::exit_code(RunStatus::Cancelled), 130);
}

#[test]
fn supported_auth_shapes_prepare_without_unnecessary_secret_reads() {
    use interl_core::{parse_scenario, validate_scenario, ScenarioFormat};
    let auth = parse_scenario(
        include_str!("../../../scenarios/examples/auth-login.interl.json"),
        ScenarioFormat::Json,
    )
    .unwrap();
    let base = support::scenario("http://127.0.0.1:1", "/health");
    let mut declarations = base.clone();
    declarations.identities = auth.identities.clone();
    let mut template = base.clone();
    if let WorkflowNode::Http { config, .. } = &mut template.workflows[0].nodes[1] {
        config.body = Some(RequestBody::JsonTemplate {
            fields: Default::default(),
        });
    }
    let mut session = base.clone();
    session.workflows[0].nodes.push(WorkflowNode::SetSession {
        id: "update".into(),
        config: interl_core::SetSessionConfig {
            set_headers: Default::default(),
            remove_headers: vec!["authorization".into()],
        },
    });
    session.workflows[0].edges = vec![
        interl_core::WorkflowEdge {
            id: "start-update".into(),
            condition: None,
            is_default: None,
            from: "start".into(),
            to: "update".into(),
        },
        interl_core::WorkflowEdge {
            id: "update-health".into(),
            condition: None,
            is_default: None,
            from: "update".into(),
            to: "health".into(),
        },
        interl_core::WorkflowEdge {
            id: "health-end".into(),
            condition: None,
            is_default: None,
            from: "health".into(),
            to: "end".into(),
        },
    ];
    let mut extract = base;
    extract.workflows[0].nodes.push(WorkflowNode::Extract {
        id: "extract".into(),
        config: interl_core::ExtractConfig {
            source_node_id: "health".into(),
            expression: "response.body.token".into(),
            output: "token".into(),
        },
    });
    extract.workflows[0].edges = vec![
        interl_core::WorkflowEdge {
            id: "start-health".into(),
            condition: None,
            is_default: None,
            from: "start".into(),
            to: "health".into(),
        },
        interl_core::WorkflowEdge {
            id: "health-extract".into(),
            condition: None,
            is_default: None,
            from: "health".into(),
            to: "extract".into(),
        },
        interl_core::WorkflowEdge {
            id: "extract-end".into(),
            condition: None,
            is_default: None,
            from: "extract".into(),
            to: "end".into(),
        },
    ];
    for doc in [declarations, template, session, extract] {
        validate_scenario(&doc).unwrap();
        assert!(PreparedRun::new(&doc, RunOptions::default()).is_ok());
    }
}

#[test]
fn provider_is_not_called_for_anonymous_or_statically_rejected_runs() {
    struct NeverRead;
    impl interl_engine::SecretProvider for NeverRead {
        fn lookup(&self, _: &str) -> Result<std::ffi::OsString, interl_engine::SecretLookupError> {
            panic!("credential provider must not be called");
        }
    }
    let doc = support::scenario("http://127.0.0.1:1", "/health");
    assert!(PreparedRun::new_with_secret_provider(&doc, RunOptions::default(), &NeverRead).is_ok());
    let mut auth = interl_core::parse_scenario(
        include_str!("../../../scenarios/examples/auth-login.interl.json"),
        interl_core::ScenarioFormat::Json,
    )
    .unwrap();
    auth.groups.push(auth.groups[0].clone());
    let errors = PreparedRun::new_with_secret_provider(&auth, RunOptions::default(), &NeverRead)
        .err()
        .unwrap();
    assert_eq!(errors[0].code, "runConfiguration");
}
#[tokio::test]
async fn layout_changes_preserve_execution_events() {
    let server = support::TestServer::start().await;
    let mut doc = support::scenario(&server.base_url, "/health");
    let first = run(&doc).await;
    doc.workflows[0]
        .layout
        .as_mut()
        .unwrap()
        .positions
        .get_mut("health")
        .unwrap()
        .x = 999.;
    let second = run(&doc).await;
    assert_eq!(
        first.graph["workflows"][0]["nodes"][1]["position"]["x"],
        340.
    );
    assert_eq!(
        second.graph["workflows"][0]["nodes"][1]["position"]["x"],
        999.
    );
    let normalize = |r: RunReport| {
        r.events
            .into_iter()
            .map(|e| {
                let mut v = serde_json::to_value(e).unwrap();
                for key in ["runId", "elapsedNs"] {
                    v.as_object_mut().unwrap().remove(key);
                }
                // Captured layout is observational; compare execution semantics separately.
                if let Some(workflows) = v
                    .pointer_mut("/payload/graph/workflows")
                    .and_then(Value::as_array_mut)
                {
                    for workflow in workflows {
                        for node in workflow["nodes"].as_array_mut().unwrap() {
                            node.as_object_mut().unwrap().remove("position");
                        }
                    }
                }
                if let Some(headers) = v
                    .pointer_mut("/payload/headers")
                    .and_then(Value::as_object_mut)
                {
                    headers.remove("date");
                }
                v
            })
            .collect::<Vec<Value>>()
    };
    assert_eq!(normalize(first), normalize(second));
}

#[tokio::test]
async fn yaml_json_multi_request_examples_execute_in_graph_order() {
    let server = support::TestServer::start().await;
    for (source, format) in [
        (
            include_str!("../../../scenarios/examples/http-workflow.interl.json"),
            interl_core::ScenarioFormat::Json,
        ),
        (
            include_str!("../../../scenarios/examples/http-workflow.interl.yaml"),
            interl_core::ScenarioFormat::Yaml,
        ),
    ] {
        let mut doc = interl_core::parse_scenario(
            &source.replace("http://127.0.0.1:8080", &server.base_url),
            format,
        )
        .unwrap();
        doc.workflows[0].nodes.reverse(); // Connectivity, not array or layout order, schedules steps.
        let report = run(&doc).await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(report.summary.responses, 2);
        let nodes: Vec<_> = report
            .events
            .iter()
            .filter(|e| matches!(e.event, EventKind::RequestSent { .. }))
            .map(|e| e.node_id.as_deref().unwrap())
            .collect();
        assert_eq!(nodes, ["health", "echo"]);
    }
    let report = run(&support::scenario(
        &server.base_url,
        "/echo?token=literal-secret%3F",
    ))
    .await;
    let wire = serde_json::to_string(&report).unwrap();
    assert!(!wire.contains("literal-secret"));
}

#[tokio::test]
async fn closing_event_receiver_cancels_an_inflight_request() {
    let server = support::TestServer::start().await;
    let doc = support::scenario(&server.base_url, "/slow");
    let handle = PreparedRun::new(&doc, RunOptions::default())
        .unwrap()
        .start(CancellationToken::new());
    server.entered.notified().await;
    drop(handle.events);
    let report = handle.task.await.unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.summary.responses, 0);
    assert!(report.delivery_dropped_events > 0);
}
