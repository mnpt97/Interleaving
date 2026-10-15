use super::*;
use crate::AuthPlan;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ValueKey {
    Input(String),
    Extraction(String, String),
    Call(String, String),
    Response(String),
}
impl ScalarSource {
    pub fn key(&self) -> Option<ValueKey> {
        match self {
            Self::Input { name } => Some(ValueKey::Input(name.clone())),
            Self::Extraction { node_id, name } => {
                Some(ValueKey::Extraction(node_id.clone(), name.clone()))
            }
            Self::CallOutput { node_id, name } => {
                Some(ValueKey::Call(node_id.clone(), name.clone()))
            }
            Self::Response { node_id, .. } => Some(ValueKey::Response(node_id.clone())),
            _ => None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct CompiledWorkflow {
    pub(crate) definition: WorkflowDefinition,
    pub(crate) order: Vec<usize>,
    pub(crate) auth: AuthPlan,
    pub(crate) last_uses: BTreeMap<ValueKey, String>,
}
impl CompiledWorkflow {
    pub fn definition(&self) -> &WorkflowDefinition {
        &self.definition
    }
    pub fn order(&self) -> &[usize] {
        &self.order
    }
    pub fn auth(&self) -> &AuthPlan {
        &self.auth
    }
    pub fn last_uses(&self) -> &BTreeMap<ValueKey, String> {
        &self.last_uses
    }
}
#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) document: ScenarioDocument,
    pub(crate) flows: Vec<CompiledWorkflow>,
    pub(crate) reachable: Vec<usize>,
    pub(crate) auth: AuthPlan,
}
impl Program {
    pub fn document(&self) -> &ScenarioDocument {
        &self.document
    }
    pub fn workflows(&self) -> &[CompiledWorkflow] {
        &self.flows
    }
    pub fn reachable(&self) -> &[usize] {
        &self.reachable
    }
    pub fn auth(&self) -> &AuthPlan {
        &self.auth
    }
    pub fn workflow_index(&self, id: &str) -> Option<usize> {
        self.flows.iter().position(|w| w.definition.id == id)
    }
}
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
