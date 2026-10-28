//! The single boundary for sanitized snapshots and HTTP observations.
use crate::{redaction::Redactor, secrets::ResolvedCredentials, EventKind, SecretProvider};
use interl_core::Diagnostic;
use interl_http::{HttpResponse, PreparedRequest, Url};
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) struct RunSecurity {
    credentials: ResolvedCredentials,
    redactor: Redactor,
    sensitive_http_nodes: BTreeSet<String>,
}
impl std::fmt::Debug for RunSecurity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunSecurity")
            .field("credential_references", &self.credentials.len())
            .finish_non_exhaustive()
    }
}
impl RunSecurity {
    pub(crate) fn prepare_program(
        program: &interl_core::Program,
        fields: Vec<String>,
        provider: &dyn SecretProvider,
    ) -> Result<Self, Diagnostic> {
        let credentials = ResolvedCredentials::resolve(program.auth(), provider)?;
        let mut redactor = Redactor::new(fields);
        credentials.register(&mut redactor);
        redactor.learn_program(program);
        let mut sensitive_http_nodes = BTreeSet::new();
        let conservative = !program.auth().required_secrets().is_empty()
            || program
                .reachable()
                .iter()
                .map(|i| &program.workflows()[*i])
                .any(|w| {
                    !w.auth().response_last_uses().is_empty()
                        || !w.definition().inputs.is_empty()
                        || !w.definition().outputs.is_empty()
                        || w.definition().nodes.iter().any(|n| {
                            matches!(
                                n,
                                interl_core::ProgramNode::Extract { .. }
                                    | interl_core::ProgramNode::SetSession { .. }
                            )
                        })
                });
        for (i, w) in program.workflows().iter().enumerate() {
            for (j, n) in w.definition().nodes.iter().enumerate() {
                if conservative || w.auth().sensitive_http_nodes().contains(n.id()) {
                    sensitive_http_nodes.insert(format!("wf:{i}/node:{j}"));
                }
            }
        }
        Ok(Self {
            credentials,
            redactor,
            sensitive_http_nodes,
        })
    }
    pub(crate) fn program_snapshot(&self, program: &interl_core::Program) -> Value {
        self.redactor.program_snapshot(program)
    }
    pub(crate) fn text(&self, input: &str) -> String {
        self.redactor.text(input)
    }
    pub(crate) fn learn_url(&mut self, url: &Url) {
        self.redactor.learn_url(url);
    }
    pub(crate) fn credential(&self, identity: &str, field: &str) -> Option<&str> {
        self.credentials
            .get(identity, field)
            .map(|value| value.expose())
    }
    pub(crate) fn remember_secret(&mut self, value: &str) {
        self.redactor.remember_secret(value);
    }
    pub(crate) fn request_event(
        &mut self,
        node: &str,
        request: &PreparedRequest,
        limit: usize,
    ) -> EventKind {
        self.redactor.learn_headers(request.headers());
        self.redactor.learn_url(request.url());
        let sensitive = self.sensitive_http_nodes.contains(node);
        EventKind::RequestSent {
            method: request.method().into(),
            url: self.redactor.url(request.url().as_str()),
            headers: if sensitive {
                Redactor::omitted_headers("Sensitive authentication header values omitted")
            } else {
                self.redactor.headers(request.headers())
            },
            body: if sensitive {
                Redactor::omitted_body(
                    request.body_bytes(),
                    false,
                    "Sensitive authentication payload omitted",
                )
            } else {
                self.redactor
                    .body(request.body.as_ref(), request.body_bytes(), false, limit)
            },
        }
    }
    pub(crate) fn response_event(
        &mut self,
        node: &str,
        response: &HttpResponse,
        limit: usize,
    ) -> EventKind {
        self.redactor.learn_headers(&response.headers);
        if self.sensitive_http_nodes.contains(node) {
            // Do not parse or preview the response, and do not run a future
            // extraction early. Unknown token field names are safe by omission.
            return EventKind::ResponseReceived {
                status: response.status,
                headers: Redactor::omitted_headers(
                    "Sensitive authentication header values omitted",
                ),
                body: Redactor::omitted_body(
                    response.body.len(),
                    response.truncated,
                    "Sensitive authentication payload omitted",
                ),
            };
        }
        let body: Option<Value> = if response.truncated {
            None
        } else {
            serde_json::from_slice(&response.body).ok()
        };
        if let Some(body) = &body {
            self.redactor.learn(body);
        }
        EventKind::ResponseReceived {
            status: response.status,
            headers: self.redactor.headers(&response.headers),
            body: self.redactor.body(
                body.as_ref(),
                response.body.len(),
                response.truncated,
                limit,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CancellationToken, RunReport, RunStatus, RunSummary, SecretLookupError, Trace};
    use interl_core::{
        compile_scenario, parse_scenario, serialize_scenario, HttpConfig,
        ProgramBody as RequestBody, ScenarioDocument, ScenarioFormat, WorkflowNode,
    };
    use interl_http::{HeaderMap, HttpSession};
    use serde_json::json;
    use std::ffi::OsString;

    const PASSWORD: &str = "quote\"slash\\/?secret";
    struct Provider;
    impl SecretProvider for Provider {
        fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
            match name {
                "INTERL_ALICE_USERNAME" => Ok("resolved-user-marker".into()),
                "INTERL_ALICE_PASSWORD" => Ok(PASSWORD.into()),
                _ => Err(SecretLookupError::Missing),
            }
        }
    }
    fn document() -> ScenarioDocument {
        parse_scenario(
            include_str!("../../../scenarios/examples/auth-login.interl.json"),
            ScenarioFormat::Json,
        )
        .unwrap()
    }
    fn security(doc: &ScenarioDocument) -> RunSecurity {
        RunSecurity::prepare_program(&compile_scenario(doc).unwrap(), vec![], &Provider).unwrap()
    }
    fn request(doc: &ScenarioDocument) -> PreparedRequest {
        let mut config: HttpConfig = serde_json::from_value(json!({"method":"POST","path":"/auth/login?private=hidden-query","body":{"kind":"json","value":{"opaque":PASSWORD,"username":"resolved-user-marker"}},"headers":{"x-unfamiliar":"request-header-marker"}})).unwrap();
        config
            .headers
            .insert("authorization".into(), "Bearer request-token-marker".into());
        HttpSession::new(&doc.client_profiles[0], 65536, 65536)
            .unwrap()
            .prepare(&config, &Default::default())
            .unwrap()
    }
    fn response(body: Vec<u8>, truncated: bool) -> HttpResponse {
        let mut headers = HeaderMap::new();
        headers.insert("x-unfamiliar", "fresh-header-marker".parse().unwrap());
        headers.insert(
            "set-cookie",
            "sid=fresh-cookie-marker; HttpOnly".parse().unwrap(),
        );
        HttpResponse {
            status: 200,
            headers,
            body,
            truncated,
        }
    }
    #[tokio::test]
    async fn sensitive_payloads_are_safe_before_live_delivery_and_report_retention() {
        let doc = document();
        let mut security = security(&doc);
        let request = request(&doc);
        let request_bytes = request.body_bytes();
        let raw_body = serde_json::to_vec(
            &json!({"opaqueArtifact":"fresh-token-marker","user":"resolved-user-marker"}),
        )
        .unwrap();
        let response = response(raw_body, false);
        let response_bytes = response.body.len();
        let request = security.request_event("wf:0/node:1", &request, 16384);
        let response = security.response_event("wf:0/node:1", &response, 16384);
        let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
        let mut trace = Trace::new(
            "test-run".into(),
            "user:1".into(),
            sender,
            CancellationToken::new(),
            security.program_snapshot(&compile_scenario(&doc).unwrap()),
            131072,
        );
        assert!(trace.emit(Some("login"), request, false).await);
        assert!(trace.emit(Some("login"), response, false).await);
        let live = vec![receiver.try_recv().unwrap(), receiver.try_recv().unwrap()];
        let report = RunReport {
            report_version: 3,
            graph: serde_json::json!({}),
            cleanup: crate::CleanupOutcome::default(),
            run_id: "test-run".into(),
            status: RunStatus::Passed,
            snapshot: trace.snapshot,
            summary: RunSummary::default(),
            events: trace.events,
            error: None,
            events_truncated: false,
            delivery_dropped_events: 0,
        };
        assert_eq!(
            serde_json::to_value(&live).unwrap(),
            serde_json::to_value(&report.events).unwrap()
        );
        let payload = serde_json::to_value(&live).unwrap();
        for (index, bytes) in [(0, request_bytes), (1, response_bytes)] {
            assert_eq!(payload[index]["payload"]["body"]["value"], Value::Null);
            assert_eq!(payload[index]["payload"]["body"]["omitted"], true);
            assert_eq!(payload[index]["payload"]["body"]["bytes"], bytes);
            assert!(payload[index]["payload"]["headers"]["[omitted]"].is_string());
        }
        assert_eq!(payload[1]["payload"]["status"], 200);
        let wire = serde_json::to_string(&report).unwrap();
        for marker in [
            "fresh-token-marker",
            "fresh-header-marker",
            "fresh-cookie-marker",
            "request-header-marker",
            "request-token-marker",
            "resolved-user-marker",
            "hidden-query",
            PASSWORD,
        ] {
            assert!(!wire.contains(marker), "leaked fake marker");
        }
        assert!(!format!("{security:?}").contains("resolved-user-marker"));
    }
    #[test]
    fn omission_does_not_depend_on_json_shape_or_complete_body() {
        let doc = document();
        let mut security = security(&doc);
        for (bytes, truncated) in [
            (b"not-json fresh-token-marker".to_vec(), false),
            (b"{\"opaque\":\"fresh-token-marker".to_vec(), true),
        ] {
            let raw = response(bytes, truncated);
            let event =
                serde_json::to_value(security.response_event("wf:0/node:1", &raw, 16384)).unwrap();
            assert_eq!(event["payload"]["body"]["omitted"], true);
            assert_eq!(event["payload"]["body"]["truncated"], truncated);
            assert!(!event.to_string().contains("fresh-token-marker"));
        }
    }
    #[test]
    fn registered_values_raw_encoded_and_escaped_are_masked_before_later_use() {
        let doc = parse_scenario(
            include_str!("../../../scenarios/examples/hello-api.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        let mut security = security(&doc);
        security.remember_secret(PASSWORD);
        let escaped = serde_json::to_string(PASSWORD).unwrap();
        let forms = [
            PASSWORD.to_owned(),
            interl_http::encode_component(PASSWORD),
            "quote%22slash%5c%2f%3fsecret".into(),
            escaped[1..escaped.len() - 1].into(),
        ];
        for form in forms {
            let raw = response(
                serde_json::to_vec(&json!({"echo":form,"public":"visible"})).unwrap(),
                false,
            );
            let EventKind::ResponseReceived { body, .. } =
                security.response_event("wf:0/node:1", &raw, 16384)
            else {
                panic!()
            };
            assert_eq!(body["omitted"], false);
            assert_eq!(body["value"]["echo"], "[REDACTED]");
            assert_eq!(body["value"]["public"], "visible");
        }
    }
    #[test]
    fn snapshots_and_exports_keep_references_and_never_resolved_values() {
        let doc = document();
        let original = serialize_scenario(&doc).unwrap();
        let security = security(&doc);
        let snapshot = security.program_snapshot(&compile_scenario(&doc).unwrap());
        assert_eq!(
            snapshot["identities"][0]["credentials"]["password"]["name"],
            "INTERL_ALICE_PASSWORD"
        );
        assert_eq!(
            snapshot["identities"][0]["credentials"]["username"]["source"],
            "env"
        );
        assert!(snapshot.get("layout").is_none());
        assert!(!snapshot.to_string().contains("resolved-user-marker"));
        assert!(!snapshot.to_string().contains(PASSWORD));
        assert_eq!(serialize_scenario(&doc).unwrap(), original);
        // Reference descriptors must not teach the matcher words like "env" or "identity".
        assert_eq!(
            security.text("env identity INTERL_ALICE_PASSWORD"),
            "env identity INTERL_ALICE_PASSWORD"
        );
    }
    #[test]
    fn ordinary_text_request_metadata_is_accurate_and_echoes_are_redacted() {
        let mut doc = parse_scenario(
            include_str!("../../../scenarios/examples/hello-api.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        let WorkflowNode::Http { config, .. } = &mut doc.workflows[0].nodes[1] else {
            panic!()
        };
        config.body = Some(RequestBody::Text {
            value: "é-text-secret".into(),
        });
        let mut security = security(&doc);
        let WorkflowNode::Http { config, .. } = &doc.workflows[0].nodes[1] else {
            panic!()
        };
        let request = HttpSession::new(&doc.client_profiles[0], 65536, 65536)
            .unwrap()
            .prepare(
                &serde_json::from_value(serde_json::to_value(config).unwrap()).unwrap(),
                &Default::default(),
            )
            .unwrap();
        let event =
            serde_json::to_value(security.request_event("health", &request, 16384)).unwrap();
        assert_eq!(event["payload"]["body"]["bytes"], "é-text-secret".len());
        assert_eq!(event["payload"]["body"]["omitted"], true);
        let raw = response(
            serde_json::to_vec(&json!({"echo":"é-text-secret","public":"visible"})).unwrap(),
            false,
        );
        let event = serde_json::to_value(security.response_event("health", &raw, 16384)).unwrap();
        assert_eq!(event["payload"]["body"]["value"]["echo"], "[REDACTED]");
        assert_eq!(event["payload"]["body"]["value"]["public"], "visible");
    }
    #[test]
    fn redaction_exhaustion_fails_closed_without_recursive_registration() {
        let doc = parse_scenario(
            include_str!("../../../scenarios/examples/hello-api.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        let mut security = security(&doc);
        security.remember_secret(&"Bearer ".repeat(1170));
        let snapshot = security.program_snapshot(&compile_scenario(&doc).unwrap());
        assert!(snapshot.is_string());
        let raw = response(b"{\"unknown\":\"unregistered-marker\"}".to_vec(), false);
        let event =
            serde_json::to_value(security.response_event("wf:0/node:1", &raw, 16384)).unwrap();
        assert_eq!(event["payload"]["body"]["omitted"], true);
        assert_eq!(
            event["payload"]["body"]["reason"],
            "Redaction budget exceeded"
        );
        assert!(event["payload"]["headers"]["[omitted]"].is_string());
        assert!(!event.to_string().contains("unregistered-marker"));
    }
}
