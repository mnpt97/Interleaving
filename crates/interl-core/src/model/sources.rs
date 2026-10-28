//! Explicit value-source descriptors for workflow scalars, templates, and session headers.
//! These declarations describe how the engine obtains values; they never resolve secrets.
use crate::RequestBody;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Describe a scalar producer without storing a resolved credential or runtime value.
/// Each enum variant carries only the fields valid for that `source` tag. References
/// to local producers must be earlier in the same invocation; lifecycle access is restricted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScalarSource {
    /// Use a scalar literal directly.
    Literal {
        /// Literal payload retained exactly as authored.
        value: Value,
    },
    /// Read a named value from scenario configuration variables.
    Environment {
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read a credential field from the workload's assigned identity.
    Identity {
        /// Credential or response-metadata field selected by this source.
        field: String,
    },
    /// Read an input of the current workflow invocation.
    Input {
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read a named string from an earlier local extraction.
    Extraction {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read a declared export from an earlier local workflow call.
    CallOutput {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read supported metadata from an earlier local HTTP response.
    Response {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Credential or response-metadata field selected by this source.
        field: ResponseField,
    },
    /// Read a declared initialization export in an allowed lifecycle binding.
    Lifecycle {
        /// Lifecycle phase that owns the referenced export.
        phase: InitializePhase,
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
}
/// HTTP response metadata currently available as a scalar source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ResponseField {
    /// The integer HTTP response status code.
    Status,
}
/// The lifecycle phase whose exports may feed iteration inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum InitializePhase {
    /// The successful initialization phase.
    Initialize,
}
/// Recursive JSON composition whose leaf values may be explicit scalar references.
/// `Object` and `Array` own recursive collections, so the enum has a finite size
/// without requiring a `Box` around every child; containers allocate their elements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProgramTemplate {
    /// Embed literal JSON as-is, including structured payloads.
    Literal {
        /// Literal payload retained exactly as authored.
        value: Value,
    },
    /// Read a named scenario variable.
    Environment {
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read an assigned identity credential field in Rust.
    Identity {
        /// Credential or response-metadata field selected by this source.
        field: String,
    },
    /// Read a current-invocation input.
    Input {
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read an earlier local extraction output.
    Extraction {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read an earlier local call's declared output.
    CallOutput {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
    },
    /// Read supported metadata from an earlier HTTP response.
    Response {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Credential or response-metadata field selected by this source.
        field: ResponseField,
    },
    /// Recursively compose object properties.
    Object {
        /// Named child templates used to compose an object.
        fields: BTreeMap<String, ProgramTemplate>,
    },
    /// Recursively compose array elements, preserving their order.
    Array {
        /// Child templates used to compose an array.
        items: Vec<ProgramTemplate>,
    },
}
impl ProgramTemplate {
    /// Convert a template leaf into a general scalar source.
    /// Structured object/array nodes return `None`; callers must walk their children.
    /// `&self` only borrows the template, so owned strings/JSON are cloned into the result.
    pub fn scalar(&self) -> Option<ScalarSource> {
        crate::scalar::ScalarRef::template(self).map(crate::scalar::ScalarRef::into_owned)
    }
}
// Header sources deliberately exclude structured and response-status variants.
/// String-valued session-header sources with an optional literal prefix.
/// Structured bodies and integer response status deliberately have no header variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProgramHeader {
    /// Use an explicit string and prefix.
    Literal {
        /// Literal payload retained exactly as authored.
        value: String,
        /// Literal text prepended to the resolved header string; defaults to empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    /// Read a scenario string variable.
    Environment {
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
        /// Literal text prepended to the resolved header string; defaults to empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    /// Read an assigned identity credential field in Rust.
    Identity {
        /// Credential or response-metadata field selected by this source.
        field: String,
        /// Literal text prepended to the resolved header string; defaults to empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    /// Read a string input in this invocation.
    Input {
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
        /// Literal text prepended to the resolved header string; defaults to empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    /// Read a string from an earlier extraction.
    Extraction {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
        /// Literal text prepended to the resolved header string; defaults to empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
    /// Read a declared string export from an earlier workflow call.
    CallOutput {
        /// Stable ID of an earlier producer in the same workflow.
        #[serde(rename = "nodeId")]
        node_id: String,
        /// Referenced port/output/variable name, or this node's display name.
        name: String,
        /// Literal text prepended to the resolved header string; defaults to empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        prefix: String,
    },
}
impl ProgramHeader {
    /// Return the owned scalar descriptor and a borrowed prefix for a header source.
    /// The returned `&str` borrows from `self`; Rust infers that lifetime and prevents
    /// the reference from outliving the header descriptor.
    pub fn parts(&self) -> (ScalarSource, &str) {
        let (source, prefix) = crate::scalar::ScalarRef::header(self);
        (source.into_owned(), prefix)
    }
}
/// Request bodies use one canonical representation in documents and transport configuration.
/// This alias introduces no new type, allocation, or serialized wrapper.
pub type ProgramBody = RequestBody;
