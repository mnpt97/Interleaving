use interl_core::{compile_document, parse_document, ScenarioFormat};
use serde_json::{json, Value};
fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../interl-engine/tests/fixtures/editor-auth-native.json"
    ))
    .unwrap()
}
fn compile(value: &Value) -> Result<interl_core::Program, Vec<interl_core::Diagnostic>> {
    compile_document(&parse_document(&value.to_string(), ScenarioFormat::Json).unwrap())
}
#[test]
fn credentials_are_node_scoped_and_only_reachable_refs_are_resolved() {
    let mut value = fixture();
    let mut unused = value["workflows"][0].clone();
    unused["id"] = json!("unused");
    unused["nodes"][1]["config"]["username"]["name"] = json!("UNUSED_USERNAME");
    value["workflows"].as_array_mut().unwrap().push(unused);
    let program = compile(&value).unwrap();
    assert_eq!(program.auth().required_secrets().len(), 2);
    assert!(program.document().identities.is_empty());
    assert_eq!(
        program.workflows()[0].definition().nodes[1].kind(),
        "authentication"
    );
}
#[test]
fn invalid_auth_fields_and_base_urls_fail_before_runtime() {
    for (field, bad) in [
        ("baseUrl", json!("https://user:secret@example.com/api")),
        ("baseUrl", json!("https://example.com/api?q=x")),
        ("path", json!("//other-host/login")),
        ("method", json!("GET")),
        ("usernameKey", json!("password")),
        (
            "tokenExpression",
            json!("response.body.accessToken + secret"),
        ),
        ("headerName", json!("bad\r\nheader")),
        ("headerPrefix", json!("Bearer\n")),
    ] {
        let mut value = fixture();
        value["workflows"][0]["nodes"][1]["config"][field] = bad;
        assert!(compile(&value).is_err(), "accepted invalid {field}");
    }
    for bad in [
        "relative",
        "ftp://example.com",
        "https://example.com/#fragment",
    ] {
        let mut value = fixture();
        value["clientProfiles"][0]["baseUrl"] = json!(bad);
        assert!(compile(&value).is_err());
    }
}
#[test]
fn independent_nodes_roundtrip_without_identity_references() {
    let mut value = fixture();
    let mut next = value["workflows"][0]["nodes"][1].clone();
    next["id"] = json!("second");
    next["config"]["username"]["name"] = json!("DEVICE_USER");
    next["config"]["password"]["name"] = json!("DEVICE_PASSWORD");
    value["workflows"][0]["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(2, next);
    value["workflows"][0]["edges"][1]["to"] = json!("second");
    value["workflows"][0]["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"last","from":"second","to":"end"}));
    let program = compile(&value).unwrap();
    let refs = program.auth().required_secrets();
    assert_eq!(refs.len(), 4);
    assert_ne!(refs[0].identity_id(), refs[2].identity_id());
    let saved = interl_core::serialize_document(program.document()).unwrap();
    let reopened = parse_document(&saved, ScenarioFormat::Json).unwrap();
    assert_eq!(&reopened, program.document());
}
