use interl_core::*;
use serde_json::{json, Value};

const JSON: &str = include_str!("../../../scenarios/examples/auth-login.interl.json");
const YAML: &str = include_str!("../../../scenarios/examples/auth-login.interl.yaml");

fn value() -> Value {
    serde_json::from_str(JSON).unwrap()
}
fn document(value: &Value) -> ScenarioDocument {
    parse_scenario(&value.to_string(), ScenarioFormat::Json).unwrap()
}
fn errors(value: &Value) -> Vec<Diagnostic> {
    validate_scenario(&document(value)).unwrap_err()
}
fn has(value: &Value, code: &str) -> bool {
    errors(value).iter().any(|error| error.code == code)
}
fn changed(path: &str, replacement: Value) -> Value {
    let mut value = value();
    *value.pointer_mut(path).unwrap() = replacement;
    value
}

#[test]
fn plans_are_format_array_order_and_layout_independent() {
    let original = document(&value());
    let plan = compile_scenario(&original).unwrap();
    let mut yaml = parse_scenario(YAML, ScenarioFormat::Yaml).unwrap();
    yaml.workflow.nodes.reverse();
    yaml.workflow.edges.reverse();
    yaml.layout = None;
    assert_eq!(plan, compile_scenario(&yaml).unwrap());
    assert_eq!(
        plan.steps()
            .iter()
            .map(WorkflowNode::id)
            .collect::<Vec<_>>(),
        [
            "start",
            "login",
            "extract-token",
            "apply-token",
            "authenticated",
            "end"
        ]
    );
    let auth = plan.auth();
    assert_eq!(
        auth.required_secrets()
            .iter()
            .map(|secret| (secret.identity_id(), secret.field()))
            .collect::<Vec<_>>(),
        [("alice", "password"), ("alice", "username")]
    );
    assert_eq!(
        auth.required_secrets()[0].reference(),
        &SecretReference::Env {
            name: "INTERL_ALICE_PASSWORD".into()
        }
    );
    assert_eq!(
        auth.sensitive_http_nodes()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["authenticated", "login"]
    );
    assert_eq!(auth.response_last_uses()["login"], "extract-token");
    let extraction = &auth.extractions()["extract-token"];
    assert_eq!(extraction.source_node_id(), "login");
    assert_eq!(extraction.output(), "token");
    assert_eq!(extraction.expression().body_path(), ["accessToken"]);
    assert_eq!(extraction.last_use_node_id(), "apply-token");
}

#[test]
fn identities_names_and_environment_references_are_checked_without_lookup() {
    let mut duplicate = value();
    let identity = duplicate["identities"][0].clone();
    duplicate["identities"]
        .as_array_mut()
        .unwrap()
        .push(identity);
    assert!(has(&duplicate, "duplicateId"));
    for (path, replacement, code) in [
        ("/identities/0/id", json!(" "), "emptyId"),
        (
            "/identities/0/id",
            json!("x".repeat(129)),
            "invalidAuthName",
        ),
        (
            "/groups/0/identityAssignment/identityId",
            json!("missing"),
            "missingIdentity",
        ),
        (
            "/groups/0/identityAssignment/identityId",
            Value::Null,
            "missingAssignedIdentity",
        ),
        (
            "/identities/0/credentials/password/name",
            json!("MISSING_BUT_WELL_FORMED_VARIABLE_123"),
            "",
        ),
    ] {
        let v = changed(path, replacement);
        if code.is_empty() {
            validate_scenario(&document(&v)).unwrap();
        } else {
            assert!(has(&v, code), "{path}: {code}");
        }
    }
    for name in [
        "",
        "1PASSWORD",
        "PASSWORD-NAME",
        "PASSWORD NAME",
        "é",
        &"A".repeat(129),
    ] {
        assert!(has(
            &changed("/identities/0/credentials/password/name", json!(name)),
            "invalidSecretReference"
        ));
    }
    let mut missing = value();
    missing["identities"][0]["credentials"]
        .as_object_mut()
        .unwrap()
        .remove("password");
    assert!(has(&missing, "missingCredentialField"));
    let mut empty_field = value();
    empty_field["identities"][0]["credentials"][""] = json!({"source":"env","name":"VALID"});
    assert!(has(&empty_field, "invalidAuthName"));
}

