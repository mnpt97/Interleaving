#[path = "../../../tests/support/mod.rs"]
mod support;
use interl_core::{ScenarioDocument, ScenarioFormat, WorkflowEdge, WorkflowNode};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "interl-cli-auth-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn source(base: &str, format: ScenarioFormat) -> String {
    match format {
        ScenarioFormat::Json => include_str!("../../../scenarios/examples/auth-login.interl.json"),
        ScenarioFormat::Yaml => include_str!("../../../scenarios/examples/auth-login.interl.yaml"),
    }
    .replace("http://127.0.0.1:8080", base)
}
fn document(base: &str) -> ScenarioDocument {
    interl_core::parse_scenario(&source(base, ScenarioFormat::Json), ScenarioFormat::Json).unwrap()
}
fn command(input: &Path, report: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_interl"));
    command
        .args(["run"])
        .arg(input)
        .arg("--output")
        .arg(report)
        .env("INTERL_ALICE_USERNAME", "alice")
        .env("INTERL_ALICE_PASSWORD", "alice-password");
    command
}
async fn execute(mut command: Command) -> Output {
    tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap()
}
fn safe(bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes);
    for marker in [
        "alice-password",
        "alice%2Dpassword",
        "fake-token-",
        "fake-cookie-",
        "private-marker",
    ] {
        assert!(!text.contains(marker), "fake secret leaked");
    }
}
fn normalized(mut value: Value) -> Value {
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
fn write(directory: &Directory, name: &str, doc: &ScenarioDocument) -> PathBuf {
    let path = directory.file(name);
    std::fs::write(&path, serde_json::to_vec(doc).unwrap()).unwrap();
    path
}
fn http<'a>(doc: &'a mut ScenarioDocument, id: &str) -> &'a mut interl_core::HttpConfig {
    match doc
        .workflow
        .nodes
        .iter_mut()
        .find(|node| node.id() == id)
        .unwrap()
    {
        WorkflowNode::Http { config, .. } => config,
        _ => panic!(),
    }
}

#[tokio::test]
async fn authenticated_formats_use_child_environment_and_write_equivalent_safe_reports() {
    let server = support::TestServer::start().await;
    let directory = Directory::new();
    let mut reports = Vec::new();
    for (name, format) in [
        ("auth.interl.json", ScenarioFormat::Json),
        ("auth.interl.yaml", ScenarioFormat::Yaml),
    ] {
        let input = directory.file(name);
        let original = source(&server.base_url, format);
        std::fs::write(&input, &original).unwrap();
        let output = directory.file(&format!("{name}.report.json"));
        let child = execute(command(&input, &output)).await;
        assert_eq!(child.status.code(), Some(0));
        assert!(child.stderr.is_empty());
        safe(&child.stdout);
        safe(&child.stderr);
        assert!(!String::from_utf8_lossy(&child.stdout).contains("reportVersion"));
        assert!(String::from_utf8_lossy(&child.stdout).contains("session.updated"));
        let bytes = std::fs::read(output).unwrap();
        safe(&bytes);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["reportVersion"], 2);
        assert_eq!(value["status"], "passed");
        assert_eq!(value["summary"]["responses"], 2);
        assert_eq!(std::fs::read_to_string(input).unwrap(), original);
        reports.push(normalized(value));
    }
    assert_eq!(reports[0], reports[1]);
    struct Provider;
    impl interl_engine::SecretProvider for Provider {
        fn lookup(
            &self,
            name: &str,
        ) -> Result<std::ffi::OsString, interl_engine::SecretLookupError> {
            match name {
                "INTERL_ALICE_USERNAME" => Ok("alice".into()),
                "INTERL_ALICE_PASSWORD" => Ok("alice-password".into()),
                _ => Err(interl_engine::SecretLookupError::Missing),
            }
        }
    }
    let mut handle = interl_engine::PreparedRun::new_with_secret_provider(
        &document(&server.base_url),
        interl_engine::RunOptions::default(),
        &Provider,
    )
    .unwrap()
    .start(interl_engine::CancellationToken::new());
    while handle.events.recv().await.is_some() {}
    let headless = serde_json::to_value(handle.task.await.unwrap()).unwrap();
    assert_eq!(reports[0], normalized(headless));
}

