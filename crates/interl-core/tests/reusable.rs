use interl_core::*;
use serde_json::{json, Value};
const SOURCE: &str = include_str!("../../../scenarios/examples/reusable-auth.interl.json");
fn value() -> Value {
    serde_json::from_str(SOURCE).unwrap()
}
fn file(v: Value) -> ScenarioDocument {
    parse_document(&v.to_string(), ScenarioFormat::Json).unwrap()
}
fn codes(v: Value) -> Vec<String> {
    compile_document(&file(v))
        .unwrap_err()
        .into_iter()
        .map(|e| e.code.clone())
        .collect()
}
#[test]
fn v2_roundtrip_and_scoped_ids() {
    let file = parse_document(SOURCE, ScenarioFormat::Json).unwrap();
    let yaml = parse_document(
        include_str!("../../../scenarios/examples/reusable-auth.interl.yaml"),
        ScenarioFormat::Yaml,
    )
    .unwrap();
    assert_eq!(file, yaml);
    let p = compile_document(&file).unwrap();
    assert_eq!(p.workflows().len(), 3);
    assert_eq!(p.reachable().len(), 3);
    assert_eq!(p.auth().required_secrets().len(), 2);
    let out = serialize_document(&file).unwrap();
    assert_eq!(file, parse_document(&out, ScenarioFormat::Json).unwrap());
    assert_eq!(out, serialize_document(&yaml).unwrap());
    assert_eq!(file, parse_scenario(SOURCE, ScenarioFormat::Json).unwrap());
    assert_eq!(
        p.workflows()[0].auth().extractions()["extract-token"].last_use_node_id(),
        "$return"
    );
}
#[test]
fn only_v2_is_accepted_by_all_entry_points() {
    for version in [json!(1), json!(3), json!("2"), json!(2.0), Value::Null] {
        let mut v = value();
        v["schemaVersion"] = version;
        let source = v.to_string();
        for parse in [parse_document, parse_scenario] {
            let error = parse(&source, ScenarioFormat::Json).unwrap_err();
            assert_eq!(error.diagnostic.code, "unsupportedVersion");
        }
        assert!(!inspect_document(&source, ScenarioFormat::Json).valid);
    }
    let mut document = file(value());
    document.schema_version = 1;
    assert!(compile_document(&document).is_err());
    assert!(serialize_document(&document).is_err());
    assert!(serialize_scenario(&document).is_err());
}
#[test]
fn strict_shapes_duplicate_keys_future_capabilities() {
    for (path, bad) in [
        ("/groups/0/executor", json!({"kind":"rampingVus"})),
        ("/workflows/0/typo", json!(true)),
        ("/groups/0/count", json!(1)),
    ] {
        let mut v = value();
        if path.ends_with("count") {
            v["groups"][0]["count"] = bad;
        } else if path.ends_with("typo") {
            v["workflows"][0]["typo"] = bad;
        } else {
            v["groups"][0]["executor"] = bad;
        }
        assert!(parse_document(&v.to_string(), ScenarioFormat::Json).is_err());
    }
    assert_eq!(
        parse_document(
            "{\"schemaVersion\":2,\"schemaVersion\":2}",
            ScenarioFormat::Json
        )
        .unwrap_err()
        .diagnostic
        .code,
        "duplicateKey"
    );
    let mut v = value();
    v["workflows"][0]["nodes"][0] =
        json!({"id":"start","type":"exclusiveSplit","config":{"sourceNodeId":"login"}});
    assert!(codes(v).contains(&"startCount".into()));
}
#[test]
fn missing_workflow_port_type_and_forward_reference_fail() {
    let mut v = value();
    v["groups"][0]["lifecycle"]["iteration"]["workflowId"] = json!("absent");
    assert!(codes(v).contains(&"missingWorkflow".into()));
    let mut v = value();
    v["groups"][0]["lifecycle"]["iteration"]["inputs"] = json!({});
    assert!(codes(v).contains(&"missingInput".into()));
    let mut v = value();
    v["groups"][0]["lifecycle"]["iteration"]["inputs"]["token"] =
        json!({"source":"literal","value":123});
    assert!(codes(v).contains(&"portTypeMismatch".into()));
    let mut v = value();
    v["workflows"][1]["nodes"][1]["config"]["setHeaders"]["authorization"] =
        json!({"source":"callOutput","nodeId":"inspect","name":"token"});
    assert!(codes(v).contains(&"valueNotAvailable".into()));
}
#[test]
fn recursion_and_resource_bounds_are_rejected_before_execution() {
    let mut v = value();
    v["workflows"][1]["nodes"][2]["config"] =
        json!({"workflowId":"main","inputs":{"token":{"source":"input","name":"token"}}});
    assert!(codes(v).contains(&"recursiveWorkflow".into()));
    let mut v = value();
    v["groups"][0]["lifecycle"]["cleanupTimeoutMs"] = json!(5001);
    assert!(codes(v).contains(&"invalidTimeout".into()));
    let mut v = value();
    v["workflows"][1]["inputs"]["token"]["default"] = json!("x".repeat(8193));
    assert!(codes(v).contains(&"portTypeMismatch".into()));
    let mut v = value();
    v["workflows"][1]["inputs"]["token"]["default"] = json!(9007199254740992u64);
    assert!(codes(v).contains(&"portTypeMismatch".into()));
}
#[test]
fn cleanup_cannot_depend_on_failed_initialize_outputs() {
    let mut v = value();
    v["groups"][0]["lifecycle"]["cleanup"] = v["groups"][0]["lifecycle"]["iteration"].clone();
    assert!(codes(v).contains(&"valueNotAvailable".into()));
}
#[test]
fn unused_definitions_validate_but_do_not_require_environment_lookup() {
    let mut v = value();
    let mut unused = v["workflows"][0].clone();
    unused["id"] = json!("unused");
    unused["nodes"][1]["config"]["body"]["fields"]["password"] =
        json!({"source":"identity","field":"unused"});
    v["identities"][0]["credentials"]["unused"] = json!({"source":"env","name":"INTERL_UNUSED"});
    v["workflows"].as_array_mut().unwrap().push(unused);
    let p = compile_document(&file(v)).unwrap();
    assert_eq!(p.auth().required_secrets().len(), 2);
}
#[test]
fn semantic_draft_is_returned_but_validated_serialization_stays_strict() {
    let mut v = value();
    v["workflows"][0]["edges"] = json!([]);
    let result = inspect_document(&v.to_string(), ScenarioFormat::Json);
    assert!(!result.valid);
    assert!(result.document.is_some());
    assert!(serialize_document(&result.document.unwrap()).is_err());
}

