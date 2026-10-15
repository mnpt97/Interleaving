#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{parse_scenario, ScenarioDocument, ScenarioFormat, WorkflowEdge, WorkflowNode};
use interl_engine::{
    CancellationToken, EventKind, PreparedRun, RunOptions, RunReport, RunStatus, SecretLookupError,
    SecretProvider,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, ffi::OsString, sync::Mutex, time::Duration};

struct Provider {
    values: Mutex<BTreeMap<String, OsString>>,
    calls: Mutex<Vec<String>>,
}
impl Default for Provider {
    fn default() -> Self {
        Self {
            values: Mutex::new(BTreeMap::from([
                ("INTERL_ALICE_USERNAME".into(), "alice".into()),
                ("INTERL_ALICE_PASSWORD".into(), "alice-password".into()),
            ])),
            calls: Mutex::default(),
        }
    }
}
impl SecretProvider for Provider {
    fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
        self.calls.lock().unwrap().push(name.into());
        self.values
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .ok_or(SecretLookupError::Missing)
    }
}
fn document(base: &str) -> ScenarioDocument {
    parse_scenario(
        &include_str!("../../../scenarios/examples/auth-login.interl.json")
            .replace("http://127.0.0.1:8080", base),
        ScenarioFormat::Json,
    )
    .unwrap()
}
fn http<'a>(doc: &'a mut ScenarioDocument, id: &str) -> &'a mut interl_core::ProgramHttp {
    match doc.workflows[0]
        .nodes
        .iter_mut()
        .find(|node| node.id() == id)
        .unwrap()
    {
        WorkflowNode::Http { config, .. } => config,
        _ => panic!(),
    }
}
fn reconnect(doc: &mut ScenarioDocument) {
    doc.workflows[0].edges = doc.workflows[0]
        .nodes
        .windows(2)
        .map(|nodes| WorkflowEdge {
            id: format!("{}-{}", nodes[0].id(), nodes[1].id()),
            condition: None,
            is_default: None,
            from: nodes[0].id().into(),
            to: nodes[1].id().into(),
        })
        .collect();
    doc.workflows[0].layout = None;
}
async fn collect(mut handle: interl_engine::RunHandle) -> RunReport {
    let mut live = Vec::new();
    while let Some(event) = handle.events.recv().await {
        live.push(event);
    }
    let report = handle.task.await.unwrap();
    assert_eq!(
        serde_json::to_value(live).unwrap(),
        serde_json::to_value(&report.events).unwrap()
    );
    assert_eq!(report.report_version, 3);
    report
}
async fn run(doc: &ScenarioDocument, provider: &Provider, options: RunOptions) -> RunReport {
    collect(
        PreparedRun::new_with_secret_provider(doc, options, provider)
            .unwrap()
            .start(CancellationToken::new()),
    )
    .await
}
fn assert_safe(report: &RunReport) {
    let wire = serde_json::to_string(report).unwrap();
    for secret in ["alice-password", "fake-token-", "fake-cookie-"] {
        assert!(!wire.contains(secret), "fake secret leaked");
    }
    for (index, event) in report.events.iter().enumerate() {
        assert_eq!(event.sequence, index as u64 + 1);
    }
    assert!(matches!(
        report.events.last().unwrap().event,
        EventKind::RunFinished { .. }
    ));
}
fn statuses(report: &RunReport) -> Vec<u16> {
    report
        .events
        .iter()
        .filter_map(|event| match event.event {
            EventKind::ResponseReceived { status, .. } => Some(status),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn bearer_login_works_in_both_formats_and_connectivity_order() {
    let server = support::TestServer::start().await;
    for (source, format) in [
        (
            include_str!("../../../scenarios/examples/auth-login.interl.json"),
            ScenarioFormat::Json,
        ),
        (
            include_str!("../../../scenarios/examples/auth-login.interl.yaml"),
            ScenarioFormat::Yaml,
        ),
    ] {
        let mut doc = parse_scenario(
            &source.replace("http://127.0.0.1:8080", &server.base_url),
            format,
        )
        .unwrap();
        doc.workflows[0].nodes.reverse();
        let provider = Provider::default();
        let report = run(&doc, &provider, RunOptions::default()).await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(statuses(&report), [200, 200]);
        assert_eq!(report.summary.requests, 2);
        assert_eq!(provider.calls.lock().unwrap().len(), 2);
        let update = report
            .events
            .iter()
            .find(|event| matches!(event.event, EventKind::SessionUpdated { .. }))
            .unwrap();
        assert_eq!(update.node_id.as_deref(), Some("apply-token"));
        assert_eq!(
            serde_json::to_value(&update.event).unwrap()["payload"],
            json!({"setHeaders":["authorization"],"removedHeaders":[]})
        );
        for event in &report.events {
            if let EventKind::RequestSent { headers, body, .. }
            | EventKind::ResponseReceived { headers, body, .. } = &event.event
            {
                assert_eq!(body["omitted"], true);
                assert!(headers.get("[omitted]").is_some());
            }
        }
        assert_safe(&report);
    }
}

#[tokio::test]
async fn preparation_captures_credentials_once_and_exports_only_references() {
    let server = support::TestServer::start().await;
    let mut doc = document(&server.base_url);
    let WorkflowNode::SetSession { config, .. } = &mut doc.workflows[0].nodes[3] else {
        panic!()
    };
    config.set_headers.insert(
        "x-opaque".into(),
        interl_core::SessionHeaderValue::Literal {
            value: "literal-session-private-marker".into(),
            prefix: "Custom ".into(),
        },
    );
    let original = interl_core::serialize_scenario(&doc).unwrap();
    let provider = Provider::default();
    let prepared =
        PreparedRun::new_with_secret_provider(&doc, RunOptions::default(), &provider).unwrap();
    provider.values.lock().unwrap().insert(
        "INTERL_ALICE_PASSWORD".into(),
        "changed-after-preparation".into(),
    );
    let report = collect(prepared.start(CancellationToken::new())).await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(provider.calls.lock().unwrap().len(), 2);
    assert_eq!(
        report.snapshot["identities"][0]["credentials"]["password"]["name"],
        "INTERL_ALICE_PASSWORD"
    );
    assert_eq!(interl_core::serialize_scenario(&doc).unwrap(), original);
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("literal-session-private-marker"));
    assert_safe(&report);
}

#[tokio::test]
async fn cookie_only_login_reuses_the_user_transport_without_extraction() {
    let server = support::TestServer::start().await;
    for (source, format) in [
        (
            include_str!("../../../scenarios/examples/auth-cookie.interl.json"),
            ScenarioFormat::Json,
        ),
        (
            include_str!("../../../scenarios/examples/auth-cookie.interl.yaml"),
            ScenarioFormat::Yaml,
        ),
    ] {
        let doc = parse_scenario(
            &source.replace("http://127.0.0.1:8080", &server.base_url),
            format,
        )
        .unwrap();
        let report = run(&doc, &Provider::default(), RunOptions::default()).await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(statuses(&report), [200, 200]);
        assert!(!report
            .events
            .iter()
            .any(|event| matches!(event.event, EventKind::SessionUpdated { .. })));
        assert_safe(&report);
    }
}

#[tokio::test]
async fn static_identity_bearer_header_does_not_require_a_login_step() {
    let server = support::TestServer::start().await;
    let mut doc = document(&server.base_url);
    let session = interl_http::HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    let login = http(&mut doc, "login").clone();
    let response = session
        .execute(
            session
                .prepare_resolved(
                    &serde_json::from_value(serde_json::to_value(&login).unwrap()).unwrap(),
                    &Default::default(),
                    interl_http::ResolvedBody::Json(
                        json!({"username":"alice","password":"alice-password"}),
                    ),
                )
                .unwrap(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let value: Value = serde_json::from_slice(&response.body).unwrap();
    let provider = Provider::default();
    provider.values.lock().unwrap().insert(
        "STATIC_TOKEN".into(),
        value["accessToken"].as_str().unwrap().into(),
    );
    doc.identities[0].credentials.insert(
        "token".into(),
        interl_core::SecretReference::Env {
            name: "STATIC_TOKEN".into(),
        },
    );
    doc.workflows[0]
        .nodes
        .retain(|node| node.id() != "login" && !matches!(node, WorkflowNode::Extract { .. }));
    let WorkflowNode::SetSession { config, .. } = &mut doc.workflows[0].nodes[1] else {
        panic!()
    };
    config.set_headers.insert(
        "authorization".into(),
        interl_core::SessionHeaderValue::Identity {
            field: "token".into(),
            prefix: "Bearer ".into(),
        },
    );
    reconnect(&mut doc);
    let report = run(&doc, &provider, RunOptions::default()).await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(statuses(&report), [200]);
    assert_eq!(provider.calls.lock().unwrap().as_slice(), ["STATIC_TOKEN"]);
    assert_safe(&report);
}

#[tokio::test]
async fn extraction_failures_stop_before_session_update_or_following_requests() {
    let server = support::TestServer::start().await;
    for (path, expected) in [
        ("/auth/login-missing-token", "extractionMissingProperty"),
        ("/auth/login-malformed", "extractionInvalidJson"),
        ("/auth/login-text", "extractionInvalidJson"),
        ("/auth/login-large", "extractionResponseTruncated"),
        ("/auth/login-unfamiliar", "extractionMissingProperty"),
    ] {
        let mut doc = document(&server.base_url);
        http(&mut doc, "login").path = path.into();
        http(&mut doc, "authenticated").path = "/echo".into();
        let report = run(&doc, &Provider::default(), RunOptions::default()).await;
        assert_eq!(report.status, RunStatus::Error);
        assert_eq!(report.error.as_ref().unwrap().kind, expected);
        assert_eq!(report.summary.requests, 1);
        assert!(!report
            .events
            .iter()
            .any(|event| matches!(event.event, EventKind::SessionUpdated { .. })));
        assert_eq!(server.hits.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_safe(&report);
    }
}

#[tokio::test]
async fn unfamiliar_token_fields_are_omitted_before_extraction() {
    let server = support::TestServer::start().await;
    let mut doc = document(&server.base_url);
    http(&mut doc, "login").path = "/auth/login-unfamiliar".into();
    let WorkflowNode::Extract { config, .. } = &mut doc.workflows[0].nodes[2] else {
        panic!()
    };
    config.expression = "response.body.opaqueArtifact".into();
    let report = run(&doc, &Provider::default(), RunOptions::default()).await;
    assert_eq!(statuses(&report), [200, 200]);
    assert_safe(&report);
}

#[tokio::test]
async fn nested_templates_reuse_outputs_without_interpreting_literal_objects() {
    let server = support::TestServer::start().await;
    let mut doc = document(&server.base_url);
    let config = http(&mut doc, "authenticated");
    config.path = "/echo".into();
    config.method = interl_core::HttpMethod::Post;
    config.body = Some(serde_json::from_value(json!({"kind":"jsonTemplate","fields":{
        "nested":{"source":"object","fields":{"list":{"source":"array","items":[{"source":"extraction","nodeId":"extract-token","name":"token"},{"source":"identity","field":"username"}]}}},
        "literal":{"source":"literal","value":{"source":"identity","field":"not-a-reference","text":"${untouched}"}}
    }})).unwrap());
    let report = run(&doc, &Provider::default(), RunOptions::default()).await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(server.hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_safe(&report);
}

#[tokio::test]
async fn rendered_body_limit_stops_before_http_and_missing_secrets_send_nothing() {
    let server = support::TestServer::start().await;
    let doc = document(&server.base_url);
    let report = run(
        &doc,
        &Provider::default(),
        RunOptions {
            max_request_bytes: 32,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.summary.requests, 0);
    assert_eq!(report.error.as_ref().unwrap().kind, "configuration");
    let provider = Provider::default();
    provider.values.lock().unwrap().clear();
    let errors = PreparedRun::new_with_secret_provider(&doc, RunOptions::default(), &provider)
        .err()
        .unwrap();
    assert_eq!(errors[0].code, "missingSecret");
    assert_safe(&report);
}

#[tokio::test]
async fn failed_credentials_are_http_responses_without_hidden_retries() {
    let server = support::TestServer::start().await;
    let doc = document(&server.base_url);
    let provider = Provider::default();
    provider.values.lock().unwrap().insert(
        "INTERL_ALICE_PASSWORD".into(),
        "wrong-private-marker".into(),
    );
    let report = run(&doc, &provider, RunOptions::default()).await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(statuses(&report), [401]);
    assert_eq!(report.summary.http_error_responses, 1);
    assert_eq!(report.summary.requests, 1);
    assert_eq!(
        report.error.as_ref().unwrap().kind,
        "extractionMissingProperty"
    );
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("wrong-private-marker"));
    assert_safe(&report);
}

#[tokio::test]
async fn removing_authorization_suppresses_profile_defaults_and_keeps_cookies() {
    let server = support::TestServer::start().await;
    let mut doc = document(&server.base_url);
    doc.client_profiles[0].headers.insert(
        "Authorization".into(),
        "Bearer obsolete-profile-marker".into(),
    );
    let remove: WorkflowNode = serde_json::from_value(
        json!({"id":"remove","type":"setSession","config":{"removeHeaders":["AUTHORIZATION"]}}),
    )
    .unwrap();
    let cookie: WorkflowNode = serde_json::from_value(
        json!({"id":"cookie","type":"http","config":{"method":"GET","path":"/auth/cookie-me"}}),
    )
    .unwrap();
    let bearer: WorkflowNode = serde_json::from_value(
        json!({"id":"removed-bearer","type":"http","config":{"method":"GET","path":"/auth/me"}}),
    )
    .unwrap();
    let end = doc.workflows[0].nodes.pop().unwrap();
    doc.workflows[0].nodes.extend([remove, cookie, bearer, end]);
    reconnect(&mut doc);
    let report = run(&doc, &Provider::default(), RunOptions::default()).await;
    assert_eq!(report.status, RunStatus::Passed);
    assert_eq!(statuses(&report), [200, 200, 200, 401]);
    assert_eq!(report.summary.http_error_responses, 1);
    let removes: Vec<_> = report
        .events
        .iter()
        .filter_map(|event| match &event.event {
            EventKind::SessionUpdated {
                removed_headers, ..
            } if !removed_headers.is_empty() => Some(removed_headers),
            _ => None,
        })
        .collect();
    assert_eq!(removes.len(), 1);
    assert_eq!(removes[0].as_slice(), ["authorization"]);
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("obsolete-profile-marker"));
    assert_safe(&report);
}

#[tokio::test]
async fn login_timeout_cancellation_and_receiver_close_keep_safe_partial_reports() {
    let server = support::TestServer::start().await;
    let mut doc = document(&server.base_url);
    http(&mut doc, "login").path = "/auth/login-slow".into();
    http(&mut doc, "login").timeout_ms = Some(20);
    let report = run(&doc, &Provider::default(), RunOptions::default()).await;
    assert_eq!(report.error.as_ref().unwrap().kind, "timeout");
    server.auth_entered.notified().await;
    assert_safe(&report);
    http(&mut doc, "login").timeout_ms = Some(1000);
    for close in [false, true] {
        let cancel = CancellationToken::new();
        let mut handle = PreparedRun::new_with_secret_provider(
            &doc,
            RunOptions::default(),
            &Provider::default(),
        )
        .unwrap()
        .start(cancel.clone());
        tokio::time::timeout(Duration::from_secs(2), server.auth_entered.notified())
            .await
            .unwrap();
        if close {
            drop(handle.events);
        } else {
            cancel.cancel();
            while handle.events.recv().await.is_some() {}
        }
        let report = tokio::time::timeout(Duration::from_secs(2), handle.task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(report.status, RunStatus::Cancelled);
        assert_eq!(report.summary.requests, 1);
        assert_eq!(report.summary.responses, 0);
        assert_safe(&report);
    }
}

#[tokio::test]
async fn cancellation_at_extraction_boundaries_never_sends_authenticated_request() {
    let server = support::TestServer::start().await;
    let doc = document(&server.base_url);
    for after_extract_started in [false, true] {
        let cancel = CancellationToken::new();
        let mut handle = PreparedRun::new_with_secret_provider(
            &doc,
            RunOptions {
                event_capacity: 1,
                ..Default::default()
            },
            &Provider::default(),
        )
        .unwrap()
        .start(cancel.clone());
        loop {
            let event = handle.events.recv().await.unwrap();
            let stop = if after_extract_started {
                event.node_id.as_deref() == Some("extract-token")
                    && matches!(event.event, EventKind::StepStarted { .. })
            } else {
                event.node_id.as_deref() == Some("login")
                    && matches!(event.event, EventKind::ResponseReceived { .. })
            };
            if stop {
                cancel.cancel();
                break;
            }
        }
        while handle.events.recv().await.is_some() {}
        let report = handle.task.await.unwrap();
        assert_eq!(report.status, RunStatus::Cancelled);
        assert_eq!(report.summary.requests, 1);
        assert!(!report
            .events
            .iter()
            .any(|event| matches!(event.event, EventKind::SessionUpdated { .. })));
        assert_safe(&report);
    }
}

#[tokio::test]
async fn auth_report_budget_stops_execution_and_reserves_terminal_events() {
    let server = support::TestServer::start().await;
    let mut doc = document(&server.base_url);
    let end = doc.workflows[0].nodes.pop().unwrap();
    for index in 0..100 {
        doc.workflows[0].nodes.push(serde_json::from_value(json!({"id":format!("more-{index}"),"type":"http","config":{"method":"GET","path":"/auth/me"}})).unwrap());
    }
    doc.workflows[0].nodes.push(end);
    reconnect(&mut doc);
    let report = run(
        &doc,
        &Provider::default(),
        RunOptions {
            max_report_bytes: 262144,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(report.error.as_ref().unwrap().kind, "resourceLimit");
    assert!(report.events_truncated);
    assert!(serde_json::to_vec(&report).unwrap().len() <= 262144);
    assert_safe(&report);
}

#[tokio::test]
async fn invalid_rendered_headers_stop_the_workflow_without_partial_update_events() {
    let server = support::TestServer::start().await;
    for (token, expected) in [
        ("private-marker\r\ninjected".into(), "configuration"),
        ("t".repeat(8192), "sessionHeaderLimit"),
    ] {
        let mut doc = document(&server.base_url);
        let config = http(&mut doc, "login");
        config.path = "/echo".into();
        config.body = Some(serde_json::from_value(json!({"kind":"jsonTemplate","fields":{"accessToken":{"source":"literal","value":token}}})).unwrap());
        let WorkflowNode::Extract { config, .. } = &mut doc.workflows[0].nodes[2] else {
            panic!()
        };
        config.expression = "response.body.body.accessToken".into();
        let report = run(&doc, &Provider::default(), RunOptions::default()).await;
        assert_eq!(report.status, RunStatus::Error);
        assert_eq!(report.error.as_ref().unwrap().kind, expected);
        assert_eq!(report.summary.requests, 1);
        assert!(!report
            .events
            .iter()
            .any(|event| matches!(event.event, EventKind::SessionUpdated { .. })));
        assert!(!serde_json::to_string(&report)
            .unwrap()
            .contains("private-marker"));
        assert_safe(&report);
    }
}

#[tokio::test]
async fn cancellation_interrupts_incomplete_login_body_without_emitting_payloads() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let entered = std::sync::Arc::new(tokio::sync::Notify::new());
    let ready = entered.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0u8; 1024];
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let count = stream.read(&mut chunk).await.unwrap();
            assert!(count > 0 && request.len() + count <= 65536);
            request.extend_from_slice(&chunk[..count]);
        }
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 200\r\nConnection: close\r\n\r\n{\"accessToken\":\"fake-token-body-read-marker").await.unwrap();
        stream.flush().await.unwrap();
        ready.notify_one();
        let _ = stopped.await;
    });
    let cancel = CancellationToken::new();
    let mut handle = PreparedRun::new_with_secret_provider(
        &document(&base),
        RunOptions::default(),
        &Provider::default(),
    )
    .unwrap()
    .start(cancel.clone());
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    cancel.cancel();
    while handle.events.recv().await.is_some() {}
    let report = tokio::time::timeout(Duration::from_secs(2), handle.task)
        .await
        .unwrap()
        .unwrap();
    let _ = stop.send(());
    peer.await.unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.summary.responses, 0);
    assert_eq!(report.summary.requests, 1);
    assert_safe(&report);
}
