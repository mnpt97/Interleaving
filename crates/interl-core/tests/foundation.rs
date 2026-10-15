use interl_core::*;
use serde_json::{json, Value};
use std::path::PathBuf;

const JSON: &str = include_str!("../../../scenarios/examples/hello-api.json");
const YAML: &str = include_str!("../../../scenarios/examples/hello-api.interl.yaml");

fn example() -> ScenarioDocument {
    parse_scenario(JSON, ScenarioFormat::Json).unwrap()
}
fn invalid(value: Value) -> Vec<Diagnostic> {
    inspect_scenario(&value.to_string(), ScenarioFormat::Json).diagnostics
}
fn has(errors: &[Diagnostic], code: &str) -> bool {
    errors.iter().any(|error| error.code == code)
}
fn value() -> Value {
    serde_json::from_str(JSON).unwrap()
}

#[test]
fn formats_models_plans_and_round_trips_are_equivalent() {
    let json = example();
    let yaml = parse_scenario(YAML, ScenarioFormat::Yaml).unwrap();
    assert_eq!(json, yaml);
    assert_eq!(
        compile_scenario(&json).unwrap(),
        compile_scenario(&yaml).unwrap()
    );
    let serialized = serialize_scenario(&yaml).unwrap();
    assert_eq!(
        json,
        parse_scenario(&serialized, ScenarioFormat::Json).unwrap()
    );
    assert_eq!(serialized, serialize_scenario(&json).unwrap());
}

#[test]
fn missing_unsupported_and_wrong_type_versions_fail() {
    for version in [json!(2), json!("1"), json!(1.0), Value::Null] {
        let mut v = value();
        v["schemaVersion"] = version;
        assert!(has(&invalid(v), "unsupportedVersion"));
    }
    let mut v = value();
    v.as_object_mut().unwrap().remove("schemaVersion");
    assert!(has(&invalid(v), "missingVersion"));
}

#[test]
fn schema_is_strict_and_future_declarations_are_rejected() {
    let mut v = value();
    v["workflow"]["nodes"][1]["config"]["typo"] = json!(true);
    assert!(has(&invalid(v), "invalidSchema"));
    let mut v = value();
    v["assertions"] = json!([{"id":"future"}]);
    assert!(has(&invalid(v), "unsupportedFeature"));
    for kind in ["barrier", "assert", "fork", "loop"] {
        let mut v = value();
        v["workflow"]["nodes"][1]["type"] = json!(kind);
        assert!(has(&invalid(v), "invalidSchema"));
    }
    let mut v = value();
    v["groups"][0]["identityAssignment"]["identityId"] = json!("identity");
    assert!(has(&invalid(v), "missingIdentity"));
    let mut v = value();
    v["workflow"]["nodes"][0]["config"] = json!({});
    assert!(has(&invalid(v), "invalidSchema"));
}

#[test]
fn duplicate_keys_are_rejected_including_nested_payloads_and_decoded_keys() {
    for source in [
        r#"{"schemaVersion":1,"schemaVersion":1}"#,
        r#"{"id":1,"\u0069d":2}"#,
        r#"{"payload":{"a":1,"a":2}}"#,
    ] {
        let e = parse_scenario(source, ScenarioFormat::Json)
            .unwrap_err()
            .diagnostic;
        assert_eq!(e.code, "duplicateKey");
        assert!(e.line.is_some());
    }
    let e = parse_scenario(
        "schemaVersion: 1\nenvironment:\n  variables:\n    a: 1\n    a: 2\n",
        ScenarioFormat::Yaml,
    )
    .unwrap_err()
    .diagnostic;
    assert_eq!(e.code, "duplicateKey");
    assert_eq!(e.field_path, "/environment/variables/a");
    assert!(e.line.is_some());
}

#[test]
fn prohibited_yaml_constructs_and_ambiguous_scalars_fail() {
    for source in [
        "a: &anchor value",
        "a: *missing",
        "a: !custom value",
        "a: !!str value",
        "a: {}\n---\nb: {}",
        "? [a, b]\n: value",
        "1: value",
        "a: yes",
        "a: 01",
        "a: 2026-10-13",
        "a: .nan",
        "a: ~",
        "a:",
        "<<: {}",
    ] {
        assert!(
            parse_scenario(source, ScenarioFormat::Yaml).is_err(),
            "accepted {source}"
        );
    }
    for source in ["", "a: [", "a: 'unterminated"] {
        assert!(parse_scenario(source, ScenarioFormat::Yaml).is_err());
    }
}