#[test]
fn output_liveness_marker_cannot_collide_with_a_user_node_id() {
    let mut v = value();
    v["workflows"][0]["nodes"][3]["id"] = json!("$return");
    v["workflows"][0]["edges"][2]["to"] = json!("$return");
    let p = compile_document(&file(v)).unwrap();
    assert_ne!(
        p.workflows()[0].auth().extractions()["extract-token"].last_use_node_id(),
        "$return"
    );
}
#[test]
fn extraction_cannot_read_a_callees_private_response() {
    let mut v = value();
    v["workflows"][1]["nodes"].as_array_mut().unwrap().insert(3,json!({"id":"bad-extract","type":"extract","config":{"sourceNodeId":"inspect","expression":"response.body.accessToken","output":"token"}}));
    v["workflows"][1]["edges"] = json!([{"id":"a","from":"start","to":"apply-token"},{"id":"b","from":"apply-token","to":"inspect"},{"id":"c","from":"inspect","to":"bad-extract"},{"id":"d","from":"bad-extract","to":"end"}]);
    assert!(codes(v).contains(&"invalidSourceType".into()));
}

#[test]
fn null_defaults_and_foreign_lifecycle_locals_are_invalid() {
    let mut v = value();
    v["workflows"][1]["inputs"]["token"]["default"] = Value::Null;
    assert!(codes(v).contains(&"portTypeMismatch".into()));
    let mut v = value();
    v["groups"][0]["lifecycle"]["iteration"]["inputs"]["token"] =
        json!({"source":"input","name":"token"});
    assert!(codes(v).contains(&"invalidLifecycleReference".into()));
}
#[test]
fn depth_and_aggregate_visit_limits_are_static() {
    let mut v = value();
    v["workflows"] = json!([]);
    v["groups"][0]["lifecycle"] = json!({"iteration":{"workflowId":"w0","inputs":{}}});
    for i in 0..9 {
        let middle = if i == 8 {
            json!({"id":"action","type":"http","config":{"method":"GET","path":"/health"}})
        } else {
            json!({"id":"action","type":"call","config":{"workflowId":format!("w{}",i+1),"inputs":{}}})
        };
        v["workflows"].as_array_mut().unwrap().push(json!({"id":format!("w{i}"),"name":"Workflow","inputs":{},"outputs":{},"nodes":[{"id":"start","type":"start"},middle,{"id":"end","type":"end"}],"edges":[{"id":"a","from":"start","to":"action"},{"id":"b","from":"action","to":"end"}]}));
    }
    assert!(codes(v).contains(&"invocationLimit".into()));
}

#[test]
fn v2_display_and_identifier_limits_apply_consistently() {
    let mut v = value();
    v["name"] = json!("x".repeat(257));
    assert!(codes(v).contains(&"resourceLimit".into()));
    let mut v = value();
    v["clientProfiles"][0]["id"] = json!("p".repeat(129));
    assert!(codes(v).contains(&"invalidId".into()));
}

#[test]
fn http_display_names_roundtrip_without_changing_ids_or_connections() {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../scenarios/examples/hello-api.json")).unwrap();
    let original = file(value.clone());
    value["workflows"][0]["nodes"][1]["name"] = json!("Get inventory");
    let named = file(value.clone());
    assert_eq!(
        named.workflows[0].nodes[1].id(),
        original.workflows[0].nodes[1].id()
    );
    assert_eq!(named.workflows[0].edges, original.workflows[0].edges);
    assert!(compile_document(&named).is_ok());
    let saved = serialize_document(&named).unwrap();
    assert!(saved.contains("Get inventory"));
    assert_eq!(parse_document(&saved, ScenarioFormat::Json).unwrap(), named);
    let yaml = include_str!("../../../scenarios/examples/hello-api.interl.yaml").replace(
        "    type: http\n",
        "    type: http\n    name: Get inventory\n",
    );
    assert_eq!(parse_document(&yaml, ScenarioFormat::Yaml).unwrap(), named);
    value["workflows"][0]["nodes"][1]["name"] = json!("x".repeat(257));
    assert!(codes(value).contains(&"resourceLimit".to_string()));
}