#[test]
fn unused_identity_fields_are_not_runtime_dependencies() {
    let mut v = value();
    v["identities"][0]["credentials"]["unused"] =
        json!({"source":"env","name":"UNUSED_CREDENTIAL"});
    v["identities"].as_array_mut().unwrap().push(json!({"id":"unselected","credentials":{"token":{"source":"env","name":"UNSELECTED_TOKEN"}}}));
    let plan = compile_scenario(&document(&v)).unwrap();
    assert_eq!(plan.auth().required_secrets().len(), 2);
    let wire = serde_json::to_string(plan.auth()).unwrap();
    assert!(!wire.contains("UNUSED_CREDENTIAL"));
    assert!(!wire.contains("UNSELECTED_TOKEN"));
    // One identity used by multiple declarations still produces one reference per field.
    let group = v["groups"][0].clone();
    v["groups"].as_array_mut().unwrap().push(group);
    v["groups"][1]["id"] = json!("other");
    assert_eq!(
        compile_scenario(&document(&v))
            .unwrap()
            .auth()
            .required_secrets()
            .len(),
        2
    );
}

#[test]
fn every_group_assignment_must_supply_the_consumed_fields() {
    let mut v = value();
    v["identities"].as_array_mut().unwrap().push(
        json!({"id":"other","credentials":{"username":{"source":"env","name":"OTHER_USERNAME"}}}),
    );
    let group = v["groups"][0].clone();
    v["groups"].as_array_mut().unwrap().push(group);
    v["groups"][1]["id"] = json!("other");
    v["groups"][1]["identityAssignment"]["identityId"] = json!("other");
    let errors = errors(&v);
    assert!(errors.iter().any(|e| e.code == "missingCredentialField"
        && e.field_path == "/groups/1/identityAssignment/identityId"));
}

#[test]
fn extraction_sources_must_be_earlier_http_nodes() {
    for (source, code) in [
        ("absent", "missingSourceNode"),
        ("extract-token", "forwardReference"),
        ("authenticated", "forwardReference"),
        ("start", "invalidSourceType"),
    ] {
        let v = changed("/workflow/nodes/2/config/sourceNodeId", json!(source));
        assert!(has(&v, code), "{source}");
        assert!(compile_scenario(&document(&v)).is_err());
    }
}

#[test]
fn output_references_require_earlier_extraction_and_matching_name() {
    for (source, name, code) in [
        ("absent", "token", "missingSourceNode"),
        ("login", "token", "invalidSourceType"),
        ("apply-token", "token", "forwardReference"),
        ("authenticated", "token", "forwardReference"),
        ("extract-token", "wrong", "missingOutput"),
    ] {
        let mut v = value();
        v["workflow"]["nodes"][3]["config"]["setHeaders"]["authorization"] =
            json!({"source":"extraction","nodeId":source,"name":name});
        assert!(has(&v, code), "{source}/{name}");
    }
    let mut v = value();
    v["workflow"]["nodes"][1]["config"]["body"]["fields"]["password"] =
        json!({"source":"extraction","nodeId":"extract-token","name":"token"});
    assert!(has(&v, "forwardReference"));
    assert!(has(
        &changed("/workflow/nodes/2/config/output", json!("")),
        "invalidAuthName"
    ));
}

#[test]
fn session_operations_validate_names_conflicts_and_literal_values() {
    let path = "/workflow/nodes/3/config";
    for (config, code) in [
        (json!({}), "emptySessionUpdate"),
        (
            json!({"removeHeaders":["Authorization","authorization"]}),
            "duplicateHeaderOperation",
        ),
        (
            json!({"setHeaders":{"Authorization":{"source":"literal","value":"a"},"authorization":{"source":"literal","value":"b"}}}),
            "duplicateHeaderOperation",
        ),
        (
            json!({"setHeaders":{"Authorization":{"source":"literal","value":"a"}},"removeHeaders":["authorization"]}),
            "conflictingHeaderOperation",
        ),
        (json!({"removeHeaders":["bad name"]}), "invalidHeader"),
        (
            json!({"setHeaders":{"x-test":{"source":"literal","value":"a\r\nb"}}}),
            "invalidHeaderValue",
        ),
        (
            json!({"setHeaders":{"x-test":{"source":"literal","value":"a\u{0000}b"}}}),
            "invalidHeaderValue",
        ),
        (
            json!({"setHeaders":{"authorization":{"source":"extraction","nodeId":"extract-token","name":"token","prefix":"Bearer\n"}}}),
            "invalidHeaderValue",
        ),
        (
            json!({"setHeaders":{"x-test":{"source":"literal","value":"a".repeat(8193)}}}),
            "invalidHeaderValue",
        ),
    ] {
        assert!(has(&changed(path, config), code), "{code}");
    }
    validate_scenario(&document(&changed(
        path,
        json!({"removeHeaders":["Authorization"]}),
    )))
    .unwrap();
    validate_scenario(&document(&changed(
        path,
        json!({"setHeaders":{"x-test":{"source":"literal","value":"a".repeat(8192)}}}),
    )))
    .unwrap();
}

