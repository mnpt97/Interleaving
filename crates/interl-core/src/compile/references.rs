//! Scope, type, and producer-order checks shared by compiler passes.

use super::problem;
use crate::scalar::ScalarRef;
use crate::{
    scalar_type, Diagnostic, PortType, ProgramBody, ProgramNode, ProgramTemplate, ScenarioDocument,
    WorkflowBinding, WorkflowDefinition,
};
use std::collections::{BTreeMap, BTreeSet};

/// Borrow the document and accumulate bounded diagnostics and identity-field uses.
/// The lifetime `'a` guarantees the document outlives this context. `pub(super)`
/// allows the parent compiler module to access this helper without making it a public API.
pub(super) struct ValidationContext<'a> {
    pub(super) doc: &'a ScenarioDocument,
    pub(super) dominators: BTreeMap<&'a str, BTreeSet<&'a str>>,
    pub(super) errors: Vec<Diagnostic>,
    pub(super) identity_fields: BTreeSet<String>,
}
impl ValidationContext<'_> {
    /// Append one semantic diagnostic while respecting the shared 100-error cap.
    pub(super) fn add(&mut self, code: &str, path: &str, msg: &str) {
        if self.errors.len() < 100 {
            self.errors.push(problem(code, path, msg));
        }
    }
    /// Check nonempty stable identifiers and their UTF-8 byte limit.
    pub(super) fn validate_id(&mut self, id: &str, path: &str) {
        if id.trim().is_empty() || id.len() > 128 {
            self.add(
                "invalidId",
                path,
                "IDs must be nonempty and at most 128 UTF-8 bytes.",
            );
        }
    }
    /// Check the bounded ASCII identifier grammar for ports and related named fields.
    pub(super) fn validate_identifier(&mut self, id: &str, path: &str) {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .enumerate()
                .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
        {
            self.add(
                "invalidPortName",
                path,
                "Use an ASCII identifier of at most 128 bytes.",
            );
        }
    }
    /// Determine a scalar source's type and check scope/availability at a consumer.
    /// `at` is the consumer's position in connected execution order; local producers
    /// must have smaller positions. Optional workflow/initialize arguments distinguish
    /// invocation-local, lifecycle, and initialization-export scopes.
    pub(super) fn source_type(
        &mut self,
        src: ScalarRef<'_>,
        w: Option<&WorkflowDefinition>,
        pos: &BTreeMap<&str, usize>,
        at: usize,
        path: &str,
        init: Option<&WorkflowDefinition>,
    ) -> Option<PortType> {
        if let ScalarRef::Extraction { node_id, .. }
        | ScalarRef::CallOutput { node_id, .. }
        | ScalarRef::Response { node_id, .. } = src
        {
            if w.is_some() {
                let consumer = pos.iter().find_map(|(id, p)| (*p == at).then_some(*id));
                if consumer.is_none_or(|id| {
                    id == node_id
                        || !self
                            .dominators
                            .get(id)
                            .is_some_and(|producers| producers.contains(node_id))
                }) {
                    self.add(
                        "valueNotAvailable",
                        path,
                        "Source must be available on every path reaching its consumer.",
                    );
                    return None;
                }
            }
        }
        let kind = match src {
            ScalarRef::Literal { value } => scalar_type(value),
            ScalarRef::Text { value } => (value.len() <= 8192).then_some(PortType::String),
            ScalarRef::Environment { name } => self
                .doc
                .environment
                .variables
                .get(name)
                .and_then(scalar_type),
            ScalarRef::Identity { field } => {
                self.validate_id(field, path);
                self.identity_fields.insert(field.to_owned());
                Some(PortType::String)
            }
            ScalarRef::Input { name } => w.and_then(|w| w.inputs.get(name)).map(|p| p.kind),
            ScalarRef::Lifecycle { name, .. } => {
                init.and_then(|w| w.outputs.get(name)).map(|p| p.kind)
            }
            ScalarRef::Extraction { node_id, name } | ScalarRef::CallOutput { node_id, name } => {
                if pos.get(node_id).is_none_or(|p| *p >= at) {
                    self.add(
                        "valueNotAvailable",
                        path,
                        "Source must precede its consumer in this invocation.",
                    );
                    return None;
                }
                w.and_then(|w| w.nodes.iter().find(|n| n.id() == node_id))
                    .and_then(|node| match (src, node) {
                        (ScalarRef::Extraction { .. }, ProgramNode::Extract { config, .. }) => {
                            config.type_of(name)
                        }
                        (ScalarRef::CallOutput { .. }, ProgramNode::Call { config, .. }) => self
                            .doc
                            .workflows
                            .iter()
                            .find(|w| w.id == config.workflow_id)
                            .and_then(|w| w.outputs.get(name))
                            .map(|p| p.kind),
                        _ => None,
                    })
            }
            ScalarRef::Response { node_id, .. } => {
                if pos.get(node_id).is_none_or(|p| *p >= at) {
                    self.add(
                        "valueNotAvailable",
                        path,
                        "Response source must be an earlier HTTP node.",
                    );
                    return None;
                }
                w.and_then(|w| w.nodes.iter().find(|n| n.id() == node_id))
                    .filter(|n| matches!(n, ProgramNode::Http { .. }))
                    .map(|_| PortType::Integer)
            }
        };
        if kind.is_none() {
            self.add(
                "valueNotAvailable",
                path,
                "Scalar source is missing, oversized, mistyped, or outside its allowed scope.",
            );
        }
        kind
    }
    /// Validate a workflow invocation's required inputs, unknown keys, source types, and scope.
    /// A missing mapping is allowed only when the declared input has a default.
    pub(super) fn validate_binding(
        &mut self,
        b: &WorkflowBinding,
        w: Option<&WorkflowDefinition>,
        pos: &BTreeMap<&str, usize>,
        at: usize,
        path: &str,
        init: Option<&WorkflowDefinition>,
    ) {
        let Some(callee) = self.doc.workflows.iter().find(|w| w.id == b.workflow_id) else {
            self.add(
                "missingWorkflow",
                path,
                "Referenced workflow does not exist.",
            );
            return;
        };
        for (name, port) in &callee.inputs {
            match b.inputs.get(name) {
                Some(src) => {
                    let kind = self.source_type(src.into(), w, pos, at, path, init);
                    if kind.is_some_and(|k| k != port.kind) {
                        self.add(
                            "portTypeMismatch",
                            path,
                            "Input type does not match its declaration.",
                        );
                    }
                }
                None if port.default.is_none() => {
                    self.add("missingInput", path, "Required workflow input is missing.")
                }
                _ => {}
            }
        }
        for name in b.inputs.keys() {
            if !callee.inputs.contains_key(name) {
                self.add("unknownInput", path, "Mapping names an undeclared input.");
            }
        }
        if w.is_none() {
            for src in b.inputs.values() {
                if !matches!(
                    ScalarRef::from(src),
                    ScalarRef::Literal { .. }
                        | ScalarRef::Environment { .. }
                        | ScalarRef::Identity { .. }
                        | ScalarRef::Lifecycle { .. }
                ) {
                    self.add(
                        "invalidLifecycleReference",
                        path,
                        "Lifecycle inputs cannot access invocation-local values.",
                    );
                }
            }
        }
    }
}
/// Visit dependencies without collecting or cloning their source descriptors.
/// The callback is statically dispatched (`impl FnMut`); the template work stack
/// holds only borrowed child nodes. Literal payload JSON is never traversed.
pub(super) fn visit_node_sources<'a>(node: &'a ProgramNode, mut visit: impl FnMut(ScalarRef<'a>)) {
    match node {
        ProgramNode::ExclusiveSplit { config, .. } => {
            visit(ScalarRef::Response {
                node_id: &config.source_node_id,
                field: &crate::ResponseField::Status,
            });
            config
                .inputs
                .values()
                .for_each(|source| visit(source.into()));
        }
        ProgramNode::Call { config, .. } => {
            config
                .inputs
                .values()
                .for_each(|source| visit(source.into()));
        }
        ProgramNode::SetSession { config, .. } => {
            config
                .set_headers
                .values()
                .for_each(|header| visit(ScalarRef::header(header).0));
        }
        ProgramNode::Http { config, .. } => {
            if let Some(ProgramBody::JsonTemplate { fields }) = &config.body {
                let mut pending: Vec<_> = fields.values().collect();
                while let Some(value) = pending.pop() {
                    match value {
                        ProgramTemplate::Object { fields } => pending.extend(fields.values()),
                        ProgramTemplate::Array { items } => pending.extend(items),
                        ProgramTemplate::Literal { .. } => {}
                        leaf => {
                            if let Some(source) = ScalarRef::template(leaf) {
                                visit(source);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}
