//! Borrowed scalar descriptors shared by validation of bindings, templates, and headers.
//!
//! Persisted models own their strings/JSON. This view lets compiler passes inspect
//! those values without cloning them; only retained plan keys need owned strings.

use crate::{
    InitializePhase, ProgramHeader, ProgramTemplate, ResponseField, ScalarSource, ValueKey,
};
use serde_json::Value;

/// A scalar descriptor borrowing all data from its persisted owner.
/// Text literals let headers participate without allocating a temporary JSON string.
#[derive(Clone, Copy)]
pub(crate) enum ScalarRef<'a> {
    Literal {
        value: &'a Value,
    },
    Text {
        value: &'a str,
    },
    Environment {
        name: &'a str,
    },
    Identity {
        field: &'a str,
    },
    Input {
        name: &'a str,
    },
    Extraction {
        node_id: &'a str,
        name: &'a str,
    },
    CallOutput {
        node_id: &'a str,
        name: &'a str,
    },
    Response {
        node_id: &'a str,
        field: &'a ResponseField,
    },
    Lifecycle {
        phase: &'a InitializePhase,
        name: &'a str,
    },
}

impl<'a> From<&'a ScalarSource> for ScalarRef<'a> {
    fn from(source: &'a ScalarSource) -> Self {
        match source {
            ScalarSource::Literal { value } => Self::Literal { value },
            ScalarSource::Environment { name } => Self::Environment { name },
            ScalarSource::Identity { field } => Self::Identity { field },
            ScalarSource::Input { name } => Self::Input { name },
            ScalarSource::Extraction { node_id, name } => Self::Extraction { node_id, name },
            ScalarSource::CallOutput { node_id, name } => Self::CallOutput { node_id, name },
            ScalarSource::Response { node_id, field } => Self::Response { node_id, field },
            ScalarSource::Lifecycle { phase, name } => Self::Lifecycle { phase, name },
        }
    }
}

impl<'a> ScalarRef<'a> {
    /// Borrow a template leaf; structured containers must be traversed separately.
    pub(crate) fn template(template: &'a ProgramTemplate) -> Option<Self> {
        Some(match template {
            ProgramTemplate::Literal { value } => Self::Literal { value },
            ProgramTemplate::Environment { name } => Self::Environment { name },
            ProgramTemplate::Identity { field } => Self::Identity { field },
            ProgramTemplate::Input { name } => Self::Input { name },
            ProgramTemplate::Extraction { node_id, name } => Self::Extraction { node_id, name },
            ProgramTemplate::CallOutput { node_id, name } => Self::CallOutput { node_id, name },
            ProgramTemplate::Response { node_id, field } => Self::Response { node_id, field },
            ProgramTemplate::Object { .. } | ProgramTemplate::Array { .. } => return None,
        })
    }

    /// Borrow the scalar source and prefix of a session header.
    pub(crate) fn header(header: &'a ProgramHeader) -> (Self, &'a str) {
        match header {
            ProgramHeader::Literal { value, prefix } => (Self::Text { value }, prefix),
            ProgramHeader::Environment { name, prefix } => (Self::Environment { name }, prefix),
            ProgramHeader::Identity { field, prefix } => (Self::Identity { field }, prefix),
            ProgramHeader::Input { name, prefix } => (Self::Input { name }, prefix),
            ProgramHeader::Extraction {
                node_id,
                name,
                prefix,
            } => (Self::Extraction { node_id, name }, prefix),
            ProgramHeader::CallOutput {
                node_id,
                name,
                prefix,
            } => (Self::CallOutput { node_id, name }, prefix),
        }
    }

    /// Allocate only the local key that the immutable plan must retain.
    pub(crate) fn key(self) -> Option<ValueKey> {
        match self {
            Self::Input { name } => Some(ValueKey::Input(name.into())),
            Self::Extraction { node_id, name } => {
                Some(ValueKey::Extraction(node_id.into(), name.into()))
            }
            Self::CallOutput { node_id, name } => Some(ValueKey::Call(node_id.into(), name.into())),
            Self::Response { node_id, .. } => Some(ValueKey::Response(node_id.into())),
            _ => None,
        }
    }

    /// Materialize a descriptor for APIs whose callers explicitly need owned data.
    pub(crate) fn into_owned(self) -> ScalarSource {
        match self {
            Self::Literal { value } => ScalarSource::Literal {
                value: value.clone(),
            },
            Self::Text { value } => ScalarSource::Literal {
                value: Value::String(value.into()),
            },
            Self::Environment { name } => ScalarSource::Environment { name: name.into() },
            Self::Identity { field } => ScalarSource::Identity {
                field: field.into(),
            },
            Self::Input { name } => ScalarSource::Input { name: name.into() },
            Self::Extraction { node_id, name } => ScalarSource::Extraction {
                node_id: node_id.into(),
                name: name.into(),
            },
            Self::CallOutput { node_id, name } => ScalarSource::CallOutput {
                node_id: node_id.into(),
                name: name.into(),
            },
            Self::Response { node_id, field } => ScalarSource::Response {
                node_id: node_id.into(),
                field: field.clone(),
            },
            Self::Lifecycle { phase, name } => ScalarSource::Lifecycle {
                phase: phase.clone(),
                name: name.into(),
            },
        }
    }
}
