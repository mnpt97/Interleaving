//! Stack-based workflow execution with bounded calls and cancellable HTTP requests.
use super::*;
impl Runtime {
    /// Builds positional graph coordinates without including source values or credentials.
    fn context(
        &self,
        frame: Option<&Frame>,
        node: Option<usize>,
        edge: Option<usize>,
    ) -> EventContext {
        EventContext {
            group_key: Some("group:0".into()),
            phase: (!self.phase.is_empty()).then(|| self.phase.clone()),
            iteration: (self.phase == "iteration").then_some(1),
            workflow_key: frame.map(|f| format!("wf:{}", f.flow)),
            invocation_id: frame.map(|f| f.invocation.clone()),
            parent_invocation_id: frame.and_then(|f| f.parent.clone()),
            caller_node_key: frame.and_then(|f| f.caller.clone()),
            node_key: frame.and_then(|f| node.map(|n| format!("wf:{}/node:{n}", f.flow))),
            edge_key: frame.and_then(|f| edge.map(|e| format!("wf:{}/edge:{e}", f.flow))),
        }
    }
    /// Redacts node labels and admits an observation to the shared trace.
    /// Delivery timeout or exhausted report storage becomes a safe runtime failure.
    async fn event(
        &mut self,
        frame: Option<&Frame>,
        node: Option<usize>,
        edge: Option<usize>,
        event: EventKind,
        terminal: bool,
    ) -> Result<(), RunFailure> {
        self.trace.context = self.context(frame, node, edge);
        let label = frame.and_then(|f| {
            node.map(|n| {
                self.security
                    .text(self.program.workflows()[f.flow].definition().nodes[n].id())
                    .chars()
                    .take(256)
                    .collect::<String>()
            })
        });
        if !self.trace.emit(label.as_deref(), event, terminal).await {
            if self.trace.cleanup_expired {
                return Err(failure("cleanupTimeout", "Cleanup deadline exceeded"));
            }
            self.control.stop(StopCause::Safety);
            return Err(report_limit());
        }
        Ok(())
    }
    /// Creates invocation-local storage and enforces call count and stack depth limits.
    fn frame(
        &mut self,
        flow: usize,
        inputs: BTreeMap<String, Value>,
        parent: Option<String>,
        caller: Option<String>,
    ) -> Result<Frame, RunFailure> {
        self.next_invocation += 1;
        if self.next_invocation > 128 || self.frames.len() + 1 + usize::from(parent.is_some()) > 8 {
            return Err(failure(
                "invocationLimit",
                "Workflow invocation limit exceeded.",
            ));
        }
        let values = inputs
            .into_iter()
            .map(|(k, v)| (ValueKey::Input(k), v))
            .filter(|(k, _)| self.program.workflows()[flow].last_uses().contains_key(k))
            .collect();
        Ok(Frame {
            flow,
            pc: 0,
            invocation: format!("inv:{}", self.next_invocation),
            parent,
            caller,
            values,
            extraction: ExtractionState::new(
                self.program.workflows()[flow].auth(),
                self.options.max_response_bytes,
            )?,
            pending: false,
        })
    }
    /// Checks cancellation, adapter shutdown, receiver loss, and the cleanup deadline.
    fn boundary(&self, cancel: &CancellationToken) -> Result<(), RunFailure> {
        check(cancel)?;
        if self.control.shutdown.is_cancelled() || self.trace.sender.is_closed() {
            return Err(failure("cancelled", "Run owner or event receiver stopped"));
        }
        if self
            .cleanup_deadline
            .is_some_and(|d| tokio::time::Instant::now() >= d)
        {
            return Err(failure("cleanupTimeout", "Cleanup deadline exceeded"));
        }
        Ok(())
    }
    /// Finishes a node, releases values at their last use, and emits the next edge.
    async fn advance(
        &mut self,
        frame: &mut Frame,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        let program = Arc::clone(&self.program);
        let w = &program.workflows()[frame.flow];
        let index = w.order()[frame.pc];
        let node = &w.definition().nodes[index];
        let outgoing: Vec<_> = w
            .definition()
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.from == node.id())
            .collect();
        let selected_edge = if let ProgramNode::ExclusiveSplit { config, .. } = node {
            let status = frame
                .values
                .get(&ValueKey::Response(config.source_node_id.clone()))
                .and_then(Value::as_u64)
                .and_then(|v| u16::try_from(v).ok())
                .ok_or_else(|| {
                    failure(
                        "routeSourceUnavailable",
                        "Route source status is unavailable.",
                    )
                })?;
            let body = if outgoing
                .iter()
                .any(|(index, _)| w.predicate(*index).is_some_and(|p| p.requires_body()))
            {
                Some(
                    frame
                        .extraction
                        .response_body(&config.source_node_id, cancel)?,
                )
            } else {
                None
            };
            let inputs = config
                .inputs
                .iter()
                .map(|(name, source)| {
                    Ok((
                        name.clone(),
                        self.scalar(source, Some(frame), None)?.owned(),
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, RunFailure>>()?;
            let (entries, bytes, responses) = frame.extraction.usage();
            self.capacity(
                entries + frame.values.len() + inputs.len(),
                bytes
                    + frame
                        .values
                        .values()
                        .map(crate::budget::value_bytes)
                        .sum::<usize>()
                    + inputs
                        .values()
                        .map(crate::budget::value_bytes)
                        .sum::<usize>(),
                responses,
            )?;
            let mut matches = Vec::new();
            let mut fallback = None;
            for &(edge_index, edge) in &outgoing {
                self.boundary(cancel)?;
                if edge.is_default == Some(true) {
                    fallback = Some(edge_index);
                    continue;
                }
                let predicate = w.predicate(edge_index).ok_or_else(|| {
                    failure("invalidProgram", "Compiled predicate is unavailable.")
                })?;
                let result = predicate.evaluate_context(status, body.as_ref(), &inputs, || {
                    cancel.is_cancelled() || self.control.shutdown.is_cancelled()
                });
                let result = result.map_err(|error| {
                    failure(
                        match error {
                            interl_core::PredicateError::Cancelled => "cancelled",
                            interl_core::PredicateError::Unavailable => "routeSourceUnavailable",
                            interl_core::PredicateError::TypeMismatch => "routeTypeMismatch",
                        },
                        &error.to_string(),
                    )
                })?;
                if result {
                    matches.push(edge_index);
                }
            }
            if matches.len() > 1 {
                return Err(failure(
                    "routeAmbiguous",
                    "More than one route condition matched.",
                ));
            }
            let selected = matches.first().copied().or(fallback).ok_or_else(|| {
                failure(
                    "routeNoMatch",
                    "No route condition matched and no default exists.",
                )
            })?;
            self.event(
                Some(frame),
                Some(index),
                None,
                EventKind::RouteSelected {
                    selected_edge_key: format!("wf:{}/edge:{selected}", frame.flow),
                    unselected_edge_keys: outgoing
                        .iter()
                        .filter(|(i, _)| *i != selected)
                        .map(|(i, _)| format!("wf:{}/edge:{i}", frame.flow))
                        .collect(),
                },
                false,
            )
            .await?;
            Some(selected)
        } else {
            outgoing.first().map(|(i, _)| *i)
        };
        self.event(
            Some(frame),
            Some(index),
            None,
            EventKind::StepFinished {},
            false,
        )
        .await?;
        frame.pending = false;
        frame.extraction.release_after(node.id());
        frame.values.retain(|key, _| {
            self.program.workflows()[frame.flow]
                .last_uses()
                .get(key)
                .is_some_and(|last| last != node.id())
        });
        if let Some(edge) = selected_edge {
            self.boundary(cancel)?;
            self.event(
                Some(frame),
                None,
                Some(edge),
                EventKind::EdgeTraversed {},
                false,
            )
            .await?;
            let target = &w.definition().edges[edge].to;
            frame.pc = w
                .order()
                .iter()
                .position(|i| w.definition().nodes[*i].id() == target)
                .ok_or_else(|| failure("invalidProgram", "Compiled edge target is unavailable."))?;
        } else {
            frame.pc = w.order().len();
        }
        Ok(())
    }
    /// Emits sanitized observations around a cancellable HTTP exchange.
    /// Tokio select drops the HTTP future when a stronger shutdown branch wins.
    async fn send_request(
        &mut self,
        frame: &Frame,
        index: usize,
        request: interl_http::PreparedRequest,
        cancel: &CancellationToken,
    ) -> Result<interl_http::HttpResponse, RunFailure> {
        let key = format!("wf:{}/node:{index}", frame.flow);
        let event = self
            .security
            .request_event(&key, &request, self.options.max_preview_bytes);
        self.event(Some(frame), Some(index), None, event, false)
            .await?;
        self.boundary(cancel)?;
        self.trace.summary.requests += 1;
        let deadline = self.cleanup_deadline;
        let response = tokio::select! {
            biased;
            _ = self.trace.sender.closed() => {
                self.control.stop(StopCause::ReceiverClosed);
                return Err(failure("cancelled", "Event receiver closed"));
            }
            _ = self.control.shutdown.cancelled() => return Err(failure("cancelled", "Run owner stopped")),
            _ = async {
                match deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending::<()>().await,
                }
            } => return Err(failure("cleanupTimeout", "Cleanup deadline exceeded")),
            response = self.session.execute(request, cancel) => response?,
        };
        self.trace.summary.responses += 1;
        if response.status >= 400 {
            self.trace.summary.http_error_responses += 1;
        }
        let event = self
            .security
            .response_event(&key, &response, self.options.max_preview_bytes);
        self.event(Some(frame), Some(index), None, event, false)
            .await?;
        Ok(response)
    }
    /// Replaces the current session with a fresh login and retains only safe observations.
    /// Failed login cannot leave a partially authenticated session for later cleanup.
    async fn authenticate(
        &mut self,
        frame: &Frame,
        index: usize,
        auth: &AuthenticationConfig,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        // Clear before attempting login: failures and cleanup cannot reuse a stale identity.
        self.session = self.session.fresh()?;
        let key = authentication_key(frame.flow, index);
        let username = self.security.credential(&key, "username").ok_or_else(|| {
            failure(
                "authenticationCredentials",
                "Authentication credentials unavailable",
            )
        })?;
        let password = self.security.credential(&key, "password").ok_or_else(|| {
            failure(
                "authenticationCredentials",
                "Authentication credentials unavailable",
            )
        })?;
        let mut fields = serde_json::Map::new();
        fields.insert(auth.username_key.clone(), json!(username));
        fields.insert(auth.password_key.clone(), json!(password));
        let cfg = interl_core::HttpConfig {
            method: auth.method,
            path: auth.path.clone(),
            headers: BTreeMap::new(),
            timeout_ms: None,
            body: None,
        };
        let request = self.session.prepare_target(
            &cfg,
            &self.program.document().environment.variables,
            Some(ResolvedBody::Json(Value::Object(fields))),
            auth.base_url
                .as_deref()
                .or(scenario_base_url(self.program.document())),
        )?;
        let url = request.url().clone();
        let result = async {
            let response = self.send_request(frame, index, request, cancel).await?;
            if !(200..300).contains(&response.status) {
                return Err(failure(
                    "authenticationFailed",
                    "Authentication requires a successful HTTP response",
                ));
            }
            match auth.mode {
                AuthenticationMode::Bearer => {
                    let token = crate::extract::authentication_token(
                        &response,
                        &auth.token_expression,
                        cancel,
                    )?;
                    self.security.remember_secret(&token);
                    self.session.update_headers(
                        &BTreeMap::from([(
                            auth.header_name.clone(),
                            format!("{}{}", auth.header_prefix, token),
                        )]),
                        &[],
                    )?;
                }
                AuthenticationMode::Cookie => {
                    if !response.headers.contains_key("set-cookie")
                        || !self.session.has_response_cookies(&url, &response.headers)
                    {
                        return Err(failure(
                            "authenticationFailed",
                            "Authentication response did not establish a session cookie",
                        ));
                    }
                }
            }
            self.boundary(cancel)?;
            Ok(())
        }
        .await;
        if result.is_err() {
            self.session = self.session.fresh()?;
        }
        result
    }

    /// Prepares and sends a request, retaining response data only for planned extraction.
    async fn http(
        &mut self,
        frame: &mut Frame,
        index: usize,
        node_id: &str,
        http: &ProgramHttp,
        cancel: &CancellationToken,
    ) -> Result<(), RunFailure> {
        let request = self.prepare_http(frame, http, cancel)?;
        let response = self.send_request(frame, index, request, cancel).await?;
        let flow = &self.program.workflows()[frame.flow];
        let status_key = ValueKey::Response(node_id.into());
        if flow.last_uses().contains_key(&status_key) {
            frame.values.insert(status_key, json!(response.status));
        }
        let retained = if flow.auth().response_last_uses().contains_key(node_id) {
            response.body.len()
        } else {
            0
        };
        let (entries, bytes, responses) = frame.extraction.usage();
        self.capacity(
            entries + frame.values.len(),
            bytes + scalar_bytes(&frame.values),
            responses + retained,
        )?;
        frame
            .extraction
            .retain_response(node_id, response, cancel)?;
        Ok(())
    }

    /// Executes one compiled node or returns a child frame for a workflow call.
    async fn step(
        &mut self,
        frame: &mut Frame,
        cancel: &CancellationToken,
    ) -> Result<Option<Frame>, RunFailure> {
        self.visits += 1;
        if self.visits > 1024 {
            return Err(failure(
                "invocationLimit",
                "Executed node visit limit exceeded.",
            ));
        }
        // Arc::clone shares the immutable program. Borrowing through this local handle
        // lets us mutate the runtime across await without copying node bodies.
        let program = Arc::clone(&self.program);
        let flow = &program.workflows()[frame.flow];
        let index = flow.order()[frame.pc];
        let node = &flow.definition().nodes[index];
        self.event(
            Some(frame),
            Some(index),
            None,
            EventKind::StepStarted {
                kind: node.kind().into(),
            },
            false,
        )
        .await?;
        frame.pending = true;
        self.boundary(cancel)?;
        match node {
            ProgramNode::Call { config, .. } => {
                let inputs = self.bindings(config, Some(frame), None)?;
                let flow = self
                    .program
                    .workflow_index(&config.workflow_id)
                    .ok_or_else(|| failure("invalidProgram", "Compiled callee is unavailable."))?;
                let child = self.frame(
                    flow,
                    inputs,
                    Some(frame.invocation.clone()),
                    Some(format!("wf:{}/node:{index}", frame.flow)),
                )?;
                self.event(
                    Some(&child),
                    None,
                    None,
                    EventKind::WorkflowStarted {},
                    false,
                )
                .await?;
                return Ok(Some(child));
            }
            ProgramNode::Authentication { config, .. } => {
                self.authenticate(frame, index, config, cancel).await?;
            }
            ProgramNode::Http { config, .. } => {
                self.http(frame, index, node.id(), config, cancel).await?
            }
            ProgramNode::Extract { config, .. } => {
                let (entries, bytes) = self.extraction_budget(frame)?;
                frame
                    .extraction
                    .extract_bounded(node.id(), cancel, entries, bytes)?;
                for (name, _, _) in config.entries() {
                    let value = frame.extraction.output(node.id(), name)?;
                    self.security.remember_secret(value.expose());
                }
            }
            ProgramNode::SetSession { config, .. } => {
                self.session_update(config, frame, cancel)?;
                let event = EventKind::SessionUpdated {
                    set_headers: config
                        .set_headers
                        .keys()
                        .map(|name| self.security.text(&name.to_ascii_lowercase()))
                        .collect(),
                    removed_headers: config
                        .remove_headers
                        .iter()
                        .map(|name| self.security.text(&name.to_ascii_lowercase()))
                        .collect(),
                };
                self.event(Some(frame), Some(index), None, event, false)
                    .await?;
            }
            ProgramNode::Start { .. }
            | ProgramNode::End { .. }
            | ProgramNode::ExclusiveSplit { .. }
            | ProgramNode::ExclusiveMerge { .. } => {}
        }
        self.boundary(cancel)?;
        self.advance(frame, cancel).await?;
        Ok(None)
    }

    /// Runs the explicit call stack and moves completed outputs into the suspended caller.
    /// Popping frames avoids recursive async futures and keeps invocation storage bounded.
    async fn loop_frames(
        &mut self,
        cancel: &CancellationToken,
    ) -> Result<BTreeMap<String, Value>, RunFailure> {
        while let Some(mut frame) = self.frames.pop() {
            if let Err(error) = self.boundary(cancel) {
                self.frames.push(frame);
                return Err(error);
            }
            if frame.pc < self.program.workflows()[frame.flow].order().len() {
                match self.step(&mut frame, cancel).await {
                    Ok(child) => {
                        self.frames.push(frame);
                        if let Some(child) = child {
                            self.frames.push(child);
                        }
                    }
                    Err(error) => {
                        self.frames.push(frame);
                        return Err(error);
                    }
                }
                continue;
            }
            let result = self.outputs(&frame);
            let outputs = match result {
                Ok(outputs) => outputs,
                Err(error) => {
                    self.frames.push(frame);
                    return Err(error);
                }
            };
            if let Err(error) = self
                .event(
                    Some(&frame),
                    None,
                    None,
                    EventKind::WorkflowFinished {
                        status: RunStatus::Passed,
                    },
                    false,
                )
                .await
            {
                self.frames.push(frame);
                return Err(error);
            }
            drop(frame);
            let Some(mut parent) = self.frames.pop() else {
                return Ok(outputs);
            };
            let flow = &self.program.workflows()[parent.flow];
            let node_id = flow.definition().nodes[flow.order()[parent.pc]]
                .id()
                .to_owned();
            for (name, value) in outputs {
                let key = ValueKey::Call(node_id.clone(), name);
                if flow.last_uses().contains_key(&key) {
                    parent.values.insert(key, value);
                }
            }
            if let Err(error) = self.advance(&mut parent, cancel).await {
                self.frames.push(parent);
                return Err(error);
            }
            self.frames.push(parent);
        }
        Ok(BTreeMap::new())
    }

    /// Runs one lifecycle binding and records its terminal outcome, including failures.
    async fn root(
        &mut self,
        binding: &WorkflowBinding,
        init: Option<BTreeMap<String, Value>>,
        cancel: &CancellationToken,
    ) -> Result<BTreeMap<String, Value>, RunFailure> {
        self.event(
            None,
            None,
            None,
            EventKind::LifecycleStarted {
                phase: self.phase.clone(),
            },
            false,
        )
        .await?;
        let result = async {
            check(cancel)?;
            let inputs = self.bindings(binding, None, init.as_ref())?;
            drop(init);
            let frame = self.frame(
                self.program
                    .workflow_index(&binding.workflow_id)
                    .ok_or_else(|| failure("invalidProgram", "Root workflow is unavailable."))?,
                inputs,
                None,
                None,
            )?;
            self.event(
                Some(&frame),
                None,
                None,
                EventKind::WorkflowStarted {},
                false,
            )
            .await?;
            self.frames.push(frame);
            self.loop_frames(cancel).await
        }
        .await;
        if let Err(error) = &result {
            let status = if error.kind == "cancelled" {
                RunStatus::Cancelled
            } else {
                RunStatus::Error
            };
            while let Some(frame) = self.frames.pop() {
                let flow = &self.program.workflows()[frame.flow];
                let index = flow.order().get(frame.pc).copied();
                if frame.pending {
                    let _ = self
                        .event(
                            Some(&frame),
                            index,
                            None,
                            EventKind::StepFailed {
                                error: error.clone(),
                            },
                            true,
                        )
                        .await;
                }
                let _ = self
                    .event(
                        Some(&frame),
                        None,
                        None,
                        EventKind::WorkflowFinished { status },
                        true,
                    )
                    .await;
            }
        }
        let status = match &result {
            Ok(_) => RunStatus::Passed,
            Err(e) if e.kind == "cancelled" => RunStatus::Cancelled,
            _ => RunStatus::Error,
        };
        let _ = self
            .event(
                None,
                None,
                None,
                EventKind::LifecycleFinished {
                    phase: self.phase.clone(),
                    status,
                },
                true,
            )
            .await;
        result
    }
    /// Executes initialize, iteration, and eligible bounded cleanup, then returns the report.
    pub(super) async fn run(mut self, graph: Value) -> RunReport {
        self.trace.context = EventContext::default();
        let scenario_id = self.security.text(&self.program.document().id);
        let mut primary = if self
            .trace
            .emit(
                None,
                EventKind::RunStarted {
                    scenario_id,
                    graph: graph.clone(),
                },
                false,
            )
            .await
        {
            None
        } else {
            Some(report_limit())
        };
        if primary.is_none() {
            primary = self
                .event(None, None, None, EventKind::UserSpawned {}, false)
                .await
                .err();
        }
        let program = Arc::clone(&self.program);
        let lifecycle = &program.document().groups[0].lifecycle;
        let mut init = BTreeMap::new();
        if primary.is_none() {
            if let Some(binding) = &lifecycle.initialize {
                self.phase = "initialize".into();
                match self.root(binding, None, &self.control.token.clone()).await {
                    Ok(values) => init = values,
                    Err(e) => primary = Some(e),
                }
            }
        }
        if primary.is_none() {
            self.phase = "iteration".into();
            primary = self
                .root(
                    &lifecycle.iteration,
                    Some(init),
                    &self.control.token.clone(),
                )
                .await
                .err();
        }
        let mut cleanup = CleanupOutcome::default();
        if let Some(binding) = &lifecycle.cleanup {
            self.phase = "cleanup".into();
            let skip = if self.trace.truncated
                || primary.as_ref().is_some_and(|e| {
                    matches!(
                        e.kind.as_str(),
                        "resourceLimit"
                            | "valueLimit"
                            | "invocationLimit"
                            | "templateLimit"
                            | "extractionOutputLimit"
                            | "extractionResponseLimit"
                            | "extractionValueLimit"
                            | "extractionJsonDepthLimit"
                            | "extractionJsonValueLimit"
                    )
                }) {
                Some("safetyLimit")
            } else {
                match self.control.cause() {
                    Some(StopCause::OwnerClosed) => Some("ownerClosed"),
                    Some(StopCause::ReceiverClosed) => Some("receiverClosed"),
                    Some(StopCause::Safety) => Some("safetyLimit"),
                    _ => None,
                }
            };
            if let Some(reason) = skip {
                cleanup.status = "skipped".into();
                cleanup.reason = Some(reason.into());
                let _ = self
                    .event(
                        None,
                        None,
                        None,
                        EventKind::CleanupSkipped {
                            reason: reason.into(),
                        },
                        true,
                    )
                    .await;
            } else {
                self.cleanup_deadline = Some(
                    tokio::time::Instant::now()
                        + std::time::Duration::from_millis(lifecycle.cleanup_timeout_ms),
                );
                self.trace.cleanup_deadline = self.cleanup_deadline;
                let token = CancellationToken::new();
                match self.root(binding, None, &token).await {
                    Ok(_) => cleanup.status = "completed".into(),
                    Err(e) => {
                        cleanup.status = if e.kind == "cancelled" {
                            "cancelled"
                        } else {
                            "error"
                        }
                        .into();
                        cleanup.error = Some(e.clone());
                        if primary.is_none() {
                            primary = Some(e);
                        }
                    }
                }
            }
        }
        self.trace.cleanup_deadline = None;
        if primary.is_none() && self.control.is_cancelled() {
            primary = Some(failure("cancelled", "Run cancelled"));
        }
        let status = match &primary {
            None => RunStatus::Passed,
            Some(e) if e.kind == "cancelled" => RunStatus::Cancelled,
            _ => RunStatus::Error,
        };
        self.phase.clear();
        let _ = self
            .event(None, None, None, EventKind::UserFinished { status }, true)
            .await;
        self.trace.context = EventContext::default();
        if status == RunStatus::Cancelled {
            let _ = self
                .trace
                .emit(None, EventKind::RunCancelled {}, true)
                .await;
        }
        let _ = self
            .trace
            .emit(None, EventKind::RunFinished { status }, true)
            .await;
        self.trace.summary.elapsed_ns = self.trace.elapsed();
        RunReport {
            report_version: 3,
            run_id: self.trace.run_id,
            status,
            snapshot: self.trace.snapshot,
            graph,
            cleanup,
            summary: self.trace.summary,
            events: self.trace.events,
            error: primary,
            events_truncated: self.trace.truncated,
            delivery_dropped_events: self.trace.dropped,
        }
    }
}
