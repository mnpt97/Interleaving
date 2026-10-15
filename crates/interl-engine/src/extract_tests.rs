use super::*;
use interl_core::{
    compile_scenario, parse_scenario, ExtractConfig, ScenarioDocument, ScenarioFormat,
    SessionHeaderValue, WorkflowEdge, WorkflowNode,
};
use interl_http::{HeaderMap, HttpSession, ResolvedBody};
use serde_json::{json, Value};
#[path = "../../../tests/support/mod.rs"]
mod support;

fn document() -> ScenarioDocument {
    parse_scenario(
        include_str!("../../../scenarios/examples/auth-login.interl.json"),
        ScenarioFormat::Json,
    )
    .unwrap()
}
fn plan(expression: &str) -> AuthPlan {
    let mut doc = document();
    let WorkflowNode::Extract { config, .. } = &mut doc.workflow.nodes[2] else {
        panic!()
    };
    config.expression = expression.into();
    compile_scenario(&doc).unwrap().auth().clone()
}
fn state(expression: &str) -> ExtractionState {
    ExtractionState::new(&plan(expression), MAX_RESPONSE_BYTES).unwrap()
}
fn response(body: Vec<u8>, truncated: bool) -> HttpResponse {
    HttpResponse {
        status: 200,
        headers: HeaderMap::new(),
        body,
        truncated,
    }
}
fn put(state: &mut ExtractionState, body: Vec<u8>) {
    state
        .retain_response("login", response(body, false), &CancellationToken::new())
        .unwrap();
}
fn padded(size: usize) -> Vec<u8> {
    let mut bytes = b"{\"accessToken\":\"token\",\"padding\":\"".to_vec();
    bytes.resize(size - 2, b'x');
    bytes.extend_from_slice(b"\"}");
    assert_eq!(bytes.len(), size);
    bytes
}
fn many_document(count: usize) -> ScenarioDocument {
    let mut doc = document();
    let end = doc.workflow.nodes.last().unwrap().clone();
    let mut apply = doc.workflow.nodes[3].clone();
    let WorkflowNode::SetSession { config, .. } = &mut apply else {
        panic!()
    };
    config.set_headers.clear();
    doc.workflow.nodes.truncate(2);
    for index in 0..count {
        let id = format!("extract-{index}");
        doc.workflow.nodes.push(WorkflowNode::Extract {
            id: id.clone(),
            config: ExtractConfig {
                source_node_id: "login".into(),
                expression: "response.body.accessToken".into(),
                output: "token".into(),
            },
        });
        config.set_headers.insert(
            format!("x-token-{index}"),
            SessionHeaderValue::Extraction {
                node_id: id,
                name: "token".into(),
                prefix: String::new(),
            },
        );
    }
    doc.workflow.nodes.extend([apply, end]);
    doc.workflow.edges = doc
        .workflow
        .nodes
        .windows(2)
        .map(|nodes| WorkflowEdge {
            from: nodes[0].id().into(),
            to: nodes[1].id().into(),
        })
        .collect();
    doc.layout = None;
    doc
}

#[test]
fn formats_compile_to_the_same_read_only_nested_lookup() {
    let mut documents = [
        document(),
        parse_scenario(
            include_str!("../../../scenarios/examples/auth-login.interl.yaml"),
            ScenarioFormat::Yaml,
        )
        .unwrap(),
    ];
    let bytes =
        br#"{"nested":{"token":"env(SECRET) ${untouched}"},"ignored":"private-marker"}"#.to_vec();
    for doc in &mut documents {
        let WorkflowNode::Extract { config, .. } = &mut doc.workflow.nodes[2] else {
            panic!()
        };
        config.expression = " response . body . nested . token \n".into();
        let compiled = compile_scenario(doc).unwrap();
        let mut state = ExtractionState::new(compiled.auth(), MAX_RESPONSE_BYTES).unwrap();
        put(&mut state, bytes.clone());
        assert_eq!(
            state
                .extract("extract-token", &CancellationToken::new())
                .unwrap()
                .expose(),
            "env(SECRET) ${untouched}"
        );
        assert_eq!(state.responses["login"].bytes, bytes);
        assert_eq!(
            state.output("extract-token", "token").unwrap().expose(),
            "env(SECRET) ${untouched}"
        );
        assert_eq!(
            state.output("extract-token", "wrong-name").unwrap_err(),
            ExtractionError::UnavailableOutput
        );
        assert_eq!(
            state
                .extract("unknown-node", &CancellationToken::new())
                .unwrap_err(),
            ExtractionError::UnknownExtraction
        );
    }
}

