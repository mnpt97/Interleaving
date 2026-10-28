//! Persisted node operations, request/authentication settings, and directed graph edges.
//! Serde tags select exact node shapes; semantic compilation decides which are executable.
use crate::{ExtractConfig, HttpMethod, ProgramBody, ProgramHeader, WorkflowBinding};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// An HTTP node's persisted request configuration, including its optional target override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramHttp {
    /// Optional target default/override including its path prefix; absence inherits the enclosing default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
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
    pub body: Option<ProgramBody>,
}
/// A node-owned login request and the session it must establish.
/// Only [`crate::SecretReference`] descriptors are persisted; the engine reads
/// credential values, sends login, and replaces the session after checking success.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "AuthenticationDocument", into = "AuthenticationDocument")]
pub struct AuthenticationConfig {
    /// Optional target default/override including its path prefix; absence inherits the enclosing default.
    pub base_url: Option<String>,
    /// Request path starting with one slash, appended beneath the effective base URL prefix.
    pub path: String,
    /// HTTP method to send; login validation restricts authentication to POST/PUT/PATCH.
    pub method: HttpMethod,
    /// Secret reference for the login user/device identifier.
    pub username: crate::SecretReference,
    /// Secret reference for the login password/device credential.
    pub password: crate::SecretReference,
    /// JSON request-body property used for the user/device identifier.
    pub username_key: String,
    /// JSON request-body property used for the password/device credential.
    pub password_key: String,
    /// Whether login establishes a token header or usable response cookies.
    pub mode: AuthenticationMode,
    /// Bounded `response.body` property path used in bearer mode.
    pub token_expression: String,
    /// Session header replaced with the extracted token in bearer mode.
    pub header_name: String,
    /// Literal text prepended to the token, normally `Bearer `.
    pub header_prefix: String,
}
/// How a successful login establishes the session for following nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AuthenticationMode {
    /// Extract a response token and install the configured session header.
    Bearer,
    /// Require usable response cookies and retain their isolated jar.
    Cookie,
}
/// Internal lookup key; never persisted as an identity reference.
/// Build a stable internal credential-lookup namespace from workflow/node indices.
///
/// The repeated prefix intentionally exceeds the maximum 128-byte user identity ID,
/// so a persisted identity can never collide with a node-owned credential namespace.
/// The key contains indices only and is not persisted as a user identity reference.
pub fn authentication_key(workflow: usize, node: usize) -> String {
    format!("{}:{workflow}:{node}", "authentication".repeat(11))
}
/// An atomic session-header update: validated additions/replacements and removals.
/// The engine validates the complete operation before mutating its session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramSession {
    /// Header additions/replacements with explicit scalar descriptors.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set_headers: BTreeMap<String, ProgramHeader>,
    /// Header names to suppress/remove in the same atomic update.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove_headers: Vec<String>,
}
/// All node shapes understood by the schema.
/// `#[serde(tag = "type")]` stores the variant name beside its fields rather than
/// as a nested wrapper. Semantic compilation checks connectivity and the bounded
/// exclusive-routing contract before any of these shapes can execute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ProgramNode {
    /// Enter the workflow; exactly one Start is required.
    Start {
        /// Stable local node identifier.
        id: String,
        /// Optional display label; the node ID is shown when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Send one configured HTTP request in the current session.
    Http {
        /// Stable local node identifier.
        id: String,
        /// Referenced port/output/variable name, or this node's display name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Configuration specific to this node variant.
        config: ProgramHttp,
    },
    /// Log in with node-owned credentials and replace the previous session.
    Authentication {
        /// Stable local node identifier.
        id: String,
        /// Referenced port/output/variable name, or this node's display name.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Configuration specific to this node variant.
        config: AuthenticationConfig,
    },
    /// Read a typed scalar from an earlier HTTP response through a bounded expression.
    Extract {
        /// Stable local node identifier.
        id: String,
        /// Optional display label; the node ID is shown when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Configuration specific to this node variant.
        config: ExtractConfig,
    },
    /// Apply an atomic session-header update.
    SetSession {
        /// Stable local node identifier.
        id: String,
        /// Optional display label; the node ID is shown when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Configuration specific to this node variant.
        config: ProgramSession,
    },
    /// Invoke a declared workflow in the same VU session with a new local frame.
    Call {
        /// Stable local node identifier.
        id: String,
        /// Optional display label; the node ID is shown when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Configuration specific to this node variant.
        config: WorkflowBinding,
    },
    /// Return the workflow's declared outputs.
    End {
        /// Stable local node identifier.
        id: String,
        /// Optional display label; the node ID is shown when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Select exactly one conditional/default route in a structured region.
    ExclusiveSplit {
        /// Stable local node identifier.
        id: String,
        /// Optional display label; the node ID is shown when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Configuration specific to this node variant.
        config: SplitConfig,
    },
    /// Rejoin the selected arm of the paired exclusive split.
    ExclusiveMerge {
        /// Stable local node identifier.
        id: String,
        /// Optional display label; the node ID is shown when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// Configuration specific to this node variant.
        #[serde(default, skip_serializing_if = "MergeConfig::is_empty")]
        config: MergeConfig,
    },
}
/// Response source and explicit scalar bindings for a structured exclusive split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitConfig {
    /// Explicit scalar bindings exposed as inputs.<name> in route conditions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, crate::ScalarSource>,
    /// Local producer node whose response/status is consumed by this configuration.
    pub source_node_id: String,
}
/// Join paired with an exclusive split; it never schedules parallel work.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeConfig {}
impl ProgramNode {
    /// Borrow the stable ID carried by every node variant.
    /// The `|` patterns share one return expression; `..` ignores fields irrelevant here.
    pub fn id(&self) -> &str {
        match self {
            Self::Start { id, .. }
            | Self::Http { id, .. }
            | Self::Authentication { id, .. }
            | Self::Extract { id, .. }
            | Self::SetSession { id, .. }
            | Self::Call { id, .. }
            | Self::End { id, .. }
            | Self::ExclusiveSplit { id, .. }
            | Self::ExclusiveMerge { id, .. } => id,
        }
    }
    /// Optional display label shared by every node kind; IDs remain reference keys.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Start { name, .. }
            | Self::End { name, .. }
            | Self::Http { name, .. }
            | Self::Authentication { name, .. }
            | Self::Extract { name, .. }
            | Self::SetSession { name, .. }
            | Self::Call { name, .. }
            | Self::ExclusiveSplit { name, .. }
            | Self::ExclusiveMerge { name, .. } => name.as_deref(),
        }
    }
    /// Return the serialized node kind for diagnostics, safe graphs, and execution events.
    /// `&'static str` borrows a string literal that remains valid for the whole program;
    /// no new `String` is allocated on each call.
    /// This exhaustive `match` also makes new enum variants require an explicit kind.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Start { .. } => "start",
            Self::Http { .. } => "http",
            Self::Authentication { .. } => "authentication",
            Self::Extract { .. } => "extract",
            Self::SetSession { .. } => "setSession",
            Self::Call { .. } => "call",
            Self::End { .. } => "end",
            Self::ExclusiveSplit { .. } => "exclusiveSplit",
            Self::ExclusiveMerge { .. } => "exclusiveMerge",
        }
    }
}
/// A directed connection between local node IDs, with a stable edge ID.
/// Conditions/default routes are permitted only on exclusive split outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgramEdge {
    /// Stable identifier within the containing collection; display names may change independently.
    pub id: String,
    /// Local source-node ID.
    pub from: String,
    /// Local destination-node ID.
    pub to: String,
    /// Optional bounded, read-only FEEL predicate on a split output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<RouteCondition>,
    /// Fallback-branch marker, serialized as `default`; only true is admitted.
    #[serde(default, rename = "default", skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}
