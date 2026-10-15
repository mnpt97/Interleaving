mod json;
mod yaml;

use crate::diagnostic::pointer;
use crate::{Diagnostic, ErrorCategory, ScenarioDocument, ScenarioLoadError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScenarioFormat {
    Json,
    Yaml,
}

#[derive(Debug, Clone, Copy)]
pub struct ScenarioLimits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_collection_size: usize,
    pub max_values: usize,
}

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

struct Budget {
    limits: ScenarioLimits,
    values: usize,
    failure: Option<Diagnostic>,
}

impl Budget {
    fn new(limits: ScenarioLimits) -> Self {
        Self {
            limits,
            values: 0,
            failure: None,
        }
    }
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

pub fn parse_scenario(
    source: &str,
    format: ScenarioFormat,
) -> Result<ScenarioDocument, ScenarioLoadError> {
    parse_scenario_with_limits(source, format, ScenarioLimits::default())
}

pub fn parse_scenario_with_limits(
    source: &str,
    format: ScenarioFormat,
    limits: ScenarioLimits,
) -> Result<ScenarioDocument, ScenarioLoadError> {
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
    match value.get("schemaVersion") {
        None => {
            return Err(Diagnostic::new(
                ErrorCategory::Schema,
                "missingVersion",
                "/schemaVersion",
                "Declare schemaVersion: 1.",
            )
            .into())
        }
        Some(version) if version.as_u64() != Some(1) => {
            return Err(Diagnostic::new(
                ErrorCategory::Schema,
                "unsupportedVersion",
                "/schemaVersion",
                "Only integer schemaVersion: 1 is supported.",
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
            "Field has an invalid type or unsupported configuration; see the schema v1 contract."
                .into()
        };
        Diagnostic::new(ErrorCategory::Schema, "invalidSchema", &path, message).into()
    })
}

/// Stable JSON, including nested arbitrary payload maps; arrays retain their order.
pub fn serialize_scenario(scenario: &ScenarioDocument) -> Result<String, ScenarioLoadError> {
    if let Err(errors) = crate::validate_scenario(scenario) {
        if let Some(diagnostic) = errors.into_iter().next() {
            return Err(diagnostic.into());
        }
    }
    fn sort(value: &mut Value) {
        match value {
            Value::Object(map) => {
                let old = std::mem::take(map);
                let sorted: std::collections::BTreeMap<_, _> = old.into_iter().collect();
                for (key, mut value) in sorted {
                    sort(&mut value);
                    map.insert(key, value);
                }
            }
            Value::Array(array) => array.iter_mut().for_each(sort),
            _ => {}
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
    sort(&mut value);
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
