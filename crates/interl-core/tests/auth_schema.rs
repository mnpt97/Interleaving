use interl_core::*;
use serde_json::{json, Value};

const JSON: &str = include_str!("../../../scenarios/examples/auth-login.interl.json");
const YAML: &str = include_str!("../../../scenarios/examples/auth-login.interl.yaml");

fn document() -> ScenarioDocument {
    parse_scenario(JSON, ScenarioFormat::Json).unwrap()
}

fn value() -> Value {
    serde_json::from_str(JSON).unwrap()
}

#[test]
fn auth_formats_have_equal_typed_models_and_deterministic_round_trips() {
    let json = document();
    let yaml = parse_scenario(YAML, ScenarioFormat::Yaml).unwrap();
    assert_eq!(json, yaml);
    assert_eq!(
        json.identities[0].credentials["password"],
        SecretReference::Env {
            name: "INTERL_ALICE_PASSWORD".into()
        }
    );
    let serialized = serde_json::to_string_pretty(&json).unwrap();
    assert_eq!(serialized, serde_json::to_string_pretty(&yaml).unwrap());
    assert_eq!(
        json,
        parse_scenario(&serialized, ScenarioFormat::Json).unwrap()
    );
    assert_eq!(
        json.workflow
            .nodes
            .iter()
            .map(WorkflowNode::kind)
            .collect::<Vec<_>>(),
        ["start", "http", "extract", "setSession", "http", "end"]
    );
    assert_eq!(json.workflow.nodes[2].id(), "extract-token");
    assert_eq!(json.workflow.nodes[3].id(), "apply-token");
}

#[test]
fn identity_fields_require_explicit_environment_references() {
    for bad in [
        json!("inline-password"),
        json!(null),
        json!({"source":"literal","value":"inline-password"}),
        json!({"source":"keychain","name":"password"}),
        json!({"source":"env"}),
        json!({"source":"env","name":42}),
        json!({"source":"env","name":"PASSWORD","value":"inline-password"}),
    ] {
        let mut v = value();
        v["identities"][0]["credentials"]["password"] = bad;
        let error = parse_scenario(&v.to_string(), ScenarioFormat::Json)
            .unwrap_err()
            .diagnostic;
        assert_eq!(error.code, "invalidSchema");
        assert!(error
            .field_path
            .starts_with("/identities/0/credentials/password"));
        assert!(!error.message.contains("inline-password"));
    }
    let yaml = YAML.replace(
        "{ source: env, name: INTERL_ALICE_PASSWORD }",
        "inline-password",
    );
    assert_eq!(
        parse_scenario(&yaml, ScenarioFormat::Yaml)
            .unwrap_err()
            .diagnostic
            .code,
        "invalidSchema"
    );
}

#[test]
fn unknown_auth_fields_and_future_modes_are_rejected() {
    for path in [
        "/identities/0",
        "/identities/0/credentials/password",
        "/workflow/nodes/1/config/body",
        "/workflow/nodes/1/config/body/fields/password",
        "/workflow/nodes/2",
        "/workflow/nodes/2/config",
        "/workflow/nodes/3/config",
        "/workflow/nodes/3/config/setHeaders/authorization",
    ] {
        let mut v = value();
        v.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("typo".into(), json!(true));
        assert_eq!(
            parse_scenario(&v.to_string(), ScenarioFormat::Json)
                .unwrap_err()
                .diagnostic
                .code,
            "invalidSchema",
            "{path}"
        );
    }
    for mode in ["pool", "unique"] {
        let mut v = value();
        v["groups"][0]["identityAssignment"]["mode"] = json!(mode);
        assert!(parse_scenario(&v.to_string(), ScenarioFormat::Json).is_err());
    }
    let mut v = value();
    v["assertions"] = json!([{"expression":"true"}]);
    assert_eq!(
        parse_scenario(&v.to_string(), ScenarioFormat::Json)
            .unwrap_err()
            .diagnostic
            .code,
        "unsupportedFeature"
    );
}

