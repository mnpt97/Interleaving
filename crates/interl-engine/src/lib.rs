//! Shared single-user runner. Adapters consume the same bounded, redacted trace.
mod redaction;
use interl_core::{
    compile_scenario, Diagnostic, ErrorCategory, ExecutionPlan, ScenarioDocument, WorkflowNode,
};
use interl_http::{HttpError, HttpSession};
use redaction::Redactor;
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
    RunStarted { scenario_id: String },
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
    pub summary: RunSummary,
    pub events: Vec<RunEvent>,
    pub error: Option<RunFailure>,
    pub events_truncated: bool,
    pub delivery_dropped_events: u64,
}

/// Owns a validated immutable snapshot and one isolated session; construction sends no requests.
pub struct PreparedRun {
    plan: ExecutionPlan,
    document: ScenarioDocument,
    session: HttpSession,
    options: RunOptions,
    redactor: Redactor,
    snapshot: Value,
}
pub struct RunHandle {
    pub run_id: String,
    pub events: mpsc::Receiver<RunEvent>,
    pub task: JoinHandle<RunReport>,
    pub cancel: CancellationToken,
}
impl PreparedRun {
    pub fn new(document: &ScenarioDocument, options: RunOptions) -> Result<Self, Vec<Diagnostic>> {
        let plan = compile_scenario(document)?;
        let invalid = |message: &str| {
            vec![Diagnostic::new(
                ErrorCategory::Validation,
                "m2Configuration",
                "$",
                message,
            )]
        };
        if document.groups.len() != 1 || document.groups[0].count != 1 {
            return Err(invalid(
                "M2 execution requires exactly one group with count 1",
            ));
        }
        if plan.steps().len() > 256 {
            return Err(invalid("M2 execution supports at most 256 workflow nodes"));
        }
        if document.id.len() > 128
            || document.groups[0].id.len() > 128
            || plan.steps().iter().any(|n| n.id().len() > 128)
        {
            return Err(invalid("M2 run identifiers must be at most 128 bytes"));
        }
        if !(1..=1024).contains(&options.event_capacity)
            || !(1..=65_536).contains(&options.max_preview_bytes)
            || !(65_536..=16_777_216).contains(&options.max_report_bytes)
            || options.sensitive_fields.len() > 64
            || options.sensitive_fields.iter().any(|s| s.len() > 128)
        {
            return Err(invalid("Invalid run resource limits"));
        }
        let profile = document
            .client_profiles
            .iter()
            .find(|p| p.id == document.groups[0].client_profile_id)
            .expect("core validated references");
        let session = HttpSession::new(
            profile,
            options.max_response_bytes,
            options.max_request_bytes,
        )
        .map_err(|e| invalid(&e.message))?;
        for node in plan.steps() {
            if let WorkflowNode::Http { config, .. } = node {
                session
                    .prepare(config, &document.environment.variables)
                    .map_err(|e| invalid(&e.message))?;
            }
        }
        let mut redactor = Redactor::new(options.sensitive_fields.clone());
        redactor.learn(&serde_json::to_value(document).expect("scenario is serializable"));
        for value in document.environment.variables.values() {
            redactor.remember_value(value);
        }
        for node in plan.steps() {
            if let WorkflowNode::Http { config, .. } = node {
                let request = session
                    .prepare(config, &document.environment.variables)
                    .expect("requests were preflighted");
                redactor.learn_url(request.url());
            }
        }
        for profile in &document.client_profiles {
            if let Ok(url) = interl_http::Url::parse(&profile.base_url) {
                redactor.learn_url(&url);
            }
        }
        let snapshot = redactor.snapshot(document);
        if serde_json::to_vec(&snapshot)
            .expect("snapshot is serializable")
            .len()
            + 32768
            > options.max_report_bytes
        {
            return Err(invalid("Redacted snapshot exceeds the run report budget"));
        }
        Ok(Self {
            plan,
            document: document.clone(),
            session,
            options,
            redactor,
            snapshot,
        })
    }
    pub fn start(self, cancel: CancellationToken) -> RunHandle {
        let run_id = uuid::Uuid::new_v4().to_string();
        let (sender, events) = mpsc::channel(self.options.event_capacity);
        let task = tokio::spawn(self.execute(run_id.clone(), sender, cancel.clone()));
        RunHandle {
            run_id,
            events,
            task,
            cancel,
        }
    }
    async fn execute(
        mut self,
        run_id: String,
        sender: mpsc::Sender<RunEvent>,
        cancel: CancellationToken,
    ) -> RunReport {
        let user = self
            .redactor
            .text(&format!("{}:1", self.document.groups[0].id));
        let mut trace = Trace::new(
            run_id,
            user,
            sender,
            cancel.clone(),
            self.snapshot,
            self.options.max_report_bytes,
        );
        let mut status = RunStatus::Passed;
        let mut failure = None;
        trace
            .emit(
                None,
                EventKind::RunStarted {
                    scenario_id: self.redactor.text(self.plan.scenario_id()),
                },
                false,
            )
            .await;
        trace.emit(None, EventKind::UserSpawned {}, false).await;
        for node in self.plan.steps() {
            if cancel.is_cancelled() {
                status = RunStatus::Cancelled;
                break;
            }
            let id = self.redactor.text(node.id());
            let result = async {
                if !trace
                    .emit(
                        Some(&id),
                        EventKind::StepStarted {
                            kind: node.kind().into(),
                        },
                        false,
                    )
                    .await
                {
                    return Err(report_limit());
                }
                if let WorkflowNode::Http { config, .. } = node {
                    let request = self
                        .session
                        .prepare(config, &self.document.environment.variables)
                        .map_err(RunFailure::from)?;
                    self.redactor.learn_headers(request.headers());
                    let request_body_bytes = request
                        .body
                        .as_ref()
                        .map(|v| serde_json::to_vec(v).expect("JSON is serializable").len())
                        .unwrap_or_default();
                    let event = EventKind::RequestSent {
                        method: request.method().into(),
                        url: self.redactor.url(request.url().as_str()),
                        headers: self.redactor.headers(request.headers()),
                        body: self.redactor.body(
                            request.body.as_ref(),
                            request_body_bytes,
                            false,
                            self.options.max_preview_bytes,
                        ),
                    };
                    if !trace.emit(Some(&id), event, false).await {
                        return Err(report_limit());
                    }
                    trace.summary.requests += 1;
                    let response = tokio::select! {
                        biased;
                        _ = trace.sender.closed() => { cancel.cancel(); return Err(RunFailure { kind: "cancelled".into(), message: "Run cancelled: event receiver closed".into() }); },
                        response = self.session.execute(request, &cancel) => response.map_err(RunFailure::from)?,
                    };
                    self.redactor.learn_headers(&response.headers);
                    let body: Option<Value> = if response.truncated {
                        None
                    } else {
                        serde_json::from_slice(&response.body).ok()
                    };
                    if let Some(body) = &body {
                        self.redactor.learn(body);
                    }
                    trace.summary.responses += 1;
                    if response.status >= 400 {
                        trace.summary.http_error_responses += 1;
                    }
                    let event = EventKind::ResponseReceived {
                        status: response.status,
                        headers: self.redactor.headers(&response.headers),
                        body: self.redactor.body(
                            body.as_ref(),
                            response.body.len(),
                            response.truncated,
                            self.options.max_preview_bytes,
                        ),
                    };
                    if !trace.emit(Some(&id), event, false).await {
                        return Err(report_limit());
                    }
                }
                if !trace
                    .emit(Some(&id), EventKind::StepFinished {}, false)
                    .await
                {
                    return Err(report_limit());
                }
                Ok::<(), RunFailure>(())
            }
            .await;
            if let Err(error) = result {
                status = if cancel.is_cancelled() || error.kind == "cancelled" {
                    RunStatus::Cancelled
                } else {
                    RunStatus::Error
                };
                trace
                    .emit(
                        Some(&id),
                        EventKind::StepFailed {
                            error: error.clone(),
                        },
                        true,
                    )
                    .await;
                failure = Some(error);
                break;
            }
        }
        if cancel.is_cancelled() && status == RunStatus::Passed {
            status = RunStatus::Cancelled;
        }
        trace
            .emit(None, EventKind::UserFinished { status }, true)
            .await;
        if status == RunStatus::Cancelled {
            trace.emit(None, EventKind::RunCancelled {}, true).await;
        }
        trace
            .emit(None, EventKind::RunFinished { status }, true)
            .await;
        trace.summary.elapsed_ns = trace.elapsed();
        RunReport {
            report_version: 1,
            run_id: trace.run_id,
            status,
            snapshot: trace.snapshot,
            summary: trace.summary,
            events: trace.events,
            error: failure,
            events_truncated: trace.truncated,
            delivery_dropped_events: trace.dropped,
        }
    }
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
        let event = RunEvent {
            run_id: self.run_id.clone(),
            sequence: self.events.len() as u64 + 1,
            elapsed_ns: self.elapsed(),
            user_id: Some(self.user.clone()),
            node_id: node.map(str::to_owned),
            event,
        };
        let bytes = serde_json::to_vec(&event)
            .expect("event is serializable")
            .len()
            + 1;
        if !terminal && self.bytes + bytes > self.limit - 16384 {
            self.truncated = true;
            return false;
        }
        self.bytes += bytes;
        self.events.push(event.clone());
        // Backpressure during execution. On cancellation keep terminal events in the report,
        // and explicitly count any live events the full/closed channel cannot accept.
        tokio::select! { biased;
            _ = self.cancel.cancelled() => { if self.sender.try_send(event).is_err() { self.dropped += 1; } },
            result = self.sender.send(event.clone()) => if result.is_err() { self.dropped += 1; self.cancel.cancel(); },
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
