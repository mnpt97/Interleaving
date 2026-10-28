use interl_core::{compile_document, parse_document, ScenarioFormat};
use serde_json::{json, Value};
fn source() -> Value {
    serde_json::from_str(include_str!(
        "../../../scenarios/examples/exclusive-routing.interl.json"
    ))
    .unwrap()
}
fn check(v: Value) -> Result<interl_core::Program, Vec<interl_core::Diagnostic>> {
    compile_document(&parse_document(&v.to_string(), ScenarioFormat::Json).unwrap())
}
#[test]
fn structured_routes_compile_independently_of_array_order() {
    let mut v = source();
    v["workflows"][0]["nodes"].as_array_mut().unwrap().reverse();
    v["workflows"][0]["edges"].as_array_mut().unwrap().reverse();
    let program = check(v).unwrap();
    assert_eq!(program.workflows()[0].order().len(), 7);
}
#[test]
fn invalid_routes_have_scoped_static_diagnostics() {
    for (pointer, value, code) in [
        (
            "/workflows/0/nodes/2/config/sourceNodeId",
            json!("accepted"),
            "routeSourceUnavailable",
        ),
        (
            "/workflows/0/edges/3/default",
            json!(false),
            "invalidRouteEdge",
        ),
        (
            "/workflows/0/edges/0/default",
            json!(true),
            "invalidRouteEdge",
        ),
        (
            "/workflows/0/edges/4/to",
            json!("fallback"),
            "nonLinearGraph",
        ),
        (
            "/workflows/0/edges/2/condition/expression",
            json!("response.body.password() = 200"),
            "invalidRouteCondition",
        ),
    ] {
        let mut v = source();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        v.pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.into(), value);
        let errors = check(v).unwrap_err();
        assert!(errors.iter().any(|e| e.code == code), "{code}: {errors:?}");
        assert!(errors
            .iter()
            .all(|e| e.workflow_id.as_deref() == Some("main")));
    }
}
#[test]
fn branch_local_outputs_cannot_escape_their_arm() {
    let mut v = source();
    v["workflows"][0]["outputs"] = json!({"status":{"type":"integer","value":{"source":"response","nodeId":"accepted","field":"status"}}});
    assert!(check(v)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "valueNotAvailable"));
}
#[test]
fn common_source_outputs_remain_available_after_merge() {
    let mut v = source();
    v["workflows"][0]["outputs"] = json!({"status":{"type":"integer","value":{"source":"response","nodeId":"health","field":"status"}}});
    assert!(check(v).is_ok());
}
#[test]
fn empty_default_arm_and_nested_regions_are_supported() {
    let mut v = source();
    let w = &mut v["workflows"][0];
    w["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["id"] != "fallback");
    w["edges"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["id"] != "fallback-join");
    w["edges"][3]["to"] = json!("joined");
    w["layout"]["positions"]
        .as_object_mut()
        .unwrap()
        .remove("fallback");
    assert!(check(v.clone()).is_ok());
    let w = &mut v["workflows"][0];
    w["nodes"].as_array_mut().unwrap().extend([
        json!({"id":"inner","type":"exclusiveSplit","config":{"sourceNodeId":"health"}}),
        json!({"id":"inner-join","type":"exclusiveMerge","config":{}}),
    ]);
    w["edges"][4]["to"] = json!("inner");
    w["edges"].as_array_mut().unwrap().extend([json!({"id":"inner-yes","from":"inner","to":"inner-join","condition":{"expression":"true"}}),json!({"id":"inner-no","from":"inner","to":"inner-join","default":true}),json!({"id":"inner-out","from":"inner-join","to":"joined"})]);
    remove_pair_hints(&mut v);
    reverse_graph_arrays(&mut v);
    assert!(check(v).is_ok());
}

#[test]
fn predicates_have_a_document_wide_retained_storage_limit() {
    let mut v = source();
    let template = v["workflows"][0].clone();
    for index in 1..32 {
        let mut w = template.clone();
        w["id"] = json!(format!("extra-{index}"));
        v["workflows"].as_array_mut().unwrap().push(w);
    }
    for w in v["workflows"].as_array_mut().unwrap() {
        w["edges"][2]["condition"]["expression"] = json!(format!("{}true", " ".repeat(4092)));
        w["edges"][3].as_object_mut().unwrap().remove("default");
        w["edges"][3]["condition"] = json!({"expression":format!("{}false"," ".repeat(4091))});
    }
    assert!(check(v)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "expressionLimit"));
}
#[test]
fn duplicate_defaults_and_crossing_regions_are_rejected() {
    let mut v = source();
    v["workflows"][0]["edges"][2]
        .as_object_mut()
        .unwrap()
        .remove("condition");
    v["workflows"][0]["edges"][2]["default"] = json!(true);
    assert!(check(v)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "multipleDefaultRoutes"));
    let mut v = source();
    v["workflows"][0]["edges"][4]["to"] = json!("result");
    assert!(check(v)
        .unwrap_err()
        .iter()
        .any(|e| matches!(e.code.as_str(), "cycle" | "nonLinearGraph")));
}