#[test]
fn expression_strings_and_json_body_are_preserved_without_evaluation() {
    let text = "response.status = 201 and response.body.id != null";
    let mut v = value();
    v["environment"]["variables"]["expression"] = json!(text);
    v["workflow"]["nodes"][1]["config"]["body"] =
        json!({"kind":"json","value":{"expression":text,"nested":[true,null,3]}});
    let scenario = parse_scenario(&v.to_string(), ScenarioFormat::Json).unwrap();
    assert_eq!(scenario.environment.variables["expression"], text);
    let yaml = YAML.replace(
        "variables: {}",
        &format!("variables:\n    expression: \"{text}\""),
    );
    assert_eq!(
        parse_scenario(&yaml, ScenarioFormat::Yaml)
            .unwrap()
            .environment
            .variables["expression"],
        text
    );
    assert!(compile_scenario(&scenario).is_ok());
}

#[test]
fn limits_apply_to_both_formats() {
    for format in [ScenarioFormat::Json, ScenarioFormat::Yaml] {
        for limits in [
            ScenarioLimits {
                max_bytes: 8,
                ..ScenarioLimits::default()
            },
            ScenarioLimits {
                max_depth: 1,
                ..ScenarioLimits::default()
            },
            ScenarioLimits {
                max_collection_size: 1,
                ..ScenarioLimits::default()
            },
            ScenarioLimits {
                max_values: 4,
                ..ScenarioLimits::default()
            },
        ] {
            let source = if format == ScenarioFormat::Json {
                JSON
            } else {
                YAML
            };
            assert_eq!(
                parse_scenario_with_limits(source, format, limits)
                    .unwrap_err()
                    .diagnostic
                    .category,
                ErrorCategory::ResourceLimit
            );
        }
    }
    let too_deep = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    for format in [ScenarioFormat::Json, ScenarioFormat::Yaml] {
        assert!(parse_scenario(&too_deep, format).is_err());
    }
}

#[test]
fn reference_counts_urls_timeouts_and_duplicate_ids_are_validated() {
    let cases = [
        (
            "missingProfile",
            "/groups/0/clientProfileId",
            json!("missing"),
        ),
        ("invalidUserCount", "/groups/0/count", json!(0)),
        ("invalidTimeout", "/clientProfiles/0/timeoutMs", json!(0)),
        (
            "invalidTimeout",
            "/workflow/nodes/1/config/timeoutMs",
            json!(0),
        ),
        (
            "invalidBaseUrl",
            "/clientProfiles/0/baseUrl",
            json!("file:///tmp/test"),
        ),
        (
            "invalidBaseUrl",
            "/clientProfiles/0/baseUrl",
            json!("https://user:secret@example.com"),
        ),
        (
            "invalidRequestPath",
            "/workflow/nodes/1/config/path",
            json!("//example.com"),
        ),
        (
            "unknownLayoutNode",
            "/layout/positions/missing",
            json!({"x":0,"y":0}),
        ),
    ];
    for (code, path, replacement) in cases {
        let mut v = value();
        if path.ends_with("missing") {
            v["layout"]["positions"]["missing"] = replacement;
        } else if path.ends_with("timeoutMs") && path.starts_with("/workflow") {
            v["workflow"]["nodes"][1]["config"]["timeoutMs"] = replacement;
        } else {
            *v.pointer_mut(path).unwrap() = replacement;
        }
        assert!(has(&invalid(v), code), "missing {code}");
    }
    for collection in ["clientProfiles", "groups"] {
        let mut v = value();
        let item = v[collection][0].clone();
        v[collection].as_array_mut().unwrap().push(item);
        assert!(has(&invalid(v), "duplicateId"));
    }
    let mut v = value();
    v["workflow"]["nodes"][1]["id"] = json!("start");
    assert!(has(&invalid(v), "duplicateId"));
    let mut v = value();
    v["groups"][0]["count"] = json!(-1);
    assert!(has(&invalid(v), "invalidSchema"));
    let mut v = value();
    v["clientProfiles"][0]["headers"]["bad name"] = json!("value");
    assert!(has(&invalid(v), "invalidHeader"));
}

