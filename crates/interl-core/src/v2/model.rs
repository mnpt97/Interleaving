use crate::{
    ClientProfile, EmptyDeclarations, Environment, ExtractConfig, HttpMethod, Identity,
    IdentityAssignment, Layout,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub environment: Environment,
    pub client_profiles: Vec<ClientProfile>,
    pub identities: Vec<Identity>,
    pub groups: Vec<WorkloadGroup>,
    pub workflows: Vec<WorkflowDefinition>,
    pub assertions: EmptyDeclarations,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkloadGroup {
    pub id: String,
    pub client_profile_id: String,
    pub identity_assignment: IdentityAssignment,
    pub executor: Executor,
    pub lifecycle: Lifecycle,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Executor {
    Once,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lifecycle {
    #[serde(default)]
    pub initialize: Option<WorkflowBinding>,
    pub iteration: WorkflowBinding,
    #[serde(default)]
    pub cleanup: Option<WorkflowBinding>,
    #[serde(default = "cleanup_timeout")]
    pub cleanup_timeout_ms: u64,
}
fn cleanup_timeout() -> u64 {
    2000
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowBinding {
    pub workflow_id: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, ScalarSource>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortType {
    String,
    Integer,
    Boolean,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputPort {
    #[serde(rename = "type")]
    pub kind: PortType,
    #[serde(
        default,
        deserialize_with = "port_default",
        skip_serializing_if = "Option::is_none"
    )]
    pub default: Option<Value>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    #[serde(rename = "type")]
    pub kind: PortType,
    pub value: ScalarSource,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, InputPort>,
    #[serde(default)]
    pub outputs: BTreeMap<String, OutputPort>,
    pub nodes: Vec<ProgramNode>,
    pub edges: Vec<ProgramEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScalarSource {
    Literal {
        value: Value,
    },
    Environment {
        name: String,
    },
    Identity {
        field: String,
    },
    Input {
        name: String,
    },
    Extraction {
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
    },
    CallOutput {
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
    },
    Response {
        #[serde(rename = "nodeId")]
        node_id: String,
        field: ResponseField,
    },
    Lifecycle {
        phase: InitializePhase,
        name: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResponseField {
    Status,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InitializePhase {
    Initialize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProgramTemplate {
    Literal {
        value: Value,
    },
    Environment {
        name: String,
    },
    Identity {
        field: String,
    },
    Input {
        name: String,
    },
    Extraction {
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
    },
    CallOutput {
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
    },
    Response {
        #[serde(rename = "nodeId")]
        node_id: String,
        field: ResponseField,
    },
    Object {
        fields: BTreeMap<String, ProgramTemplate>,
    },
    Array {
        items: Vec<ProgramTemplate>,
    },
}
impl ProgramTemplate {
    pub fn scalar(&self) -> Option<ScalarSource> {
        Some(match self {
            Self::Literal { value } => ScalarSource::Literal {
                value: value.clone(),
            },
            Self::Environment { name } => ScalarSource::Environment { name: name.clone() },
            Self::Identity { field } => ScalarSource::Identity {
                field: field.clone(),
            },
            Self::Input { name } => ScalarSource::Input { name: name.clone() },
            Self::Extraction { node_id, name } => ScalarSource::Extraction {
                node_id: node_id.clone(),
                name: name.clone(),
            },
            Self::CallOutput { node_id, name } => ScalarSource::CallOutput {
                node_id: node_id.clone(),
                name: name.clone(),
            },
            Self::Response { node_id, field } => ScalarSource::Response {
                node_id: node_id.clone(),
                field: field.clone(),
            },
            Self::Object { .. } | Self::Array { .. } => return None,
        })
    }
}
// Header sources deliberately exclude structured and response-status variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProgramHeader {
    Literal {
        value: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    Environment {
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    Identity {
        field: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    Input {
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    Extraction {
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    CallOutput {
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
}
impl ProgramHeader {
    pub fn parts(&self) -> (ScalarSource, &str) {
        match self {
            Self::Literal { value, prefix } => (
                ScalarSource::Literal {
                    value: Value::String(value.clone()),
                },
                prefix,
            ),
            Self::Environment { name, prefix } => {
                (ScalarSource::Environment { name: name.clone() }, prefix)
            }
            Self::Identity { field, prefix } => (
                ScalarSource::Identity {
                    field: field.clone(),
                },
                prefix,
            ),
            Self::Input { name, prefix } => (ScalarSource::Input { name: name.clone() }, prefix),
            Self::Extraction {
                node_id,
                name,
                prefix,
            } => (
                ScalarSource::Extraction {
                    node_id: node_id.clone(),
                    name: name.clone(),
                },
                prefix,
            ),
            Self::CallOutput {
                node_id,
                name,
                prefix,
            } => (
                ScalarSource::CallOutput {
                    node_id: node_id.clone(),
                    name: name.clone(),
                },
                prefix,
            ),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProgramBody {
    Json {
        value: Value,
    },
    Text {
        value: String,
    },
    JsonTemplate {
        fields: BTreeMap<String, ProgramTemplate>,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramHttp {
    pub method: HttpMethod,
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<ProgramBody>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramSession {
    #[serde(default)]
    pub set_headers: BTreeMap<String, ProgramHeader>,
    #[serde(default)]
    pub remove_headers: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProgramNode {
    Start { id: String },
    Http { id: String, config: ProgramHttp },
    Extract { id: String, config: ExtractConfig },
    SetSession { id: String, config: ProgramSession },
    Call { id: String, config: WorkflowBinding },
    End { id: String },
    ExclusiveSplit { id: String, config: SplitConfig },
    ExclusiveMerge { id: String, config: MergeConfig },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitConfig {
    pub source_node_id: String,
    pub merge_node_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeConfig {
    pub split_node_id: String,
}
impl ProgramNode {
    pub fn id(&self) -> &str {
        match self {
            Self::Start { id }
            | Self::Http { id, .. }
            | Self::Extract { id, .. }
            | Self::SetSession { id, .. }
            | Self::Call { id, .. }
            | Self::End { id }
            | Self::ExclusiveSplit { id, .. }
            | Self::ExclusiveMerge { id, .. } => id,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Http { .. } => "http",
            Self::Extract { .. } => "extract",
            Self::SetSession { .. } => "setSession",
            Self::Call { .. } => "call",
            Self::End { .. } => "end",
            Self::ExclusiveSplit { .. } => "exclusiveSplit",
            Self::ExclusiveMerge { .. } => "exclusiveMerge",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<RouteCondition>,
    #[serde(default, rename = "default", skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteCondition {
    pub expression: String,
}

fn port_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Ok(Some(Value::deserialize(d)?))
}