#[test]
fn unsupported_feel_cannot_become_a_runtime_program() {
    for source in [
        "response.headers.authorization",
        "credentials.password",
        "env(SECRET)",
        "response.body.items[1]",
        "response.body.token + 1",
        "for x in response.body.items return x",
        "response.body.token()",
    ] {
        let mut doc = document();
        let WorkflowNode::Extract { config, .. } = &mut doc.workflow.nodes[2] else {
            panic!()
        };
        config.expression = source.into();
        assert!(compile_scenario(&doc).is_err());
    }
}

#[test]
fn missing_null_wrong_type_empty_and_oversized_values_are_distinct() {
    for (body, expected) in [
        (json!({}), ExtractionError::MissingProperty),
        (json!({"accessToken":null}), ExtractionError::NullValue),
        (Value::Null, ExtractionError::NullValue),
        (json!({"accessToken":42}), ExtractionError::WrongType),
        (json!({"accessToken":true}), ExtractionError::WrongType),
        (
            json!({"accessToken":["private-marker"]}),
            ExtractionError::WrongType,
        ),
        (
            json!([{"accessToken":"private-marker"}]),
            ExtractionError::WrongType,
        ),
        (json!({"accessToken":{}}), ExtractionError::WrongType),
        (json!({"accessToken":""}), ExtractionError::EmptyValue),
        (
            json!({"accessToken":"é".repeat(4097)}),
            ExtractionError::ValueLimit,
        ),
    ] {
        let mut state = state("response.body.accessToken");
        put(&mut state, serde_json::to_vec(&body).unwrap());
        let error = state
            .extract("extract-token", &CancellationToken::new())
            .unwrap_err();
        assert_eq!(error, expected);
        assert!(!format!("{error:?} {error} {}", error.code()).contains("private-marker"));
        assert!(state.outputs.is_empty());
    }
    let mut state = state("response.body.nested.token");
    put(&mut state, br#"{"nested":null}"#.to_vec());
    assert_eq!(
        state
            .extract("extract-token", &CancellationToken::new())
            .unwrap_err(),
        ExtractionError::NullValue
    );
}

#[test]
fn utf8_string_boundary_and_debug_are_safe() {
    let mut state = state("response.body.accessToken");
    let value = "é".repeat(4096);
    put(
        &mut state,
        serde_json::to_vec(&json!({"accessToken":value})).unwrap(),
    );
    let extracted = state
        .extract("extract-token", &CancellationToken::new())
        .unwrap();
    assert_eq!(extracted.expose().len(), MAX_VALUE_BYTES);
    assert_eq!(format!("{extracted:?}"), "ExtractedValue([REDACTED])");
    assert!(!format!("{state:?}").contains('é'));
    let mut nested = json!({"token":"nested-token"});
    for _ in 0..29 {
        nested = json!({"nested":nested});
    }
    let expression = format!("response.body.{}token", "nested.".repeat(29));
    let mut deep = ExtractionState::new(&plan(&expression), MAX_RESPONSE_BYTES).unwrap();
    put(&mut deep, serde_json::to_vec(&nested).unwrap());
    assert_eq!(
        deep.extract("extract-token", &CancellationToken::new())
            .unwrap()
            .expose(),
        "nested-token"
    );
}

#[test]
fn invalid_and_duplicate_json_never_retain_secret_parser_context() {
    for (bytes, expected) in [
        (
            b"private-marker not-json".to_vec(),
            ExtractionError::InvalidJson,
        ),
        (
            br#"{"accessToken":"private-marker"} {}"#.to_vec(),
            ExtractionError::InvalidJson,
        ),
        (
            br#"{"accessToken":"private-marker""#.to_vec(),
            ExtractionError::InvalidJson,
        ),
        (b"\xffprivate-marker".to_vec(), ExtractionError::InvalidJson),
        (
            b"{\"accessToken\":1e999}".to_vec(),
            ExtractionError::InvalidJson,
        ),
        (
            br#"{"accessToken":"private-marker","\u0061ccessToken":"other"}"#.to_vec(),
            ExtractionError::DuplicateJsonKey,
        ),
        (
            br#"{"accessToken":"token","other":[{"private-marker":0,"private-marker":1}]}"#
                .to_vec(),
            ExtractionError::DuplicateJsonKey,
        ),
    ] {
        let mut state = state("response.body.accessToken");
        put(&mut state, bytes);
        let error = state
            .extract("extract-token", &CancellationToken::new())
            .unwrap_err();
        assert_eq!(error, expected);
        assert!(!format!("{error:?} {error} {} {state:?}", error.code()).contains("private-marker"));
    }
}

#[test]
fn json_depth_and_value_budgets_include_unselected_data() {
    let cancel = CancellationToken::new();
    let nested = |depth: usize| format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
    assert!(json::parse(nested(64).as_bytes(), &cancel).is_ok());
    assert_eq!(
        json::parse(nested(65).as_bytes(), &cancel).unwrap_err(),
        ExtractionError::JsonDepthLimit
    );
    let array = |count: usize| format!("[{}]", vec!["null"; count].join(","));
    assert!(json::parse(array(MAX_JSON_VALUES - 1).as_bytes(), &cancel).is_ok());
    assert_eq!(
        json::parse(array(MAX_JSON_VALUES).as_bytes(), &cancel).unwrap_err(),
        ExtractionError::JsonValueLimit
    );
    let mut state = state("response.body.accessToken");
    let bytes = format!("{{\"accessToken\":\"token\",\"ignored\":{}}}", nested(64));
    put(&mut state, bytes.into_bytes());
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap_err(),
        ExtractionError::JsonDepthLimit
    );
}

