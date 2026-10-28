//! Shared single-user runner for compiled scenarios.
//!
//! [`PreparedRun`] validates resource limits and resolves credentials before execution.
//! Starting it creates an isolated HTTP session and returns a [`RunHandle`]. Drain
//! its bounded event receiver while awaiting its task: an undrained receiver applies
//! backpressure to execution. The final [`RunReport`] retains the same redacted events.
//!
//! ```
//! use interl_engine::{RunControl, StopCause};
//! let control = RunControl::new();
//! let adapter = control.clone(); // Both handles signal the same run.
//! adapter.cancel();
//! assert_eq!(control.cause(), Some(StopCause::UserCancel));
//! ```
#![deny(missing_docs, rustdoc::broken_intra_doc_links)]
#![warn(clippy::redundant_clone, clippy::needless_collect)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
mod budget;
mod extract;
mod program;
mod redaction;
mod report;
pub use report::*;
mod secrets;
mod security;
pub use program::PreparedRun;
mod control;
pub use control::{RunControl, StopCause};
pub use secrets::{
    EnvironmentSecretProvider, SecretLookupError, SecretProvider, MAX_CREDENTIAL_BYTES,
    MAX_CREDENTIAL_REFERENCES, MAX_TOTAL_CREDENTIAL_BYTES,
};
use serde_json::Value;
mod trace;
use tokio::sync::mpsc;
pub use tokio_util::sync::CancellationToken;
use trace::Trace;

fn report_limit() -> RunFailure {
    RunFailure {
        kind: "resourceLimit".into(),
        message: "Run report byte limit reached; execution stopped".into(),
    }
}
/// Stable CLI exit semantics, also available to adapter parity tests.
pub fn exit_code(status: RunStatus) -> u8 {
    match status {
        RunStatus::Passed => 0,
        RunStatus::Failed => 1,
        RunStatus::Cancelled => 130,
        RunStatus::Error => 3,
    }
}