#[test]
fn acyclic_crossing_gateway_regions_are_not_structured_branches() {
    let mut v = source();
    let w = &mut v["workflows"][0];
    w["nodes"].as_array_mut().unwrap().extend([
        json!({"id":"inner","type":"exclusiveSplit","config":{"sourceNodeId":"health"}}),
        json!({"id":"inner-join","type":"exclusiveMerge","config":{}}),
    ]);
    w["edges"][4]["to"] = json!("inner");
    w["edges"][6]["to"] = json!("inner-join");
    w["edges"].as_array_mut().unwrap().extend([
        json!({"id":"inside","from":"inner","to":"joined","condition":{"expression":"true"}}),
        json!({"id":"escape","from":"inner","to":"inner-join","default":true}),
        json!({"id":"last","from":"inner-join","to":"end"}),
    ]);
    assert!(check(v)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "unstructuredBranch"));
}

/// Exercise imported drafts without the now-optional legacy pairing fields.
fn remove_pair_hints(value: &mut Value) {
    for workflow in value["workflows"].as_array_mut().unwrap() {
        for node in workflow["nodes"].as_array_mut().unwrap() {
            if let Some(config) = node.get_mut("config").and_then(Value::as_object_mut) {
                config.remove("mergeNodeId");
                config.remove("splitNodeId");
            }
        }
    }
}
fn reverse_graph_arrays(value: &mut Value) {
    value["workflows"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .reverse();
    value["workflows"][0]["edges"]
        .as_array_mut()
        .unwrap()
        .reverse();
}
#[test]
fn pairing_is_inferred_without_hints_in_json_and_yaml() {
    let mut value = source();
    remove_pair_hints(&mut value);
    reverse_graph_arrays(&mut value);
    assert!(check(value).is_ok());
    let yaml = include_str!("../../../scenarios/examples/exclusive-routing.interl.yaml")
        .replace("          \"mergeNodeId\": \"joined\"\n", "")
        .replace(
            "        \"config\":\n          \"splitNodeId\": \"result\"",
            "        \"config\": {}",
        );
    let document = parse_document(&yaml, ScenarioFormat::Yaml).unwrap();
    assert!(compile_document(&document).is_ok());
}
#[test]
fn removed_pairing_fields_are_rejected_by_the_file_schema() {
    let mut value = source();
    for node in value["workflows"][0]["nodes"].as_array_mut().unwrap() {
        if node["type"] == "exclusiveSplit" {
            node["config"]["mergeNodeId"] = json!("missing");
        }
        if node["type"] == "exclusiveMerge" {
            node["config"]["splitNodeId"] = json!("missing");
        }
    }
    assert!(parse_document(&value.to_string(), ScenarioFormat::Json).is_err());
}
#[test]
fn branches_without_a_merge_report_wiring_instead_of_pairing_fields() {
    let mut value = source();
    let workflow = &mut value["workflows"][0];
    // Bypassing the join produces a graph diagnostic, not a request to fill
    // in a pairing field that is no longer part of the authoring UI.
    workflow["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|node| node["id"] != "joined");
    for edge in workflow["edges"].as_array_mut().unwrap() {
        if edge["to"] == "joined" {
            edge["to"] = json!("end");
        }
    }
    workflow["edges"]
        .as_array_mut()
        .unwrap()
        .retain(|edge| edge["from"] != "joined");
    workflow["layout"]["positions"]
        .as_object_mut()
        .unwrap()
        .remove("joined");
    remove_pair_hints(&mut value);
    let errors = check(value).unwrap_err();
    assert!(errors
        .iter()
        .all(|error| !error.field_path.contains("mergeNodeId")
            && !error.field_path.contains("splitNodeId")));
}

#[test]
fn unicode_expression_limit_offsets_produce_diagnostics_without_panicking() {
    let mut value = source();
    value["workflows"][0]["edges"][2]["condition"]["expression"] =
        json!(format!("response.body.label = \"{}\"", "é".repeat(3000)));
    assert!(check(value)
        .unwrap_err()
        .iter()
        .any(|error| error.code == "expressionLimit"));
}
#[test]
fn bound_route_inputs_are_statically_typed_and_must_be_declared() {
    for (inputs, expression, code) in [
        (json!({}), "inputs.ready = true", "missingRouteInput"),
        (
            json!({"ready":{"source":"literal","value":"true"}}),
            "inputs.ready = true",
            "invalidRouteCondition",
        ),
    ] {
        let mut value = source();
        value["workflows"][0]["nodes"][2]["config"]["inputs"] = inputs;
        value["workflows"][0]["edges"][2]["condition"]["expression"] = json!(expression);
        assert!(check(value)
            .unwrap_err()
            .iter()
            .any(|error| error.code == code));
    }
}
