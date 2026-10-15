use super::*;
use interl_engine::{EventKind, RunStatus, SecretLookupError};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct Provider {
    calls: AtomicUsize,
}
impl SecretProvider for Provider {
    fn lookup(&self, name: &str) -> Result<std::ffi::OsString, SecretLookupError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        match name {
            "INTERL_ALICE_USERNAME" => Ok("alice".into()),
            "INTERL_ALICE_PASSWORD" => Ok("alice-password".into()),
            _ => Err(SecretLookupError::Missing),
        }
    }
}
fn source(base: &str, format: ScenarioFormat) -> String {
    match format {
        ScenarioFormat::Json => {
            include_str!("../../../../scenarios/examples/auth-login.interl.json")
        }
        ScenarioFormat::Yaml => {
            include_str!("../../../../scenarios/examples/auth-login.interl.yaml")
        }
    }
    .replace("http://127.0.0.1:8080", base)
}
fn normalized(report: &RunReport) -> Value {
    let mut value = serde_json::to_value(report).unwrap();
    value.as_object_mut().unwrap().remove("runId");
    value["summary"]
        .as_object_mut()
        .unwrap()
        .remove("elapsedNs");
    for event in value["events"].as_array_mut().unwrap() {
        event.as_object_mut().unwrap().remove("runId");
        event.as_object_mut().unwrap().remove("elapsedNs");
        if let Some(headers) = event
            .pointer_mut("/payload/headers")
            .and_then(Value::as_object_mut)
        {
            headers.remove("date");
        }
    }
    value
}
fn safe(value: &Value) {
    let wire = value.to_string();
    for marker in [
        "alice-password",
        "fake-token-",
        "fake-cookie-",
        "provider-private-marker",
    ] {
        assert!(!wire.contains(marker), "fake secret leaked");
    }
}
async fn complete(state: &RunRegistry, handle: RunHandle) -> RunReport {
    let mut live = Vec::new();
    let (id, result) = forward_events(handle, |event| {
        live.push(
            serde_json::to_value(RunMessage::Event {
                protocol_version: 3,
                event: Box::new(event),
            })
            .unwrap(),
        );
        true
    })
    .await;
    let report = result.unwrap();
    state.finish(&id, Some(&report));
    assert_eq!(
        live.iter()
            .map(|message| message["event"].clone())
            .collect::<Vec<_>>(),
        serde_json::to_value(&report.events)
            .unwrap()
            .as_array()
            .unwrap()
            .clone()
    );
    let finished = serde_json::to_value(RunMessage::Finished {
        protocol_version: 3,
        report: Box::new(report.clone()),
    })
    .unwrap();
    assert_eq!(finished["kind"], "finished");
    assert_eq!(finished["report"]["reportVersion"], 3);
    safe(&finished);
    safe(&serde_json::to_value(live).unwrap());
    report
}
fn start(state: &RunRegistry, base: &str, provider: &dyn SecretProvider) -> RunHandle {
    state
        .start(|| {
            prepare_with_provider(
                &source(base, ScenarioFormat::Json),
                ScenarioFormat::Json,
                RunOptions::default(),
                provider,
            )
        })
        .unwrap()
}

