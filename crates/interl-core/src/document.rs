//! Shared v2 document inspection and loading.
use crate::{Diagnostic, ScenarioDocument, ScenarioFormat, ScenarioLimits, ScenarioLoadError};
use serde::Serialize;
use std::path::Path;

pub use crate::parse::{
    parse_scenario as parse_document, parse_scenario_with_limits as parse_document_with_limits,
    serialize_scenario as serialize_document,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentValidation {
    pub valid: bool,
    pub document: Option<ScenarioDocument>,
    pub diagnostics: Vec<Diagnostic>,
}
pub fn inspect_document(source: &str, format: ScenarioFormat) -> DocumentValidation {
    match parse_document(source, format) {
        Ok(document) => {
            let diagnostics = crate::compile_document(&document).err().unwrap_or_default();
            DocumentValidation {
                valid: diagnostics.is_empty(),
                document: Some(document),
                diagnostics,
            }
        }
        Err(error) => DocumentValidation {
            valid: false,
            document: None,
            diagnostics: vec![error.diagnostic],
        },
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocumentResult {
    pub path: String,
    pub format: ScenarioFormat,
    pub source: String,
    pub validation: DocumentValidation,
}
pub fn open_document(path: &Path) -> Result<OpenDocumentResult, ScenarioLoadError> {
    let (source, format) = crate::load::read_source(path, ScenarioLimits::default())?;
    let mut validation = inspect_document(&source, format);
    for diagnostic in &mut validation.diagnostics {
        diagnostic.file_path = Some(path.to_string_lossy().into_owned());
    }
    Ok(OpenDocumentResult {
        path: path.to_string_lossy().into_owned(),
        format,
        source,
        validation,
    })
}
