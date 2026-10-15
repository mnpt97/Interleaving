//! Shared single-user runner. Adapters consume the same bounded, redacted trace.
mod extract;
mod program;
mod redaction;
mod secrets;
mod security;
pub use program::PreparedRun;
mod control;
pub use control::{RunControl, StopCause};
use interl_http::HttpError;
pub use secrets::{
    EnvironmentSecretProvider, SecretLookupError, SecretProvider, MAX_CREDENTIAL_BYTES,
    MAX_CREDENTIAL_REFERENCES, MAX_TOTAL_CREDENTIAL_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;
use tokio::{sync::mpsc, task::JoinHandle};
pub use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RunOptions {
    pub max_response_bytes: usize,
    pub max_request_bytes: usize,
    pub max_preview_bytes: usize,
    pub max_report_bytes: usize,
    pub event_capacity: usize,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Passed,
    Failed,
    Cancelled,
    Error,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFailure {
    pub kind: String,
    pub message: String,
}
impl From<HttpError> for RunFailure {
    fn from(error: HttpError) -> Self {
        Self {
            kind: serde_json::to_value(error.kind)
                .expect("enum is serializable")
                .as_str()
                .unwrap()
                .into(),
            message: error.message,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "payload", rename_all_fields = "camelCase")]
pub enum EventKind {
    #[serde(rename = "run.started")]
    RunStarted { scenario_id: String, graph: Value },
    #[serde(rename = "lifecycle.started")]
    LifecycleStarted { phase: String },
    #[serde(rename = "lifecycle.finished")]
    LifecycleFinished { phase: String, status: RunStatus },
    #[serde(rename = "workflow.started")]
    WorkflowStarted {},
    #[serde(rename = "workflow.finished")]
    WorkflowFinished { status: RunStatus },
    #[serde(rename = "edge.traversed")]
    EdgeTraversed {},
    #[serde(rename = "cleanup.skipped")]
    CleanupSkipped { reason: String },
    #[serde(rename = "user.spawned")]
    UserSpawned {},
    #[serde(rename = "step.started")]
    StepStarted { kind: String },
    #[serde(rename = "request.sent")]
    RequestSent {
        method: String,
        url: String,
        headers: Value,
        body: Value,
    },
    #[serde(rename = "response.received")]
    ResponseReceived {
        status: u16,
        headers: Value,
        body: Value,
    },
    #[serde(rename = "session.updated")]
    SessionUpdated {
        set_headers: Vec<String>,
        removed_headers: Vec<String>,
    },
    #[serde(rename = "step.finished")]
    StepFinished {},
    #[serde(rename = "step.failed")]
    StepFailed { error: RunFailure },
    #[serde(rename = "user.finished")]
    UserFinished { status: RunStatus },
    #[serde(rename = "run.cancelled")]
    RunCancelled {},
    #[serde(rename = "run.finished")]
    RunFinished { status: RunStatus },
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunEvent {
    pub run_id: String,
    pub sequence: u64,
    pub elapsed_ns: u64,
    pub user_id: Option<String>,
    pub node_id: Option<String>,
    #[serde(flatten)]
    pub event: EventKind,
    pub context: EventContext,
}
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventContext {
    pub group_key: Option<String>,
    pub phase: Option<String>,
    pub iteration: Option<u32>,
    pub workflow_key: Option<String>,
    pub invocation_id: Option<String>,
    pub parent_invocation_id: Option<String>,
    pub caller_node_key: Option<String>,
    pub node_key: Option<String>,
    pub edge_key: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupOutcome {
    pub status: String,
    pub error: Option<RunFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub requests: u32,
    pub responses: u32,
    pub http_error_responses: u32,
    pub elapsed_ns: u64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub report_version: u32,
    pub run_id: String,
    pub status: RunStatus,
    pub snapshot: Value,
    pub graph: Value,
    pub cleanup: CleanupOutcome,
    pub summary: RunSummary,
    pub events: Vec<RunEvent>,
    pub error: Option<RunFailure>,
    pub events_truncated: bool,
    pub delivery_dropped_events: u64,
}

pub struct RunHandle {
    pub run_id: String,
    pub events: mpsc::Receiver<RunEvent>,
    pub task: JoinHandle<RunReport>,
    pub cancel: CancellationToken,
    pub control: RunControl,
}
fn report_limit() -> RunFailure {
    RunFailure {
        kind: "resourceLimit".into(),
        message: "Run report byte limit reached; execution stopped".into(),
    }
}
struct Trace {
    run_id: String,
    user: String,
    sender: mpsc::Sender<RunEvent>,
    cancel: CancellationToken,
    start: Instant,
    snapshot: Value,
    summary: RunSummary,
    events: Vec<RunEvent>,
    bytes: usize,
    limit: usize,
    truncated: bool,
    dropped: u64,
    context: EventContext,
    control: RunControl,
    cleanup_deadline: Option<tokio::time::Instant>,
    cleanup_expired: bool,
}
impl Trace {
    fn new(
        run_id: String,
        user: String,
        sender: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
        snapshot: Value,
        limit: usize,
    ) -> Self {
        let bytes = serde_json::to_vec(&snapshot)
            .expect("snapshot is serializable")
            .len()
            + 1024;
        Self {
            run_id,
            user,
            sender,
            control: RunControl::from_token(cancel.clone()),
            context: EventContext::default(),
            cleanup_deadline: None,
            cleanup_expired: false,
            cancel,
            start: Instant::now(),
            snapshot,
            summary: RunSummary::default(),
            events: Vec::new(),
            bytes,
            limit,
            truncated: false,
            dropped: 0,
        }
    }
    fn elapsed(&self) -> u64 {
        self.start.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
    async fn emit(&mut self, node: Option<&str>, event: EventKind, terminal: bool) -> bool {
        if !terminal
            && self
                .cleanup_deadline
                .is_some_and(|d| tokio::time::Instant::now() >= d)
        {
            self.cleanup_expired = true;
            return false;
        }
        let event = RunEvent {
            run_id: self.run_id.clone(),
            sequence: self.events.len() as u64 + 1,
            elapsed_ns: self.elapsed(),
            user_id: Some(self.user.clone()),
            node_id: node.map(str::to_owned),
            event,
            context: self.context.clone(),
        };
        let bytes = serde_json::to_vec(&event)
            .expect("event is serializable")
            .len()
            + 1;
        if !terminal
            && (self.bytes + bytes > self.limit.saturating_sub(65536)
                || self.events.len() >= 2048 - 64)
        {
            self.truncated = true;
            return false;
        }
        self.bytes += bytes;
        self.events.push(event.clone());
        // Backpressure during execution. On cancellation keep terminal events in the report,
        // and explicitly count any live events the full/closed channel cannot accept.
        tokio::select! { biased;
            _ = async {if !terminal {if let Some(d)=self.cleanup_deadline {tokio::time::sleep_until(d).await;return;}}std::future::pending::<()>().await} => {self.dropped+=1;self.cleanup_expired=true;return false;},
            _ = self.cancel.cancelled() => { if let Err(e)=self.sender.try_send(event) { self.dropped += 1; if matches!(e,mpsc::error::TrySendError::Closed(_)){self.control.stop(StopCause::ReceiverClosed);} } },
            result = self.sender.send(event.clone()) => if result.is_err() { self.dropped += 1; self.control.stop(StopCause::ReceiverClosed); },
        }
        true
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
