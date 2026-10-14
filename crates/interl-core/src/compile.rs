use crate::{validate_scenario, ScenarioDocument, ValidationError, WorkflowNode};
use serde::Serialize;
use std::collections::BTreeMap;

/// Validated workflow only. No sessions, resolved secrets, layout, or execution.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlan {
    schema_version: u32,
    scenario_id: String,
    steps: Vec<WorkflowNode>,
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
}

pub fn compile_scenario(
    scenario: &ScenarioDocument,
) -> Result<ExecutionPlan, Vec<ValidationError>> {
    validate_scenario(scenario)?;
    let nodes: BTreeMap<_, _> = scenario
        .workflow
        .nodes
        .iter()
        .map(|node| (node.id(), node))
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
        .find(|node| matches!(node, WorkflowNode::Start { .. }));
    let mut steps = Vec::new();
    while let Some(node) = current {
        steps.push(node.clone());
        current = edges.get(node.id()).and_then(|id| nodes.get(id)).copied();
    }
    Ok(ExecutionPlan {
        schema_version: scenario.schema_version,
        scenario_id: scenario.id.clone(),
        steps,
    })
}