#[test]
fn explicit_templates_compose_objects_arrays_and_output_references() {
    let mut v = value();
    let fields = json!({
        "identity": {"source":"identity","field":"username"},
        "output": {"source":"extraction","nodeId":"extract-token","name":"token"},
        "nested": {"source":"object","fields":{"list":{"source":"array","items":[{"source":"literal","value":false},{"source":"literal","value":null},{"source":"literal","value":3}]}}},
        "literalObject": {"source":"literal","value":{"source":"identity","field":"password","text":"${password}"}}
    });
    v["workflow"]["nodes"][1]["config"]["body"] = json!({"kind":"jsonTemplate","fields":fields});
    let parsed = parse_scenario(&v.to_string(), ScenarioFormat::Json).unwrap();
    let WorkflowNode::Http { config, .. } = &parsed.workflow.nodes[1] else {
        panic!()
    };
    let Some(RequestBody::JsonTemplate { fields }) = &config.body else {
        panic!()
    };
    assert!(matches!(&fields["nested"], TemplateValue::Object { .. }));
    assert!(
        matches!(&fields["output"], TemplateValue::Extraction { node_id, name } if node_id == "extract-token" && name == "token")
    );
    assert!(
        matches!(&fields["literalObject"], TemplateValue::Literal { value } if value["text"] == "${password}")
    );
    assert_eq!(
        serde_json::to_value(&parsed).unwrap()["workflow"]["nodes"][1]["config"]["body"],
        v["workflow"]["nodes"][1]["config"]["body"]
    );
    // This is a shape test; the forward output reference is deliberately not
    // claimed valid. Phase 2 owns ordering and dataflow checks.
}

#[test]
fn malformed_templates_and_structured_header_sources_are_rejected() {
    for bad in [
        json!({"source":"env","name":"PASSWORD"}),
        json!({"source":"literal"}),
        json!({"source":"identity","field":true}),
        json!({"source":"extraction","node_id":"extract-token","name":"token"}),
        json!({"source":"object","fields":[]}),
        json!({"source":"array","items":{}}),
    ] {
        let mut v = value();
        v["workflow"]["nodes"][1]["config"]["body"]["fields"]["password"] = bad;
        assert!(parse_scenario(&v.to_string(), ScenarioFormat::Json).is_err());
    }
    for bad in [
        json!({"source":"literal","value":123}),
        json!({"source":"literal","value":{"token":"value"}}),
        json!({"source":"array","items":[]}),
        json!({"source":"object","fields":{}}),
        json!({"source":"identity","field":"token","prefix":false}),
    ] {
        let mut v = value();
        v["workflow"]["nodes"][3]["config"]["setHeaders"]["authorization"] = bad;
        assert!(parse_scenario(&v.to_string(), ScenarioFormat::Json).is_err());
    }
}

#[test]
fn session_header_sources_and_operation_defaults_are_typed() {
    let config: SetSessionConfig = serde_json::from_value(json!({"setHeaders":{
        "x-mode":{"source":"literal","value":"test"},
        "authorization":{"source":"identity","field":"token","prefix":"Bearer "},
        "x-token":{"source":"extraction","nodeId":"extract-token","name":"token"}
    }}))
    .unwrap();
    assert!(config.remove_headers.is_empty());
    assert!(
        matches!(&config.set_headers["x-mode"], SessionHeaderValue::Literal { prefix, .. } if prefix.is_empty())
    );
    assert!(
        matches!(&config.set_headers["authorization"], SessionHeaderValue::Identity { prefix, .. } if prefix == "Bearer ")
    );
    assert!(
        matches!(&config.set_headers["x-token"], SessionHeaderValue::Extraction { prefix, .. } if prefix.is_empty())
    );
    let remove: SetSessionConfig =
        serde_json::from_value(json!({"removeHeaders":["Authorization"]})).unwrap();
    assert!(remove.set_headers.is_empty());
    assert_eq!(remove.remove_headers, ["Authorization"]);
}

