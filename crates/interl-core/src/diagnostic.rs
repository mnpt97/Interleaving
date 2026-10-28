//! Safe error data shared by parsers, validators, CLI output, and the desktop.
//! Messages describe failures without including runtime credential or payload values.

use serde::Serialize;
use std::path::Path;

/// The processing stage responsible for a diagnostic, serialized in camelCase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCategory {
    /// The file cannot be accessed/read as required.
    Io,
    /// The file extension or requested syntax is unsupported.
    Format,
    /// The JSON/YAML text is malformed or uses a prohibited construct.
    Parse,
    /// The parsed data does not match the typed scenario shape.
    Schema,
    /// The typed document violates semantic rules.
    Validation,
    /// A configured or built-in size/count/depth budget was exceeded.
    ResourceLimit,
}

/// `fieldPath` is a JSON Pointer; `$` denotes the document root.
/// Box the payload once so Result error variants stay small as context grows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Diagnostic(Box<DiagnosticFields>);

/// Serializable diagnostic payload wrapped by [`Diagnostic`].
/// Optional locations distinguish file/parser positions from positions inside FEEL text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticFields {
    /// Processing stage that reported the problem.
    pub category: ErrorCategory,
    /// Stable machine-readable diagnostic code.
    pub code: String,
    /// Human-readable explanation that avoids embedding sensitive input values.
    pub message: String,
    /// JSON Pointer to the failing field; `$` denotes the document root.
    pub field_path: String,
    /// Optional local node ID for inspector/graph correlation.
    pub node_id: Option<String>,
    /// Optional workflow ID that scopes local node/edge identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    /// Optional source-file path for display and CLI diagnostics.
    pub file_path: Option<String>,
    /// Optional one-based line in the source document.
    pub line: Option<usize>,
    /// Optional one-based column in the source document.
    pub column: Option<usize>,
    /// One-based location within the FEEL string, never a document location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression_column: Option<usize>,
}

/// Allow `diagnostic.code` and other field access through the boxed payload.
/// Rust's `Deref` coercion borrows the inner value; it does not copy or unwrap ownership.
impl std::ops::Deref for Diagnostic {
    type Target = DiagnosticFields;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Allow exclusive field updates through the wrapper, such as adding workflow context.
/// An `&mut` borrow prevents other simultaneous access while the update occurs.
impl std::ops::DerefMut for Diagnostic {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Diagnostic {
    /// Construct a diagnostic without file, node, or expression context.
    /// An empty field path is normalized to `$` (the root). `impl Into<String>` accepts
    /// both a borrowed string literal and an already owned `String`.
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
            workflow_id: None,
            file_path: None,
            line: None,
            column: None,
            expression_line: None,
            expression_column: None,
        }))
    }
    /// Attach a one-based document line/column and return the updated diagnostic.
    /// Taking `self` by value permits builder-style chaining without a second allocation.
    pub(crate) fn at(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }
    /// Attach a displayable filesystem path and return the updated diagnostic.
    /// `to_string_lossy` safely replaces non-UTF-8 path bytes for UI output.
    pub(crate) fn with_file(mut self, path: &Path) -> Self {
        self.file_path = Some(path.to_string_lossy().into_owned());
        self
    }
}

/// Format a concise CLI error, including whichever location context is available.
/// Each `write!` uses `?` to propagate formatting failures to the caller.
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
        )?;
        if let Some(line) = self.expression_line {
            write!(
                f,
                " (expression {line}:{})",
                self.expression_column.unwrap_or(1)
            )?;
        }
        Ok(())
    }
}

/// The single-error wrapper used by load/parse/serialize APIs.
/// `thiserror` derives the standard `Error` and `Display` implementations from the attribute.
#[derive(Debug, thiserror::Error)]
#[error("{diagnostic}")]
pub struct ScenarioLoadError {
    /// Structured safe diagnostic carried by this error wrapper.
    pub diagnostic: Diagnostic,
}

/// Enable `diagnostic.into()` and automatic `?` conversion at API boundaries.
impl From<Diagnostic> for ScenarioLoadError {
    fn from(diagnostic: Diagnostic) -> Self {
        Self { diagnostic }
    }
}

/// Semantic diagnostics use the same representation as other core errors.
/// This type alias introduces no wrapper or runtime conversion.
pub type ValidationError = Diagnostic;

/// Append an escaped JSON Pointer segment (`~` → `~0`, `/` → `~1`).
/// Escaping `~` first avoids escaping the replacement sequences a second time.
pub(crate) fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}
