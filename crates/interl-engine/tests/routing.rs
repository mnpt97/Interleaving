#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{parse_document, ScenarioFormat};
use interl_engine::*;
use serde_json::{json, Value};
async fn run(expression: &str, fallback: bool) -> RunReport {
    let server = support::TestServer::start().await;
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../scenarios/examples/exclusive-routing.interl.json"
    ))
    .unwrap();
    value["clientProfiles"][0]["baseUrl"] = json!(server.base_url);
    value["workflows"][0]["edges"][2]["condition"]["expression"] = json!(expression);
    if !fallback {
        value["workflows"][0]["edges"][3]
            .as_object_mut()
            .unwrap()
            .remove("default");
        value["workflows"][0]["edges"][3]["condition"] =
            json!({"expression":if expression=="true" {"true"} else {"false"}});
    }
    let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    let mut handle = PreparedRun::new(&doc, RunOptions::default())
        .unwrap()
        .start(CancellationToken::new());
    let mut events = Vec::new();
    while let Some(event) = handle.events.recv().await {
        events.push(event);
    }
    let report = handle.task.await.unwrap();
    assert_eq!(
        serde_json::to_value(events).unwrap(),
        serde_json::to_value(&report.events).unwrap()
    );
    report
}
#[tokio::test]
async fn selects_one_route_and_never_runs_the_other_arm() {
    for (condition, chosen, skipped) in [
        ("response.status = 200", "accepted", "fallback"),
        ("false", "fallback", "accepted"),
    ] {
        let report = run(condition, true).await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(report.summary.requests, 2);
        let nodes: Vec<_> = report
            .events
            .iter()
            .filter(|e| matches!(e.event, EventKind::StepStarted { .. }))
            .filter_map(|e| e.node_id.as_deref())
            .collect();
        assert!(nodes.contains(&chosen));
        assert!(!nodes.contains(&skipped));
        assert!(nodes.contains(&"joined"));
        let routes: Vec<_> = report
            .events
            .iter()
            .filter(|e| matches!(e.event, EventKind::RouteSelected { .. }))
            .collect();
        assert_eq!(routes.len(), 1);
        let EventKind::RouteSelected {
            selected_edge_key,
            unselected_edge_keys,
        } = &routes[0].event
        else {
            unreachable!()
        };
        assert_eq!(unselected_edge_keys.len(), 1);
        assert!(!unselected_edge_keys.contains(selected_edge_key));
        assert!(report
            .events
            .iter()
            .any(|e| matches!(e.event, EventKind::EdgeTraversed {})
                && e.context.edge_key.as_ref() == Some(selected_edge_key)));
    }
}
#[tokio::test]
async fn ambiguous_and_missing_routes_fail_without_running_an_arm() {
    for (condition, code) in [("true", "routeAmbiguous"), ("false", "routeNoMatch")] {
        let report = run(condition, false).await;
        assert_eq!(report.status, RunStatus::Error);
        assert_eq!(report.error.unwrap().kind, code);
        assert_eq!(report.summary.requests, 1);
        assert!(!report
            .events
            .iter()
            .any(|e| matches!(e.event, EventKind::RouteSelected { .. })));
    }
}

#[tokio::test]
async fn selected_arm_cancellation_stops_before_merge_and_preserves_bounded_cleanup() {
    let server = support::TestServer::start().await;
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../scenarios/examples/exclusive-routing.interl.json"
    ))
    .unwrap();
    value["clientProfiles"][0]["baseUrl"] = json!(server.base_url);
    value["workflows"][0]["nodes"][3]["config"]["path"] = json!("/slow");
    let cleanup = json!({"id":"cleanup","name":"Cleanup","nodes":[{"id":"start","type":"start"},{"id":"clean","type":"http","config":{"method":"GET","path":"/health"}},{"id":"end","type":"end"}],"edges":[{"id":"a","from":"start","to":"clean"},{"id":"b","from":"clean","to":"end"}]});
    value["workflows"].as_array_mut().unwrap().push(cleanup);
    value["groups"][0]["lifecycle"]["cleanup"] = json!({"workflowId":"cleanup","inputs":{}});
    let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    let control = RunControl::new();
    let mut handle = PreparedRun::new(&doc, RunOptions::default())
        .unwrap()
        .start_control(control.clone());
    while let Some(e) = handle.events.recv().await {
        if e.node_id.as_deref() == Some("accepted")
            && matches!(e.event, EventKind::RequestSent { .. })
        {
            control.cancel();
        }
    }
    let report = handle.task.await.unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.cleanup.status, "completed");
    assert!(!report
        .events
        .iter()
        .any(|e| e.node_id.as_deref() == Some("joined")
            && matches!(e.event, EventKind::StepStarted { .. })));
    assert!(!report
        .events
        .iter()
        .any(|e| e.node_id.as_deref() == Some("fallback")
            && matches!(e.event, EventKind::StepStarted { .. })));
}

