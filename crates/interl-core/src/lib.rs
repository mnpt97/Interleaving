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
pub use compile::{compile_scenario, AuthPlan, CompiledExtraction, ExecutionPlan, RequiredSecret};
pub use diagnostic::{Diagnostic, ErrorCategory, ScenarioLoadError, ValidationError};
pub use expressions::{CompiledExpression, MAX_EXPRESSION_BYTES, MAX_EXPRESSION_SEGMENTS};
pub use load::{load_scenario, load_scenario_with_limits, open_scenario, OpenScenarioResult};
pub use model::*;
pub use parse::{
    parse_scenario, parse_scenario_with_limits, serialize_scenario, ScenarioFormat, ScenarioLimits,
};
pub use validate::{inspect_scenario, validate_scenario, ValidationResult};
