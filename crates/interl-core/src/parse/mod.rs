//! Format-independent parse limits, typed deserialization, and deterministic JSON output.
//! The JSON and YAML readers detect duplicates before insertion can overwrite earlier values.

mod json;
mod yaml;

use crate::diagnostic::pointer;
use crate::{Diagnostic, ErrorCategory, ScenarioDocument, ScenarioLoadError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The explicitly selected input syntax; parsing never guesses it from the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScenarioFormat {
    /// Parse duplicate-aware JSON.
    Json,
    /// Parse the restricted JSON-compatible YAML subset.
    Yaml,
}

/// Admission budgets shared by both syntax readers.
/// String lengths are measured in UTF-8 bytes; depth and value/collection counts
/// are checked while reading rather than after allocating the whole document.
#[derive(Debug, Clone, Copy)]
pub struct ScenarioLimits {
    /// Maximum UTF-8 source bytes read or admitted.
    pub max_bytes: usize,
    /// Maximum nested value depth; effective depth is also hard-capped at 128.
    pub max_depth: usize,
    /// Maximum entries in an individual object or array.
    pub max_collection_size: usize,
    /// Maximum aggregate JSON-compatible values across the document.
    pub max_values: usize,
}

/// Provide the standard 1 MiB, depth-64, 10,000-entry, 100,000-value limits.
/// `Default` lets callers use `ScenarioLimits::default()` or struct-update syntax.
impl Default for ScenarioLimits {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024,
            max_depth: 64,
            max_collection_size: 10_000,
            max_values: 100_000,
        }
    }
}

/// Mutable accounting owned by one parse operation.
/// `failure` preserves a safe diagnostic across Serde's generic error interface.
struct Budget {
    limits: ScenarioLimits,
    values: usize,
    failure: Option<Diagnostic>,
}

impl Budget {
    /// Start a fresh parse budget; no counters are shared between documents.
    fn new(limits: ScenarioLimits) -> Self {
        Self {
            limits,
            values: 0,
            failure: None,
        }
    }
    /// Charge one value and enforce aggregate/depth limits, with a hard depth cap of 128.
    fn count(&mut self, depth: usize, path: &str) -> Result<(), Diagnostic> {
        self.values += 1;
        if depth > self.limits.max_depth.min(128) || self.values > self.limits.max_values {
            return Err(Diagnostic::new(
                ErrorCategory::ResourceLimit,
                "resourceLimit",
                path,
                "Scenario nesting or aggregate value limit exceeded.",
            ));
        }
        Ok(())
    }
    /// Check a proposed collection size before appending another entry.
    fn collection(&self, size: usize, path: &str) -> Result<(), Diagnostic> {
        if size > self.limits.max_collection_size {
            return Err(Diagnostic::new(
                ErrorCategory::ResourceLimit,
                "collectionLimit",
                path,
                "Scenario collection size limit exceeded.",
            ));
        }
        Ok(())
    }
}

/// Parse JSON/YAML into the canonical typed model using default limits.
///
/// # Errors
/// Returns syntax, schema, duplicate-key, or resource-limit failures. Semantic
/// issues such as broken edges are deferred to [`crate::compile_document`] so
/// the editor can retain structurally valid drafts.
pub fn parse_scenario(
    source: &str,
    format: ScenarioFormat,
) -> Result<ScenarioDocument, ScenarioLoadError> {
    parse_scenario_with_limits(source, format, ScenarioLimits::default())
}

/// Parse bounded syntax, then deserialize the strict scenario shape with field paths.
///
/// `serde_path_to_error` tracks the failing field; safe diagnostics deliberately
/// avoid raw Serde errors that may contain payload or credential values.
///
/// # Errors
/// Returns the first syntax, schema, unsupported-declaration, or parser-budget error.
pub fn parse_scenario_with_limits(
    source: &str,
    format: ScenarioFormat,
    limits: ScenarioLimits,
) -> Result<ScenarioDocument, ScenarioLoadError> {
    let value = parse_value(source, format, limits)?;
    match value.get("schemaVersion") {
        None => {
            return Err(Diagnostic::new(
                ErrorCategory::Schema,
                "missingVersion",
                "/schemaVersion",
                "Declare schemaVersion: 2.",
            )
            .into())
        }
        Some(version) if version.as_u64() != Some(2) => {
            return Err(Diagnostic::new(
                ErrorCategory::Schema,
                "unsupportedVersion",
                "/schemaVersion",
                "Only integer schemaVersion: 2 is supported.",
            )
            .into())
        }
        _ => {}
    }
    if value
        .get("assertions")
        .and_then(Value::as_array)
        .is_some_and(|values| !values.is_empty())
    {
        return Err(Diagnostic::new(
            ErrorCategory::Schema,
            "unsupportedFeature",
            "/assertions",
            "Nonempty assertions are unsupported in the current schema; use an empty array.",
        )
        .into());
    }
    serde_path_to_error::deserialize(value).map_err(|error| {
        let mut path = String::new();
        for segment in error.path() {
            use serde_path_to_error::Segment;
            match segment {
                Segment::Seq { index } => path = pointer(&path, &index.to_string()),
                Segment::Map { key } => path = pointer(&path, key),
                Segment::Enum { .. } | Segment::Unknown => {}
            }
        }
        // Type errors may embed input values. Return safe shape information instead.
        let inner = error.inner().to_string();
        let message = if inner.starts_with("missing field") || inner.starts_with("unknown field") {
            inner
        } else {
            "Field has an invalid type or unsupported configuration; see the scenario schema contract."
                .into()
        };
        Diagnostic::new(ErrorCategory::Schema, "invalidSchema", &path, message).into()
    })
}

/// Validate and serialize a runnable scenario as deterministic, pretty-printed JSON.
///
/// Serde's built-in recursive sorter orders object keys without an intermediate
/// collection; array order is preserved, including inside arbitrary literal payloads.
/// For incomplete editor work use [`crate::serialize_draft`] instead.
///
/// # Errors
/// Returns the first semantic diagnostic or a safe serialization failure.
pub fn serialize_scenario(scenario: &ScenarioDocument) -> Result<String, ScenarioLoadError> {
    if let Err(errors) = crate::validate_scenario(scenario) {
        if let Some(diagnostic) = errors.into_iter().next() {
            return Err(diagnostic.into());
        }
    }
    let mut value = serde_json::to_value(scenario).map_err(|_| {
        Diagnostic::new(
            ErrorCategory::Schema,
            "serialization",
            "",
            "Scenario cannot be serialized.",
        )
    })?;
    value.sort_all_objects();
    serde_json::to_string_pretty(&value).map_err(|_| {
        Diagnostic::new(
            ErrorCategory::Schema,
            "serialization",
            "",
            "Scenario cannot be serialized.",
        )
        .into()
    })
}

/// Enforce the source byte budget and dispatch to the chosen duplicate-aware reader.
pub(crate) fn parse_value(
    source: &str,
    format: ScenarioFormat,
    limits: ScenarioLimits,
) -> Result<Value, ScenarioLoadError> {
    if source.len() > limits.max_bytes {
        return Err(Diagnostic::new(
            ErrorCategory::ResourceLimit,
            "fileSizeLimit",
            "",
            "Scenario source exceeds the configured byte limit.",
        )
        .into());
    }
    let value = match format {
        ScenarioFormat::Json => json::parse(source, limits)?,
        ScenarioFormat::Yaml => yaml::parse(source, limits)?,
    };
    Ok(value)
}
