//! Trusted Rust providers return values only at explicit runtime preparation.
use crate::redaction::Redactor;
use interl_core::{AuthPlan, Diagnostic, ErrorCategory, SecretReference};
use std::{collections::BTreeMap, ffi::OsString, fmt, sync::Arc};

pub const MAX_CREDENTIAL_BYTES: usize = 8192;
pub const MAX_CREDENTIAL_REFERENCES: usize = 4096;
pub const MAX_TOTAL_CREDENTIAL_BYTES: usize = 1_048_576;

/// Typed failures cannot retain a provider's secret-bearing error text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretLookupError {
    Missing,
    Unavailable,
}

/// Providers must not log values or panic. They are trusted application code,
/// not scenario-supplied plugins. No provider is retained after preparation.
pub trait SecretProvider: Send + Sync {
    fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError>;
}

#[derive(Debug, Default)]
pub struct EnvironmentSecretProvider;
impl SecretProvider for EnvironmentSecretProvider {
    fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
        // No .env loading, persistence, lossy Unicode conversion, or fallbacks.
        if name.is_empty() || name.contains(['=', '\0']) {
            return Err(SecretLookupError::Unavailable);
        }
        std::env::var_os(name).ok_or(SecretLookupError::Missing)
    }
}

/// No Serialize, Display, or Clone. Raw access stays inside the engine.
pub(crate) struct SecretValue(String);
impl SecretValue {
    fn from_os(value: OsString) -> Result<Self, &'static str> {
        let value = value.into_string().map_err(|_| "nonUnicodeSecret")?;
        if value.is_empty() {
            return Err("emptySecret");
        }
        if value.len() > MAX_CREDENTIAL_BYTES {
            return Err("secretLimit");
        }
        Ok(Self(value))
    }
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

