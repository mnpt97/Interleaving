//! Aggregate public credential references without ever resolving secret values.

use super::{problem, references::visit_node_sources};
use crate::{authentication_key, AuthPlan, Diagnostic, ProgramNode, ScenarioDocument};
use std::collections::BTreeSet;

/// Build the run-level secret plan from reachable workflow and lifecycle dependencies.
/// All consumed identity fields are still checked for semantic correctness, including
/// unused definitions. Native Authentication dependencies get independent namespaces.
/// `BTreeSet` deduplicates explicit identity/field pairs deterministically.
pub(super) fn authentication_plan(
    doc: &ScenarioDocument,
    flows: &[super::WorkflowAnalysis<'_>],
    reachable: &BTreeSet<usize>,
    all_identity_fields: BTreeSet<String>,
) -> Result<AuthPlan, Vec<Diagnostic>> {
    let lifecycle = &doc.groups[0].lifecycle;
    let mut auth = AuthPlan::default();
    let mut required = BTreeSet::new();
    for i in reachable {
        for r in flows[*i].auth.required_secrets() {
            required.insert((r.identity_id().to_owned(), r.field().to_owned()));
        }
    }
    // Include identity dependencies in call/output/lifecycle mappings, not merely HTTP bodies.
    /// Record an identity-field dependency when this scalar descriptor names one.
    fn fields(src: crate::scalar::ScalarRef<'_>, set: &mut BTreeSet<String>) {
        if let crate::scalar::ScalarRef::Identity { field } = src {
            set.insert(field.to_owned());
        }
    }
    let mut identity_fields = BTreeSet::new();
    for i in reachable {
        for n in &flows[*i].definition.nodes {
            visit_node_sources(n, |src| fields(src, &mut identity_fields));
        }
        for o in flows[*i].definition.outputs.values() {
            fields((&o.value).into(), &mut identity_fields);
        }
    }
    for b in [
        lifecycle.initialize.as_ref(),
        Some(&lifecycle.iteration),
        lifecycle.cleanup.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        for src in b.inputs.values() {
            fields(src.into(), &mut identity_fields);
        }
    }
    let crate::IdentityAssignment::Shared { identity_id } = &doc.groups[0].identity_assignment;
    let identity = identity_id
        .as_ref()
        .and_then(|id| doc.identities.iter().find(|v| &v.id == id));
    // All definitions must be semantically valid even if unused; only reachable refs are resolved.
    for field in all_identity_fields {
        if identity.is_none_or(|v| !v.credentials.contains_key(&field)) {
            return Err(vec![problem(
                "missingIdentityCredential",
                "/identities",
                "Assigned identity does not declare a consumed field.",
            )]);
        }
    }
    for field in identity_fields {
        let identity = identity.ok_or_else(|| {
            vec![problem(
                "missingIdentity",
                "/groups",
                "Assigned identity is required.",
            )]
        })?;
        required.insert((identity.id.clone(), field));
    }
    for (id, field) in required {
        let reference = doc
            .identities
            .iter()
            .find(|v| v.id == id)
            .and_then(|v| v.credentials.get(&field))
            .ok_or_else(|| {
                vec![problem(
                    "missingIdentityCredential",
                    "/identities",
                    "Assigned identity does not declare a consumed field.",
                )]
            })?
            .clone();
        auth.required_secrets.push(crate::RequiredSecret {
            identity_id: id,
            field,
            reference,
        });
    }
    for i in reachable {
        for (j, node) in flows[*i].definition.nodes.iter().enumerate() {
            if let ProgramNode::Authentication { config, .. } = node {
                for (field, reference) in [
                    ("username", &config.username),
                    ("password", &config.password),
                ] {
                    auth.required_secrets.push(crate::RequiredSecret {
                        identity_id: authentication_key(*i, j),
                        field: field.into(),
                        reference: reference.clone(),
                    });
                }
            }
        }
    }
    Ok(auth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_document, RequiredSecret, ScenarioFormat, SecretReference};
    use std::collections::BTreeMap;

    #[test]
    fn authentication_plan_should_report_an_unavailable_identity_reference() {
        let document = parse_document(
            include_str!("../../../../scenarios/examples/hello-api.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        let mut auth = AuthPlan::default();
        auth.required_secrets.push(RequiredSecret {
            identity_id: "missing".into(),
            field: "password".into(),
            reference: SecretReference::Env {
                name: "PASSWORD_ENV".into(),
            },
        });
        let analysis = super::super::WorkflowAnalysis {
            definition: &document.workflows[0],
            order: vec![0, 1, 2],
            auth,
            predicates: BTreeMap::new(),
            last_uses: BTreeMap::new(),
        };

        let errors = authentication_plan(
            &document,
            &[analysis],
            &BTreeSet::from([0]),
            BTreeSet::new(),
        )
        .unwrap_err();

        assert_eq!(errors[0].code, "missingIdentityCredential");
    }
}
