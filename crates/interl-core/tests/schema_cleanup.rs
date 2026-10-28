use interl_core::{
    compile_document, parse_document, serialize_document, ScenarioDocument, ScenarioFormat,
};
use serde_json::{json, Value};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../interl-engine/tests/fixtures/editor-auth-native.json"
    ))
    .unwrap()
}
#[test]
fn generated_contract_matches_the_authoritative_rust_model() {
    let generated: Value =
        serde_json::from_str(include_str!("../../../schemas/scenario.schema.json")).unwrap();
    let actual = serde_json::to_value(schemars::schema_for!(ScenarioDocument)).unwrap();
    fn difference(a: &Value, b: &Value, path: String) -> Option<String> {
        // Node's JSON parser normalizes integral schema bounds such as 0.0 to 0.
        if a == b || (a.is_number() && b.is_number() && a.as_f64() == b.as_f64()) {
            return None;
        }
        if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
            for key in a.keys().chain(b.keys()) {
                let (Some(a), Some(b)) = (a.get(key), b.get(key)) else {
                    return Some(format!("{path}/{key}: missing field"));
                };
                if let Some(diff) = difference(a, b, format!("{path}/{key}")) {
                    return Some(diff);
                }
            }
            return None;
        }
        if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
            if a.len() == b.len() {
                for (index, (a, b)) in a.iter().zip(b).enumerate() {
                    if let Some(diff) = difference(a, b, format!("{path}/{index}")) {
                        return Some(diff);
                    }
                }
                return None;
            }
        }
        Some(format!("{path}: {a} != {b}"))
    }
    let difference = difference(&actual, &generated, String::new());
    assert!(difference.is_none(), "{}", difference.unwrap_or_default());
}
#[test]
fn serialization_omits_defaults_and_empty_placeholders() {
    let input = include_str!("../../../scenarios/examples/hello-api.json");
    let document = parse_document(input, ScenarioFormat::Json).unwrap();
    let output: Value = serde_json::from_str(&serialize_document(&document).unwrap()).unwrap();
    for name in ["assertions", "identities", "environment", "baseUrl"] {
        assert!(output.get(name).is_none(), "{name}");
    }
    let group = &output["groups"][0];
    assert!(group.get("executor").is_none());
    assert!(group.get("identityAssignment").is_none());
    assert!(group["lifecycle"].get("initialize").is_none());
    assert_eq!(
        parse_document(&output.to_string(), ScenarioFormat::Json).unwrap(),
        document
    );
}
#[test]
fn cookie_authentication_excludes_bearer_fields_and_names_are_optional() {
    let mut value = fixture();
    let node = &mut value["workflows"][0]["nodes"][1];
    node.as_object_mut().unwrap().remove("name");
    node["config"]["mode"] = json!("cookie");
    for field in ["tokenExpression", "headerName", "headerPrefix"] {
        node["config"].as_object_mut().unwrap().remove(field);
    }
    let document = parse_document(&value.to_string(), ScenarioFormat::Json).unwrap();
    assert!(compile_document(&document).is_ok());
    value["workflows"][0]["nodes"][1]["config"]["tokenExpression"] = json!("response.body.token");
    assert!(parse_document(&value.to_string(), ScenarioFormat::Json).is_err());
}