/// Restricted route-expression source text, compiled before run preparation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RouteCondition {
    /// Restricted FEEL expression text; compilation admits only the supported subset.
    pub expression: String,
}

// The persisted union excludes bearer-only settings from cookie authentication.
// The runtime struct above remains a normalized view with inexpensive direct access.
#[derive(Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
enum AuthenticationDocument {
    Bearer {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "baseUrl")]
        base_url: Option<String>,
        path: String,
        method: HttpMethod,
        username: crate::SecretReference,
        password: crate::SecretReference,
        #[serde(rename = "usernameKey")]
        username_key: String,
        #[serde(rename = "passwordKey")]
        password_key: String,
        #[serde(rename = "tokenExpression")]
        token_expression: String,
        #[serde(
            default = "default_header_name",
            skip_serializing_if = "is_default_header_name"
        )]
        #[serde(rename = "headerName")]
        header_name: String,
        #[serde(
            default = "default_header_prefix",
            skip_serializing_if = "is_default_header_prefix"
        )]
        #[serde(rename = "headerPrefix")]
        header_prefix: String,
    },
    Cookie {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "baseUrl")]
        base_url: Option<String>,
        path: String,
        method: HttpMethod,
        username: crate::SecretReference,
        password: crate::SecretReference,
        #[serde(rename = "usernameKey")]
        username_key: String,
        #[serde(rename = "passwordKey")]
        password_key: String,
    },
}
fn default_header_name() -> String {
    "Authorization".into()
}
fn default_header_prefix() -> String {
    "Bearer ".into()
}
fn is_default_header_name(value: &str) -> bool {
    value == "Authorization"
}
fn is_default_header_prefix(value: &str) -> bool {
    value == "Bearer "
}
impl From<AuthenticationDocument> for AuthenticationConfig {
    fn from(value: AuthenticationDocument) -> Self {
        match value {
            AuthenticationDocument::Bearer {
                base_url,
                path,
                method,
                username,
                password,
                username_key,
                password_key,
                token_expression,
                header_name,
                header_prefix,
            } => Self {
                base_url,
                path,
                method,
                username,
                password,
                username_key,
                password_key,
                token_expression,
                header_name,
                header_prefix,
                mode: AuthenticationMode::Bearer,
            },
            AuthenticationDocument::Cookie {
                base_url,
                path,
                method,
                username,
                password,
                username_key,
                password_key,
            } => Self {
                base_url,
                path,
                method,
                username,
                password,
                username_key,
                password_key,
                mode: AuthenticationMode::Cookie,
                token_expression: String::new(),
                header_name: String::new(),
                header_prefix: String::new(),
            },
        }
    }
}
impl From<AuthenticationConfig> for AuthenticationDocument {
    fn from(value: AuthenticationConfig) -> Self {
        match value.mode {
            AuthenticationMode::Bearer => Self::Bearer {
                base_url: value.base_url,
                path: value.path,
                method: value.method,
                username: value.username,
                password: value.password,
                username_key: value.username_key,
                password_key: value.password_key,
                token_expression: value.token_expression,
                header_name: value.header_name,
                header_prefix: value.header_prefix,
            },
            AuthenticationMode::Cookie => Self::Cookie {
                base_url: value.base_url,
                path: value.path,
                method: value.method,
                username: value.username,
                password: value.password,
                username_key: value.username_key,
                password_key: value.password_key,
            },
        }
    }
}

impl MergeConfig {
    fn is_empty(&self) -> bool {
        true
    }
}

impl schemars::JsonSchema for AuthenticationConfig {
    fn schema_name() -> String {
        "AuthenticationConfig".into()
    }
    fn json_schema(generator: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        AuthenticationDocument::json_schema(generator)
    }
}