/// Immutable values may be shared by reference; mutable HTTP state is not.
/// No Serialize/Clone, and Debug never prints identity IDs, fields, or values.
pub(crate) struct ResolvedCredentials {
    values: BTreeMap<(String, String), Arc<SecretValue>>,
}
impl fmt::Debug for ResolvedCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedCredentials")
            .field("references", &self.values.len())
            .finish_non_exhaustive()
    }
}
impl ResolvedCredentials {
    pub(crate) fn resolve(
        plan: &AuthPlan,
        provider: &dyn SecretProvider,
    ) -> Result<Self, Diagnostic> {
        if plan.required_secrets().len() > MAX_CREDENTIAL_REFERENCES {
            return Err(failure("secretLimit"));
        }
        let mut by_name: BTreeMap<&str, Arc<SecretValue>> = BTreeMap::new();
        let mut values = BTreeMap::new();
        let mut bytes = 0usize;
        for required in plan.required_secrets() {
            let SecretReference::Env { name } = required.reference();
            let secret = if let Some(value) = by_name.get(name.as_str()) {
                Arc::clone(value)
            } else {
                let value = provider.lookup(name).map_err(|error| {
                    reference_failure(
                        match error {
                            SecretLookupError::Missing => "missingSecret",
                            SecretLookupError::Unavailable => "secretProviderUnavailable",
                        },
                        name,
                    )
                })?;
                let value = Arc::new(
                    SecretValue::from_os(value).map_err(|code| reference_failure(code, name))?,
                );
                bytes = bytes.saturating_add(value.expose().len());
                if bytes > MAX_TOTAL_CREDENTIAL_BYTES {
                    return Err(failure("secretLimit"));
                }
                by_name.insert(name, Arc::clone(&value));
                value
            };
            values.insert(
                (required.identity_id().into(), required.field().into()),
                secret,
            );
        }
        Ok(Self { values })
    }
    pub(crate) fn register(&self, redactor: &mut Redactor) {
        for value in self.values.values() {
            redactor.remember_secret(value.expose());
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
    pub(crate) fn get(&self, identity: &str, field: &str) -> Option<&SecretValue> {
        self.values
            .get(&(identity.into(), field.into()))
            .map(Arc::as_ref)
    }
}

fn failure(code: &'static str) -> Diagnostic {
    let message = match code {
        "missingSecret" => "A required environment credential is missing.",
        "emptySecret" => "A required environment credential is empty.",
        "nonUnicodeSecret" => "A required environment credential is not valid Unicode.",
        "secretLimit" => "Credentials exceed an 8192-byte value, 4096-reference, or 1048576-byte aggregate limit.",
        _ => "The credential provider is unavailable.",
    };
    Diagnostic::new(
        if code == "secretLimit" {
            ErrorCategory::ResourceLimit
        } else {
            ErrorCategory::Validation
        },
        code,
        "/identities",
        message,
    )
}

fn reference_failure(code: &'static str, name: &str) -> Diagnostic {
    let mut diagnostic = failure(code);
    // Compiled environment names are bounded ASCII public references, not values.
    diagnostic.message.push_str(" Environment reference: ");
    diagnostic.message.push_str(name);
    diagnostic.message.push('.');
    diagnostic
}

#[cfg(test)]
mod tests {
    use super::*;
    use interl_core::{
        compile_scenario, parse_scenario, RequestBody, ScenarioDocument, ScenarioFormat,
        SessionHeaderValue, WorkflowNode,
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Provider {
        values: BTreeMap<String, OsString>,
        calls: Mutex<Vec<String>>,
        unavailable: bool,
    }
    impl SecretProvider for Provider {
        fn lookup(&self, name: &str) -> Result<OsString, SecretLookupError> {
            self.calls.lock().unwrap().push(name.into());
            if self.unavailable {
                return Err(SecretLookupError::Unavailable);
            }
            self.values
                .get(name)
                .cloned()
                .ok_or(SecretLookupError::Missing)
        }
    }
    fn document() -> ScenarioDocument {
        parse_scenario(
            include_str!("../../../scenarios/examples/auth-login.interl.json"),
            ScenarioFormat::Json,
        )
        .unwrap()
    }
    fn provider() -> Provider {
        Provider {
            values: [
                ("INTERL_ALICE_USERNAME".into(), "fake-user".into()),
                ("INTERL_ALICE_PASSWORD".into(), "fake-password".into()),
            ]
            .into(),
            ..Default::default()
        }
    }
    #[test]
    fn only_required_references_are_resolved_once_per_environment_name() {
        let mut doc = document();
        doc.identities[0].credentials.insert(
            "unused".into(),
            SecretReference::Env {
                name: "UNUSED".into(),
            },
        );
        doc.identities.push(interl_core::Identity {
            id: "unselected".into(),
            credentials: [(
                "token".into(),
                SecretReference::Env {
                    name: "UNSELECTED".into(),
                },
            )]
            .into(),
        });
        let plan = compile_scenario(&doc).unwrap();
        let provider = provider();
        let resolved = ResolvedCredentials::resolve(plan.auth(), &provider).unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(
            provider.calls.lock().unwrap().as_slice(),
            ["INTERL_ALICE_PASSWORD", "INTERL_ALICE_USERNAME"]
        );
        assert_eq!(
            resolved.values[&("alice".into(), "password".into())].expose(),
            "fake-password"
        );
    }
    #[test]
    fn repeated_references_share_a_single_immutable_value_per_preparation() {
        let mut doc = document();
        for reference in doc.identities[0].credentials.values_mut() {
            *reference = SecretReference::Env {
                name: "SHARED".into(),
            };
        }
        let plan = compile_scenario(&doc).unwrap();
        let mut provider = Provider {
            values: [("SHARED".into(), "first-value".into())].into(),
            ..Default::default()
        };
        let first = ResolvedCredentials::resolve(plan.auth(), &provider).unwrap();
        assert!(Arc::ptr_eq(
            &first.values[&("alice".into(), "username".into())],
            &first.values[&("alice".into(), "password".into())]
        ));
        provider
            .values
            .insert("SHARED".into(), "changed-value".into());
        let second = ResolvedCredentials::resolve(plan.auth(), &provider).unwrap();
        assert_eq!(provider.calls.lock().unwrap().len(), 2);
        assert_eq!(
            first.values[&("alice".into(), "password".into())].expose(),
            "first-value"
        );
        assert_eq!(
            second.values[&("alice".into(), "password".into())].expose(),
            "changed-value"
        );
    }
    #[test]
    fn lookup_and_value_failures_have_safe_typed_diagnostics() {
        let plan = compile_scenario(&document()).unwrap();
        let cases = vec![
            (None, "missingSecret"),
            (Some(OsString::new()), "emptySecret"),
            (Some(OsString::from("s".repeat(8193))), "secretLimit"),
        ];
        #[cfg(unix)]
        let cases = {
            let mut cases = cases;
            use std::os::unix::ffi::OsStringExt;
            cases.push((
                Some(OsString::from_vec(b"secret-marker-\xff".to_vec())),
                "nonUnicodeSecret",
            ));
            cases
        };
        for (value, code) in cases {
            let mut provider = provider();
            provider.values.remove("INTERL_ALICE_PASSWORD");
            if let Some(value) = value {
                provider
                    .values
                    .insert("INTERL_ALICE_PASSWORD".into(), value);
            }
            let error = ResolvedCredentials::resolve(plan.auth(), &provider)
                .err()
                .unwrap();
            assert_eq!(error.code, code);
            assert_eq!(error.field_path, "/identities");
            assert!(error.message.contains("INTERL_ALICE_PASSWORD"));
            assert!(!format!("{error:?} {error}").contains("secret-marker"));
        }
        let provider = Provider {
            unavailable: true,
            ..provider()
        };
        assert_eq!(
            ResolvedCredentials::resolve(plan.auth(), &provider)
                .err()
                .unwrap()
                .code,
            "secretProviderUnavailable"
        );
    }
    #[test]
    fn unicode_values_are_byte_bounded_and_debug_never_exposes_values() {
        let value = SecretValue::from_os(OsString::from("é".repeat(4096))).unwrap();
        assert_eq!(value.expose().len(), MAX_CREDENTIAL_BYTES);
        assert_eq!(format!("{value:?}"), "SecretValue([REDACTED])");
        assert!(SecretValue::from_os(OsString::from("é".repeat(4097))).is_err());
        let resolved = ResolvedCredentials::resolve(
            compile_scenario(&document()).unwrap().auth(),
            &provider(),
        )
        .unwrap();
        let formatted = format!("error context: {resolved:?}");
        assert!(!formatted.contains("fake-password"));
        assert!(!formatted.contains("fake-user"));
        assert!(!formatted.contains("alice"));
    }
    fn reference_plan(count: usize, shared: bool) -> interl_core::ExecutionPlan {
        let mut doc = document();
        doc.identities[0].credentials.clear();
        if let WorkflowNode::Http { config, .. } = &mut doc.workflow.nodes[1] {
            config.body = Some(RequestBody::Json { value: json!({}) });
        }
        let WorkflowNode::SetSession { config, .. } = &mut doc.workflow.nodes[3] else {
            panic!()
        };
        config.set_headers.clear();
        for index in 0..count {
            let field = format!("field_{index}");
            doc.identities[0].credentials.insert(
                field.clone(),
                SecretReference::Env {
                    name: if shared {
                        "SHARED".into()
                    } else {
                        format!("FIELD_{index}")
                    },
                },
            );
            config.set_headers.insert(
                format!("x-field-{index}"),
                SessionHeaderValue::Identity {
                    field,
                    prefix: String::new(),
                },
            );
        }
        compile_scenario(&doc).unwrap()
    }
    #[test]
    fn reference_and_aggregate_limits_bound_resolution() {
        let mut provider = Provider {
            values: [("SHARED".into(), "value".into())].into(),
            ..Default::default()
        };
        let plan = reference_plan(4097, true);
        assert_eq!(
            ResolvedCredentials::resolve(plan.auth(), &provider)
                .err()
                .unwrap()
                .code,
            "secretLimit"
        );
        assert!(provider.calls.lock().unwrap().is_empty());
        let plan = reference_plan(4096, true);
        assert_eq!(
            ResolvedCredentials::resolve(plan.auth(), &provider)
                .unwrap()
                .len(),
            4096
        );
        assert_eq!(provider.calls.lock().unwrap().len(), 1);
        for index in 0..129 {
            provider.values.insert(
                format!("FIELD_{index}"),
                format!("{index:04}{}", "s".repeat(8188)).into(),
            );
        }
        assert!(ResolvedCredentials::resolve(reference_plan(128, false).auth(), &provider).is_ok());
        assert_eq!(
            ResolvedCredentials::resolve(reference_plan(129, false).auth(), &provider)
                .err()
                .unwrap()
                .code,
            "secretLimit"
        );
    }
    #[test]
    fn environment_provider_rejects_names_that_could_panic() {
        for name in ["", "INVALID=NAME", "INVALID\0NAME"] {
            assert_eq!(
                EnvironmentSecretProvider.lookup(name),
                Err(SecretLookupError::Unavailable)
            );
        }
    }
}
