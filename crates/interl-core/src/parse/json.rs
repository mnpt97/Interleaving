//! Duplicate-aware JSON deserialization with budgets checked as values are visited.

use super::{Budget, ScenarioLimits};
use crate::diagnostic::pointer;
use crate::{Diagnostic, ErrorCategory, ScenarioLoadError};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Carry a mutable budget, depth, and JSON Pointer into each child deserializer.
/// `DeserializeSeed` supplies state that ordinary `Deserialize` implementations
/// cannot accept as arguments; all children reborrow the same parse budget.
struct Seed<'a> {
    budget: &'a mut Budget,
    depth: usize,
    path: String,
    collection_full: bool,
}

impl Seed<'_> {
    /// Preserve the structured diagnostic and return a generic Serde failure.
    /// `E` is chosen by the calling deserializer; raw input values are not placed in its message.
    fn fail<E: de::Error>(&mut self, diagnostic: Diagnostic) -> E {
        self.budget.failure = Some(diagnostic);
        E::custom("Scenario rejected")
    }
}

/// Admit a value before asking Serde to visit it.
/// `'de` is the input-text lifetime; the separate budget borrow can be shorter.
/// `type Value` is the trait's associated output type, here a JSON value.
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(mut self, deserializer: D) -> Result<Value, D::Error> {
        if self.collection_full {
            return Err(self.fail(Diagnostic::new(
                ErrorCategory::ResourceLimit,
                "collectionLimit",
                &self.path,
                "Scenario collection size limit exceeded.",
            )));
        }
        if let Err(diagnostic) = self.budget.count(self.depth, &self.path) {
            return Err(self.fail(diagnostic));
        }
        deserializer.deserialize_any(self)
    }
}

/// Build JSON values through type-specific callbacks.
/// Maps check each key before insertion, so duplicate keys cannot disappear through
/// normal map overwrite behavior. Child seeds enforce limits before child allocation.
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JSON-compatible scenario data")
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(value.into())
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("Nonfinite number"))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.into()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: SeqAccess<'de>>(mut self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        let collection_limit = self.budget.limits.max_collection_size;
        while let Some(value) = seq.next_element_seed(Seed {
            budget: self.budget,
            depth: self.depth + 1,
            path: pointer(&self.path, &values.len().to_string()),
            collection_full: values.len() >= collection_limit,
        })? {
            if let Err(diagnostic) = self.budget.collection(values.len() + 1, &self.path) {
                return Err(self.fail(diagnostic));
            }
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let path = pointer(&self.path, &key);
            if values.contains_key(&key) {
                return Err(self.fail(Diagnostic::new(
                    ErrorCategory::Parse,
                    "duplicateKey",
                    &path,
                    "Duplicate object key.",
                )));
            }
            if let Err(diagnostic) = self.budget.collection(values.len() + 1, &self.path) {
                return Err(self.fail(diagnostic));
            }
            let value = map.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
                path,
                collection_full: false,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

/// Read exactly one bounded JSON value and reject trailing content.
/// A retained structured failure takes precedence over the generic Serde error;
/// parser line/column context is attached without exposing raw payload text.
pub(super) fn parse(source: &str, limits: ScenarioLimits) -> Result<Value, ScenarioLoadError> {
    let mut budget = Budget::new(limits);
    let mut deserializer = serde_json::Deserializer::from_str(source);
    let result = Seed {
        budget: &mut budget,
        depth: 0,
        path: String::new(),
        collection_full: false,
    }
    .deserialize(&mut deserializer);
    let value = result.map_err(|error| {
        budget
            .failure
            .take()
            .unwrap_or_else(|| {
                Diagnostic::new(
                    ErrorCategory::Parse,
                    "invalidJson",
                    "",
                    "Invalid JSON syntax or number.",
                )
            })
            .at(error.line(), error.column())
    })?;
    deserializer.end().map_err(|error| {
        Diagnostic::new(
            ErrorCategory::Parse,
            "trailingContent",
            "",
            "Unexpected content after the JSON document.",
        )
        .at(error.line(), error.column())
    })?;
    Ok(value)
}
