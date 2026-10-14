use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCategory {
    Io,
    Format,
    Parse,
    Schema,
    Validation,
    ResourceLimit,
}

/// `fieldPath` is a JSON Pointer; `$` denotes the document root.
/// Box the payload once so Result error variants stay small as context grows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Diagnostic(Box<DiagnosticFields>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFields {
    pub category: ErrorCategory,
    pub code: String,
    pub message: String,
    pub field_path: String,
    pub node_id: Option<String>,
    pub file_path: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl std::ops::Deref for Diagnostic {
    type Target = DiagnosticFields;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Diagnostic {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Diagnostic {
    pub fn new(
        category: ErrorCategory,
        code: &str,
        path: &str,
        message: impl Into<String>,
    ) -> Self {
        Self(Box::new(DiagnosticFields {
            category,
            code: code.into(),
            message: message.into(),
            field_path: if path.is_empty() {
                "$".into()
            } else {
                path.into()
            },
            node_id: None,
            file_path: None,
            line: None,
            column: None,
        }))
    }
    pub(crate) fn at(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }
    pub(crate) fn with_file(mut self, path: &Path) -> Self {
        self.file_path = Some(path.to_string_lossy().into_owned());
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(path) = &self.file_path {
            write!(f, "{path}")?;
        }
        if let Some(line) = self.line {
            write!(f, ":{line}:{}", self.column.unwrap_or(1))?;
        }
        write!(
            f,
            " [{:?}/{}] {}: {}",
            self.category, self.code, self.field_path, self.message
        )
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{diagnostic}")]
pub struct ScenarioLoadError {
    pub diagnostic: Diagnostic,
}

impl From<Diagnostic> for ScenarioLoadError {
    fn from(diagnostic: Diagnostic) -> Self {
        Self { diagnostic }
    }
}

pub type ValidationError = Diagnostic;

pub(crate) fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
