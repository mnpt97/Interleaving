use crate::{ExtractConfig, Identity, SetSessionConfig, TemplateValue};
use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
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
    pub groups: Vec<UserGroup>,
    pub workflow: Workflow,
    pub assertions: EmptyDeclarations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Layout>,
}

/// Reserved declarations have no accepted nonempty representation in the current schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmptyDeclarations;

impl Serialize for EmptyDeclarations {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_seq(Some(0))?.end()
    }
}

impl<'de> Deserialize<'de> for EmptyDeclarations {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Empty;
        impl<'de> Visitor<'de> for Empty {
            type Value = EmptyDeclarations;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an empty array; declarations are unsupported in the current schema")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                if seq.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom(
                        "nonempty declarations are unsupported in the current schema",
                    ));
                }
                Ok(EmptyDeclarations)
            }
        }
        deserializer.deserialize_seq(Empty)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProfile {
    pub id: String,
    pub base_url: String,
    pub timeout_ms: u64,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserGroup {
    pub id: String,
    pub client_profile_id: String,
    pub count: u32,
    pub identity_assignment: IdentityAssignment,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum IdentityAssignment {
    Shared {
        #[serde(rename = "identityId")]
        identity_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkflowNode {
    Start {
        id: String,
    },
    Http {
        id: String,
        config: HttpConfig,
    },
    Extract {
        id: String,
        config: ExtractConfig,
    },
    SetSession {
        id: String,
        config: SetSessionConfig,
    },
    End {
        id: String,
    },
}

impl WorkflowNode {
    pub fn id(&self) -> &str {
        match self {
            Self::Start { id }
            | Self::Http { id, .. }
            | Self::Extract { id, .. }
            | Self::SetSession { id, .. }
            | Self::End { id } => id,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Http { .. } => "http",
            Self::Extract { .. } => "extract",
            Self::SetSession { .. } => "setSession",
            Self::End { .. } => "end",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpConfig {
    pub method: HttpMethod,
    pub path: String,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<RequestBody>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RequestBody {
    Json {
        value: Value,
    },
    Text {
        value: String,
    },
    JsonTemplate {
        fields: BTreeMap<String, TemplateValue>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default)]
    pub positions: BTreeMap<String, Position>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}
