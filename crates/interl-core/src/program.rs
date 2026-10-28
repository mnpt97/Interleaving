//! Immutable analysis results consumed by the execution engine.
//!
//! These types expose borrowed views through getters. Their `pub(crate)` fields allow
//! the compiler to construct plans while preventing other crates from mutating them.

use crate::{
    CompiledExpression, PortType, ScalarSource, ScenarioDocument, SecretReference,
    WorkflowDefinition,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Immutable auth references and capture/liveness instructions. No values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthPlan {
    pub(crate) required_secrets: Vec<RequiredSecret>,
    pub(crate) extractions: BTreeMap<String, CompiledExtraction>,
    pub(crate) sensitive_http_nodes: BTreeSet<String>,
    pub(crate) response_last_uses: BTreeMap<String, String>,
}
impl AuthPlan {
    /// Borrow the credential references needed by reachable workflows.
    /// No secret values are stored or resolved here; the engine owns that work.
    pub fn required_secrets(&self) -> &[RequiredSecret] {
        &self.required_secrets
    }
    /// Borrow extraction instructions keyed by their workflow-local node ID.
    pub fn extractions(&self) -> &BTreeMap<String, CompiledExtraction> {
        &self.extractions
    }
    /// Borrow the HTTP node IDs whose request/response previews must be omitted.
    pub fn sensitive_http_nodes(&self) -> &BTreeSet<String> {
        &self.sensitive_http_nodes
    }
    /// Borrow the final consumer of each retained HTTP response body.
    /// The engine can release a body as soon as that consumer finishes.
    pub fn response_last_uses(&self) -> &BTreeMap<String, String> {
        &self.response_last_uses
    }
}

/// One named credential dependency, containing a reference rather than its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredSecret {
    pub(crate) identity_id: String,
    pub(crate) field: String,
    pub(crate) reference: SecretReference,
}
impl RequiredSecret {
    /// Borrow the lookup namespace for this credential.
    /// It may be an explicit identity ID or an internal Authentication-node key.
    pub fn identity_id(&self) -> &str {
        &self.identity_id
    }
    /// Borrow the credential field within its lookup namespace, such as `password`.
    pub fn field(&self) -> &str {
        &self.field
    }
    /// Borrow the descriptor telling the engine which secret source to read.
    pub fn reference(&self) -> &SecretReference {
        &self.reference
    }
}

/// A checked property-access expression and the response/output lifetime it requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledExtraction {
    pub(crate) source_node_id: String,
    pub(crate) outputs: BTreeMap<String, CompiledExtractOutput>,
    pub(crate) last_use_node_id: String,
}
impl CompiledExtraction {
    /// Borrow the earlier HTTP node whose body this extraction reads.
    pub fn source_node_id(&self) -> &str {
        &self.source_node_id
    }
    /// Borrow named, typed property paths evaluated atomically by this node.
    pub fn outputs(&self) -> &BTreeMap<String, CompiledExtractOutput> {
        &self.outputs
    }
    /// Borrow the final consumer of this extracted value.
    /// An internal return marker can keep the value alive through workflow output evaluation.
    pub fn last_use_node_id(&self) -> &str {
        &self.last_use_node_id
    }
}

/// One checked property path and its expected scalar type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledExtractOutput {
    pub(crate) expression: CompiledExpression,
    pub(crate) output_type: PortType,
}
impl CompiledExtractOutput {
    /// Borrow the admitted FEEL property path without evaluating it.
    pub fn expression(&self) -> &CompiledExpression {
        &self.expression
    }
    /// Return the expected scalar result type.
    pub fn output_type(&self) -> PortType {
        self.output_type
    }
}