#[tokio::test]
async fn auth_adapter_formats_headless_events_and_exports_have_parity() {
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let provider = Provider::default();
    let mut reports = Vec::new();
    for format in [ScenarioFormat::Json, ScenarioFormat::Yaml] {
        let input = source(&server.base_url, format);
        let handle = state
            .start(|| prepare_with_provider(&input, format, RunOptions::default(), &provider))
            .unwrap();
        let report = complete(&state, handle).await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(report.summary.responses, 2);
        assert!(
            report
                .events
                .iter()
                .any(|event| matches!(event.event, EventKind::SessionUpdated { .. }))
        );
        let saved: Value =
            serde_json::from_slice(&state.report_bytes(&report.run_id).unwrap()).unwrap();
        assert_eq!(saved, serde_json::to_value(&report).unwrap());
        safe(&saved);
        reports.push(normalized(&report));
    }
    let doc = interl_core::parse_scenario(
        &source(&server.base_url, ScenarioFormat::Json),
        ScenarioFormat::Json,
    )
    .unwrap();
    let handle = PreparedRun::new_with_secret_provider(&doc, RunOptions::default(), &provider)
        .unwrap()
        .start(CancellationToken::new());
    let (id, result) = forward_events(handle, |_| true).await;
    let headless = result.unwrap();
    assert_eq!(id, headless.run_id);
    reports.push(normalized(&headless));
    assert!(reports.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(provider.calls.load(Ordering::Relaxed), 6);
}

#[tokio::test]
async fn preparing_and_running_slots_reject_second_start_before_preparation() {
    let state = RunRegistry::default();
    let reservation = state.reserve().unwrap();
    let errors = state
        .start(|| panic!("rejected start must not parse or read secrets"))
        .err()
        .unwrap();
    assert_eq!(errors[0].code, "runOperation");
    drop(reservation);
    let server = support::TestServer::start().await;
    let handle = start(&state, &server.base_url, &Provider::default());
    let id = handle.run_id.clone();
    let errors = state
        .start(|| panic!("rejected start must not parse or read secrets"))
        .err()
        .unwrap();
    assert!(errors[0].message.contains("already active"));
    assert!(!state.cancel("stale"));
    assert!(state.cancel(&id));
    let report = complete(&state, handle).await;
    assert_eq!(report.status, RunStatus::Cancelled);
    assert!(!state.cancel(&id));
}

#[tokio::test]
async fn failed_and_panicking_preparation_release_slot_and_preserve_last_report() {
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let report = complete(
        &state,
        start(&state, &server.base_url, &Provider::default()),
    )
    .await;
    let before = state.report_bytes(&report.run_id).unwrap();
    let errors = state
        .start(|| {
            prepare_with_provider(
                "{",
                ScenarioFormat::Json,
                RunOptions::default(),
                &Provider::default(),
            )
        })
        .err()
        .unwrap();
    assert_eq!(errors[0].code, "invalidJson");
    assert!(state.0.lock().unwrap().active.is_none());
    assert_eq!(state.report_bytes(&report.run_id).unwrap(), before);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.start(|| panic!("injected preparation panic"))
    }));
    assert!(panic.is_err());
    assert!(state.0.lock().unwrap().active.is_none());
    assert_eq!(state.report_bytes(&report.run_id).unwrap(), before);
    let next = complete(
        &state,
        start(&state, &server.base_url, &Provider::default()),
    )
    .await;
    assert_eq!(next.status, RunStatus::Passed);
}

#[tokio::test]
async fn cancellation_during_credential_preparation_prevents_spawn_and_preserves_report() {
    struct CancelProvider<'a>(&'a RunRegistry);
    impl SecretProvider for CancelProvider<'_> {
        fn lookup(&self, name: &str) -> Result<std::ffi::OsString, SecretLookupError> {
            // This must not deadlock: the registry lock is released during IO.
            self.0.cancel_active();
            Provider::default().lookup(name)
        }
    }
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let report = complete(
        &state,
        start(&state, &server.base_url, &Provider::default()),
    )
    .await;
    let before = state.report_bytes(&report.run_id).unwrap();
    let errors = state
        .start(|| {
            prepare_with_provider(
                &source(&server.base_url, ScenarioFormat::Json),
                ScenarioFormat::Json,
                RunOptions::default(),
                &CancelProvider(&state),
            )
        })
        .err()
        .unwrap();
    assert!(errors[0].message.contains("cancelled"));
    assert!(state.0.lock().unwrap().active.is_none());
    assert_eq!(state.report_bytes(&report.run_id).unwrap(), before);
    let reservation = state.reserve().unwrap();
    drop(reservation);
}

#[tokio::test]
async fn stale_completion_and_cancellation_cannot_change_a_new_run() {
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let old = complete(
        &state,
        start(&state, &server.base_url, &Provider::default()),
    )
    .await;
    let handle = start(&state, &server.base_url, &Provider::default());
    let id = handle.run_id.clone();
    assert!(state.report_bytes(&old.run_id).is_err());
    state.finish(&old.run_id, Some(&old));
    assert!(!state.cancel(&old.run_id));
    assert!(!handle.cancel.is_cancelled());
    assert_eq!(state.0.lock().unwrap().active.as_ref().unwrap().0, id);
    state.cancel_active();
    let report = complete(&state, handle).await;
    assert_eq!(report.status, RunStatus::Cancelled);
}

