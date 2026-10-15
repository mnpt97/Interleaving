//! Authoritative scenario contract. This crate performs no HTTP execution.
mod auth;
mod auth_validation;
mod compile;
mod diagnostic;
mod expressions;
mod load;
mod model;
mod parse;
mod validate;

pub use auth::*;
pub use compile::{AuthPlan, CompiledExtraction, RequiredSecret};
pub use diagnostic::{Diagnostic, ErrorCategory, ScenarioLoadError, ValidationError};
pub use expressions::{CompiledExpression, MAX_EXPRESSION_BYTES, MAX_EXPRESSION_SEGMENTS};
pub use load::{load_scenario, load_scenario_with_limits};
pub use model::*;
pub use parse::{
    parse_scenario, parse_scenario_with_limits, serialize_scenario, ScenarioFormat, ScenarioLimits,
};

mod document;
pub mod v2;
pub use document::*;
pub use document::{open_document as open_scenario, OpenDocumentResult as OpenScenarioResult};

pub use document::{inspect_document as inspect_scenario, DocumentValidation as ValidationResult};
pub use v2::compile_document as compile_scenario;
pub use v2::*;
pub use v2::{
    ProgramEdge as WorkflowEdge, ProgramHeader as SessionHeaderValue, ProgramNode as WorkflowNode,
    ProgramSession as SetSessionConfig, ProgramTemplate as TemplateValue,
};
pub fn validate_scenario(document: &ScenarioDocument) -> Result<(), Vec<Diagnostic>> {
    compile_document(document).map(|_| ())
}
