use super::{Budget, ScenarioLimits};
use crate::diagnostic::pointer;
use crate::{Diagnostic, ErrorCategory, ScenarioLoadError};
use saphyr_parser::{Event, Parser, ScalarStyle};
use serde_json::{Map, Value};

enum Collection {
    Array(Vec<Value>),
    Object {
        values: Map<String, Value>,
        key: Option<String>,
    },
}
struct Frame {
    path: String,
    collection: Collection,
}

fn next_path(stack: &[Frame]) -> String {
    match stack.last() {
        Some(Frame {
            path,
            collection: Collection::Array(values),
        }) => pointer(path, &values.len().to_string()),
        Some(Frame {
            path,
            collection: Collection::Object { key: Some(key), .. },
        }) => pointer(path, key),
        Some(Frame { path, .. }) => path.clone(),
        None => String::new(),
    }
}

fn scalar(text: &str, style: ScalarStyle, path: &str) -> Result<Value, Diagnostic> {
    if style != ScalarStyle::Plain {
        return Ok(Value::String(text.into()));
    }
    if matches!(text, "true" | "false" | "null") {
        return Ok(match text {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::Null,
        });
    }
    let lower = text.to_ascii_lowercase();
    let ambiguous = text.is_empty()
        || matches!(
            lower.as_str(),
            "yes"
                | "no"
                | "on"
                | "off"
                | "y"
                | "n"
                | "~"
                | "true"
                | "false"
                | "null"
                | ".inf"
                | ".nan"
                | "+.inf"
                | "-.inf"
        );
    let numeric = text.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '+' | '-' | '.'));
    if numeric {
        if let Ok(value @ Value::Number(_)) = serde_json::from_str::<Value>(text) {
            return Ok(value);
        }
    }
    if ambiguous || numeric {
        return Err(Diagnostic::new(
            ErrorCategory::Parse,
            "ambiguousScalar",
            path,
            "Quote ambiguous YAML scalars or use a JSON-compatible primitive.",
        ));
    }
    Ok(Value::String(text.into()))
}

fn attach(
    value: Value,
    stack: &mut [Frame],
    root: &mut Option<Value>,
    budget: &Budget,
) -> Result<(), Diagnostic> {
    match stack.last_mut() {
        Some(Frame {
            path,
            collection: Collection::Array(values),
        }) => {
            budget.collection(values.len() + 1, path)?;
            values.push(value);
        }
        Some(Frame {
            path,
            collection: Collection::Object { values, key },
        }) => {
            if let Some(key) = key.take() {
                values.insert(key, value);
            } else {
                let Value::String(name) = value else {
                    return Err(Diagnostic::new(
                        ErrorCategory::Parse,
                        "nonStringKey",
                        path,
                        "YAML object keys must be strings.",
                    ));
                };
                if values.contains_key(&name) {
                    return Err(Diagnostic::new(
                        ErrorCategory::Parse,
                        "duplicateKey",
                        &pointer(path, &name),
                        "Duplicate object key.",
                    ));
                }
                budget.collection(values.len() + 1, path)?;
                *key = Some(name);
            }
        }
        None if root.is_none() => *root = Some(value),
        None => {
            return Err(Diagnostic::new(
                ErrorCategory::Parse,
                "multipleDocuments",
                "",
                "Only one YAML document is allowed.",
            ))
        }
    }
    Ok(())
}

