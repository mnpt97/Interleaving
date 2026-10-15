//! Persisted authentication references and operations, never resolved secrets.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub credentials: BTreeMap<String, SecretReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum SecretReference {
    Env { name: String },
}

/// Explicit composition; a literal object is never recursively interpreted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum TemplateValue {
    Literal {
        value: Value,
    },
    Identity {
        field: String,
    },
    Extraction {
        #[serde(rename = "nodeId")]
        node_id: String,
        name: String,
    },
    Object {
        fields: BTreeMap<String, TemplateValue>,
    },
    Array {
        items: Vec<TemplateValue>,
    },
}

/// Session headers are string-only; structured template sources are excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum SessionHeaderValue {
    Literal {
        value: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    Identity {
        field: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractConfig {
    pub source_node_id: String,
    pub expression: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetSessionConfig {
    #[serde(default)]
    pub set_headers: BTreeMap<String, SessionHeaderValue>,
    #[serde(default)]
    pub remove_headers: Vec<String>,
}