#[tokio::test]
async fn missing_empty_and_oversized_credentials_exit_2_before_http_or_report_creation() {
    let server = support::TestServer::start().await;
    let directory = Directory::new();
    let mut doc = document(&server.base_url);
    http(&mut doc, "login").path = "/echo".into();
    let input = write(&directory, "missing.interl.json", &doc);
    for (index, value, expected) in [
        (0, None, "missingSecret"),
        (1, Some(String::new()), "emptySecret"),
        (2, Some("private-marker".repeat(700)), "secretLimit"),
    ] {
        let report = directory.file(&format!("missing-{index}.json"));
        let mut child = command(&input, &report);
        child.env_remove("INTERL_ALICE_PASSWORD");
        if let Some(value) = value {
            child.env("INTERL_ALICE_PASSWORD", value);
        }
        let output = execute(child).await;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
        assert!(String::from_utf8_lossy(&output.stderr).contains("INTERL_ALICE_PASSWORD"));
        assert!(!report.exists());
        safe(&output.stderr);
    }
    assert_eq!(server.hits.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn runtime_failures_exit_3_while_http_statuses_keep_completion_semantics() {
    let server = support::TestServer::start().await;
    let directory = Directory::new();
    for case in 0..5 {
        let mut doc = document(&server.base_url);
        let mut code = None;
        let expected;
        match case {
            0 => {
                http(&mut doc, "login").path = "/auth/login-missing-token".into();
                code = Some("extractionMissingProperty");
                expected = 3;
            }
            1 => {
                let config = http(&mut doc, "login");
                config.path = "/echo".into();
                config.body = Some(serde_json::from_value(json!({"kind":"jsonTemplate","fields":{"accessToken":{"source":"literal","value":"private-marker\r\ninjected"}}})).unwrap());
                let WorkflowNode::Extract { config, .. } = &mut doc.workflow.nodes[2] else {
                    panic!()
                };
                config.expression = "response.body.body.accessToken".into();
                code = Some("configuration");
                expected = 3;
            }
            2 | 3 => {
                doc.workflow.nodes.retain(|node| {
                    !matches!(
                        node,
                        WorkflowNode::Extract { .. } | WorkflowNode::SetSession { .. }
                    )
                });
                if case == 2 {
                    http(&mut doc, "authenticated").path = "/auth/cookie-me".into();
                }
                doc.workflow.edges = doc
                    .workflow
                    .nodes
                    .windows(2)
                    .map(|nodes| WorkflowEdge {
                        from: nodes[0].id().into(),
                        to: nodes[1].id().into(),
                    })
                    .collect();
                doc.layout = None;
                expected = 0;
            }
            _ => {
                code = Some("extractionMissingProperty");
                expected = 3;
            }
        }
        let input = write(&directory, &format!("case-{case}.interl.json"), &doc);
        let before = std::fs::read(&input).unwrap();
        let report = directory.file(&format!("case-{case}.report.json"));
        let mut child = command(&input, &report);
        if case == 4 {
            child.env("INTERL_ALICE_PASSWORD", "wrong-private-marker");
        }
        let output = execute(child).await;
        assert_eq!(output.status.code(), Some(expected));
        safe(&output.stdout);
        safe(&output.stderr);
        let bytes = std::fs::read(report).unwrap();
        safe(&bytes);
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        if let Some(code) = code {
            assert_eq!(value["status"], "error");
            assert_eq!(value["error"]["kind"], code);
            assert_eq!(value["summary"]["requests"], 1);
        } else {
            assert_eq!(value["status"], "passed");
            assert_eq!(value["summary"]["responses"], 2);
            assert_eq!(value["summary"]["httpErrorResponses"], u32::from(case == 3));
        }
        assert_eq!(std::fs::read(input).unwrap(), before);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn auth_interrupt_exits_130_with_redacted_partial_report() {
    use std::process::Stdio;
    let server = support::TestServer::start_with_auth(support::auth::AuthState::new(
        std::time::Duration::from_secs(10),
    ))
    .await;
    let directory = Directory::new();
    let mut doc = document(&server.base_url);
    http(&mut doc, "login").path = "/auth/login-slow".into();
    let input = write(&directory, "cancel.interl.json", &doc);
    let report = directory.file("cancel.report.json");
    let mut command = command(&input, &report);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let child = command.spawn().unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        server.auth_entered.notified(),
    )
    .await
    .unwrap();
    assert_eq!(
        unsafe { libc::kill(child.id().unwrap() as i32, libc::SIGINT) },
        0
    );
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output.status.code(), Some(130));
    safe(&output.stdout);
    safe(&output.stderr);
    let bytes = std::fs::read(report).unwrap();
    safe(&bytes);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["status"], "cancelled");
    assert_eq!(value["summary"]["requests"], 1);
    assert_eq!(value["summary"]["responses"], 0);
    assert!(value["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["type"] == "run.cancelled"));
}
