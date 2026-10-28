use interl_core::{compile_document, parse_document, ScenarioFormat};
use serde_json::{json, Value};

fn document() -> Value {
    serde_json::from_str(include_str!(
        "../../../scenarios/examples/feature-showcase.interl.json"
    ))
    .unwrap()
}

fn extraction_config(value: &mut Value) -> &mut Value {
    let workflow = value["workflows"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|workflow| workflow["id"] == "inspect")
        .unwrap();
    &mut workflow["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|node| node["id"] == "extract-health")
        .unwrap()["config"]
}

#[test]
fn showcase_compiles_one_extraction_with_three_named_typed_outputs() {
    let doc = parse_document(&document().to_string(), ScenarioFormat::Json).unwrap();
    let program = compile_document(&doc).unwrap();
    let extraction = program
        .workflows()
        .iter()
        .find_map(|workflow| workflow.auth().extractions().get("extract-health"))
        .unwrap();
    assert_eq!(extraction.outputs().len(), 3);
}

#[test]
fn rejects_mixed_single_and_multiple_output_shapes() {
    let mut value = document();
    extraction_config(&mut value)["expression"] = json!("response.body.value.status");
    assert!(parse_document(&value.to_string(), ScenarioFormat::Json).is_err());
}

#[test]
fn expression_errors_identify_the_named_output_and_its_workflow() {
    let mut value = document();
    extraction_config(&mut value)["outputs"]["ready"]["expression"] =
        json!("response.body.value.status + 1");
    let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    let errors = compile_document(&doc).unwrap_err();
    assert!(errors.iter().any(|error| error
        .field_path
        .ends_with("/config/outputs/ready/expression")
        && error.workflow_id.as_deref() == Some("inspect")
        && error.expression_line == Some(1)));
}

#[test]
fn rejects_empty_or_excessive_output_sets() {
    for count in [0, 33] {
        let mut value = document();
        extraction_config(&mut value)["outputs"] = Value::Object(
            (0..count)
                .map(|index| {
                    (
                        format!("field{index}"),
                        json!({"expression":"response.body.value.label"}),
                    )
                })
                .collect(),
        );
        let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
        assert!(compile_document(&doc).is_err());
    }
}

#[test]
fn grouped_outputs_remain_live_until_the_latest_consumer_regardless_of_name_order() {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../scenarios/examples/auth-login.interl.json"
    ))
    .unwrap();
    let workflow = &mut value["workflows"][0];
    workflow["nodes"][2]["config"] = json!({"sourceNodeId":"login", "outputs": {
        "a": {"expression":"response.body.accessToken"},
        "z": {"expression":"response.body.accessToken"}
    }});
    workflow["nodes"][3]["config"]["setHeaders"]["authorization"]["name"] = json!("z");
    workflow["nodes"].as_array_mut().unwrap().insert(
        4,
        json!({
            "id":"late-consumer", "type":"setSession", "config":{"setHeaders": {
                "x-later":{"source":"extraction","nodeId":"extract-token","name":"a"}
            }}
        }),
    );
    workflow["edges"] = Value::Array(workflow["nodes"].as_array().unwrap().windows(2).enumerate().map(|(index,nodes)| json!({"id":format!("e{index}"),"from":nodes[0]["id"],"to":nodes[1]["id"]})).collect());
    workflow.as_object_mut().unwrap().remove("layout");
    let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    let program = compile_document(&doc).unwrap();
    assert_eq!(
        program.workflows()[0].auth().extractions()["extract-token"].last_use_node_id(),
        "late-consumer"
    );
}