#[test]
fn invalid_graph_shapes_never_compile() {
    let cases = [
        ("danglingEdge", json!([{"from":"start","to":"absent"}])),
        (
            "duplicateEdge",
            json!([{"from":"start","to":"health"},{"from":"start","to":"health"},{"from":"health","to":"end"}]),
        ),
        (
            "cycle",
            json!([{"from":"start","to":"health"},{"from":"health","to":"start"}]),
        ),
        (
            "selfLoop",
            json!([{"from":"start","to":"health"},{"from":"health","to":"health"}]),
        ),
        ("unreachableNode", json!([{"from":"start","to":"end"}])),
        (
            "nonLinearGraph",
            json!([{"from":"start","to":"health"},{"from":"start","to":"end"},{"from":"health","to":"end"}]),
        ),
    ];
    for (code, edges) in cases {
        let mut v = value();
        v["workflow"]["edges"] = edges;
        let s = parse_scenario(&v.to_string(), ScenarioFormat::Json).unwrap();
        assert!(has(&validate_scenario(&s).unwrap_err(), code));
        assert!(compile_scenario(&s).is_err());
    }
    for (kind, code) in [
        ("start", "startCount"),
        ("end", "endCount"),
        ("http", "missingHttp"),
    ] {
        let mut v = value();
        v["workflow"]["nodes"]
            .as_array_mut()
            .unwrap()
            .retain(|node| node["type"] != kind);
        assert!(has(&invalid(v), code));
    }
}

#[test]
fn multiple_http_nodes_array_order_and_layout_do_not_change_connectivity() {
    let mut v = value();
    v["workflow"]["nodes"].as_array_mut().unwrap().push(json!({"id":"echo","type":"http","config":{"method":"POST","path":"/echo","body":{"kind":"text","value":"hello"}}}));
    v["workflow"]["edges"] = json!([{"from":"start","to":"health"},{"from":"health","to":"echo"},{"from":"echo","to":"end"}]);
    let mut scenario = parse_scenario(&v.to_string(), ScenarioFormat::Json).unwrap();
    let plan = compile_scenario(&scenario).unwrap();
    assert_eq!(
        plan.steps()
            .iter()
            .map(WorkflowNode::id)
            .collect::<Vec<_>>(),
        ["start", "health", "echo", "end"]
    );
    scenario.workflow.nodes.reverse();
    scenario.workflow.edges.reverse();
    scenario.layout = None;
    assert_eq!(plan, compile_scenario(&scenario).unwrap());
}

#[test]
fn loader_extensions_file_errors_utf8_and_limits() {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/examples");
    assert_eq!(
        load_scenario(&examples.join("hello-api.json")).unwrap(),
        load_scenario(&examples.join("hello-api.interl.yaml")).unwrap()
    );
    assert_eq!(
        load_scenario(&examples.join("missing.txt"))
            .unwrap_err()
            .diagnostic
            .code,
        "unsupportedExtension"
    );
    assert_eq!(
        load_scenario(&examples.join("missing.json"))
            .unwrap_err()
            .diagnostic
            .code,
        "readFile"
    );
    let dir = std::env::temp_dir().join(format!("interl-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("scenario.interl.yml");
    std::fs::write(&path, YAML).unwrap();
    assert_eq!(load_scenario(&path).unwrap(), example());
    assert_eq!(
        load_scenario_with_limits(
            &path,
            ScenarioLimits {
                max_bytes: 1,
                ..ScenarioLimits::default()
            }
        )
        .unwrap_err()
        .diagnostic
        .code,
        "fileSizeLimit"
    );
    std::fs::write(&path, [0xff]).unwrap();
    let error = load_scenario(&path).unwrap_err().diagnostic;
    assert_eq!(error.code, "invalidUtf8");
    assert!(error.file_path.is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn direct_models_cannot_bypass_semantic_validation() {
    let mut scenario = example();
    scenario.schema_version = 99;
    assert!(compile_scenario(&scenario).is_err());
    scenario.schema_version = 1;
    scenario
        .layout
        .as_mut()
        .unwrap()
        .positions
        .get_mut("start")
        .unwrap()
        .x = f64::NAN;
    assert!(has(
        &validate_scenario(&scenario).unwrap_err(),
        "invalidPosition"
    ));
}