#[tokio::test]
async fn failed_event_delivery_cancels_auth_and_retains_safe_partial_report() {
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let input = source(&server.base_url, ScenarioFormat::Json)
        .replace("/auth/login\"", "/auth/login-slow\"");
    let handle = state
        .start(|| {
            prepare_with_provider(
                &input,
                ScenarioFormat::Json,
                RunOptions::default(),
                &Provider::default(),
            )
        })
        .unwrap();
    let (id, result) = forward_events(handle, |_| false).await;
    let report = result.unwrap();
    state.finish(&id, Some(&report));
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.delivery_dropped_events, report.events.len() as u64);
    assert!(
        report
            .events
            .iter()
            .any(|event| matches!(event.event, EventKind::RunCancelled { .. }))
    );
    safe(&serde_json::from_slice(&state.report_bytes(&id).unwrap()).unwrap());
    assert!(state.0.lock().unwrap().active.is_none());
}

#[tokio::test]
async fn task_failure_cleans_up_active_state_without_publishing_an_old_report() {
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let handle = start(&state, &server.base_url, &Provider::default());
    handle.task.abort();
    let (id, result) = forward_events(handle, |_| true).await;
    assert!(result.is_err());
    state.finish(&id, result.as_ref().ok());
    assert!(state.0.lock().unwrap().active.is_none());
    assert!(state.report_bytes(&id).is_err());
    let report = complete(
        &state,
        start(&state, &server.base_url, &Provider::default()),
    )
    .await;
    assert_eq!(report.status, RunStatus::Passed);
}

#[tokio::test]
async fn preparation_failure_diagnostics_match_headless_engine_without_secret_values() {
    struct Missing;
    impl SecretProvider for Missing {
        fn lookup(&self, _: &str) -> Result<std::ffi::OsString, SecretLookupError> {
            Err(SecretLookupError::Missing)
        }
    }
    let input = source("http://127.0.0.1:1", ScenarioFormat::Json);
    let doc = interl_core::parse_scenario(&input, ScenarioFormat::Json).unwrap();
    let desktop = prepare_with_provider(
        &input,
        ScenarioFormat::Json,
        RunOptions::default(),
        &Missing,
    )
    .err()
    .unwrap();
    let headless = PreparedRun::new_with_secret_provider(&doc, RunOptions::default(), &Missing)
        .err()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&desktop).unwrap(),
        serde_json::to_value(&headless).unwrap()
    );
    assert_eq!(desktop[0].code, "missingSecret");
    safe(&serde_json::to_value(desktop).unwrap());
}

