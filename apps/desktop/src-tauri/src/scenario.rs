use interl_core::{Diagnostic, DocumentValidation, OpenDocumentResult, ScenarioFormat};
use std::path::Path;

#[tauri::command]
pub fn validate_scenario(source: &str, format: ScenarioFormat) -> DocumentValidation {
    interl_core::inspect_document(source, format)
}

#[tauri::command]
pub fn open_scenario(path: &str) -> Result<OpenDocumentResult, Diagnostic> {
    interl_core::open_document(Path::new(path)).map_err(|error| error.diagnostic)
}

#[tauri::command]
pub fn save_scenario_json(path: &str, source: &str) -> Result<(), Diagnostic> {
    use interl_core::{ErrorCategory, parse_document, serialize_document};
    let destination = Path::new(path);
    if destination
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("json")
    {
        return Err(Diagnostic::new(
            ErrorCategory::Format,
            "unsupportedExtension",
            "",
            "Save as a .json file; YAML files are never overwritten by JSON export.",
        ));
    }
    let document =
        parse_document(source, ScenarioFormat::Json).map_err(|error| error.diagnostic)?;
    let canonical = serialize_document(&document).map_err(|error| error.diagnostic)?;
    std::fs::write(destination, canonical).map_err(|_| {
        Diagnostic::new(
            ErrorCategory::Io,
            "writeFile",
            "",
            "Cannot save JSON; check the destination and permissions.",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const JSON: &str = include_str!("../../../../scenarios/examples/hello-api.json");
    const YAML: &str = include_str!("../../../../scenarios/examples/hello-api.interl.yaml");

    #[test]
    fn commands_return_the_same_core_results_for_both_formats() {
        let json = validate_scenario(JSON, ScenarioFormat::Json);
        let yaml = validate_scenario(YAML, ScenarioFormat::Yaml);
        assert!(json.valid && yaml.valid);
        assert_eq!(json.document, yaml.document);
        for (source, format) in [
            ("{", ScenarioFormat::Json),
            ("a: [", ScenarioFormat::Yaml),
            ("{\"schemaVersion\":1}", ScenarioFormat::Json),
            ("{\"schemaVersion\":3}", ScenarioFormat::Json),
        ] {
            assert_eq!(
                validate_scenario(source, format).diagnostics,
                interl_core::inspect_document(source, format).diagnostics
            );
        }
    }

    #[test]
    fn auth_validation_reuses_core_semantics_and_expression_diagnostics() {
        let json = include_str!("../../../../scenarios/examples/auth-login.interl.json");
        let yaml = include_str!("../../../../scenarios/examples/auth-login.interl.yaml");
        assert!(validate_scenario(json, ScenarioFormat::Json).valid);
        assert!(validate_scenario(yaml, ScenarioFormat::Yaml).valid);
        let json = json.replace("response.body.accessToken", "response.body.token()");
        let yaml = yaml.replace("response.body.accessToken", "response.body.token()");
        let left = validate_scenario(&json, ScenarioFormat::Json);
        let right = validate_scenario(&yaml, ScenarioFormat::Yaml);
        assert!(!left.valid && !right.valid);
        assert_eq!(left.diagnostics, right.diagnostics);
        assert_eq!(
            left.diagnostics,
            interl_core::inspect_document(&json, ScenarioFormat::Json).diagnostics
        );
        assert_eq!(left.diagnostics[0].expression_line, Some(1));
        assert_eq!(
            left.diagnostics[0].node_id.as_deref(),
            Some("extract-token")
        );
    }

    #[test]
    fn opening_uses_the_shared_loader_and_returns_source_metadata() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scenarios/examples/hello-api.interl.yaml");
        let result = open_scenario(path.to_str().unwrap()).unwrap();
        assert!(result.validation.valid);
        assert_eq!(result.format, ScenarioFormat::Yaml);
        assert_eq!(result.source, YAML);
        assert_eq!(
            result.validation.document.unwrap(),
            interl_core::open_document(&path)
                .map(|r| r.validation.document.unwrap())
                .unwrap()
        );
        assert_eq!(
            open_scenario("missing.txt").unwrap_err().code,
            "unsupportedExtension"
        );
    }

    #[test]
    fn json_export_preserves_semantics_and_never_overwrites_yaml_or_invalid_input() {
        let dir = std::env::temp_dir().join(format!("interl-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let json_path = dir.join("copy.interl.json");
        let yaml_path = dir.join("original.interl.yaml");
        std::fs::write(&yaml_path, YAML).unwrap();
        save_scenario_json(json_path.to_str().unwrap(), JSON).unwrap();
        assert_eq!(
            interl_core::load_scenario(&json_path).unwrap(),
            interl_core::load_scenario(&yaml_path).unwrap()
        );
        assert_eq!(
            save_scenario_json(yaml_path.to_str().unwrap(), JSON)
                .unwrap_err()
                .code,
            "unsupportedExtension"
        );
        assert_eq!(std::fs::read_to_string(&yaml_path).unwrap(), YAML);
        let saved = std::fs::read_to_string(&json_path).unwrap();
        assert!(save_scenario_json(json_path.to_str().unwrap(), "{").is_err());
        assert_eq!(std::fs::read_to_string(&json_path).unwrap(), saved);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod reusable_tests {
    use super::*;
    #[test]
    fn v2_validation_and_json_serialization_preserve_source_version_and_references() {
        let source = include_str!("../../../../scenarios/examples/reusable-auth.interl.json");
        let result = validate_scenario(source, ScenarioFormat::Json);
        assert!(result.valid);
        let file = result.document.unwrap();
        assert_eq!(file.schema_version, 2);
        let saved = interl_core::serialize_document(&file).unwrap();
        assert!(saved.contains("INTERL_ALICE_PASSWORD"));
        assert!(!saved.contains("alice-password"));
        assert_eq!(
            file,
            interl_core::parse_document(&saved, ScenarioFormat::Json).unwrap()
        );
    }
}
