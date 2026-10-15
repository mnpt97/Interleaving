//! Parse response JSON with bounded allocation/work and decoded-key uniqueness.
//! Scenario diagnostics are deliberately not reused: response keys are secret.
use super::{check_cancel, ExtractionError, MAX_JSON_DEPTH, MAX_JSON_VALUES};
use crate::CancellationToken;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

struct Budget<'a> {
    cancel: &'a CancellationToken,
    values: usize,
    failure: Option<ExtractionError>,
}
struct Seed<'a, 'b> {
    budget: &'a mut Budget<'b>,
    depth: usize,
}
impl Seed<'_, '_> {
    fn fail<E: de::Error>(&mut self, error: ExtractionError) -> E {
        self.budget.failure = Some(error);
        E::custom("Response rejected")
    }
}
impl<'de> DeserializeSeed<'de> for Seed<'_, '_> {
    type Value = Value;
    fn deserialize<D: de::Deserializer<'de>>(mut self, deserializer: D) -> Result<Value, D::Error> {
        if let Err(error) = check_cancel(self.budget.cancel) {
            return Err(self.fail(error));
        }
        if self.depth > MAX_JSON_DEPTH {
            return Err(self.fail(ExtractionError::JsonDepthLimit));
        }
        if self.budget.values >= MAX_JSON_VALUES {
            return Err(self.fail(ExtractionError::JsonValueLimit));
        }
        self.budget.values += 1;
        deserializer.deserialize_any(self)
    }
}
impl<'de> Visitor<'de> for Seed<'_, '_> {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JSON response data")
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
            .ok_or_else(|| E::custom("Invalid number"))
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
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }
    fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(self.fail(ExtractionError::DuplicateJsonKey));
            }
            let value = map.next_value_seed(Seed {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}
pub(super) fn parse(bytes: &[u8], cancel: &CancellationToken) -> Result<Value, ExtractionError> {
    check_cancel(cancel)?;
    let mut budget = Budget {
        cancel,
        values: 0,
        failure: None,
    };
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let result = Seed {
        budget: &mut budget,
        depth: 0,
    }
    .deserialize(&mut deserializer);
    let value = result.map_err(|_| budget.failure.unwrap_or(ExtractionError::InvalidJson))?;
    check_cancel(cancel)?;
    deserializer
        .end()
        .map_err(|_| ExtractionError::InvalidJson)?;
    check_cancel(cancel)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_between_values_stops_deserialization() {
        // Exercise the same serde seed protocol as JSON without timing races or
        // production test hooks: this source cancels as its second key arrives.
        let cancel = CancellationToken::new();
        let mut seen = 0;
        let values = [("first", "private-marker"), ("second", "private-marker")]
            .into_iter()
            .inspect(|_| {
                seen += 1;
                if seen == 2 {
                    cancel.cancel();
                }
            });
        let source = serde::de::value::MapDeserializer::<_, serde::de::value::Error>::new(values);
        let mut budget = Budget {
            cancel: &cancel,
            values: 0,
            failure: None,
        };
        assert!(Seed {
            budget: &mut budget,
            depth: 0
        }
        .deserialize(source)
        .is_err());
        assert_eq!(budget.failure, Some(ExtractionError::Cancelled));
        assert_eq!(budget.values, 2); // root object and first value only
    }
}
