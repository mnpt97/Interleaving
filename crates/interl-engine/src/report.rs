//! Public adapter contract: run limits, safe events, reports, and task handles.
use crate::{CancellationToken, RunControl};
use interl_http::HttpError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{sync::mpsc, task::JoinHandle};

/// Resource limits and additional sensitive field names for one run.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RunOptions {
    /// Maximum buffered bytes per HTTP response.
    pub max_response_bytes: usize,
    /// Maximum serialized bytes per outgoing request body.
    pub max_request_bytes: usize,
    /// Maximum bytes admitted to each redacted request or response preview.
    pub max_preview_bytes: usize,
    /// Total retained report budget, including snapshot and graph.
    pub max_report_bytes: usize,
    /// Bounded live-channel capacity; a full channel pauses execution.
    pub event_capacity: usize,
    /// Additional case-insensitive field names whose values must be redacted.
    pub sensitive_fields: Vec<String>,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_response_bytes: 65_536,
            max_request_bytes: 65_536,
            max_preview_bytes: 16_384,
            max_report_bytes: 8_388_608,
            event_capacity: 64,
            sensitive_fields: Vec::new(),
        }
    }
}
/// Terminal outcome of a run, lifecycle phase, or workflow invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    /// Execution completed successfully.
    Passed,
    /// Execution completed with a failed outcome.
    Failed,
    /// Execution was cancelled.
    Cancelled,
    /// Execution stopped because of a runtime or resource failure.
    Error,
}
/// Safe structured failure suitable for reports and adapter error messages.
#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[error("{message}")]
#[serde(rename_all = "camelCase")]
pub struct RunFailure {
    /// Stable machine-readable failure category.
    pub kind: String,
    /// Safe human-readable explanation without credential values.
    pub message: String,
}
impl From<HttpError> for RunFailure {
    fn from(error: HttpError) -> Self {
        Self {
            kind: match error.kind {
                interl_http::HttpErrorKind::Configuration => "configuration",
                interl_http::HttpErrorKind::Dns => "dns",
                interl_http::HttpErrorKind::Tls => "tls",
                interl_http::HttpErrorKind::Connect => "connect",
                interl_http::HttpErrorKind::Timeout => "timeout",
                interl_http::HttpErrorKind::Cancelled => "cancelled",
                interl_http::HttpErrorKind::Body => "body",
                interl_http::HttpErrorKind::Transport => "transport",
            }
            .into(),
            message: error.message,
        }
    }
}
/// Tagged live-event payloads shared by adapters and retained reports.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "payload", rename_all_fields = "camelCase")]
pub enum EventKind {
    #[serde(rename = "run.started")]
    /// Run initialization and safe graph projection.
    RunStarted {
        /// Redacted scenario identifier.
        scenario_id: String,
        /// Safe positional graph projection.
        graph: Value,
    },
    #[serde(rename = "lifecycle.started")]
    /// Beginning of a lifecycle phase.
    LifecycleStarted {
        /// Lifecycle phase name.
        phase: String,
    },
    #[serde(rename = "lifecycle.finished")]
    /// Terminal outcome of a lifecycle phase.
    LifecycleFinished {
        /// Lifecycle phase name.
        phase: String,
        /// Outcome of this execution scope, or HTTP response status.
        status: RunStatus,
    },
    #[serde(rename = "workflow.started")]
    /// Beginning of a workflow invocation.
    WorkflowStarted {},
    #[serde(rename = "workflow.finished")]
    /// Terminal outcome of a workflow invocation.
    WorkflowFinished {
        /// Outcome of this execution scope, or HTTP response status.
        status: RunStatus,
    },
    #[serde(rename = "edge.traversed")]
    /// Traversal of a compiled workflow edge.
    EdgeTraversed {},
    /// Selected and skipped outgoing edges, expressed only as generated graph keys.
    #[serde(rename = "route.selected")]
    RouteSelected {
        /// The single selected outgoing edge key.
        selected_edge_key: String,
        /// Other outgoing edge keys; their branch nodes were not executed.
        unselected_edge_keys: Vec<String>,
    },
    #[serde(rename = "cleanup.skipped")]
    /// Cleanup was omitted with an explicit reason.
    CleanupSkipped {
        /// Safe reason for skipping cleanup.
        reason: String,
    },
    #[serde(rename = "user.spawned")]
    /// The single virtual user began execution.
    UserSpawned {},
    #[serde(rename = "step.started")]
    /// Beginning of a node execution.
    StepStarted {
        /// Scenario node kind.
        kind: String,
    },
    #[serde(rename = "request.sent")]
    /// Safe outgoing request metadata and redacted previews.
    RequestSent {
        /// HTTP request method.
        method: String,
        /// Sanitized target URL.
        url: String,
        /// Redacted header preview.
        headers: Value,
        /// Redacted or omitted body preview.
        body: Value,
    },
    #[serde(rename = "response.received")]
    /// Safe received response metadata and redacted previews.
    ResponseReceived {
        /// Outcome of this execution scope, or HTTP response status.
        status: u16,
        /// Redacted header preview.
        headers: Value,
        /// Redacted or omitted body preview.
        body: Value,
    },
    #[serde(rename = "session.updated")]
    /// Names of session headers changed; header values are excluded.
    SessionUpdated {
        /// Names of headers assigned to the session.
        set_headers: Vec<String>,
        /// Names of headers removed from the session.
        removed_headers: Vec<String>,
    },
    #[serde(rename = "step.finished")]
    /// Successful completion of a node execution.
    StepFinished {},
    #[serde(rename = "step.failed")]
    /// A node stopped with a safe structured failure.
    StepFailed {
        /// Safe structured failure.
        error: RunFailure,
    },
    #[serde(rename = "user.finished")]
    /// Terminal outcome of the virtual user.
    UserFinished {
        /// Outcome of this execution scope, or HTTP response status.
        status: RunStatus,
    },
    #[serde(rename = "run.cancelled")]
    /// Run cancellation was observed.
    RunCancelled {},
    #[serde(rename = "run.finished")]
    /// Terminal outcome of the complete run.
    RunFinished {
        /// Outcome of this execution scope, or HTTP response status.
        status: RunStatus,
    },
}
/// One ordered, redacted observation with its workflow invocation context.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    /// Unique identifier of the run producing this event.
    pub run_id: String,
    /// One-based sequence number in retained event order.
    pub sequence: u64,
    /// Nanoseconds since trace creation, saturated at u64::MAX.
    pub elapsed_ns: u64,
    /// Virtual-user identifier, currently the single user vu:1.
    pub user_id: Option<String>,
    /// Redacted source node label, when applicable.
    pub node_id: Option<String>,
    #[serde(flatten)]
    /// Tagged event type and payload, flattened by Serde into the wire object.
    pub event: EventKind,
    /// Graph and invocation coordinates for this observation.
    pub context: EventContext,
}
/// Stable graph keys and invocation identifiers locating an event.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventContext {
    /// Stable positional key of the executing group.
    pub group_key: Option<String>,
    /// Lifecycle phase: initialize, iteration, or cleanup.
    pub phase: Option<String>,
    /// One-based iteration number, present during iteration.
    pub iteration: Option<u32>,
    /// Stable positional workflow key.
    pub workflow_key: Option<String>,
    /// Unique identifier of this workflow call within the run.
    pub invocation_id: Option<String>,
    /// Calling workflow invocation, absent for lifecycle roots.
    pub parent_invocation_id: Option<String>,
    /// Graph key of the node that invoked this workflow.
    pub caller_node_key: Option<String>,
    /// Graph key of the node producing the event.
    pub node_key: Option<String>,
    /// Graph key of the traversed edge.
    pub edge_key: Option<String>,
}
/// Cleanup result recorded separately from the primary run failure.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupOutcome {
    /// Cleanup disposition such as notConfigured, completed, skipped, or error.
    pub status: String,
    /// Safe cleanup failure, retained independently of the primary error.
    pub error: Option<RunFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Reason cleanup was skipped, omitted from JSON when absent.
    pub reason: Option<String>,
}
impl Default for CleanupOutcome {
    fn default() -> Self {
        Self {
            status: "notConfigured".into(),
            error: None,
            reason: None,
        }
    }
}
/// Aggregate transport counts and elapsed time for the complete run.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    /// Number of requests submitted to the transport.
    pub requests: u32,
    /// Number of responses received by the interpreter.
    pub responses: u32,
    /// Responses with an HTTP status of 400 or higher.
    pub http_error_responses: u32,
    /// Elapsed nanoseconds for the complete run.
    pub elapsed_ns: u64,
}
/// Bounded, redacted result returned when the interpreter task finishes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    /// Report wire-format version.
    pub report_version: u32,
    /// Unique identifier shared with live events.
    pub run_id: String,
    /// Primary terminal outcome, including cleanup failure when no primary failure exists.
    pub status: RunStatus,
    /// Redacted scenario snapshot captured before execution.
    pub snapshot: Value,
    /// Safe graph projection with stable positional keys.
    pub graph: Value,
    /// Independent cleanup disposition and failure details.
    pub cleanup: CleanupOutcome,
    /// Aggregate transport counts and duration.
    pub summary: RunSummary,
    /// Retained redacted events in sequence order.
    pub events: Vec<RunEvent>,
    /// Primary safe failure, if execution failed.
    pub error: Option<RunFailure>,
    /// Whether the trace stopped admitting events at a safety boundary.
    pub events_truncated: bool,
    /// Retained events that could not be delivered to the live receiver.
    pub delivery_dropped_events: u64,
}

