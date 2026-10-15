#[path = "../../../tests/support/mod.rs"]
mod support;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
fn directory() -> PathBuf {
    // Wall-clock resolution does not guarantee uniqueness across parallel tests.
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "interl-run-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}
async fn execute(path: PathBuf, report: PathBuf) -> Output {
    tokio::task::spawn_blocking(move || {
        Command::new(env!("CARGO_BIN_EXE_interl"))
            .arg("run")
            .arg(path)
            .arg("--output")
            .arg(report)
            .output()
            .unwrap()
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn reports_success_http_errors_and_transport_failure_with_stable_exit_codes() {
    let server = support::TestServer::start().await;
    let dir = directory();
    for (index, path, status) in [(0, "/health", 0), (1, "/not-found", 0), (2, "/slow", 3)] {
        let mut doc = support::scenario(&server.base_url, path);
        if index == 2 {
            if let interl_core::WorkflowNode::Http { config, .. } = &mut doc.workflows[0].nodes[1] {
                config.timeout_ms = Some(20);
            }
        }
        let input = dir.join(format!("case{index}.interl.json"));
        let report = dir.join(format!("report{index}.json"));
        std::fs::write(&input, serde_json::to_vec(&doc).unwrap()).unwrap();
        let output = execute(input, report.clone()).await;
        assert_eq!(output.status.code(), Some(status));
        let value: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        assert_eq!(value["reportVersion"], 3);
        assert!(value["events"].as_array().unwrap().last().unwrap()["type"] == "run.finished");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("reportVersion"));
    }
    let mut doc = support::scenario(&server.base_url, "/health");
    doc.groups.push(doc.groups[0].clone());
    let input = dir.join("invalid.interl.json");
    std::fs::write(&input, serde_json::to_vec(&doc).unwrap()).unwrap();
    let report = dir.join("invalid-report.json");
    assert_eq!(execute(input, report.clone()).await.status.code(), Some(2));
    assert!(!report.exists());
    std::fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn yaml_and_json_have_equivalent_reports_and_output_cannot_overwrite_input() {
    let server = support::TestServer::start().await;
    let dir = directory();
    let doc = support::scenario(&server.base_url, "/health");
    let json_path = dir.join("scenario.interl.json");
    let yaml_path = dir.join("scenario.interl.yaml");
    std::fs::write(&json_path, serde_json::to_vec(&doc).unwrap()).unwrap();
    let yaml = include_str!("../../../scenarios/examples/hello-api.interl.yaml")
        .replace("http://127.0.0.1:8080", &server.base_url);
    std::fs::write(&yaml_path, yaml).unwrap();
    let mut reports = Vec::new();
    for (index, path) in [json_path.clone(), yaml_path].into_iter().enumerate() {
        let output = dir.join(format!("result{index}.json"));
        assert!(execute(path, output.clone()).await.status.success());
        let mut value: Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        for event in value["events"].as_array_mut().unwrap() {
            if let Some(headers) = event
                .pointer_mut("/payload/headers")
                .and_then(Value::as_object_mut)
            {
                headers.remove("date");
            }
        }
        reports.push(json!({"status":value["status"],"summary":[value["summary"]["requests"],value["summary"]["responses"]],"events":value["events"].as_array().unwrap().iter().map(|e|json!({"type":e["type"],"nodeId":e["nodeId"],"payload":e["payload"]})).collect::<Vec<_>>()}));
    }
    assert_eq!(reports[0], reports[1]);
    let before = std::fs::read(&json_path).unwrap();
    assert_eq!(
        execute(json_path.clone(), json_path.clone())
            .await
            .status
            .code(),
        Some(2)
    );
    assert_eq!(std::fs::read(json_path).unwrap(), before);
    std::fs::remove_dir_all(dir).unwrap();
}
#[cfg(unix)]
#[tokio::test]
async fn interrupt_writes_partial_cancelled_report_and_exits_130() {
    use std::process::Stdio;
    let server = support::TestServer::start().await;
    let dir = directory();
    let doc = support::scenario(&server.base_url, "/slow");
    let input = dir.join("cancel.interl.json");
    let report = dir.join("cancel.json");
    std::fs::write(&input, serde_json::to_vec(&doc).unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_interl"))
        .arg("run")
        .arg(input)
        .arg("--output")
        .arg(&report)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        server.entered.notified(),
    )
    .await
    .unwrap();
    // Send SIGINT only to the child created by this test.
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGINT) }, 0);
    let exit = tokio::task::spawn_blocking(move || child.wait().unwrap())
        .await
        .unwrap();
    assert_eq!(exit.code(), Some(130));
    let value: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(value["status"], "cancelled");
    std::fs::remove_dir_all(dir).unwrap();
}
