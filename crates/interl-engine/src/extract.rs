//! Bounded read-only execution of core's compiled FEEL property-access subset.
//! No general evaluator, credentials, HTTP/session mutation, or event emission.
mod json;
use crate::CancellationToken;
use interl_core::AuthPlan;
use interl_http::HttpResponse;
use std::{collections::BTreeMap, fmt};

pub(crate) const MAX_RESPONSE_BYTES: usize = 1_048_576;
pub(crate) const MAX_JSON_DEPTH: usize = 64;
pub(crate) const MAX_JSON_VALUES: usize = 100_000;
pub(crate) const MAX_OUTPUTS: usize = 128;
pub(crate) const MAX_OUTPUT_BYTES: usize = 65_536;
pub(crate) const MAX_VALUE_BYTES: usize = 8192;

/// All messages/codes are static. Never retain parser errors, keys, or values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ExtractionError {
    #[error("Extraction cancelled.")]
    Cancelled,
    #[error("Compiled extraction is unavailable.")]
    UnknownExtraction,
    #[error("Required response body is unavailable.")]
    UnavailableResponse,
    #[error("Extraction requires a complete, untruncated response body.")]
    TruncatedResponse,
    #[error("Response data exceeds the per-response or aggregate byte limit.")]
    ResponseLimit,
    #[error("Response body must contain exactly one valid JSON value.")]
    InvalidJson,
    #[error("Response JSON contains a duplicate object key.")]
    DuplicateJsonKey,
    #[error("Response JSON exceeds the nesting limit.")]
    JsonDepthLimit,
    #[error("Response JSON exceeds the value-count limit.")]
    JsonValueLimit,
    #[error("Required response property is missing.")]
    MissingProperty,
    #[error("Required response property is null.")]
    NullValue,
    #[error("Extraction requires object property access ending in a string.")]
    WrongType,
    #[error("Extracted string must not be empty.")]
    EmptyValue,
    #[error("Extracted string exceeds the 8192-byte limit.")]
    ValueLimit,
    #[error("Extraction outputs exceed the entry or aggregate byte limit.")]
    OutputLimit,
    #[error("Required extraction output is unavailable.")]
    UnavailableOutput,
}
impl ExtractionError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::UnknownExtraction => "unknownExtraction",
            Self::UnavailableResponse => "extractionResponseUnavailable",
            Self::TruncatedResponse => "extractionResponseTruncated",
            Self::ResponseLimit => "extractionResponseLimit",
            Self::InvalidJson => "extractionInvalidJson",
            Self::DuplicateJsonKey => "extractionDuplicateKey",
            Self::JsonDepthLimit => "extractionJsonDepthLimit",
            Self::JsonValueLimit => "extractionJsonValueLimit",
            Self::MissingProperty => "extractionMissingProperty",
            Self::NullValue => "extractionNullValue",
            Self::WrongType => "extractionWrongType",
            Self::EmptyValue => "extractionEmptyValue",
            Self::ValueLimit => "extractionValueLimit",
            Self::OutputLimit => "extractionOutputLimit",
            Self::UnavailableOutput => "extractionOutputUnavailable",
        }
    }
}
fn check_cancel(cancel: &CancellationToken) -> Result<(), ExtractionError> {
    if cancel.is_cancelled() {
        Err(ExtractionError::Cancelled)
    } else {
        Ok(())
    }
}

/// No Serialize, Clone, or Display; raw access stays inside the engine.
pub(crate) struct ExtractedValue(String);
impl ExtractedValue {
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for ExtractedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ExtractedValue([REDACTED])")
    }
}
struct RetainedResponse {
    bytes: Vec<u8>,
    truncated: bool,
}

/// Owned by one user. Only the immutable compiled plan is copied; raw response
/// bytes and extraction outputs are never shared or serialized.
pub(crate) struct ExtractionState {
    plan: AuthPlan,
    responses: BTreeMap<String, RetainedResponse>,
    outputs: BTreeMap<String, ExtractedValue>,
    response_bytes: usize,
    output_bytes: usize,
    per_response_limit: usize,
}
impl fmt::Debug for ExtractionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractionState")
            .field("responses", &self.responses.len())
            .field("response_bytes", &self.response_bytes)
            .field("outputs", &self.outputs.len())
            .field("output_bytes", &self.output_bytes)
            .finish_non_exhaustive()
    }
}
impl From<ExtractionError> for crate::RunFailure {
    fn from(e: ExtractionError) -> Self {
        Self {
            kind: e.code().into(),
            message: e.to_string(),
        }
    }
}
impl ExtractionState {
    pub(crate) fn usage(&self) -> (usize, usize, usize) {
        (
            self.outputs.len(),
            self.outputs
                .values()
                .map(|v| serde_json::to_vec(v.expose()).expect("string scalar").len())
                .sum(),
            self.response_bytes,
        )
    }
    pub(crate) fn new(plan: &AuthPlan, per_response_limit: usize) -> Result<Self, ExtractionError> {
        if !(1..=MAX_RESPONSE_BYTES).contains(&per_response_limit) {
            return Err(ExtractionError::ResponseLimit);
        }
        Ok(Self {
            plan: plan.clone(),
            responses: BTreeMap::new(),
            outputs: BTreeMap::new(),
            response_bytes: 0,
            output_bytes: 0,
            per_response_limit,
        })
    }

