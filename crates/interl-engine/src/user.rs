//! One user's owned mutable runtime state; immutable workflow order stays in core.
use crate::{
    extract::{ExtractionError, ExtractionState},
    security::RunSecurity,
    CancellationToken, RunFailure,
};
use interl_core::{
    AuthPlan, HttpConfig, IdentityAssignment, RequestBody, ScenarioDocument, SessionHeaderValue,
    SetSessionConfig, TemplateValue,
};
use interl_http::{
    HttpResponse, HttpSession, PreparedRequest, ResolvedBody, MAX_SESSION_HEADER_BYTES,
};
use serde_json::Value;
use std::collections::BTreeMap;

impl From<ExtractionError> for RunFailure {
    fn from(error: ExtractionError) -> Self {
        Self {
            kind: error.code().into(),
            message: error.to_string(),
        }
    }
}
pub(crate) struct UserContext {
    pub(crate) security: RunSecurity,
    pub(crate) session: HttpSession,
    extraction: ExtractionState,
    identity: Option<String>,
}
impl UserContext {
    pub(crate) fn new(
        doc: &ScenarioDocument,
        plan: &AuthPlan,
        session: HttpSession,
        security: RunSecurity,
        response_limit: usize,
    ) -> Result<Self, RunFailure> {
        let IdentityAssignment::Shared { identity_id } = &doc.groups[0].identity_assignment;
        let identity = identity_id.clone();
        Ok(Self {
            session,
            security,
            identity,
            extraction: ExtractionState::new(plan, response_limit)?,
        })
    }
    fn identity_value(&self, field: &str) -> Result<&str, RunFailure> {
        self.identity
            .as_deref()
            .and_then(|id| self.security.credential(id, field))
            .ok_or_else(|| {
                failure(
                    "identityCredentialUnavailable",
                    "Required identity credential is unavailable.",
                )
            })
    }
    pub(crate) fn prepare_http(
        &self,
        config: &HttpConfig,
        variables: &BTreeMap<String, Value>,
        cancel: &CancellationToken,
    ) -> Result<PreparedRequest, RunFailure> {
        check_cancel(cancel)?;
        let request = match &config.body {
            Some(RequestBody::JsonTemplate { fields }) => {
                let mut nodes = 1;
                let body = fields
                    .iter()
                    .map(|(key, value)| {
                        Ok((key.clone(), self.template(value, 2, &mut nodes, cancel)?))
                    })
                    .collect::<Result<serde_json::Map<_, _>, RunFailure>>()?;
                self.session.prepare_resolved(
                    config,
                    variables,
                    ResolvedBody::Json(Value::Object(body)),
                )?
            }
            _ => self.session.prepare(config, variables)?,
        };
        check_cancel(cancel)?;
        Ok(request)
    }
    fn template(
        &self,
        value: &TemplateValue,
        depth: usize,
        nodes: &mut usize,
        cancel: &CancellationToken,
    ) -> Result<Value, RunFailure> {
        check_cancel(cancel)?;
        *nodes += 1;
        if depth > 32 || *nodes > 256 {
            return Err(failure(
                "templateLimit",
                "Template composition exceeds runtime limits.",
            ));
        }
        Ok(match value {
            TemplateValue::Literal { value } => value.clone(),
            TemplateValue::Identity { field } => Value::String(self.identity_value(field)?.into()),
            TemplateValue::Extraction { node_id, name } => {
                Value::String(self.extraction.output(node_id, name)?.expose().into())
            }
            TemplateValue::Object { fields } => Value::Object(
                fields
                    .iter()
                    .map(|(key, value)| {
                        Ok((key.clone(), self.template(value, depth + 1, nodes, cancel)?))
                    })
                    .collect::<Result<_, RunFailure>>()?,
            ),
            TemplateValue::Array { items } => Value::Array(
                items
                    .iter()
                    .map(|value| self.template(value, depth + 1, nodes, cancel))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
    pub(crate) fn retain_response(
        &mut self,
        node: &str,
        response: HttpResponse,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        self.extraction
            .retain_response(node, response, cancel)
            .map_err(Into::into)
    }
    pub(crate) fn extract(
        &mut self,
        node: &str,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        let value = self.extraction.extract(node, cancel)?;
        // Registration precedes every subsequent event, request, and mutation.
        self.security.remember_secret(value.expose());
        check_cancel(cancel)
    }
    pub(crate) fn update_session(
        &mut self,
        config: &SetSessionConfig,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        check_cancel(cancel)?;
        let mut set = BTreeMap::new();
        for (name, source) in &config.set_headers {
            check_cancel(cancel)?;
            let (prefix, value) = match source {
                SessionHeaderValue::Literal { value, prefix } => (prefix, value.as_str()),
                SessionHeaderValue::Identity { field, prefix } => {
                    (prefix, self.identity_value(field)?)
                }
                SessionHeaderValue::Extraction {
                    node_id,
                    name,
                    prefix,
                } => (prefix, self.extraction.output(node_id, name)?.expose()),
            };
            if value.len().saturating_add(prefix.len()) > MAX_SESSION_HEADER_BYTES {
                return Err(failure(
                    "sessionHeaderLimit",
                    "Rendered session header exceeds the byte limit.",
                ));
            }
            set.insert(name.clone(), format!("{prefix}{value}"));
        }
        check_cancel(cancel)?;
        // The transport validates all values and names before applying a batch.
        self.session.update_headers(&set, &config.remove_headers)?;
        for value in set.values() {
            self.security.remember_secret(value);
        }
        Ok(())
    }
    pub(crate) fn release_after(&mut self, node: &str) {
        self.extraction.release_after(node);
    }
}
pub(crate) fn check_cancel(cancel: &CancellationToken) -> Result<(), RunFailure> {
    if cancel.is_cancelled() {
        Err(failure("cancelled", "Run cancelled"))
    } else {
        Ok(())
    }
}
fn failure(kind: &str, message: &str) -> RunFailure {
    RunFailure {
        kind: kind.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SecretLookupError, SecretProvider};
    use interl_core::{compile_scenario, parse_scenario, ScenarioFormat};
    use std::ffi::OsString;
    struct Provider;
    impl SecretProvider for Provider {
        fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
            match name {
                "INTERL_ALICE_USERNAME" => Ok("fake-user-marker".into()),
                "INTERL_ALICE_PASSWORD" => Ok("quote\"slash\\password-marker".into()),
                _ => Err(SecretLookupError::Missing),
            }
        }
    }
    fn user(token: &str) -> UserContext {
        let mut doc = parse_scenario(
            include_str!("../../../scenarios/examples/auth-login.interl.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        doc.client_profiles[0]
            .headers
            .insert("Authorization".into(), "Bearer original-marker".into());
        let plan = compile_scenario(&doc).unwrap();
        let security = RunSecurity::prepare(&doc, &plan, vec![], &Provider).unwrap();
        let session = HttpSession::new(&doc.client_profiles[0], 65536, 65536).unwrap();
        let mut user = UserContext::new(&doc, plan.auth(), session, security, 65536).unwrap();
        let response = HttpResponse {
            status: 200,
            headers: Default::default(),
            body: serde_json::to_vec(&serde_json::json!({"accessToken":token})).unwrap(),
            truncated: false,
        };
        user.retain_response("login", response, &CancellationToken::new())
            .unwrap();
        user.extract("extract-token", &CancellationToken::new())
            .unwrap();
        assert_eq!(user.security.text(token), "[REDACTED]");
        user
    }
    #[test]
    fn materialization_preserves_literals_and_composes_only_explicit_sources() {
        let user = user("runtime-token-marker");
        let config = serde_json::from_value(serde_json::json!({"method":"POST","path":"/echo","body":{"kind":"jsonTemplate","fields":{
            "nested":{"source":"object","fields":{"list":{"source":"array","items":[{"source":"identity","field":"username"},{"source":"extraction","nodeId":"extract-token","name":"token"}]}}},
            "password":{"source":"identity","field":"password"},
            "literal":{"source":"literal","value":{"source":"identity","field":"not-a-reference","text":"${untouched}"}}
        }}})).unwrap();
        let request = user
            .prepare_http(&config, &Default::default(), &CancellationToken::new())
            .unwrap();
        assert_eq!(
            request.body.as_ref().unwrap(),
            &serde_json::json!({"nested":{"list":["fake-user-marker","runtime-token-marker"]},"password":"quote\"slash\\password-marker","literal":{"source":"identity","field":"not-a-reference","text":"${untouched}"}})
        );
        assert_eq!(
            request.body_bytes(),
            serde_json::to_vec(request.body.as_ref().unwrap())
                .unwrap()
                .len()
        );
    }
    #[test]
    fn invalid_dynamic_batch_never_partially_changes_session_headers() {
        for (token, prefix, kind) in [
            ("bad\r\nprivate-marker".to_owned(), "", "configuration"),
            ("x".repeat(8192), "Bearer ", "sessionHeaderLimit"),
        ] {
            let mut user = user(&token);
            let config = serde_json::from_value(serde_json::json!({"setHeaders":{
                "a-first":{"source":"literal","value":"would-change"},
                "authorization":{"source":"extraction","nodeId":"extract-token","name":"token","prefix":prefix}
            }})).unwrap();
            let error = user
                .update_session(&config, &CancellationToken::new())
                .unwrap_err();
            assert_eq!(error.kind, kind);
            assert!(!format!("{error:?}").contains("private-marker"));
            let http: HttpConfig =
                serde_json::from_value(serde_json::json!({"method":"GET","path":"/health"}))
                    .unwrap();
            let request = user.session.prepare(&http, &Default::default()).unwrap();
            assert!(!request.headers().contains_key("a-first"));
            assert_eq!(request.headers()["authorization"], "Bearer original-marker");
        }
    }
    #[test]
    fn cancelled_updates_and_request_materialization_do_not_proceed() {
        let mut user = user("runtime-token-marker");
        let cancel = CancellationToken::new();
        cancel.cancel();
        let config =
            serde_json::from_value(serde_json::json!({"removeHeaders":["authorization"]})).unwrap();
        assert_eq!(
            user.update_session(&config, &cancel).unwrap_err().kind,
            "cancelled"
        );
        let http: HttpConfig =
            serde_json::from_value(serde_json::json!({"method":"GET","path":"/health"})).unwrap();
        assert_eq!(
            user.prepare_http(&http, &Default::default(), &cancel)
                .err()
                .unwrap()
                .kind,
            "cancelled"
        );
        assert_eq!(
            user.session
                .prepare(&http, &Default::default())
                .unwrap()
                .headers()["authorization"],
            "Bearer original-marker"
        );
    }
}
