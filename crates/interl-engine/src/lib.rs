//! Shared single-user runner. Adapters consume the same bounded, redacted trace.
mod extract;
mod redaction;
mod secrets;
mod security;
mod user;
use interl_core::{
    compile_scenario, Diagnostic, ErrorCategory, ExecutionPlan, HttpConfig, RequestBody,
    ScenarioDocument, WorkflowNode,
};
use interl_http::{HttpError, HttpSession, PreparedRequest};
pub use secrets::{
    EnvironmentSecretProvider, SecretLookupError, SecretProvider, MAX_CREDENTIAL_BYTES,
    MAX_CREDENTIAL_REFERENCES, MAX_TOTAL_CREDENTIAL_BYTES,
};
use security::RunSecurity;
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
    user: user::UserContext,
    options: RunOptions,
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
        Self::new_with_secret_provider(document, options, &EnvironmentSecretProvider)
    }
    /// Resolve only required references after static preflight, once per accepted
    /// preparation. No requests are sent and no provider is retained.
    pub fn new_with_secret_provider(
        document: &ScenarioDocument,
        options: RunOptions,
        provider: &dyn SecretProvider,
    ) -> Result<Self, Vec<Diagnostic>> {
        let plan = compile_scenario(document)?;
        let invalid = |message: &str| {
            vec![Diagnostic::new(
                ErrorCategory::Validation,
                "runConfiguration",
                "$",
                message,
            )]
        };
        if document.groups.len() != 1 || document.groups[0].count != 1 {
            return Err(invalid("Execution requires exactly one group with count 1"));
        }
        if plan.steps().len() > 256 {
            return Err(invalid("Execution supports at most 256 workflow nodes"));
        }
        if document.id.len() > 128
            || document.groups[0].id.len() > 128
            || plan.steps().iter().any(|n| n.id().len() > 128)
        {
            return Err(invalid("Run identifiers must be at most 128 bytes"));
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
                static_request(&session, config, &document.environment.variables)
                    .map_err(|e| invalid(&e.message))?;
            }
        }
        let mut security =
            RunSecurity::prepare(document, &plan, options.sensitive_fields.clone(), provider)
                .map_err(|error| vec![error])?;
        // Learn one URL at a time; expanded paths can be much larger than
        // their source and must not accumulate across the whole workflow.
        for node in plan.steps() {
            if let WorkflowNode::Http { config, .. } = node {
                let request = static_request(&session, config, &document.environment.variables)
                    .map_err(|e| invalid(&e.message))?;
                security.learn_url(request.url());
            }
        }
        let snapshot = security.snapshot(document);
        if serde_json::to_vec(&snapshot)
            .expect("snapshot is serializable")
            .len()
            + 32768
            > options.max_report_bytes
        {
            return Err(invalid("Redacted snapshot exceeds the run report budget"));
        }
        let user = user::UserContext::new(
            document,
            plan.auth(),
            session,
            security,
            options.max_response_bytes,
        )
        .map_err(|error| invalid(&error.message))?;
        Ok(Self {
            plan,
            document: document.clone(),
            user,
            options,
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
            .user
            .security
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
                    scenario_id: self.user.security.text(self.plan.scenario_id()),
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
            let id = self.user.security.text(node.id());
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
                user::check_cancel(&cancel)?;
                match node {
                WorkflowNode::Http { config, .. } => {
                    let request = self.user.prepare_http(config, &self.document.environment.variables, &cancel)?;
                    let event = self.user.security.request_event(node.id(), &request, self.options.max_preview_bytes);
                    if !trace.emit(Some(&id), event, false).await {
                        return Err(report_limit());
                    }
                    user::check_cancel(&cancel)?;
                    trace.summary.requests += 1;
                    let response = tokio::select! {
                        biased;
                        _ = trace.sender.closed() => { cancel.cancel(); return Err(RunFailure { kind: "cancelled".into(), message: "Run cancelled: event receiver closed".into() }); },
                        response = self.user.session.execute(request, &cancel) => response.map_err(RunFailure::from)?,
                    };
                    trace.summary.responses += 1;
                    if response.status >= 400 {
                        trace.summary.http_error_responses += 1;
                    }
                    let event = self.user.security.response_event(node.id(), &response, self.options.max_preview_bytes);
                    if !trace.emit(Some(&id), event, false).await {
                        return Err(report_limit());
                    }
                    self.user.retain_response(node.id(), response, &cancel)?;
                }
                WorkflowNode::Extract { .. } => { self.user.extract(node.id(), &cancel)?; }
                WorkflowNode::SetSession { config, .. } => {
                    self.user.update_session(config, &cancel)?;
                    let event = EventKind::SessionUpdated {
                        set_headers: config.set_headers.keys().map(|name| self.user.security.text(&name.to_ascii_lowercase())).collect(),
                        removed_headers: config.remove_headers.iter().map(|name| self.user.security.text(&name.to_ascii_lowercase())).collect(),
                    };
                    if !trace.emit(Some(&id), event, false).await { return Err(report_limit()); }
                }
                WorkflowNode::Start { .. } | WorkflowNode::End { .. } => {}
                }
                user::check_cancel(&cancel)?;
                if !trace
                    .emit(Some(&id), EventKind::StepFinished {}, false)
                    .await
                {
                    return Err(report_limit());
                }
                self.user.release_after(node.id());
                Ok::<(), RunFailure>(())
            }
            .await;
            if let Err(mut error) = result {
                error.message = self.user.security.text(&error.message);
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
            report_version: 2,
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
// Dynamic outputs do not exist during preflight. Validate static HTTP
// settings now; materialized bodies/effective headers are checked before send.
fn static_request(
    session: &HttpSession,
    config: &HttpConfig,
    variables: &std::collections::BTreeMap<String, Value>,
) -> Result<PreparedRequest, HttpError> {
    let mut config = config.clone();
    if matches!(&config.body, Some(RequestBody::JsonTemplate { .. })) {
        config.body = None;
    }
    session.prepare(&config, variables)
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