#[test]
fn unavailable_and_truncated_responses_have_distinct_errors() {
    let mut state = state("response.body.accessToken");
    let cancel = CancellationToken::new();
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap_err(),
        ExtractionError::UnavailableResponse
    );
    state
        .retain_response(
            "login",
            response(br#"{"accessToken":"private-marker"}"#.to_vec(), true),
            &cancel,
        )
        .unwrap();
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap_err(),
        ExtractionError::TruncatedResponse
    );
    put(&mut state, vec![]);
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap_err(),
        ExtractionError::InvalidJson
    );
}

#[test]
fn response_byte_limits_and_unplanned_response_discard_are_enforced() {
    let plan = plan("response.body.accessToken");
    assert!(ExtractionState::new(&plan, 0).is_err());
    assert!(ExtractionState::new(&plan, MAX_RESPONSE_BYTES + 1).is_err());
    let mut state = ExtractionState::new(&plan, 128).unwrap();
    let cancel = CancellationToken::new();
    state
        .retain_response("unplanned", response(vec![b'x'; 129], false), &cancel)
        .unwrap();
    assert!(state.responses.is_empty());
    put(&mut state, padded(128));
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap().expose(),
        "token"
    );
    assert_eq!(
        state
            .retain_response("login", response(padded(129), false), &cancel)
            .unwrap_err(),
        ExtractionError::ResponseLimit
    );
    assert_eq!(state.response_bytes, 0);
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap_err(),
        ExtractionError::UnavailableResponse
    );
    assert!(state.outputs.is_empty());
}