/// Ownership of a running task, its event receiver, and cancellation controls.
///
/// Drain `events` concurrently with `task` to avoid blocking on a full channel.
/// Dropping the receiver stops execution; dropping a Tokio JoinHandle alone does
/// not cancel its task. Use `control.stop(StopCause::OwnerClosed)` when closing an adapter.
pub struct RunHandle {
    /// Unique identifier assigned when execution starts.
    pub run_id: String,
    /// Bounded live event stream; drain it concurrently with the task.
    pub events: mpsc::Receiver<RunEvent>,
    /// Tokio task yielding the final retained report.
    pub task: JoinHandle<RunReport>,
    /// Shared cancellation token; cancelling it requests user cancellation.
    pub cancel: CancellationToken,
    /// Shared control supporting user cancellation and stronger shutdown causes.
    pub control: RunControl,
}

#[cfg(test)]
mod tests {
    use super::*;
    use interl_http::HttpErrorKind;

    #[test]
    fn transport_failure_categories_keep_their_wire_names() {
        for kind in [
            HttpErrorKind::Configuration,
            HttpErrorKind::Dns,
            HttpErrorKind::Tls,
            HttpErrorKind::Connect,
            HttpErrorKind::Timeout,
            HttpErrorKind::Cancelled,
            HttpErrorKind::Body,
            HttpErrorKind::Transport,
        ] {
            let expected = serde_json::to_value(kind).unwrap();
            let failure = RunFailure::from(HttpError {
                kind,
                message: "safe".into(),
            });
            assert_eq!(serde_json::json!(failure.kind), expected);
        }
    }
}
