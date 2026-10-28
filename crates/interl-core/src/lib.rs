//! Scenario data, safe parsing, validation, and compilation shared by the app and CLI.
//!
//! This crate describes **what may run**; the HTTP and engine crates perform execution.
//! There is one canonical model and no version-specific Rust namespace or migration layer.
//!
//! # Processing a scenario
//!
//! 1. [`load_scenario`] reads a bounded UTF-8 file; [`parse_scenario`] accepts an in-memory string.
//! 2. [`compile_scenario`] checks graph structure, references, types, and resource limits,
//!    then returns an immutable [`Program`]. It never reads credentials or sends HTTP.
//! 3. [`inspect_document`] retains a structurally valid draft even when semantic checks fail.
//! 4. [`serialize_scenario`] requires a runnable document; [`serialize_draft`] also permits
//!    incomplete editor work while preserving structural and resource limits.
//!
//! The `*_document` and `*_scenario` names are re-exports of the same functions, not
//! separate implementations. Private modules organize implementation details; public
//! types and functions are imported directly from `interl_core`.
//!
//! # Rust conventions used here
//!
//! `Result<T, E>` represents success or a reported error; `?` returns early on an error.
//! `Option<T>` represents an optional value without Java-style null references.
//! `&T` borrows data without copying it; `&mut T` temporarily grants exclusive mutable access.
//! `pub(crate)` exposes an implementation detail only inside this crate. Serde's
//! `derive` implementations convert our types to/from JSON-compatible data, with
//! attributes defining field names and rejecting unsupported fields.
//! `impl Type` groups methods belonging to a type; `impl Trait for Type` provides
//! an interface implementation. Associated types such as `S::Error` let each
//! implementation choose its result/error representation. A trailing `!` marks a
//! macro (`format!`, `matches!`), which expands code rather than calling a function.
//! `Clone` requests an owned copy explicitly; `Copy` permits implicit value copies
//! for small types whose contents support that behavior.
//!
//! ```
//! use interl_core::{compile_scenario, parse_scenario, ScenarioFormat};
//!
//! # fn main() -> Result<(), interl_core::ScenarioLoadError> {
//! let source = include_str!("../../../scenarios/examples/hello-api.json");
//! let document = parse_scenario(source, ScenarioFormat::Json)?;
//! let program = compile_scenario(&document).expect("the bundled example is valid");
//! assert!(!program.reachable().is_empty());
//! # Ok(())
//! # }
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]
#![warn(clippy::redundant_clone, clippy::needless_collect)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod auth;
mod auth_validation;
mod compile;
mod diagnostic;
mod document;
mod expressions;
mod graph_validation;
mod load;
mod model;
mod parse;
mod program;
mod routing;
mod scalar;
pub use routing::{compile_predicate, CompiledPredicate, PredicateError};

pub use auth::*;
pub use diagnostic::{Diagnostic, ErrorCategory, ScenarioLoadError, ValidationError};
pub use expressions::{
    compile_expression, CompiledExpression, ExpressionError, MAX_EXPRESSION_BYTES,
    MAX_EXPRESSION_SEGMENTS,
};
pub use load::{load_scenario, load_scenario_with_limits};
pub use model::*;
pub use parse::{
    parse_scenario, parse_scenario_with_limits, serialize_scenario, ScenarioFormat, ScenarioLimits,
};
pub use program::*;

pub use document::*;
pub use document::{open_document as open_scenario, OpenDocumentResult as OpenScenarioResult};

pub use compile::compile_document;
pub use compile::compile_document as compile_scenario;
pub use document::{inspect_document as inspect_scenario, DocumentValidation as ValidationResult};
pub use model::{
    ProgramEdge as WorkflowEdge, ProgramHeader as SessionHeaderValue, ProgramNode as WorkflowNode,
    ProgramSession as SetSessionConfig, ProgramTemplate as TemplateValue,
};
/// Check every semantic rule without retaining the compiled execution plan.
///
/// # Errors
/// Returns bounded, safe diagnostics for invalid graphs, references, or limits.
/// `map(|_| ())` deliberately discards a successful [`Program`]; `()` is Rust's
/// unit value, used when success has no additional result data.
pub fn validate_scenario(document: &ScenarioDocument) -> Result<(), Vec<Diagnostic>> {
    compile_document(document).map(|_| ())
}
