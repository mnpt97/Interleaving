//! Persisted authentication references and operations, never resolved secrets.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Named credential references consumed by explicit identity-field mappings.
/// The map contains descriptors only; raw credentials belong exclusively to the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// Stable identifier within the containing collection; display names may change independently.
    pub id: String,
    /// Field-name to secret-reference map; no resolved credential values belong here.
    pub credentials: BTreeMap<String, SecretReference>,
}

/// A supported external secret source, currently an operating-system environment variable.
/// This differs from ordinary scenario `environment.variables`, which are stored configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum SecretReference {
    /// Read an operating-system environment variable during engine preparation.
    Env {
        /// Name of the process environment variable; never its resolved value.
        name: String,
    },
}

/// One named, typed scalar extracted from a response body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractOutput {
    /// Restricted FEEL property path evaluated by the engine.
    pub expression: String,
    /// Expected scalar type; an omitted type means string.
    #[serde(
        rename = "type",
        default,
        skip_serializing_if = "crate::PortType::is_string"
    )]
    pub output_type: crate::PortType,
}

/// Select one HTTP response and publish named scalar outputs atomically.
/// The single-output fields remain accepted for existing files; new nodes use `outputs`.
/// Serde's `from`/`into` adapter keeps the persisted alternatives mutually exclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "ExtractDocument", into = "ExtractDocument")]
pub struct ExtractConfig {
    /// Earlier HTTP node whose retained response is read once.
    pub source_node_id: String,
    /// Single-output property path; empty for a named-output configuration.
    pub expression: String,
    /// Single-output name; empty for a named-output configuration.
    pub output: String,
    /// Single-output scalar type.
    pub output_type: crate::PortType,
    /// Named expressions and types sharing the same HTTP source.
    pub outputs: BTreeMap<String, ExtractOutput>,
}
impl ExtractConfig {
    /// Borrow every output as a name, expression, and scalar type without cloning.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str, crate::PortType)> {
        let single = self.outputs.is_empty().then_some((
            self.output.as_str(),
            self.expression.as_str(),
            self.output_type,
        ));
        single.into_iter().chain(
            self.outputs
                .iter()
                .map(|(name, value)| (name.as_str(), value.expression.as_str(), value.output_type)),
        )
    }
    /// Look up the declared type of one output for downstream source validation.
    pub fn type_of(&self, name: &str) -> Option<crate::PortType> {
        self.entries()
            .find(|(candidate, _, _)| *candidate == name)
            .map(|(_, _, kind)| kind)
    }
}

#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged, deny_unknown_fields)]
enum ExtractDocument {
    Multiple {
        #[serde(rename = "sourceNodeId")]
        source_node_id: String,
        outputs: BTreeMap<String, ExtractOutput>,
    },
    Single {
        #[serde(rename = "sourceNodeId")]
        source_node_id: String,
        expression: String,
        output: String,
        #[serde(
            rename = "type",
            default,
            skip_serializing_if = "crate::PortType::is_string"
        )]
        output_type: crate::PortType,
    },
}
impl From<ExtractDocument> for ExtractConfig {
    fn from(value: ExtractDocument) -> Self {
        match value {
            ExtractDocument::Multiple {
                source_node_id,
                outputs,
            } => Self {
                source_node_id,
                outputs,
                expression: String::new(),
                output: String::new(),
                output_type: crate::PortType::String,
            },
            ExtractDocument::Single {
                source_node_id,
                expression,
                output,
                output_type,
            } => Self {
                source_node_id,
                expression,
                output,
                output_type,
                outputs: BTreeMap::new(),
            },
        }
    }
}
impl From<ExtractConfig> for ExtractDocument {
    fn from(value: ExtractConfig) -> Self {
        if value.outputs.is_empty() && (!value.expression.is_empty() || !value.output.is_empty()) {
            Self::Single {
                source_node_id: value.source_node_id,
                expression: value.expression,
                output: value.output,
                output_type: value.output_type,
            }
        } else {
            Self::Multiple {
                source_node_id: value.source_node_id,
                outputs: value.outputs,
            }
        }
    }
}
impl schemars::JsonSchema for ExtractConfig {
    fn schema_name() -> String {
        "ExtractConfig".into()
    }
    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        ExtractDocument::json_schema(generator)
    }
}