#[test]
fn extraction_strings_are_preserved_before_semantic_validation() {
    let expression = "  response.body.accessToken + unsupported()\n";
    let mut v = value();
    v["workflow"]["nodes"][2]["config"]["expression"] = json!(expression);
    let parsed = parse_scenario(&v.to_string(), ScenarioFormat::Json).unwrap();
    let WorkflowNode::Extract { config, .. } = &parsed.workflow.nodes[2] else {
        panic!()
    };
    assert_eq!(config.expression, expression);
    let yaml = YAML.replace(
        "expression: response.body.accessToken",
        "expression: \"  response.body.accessToken + unsupported()\\n\"",
    );
    assert_eq!(parsed, parse_scenario(&yaml, ScenarioFormat::Yaml).unwrap());
}

#[test]
fn previous_literal_bodies_are_never_reinterpreted_as_references() {
    for body in [
        json!({"kind":"json","value":{"source":"identity","field":"password","text":"${password}"}}),
        json!({"kind":"text","value":"${password}"}),
    ] {
        let mut v = value();
        v["workflow"]["nodes"][1]["config"]["body"] = body.clone();
        let parsed = parse_scenario(&v.to_string(), ScenarioFormat::Json).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap()["workflow"]["nodes"][1]["config"]["body"],
            body
        );
    }
}

#[test]
fn auth_duplicate_keys_and_parser_resource_limits_still_apply() {
    let source = JSON.replace(
        "\"source\": \"env\", \"name\": \"INTERL_ALICE_PASSWORD\"",
        "\"source\": \"env\", \"name\": \"FIRST\", \"name\": \"SECOND\"",
    );
    assert_eq!(
        parse_scenario(&source, ScenarioFormat::Json)
            .unwrap_err()
            .diagnostic
            .code,
        "duplicateKey"
    );
    let source = YAML.replace(
        "source: env, name: INTERL_ALICE_PASSWORD",
        "source: env, name: FIRST, name: SECOND",
    );
    assert_eq!(
        parse_scenario(&source, ScenarioFormat::Yaml)
            .unwrap_err()
            .diagnostic
            .code,
        "duplicateKey"
    );
    for (source, format) in [(JSON, ScenarioFormat::Json), (YAML, ScenarioFormat::Yaml)] {
        let error = parse_scenario_with_limits(
            source,
            format,
            ScenarioLimits {
                max_depth: 5,
                ..ScenarioLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.diagnostic.category, ErrorCategory::ResourceLimit);
    }
}

#[test]
fn typed_auth_models_now_validate_compile_and_serialize() {
    let parsed = document();
    validate_scenario(&parsed).unwrap();
    let plan = compile_scenario(&parsed).unwrap();
    assert_eq!(plan.steps().len(), 6);
    let serialized = serialize_scenario(&parsed).unwrap();
    assert_eq!(
        parsed,
        parse_scenario(&serialized, ScenarioFormat::Json).unwrap()
    );
    assert_eq!(
        serialized,
        serialize_scenario(&parse_scenario(YAML, ScenarioFormat::Yaml).unwrap()).unwrap()
    );
}

#[test]
fn cookie_examples_compile_equally_and_keep_only_credential_references() {
    let json = parse_scenario(
        include_str!("../../../scenarios/examples/auth-cookie.interl.json"),
        ScenarioFormat::Json,
    )
    .unwrap();
    let yaml = parse_scenario(
        include_str!("../../../scenarios/examples/auth-cookie.interl.yaml"),
        ScenarioFormat::Yaml,
    )
    .unwrap();
    assert_eq!(json, yaml);
    let plan = compile_scenario(&json).unwrap();
    assert_eq!(plan, compile_scenario(&yaml).unwrap());
    assert_eq!(
        plan.steps()
            .iter()
            .map(WorkflowNode::kind)
            .collect::<Vec<_>>(),
        ["start", "http", "http", "end"]
    );
    let serialized = serialize_scenario(&json).unwrap();
    assert!(serialized.contains("INTERL_ALICE_PASSWORD"));
    assert!(!serialized.contains("alice-password"));
    assert_eq!(
        json,
        parse_scenario(&serialized, ScenarioFormat::Json).unwrap()
    );
}
