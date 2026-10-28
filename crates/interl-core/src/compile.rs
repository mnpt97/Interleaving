//! Semantic compilation pipeline: admit bounded data, validate each workflow,
//! check lifecycle/call references, and assemble an immutable execution plan.
//!
//! Private compiler passes share checked data through borrowed references. Their
//! ordering matters: graph invariants must hold before traversal or liveness analysis.

mod calls;
mod references;
mod secrets;
use crate::scalar::ScalarRef;
use crate::{
    scalar_type, AuthenticationMode, CompiledWorkflow, Diagnostic, ErrorCategory, PortType,
    Program, ProgramBody, ProgramNode, ProgramTemplate, ScenarioDocument, ValueKey,
};
use references::{visit_node_sources, ValidationContext};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Borrow a checked definition while collecting owned execution metadata.
/// Keeping this intermediate plan borrowed defers the document snapshot until
/// every compiler pass has succeeded; rejected inputs are never cloned here.
struct WorkflowAnalysis<'a> {
    definition: &'a crate::WorkflowDefinition,
    order: Vec<usize>,
    auth: crate::AuthPlan,
    predicates: BTreeMap<usize, crate::CompiledPredicate>,
    last_uses: BTreeMap<ValueKey, String>,
}

/// Construct a safe semantic diagnostic without embedding user payload values.
fn problem(code: &str, path: &str, message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCategory::Validation, code, path, message)
}
/// Check bounded HTTP(S) targets without performing DNS or network access.
fn valid_base_url(value: &str) -> bool {
    value.len() <= 8192
        && url::Url::parse(value).is_ok_and(|u| {
            matches!(u.scheme(), "http" | "https")
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
                && u.query().is_none()
                && u.fragment().is_none()
        })
}
/// Reject oversized template trees before later passes can clone or recursively inspect them.
/// A work stack bounds both visited nodes and pending children, protecting direct Rust callers
/// as well as parsed files. Depth starts at two because the body object is the first node.
fn check_template_limits(document: &ScenarioDocument) -> Result<(), Vec<Diagnostic>> {
    for workflow in &document.workflows {
        for node in &workflow.nodes {
            if let ProgramNode::Http { config, .. } = node {
                if let Some(ProgramBody::JsonTemplate { fields }) = &config.body {
                    let mut stack = fields
                        .values()
                        .map(|template| (template, 2))
                        .collect::<Vec<_>>();
                    let mut count = 1;
                    while let Some((template, depth)) = stack.pop() {
                        count += 1;
                        if count > 256 || depth > 32 || stack.len() > 256 {
                            return Err(vec![problem(
                                "templateLimit",
                                "/workflows",
                                "Template composition exceeds depth or node limits.",
                            )]);
                        }
                        match template {
                            ProgramTemplate::Object { fields } => {
                                stack.extend(fields.values().map(|template| (template, depth + 1)))
                            }
                            ProgramTemplate::Array { items } => {
                                stack.extend(items.iter().map(|template| (template, depth + 1)))
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Validate a typed scenario and construct its immutable execution plan.
///
/// The passes check template limits first (before recursive processing/cloning),
/// then shapes, workflow connectivity, scoped scalar references, lifecycle mappings,
/// call-graph limits, and reachable secret dependencies. All definitions are checked,
/// including unused ones; only reachable credential references are prepared for the engine.
/// No credential value is resolved and no HTTP request is made here.
///
/// # Errors
/// Returns at most 100 safe diagnostics. Some structural/resource failures stop early
/// because subsequent passes depend on the missing invariant. The borrowed input is
/// never modified; a successful plan captures an owned clone for run isolation.
pub fn compile_document(document: &ScenarioDocument) -> Result<Program, Vec<Diagnostic>> {
    check_template_limits(document)?;

    let mut context = ValidationContext {
        doc: document,
        dominators: BTreeMap::new(),
        errors: Vec::new(),
        identity_fields: BTreeSet::new(),
    };
    if document.schema_version != 2 {
        context.add(
            "unsupportedVersion",
            "/schemaVersion",
            "Expected schemaVersion 2.",
        );
    }
    if document.groups.len() != 1 {
        context.add(
            "runConfiguration",
            "/groups",
            "Execution requires exactly one once workload group.",
        );
        return Err(context.errors);
    }
    if document.workflows.is_empty()
        || document.workflows.len() > 32
        || document
            .workflows
            .iter()
            .map(|workflow| workflow.nodes.len())
            .sum::<usize>()
            > 256
        || document
            .workflows
            .iter()
            .map(|workflow| workflow.edges.len())
            .sum::<usize>()
            > 1024
    {
        context.add(
            "resourceLimit",
            "/workflows",
            "Workflow, node, or edge limits exceeded.",
        );
        return Err(context.errors);
    }
    context.validate_id(&document.id, "/id");
    context.validate_id(&document.groups[0].id, "/groups/0/id");
    {
        for p in &document.client_profiles {
            context.validate_id(&p.id, "/clientProfiles");
        }
    }
    if document.name.len() > 256 {
        context.add(
            "resourceLimit",
            "/name",
            "Display names allow at most 256 UTF-8 bytes.",
        );
    }
    let mut ids = BTreeSet::new();
    let mut flows = Vec::new();
    let mut predicate_bytes = 0usize;
    for (workflow_index, workflow) in document.workflows.iter().enumerate() {
        let path = format!("/workflows/{workflow_index}");
        let start_errors = context.errors.len();
        context.validate_id(&workflow.id, &path);
        if !ids.insert(workflow.id.as_str()) {
            context.add("duplicateWorkflowId", &path, "Workflow IDs must be unique.");
        }
        if workflow.name.len() > 256 || workflow.inputs.len() > 32 || workflow.outputs.len() > 32 {
            context.add(
                "resourceLimit",
                &path,
                "Workflow name or port count exceeds limits.",
            );
        }
        for (name, port) in &workflow.inputs {
            context.validate_identifier(name, &path);
            if port
                .default
                .as_ref()
                .is_some_and(|v| scalar_type(v) != Some(port.kind))
            {
                context.add(
                    "portTypeMismatch",
                    &path,
                    "Input default must match its scalar type and bounds.",
                );
            }
        }
        for name in workflow.outputs.keys() {
            context.validate_identifier(name, &path);
        }
        let mut edge_ids = BTreeSet::new();
        for e in &workflow.edges {
            context.validate_id(&e.id, &path);
            if !edge_ids.insert(&e.id) {
                context.add(
                    "duplicateEdgeId",
                    &path,
                    "Edge IDs must be unique within a workflow.",
                );
            }
        }
        for (index, node) in workflow.nodes.iter().enumerate() {
            context.validate_id(node.id(), &path);
            let base = match node {
                ProgramNode::Http { config, .. } => config.base_url.as_ref(),
                ProgramNode::Authentication { config, .. } => config.base_url.as_ref(),
                _ => None,
            };
            if base.is_some_and(|v| !valid_base_url(v)) {
                context.add(
                    "invalidBaseUrl",
                    &format!("{path}/nodes/{index}/config/baseUrl"),
                    "Use an absolute HTTP(S) base URL without credentials, query or fragment.",
                );
            }
            if let ProgramNode::Authentication { config, .. } = node {
                let p = format!("{path}/nodes/{index}/config");
                if !matches!(
                    config.method,
                    crate::HttpMethod::Post | crate::HttpMethod::Put | crate::HttpMethod::Patch
                ) || !config.path.starts_with('/')
                    || config.path.starts_with("//")
                    || config.path.contains(['\r', '\n', '#'])
                {
                    context.add(
                        "invalidAuthenticationRequest",
                        &p,
                        "Use POST, PUT or PATCH and a path starting with one slash.",
                    );
                }
                for reference in [&config.username, &config.password] {
                    let crate::SecretReference::Env { name } = reference;
                    context.validate_identifier(name, &p);
                }
                context.validate_identifier(&config.username_key, &p);
                context.validate_identifier(&config.password_key, &p);
                if config.username_key == config.password_key {
                    context.add(
                        "invalidCredentialFields",
                        &p,
                        "Username and password JSON fields must differ.",
                    );
                }
                if config.mode == AuthenticationMode::Bearer
                    && (config.header_prefix.len() > 8192
                        || config.header_prefix.contains(['\r', '\n'])
                        || config.header_name.is_empty()
                        || config.header_name.len() > 128
                        || !config
                            .header_name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)))
                {
                    context.add(
                        "invalidHeader",
                        &p,
                        "Use a valid session header and a prefix without line breaks.",
                    );
                }
                if config.mode == AuthenticationMode::Bearer {
                    if let Err(e) = crate::expressions::compile_expression(&config.token_expression)
                    {
                        context.add(e.code, &p, e.message);
                    }
                }
            }
            if let Some(name) = node.name() {
                if name.len() > 256 {
                    context.add(
                        "resourceLimit",
                        &format!("{path}/nodes/{index}/name"),
                        "Display names allow at most 256 UTF-8 bytes.",
                    );
                }
            }
        }
        let mut auth = match crate::graph_validation::validate_workflow(document, workflow) {
            Ok(auth) => auth,
            Err(errors) => {
                for mut e in errors {
                    if context.errors.len() >= 100 {
                        break;
                    }
                    e.workflow_id = Some(workflow.id.clone());
                    if let Some(tail) = e.field_path.strip_prefix("/workflow") {
                        e.field_path = format!("{path}{tail}");
                    } else if e.field_path.starts_with("/layout") {
                        e.field_path = format!("{path}{}", e.field_path);
                    }
                    context.errors.push(e);
                }
                continue;
            }
        };
        context.dominators = crate::graph_validation::dominators(workflow);
        let mut predicates = BTreeMap::new();
        for (edge_index, edge) in workflow.edges.iter().enumerate() {
            if let Some(condition) = &edge.condition {
                match crate::compile_predicate(&condition.expression) {
                    Ok(predicate) => {
                        if let Some(ProgramNode::ExclusiveSplit { config, .. }) =
                            workflow.nodes.iter().find(|node| node.id() == edge.from)
                        {
                            for name in predicate.input_names() {
                                if !config.inputs.contains_key(name) {
                                    context.add(
                                        "missingRouteInput",
                                        &format!("{path}/edges/{edge_index}/condition/expression"),
                                        "Bind every referenced route input on the split.",
                                    );
                                }
                            }
                            if predicate.requires_body() {
                                auth.response_last_uses
                                    .insert(config.source_node_id.clone(), edge.from.clone());
                                auth.sensitive_http_nodes
                                    .insert(config.source_node_id.clone());
                            }
                        }

                        predicate_bytes = predicate_bytes
                            .saturating_add(condition.expression.len() + predicate.storage_bytes());
                        if predicate_bytes > 256 * 1024 {
                            context.add(
                                "expressionLimit",
                                &format!("{path}/edges/{edge_index}/condition/expression"),
                                "Compiled route predicates exceed the document budget.",
                            );
                        } else {
                            predicates.insert(edge_index, predicate);
                        }
                    }
                    Err(error) => {
                        let field = format!("{path}/edges/{edge_index}/condition/expression");
                        let mut diagnostic = problem(error.code, &field, error.message);
                        // Limits report byte offsets, which may fall inside a Unicode
                        // string literal. Walk characters instead of slicing at that byte.
                        let (mut line, mut column) = (1, 1);
                        for (_, character) in condition
                            .expression
                            .char_indices()
                            .take_while(|(index, _)| *index < error.offset)
                        {
                            if character == '\n' {
                                line += 1;
                                column = 1;
                            } else {
                                column += 1;
                            }
                        }
                        diagnostic.expression_line = Some(line);
                        diagnostic.expression_column = Some(column);
                        if context.errors.len() < 100 {
                            context.errors.push(diagnostic);
                        }
                    }
                }
            }
        }
        let order = crate::graph_validation::ordered_indices(workflow);
        let pos = order
            .iter()
            .enumerate()
            .map(|(i, n)| (workflow.nodes[*n].id(), i))
            .collect::<BTreeMap<_, _>>();
        let mut last_uses = BTreeMap::new();
        for (at, index) in order.iter().enumerate() {
            let node = &workflow.nodes[*index];
            if let ProgramNode::ExclusiveSplit { config, .. } = node {
                if config.inputs.len() > 32 {
                    context.add(
                        "resourceLimit",
                        &path,
                        "Gateways allow at most 32 scalar input bindings.",
                    );
                }
                for name in config.inputs.keys() {
                    context.validate_identifier(name, &path);
                }
            }
            if let ProgramNode::ExclusiveSplit { config, .. } = node {
                let types = config
                    .inputs
                    .iter()
                    .filter_map(|(name, source)| {
                        context
                            .source_type(source.into(), Some(workflow), &pos, at, &path, None)
                            .map(|kind| (name.clone(), kind))
                    })
                    .collect();
                for (edge_index, _) in workflow
                    .edges
                    .iter()
                    .enumerate()
                    .filter(|(_, edge)| edge.from == node.id())
                {
                    if let Some(predicate) = predicates.get(&edge_index) {
                        if let Err(error) = predicate.validate_input_types(&types) {
                            context.add(
                                error.code,
                                &format!("{path}/edges/{edge_index}/condition/expression"),
                                error.message,
                            );
                        }
                    }
                }
            }
            if let ProgramNode::Extract { config, .. } = node {
                if !workflow.nodes.iter().any(|n| {
                    n.id() == config.source_node_id && matches!(n, ProgramNode::Http { .. })
                }) {
                    context.add(
                        "invalidSourceType",
                        &path,
                        "Extraction source must be a local HTTP node.",
                    );
                }
            }
            if let ProgramNode::Call { config, .. } = node {
                context.validate_binding(config, Some(workflow), &pos, at, &path, None);
            }
            visit_node_sources(node, |src| {
                // Literal templates permit structured JSON; scalar references do not.
                let kind = context.source_type(src, Some(workflow), &pos, at, &path, None);
                if matches!(node, ProgramNode::SetSession { .. })
                    && kind.is_some_and(|k| k != PortType::String)
                {
                    context.add(
                        "portTypeMismatch",
                        &path,
                        "Session headers require string sources.",
                    );
                }
                if matches!(src, ScalarRef::Lifecycle { .. }) {
                    context.add(
                        "invalidLifecycleReference",
                        &path,
                        "Lifecycle references occur only in lifecycle bindings.",
                    );
                }
                if let Some(key) = src.key() {
                    last_uses.insert(key, node.id().to_owned());
                }
            });
        }
        let mut return_marker = "$return".to_owned();
        while workflow.nodes.iter().any(|n| n.id() == return_marker) {
            return_marker.push('_');
        }
        for out in workflow.outputs.values() {
            let kind = context.source_type(
                (&out.value).into(),
                Some(workflow),
                &pos,
                order
                    .iter()
                    .position(|i| matches!(workflow.nodes[*i], ProgramNode::End { .. }))
                    .unwrap_or(order.len()),
                &path,
                None,
            );
            if kind.is_some_and(|t| t != out.kind) {
                context.add(
                    "portTypeMismatch",
                    &path,
                    "Output source type differs from declaration.",
                );
            }
            if let Some(key) = out.value.key() {
                last_uses.insert(key, return_marker.clone());
            }
        }
        if workflow
            .nodes
            .iter()
            .any(|n| matches!(n, ProgramNode::ExclusiveSplit { .. }))
        {
            // Keep branch-shared values through return: a last consumer on an
            // unselected arm cannot safely determine release on another arm.
            for last in last_uses.values_mut() {
                *last = return_marker.clone();
            }
            for last in auth.response_last_uses.values_mut() {
                *last = return_marker.clone();
            }
            for extraction in auth.extractions.values_mut() {
                extraction.last_use_node_id = return_marker.clone();
            }
        }
        for (key, last) in &last_uses {
            if let ValueKey::Extraction(node, _) = key {
                if let Some(e) = auth.extractions.get_mut(node) {
                    // All outputs share one atomic extraction/storage lifetime.
                    // Keep the set until its latest consumer, including workflow return.
                    let current = pos
                        .get(e.last_use_node_id.as_str())
                        .copied()
                        .unwrap_or(usize::MAX);
                    let next = pos.get(last.as_str()).copied().unwrap_or(usize::MAX);
                    if next > current {
                        e.last_use_node_id = last.clone();
                    }
                }
            }
        }
        if context.errors.len() > start_errors {
            for e in &mut context.errors[start_errors..] {
                e.workflow_id = Some(workflow.id.clone());
            }
        }
        flows.push(WorkflowAnalysis {
            definition: workflow,
            order,
            auth,
            predicates,
            last_uses,
        });
    }
    let lifecycle = &document.groups[0].lifecycle;
    if !(1..=5000).contains(&lifecycle.cleanup_timeout_ms) {
        context.add(
            "invalidTimeout",
            "/groups/0/lifecycle/cleanupTimeoutMs",
            "Cleanup timeout must be between 1 and 5000 ms.",
        );
    }
    let init = lifecycle.initialize.as_ref().and_then(|b| {
        document
            .workflows
            .iter()
            .find(|workflow| workflow.id == b.workflow_id)
    });
    for (phase, b) in [
        ("initialize", lifecycle.initialize.as_ref()),
        ("iteration", Some(&lifecycle.iteration)),
        ("cleanup", lifecycle.cleanup.as_ref()),
    ] {
        if let Some(b) = b {
            context.validate_binding(
                b,
                None,
                &BTreeMap::new(),
                0,
                &format!("/groups/0/lifecycle/{phase}"),
                if phase == "iteration" { init } else { None },
            );
        }
    }
    if !context.errors.is_empty() {
        return Err(context.errors);
    }
    let reachable = calls::reachable_workflows(lifecycle, &flows)?;
    if !reachable.iter().any(|i| {
        flows[*i].definition.nodes.iter().any(|n| {
            matches!(
                n,
                ProgramNode::Http { .. } | ProgramNode::Authentication { .. }
            )
        })
    }) {
        return Err(vec![problem(
            "missingHttp",
            "/groups",
            "A runnable lifecycle requires at least one reachable HTTP node.",
        )]);
    }
    let auth = secrets::authentication_plan(document, &flows, &reachable, context.identity_fields)?;
    let document = Arc::new(document.clone());
    let flows = flows
        .into_iter()
        .enumerate()
        .map(|(definition_index, analysis)| CompiledWorkflow {
            document: Arc::clone(&document),
            definition_index,
            order: analysis.order,
            predicates: analysis.predicates,
            auth: analysis.auth,
            last_uses: analysis.last_uses,
        })
        .collect();
    Ok(Program {
        document,
        flows,
        reachable: reachable.into_iter().collect(),
        auth,
    })
}