#[test]
fn aggregate_response_admission_and_last_use_reclaim_bytes() {
    let mut doc = many_document(2);
    let mut other = doc.workflow.nodes[1].clone();
    let WorkflowNode::Http { id, .. } = &mut other else {
        panic!()
    };
    *id = "other-login".into();
    doc.workflow.nodes.insert(2, other);
    let WorkflowNode::Extract { config, .. } = &mut doc.workflow.nodes[4] else {
        panic!()
    };
    config.source_node_id = "other-login".into();
    doc.workflow.edges = doc
        .workflow
        .nodes
        .windows(2)
        .map(|nodes| WorkflowEdge {
            from: nodes[0].id().into(),
            to: nodes[1].id().into(),
        })
        .collect();
    let plan = compile_scenario(&doc).unwrap();
    let mut state = ExtractionState::new(plan.auth(), MAX_RESPONSE_BYTES).unwrap();
    let cancel = CancellationToken::new();
    put(&mut state, padded(MAX_RESPONSE_BYTES / 2));
    state
        .retain_response(
            "other-login",
            response(padded(MAX_RESPONSE_BYTES / 2), false),
            &cancel,
        )
        .unwrap();
    assert_eq!(state.response_bytes, MAX_RESPONSE_BYTES);
    assert_eq!(
        state
            .retain_response(
                "other-login",
                response(padded(MAX_RESPONSE_BYTES / 2 + 1), false),
                &cancel
            )
            .unwrap_err(),
        ExtractionError::ResponseLimit
    );
    assert_eq!(state.response_bytes, MAX_RESPONSE_BYTES / 2);
    state.extract("extract-0", &cancel).unwrap();
    state.release_after("extract-0");
    assert_eq!(state.response_bytes, 0);
    state
        .retain_response(
            "other-login",
            response(padded(MAX_RESPONSE_BYTES), false),
            &cancel,
        )
        .unwrap();
    assert_eq!(state.response_bytes, MAX_RESPONSE_BYTES);
    state.extract("extract-1", &cancel).unwrap();
    state.release_after("extract-1");
    assert_eq!(state.response_bytes, 0);
}

