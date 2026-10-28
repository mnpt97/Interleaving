//! One interpreter for reusable workflows and single-VU lifecycles.
mod interpreter;
mod values;
use crate::{extract::ExtractionState, security::RunSecurity, *};
use interl_core::*;
use interl_http::{HttpSession, ResolvedBody};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

/// Validated, bounded scenario and isolated session ready for one execution.
/// Credentials are resolved once during preparation and never serialized.
pub struct PreparedRun {
    program: Arc<Program>,
    session: HttpSession,
    security: RunSecurity,
    options: RunOptions,
    snapshot: Value,
    graph: Value,
}
/// One invocation owns its local values; callers remain suspended on the stack.
/// Values are released after their compiled last use rather than kept until run end.
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
/// Mutable state belongs to one task. Only the immutable compiled program is shared.
struct Runtime {
    program: Arc<Program>,
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
/// Prefer a scenario URL; otherwise use its selected client profile.
fn scenario_base_url(document: &ScenarioDocument) -> Option<&str> {
    {
        let group = document.groups.first()?;
        document
            .client_profiles
            .iter()
            .find(|profile| profile.id == group.client_profile_id)
            .map(|profile| profile.base_url.as_str())
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
    /// Compiles and prepares a run using process-environment credentials.
    ///
    /// # Errors
    /// Returns diagnostics for invalid scenarios, limits, URLs, or credentials.
    pub fn new(
        document: &ScenarioDocument,
        options: RunOptions,
    ) -> Result<Self, Vec<interl_core::Diagnostic>> {
        Self::new_with_secret_provider(document, options, &EnvironmentSecretProvider)
    }
    /// Prepares a run with a trusted credential provider borrowed only for this call.
    ///
    /// `dyn SecretProvider` allows adapters and tests to supply different providers
    /// without making the runner generic or retaining the provider during execution.
    ///
    /// # Errors
    /// Returns diagnostics before execution if compilation or admission fails.
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
            .ok_or_else(|| invalid("Selected client profile is unavailable"))?;
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
                        .prepare_target(
                            &static_config(config),
                            &doc.environment.variables,
                            None,
                            config.base_url.as_deref().or(scenario_base_url(doc)),
                        )
                        .map_err(|e| invalid(&e.message))?;
                }
            }
        }
        for i in program.reachable() {
            for node in &program.workflows()[*i].definition().nodes {
                if let ProgramNode::Authentication { config, .. } = node {
                    let cfg = interl_core::HttpConfig {
                        method: config.method,
                        path: config.path.clone(),
                        headers: BTreeMap::new(),
                        timeout_ms: None,
                        body: None,
                    };
                    session
                        .prepare_target(
                            &cfg,
                            &doc.environment.variables,
                            None,
                            config.base_url.as_deref().or(scenario_base_url(doc)),
                        )
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
                        .prepare_target(
                            &static_config(config),
                            &doc.environment.variables,
                            None,
                            config.base_url.as_deref().or(scenario_base_url(doc)),
                        )
                        .map_err(|e| invalid(&e.message))?;
                    security.learn_url(request.url());
                }
            }
        }
        let snapshot = security.program_snapshot(&program);
        let graph =
            graph(&program, &security).map_err(|_| invalid("Compiled graph is unavailable"))?;
        if crate::budget::value_bytes(&snapshot)
            + 2 * crate::budget::value_bytes(&graph)
            + 65536
            + 4096
            > options.max_report_bytes
        {
            return Err(invalid(
                "Redacted snapshot and graph exceed the report budget",
            ));
        }
        Ok(Self {
            program: Arc::new(program),
            session,
            security,
            options,
            snapshot,
            graph,
        })
    }
    /// Consumes preparation and spawns execution using a shared cancellation token.
    ///
    /// Drain the returned live receiver concurrently with the task so backpressure
    /// can make progress.
    ///
    /// # Panics
    /// Panics when called outside an active Tokio runtime.
    pub fn start(self, cancel: CancellationToken) -> RunHandle {
        self.start_control(RunControl::from_token(cancel))
    }
    /// Spawns execution with explicit adapter shutdown and user cancellation control.
    /// This prepared run can only be started once.
    ///
    /// # Panics
    /// Panics when called outside an active Tokio runtime.
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
        trace.bytes += crate::budget::value_bytes(&self.graph);
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
/// Project stable positional keys and redacted labels; never include request data.
fn graph(program: &Program, security: &RunSecurity) -> Result<Value, RunFailure> {
    let label = |text: &str| security.text(text).chars().take(256).collect::<String>();
    let mut flows = Vec::new();
    for (index, workflow) in program.workflows().iter().enumerate() {
        let definition = workflow.definition();
        let node_keys: BTreeMap<_, _> = definition
            .nodes
            .iter()
            .enumerate()
            .map(|(node_index, node)| (node.id(), format!("wf:{index}/node:{node_index}")))
            .collect();
        let nodes: Vec<_> = definition
            .nodes
            .iter()
            .enumerate()
            .map(|(node_index, node)| {
                let position = definition
                    .layout
                    .as_ref()
                    .and_then(|layout| layout.positions.get(node.id()))
                    .copied()
                    .unwrap_or(Position {
                        x: 80.0 + node_index as f64 * 260.0,
                        y: 160.0,
                    });
                json!({"key": node_keys[node.id()], "id": label(node.id()),
                "name": label(node.name().unwrap_or(node.id())), "type": node.kind(), "position": position})
            })
            .collect();
        let edges = definition
            .edges
            .iter()
            .enumerate()
            .map(|(edge_index, edge)| {
                let key = |id: &str| {
                    node_keys
                        .get(id)
                        .ok_or_else(|| failure("invalidProgram", "Graph endpoint is unavailable."))
                };
                let attachment = definition.layout.as_ref().and_then(|layout| layout.connections.get(&edge.id));
                Ok(
                    json!({"key": format!("wf:{index}/edge:{edge_index}"), "id": label(&edge.id),
                "fromNodeKey": key(&edge.from)?, "toNodeKey": key(&edge.to)?,
                "sourceHandle": attachment.map(|value| value.source).unwrap_or(interl_core::ConnectionSide::Right),
                "targetHandle": attachment.map(|value| value.target).unwrap_or(interl_core::ConnectionSide::Left)}),
                )
            })
            .collect::<Result<Vec<_>, RunFailure>>()?;
        flows.push(
            json!({"key": format!("wf:{index}"), "id": label(&definition.id),
            "name": label(&definition.name), "nodes": nodes, "edges": edges}),
        );
    }
    let group = &program.document().groups[0];
    let binding = |binding: Option<&WorkflowBinding>| -> Result<Option<String>, RunFailure> {
        binding
            .map(|binding| {
                program
                    .workflow_index(&binding.workflow_id)
                    .map(|index| format!("wf:{index}"))
                    .ok_or_else(|| failure("invalidProgram", "Graph workflow is unavailable."))
            })
            .transpose()
    };
    // A definition can be edited inline only when exactly one lifecycle phase owns
    // it and no workflow calls it. Keys remain independent of sanitized labels.
    let inline_phases: Vec<_> = [
        ("initialize", group.lifecycle.initialize.as_ref()),
        ("iteration", Some(&group.lifecycle.iteration)),
        ("cleanup", group.lifecycle.cleanup.as_ref()),
    ].into_iter().filter_map(|(phase, binding)| {
        let binding = binding?;
        let uses = program.document().groups.iter().flat_map(|group| [
            group.lifecycle.initialize.as_ref(), Some(&group.lifecycle.iteration), group.lifecycle.cleanup.as_ref()
        ]).flatten().filter(|other| other.workflow_id == binding.workflow_id).count();
        let called = program.document().workflows.iter().any(|workflow| workflow.nodes.iter().any(|node| matches!(node, interl_core::ProgramNode::Call { config, .. } if config.workflow_id == binding.workflow_id)));
        (uses == 1 && !called).then_some(phase)
    }).collect();
    Ok(json!({"groups": [{"key": "group:0", "id": label(&group.id),
        "initialize": binding(group.lifecycle.initialize.as_ref())?,
        "iteration": binding(Some(&group.lifecycle.iteration))?,
        "cleanup": binding(group.lifecycle.cleanup.as_ref())?, "inlinePhases": inline_phases}], "workflows": flows}))
}

fn scalar_bytes<K: Ord>(values: &BTreeMap<K, Value>) -> usize {
    values.values().map(crate::budget::value_bytes).sum()
}

#[cfg(test)]
#[path = "user.rs"]
mod invocation_tests;