pub(super) fn parse(source: &str, limits: ScenarioLimits) -> Result<Value, ScenarioLoadError> {
    let mut budget = Budget::new(limits);
    let mut stack = Vec::<Frame>::new();
    let mut root = None;
    let mut documents = 0;
    for event in Parser::new_from_str(source) {
        let (event, span) = event.map_err(|error| {
            Diagnostic::new(
                ErrorCategory::Parse,
                "invalidYaml",
                "",
                "Invalid YAML syntax.",
            )
            .at(error.marker().line(), error.marker().col() + 1)
        })?;
        let path = next_path(&stack);
        let sequence = matches!(&event, Event::SequenceStart(..));
        let result = (|| -> Result<(), Diagnostic> {
            if matches!(
                &event,
                Event::Scalar(..) | Event::MappingStart(..) | Event::SequenceStart(..)
            ) {
                if let Some(Frame {
                    path,
                    collection: Collection::Array(values),
                }) = stack.last()
                {
                    budget.collection(values.len() + 1, path)?;
                }
            }
            match event {
                Event::DocumentStart(_) => {
                    documents += 1;
                    if documents > 1 {
                        return Err(Diagnostic::new(
                            ErrorCategory::Parse,
                            "multipleDocuments",
                            "",
                            "Only one YAML document is allowed.",
                        ));
                    }
                }
                Event::Alias(_) => {
                    return Err(Diagnostic::new(
                        ErrorCategory::Parse,
                        "yamlAlias",
                        &path,
                        "YAML aliases are unsupported.",
                    ))
                }
                Event::Scalar(text, style, anchor, tag) => {
                    if anchor != 0 || tag.is_some() {
                        return Err(Diagnostic::new(
                            ErrorCategory::Parse,
                            "yamlMetadata",
                            &path,
                            "YAML anchors and explicit tags are unsupported.",
                        ));
                    }
                    if text == "<<" && style == ScalarStyle::Plain {
                        return Err(Diagnostic::new(
                            ErrorCategory::Parse,
                            "yamlMerge",
                            &path,
                            "YAML merge keys are unsupported.",
                        ));
                    }
                    let is_key = matches!(
                        stack.last(),
                        Some(Frame {
                            collection: Collection::Object { key: None, .. },
                            ..
                        })
                    );
                    if !is_key {
                        budget.count(stack.len(), &path)?;
                    }
                    let value = if is_key
                        && style == ScalarStyle::Plain
                        && !text.is_empty()
                        && !matches!(text.as_ref(), "true" | "false" | "null")
                        && !text.starts_with(|c: char| {
                            c.is_ascii_digit() || matches!(c, '+' | '-' | '.')
                        }) {
                        Value::String(text.into_owned())
                    } else {
                        scalar(&text, style, &path)?
                    };
                    attach(value, &mut stack, &mut root, &budget)?;
                }
                Event::SequenceStart(anchor, tag) | Event::MappingStart(anchor, tag) => {
                    if anchor != 0 || tag.is_some() {
                        return Err(Diagnostic::new(
                            ErrorCategory::Parse,
                            "yamlMetadata",
                            &path,
                            "YAML anchors and explicit tags are unsupported.",
                        ));
                    }
                    if matches!(
                        stack.last(),
                        Some(Frame {
                            collection: Collection::Object { key: None, .. },
                            ..
                        })
                    ) {
                        return Err(Diagnostic::new(
                            ErrorCategory::Parse,
                            "nonStringKey",
                            &path,
                            "YAML object keys must be strings.",
                        ));
                    }
                    budget.count(stack.len(), &path)?;
                    let collection = if sequence {
                        Collection::Array(Vec::new())
                    } else {
                        Collection::Object {
                            values: Map::new(),
                            key: None,
                        }
                    };
                    stack.push(Frame { path, collection });
                }
                Event::SequenceEnd | Event::MappingEnd => {
                    let frame = stack.pop().ok_or_else(|| {
                        Diagnostic::new(
                            ErrorCategory::Parse,
                            "invalidYaml",
                            &path,
                            "Unbalanced YAML collection.",
                        )
                    })?;
                    let value = match frame.collection {
                        Collection::Array(values) => Value::Array(values),
                        Collection::Object { values, .. } => Value::Object(values),
                    };
                    attach(value, &mut stack, &mut root, &budget)?;
                }
                _ => {}
            }
            Ok(())
        })();
        result.map_err(|diagnostic| diagnostic.at(span.start.line(), span.start.col() + 1))?;
    }
    root.ok_or_else(|| {
        Diagnostic::new(
            ErrorCategory::Parse,
            "emptyDocument",
            "",
            "Provide one scenario document.",
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_scalars_keep_exact_expression_and_numeric_text() {
        for text in [
            "response.status = 201",
            "01",
            "true",
            "2026-10-13",
            "yes",
            "${token}",
        ] {
            assert_eq!(
                scalar(text, ScalarStyle::DoubleQuoted, "").unwrap(),
                Value::String(text.into())
            );
        }
    }

    #[test]
    fn plain_scalars_use_json_number_and_primitive_semantics() {
        for text in ["true", "false", "null", "-1", "0", "1.25", "1e3"] {
            assert_eq!(
                scalar(text, ScalarStyle::Plain, "").unwrap(),
                serde_json::from_str::<Value>(text).unwrap()
            );
        }
        for text in ["01", "0x10", "yes", "FALSE", "~", ".inf", "2026-10-13"] {
            assert!(scalar(text, ScalarStyle::Plain, "").is_err());
        }
    }
}
