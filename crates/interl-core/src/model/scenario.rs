//! Document ownership, workload lifecycles, workflow definitions, and scalar ports.
use crate::{
    ClientProfile, EmptyDeclarations, Environment, Identity, IdentityAssignment, Layout,
    ProgramEdge, ProgramNode, ScalarSource,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The single persisted scenario model shared by JSON, YAML, the editor, and CLI.
/// Parsing establishes the structural shape; [`crate::compile_document`] enforces
/// semantic rules. Owning collections here avoids borrowing source text after parsing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioDocument {
    /// Persisted format discriminator; checked at parse/compile boundaries, independent of Rust module layout.
    #[schemars(range(min = 2, max = 2))]
    pub schema_version: u32,
    /// Stable identifier within the containing collection; display names may change independently.
    pub id: String,
    /// Display name or referenced name, as specified by the containing type.
    pub name: String,
    /// Scenario-owned configuration variables, not process environment secrets.
    #[serde(default, skip_serializing_if = "Environment::is_empty")]
    pub environment: Environment,
    /// Transport defaults available to workload groups.
    pub client_profiles: Vec<ClientProfile>,
    /// Named secret-reference maps for explicit identity-field sources.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<Identity>,
    /// Workload/lifecycle declarations; current compilation requires one group.
    pub groups: Vec<WorkloadGroup>,
    /// Reusable workflow definitions, each with its own local node/edge IDs.
    pub workflows: Vec<WorkflowDefinition>,
    /// Reserved declarations; only an empty array is accepted today.
    #[serde(default, skip_serializing_if = "EmptyDeclarations::is_empty")]
    pub assertions: EmptyDeclarations,
}
/// Bind transport defaults and lifecycle workflows for one virtual user.
/// Compilation currently requires exactly one group with the once executor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadGroup {
    /// Stable identifier within the containing collection; display names may change independently.
    pub id: String,
    /// ID of the declared client profile supplying transport defaults.
    pub client_profile_id: String,
    /// Optional identity used by explicit identity-field mappings.
    #[serde(default, skip_serializing_if = "IdentityAssignment::is_empty")]
    pub identity_assignment: IdentityAssignment,
    /// Scheduling policy; currently one execution by one virtual user.
    #[serde(default, skip_serializing_if = "Executor::is_once")]
    pub executor: Executor,
    /// Workflow bindings executed during initialize, iteration, and cleanup.
    pub lifecycle: Lifecycle,
}
/// Supported workload scheduling policy; concurrent/ramping policies are not yet executable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Executor {
    /// Run one iteration for a single virtual user.
    #[default]
    Once,
}
/// Optional initialization, required iteration, and optional bounded cleanup.
/// All phases run in the same VU-owned session; frame-local workflow values stay scoped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lifecycle {
    /// Optional workflow invoked before iteration; its declared exports may feed iteration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialize: Option<WorkflowBinding>,
    /// Required workflow invoked once after successful initialization.
    pub iteration: WorkflowBinding,
    /// Optional workflow attempted after normal execution or eligible failures/cancellation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<WorkflowBinding>,
    /// Deadline for the entire cleanup call tree in milliseconds; defaults to 2000.
    #[serde(
        default = "cleanup_timeout",
        skip_serializing_if = "is_cleanup_timeout"
    )]
    pub cleanup_timeout_ms: u64,
}
/// Supply Serde's default cleanup deadline in milliseconds when the field is omitted.
fn cleanup_timeout() -> u64 {
    2000
}
/// Invoke a workflow by stable ID with explicit mappings to its input ports.
/// The empty-map default permits workflows that have no required inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowBinding {
    /// Stable ID of the workflow to invoke.
    pub workflow_id: String,
    /// Named workflow input declarations or bindings, depending on this type.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, ScalarSource>,
}
/// Supported scalar port types, checked without automatic coercion.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default,
)]
#[serde(rename_all = "camelCase")]
pub enum PortType {
    /// A bounded UTF-8 string.
    #[default]
    String,
    /// An integer exactly representable by the desktop JavaScript runtime.
    Integer,
    /// A boolean value, without string/numeric coercion.
    Boolean,
}
/// Declare an input's type and optional default.
/// The custom deserializer distinguishes an absent default (`None`) from an explicit
/// JSON null (`Some(Value::Null)`), which semantic validation must reject.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputPort {
    /// Declared scalar type; serialized as `type` because `type` is a Rust keyword.
    #[serde(rename = "type")]
    pub kind: PortType,
    /// Optional scalar fallback. Explicit null remains present for validation to reject.
    #[serde(
        default,
        deserialize_with = "port_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub default: Option<Value>,
}
/// Declare a typed exported scalar and where its value comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    /// Declared scalar type; serialized as `type` because `type` is a Rust keyword.
    #[serde(rename = "type")]
    pub kind: PortType,
    /// Literal JSON/text or an explicit scalar source, as selected by this variant/type.
    pub value: ScalarSource,
}
/// A reusable graph with invocation-local inputs, outputs, nodes, edges, and layout.
/// IDs are local to this definition; separate invocations do not share temporary values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowDefinition {
    /// Stable identifier within the containing collection; display names may change independently.
    pub id: String,
    /// Display name or referenced name, as specified by the containing type.
    pub name: String,
    /// Named workflow input declarations or bindings, depending on this type.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, InputPort>,
    /// Named scalar exports evaluated when this workflow successfully returns.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, OutputPort>,
    /// Local node definitions; vector order does not determine execution order.
    pub nodes: Vec<ProgramNode>,
    /// Directed local connections that determine execution order.
    pub edges: Vec<ProgramEdge>,
    /// Optional editor-only positions; omitted when absent and never used as a schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
}
/// Preserve the difference between an absent input default and an explicit null.
/// `'de` is Serde's input lifetime; `D` is any compatible deserializer. Wrapping the
/// parsed JSON value in `Some` lets validation see null instead of treating it as absence.
fn port_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Ok(Some(Value::deserialize(d)?))
}

impl Executor {
    fn is_once(&self) -> bool {
        matches!(self, Self::Once)
    }
}
fn is_cleanup_timeout(value: &u64) -> bool {
    *value == cleanup_timeout()
}

impl PortType {
    pub(crate) fn is_string(&self) -> bool {
        *self == Self::String
    }
}
