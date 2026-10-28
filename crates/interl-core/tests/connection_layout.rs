use interl_core::{compile_document, parse_document, serialize_draft, ScenarioFormat};
use serde_json::{json, Value};
fn value() -> Value {
    serde_json::from_str(include_str!("../../../scenarios/examples/hello-api.json")).unwrap()
}
#[test]
fn attachment_sides_roundtrip_without_affecting_execution() {
    let mut value = value();
    value["workflows"][0]["layout"]["connections"] =
        json!({"edge-0":{"source":"top","target":"bottom"}});
    let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    let program = compile_document(&doc).unwrap();
    assert_eq!(program.workflows()[0].order().len(), 3);
    let output = serialize_draft(&doc).unwrap();
    let reopened = parse_document(&output, ScenarioFormat::Json).unwrap();
    assert_eq!(doc, reopened);
}
#[test]
fn invalid_sides_and_missing_edges_are_rejected() {
    let mut value = value();
    value["workflows"][0]["layout"]["connections"] =
        json!({"edge-0":{"source":"center","target":"bottom"}});
    assert!(parse_document(&value.to_string(), ScenarioFormat::Json).is_err());
    value["workflows"][0]["layout"]["connections"] =
        json!({"missing":{"source":"top","target":"bottom"}});
    let doc = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    assert!(compile_document(&doc)
        .unwrap_err()
        .iter()
        .any(|error| error.code == "unknownLayoutEdge"));
}