#[tokio::test]
async fn selected_arm_failure_does_not_enter_the_other_arm_or_merge() {
    let server = support::TestServer::start().await;
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../scenarios/examples/exclusive-routing.interl.json"
    ))
    .unwrap();
    value["clientProfiles"][0]["baseUrl"] = json!(server.base_url);
    value["workflows"][0]["nodes"][3]["config"]["path"] = json!("/slow");
    value["workflows"][0]["nodes"][3]["config"]["timeoutMs"] = json!(10);
    let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    let mut handle = PreparedRun::new(&doc, RunOptions::default())
        .unwrap()
        .start(CancellationToken::new());
    while handle.events.recv().await.is_some() {}
    let report = handle.task.await.unwrap();
    assert_eq!(report.status, RunStatus::Error);
    assert!(!report.events.iter().any(|e| matches!(
        e.node_id.as_deref(),
        Some("fallback" | "joined")
    ) && matches!(e.event, EventKind::StepStarted { .. })));
}

#[tokio::test]
async fn editor_produced_fixture_executes_the_same_route_contract() {
    let server = support::TestServer::start().await;
    let source = include_str!("fixtures/editor-routing.json")
        .replace("http://localhost:8080", &server.base_url);
    let doc = parse_document(&source, ScenarioFormat::Json).unwrap();
    let mut handle = PreparedRun::new(&doc, RunOptions::default())
        .unwrap()
        .start(CancellationToken::new());
    while handle.events.recv().await.is_some() {}
    let report = handle.task.await.unwrap();
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(report.summary.requests, 2);
    assert!(!report
        .events
        .iter()
        .any(|e| matches!(e.event, EventKind::StepStarted { .. })
            && e.node_id.as_deref() == Some("http-3")));
}

async fn run_document(value: Value) -> RunReport {
    let document = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    let mut handle = PreparedRun::new(&document, RunOptions::default())
        .unwrap()
        .start(CancellationToken::new());
    while handle.events.recv().await.is_some() {}
    handle.task.await.unwrap()
}
fn body_document(base_url: &str) -> Value {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../scenarios/examples/exclusive-routing.interl.json"
    ))
    .unwrap();
    value["clientProfiles"][0]["baseUrl"] = json!(base_url);
    value["workflows"][0]["nodes"][1]["config"]["path"] = json!("/routing-body");
    value
}
#[tokio::test]
async fn body_conditions_select_routes_without_exporting_the_body() {
    let server = support::TestServer::start().await;
    let mut value = body_document(&server.base_url);
    value["workflows"][0]["edges"][2]["condition"]["expression"] = json!("response.body.value.status = true and response.body.value.attempts >= 3 and response.body.value.label = \"route-private-label\"");
    let report = run_document(value).await;
    assert_eq!(report.status, RunStatus::Passed, "{:?}", report.error);
    assert!(report
        .events
        .iter()
        .any(|event| event.node_id.as_deref() == Some("accepted")
            && matches!(event.event, EventKind::StepStarted { .. })));
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("route-private-label"));
}
#[tokio::test]
async fn typed_extractions_feed_route_bindings_and_workflow_outputs() {
    let server = support::TestServer::start().await;
    for (kind, property, condition) in [
        ("boolean", "status", "inputs.result = true"),
        ("integer", "attempts", "inputs.result = 3"),
        ("string", "label", "inputs.result = \"route-private-label\""),
    ] {
        let mut value = body_document(&server.base_url);
        let workflow = &mut value["workflows"][0];
        workflow["nodes"].as_array_mut().unwrap().push(json!({"id":"extracted","type":"extract","config":{"sourceNodeId":"health","expression":format!("response.body.value.{property}"),"output":"result","type":kind}}));
        workflow["edges"][1]["to"] = json!("extracted");
        workflow["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"extract-split","from":"extracted","to":"result"}));
        workflow["nodes"][2]["config"]["inputs"] =
            json!({"result":{"source":"extraction","nodeId":"extracted","name":"result"}});
        workflow["edges"][2]["condition"]["expression"] = json!(condition);
        workflow["outputs"] = json!({"result":{"type":kind,"value":{"source":"extraction","nodeId":"extracted","name":"result"}}});
        let report = run_document(value).await;
        assert_eq!(
            report.status,
            RunStatus::Passed,
            "{kind}: {:?}",
            report.error
        );
        assert!(report
            .events
            .iter()
            .any(|event| event.node_id.as_deref() == Some("accepted")
                && matches!(event.event, EventKind::StepStarted { .. })));
    }
}
#[tokio::test]
async fn missing_null_and_mistyped_body_properties_fail_instead_of_selecting_default() {
    let server = support::TestServer::start().await;
    for expression in [
        "response.body.missing = true",
        "response.body.value.empty = true",
        "response.body.value.status = 1",
    ] {
        let mut value = body_document(&server.base_url);
        value["workflows"][0]["edges"][2]["condition"]["expression"] = json!(expression);
        let report = run_document(value).await;
        assert_eq!(report.status, RunStatus::Error);
        assert!(!report.events.iter().any(|event| matches!(
            event.node_id.as_deref(),
            Some("accepted" | "fallback")
        ) && matches!(
            event.event,
            EventKind::StepStarted { .. }
        )));
    }
}

#[tokio::test]
async fn route_bindings_share_the_run_scalar_budget() {
    let server = support::TestServer::start().await;
    let mut value = body_document(&server.base_url);
    let bindings: serde_json::Map<_, _> = (0..20)
        .map(|index| {
            (
                format!("value{index}"),
                json!({"source":"literal","value":"x".repeat(4096)}),
            )
        })
        .collect();
    value["workflows"][0]["nodes"][2]["config"]["inputs"] = Value::Object(bindings);
    let report = run_document(value).await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.error.unwrap().kind, "valueLimit");
}