#[test]
fn static_identity_headers_share_the_same_dependency_checks() {
    let v = changed(
        "/workflow/nodes/3/config/setHeaders/authorization",
        json!({"source":"identity","field":"password","prefix":"Bearer "}),
    );
    validate_scenario(&document(&v)).unwrap();
    let v = changed(
        "/workflow/nodes/3/config/setHeaders/authorization",
        json!({"source":"identity","field":"missing"}),
    );
    assert!(has(&v, "missingCredentialField"));
}

#[test]
fn feel_subset_is_bounded_and_has_separate_expression_locations() {
    for expression in [
        "response.body.token + 1",
        "response.body.items[1]",
        "response.body.token()",
        "for x in [1..999999] return x",
        "identity.password",
        "response.headers.authorization",
        "response.body.true",
        "response.body.é",
        "response.body.\u{2003}token",
        "response.body.a-b",
    ] {
        assert!(
            has(
                &changed("/workflow/nodes/2/config/expression", json!(expression)),
                "unsupportedExpression"
            ),
            "{expression}"
        );
    }
    for expression in ["", "response.body.", "response..token"] {
        assert!(has(
            &changed("/workflow/nodes/2/config/expression", json!(expression)),
            "invalidExpression"
        ));
    }
    let v = changed(
        "/workflow/nodes/2/config/expression",
        json!("response.body.\n token()"),
    );
    let errors = errors(&v);
    let error = errors
        .iter()
        .find(|e| e.code == "unsupportedExpression")
        .unwrap();
    assert_eq!(error.field_path, "/workflow/nodes/2/config/expression");
    assert_eq!(error.node_id.as_deref(), Some("extract-token"));
    assert_eq!(
        (error.expression_line, error.expression_column),
        (Some(2), Some(7))
    );
    assert!(error.line.is_none() && error.column.is_none());
    assert!(!error.message.contains("token()"));
    assert!(error.to_string().contains("expression 2:7"));
}

#[test]
fn feel_source_segment_and_depth_limits_are_checked_before_parser_entry() {
    for expression in [
        "x".repeat(4097),
        format!("response.body.{}", "a".repeat(129)),
        format!("response.body.{}", vec!["a"; 31].join(".")),
    ] {
        let v = changed("/workflow/nodes/2/config/expression", json!(expression));
        let errors = errors(&v);
        assert!(errors
            .iter()
            .any(|e| e.code == "expressionLimit" && e.category == ErrorCategory::ResourceLimit));
    }
    let expression = format!("response.body.{}", vec!["a"; 30].join("."));
    validate_scenario(&document(&changed(
        "/workflow/nodes/2/config/expression",
        json!(expression),
    )))
    .unwrap();
    validate_scenario(&document(&changed(
        "/workflow/nodes/2/config/expression",
        json!(" response . body . nested . access_token \n"),
    )))
    .unwrap();
}

#[test]
fn template_node_budget_and_direct_model_depth_cannot_be_bypassed() {
    let mut doc = document(&value());
    let WorkflowNode::Http { config, .. } = &mut doc.workflow.nodes[1] else {
        panic!()
    };
    let Some(RequestBody::JsonTemplate { fields }) = &mut config.body else {
        panic!()
    };
    fields.clear();
    for i in 0..255 {
        fields.insert(i.to_string(), TemplateValue::Literal { value: Value::Null });
    }
    validate_scenario(&doc).unwrap();
    let WorkflowNode::Http { config, .. } = &mut doc.workflow.nodes[1] else {
        panic!()
    };
    let Some(RequestBody::JsonTemplate { fields }) = &mut config.body else {
        panic!()
    };
    fields.insert(
        "overflow".into(),
        TemplateValue::Literal { value: Value::Null },
    );
    assert!(validate_scenario(&doc)
        .unwrap_err()
        .iter()
        .any(|e| e.code == "templateLimit"));
    for (levels, valid) in [(30, true), (31, false)] {
        let mut nested = TemplateValue::Literal { value: Value::Null };
        for _ in 0..levels {
            nested = TemplateValue::Array {
                items: vec![nested],
            };
        }
        let WorkflowNode::Http { config, .. } = &mut doc.workflow.nodes[1] else {
            panic!()
        };
        config.body = Some(RequestBody::JsonTemplate {
            fields: [("nested".into(), nested)].into(),
        });
        assert_eq!(validate_scenario(&doc).is_ok(), valid);
    }
}

