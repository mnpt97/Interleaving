use std::{
    path::PathBuf,
    process::{Command, Output},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_interl"))
        .args(args)
        .output()
        .unwrap()
}
fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/examples")
}

#[test]
fn equivalent_examples_succeed_without_stderr() {
    for name in [
        "hello-api.json",
        "hello-api.interl.json",
        "hello-api.interl.yaml",
        "http-workflow.interl.json",
        "http-workflow.interl.yaml",
    ] {
        let path = examples().join(name);
        let output = run(&["validate", path.to_str().unwrap()]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("Valid scenario"));
    }
}

#[test]
fn missing_paths_arguments_and_unsupported_extensions_fail() {
    for args in [
        vec!["validate"],
        vec!["validate", "/missing.interl.json"],
        vec!["validate", "scenario.txt"],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(!output.stderr.is_empty());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn malformed_and_semantically_invalid_files_report_context() {
    let dir = std::env::temp_dir().join(format!("interl-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("invalid scenario ü.interl.json");
    for (source, code) in [
        ("{", "invalidJson"),
        ("{\"schemaVersion\":2}", "unsupportedVersion"),
    ] {
        std::fs::write(&path, source).unwrap();
        let output = run(&["validate", path.to_str().unwrap()]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(stderr.contains(code));
        assert!(stderr.contains("invalid scenario ü"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn help_names_interl_and_run_rejects_missing_input() {
    assert!(String::from_utf8_lossy(&run(&["--help"]).stdout).contains("interl"));
    let output = run(&["run", "scenario.json"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!output.stderr.is_empty());
}
