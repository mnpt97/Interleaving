use crate::{CompiledExpression, ScenarioDocument, SecretReference, ValidationError, WorkflowNode};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

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
    pub fn required_secrets(&self) -> &[RequiredSecret] {
        &self.required_secrets
    }
    pub fn extractions(&self) -> &BTreeMap<String, CompiledExtraction> {
        &self.extractions
    }
    pub fn sensitive_http_nodes(&self) -> &BTreeSet<String> {
        &self.sensitive_http_nodes
    }
    pub fn response_last_uses(&self) -> &BTreeMap<String, String> {
        &self.response_last_uses
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredSecret {
    pub(crate) identity_id: String,
    pub(crate) field: String,
    pub(crate) reference: SecretReference,
}
impl RequiredSecret {
    pub fn identity_id(&self) -> &str {
        &self.identity_id
    }
    pub fn field(&self) -> &str {
        &self.field
    }
    pub fn reference(&self) -> &SecretReference {
        &self.reference
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledExtraction {
    pub(crate) source_node_id: String,
    pub(crate) output: String,
    pub(crate) expression: CompiledExpression,
    pub(crate) last_use_node_id: String,
}
impl CompiledExtraction {
    pub fn source_node_id(&self) -> &str {
        &self.source_node_id
    }
    pub fn output(&self) -> &str {
        &self.output
    }
    pub fn expression(&self) -> &CompiledExpression {
        &self.expression
    }
    pub fn last_use_node_id(&self) -> &str {
        &self.last_use_node_id
    }
}

/// Validated workflow only. No sessions, resolved secrets, layout, or execution.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlan {
    schema_version: u32,
    scenario_id: String,
    steps: Vec<WorkflowNode>,
    auth: AuthPlan,
}

impl ExecutionPlan {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }
    pub fn steps(&self) -> &[WorkflowNode] {
        &self.steps
    }
    pub fn auth(&self) -> &AuthPlan {
        &self.auth
    }
}

pub fn compile_scenario(
    scenario: &ScenarioDocument,
) -> Result<ExecutionPlan, Vec<ValidationError>> {
    let auth = crate::validate::validate_and_compile_auth(scenario)?;
    let steps = ordered_indices(scenario)
        .into_iter()
        .map(|index| scenario.workflow.nodes[index].clone())
        .collect();
    Ok(ExecutionPlan {
        schema_version: scenario.schema_version,
        scenario_id: scenario.id.clone(),
        steps,
        auth,
    })
}

/// Caller must first validate the unique, connected, acyclic linear graph.
pub(crate) fn ordered_indices(scenario: &ScenarioDocument) -> Vec<usize> {
    let nodes: BTreeMap<_, _> = scenario
        .workflow
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id(), index))
        .collect();
    let edges: BTreeMap<_, _> = scenario
        .workflow
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str()))
        .collect();
    let mut current = scenario
        .workflow
        .nodes
        .iter()
        .position(|node| matches!(node, WorkflowNode::Start { .. }));
    let mut indices = Vec::new();
    while let Some(index) = current {
        indices.push(index);
        current = edges
            .get(scenario.workflow.nodes[index].id())
            .and_then(|id| nodes.get(id))
            .copied();
    }
    indices
}