    /// Called after safe HTTP observation. Consume only planned source bodies;
    /// interpretation is deferred until extract, never performed for previews.
    pub(crate) fn retain_response(
        &mut self,
        node: &str,
        response: HttpResponse,
        cancel: &CancellationToken,
    ) -> Result<(), ExtractionError> {
        if let Some(old) = self.responses.remove(node) {
            self.response_bytes -= old.bytes.len();
        }
        check_cancel(cancel)?;
        if !self.plan.response_last_uses().contains_key(node) {
            return Ok(());
        }
        let bytes = response.body.len();
        if bytes > self.per_response_limit || bytes > MAX_RESPONSE_BYTES - self.response_bytes {
            return Err(ExtractionError::ResponseLimit);
        }
        check_cancel(cancel)?;
        self.response_bytes += bytes;
        self.responses.insert(
            node.into(),
            RetainedResponse {
                bytes: response.body,
                truncated: response.truncated,
            },
        );
        Ok(())
    }

    /// Evaluate without changing the JSON input or session. Callers must
    /// register the returned sensitive value before subsequent observations.
    /// A failed re-extraction cannot fall back to an earlier output.
    #[cfg(test)]
    pub(crate) fn extract(
        &mut self,
        node: &str,
        cancel: &CancellationToken,
    ) -> Result<&ExtractedValue, ExtractionError> {
        self.extract_bounded(node, cancel, MAX_OUTPUTS, usize::MAX)
    }
    pub(crate) fn extract_bounded(
        &mut self,
        node: &str,
        cancel: &CancellationToken,
        max_outputs: usize,
        max_bytes: usize,
    ) -> Result<&ExtractedValue, ExtractionError> {
        if let Some(old) = self.outputs.remove(node) {
            self.output_bytes -= old.expose().len();
        }
        check_cancel(cancel)?;
        let extraction = self
            .plan
            .extractions()
            .get(node)
            .ok_or(ExtractionError::UnknownExtraction)?;
        let response = self
            .responses
            .get(extraction.source_node_id())
            .ok_or(ExtractionError::UnavailableResponse)?;
        if response.truncated {
            return Err(ExtractionError::TruncatedResponse);
        }
        let root = json::parse(&response.bytes, cancel)?;
        let mut value = &root;
        for segment in extraction.expression().body_path() {
            check_cancel(cancel)?;
            if value.is_null() {
                return Err(ExtractionError::NullValue);
            }
            let object = value.as_object().ok_or(ExtractionError::WrongType)?;
            value = object
                .get(segment)
                .ok_or(ExtractionError::MissingProperty)?;
        }
        if value.is_null() {
            return Err(ExtractionError::NullValue);
        }
        let value = value.as_str().ok_or(ExtractionError::WrongType)?;
        if value.is_empty() {
            return Err(ExtractionError::EmptyValue);
        }
        if value.len() > MAX_VALUE_BYTES {
            return Err(ExtractionError::ValueLimit);
        }
        let encoded = serde_json::to_vec(value).expect("string scalar").len();
        if self.outputs.len() >= MAX_OUTPUTS.min(max_outputs)
            || value.len() > MAX_OUTPUT_BYTES.saturating_sub(self.output_bytes)
            || encoded > max_bytes.saturating_sub(self.usage().1)
        {
            return Err(ExtractionError::OutputLimit);
        }

        check_cancel(cancel)?;
        let value = ExtractedValue(value.into());
        check_cancel(cancel)?;
        self.output_bytes += value.expose().len();
        self.outputs.insert(node.into(), value);
        Ok(&self.outputs[node])
    }

    pub(crate) fn output(
        &self,
        node: &str,
        name: &str,
    ) -> Result<&ExtractedValue, ExtractionError> {
        if self
            .plan
            .extractions()
            .get(node)
            .is_none_or(|e| e.output() != name)
        {
            return Err(ExtractionError::UnavailableOutput);
        }
        self.outputs
            .get(node)
            .ok_or(ExtractionError::UnavailableOutput)
    }

    /// Invoke after a successfully completed step, after registering any new
    /// output. Raw storage is dropped at the last use compiled by core.
    pub(crate) fn release_after(&mut self, node: &str) {
        self.responses.retain(|source, value| {
            let keep = self
                .plan
                .response_last_uses()
                .get(source)
                .is_none_or(|last| last != node);
            if !keep {
                self.response_bytes -= value.bytes.len();
            }
            keep
        });
        self.outputs.retain(|source, value| {
            let keep = self
                .plan
                .extractions()
                .get(source)
                .is_none_or(|e| e.last_use_node_id() != node);
            if !keep {
                self.output_bytes -= value.expose().len();
            }
            keep
        });
    }
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
