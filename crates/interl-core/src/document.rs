//! Editor-facing inspection, file provenance, and incomplete-draft persistence.
use crate::{Diagnostic, ScenarioDocument, ScenarioFormat, ScenarioLimits, ScenarioLoadError};
use serde::Serialize;
use std::path::Path;

pub use crate::parse::{
    parse_scenario as parse_document, parse_scenario_with_limits as parse_document_with_limits,
    serialize_scenario as serialize_document,
};

/// Inspection outcome used by the editor.
/// A typed document can be present even when `valid` is false; syntax/schema failures
/// leave it absent, while semantic failures retain the draft for repair.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentValidation {
    /// Whether parsing and all semantic checks succeeded.
    pub valid: bool,
    /// Typed document, when parsing established its shape; semantic errors do not discard it.
    pub document: Option<ScenarioDocument>,
    /// Safe parsing/compilation failures to display in the editor.
    pub diagnostics: Vec<Diagnostic>,
}
/// Parse an in-memory document and collect semantic diagnostics.
///
/// This is deliberately not a `Result`: both successful and unsuccessful inspections
/// are useful UI data. No filesystem, credential lookup, or HTTP is involved.
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
/// A loaded file together with original source/provenance and its inspection result.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDocumentResult {
    /// Original filesystem path; separate from any later JSON save destination.
    pub path: String,
    /// Input syntax selected from the file extension.
    pub format: ScenarioFormat,
    /// Original file text preserved for provenance; loading does not rewrite it.
    pub source: String,
    /// Typed-draft inspection result, including semantic diagnostics.
    pub validation: DocumentValidation,
}
/// Read a file and inspect it while preserving its original text and format.
///
/// # Errors
/// Returns file/format/read failures. Content problems are kept inside `validation`
/// so the editor can show diagnostics; their file paths are populated here.
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

/// Serialize a typed editor draft without requiring a runnable workflow.
///
/// Workflow/node/edge/port counts and finite coordinates are checked before
/// serialization; parsing the result reapplies byte, nesting, collection, and schema
/// limits. This intentionally differs from [`crate::serialize_document`], which
/// also requires semantic compilation.
///
/// # Errors
/// Returns the first structural, resource-limit, or serialization diagnostic.
pub fn serialize_draft(document: &ScenarioDocument) -> Result<String, ScenarioLoadError> {
    if document.workflows.len() > 32
        || document
            .workflows
            .iter()
            .map(|w| w.nodes.len())
            .sum::<usize>()
            > 256
        || document
            .workflows
            .iter()
            .map(|w| w.edges.len())
            .sum::<usize>()
            > 1024
        || document
            .workflows
            .iter()
            .any(|w| w.inputs.len() > 32 || w.outputs.len() > 32)
    {
        return Err(Diagnostic::new(
            crate::ErrorCategory::ResourceLimit,
            "draftResourceLimit",
            "/workflows",
            "Workflow, node, edge, or port limits exceeded.",
        )
        .into());
    }
    for workflow in &document.workflows {
        if workflow.layout.as_ref().is_some_and(|layout| {
            layout
                .positions
                .values()
                .any(|p| !p.x.is_finite() || !p.y.is_finite())
        }) {
            return Err(Diagnostic::new(
                crate::ErrorCategory::Schema,
                "invalidCoordinate",
                "/workflows/layout",
                "Layout coordinates must be finite.",
            )
            .into());
        }
    }
    let source = serde_json::to_string_pretty(document).map_err(|_| {
        Diagnostic::new(
            crate::ErrorCategory::Schema,
            "serialization",
            "",
            "Draft cannot be serialized.",
        )
    })?;
    parse_document(&source, ScenarioFormat::Json)?;
    Ok(source)
}

#[cfg(test)]
mod draft_tests {
    use super::*;
    #[test]
    fn draft_roundtrip_keeps_incomplete_semantics_but_normal_save_rejects() {
        let mut document = parse_document(
            include_str!("../../../scenarios/examples/hello-api.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        document.workflows[0].edges.clear();
        assert!(serialize_document(&document).is_err());
        let mut oversized = document.clone();
        oversized.workflows = vec![document.workflows[0].clone(); 33];
        assert!(serialize_draft(&oversized).is_err());
        let source = serialize_draft(&document).unwrap();
        let inspected = inspect_document(&source, ScenarioFormat::Json);
        assert!(!inspected.valid);
        assert_eq!(inspected.document.unwrap(), document);
        document.workflows[0]
            .layout
            .as_mut()
            .unwrap()
            .positions
            .values_mut()
            .next()
            .unwrap()
            .x = f64::NAN;
        assert!(serialize_draft(&document).is_err());
    }
}
