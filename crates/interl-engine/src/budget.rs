//! Measure JSON wire bytes without allocating a second copy of runtime data.
use serde::Serialize;
use serde_json::Value;
use std::io::{self, Write};

/// Counts UTF-8 bytes and JSON escapes, including the surrounding quotes.
pub(crate) fn string_bytes(value: &str) -> usize {
    value.as_bytes().iter().fold(2usize, |size, byte| {
        size.saturating_add(match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
            0..=31 => 6,
            _ => 1,
        })
    })
}

/// Values already parsed by serde_json contain only valid JSON data.
pub(crate) fn value_bytes(value: &Value) -> usize {
    match value {
        Value::Null => 4,
        Value::Bool(true) => 4,
        Value::Bool(false) => 5,
        Value::Number(number) => number.to_string().len(),
        Value::String(text) => string_bytes(text),
        Value::Array(items) => items.iter().fold(
            2usize.saturating_add(items.len().saturating_sub(1)),
            |size, value| size.saturating_add(value_bytes(value)),
        ),
        Value::Object(fields) => fields.iter().fold(
            2usize.saturating_add(fields.len().saturating_sub(1)),
            |size, (key, value)| {
                size.saturating_add(string_bytes(key))
                    .saturating_add(1)
                    .saturating_add(value_bytes(value))
            },
        ),
    }
}

/// A Write implementation lets Serde count arbitrary events without storing bytes.
/// Serialization errors are propagated so admission can fail closed.
pub(crate) fn serialized_bytes(value: &impl Serialize) -> Result<usize, serde_json::Error> {
    #[derive(Default)]
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter::default();
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_matches_json_encoding_for_nested_values_and_all_ascii_escapes() {
        let controls: String = (0..=127).map(char::from).collect();
        let value = serde_json::json!({
            "escaped\"key": [controls, "雪🦀/\\", null, true, false, -1, 1.25, {}, []]
        });
        let expected = serde_json::to_vec(&value).unwrap().len();
        assert_eq!(value_bytes(&value), expected);
        assert_eq!(serialized_bytes(&value).unwrap(), expected);
    }

    #[test]
    fn counter_propagates_serialization_failures() {
        struct Invalid;
        impl Serialize for Invalid {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("rejected"))
            }
        }
        assert!(serialized_bytes(&Invalid).is_err());
    }
}
