//! One interpreter for V2 reusable workflows and single-VU lifecycles.
mod interpreter;
mod values;
use crate::{extract::ExtractionState, security::RunSecurity, *};
use interl_core::{v2::*, ScenarioDocument};
use interl_http::{HttpSession, ResolvedBody};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct PreparedRun {
    program: Program,
    session: HttpSession,
    security: RunSecurity,
    options: RunOptions,
    snapshot: Value,
    graph: Value,
}
struct Frame {
    flow: usize,
    pc: usize,
    invocation: String,
    parent: Option<String>,
    caller: Option<String>,
    values: BTreeMap<ValueKey, Value>,
    extraction: ExtractionState,
    pending: bool,
}
struct Runtime {
    program: Program,
    session: HttpSession,
    security: RunSecurity,
    options: RunOptions,
    trace: Trace,
    control: RunControl,
    frames: Vec<Frame>,
    next_invocation: usize,
    visits: usize,
    phase: String,
    cleanup_deadline: Option<tokio::time::Instant>,
}
fn failure(kind: &str, message: &str) -> RunFailure {
    RunFailure {
        kind: kind.into(),
        message: message.into(),
    }
}
fn check(cancel: &CancellationToken) -> Result<(), RunFailure> {
    if cancel.is_cancelled() {
        Err(failure("cancelled", "Run cancelled"))
    } else {
        Ok(())
    }
}
fn config(http: &ProgramHttp, body: Option<interl_core::RequestBody>) -> interl_core::HttpConfig {
    interl_core::HttpConfig {
        method: http.method,
        path: http.path.clone(),
        headers: http.headers.clone(),
        timeout_ms: http.timeout_ms,
        body,
    }
}
fn static_config(http: &ProgramHttp) -> interl_core::HttpConfig {
    config(
        http,
        match &http.body {
            Some(ProgramBody::Json { value }) => Some(interl_core::RequestBody::Json {
                value: value.clone(),
            }),
            Some(ProgramBody::Text { value }) => Some(interl_core::RequestBody::Text {
                value: value.clone(),
            }),
            _ => None,
        },
    )
}
impl PreparedRun {
    pub fn new(
        document: &ScenarioDocument,
        options: RunOptions,
    ) -> Result<Self, Vec<interl_core::Diagnostic>> {
        Self::new_with_secret_provider(document, options, &EnvironmentSecretProvider)
    }
    pub fn new_with_secret_provider(
        document: &ScenarioDocument,
        options: RunOptions,
        provider: &dyn SecretProvider,
    ) -> Result<Self, Vec<interl_core::Diagnostic>> {
        let program = compile_document(document)?;
        let invalid = |message: &str| {
            vec![interl_core::Diagnostic::new(
                interl_core::ErrorCategory::Validation,
                "runConfiguration",
                "$",
                message,
            )]
        };
        if !(1..=1024).contains(&options.event_capacity)
            || !(1..=65536).contains(&options.max_preview_bytes)
            || !(131072..=16777216).contains(&options.max_report_bytes)
            || options.sensitive_fields.len() > 64
            || options.sensitive_fields.iter().any(|s| s.len() > 128)
        {
            return Err(invalid("Invalid run resource limits"));
        }
        let doc = program.document();
        let group = &doc.groups[0];
        let profile = doc
            .client_profiles
            .iter()
            .find(|p| p.id == group.client_profile_id)
            .expect("validated profile");
        let session = HttpSession::new(
            profile,
            options.max_response_bytes,
            options.max_request_bytes,
        )
        .map_err(|e| invalid(&e.message))?;
        for i in program.reachable() {
            for n in &program.workflows()[*i].definition().nodes {
                if let ProgramNode::Http { config, .. } = n {
                    session
                        .prepare(&static_config(config), &doc.environment.variables)
                        .map_err(|e| invalid(&e.message))?;
                }
            }
        }
        let mut security =
            RunSecurity::prepare_program(&program, options.sensitive_fields.clone(), provider)
                .map_err(|e| vec![e])?;
        for i in program.reachable() {
            for n in &program.workflows()[*i].definition().nodes {
                if let ProgramNode::Http { config, .. } = n {
                    let request = session
                        .prepare(&static_config(config), &doc.environment.variables)
                        .map_err(|e| invalid(&e.message))?;
                    security.learn_url(request.url());
                }
            }
        }
        let snapshot = security.program_snapshot(&program);
        let graph = graph(&program, &security);
        if serde_json::to_vec(&snapshot).expect("snapshot").len()
            + 2 * serde_json::to_vec(&graph).expect("graph").len()
            + 65536
            + 4096
            > options.max_report_bytes
        {
            return Err(invalid(
                "Redacted snapshot and graph exceed the report budget",
            ));
        }
        Ok(Self {
            program,
            session,
            security,
            options,
            snapshot,
            graph,
        })
    }
    pub fn start(self, cancel: CancellationToken) -> RunHandle {
        self.start_control(RunControl::from_token(cancel))
    }
    pub fn start_control(self, control: RunControl) -> RunHandle {
        let run_id = uuid::Uuid::new_v4().to_string();
        let (sender, events) = mpsc::channel(self.options.event_capacity);
        let mut trace = Trace::new(
            run_id.clone(),
            "vu:1".into(),
            sender,
            control.token.clone(),
            self.snapshot,
            self.options.max_report_bytes,
        );
        trace.bytes += serde_json::to_vec(&self.graph).expect("graph").len();
        trace.control = control.clone();
        let runtime = Runtime {
            program: self.program,
            session: self.session,
            security: self.security,
            options: self.options,
            trace,
            control: control.clone(),
            frames: Vec::new(),
            next_invocation: 0,
            visits: 0,
            phase: String::new(),
            cleanup_deadline: None,
        };
        let task = tokio::spawn(runtime.run(self.graph));
        RunHandle {
            run_id,
            events,
            task,
            cancel: control.token.clone(),
            control,
        }
    }
}
fn graph(program: &Program, security: &RunSecurity) -> Value {
    let label = |s: &str| security.text(s).chars().take(256).collect::<String>();
    let flows=program.workflows().iter().enumerate().map(|(i,w)|{
        let d=w.definition();let key=|id:&str|format!("wf:{i}/node:{}",d.nodes.iter().position(|n|n.id()==id).expect("validated edge"));
        json!({"key":format!("wf:{i}"),"id":label(&d.id),"name":label(&d.name),"nodes":d.nodes.iter().enumerate().map(|(j,n)|json!({"key":format!("wf:{i}/node:{j}"),"id":label(n.id()),"type":n.kind(),"position":d.layout.as_ref().and_then(|l|l.positions.get(n.id())).copied().unwrap_or(interl_core::Position{x:80.0+j as f64*260.0,y:160.0})})).collect::<Vec<_>>(),"edges":d.edges.iter().enumerate().map(|(j,e)|json!({"key":format!("wf:{i}/edge:{j}"),"id":label(&e.id),"fromNodeKey":key(&e.from),"toNodeKey":key(&e.to)})).collect::<Vec<_>>()})
    }).collect::<Vec<_>>();
    let l = &program.document().groups[0].lifecycle;
    let binding = |b: Option<&WorkflowBinding>| {
        b.map(|b| {
            format!(
                "wf:{}",
                program.workflow_index(&b.workflow_id).expect("binding")
            )
        })
    };
    json!({"groups":[{"key":"group:0","id":label(&program.document().groups[0].id),"initialize":binding(l.initialize.as_ref()),"iteration":binding(Some(&l.iteration)),"cleanup":binding(l.cleanup.as_ref())}],"workflows":flows})
}

fn scalar_bytes<K: Ord>(values: &BTreeMap<K, Value>) -> usize {
    values
        .values()
        .map(|v| serde_json::to_vec(v).expect("scalar").len())
        .sum()
}

#[cfg(test)]
#[path = "user.rs"]
mod invocation_tests;