/// Identify a value inside one workflow invocation.
///
/// `Ord` and `PartialOrd` are derived so keys can be used in deterministic
/// `BTreeMap`/`BTreeSet` collections; the strings identify producers and output names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ValueKey {
    /// An input port name.
    Input(String),
    /// An extraction node ID followed by its output name.
    Extraction(String, String),
    /// A call node ID followed by its output name.
    Call(String, String),
    /// An HTTP node ID whose status metadata is retained.
    Response(String),
}
impl ScalarSource {
    /// Return a runtime storage key for invocation-local sources.
    /// Literals, scenario variables, identities, and lifecycle bindings are resolved
    /// elsewhere, so they return `None` rather than occupying frame-local storage.
    pub fn key(&self) -> Option<ValueKey> {
        crate::scalar::ScalarRef::from(self).key()
    }
}
/// A workflow definition plus validated execution order and value-retention plans.
#[derive(Debug, Clone)]
pub struct CompiledWorkflow {
    // Every workflow plan points into the same immutable snapshot. Arc keeps a
    // cloned workflow plan valid even after its parent Program is dropped.
    pub(crate) document: Arc<ScenarioDocument>,
    pub(crate) definition_index: usize,
    pub(crate) order: Vec<usize>,
    pub(crate) predicates: BTreeMap<usize, crate::CompiledPredicate>,
    pub(crate) auth: AuthPlan,
    pub(crate) last_uses: BTreeMap<ValueKey, String>,
}
impl CompiledWorkflow {
    /// Borrow a checked status predicate by its original edge index.
    pub fn predicate(&self, edge: usize) -> Option<&crate::CompiledPredicate> {
        self.predicates.get(&edge)
    }

    /// Borrow the original typed definition retained in this compiled workflow.
    pub fn definition(&self) -> &WorkflowDefinition {
        &self.document.workflows[self.definition_index]
    }
    /// Borrow node indices in topological analysis order.
    /// These index `definition().nodes`. The engine follows actual edges and chosen
    /// routes, so this list is not a schedule that executes every branch.
    pub fn order(&self) -> &[usize] {
        &self.order
    }
    /// Borrow workflow-local capture, extraction, and retention instructions.
    pub fn auth(&self) -> &AuthPlan {
        &self.auth
    }
    /// Borrow each local value's final consumer so the engine can reclaim storage.
    pub fn last_uses(&self) -> &BTreeMap<ValueKey, String> {
        &self.last_uses
    }
}
/// A semantically checked scenario ready for engine preparation.
///
/// Compilation takes one owned document snapshot shared with its workflow plans.
/// `Arc` shares immutable ownership across threads; it does not permit mutation.
/// Cloning a plan shares that snapshot, so later editor changes cannot affect it. Construct this through
/// [`crate::compile_document`], not by bypassing validation.
#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) document: Arc<ScenarioDocument>,
    pub(crate) flows: Vec<CompiledWorkflow>,
    pub(crate) reachable: Vec<usize>,
    pub(crate) auth: AuthPlan,
}
impl Program {
    /// Borrow the captured scenario document; no clone or file access occurs.
    pub fn document(&self) -> &ScenarioDocument {
        &self.document
    }
    /// Borrow all checked workflow plans, including definitions not reached by the lifecycle.
    pub fn workflows(&self) -> &[CompiledWorkflow] {
        &self.flows
    }
    /// Borrow indices of workflows reachable from initialize, iteration, or cleanup.
    /// Only these workflows contribute runtime secret lookup dependencies.
    pub fn reachable(&self) -> &[usize] {
        &self.reachable
    }
    /// Borrow the aggregate secret-reference plan for reachable workflows.
    pub fn auth(&self) -> &AuthPlan {
        &self.auth
    }
    /// Find a workflow by stable ID, returning its index or `None` if absent.
    pub fn workflow_index(&self, id: &str) -> Option<usize> {
        self.flows.iter().position(|w| w.definition().id == id)
    }
}
/// Classify a supported scalar without coercion.
///
/// Strings are limited to 8192 UTF-8 bytes; integers must fit JavaScript's exact
/// integer range (±(2^53−1)) for desktop interoperability. Objects, arrays, null,
/// floats, and out-of-range values return `None`. `then_some` converts a boolean
/// condition into an `Option` without an explicit `if` expression.
///
/// # Examples
/// ```
/// use interl_core::{scalar_type, PortType};
/// use serde_json::json;
/// assert_eq!(scalar_type(&json!(42)), Some(PortType::Integer));
/// assert_eq!(scalar_type(&json!("42")), Some(PortType::String));
/// assert_eq!(scalar_type(&json!([42])), None);
/// ```
pub fn scalar_type(value: &serde_json::Value) -> Option<PortType> {
    if let Some(s) = value.as_str() {
        return (s.len() <= 8192).then_some(PortType::String);
    }
    if value.is_boolean() {
        return Some(PortType::Boolean);
    }
    let valid = value
        .as_i64()
        .is_some_and(|n| (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&n))
        || value.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991);
    valid.then_some(PortType::Integer)
}
