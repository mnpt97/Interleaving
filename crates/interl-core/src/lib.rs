//! Authoritative scenario contract. This crate performs no HTTP execution.
mod compile;
mod diagnostic;
mod load;
mod model;
mod parse;
mod validate;

pub use compile::{compile_scenario, ExecutionPlan};
pub use diagnostic::{Diagnostic, ErrorCategory, ScenarioLoadError, ValidationError};
pub use load::{load_scenario, load_scenario_with_limits, open_scenario, OpenScenarioResult};
pub use model::*;
pub use parse::{
    parse_scenario, parse_scenario_with_limits, serialize_scenario, ScenarioFormat, ScenarioLimits,
};
pub use validate::{inspect_scenario, validate_scenario, ValidationResult};
