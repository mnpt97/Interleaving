use crate::{
    inspect_scenario, parse_scenario_with_limits, Diagnostic, ErrorCategory, ScenarioDocument,
    ScenarioFormat, ScenarioLimits, ScenarioLoadError, ValidationResult,
};
use serde::Serialize;
use std::{fs::File, io::Read, path::Path};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenScenarioResult {
    pub path: String,
    pub format: ScenarioFormat,
    pub source: String,
    pub validation: ValidationResult,
}

fn read_source(
    path: &Path,
    limits: ScenarioLimits,
) -> Result<(String, ScenarioFormat), ScenarioLoadError> {
    let result = (|| {
        let format = match path.extension().and_then(|extension| extension.to_str()) {
            Some("json") => ScenarioFormat::Json,
            Some("yaml" | "yml") => ScenarioFormat::Yaml,
            _ => return Err(Diagnostic::new(ErrorCategory::Format, "unsupportedExtension", "", "Use .interl.json, .interl.yaml, or .interl.yml (.json/.yaml/.yml are also accepted).")),
        };
        let metadata = std::fs::metadata(path).map_err(|_| {
            Diagnostic::new(
                ErrorCategory::Io,
                "readFile",
                "",
                "Cannot access scenario file; check its path and permissions.",
            )
        })?;
        if !metadata.is_file() {
            return Err(Diagnostic::new(
                ErrorCategory::Io,
                "notFile",
                "",
                "Select a regular scenario file.",
            ));
        }
        if metadata.len() > limits.max_bytes as u64 {
            return Err(Diagnostic::new(
                ErrorCategory::ResourceLimit,
                "fileSizeLimit",
                "",
                "Scenario file exceeds the configured byte limit.",
            ));
        }
        let file = File::open(path).map_err(|_| {
            Diagnostic::new(
                ErrorCategory::Io,
                "readFile",
                "",
                "Cannot open scenario file; check its path and permissions.",
            )
        })?;
        let mut bytes = Vec::new();
        file.take((limits.max_bytes as u64).saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| {
                Diagnostic::new(
                    ErrorCategory::Io,
                    "readFile",
                    "",
                    "Cannot read scenario file.",
                )
            })?;
        if bytes.len() > limits.max_bytes {
            return Err(Diagnostic::new(
                ErrorCategory::ResourceLimit,
                "fileSizeLimit",
                "",
                "Scenario file exceeds the configured byte limit.",
            ));
        }
        let source = String::from_utf8(bytes).map_err(|_| {
            Diagnostic::new(
                ErrorCategory::Parse,
                "invalidUtf8",
                "",
                "Scenario files must use UTF-8.",
            )
        })?;
        Ok((source, format))
    })();
    result.map_err(|diagnostic: Diagnostic| diagnostic.with_file(path).into())
}

pub fn load_scenario(path: &Path) -> Result<ScenarioDocument, ScenarioLoadError> {
    load_scenario_with_limits(path, ScenarioLimits::default())
}

pub fn load_scenario_with_limits(
    path: &Path,
    limits: ScenarioLimits,
) -> Result<ScenarioDocument, ScenarioLoadError> {
    let (source, format) = read_source(path, limits)?;
    parse_scenario_with_limits(&source, format, limits)
        .map_err(|error| error.diagnostic.with_file(path).into())
}

pub fn open_scenario(path: &Path) -> Result<OpenScenarioResult, ScenarioLoadError> {
    let (source, format) = read_source(path, ScenarioLimits::default())?;
    let mut validation = inspect_scenario(&source, format);
    for diagnostic in &mut validation.diagnostics {
        diagnostic.file_path = Some(path.to_string_lossy().into_owned());
    }
    Ok(OpenScenarioResult {
        path: path.to_string_lossy().into_owned(),
        format,
        source,
        validation,
    })
}