#[tokio::test]
async fn report_export_checks_ids_extensions_and_writes_safe_json() {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "interl-desktop-auth-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let directory = Directory(path);
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let report = complete(
        &state,
        start(&state, &server.base_url, &Provider::default()),
    )
    .await;
    let output = directory.0.join("report.json");
    assert!(
        state
            .save_report(output.to_str().unwrap(), "stale")
            .is_err()
    );
    assert!(!output.exists());
    assert!(
        state
            .save_report(
                directory.0.join("report.txt").to_str().unwrap(),
                &report.run_id
            )
            .is_err()
    );
    state
        .save_report(output.to_str().unwrap(), &report.run_id)
        .unwrap();
    let value: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(value, serde_json::to_value(&report).unwrap());
    safe(&value);
    let error = state
        .save_report(
            directory
                .0
                .join("private-path-marker/missing.json")
                .to_str()
                .unwrap(),
            &report.run_id,
        )
        .unwrap_err();
    assert!(!error.to_string().contains("private-path-marker"));
    assert_eq!(
        value["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["type"] == "session.updated")
            .count(),
        1
    );
    assert!(value["snapshot"]["identities"][0]["credentials"]["password"].is_object());
    assert_eq!(value["reportVersion"], json!(3));
}

#[tokio::test]
async fn runtime_extraction_failures_match_headless_errors_and_release_slot() {
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let input = source(&server.base_url, ScenarioFormat::Json)
        .replace("/auth/login\"", "/auth/login-missing-token\"");
    let provider = Provider::default();
    let handle = state
        .start(|| {
            prepare_with_provider(
                &input,
                ScenarioFormat::Json,
                RunOptions::default(),
                &provider,
            )
        })
        .unwrap();
    let report = complete(&state, handle).await;
    assert_eq!(report.status, RunStatus::Error);
    assert_eq!(
        report.error.as_ref().unwrap().kind,
        "extractionMissingProperty"
    );
    assert_eq!(report.summary.requests, 1);
    assert!(state.0.lock().unwrap().active.is_none());
    let doc = interl_core::parse_scenario(&input, ScenarioFormat::Json).unwrap();
    let handle = PreparedRun::new_with_secret_provider(&doc, RunOptions::default(), &provider)
        .unwrap()
        .start(CancellationToken::new());
    let (_, result) = forward_events(handle, |_| true).await;
    assert_eq!(normalized(&report), normalized(&result.unwrap()));
}

#[tokio::test]
async fn window_cleanup_cancels_inflight_auth_and_preserves_exportable_partial_report() {
    let server = support::TestServer::start().await;
    let state = std::sync::Arc::new(RunRegistry::default());
    let input = source(&server.base_url, ScenarioFormat::Json)
        .replace("/auth/login\"", "/auth/login-slow\"");
    let handle = state
        .start(|| {
            prepare_with_provider(
                &input,
                ScenarioFormat::Json,
                RunOptions::default(),
                &Provider::default(),
            )
        })
        .unwrap();
    let clone = state.clone();
    let task = tokio::spawn(async move { complete(&clone, handle).await });
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        server.auth_entered.notified(),
    )
    .await
    .unwrap();
    state.cancel_active();
    let report = tokio::time::timeout(std::time::Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.status, RunStatus::Cancelled);
    assert_eq!(report.summary.requests, 1);
    assert_eq!(report.summary.responses, 0);
    assert!(state.0.lock().unwrap().active.is_none());
    safe(&serde_json::from_slice(&state.report_bytes(&report.run_id).unwrap()).unwrap());
}

#[tokio::test]
async fn reusable_v2_adapter_formats_and_exports_match_headless() {
    let server = support::TestServer::start().await;
    let state = RunRegistry::default();
    let mut reports = Vec::new();
    for (source, format) in [
        (
            include_str!("../../../../scenarios/examples/reusable-auth.interl.json"),
            ScenarioFormat::Json,
        ),
        (
            include_str!("../../../../scenarios/examples/reusable-auth.interl.yaml"),
            ScenarioFormat::Yaml,
        ),
    ] {
        let source = source.replace("http://127.0.0.1:8080", &server.base_url);
        let handle = state
            .start(|| {
                prepare_with_provider(&source, format, RunOptions::default(), &Provider::default())
            })
            .unwrap();
        let report = complete(&state, handle).await;
        assert_eq!(report.status, RunStatus::Passed);
        assert_eq!(report.summary.responses, 2);
        assert_eq!(
            report
                .events
                .iter()
                .filter_map(|e| match e.event {
                    interl_engine::EventKind::ResponseReceived { status, .. } => Some(status),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            [200, 200]
        );
        let bytes = state.report_bytes(&report.run_id).unwrap();
        safe(&serde_json::from_slice(&bytes).unwrap());
        let doc = interl_core::parse_document(&source, format).unwrap();
        let mut headless = PreparedRun::new_with_secret_provider(
            &doc,
            RunOptions::default(),
            &Provider::default(),
        )
        .unwrap()
        .start(CancellationToken::new());
        while headless.events.recv().await.is_some() {}
        assert_eq!(
            normalized(&report),
            normalized(&headless.task.await.unwrap())
        );
        reports.push(normalized(&report));
    }
    assert_eq!(reports[0], reports[1]);
}
