//! Materialization/session regressions exercised against the production invocation runner.
use super::*;
use interl_core::{ProgramHttp, SetSessionConfig};
struct UserContext {
    runtime: Runtime,
    frame: Frame,
}
impl UserContext {
    fn prepare_http(
        &self,
        http: &ProgramHttp,
        _variables: &BTreeMap<String, Value>,
        cancel: &CancellationToken,
    ) -> Result<interl_http::PreparedRequest, RunFailure> {
        self.runtime.prepare_http(&self.frame, http, cancel)
    }
    fn update_session(
        &mut self,
        config: &SetSessionConfig,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        self.runtime.session_update(config, &self.frame, cancel)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SecretLookupError, SecretProvider};
    use interl_core::{parse_scenario, ScenarioFormat};
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
        let prepared =
            PreparedRun::new_with_secret_provider(&doc, RunOptions::default(), &Provider).unwrap();
        let control = RunControl::new();
        let (sender, _receiver) = mpsc::channel(1);
        let trace = Trace::new(
            "test".into(),
            "vu:1".into(),
            sender,
            control.token.clone(),
            prepared.snapshot,
            prepared.options.max_report_bytes,
        );
        let extraction =
            ExtractionState::new(prepared.program.workflows()[0].auth(), 65536).unwrap();
        let runtime = Runtime {
            program: prepared.program,
            session: prepared.session,
            security: prepared.security,
            options: prepared.options,
            trace,
            control,
            frames: Vec::new(),
            next_invocation: 0,
            visits: 0,
            phase: "iteration".into(),
            cleanup_deadline: None,
        };
        let frame = Frame {
            flow: 0,
            pc: 0,
            invocation: "inv:1".into(),
            parent: None,
            caller: None,
            values: BTreeMap::new(),
            extraction,
            pending: false,
        };
        let mut user = UserContext { runtime, frame };
        let response = interl_http::HttpResponse {
            status: 200,
            headers: Default::default(),
            body: serde_json::to_vec(&json!({"accessToken":token})).unwrap(),
            truncated: false,
        };
        user.frame
            .extraction
            .retain_response("login", response, &CancellationToken::new())
            .unwrap();
        let value = user
            .frame
            .extraction
            .extract("extract-token", &CancellationToken::new())
            .unwrap();
        user.runtime.security.remember_secret(value.expose());
        assert_eq!(user.runtime.security.text(token), "[REDACTED]");
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
            let http: ProgramHttp =
                serde_json::from_value(serde_json::json!({"method":"GET","path":"/health"}))
                    .unwrap();
            let request = user
                .runtime
                .session
                .prepare(&super::super::static_config(&http), &Default::default())
                .unwrap();
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
        let http: ProgramHttp =
            serde_json::from_value(serde_json::json!({"method":"GET","path":"/health"})).unwrap();
        assert_eq!(
            user.prepare_http(&http, &Default::default(), &cancel)
                .err()
                .unwrap()
                .kind,
            "cancelled"
        );
        assert_eq!(
            user.runtime
                .session
                .prepare(&super::super::static_config(&http), &Default::default())
                .unwrap()
                .headers()["authorization"],
            "Bearer original-marker"
        );
    }
}
