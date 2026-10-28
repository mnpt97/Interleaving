//! Canonical persisted scenario types and transport-facing HTTP configuration.
//!
//! Serde attributes are part of the file contract: `rename_all = "camelCase"`
//! maps Rust's snake_case fields to JSON/YAML names; `tag` selects enum variants
//! using a discriminator field; `deny_unknown_fields` rejects unsupported keys.
//! `default` supplies absent values, while `skip_serializing_if` omits empty options.
//! `BTreeMap` keeps keys sorted for deterministic comparisons and serialized output.

use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

/// Reserved declarations have no accepted nonempty representation in the current schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EmptyDeclarations;

/// Serialize the reserved declaration collection as `[]`, without allocating a vector.
/// The generic serializer returns its own output/error types through associated types.
impl Serialize for EmptyDeclarations {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_seq(Some(0))?.end()
    }
}

/// Accept only an empty array for currently unsupported declarations.
/// A local Serde `Visitor` rejects the first element rather than building an unused
/// collection. `IgnoredAny` checks for an element without interpreting its type.
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

/// Scenario-defined JSON variables used for explicit interpolation and scalar references.
/// These are ordinary configuration values, not operating-system credential lookups.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Named JSON configuration values available for explicit interpolation and references.
    #[serde(default)]
    pub variables: BTreeMap<String, Value>,
}

/// Shared transport defaults selected by a workload group.
/// Node base URL overrides affect destinations; timeout and literal headers
/// remain transport defaults unless the request or session overrides them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientProfile {
    /// Stable identifier within the containing collection; display names may change independently.
    pub id: String,
    /// Absolute HTTP(S) transport base URL, including any shared path prefix.
    pub base_url: String,
    /// Default request timeout in milliseconds; must be greater than zero.
    pub timeout_ms: u64,
    /// Literal request/profile header defaults; omitted maps deserialize as empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}

/// Select an optional shared identity for explicit identity-field references.
/// Authentication nodes own their own secret references and do not require this assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum IdentityAssignment {
    /// Use one optional named identity for this workload's explicit identity-field sources.
    Shared {
        /// Assigned identity ID; `None` means no assigned identity.
        #[serde(rename = "identityId")]
        identity_id: Option<String>,
    },
}

/// HTTP methods supported by the request model.
/// `Copy` means this small enum can be passed by value without calling `clone()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    /// Send an HTTP GET request.
    Get,
    /// Send an HTTP POST request.
    Post,
    /// Send an HTTP PUT request.
    Put,
    /// Send an HTTP PATCH request.
    Patch,
    /// Send an HTTP DELETE request.
    Delete,
}

/// Transport-facing request configuration after the engine selects a destination.
/// Unlike [`ProgramHttp`], it has no per-node base URL; the session preparation API
/// receives the resolved target separately. The engine must resolve body templates first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpConfig {
    /// HTTP method to send; login validation restricts authentication to POST/PUT/PATCH.
    pub method: HttpMethod,
    /// Request path starting with one slash, appended beneath the effective base URL prefix.
    pub path: String,
    /// Literal request/profile header defaults; omitted maps deserialize as empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// Request timeout in milliseconds; optional values inherit the client-profile default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Optional literal or explicitly templated request body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<RequestBody>,
}

/// Choose literal JSON, literal text, or an explicitly templated JSON object.
/// Literal JSON is never interpreted as reference descriptors. The Serde `kind`
/// tag makes these distinct serialized variants, avoiding ambiguous payload inference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RequestBody {
    /// Send literal JSON without interpreting embedded objects as references.
    Json {
        /// Literal payload retained exactly as authored.
        value: Value,
    },
    /// Send literal UTF-8 text.
    Text {
        /// Literal payload retained exactly as authored.
        value: String,
    },
    /// Build a JSON object from explicit template descriptors at runtime.
    JsonTemplate {
        /// Named child templates used to compose an object.
        fields: BTreeMap<String, ProgramTemplate>,
    },
}

/// Optional editor positions keyed by workflow-local node ID.
/// Layout affects presentation only; edges determine execution order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// Local node IDs mapped to finite canvas coordinates.
    #[serde(default)]
    pub positions: BTreeMap<String, Position>,
    /// Edge-local attachment sides. These affect drawing only, never execution.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub connections: BTreeMap<String, ConnectionLayout>,
}

/// Drawing attachment points for one edge, keyed by its workflow-local edge ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionLayout {
    /// Side/corner used at the source node.
    pub source: ConnectionSide,
    /// Side/corner used at the target node.
    pub target: ConnectionSide,
}

/// Supported node edges and diamond corners for canvas connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionSide {
    /// Left edge/corner.
    Left,
    /// Right edge/corner.
    Right,
    /// Top edge/corner.
    Top,
    /// Bottom edge/corner.
    Bottom,
}

/// One canvas coordinate pair.
/// `f64` is a 64-bit floating-point number; validation rejects NaN and infinity
/// before saving or executing the document.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Position {
    /// Horizontal canvas coordinate.
    pub x: f64,
    /// Vertical canvas coordinate.
    pub y: f64,
}

// Private submodules separate the domain model without introducing versioned namespaces.
// Re-exporting their types here also makes them available through the crate root.
mod nodes;
mod scenario;
mod sources;
pub use nodes::*;
pub use scenario::*;
pub use sources::*;

impl EmptyDeclarations {
    pub(crate) fn is_empty(&self) -> bool {
        true
    }
}
impl Environment {
    pub(crate) fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }
}
impl Default for IdentityAssignment {
    fn default() -> Self {
        Self::Shared { identity_id: None }
    }
}
impl IdentityAssignment {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::Shared { identity_id: None })
    }
}

impl schemars::JsonSchema for EmptyDeclarations {
    fn schema_name() -> String {
        "EmptyDeclarations".into()
    }
    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        <[(); 0]>::json_schema(generator)
    }
}