#[test]
fn output_entry_limit_and_compiled_last_uses_reclaim_storage() {
    let doc = many_document(MAX_OUTPUTS + 1);
    let plan = compile_scenario(&doc).unwrap();
    let mut state = ExtractionState::new(plan.auth(), MAX_RESPONSE_BYTES).unwrap();
    put(&mut state, br#"{"accessToken":"t"}"#.to_vec());
    let cancel = CancellationToken::new();
    for index in 0..MAX_OUTPUTS {
        state.extract(&format!("extract-{index}"), &cancel).unwrap();
    }
    assert_eq!(state.outputs.len(), MAX_OUTPUTS);
    assert_eq!(
        state.extract("extract-128", &cancel).unwrap_err(),
        ExtractionError::OutputLimit
    );
    assert_eq!(state.outputs.len(), MAX_OUTPUTS);
    state.release_after("start");
    assert_eq!(state.outputs.len(), MAX_OUTPUTS);
    state.release_after("apply-token");
    assert_eq!(state.output_bytes, 0);
    assert!(state.outputs.is_empty());
}

#[test]
fn aggregate_output_bytes_and_replacement_accounting_are_bounded() {
    let doc = many_document(9);
    let plan = compile_scenario(&doc).unwrap();
    let mut state = ExtractionState::new(plan.auth(), MAX_RESPONSE_BYTES).unwrap();
    put(
        &mut state,
        serde_json::to_vec(&json!({"accessToken":"s".repeat(MAX_VALUE_BYTES)})).unwrap(),
    );
    let cancel = CancellationToken::new();
    for index in 0..8 {
        state.extract(&format!("extract-{index}"), &cancel).unwrap();
    }
    assert_eq!(state.output_bytes, MAX_OUTPUT_BYTES);
    assert_eq!(
        state.extract("extract-8", &cancel).unwrap_err(),
        ExtractionError::OutputLimit
    );
    put(&mut state, br#"{"accessToken":"t"}"#.to_vec());
    state.extract("extract-0", &cancel).unwrap();
    assert_eq!(state.output_bytes, MAX_OUTPUT_BYTES - MAX_VALUE_BYTES + 1);
    state.extract("extract-8", &cancel).unwrap();
    state.release_after("extract-8");
    assert_eq!(state.response_bytes, 0);
    assert_eq!(state.outputs.len(), 9);
    state.release_after("apply-token");
    assert_eq!(state.output_bytes, 0);
}

#[test]
fn failed_or_cancelled_reextraction_cannot_reuse_stale_outputs() {
    let mut state = state("response.body.accessToken");
    let cancel = CancellationToken::new();
    put(
        &mut state,
        br#"{"accessToken":"old-private-marker"}"#.to_vec(),
    );
    state.extract("extract-token", &cancel).unwrap();
    put(&mut state, br#"{"accessToken":null}"#.to_vec());
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap_err(),
        ExtractionError::NullValue
    );
    assert_eq!(
        state.output("extract-token", "token").unwrap_err(),
        ExtractionError::UnavailableOutput
    );
    put(
        &mut state,
        br#"{"accessToken":"new-private-marker"}"#.to_vec(),
    );
    state.extract("extract-token", &cancel).unwrap();
    cancel.cancel();
    assert_eq!(
        state.extract("extract-token", &cancel).unwrap_err(),
        ExtractionError::Cancelled
    );
    assert!(state.outputs.is_empty());
    assert_eq!(
        state
            .retain_response("login", response(vec![], false), &cancel)
            .unwrap_err(),
        ExtractionError::Cancelled
    );
    assert!(state.responses.is_empty());
    assert_eq!(
        json::parse(b"{}", &cancel).unwrap_err(),
        ExtractionError::Cancelled
    );
}

#[test]
fn unused_outputs_are_released_at_the_extraction_step() {
    let mut doc = document();
    let WorkflowNode::SetSession { config, .. } = &mut doc.workflow.nodes[3] else {
        panic!()
    };
    config.set_headers.clear();
    config.remove_headers.push("authorization".into());
    let plan = compile_scenario(&doc).unwrap();
    let mut state = ExtractionState::new(plan.auth(), MAX_RESPONSE_BYTES).unwrap();
    put(&mut state, br#"{"accessToken":"token"}"#.to_vec());
    state
        .extract("extract-token", &CancellationToken::new())
        .unwrap();
    state.release_after("extract-token");
    assert!(state.outputs.is_empty());
    assert!(state.responses.is_empty());
}

#[tokio::test]
async fn controlled_login_variants_feed_the_extractor_without_running_workflows() {
    let server = support::TestServer::start().await;
    let doc = support::scenario(&server.base_url, "/health");
    let session = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
    for (path, expression, expected) in [
        ("/auth/login", "response.body.accessToken", None),
        (
            "/auth/login-missing-token",
            "response.body.accessToken",
            Some(ExtractionError::MissingProperty),
        ),
        (
            "/auth/login-malformed",
            "response.body.accessToken",
            Some(ExtractionError::InvalidJson),
        ),
        (
            "/auth/login-text",
            "response.body.accessToken",
            Some(ExtractionError::InvalidJson),
        ),
        (
            "/auth/login-unfamiliar",
            "response.body.opaqueArtifact",
            None,
        ),
        (
            "/auth/login-large",
            "response.body.accessToken",
            Some(ExtractionError::TruncatedResponse),
        ),
    ] {
        let config = serde_json::from_value(json!({"method":"POST","path":path})).unwrap();
        let cancel = CancellationToken::new();
        let response = session
            .execute(
                session
                    .prepare_resolved(
                        &config,
                        &Default::default(),
                        ResolvedBody::Json(json!({"username":"alice","password":"alice-password"})),
                    )
                    .unwrap(),
                &cancel,
            )
            .await
            .unwrap();
        assert_eq!(response.status, 200);
        let mut state = state(expression);
        state.retain_response("login", response, &cancel).unwrap();
        match expected {
            Some(error) => assert_eq!(state.extract("extract-token", &cancel).unwrap_err(), error),
            None => assert!(state
                .extract("extract-token", &cancel)
                .unwrap()
                .expose()
                .starts_with("fake-token-")),
        }
    }
}