#[test]
fn liveness_follows_connectivity_and_nested_template_consumers() {
    let mut v = value();
    v["workflow"]["nodes"][4]["config"]["body"] = json!({"kind":"jsonTemplate","fields":{"nested":{"source":"object","fields":{"items":{"source":"array","items":[{"source":"extraction","nodeId":"extract-token","name":"token"}]}}}}});
    // Read the same login response twice; output names are scoped by node ID.
    v["workflow"]["nodes"].as_array_mut().unwrap().push(json!({"id":"extract-again","type":"extract","config":{"sourceNodeId":"login","expression":"response.body.otherToken","output":"token"}}));
    v["workflow"]["edges"] = json!([{"from":"start","to":"login"},{"from":"login","to":"extract-token"},{"from":"extract-token","to":"extract-again"},{"from":"extract-again","to":"apply-token"},{"from":"apply-token","to":"authenticated"},{"from":"authenticated","to":"end"}]);
    let plan = compile_scenario(&document(&v)).unwrap();
    assert_eq!(plan.auth().response_last_uses()["login"], "extract-again");
    assert_eq!(
        plan.auth().extractions()["extract-token"].last_use_node_id(),
        "authenticated"
    );
    assert_eq!(
        plan.auth().extractions()["extract-again"].last_use_node_id(),
        "extract-again"
    );
    let mut doc = document(&v);
    doc.workflow.nodes.reverse();
    doc.workflow.edges.reverse();
    doc.layout = None;
    assert_eq!(plan, compile_scenario(&doc).unwrap());
}

#[test]
fn literal_objects_do_not_create_secret_dependencies() {
    let mut v = value();
    v["identities"] = json!([]);
    v["groups"][0]["identityAssignment"]["identityId"] = Value::Null;
    v["workflow"]["nodes"][1]["config"]["body"] = json!({"kind":"jsonTemplate","fields":{"payload":{"source":"literal","value":{"source":"identity","field":"password"}}}});
    let plan = compile_scenario(&document(&v)).unwrap();
    assert!(plan.auth().required_secrets().is_empty());
}

#[test]
fn invalid_auth_graphs_never_reach_compilation() {
    for edges in [
        json!([{"from":"start","to":"login"},{"from":"login","to":"end"}]),
        json!([{"from":"start","to":"login"},{"from":"login","to":"extract-token"},{"from":"extract-token","to":"login"}]),
    ] {
        let doc = document(&changed("/workflow/edges", edges));
        assert!(validate_scenario(&doc).is_err());
        assert!(compile_scenario(&doc).is_err());
    }
    let mut v = value();
    v["workflow"]["nodes"][3]["id"] = json!("extract-token");
    assert!(has(&v, "duplicateId"));
}

#[test]
fn equivalent_formats_have_equal_safe_expression_diagnostics() {
    let json = JSON.replace("response.body.accessToken", "response.body.token()");
    let yaml = YAML.replace("response.body.accessToken", "response.body.token()");
    let left = inspect_scenario(&json, ScenarioFormat::Json);
    let right = inspect_scenario(&yaml, ScenarioFormat::Yaml);
    assert_eq!(left.diagnostics, right.diagnostics);
    assert!(!left.valid && !right.valid);
    assert!(left.diagnostics.len() <= 100);
}

#[test]
fn capture_marks_extraction_sources_ahead_of_time_and_preserves_safe_prelude() {
    let mut v = value();
    v["identities"] = json!([]);
    v["groups"][0]["identityAssignment"]["identityId"] = Value::Null;
    v["workflow"]["nodes"][1]["config"]["body"] = json!({"kind":"json","value":{}});
    v["workflow"]["nodes"][3]["config"] = json!({"removeHeaders":["authorization"]});
    v["workflow"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"prelude","type":"http","config":{"method":"GET","path":"/health"}}));
    v["workflow"]["edges"][0] = json!({"from":"start","to":"prelude"});
    v["workflow"]["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from":"prelude","to":"login"}));
    let plan = compile_scenario(&document(&v)).unwrap();
    assert!(plan.auth().required_secrets().is_empty());
    assert!(!plan.auth().sensitive_http_nodes().contains("prelude"));
    assert!(plan.auth().sensitive_http_nodes().contains("login"));
    assert!(plan.auth().sensitive_http_nodes().contains("authenticated"));
}

#[test]
fn diagnostics_are_capped_and_never_echo_invalid_header_values() {
    let v = changed(
        "/workflow/nodes/3/config/setHeaders/authorization",
        json!({"source":"literal","value":"sensitive-marker\r\n"}),
    );
    assert!(!serde_json::to_string(&errors(&v))
        .unwrap()
        .contains("sensitive-marker"));
    let removals: Vec<_> = (0..150).map(|i| format!("bad header {i}")).collect();
    let v = changed(
        "/workflow/nodes/3/config",
        json!({"removeHeaders":removals}),
    );
    assert_eq!(errors(&v).len(), 100);
    assert!(compile_scenario(&document(&v)).is_err());
}
