use super::*;
use crate::{AuthPlan, Diagnostic, ErrorCategory, ScenarioDocument};
use std::collections::{BTreeMap, BTreeSet};

fn problem(code: &str, path: &str, message: &str) -> Diagnostic {
    Diagnostic::new(ErrorCategory::Validation, code, path, message)
}
struct Check<'a> {
    doc: &'a ScenarioDocument,
    errors: Vec<Diagnostic>,
    identity_fields: BTreeSet<String>,
}
impl Check<'_> {
    fn add(&mut self, code: &str, path: &str, msg: &str) {
        if self.errors.len() < 100 {
            self.errors.push(problem(code, path, msg));
        }
    }
    fn name(&mut self, id: &str, path: &str) {
        if id.trim().is_empty() || id.len() > 128 {
            self.add(
                "invalidId",
                path,
                "IDs must be nonempty and at most 128 UTF-8 bytes.",
            );
        }
    }
    fn port_name(&mut self, id: &str, path: &str) {
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .enumerate()
                .all(|(i, b)| b.is_ascii_alphabetic() || b == b'_' || (i > 0 && b.is_ascii_digit()))
        {
            self.add(
                "invalidPortName",
                path,
                "Use an ASCII identifier of at most 128 bytes.",
            );
        }
    }
    fn source(
        &mut self,
        src: &ScalarSource,
        w: Option<&WorkflowDefinition>,
        pos: &BTreeMap<String, usize>,
        at: usize,
        path: &str,
        init: Option<&WorkflowDefinition>,
    ) -> Option<PortType> {
        let kind = match src {
            ScalarSource::Literal { value } => scalar_type(value),
            ScalarSource::Environment { name } => self
                .doc
                .environment
                .variables
                .get(name)
                .and_then(scalar_type),
            ScalarSource::Identity { field } => {
                self.name(field, path);
                self.identity_fields.insert(field.clone());
                Some(PortType::String)
            }
            ScalarSource::Input { name } => w.and_then(|w| w.inputs.get(name)).map(|p| p.kind),
            ScalarSource::Lifecycle { name, .. } => {
                init.and_then(|w| w.outputs.get(name)).map(|p| p.kind)
            }
            ScalarSource::Extraction { node_id, name }
            | ScalarSource::CallOutput { node_id, name } => {
                if pos.get(node_id).is_none_or(|p| *p >= at) {
                    self.add(
                        "valueNotAvailable",
                        path,
                        "Source must precede its consumer in this invocation.",
                    );
                    return None;
                }
                w.and_then(|w| w.nodes.iter().find(|n| n.id() == node_id))
                    .and_then(|node| match (src, node) {
                        (ScalarSource::Extraction { .. }, ProgramNode::Extract { config, .. })
                            if config.output == *name =>
                        {
                            Some(PortType::String)
                        }
                        (ScalarSource::CallOutput { .. }, ProgramNode::Call { config, .. }) => self
                            .doc
                            .workflows
                            .iter()
                            .find(|w| w.id == config.workflow_id)
                            .and_then(|w| w.outputs.get(name))
                            .map(|p| p.kind),
                        _ => None,
                    })
            }
            ScalarSource::Response { node_id, .. } => {
                if pos.get(node_id).is_none_or(|p| *p >= at) {
                    self.add(
                        "valueNotAvailable",
                        path,
                        "Response source must be an earlier HTTP node.",
                    );
                    return None;
                }
                w.and_then(|w| w.nodes.iter().find(|n| n.id() == node_id))
                    .filter(|n| matches!(n, ProgramNode::Http { .. }))
                    .map(|_| PortType::Integer)
            }
        };
        if kind.is_none() {
            self.add(
                "valueNotAvailable",
                path,
                "Scalar source is missing, oversized, mistyped, or outside its allowed scope.",
            );
        }
        kind
    }
    fn mapping(
        &mut self,
        b: &WorkflowBinding,
        w: Option<&WorkflowDefinition>,
        pos: &BTreeMap<String, usize>,
        at: usize,
        path: &str,
        init: Option<&WorkflowDefinition>,
    ) {
        let Some(callee) = self.doc.workflows.iter().find(|w| w.id == b.workflow_id) else {
            self.add(
                "missingWorkflow",
                path,
                "Referenced workflow does not exist.",
            );
            return;
        };
        for (name, port) in &callee.inputs {
            match b.inputs.get(name) {
                Some(src) => {
                    let kind = self.source(src, w, pos, at, path, init);
                    if kind.is_some_and(|k| k != port.kind) {
                        self.add(
                            "portTypeMismatch",
                            path,
                            "Input type does not match its declaration.",
                        );
                    }
                }
                None if port.default.is_none() => {
                    self.add("missingInput", path, "Required workflow input is missing.")
                }
                _ => {}
            }
        }
        for name in b.inputs.keys() {
            if !callee.inputs.contains_key(name) {
                self.add("unknownInput", path, "Mapping names an undeclared input.");
            }
        }
        if w.is_none() {
            for src in b.inputs.values() {
                if !matches!(
                    src,
                    ScalarSource::Literal { .. }
                        | ScalarSource::Environment { .. }
                        | ScalarSource::Identity { .. }
                        | ScalarSource::Lifecycle { .. }
                ) {
                    self.add(
                        "invalidLifecycleReference",
                        path,
                        "Lifecycle inputs cannot access invocation-local values.",
                    );
                }
            }
        }
    }
}
fn uses(node: &ProgramNode) -> Vec<ScalarSource> {
    let mut refs = Vec::new();
    match node {
        ProgramNode::Call { config, .. } => refs.extend(config.inputs.values().cloned()),
        ProgramNode::SetSession { config, .. } => {
            refs.extend(config.set_headers.values().map(|h| h.parts().0))
        }
        ProgramNode::Http { config, .. } => {
            if let Some(ProgramBody::JsonTemplate { fields }) = &config.body {
                let mut stack = fields.values().collect::<Vec<_>>();
                while let Some(v) = stack.pop() {
                    match v {
                        ProgramTemplate::Object { fields } => stack.extend(fields.values()),
                        ProgramTemplate::Array { items } => stack.extend(items),
                        ProgramTemplate::Literal { .. } => {}
                        _ => {
                            if let Some(v) = v.scalar() {
                                refs.push(v);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    refs
}
pub fn compile_document(file: &ScenarioDocument) -> Result<Program, Vec<Diagnostic>> {
    {
        let v = file;
        for w in &v.workflows {
            for n in &w.nodes {
                if let ProgramNode::Http { config, .. } = n {
                    if let Some(ProgramBody::JsonTemplate { fields }) = &config.body {
                        let mut stack = fields.values().map(|v| (v, 2)).collect::<Vec<_>>();
                        let mut count = 1;
                        while let Some((v, d)) = stack.pop() {
                            count += 1;
                            if count > 256 || d > 32 || stack.len() > 256 {
                                return Err(vec![problem(
                                    "templateLimit",
                                    "/workflows",
                                    "Template composition exceeds depth or node limits.",
                                )]);
                            }
                            match v {
                                ProgramTemplate::Object { fields } => {
                                    stack.extend(fields.values().map(|v| (v, d + 1)))
                                }
                                ProgramTemplate::Array { items } => {
                                    stack.extend(items.iter().map(|v| (v, d + 1)))
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    let doc = file;
    let mut c = Check {
        doc,
        errors: Vec::new(),
        identity_fields: BTreeSet::new(),
    };
    if doc.schema_version != 2 {
        c.add(
            "unsupportedVersion",
            "/schemaVersion",
            "Expected schemaVersion 2.",
        );
    }
    if doc.groups.len() != 1 {
        c.add(
            "runConfiguration",
            "/groups",
            "Execution requires exactly one once workload group.",
        );
        return Err(c.errors);
    }
    if doc.workflows.is_empty()
        || doc.workflows.len() > 32
        || doc.workflows.iter().map(|w| w.nodes.len()).sum::<usize>() > 256
        || doc.workflows.iter().map(|w| w.edges.len()).sum::<usize>() > 1024
    {
        c.add(
            "resourceLimit",
            "/workflows",
            "Workflow, node, or edge limits exceeded.",
        );
        return Err(c.errors);
    }
    c.name(&doc.id, "/id");
    c.name(&doc.groups[0].id, "/groups/0/id");
    {
        for p in &doc.client_profiles {
            c.name(&p.id, "/clientProfiles");
        }
    }
    if doc.name.len() > 256 {
        c.add(
            "resourceLimit",
            "/name",
            "Display names allow at most 256 UTF-8 bytes.",
        );
    }
    let mut ids = BTreeSet::new();
    let mut flows = Vec::new();
    for (wi, w) in doc.workflows.iter().enumerate() {
        let path = format!("/workflows/{wi}");
        let start_errors = c.errors.len();
        c.name(&w.id, &path);
        if !ids.insert(w.id.clone()) {
            c.add("duplicateWorkflowId", &path, "Workflow IDs must be unique.");
        }
        if w.name.len() > 256 || w.inputs.len() > 32 || w.outputs.len() > 32 {
            c.add(
                "resourceLimit",
                &path,
                "Workflow name or port count exceeds limits.",
            );
        }
        for (name, port) in &w.inputs {
            c.port_name(name, &path);
            if port
                .default
                .as_ref()
                .is_some_and(|v| scalar_type(v) != Some(port.kind))
            {
                c.add(
                    "portTypeMismatch",
                    &path,
                    "Input default must match its scalar type and bounds.",
                );
            }
        }
        for name in w.outputs.keys() {
            c.port_name(name, &path);
        }
        let mut edge_ids = BTreeSet::new();
        for e in &w.edges {
            c.name(&e.id, &path);
            if !edge_ids.insert(&e.id) {
                c.add(
                    "duplicateEdgeId",
                    &path,
                    "Edge IDs must be unique within a workflow.",
                );
            }
            if e.condition.is_some() || e.is_default.is_some() {
                c.add(
                    "unsupportedFeature",
                    &path,
                    "Route conditions are unavailable before Phase 5.",
                );
            }
        }
        for node in &w.nodes {
            c.name(node.id(), &path);
        }
        if w.nodes.iter().any(|n| {
            matches!(
                n,
                ProgramNode::ExclusiveSplit { .. } | ProgramNode::ExclusiveMerge { .. }
            )
        }) {
            c.add(
                "unsupportedFeature",
                &path,
                "Exclusive routing is unavailable before Phase 5.",
            );
            continue;
        }
        let mut auth = match crate::validate::validate_workflow(doc, w) {
            Ok(auth) => auth,
            Err(errors) => {
                for mut e in errors {
                    if c.errors.len() >= 100 {
                        break;
                    }
                    e.workflow_id = Some(w.id.clone());
                    if let Some(tail) = e.field_path.strip_prefix("/workflow") {
                        e.field_path = format!("{path}{tail}");
                    } else if e.field_path.starts_with("/layout") {
                        e.field_path = format!("{path}{}", e.field_path);
                    }
                    c.errors.push(e);
                }
                continue;
            }
        };
        let order = crate::compile::ordered_indices(w);
        let pos = order
            .iter()
            .enumerate()
            .map(|(i, n)| (w.nodes[*n].id().to_owned(), i))
            .collect::<BTreeMap<_, _>>();
        let mut last_uses = BTreeMap::new();
        for (at, index) in order.iter().enumerate() {
            let node = &w.nodes[*index];
            if let ProgramNode::Extract { config, .. } = node {
                if !w.nodes.iter().any(|n| {
                    n.id() == config.source_node_id && matches!(n, ProgramNode::Http { .. })
                }) {
                    c.add(
                        "invalidSourceType",
                        &path,
                        "Extraction source must be a local HTTP node.",
                    );
                }
            }
            if let ProgramNode::Call { config, .. } = node {
                c.mapping(config, Some(w), &pos, at, &path, None);
            }
            for src in uses(node) {
                // Literal templates permit structured JSON; scalar references do not.
                let kind = c.source(&src, Some(w), &pos, at, &path, None);
                if matches!(node, ProgramNode::SetSession { .. })
                    && kind.is_some_and(|k| k != PortType::String)
                {
                    c.add(
                        "portTypeMismatch",
                        &path,
                        "Session headers require string sources.",
                    );
                }
                if matches!(src, ScalarSource::Lifecycle { .. }) {
                    c.add(
                        "invalidLifecycleReference",
                        &path,
                        "Lifecycle references occur only in lifecycle bindings.",
                    );
                }
                if let Some(key) = src.key() {
                    last_uses.insert(key, node.id().to_owned());
                }
            }
        }
        let mut return_marker = "$return".to_owned();
        while w.nodes.iter().any(|n| n.id() == return_marker) {
            return_marker.push('_');
        }
        for out in w.outputs.values() {
            let kind = c.source(&out.value, Some(w), &pos, usize::MAX, &path, None);
            if kind.is_some_and(|t| t != out.kind) {
                c.add(
                    "portTypeMismatch",
                    &path,
                    "Output source type differs from declaration.",
                );
            }
            if let Some(key) = out.value.key() {
                last_uses.insert(key, return_marker.clone());
            }
        }
        for (key, last) in &last_uses {
            if let ValueKey::Extraction(node, _) = key {
                if let Some(e) = auth.extractions.get_mut(node) {
                    e.last_use_node_id = last.clone();
                }
            }
        }
        if c.errors.len() > start_errors {
            for e in &mut c.errors[start_errors..] {
                e.workflow_id = Some(w.id.clone());
            }
        }
        flows.push(CompiledWorkflow {
            definition: w.clone(),
            order,
            auth,
            last_uses,
        });
    }
    let lifecycle = &doc.groups[0].lifecycle;
    if !(1..=5000).contains(&lifecycle.cleanup_timeout_ms) {
        c.add(
            "invalidTimeout",
            "/groups/0/lifecycle/cleanupTimeoutMs",
            "Cleanup timeout must be between 1 and 5000 ms.",
        );
    }
    let init = lifecycle
        .initialize
        .as_ref()
        .and_then(|b| doc.workflows.iter().find(|w| w.id == b.workflow_id));
    for (phase, b) in [
        ("initialize", lifecycle.initialize.as_ref()),
        ("iteration", Some(&lifecycle.iteration)),
        ("cleanup", lifecycle.cleanup.as_ref()),
    ] {
        if let Some(b) = b {
            c.mapping(
                b,
                None,
                &BTreeMap::new(),
                0,
                &format!("/groups/0/lifecycle/{phase}"),
                if phase == "iteration" { init } else { None },
            );
        }
    }
    if !c.errors.is_empty() {
        return Err(c.errors);
    }
    // DFS summaries bound depth/count/visits without expanding call trees.
    fn visit(
        i: usize,
        flows: &[CompiledWorkflow],
        active: &mut BTreeSet<usize>,
        memo: &mut BTreeMap<usize, (usize, usize, usize)>,
    ) -> Result<(usize, usize, usize), Diagnostic> {
        if active.contains(&i) {
            return Err(problem(
                "recursiveWorkflow",
                "/workflows",
                "Recursive workflow calls are unsupported.",
            ));
        }
        if let Some(v) = memo.get(&i) {
            return Ok(*v);
        }
        active.insert(i);
        let mut depth = 1;
        let mut calls = 1usize;
        let mut nodes = flows[i].order.len();
        for n in &flows[i].definition.nodes {
            if let ProgramNode::Call { config, .. } = n {
                let child = flows
                    .iter()
                    .position(|w| w.definition.id == config.workflow_id)
                    .ok_or_else(|| {
                        problem(
                            "missingWorkflow",
                            "/workflows",
                            "Called workflow does not exist.",
                        )
                    })?;
                let (d, c, n) = visit(child, flows, active, memo)?;
                depth = depth.max(d + 1);
                calls = calls.saturating_add(c);
                nodes = nodes.saturating_add(n);
            }
        }
        active.remove(&i);
        if depth > 8 || calls > 128 || nodes > 1024 {
            return Err(problem(
                "invocationLimit",
                "/workflows",
                "Workflow invocation depth, calls, or visits exceed limits.",
            ));
        }
        memo.insert(i, (depth, calls, nodes));
        Ok((depth, calls, nodes))
    }
    let mut memo = BTreeMap::new();
    for i in 0..flows.len() {
        visit(i, &flows, &mut BTreeSet::new(), &mut memo).map_err(|e| vec![e])?;
    }
    let mut roots = Vec::new();
    for b in [
        lifecycle.initialize.as_ref(),
        Some(&lifecycle.iteration),
        lifecycle.cleanup.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        roots.push(
            flows
                .iter()
                .position(|w| w.definition.id == b.workflow_id)
                .expect("validated binding"),
        );
    }
    if roots.iter().map(|i| memo[i].1).sum::<usize>() > 128
        || roots.iter().map(|i| memo[i].2).sum::<usize>() > 1024
    {
        return Err(vec![problem(
            "invocationLimit",
            "/groups",
            "Lifecycle exceeds aggregate call/visit limits.",
        )]);
    }
    let mut reachable = BTreeSet::new();
    let mut pending = roots;
    while let Some(i) = pending.pop() {
        if reachable.insert(i) {
            for node in &flows[i].definition.nodes {
                if let ProgramNode::Call { config, .. } = node {
                    pending.push(
                        flows
                            .iter()
                            .position(|w| w.definition.id == config.workflow_id)
                            .expect("validated call"),
                    );
                }
            }
        }
    }
    if !reachable.iter().any(|i| {
        flows[*i]
            .definition
            .nodes
            .iter()
            .any(|n| matches!(n, ProgramNode::Http { .. }))
    }) {
        return Err(vec![problem(
            "missingHttp",
            "/groups",
            "A runnable lifecycle requires at least one reachable HTTP node.",
        )]);
    }
    let mut auth = AuthPlan::default();
    let mut required = BTreeSet::new();
    for i in &reachable {
        for r in flows[*i].auth.required_secrets() {
            required.insert((r.identity_id().to_owned(), r.field().to_owned()));
        }
    }
    // Include identity dependencies in call/output/lifecycle mappings, not merely HTTP bodies.
    fn fields(src: &ScalarSource, set: &mut BTreeSet<String>) {
        if let ScalarSource::Identity { field } = src {
            set.insert(field.clone());
        }
    }
    let mut identity_fields = BTreeSet::new();
    for i in &reachable {
        for n in &flows[*i].definition.nodes {
            for src in uses(n) {
                fields(&src, &mut identity_fields);
            }
        }
        for o in flows[*i].definition.outputs.values() {
            fields(&o.value, &mut identity_fields);
        }
    }
    for b in [
        lifecycle.initialize.as_ref(),
        Some(&lifecycle.iteration),
        lifecycle.cleanup.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        for src in b.inputs.values() {
            fields(src, &mut identity_fields);
        }
    }
    let crate::IdentityAssignment::Shared { identity_id } = &doc.groups[0].identity_assignment;
    let identity = identity_id
        .as_ref()
        .and_then(|id| doc.identities.iter().find(|v| &v.id == id));
    // All definitions must be semantically valid even if unused; only reachable refs are resolved.
    for field in c.identity_fields {
        if identity.is_none_or(|v| !v.credentials.contains_key(&field)) {
            return Err(vec![problem(
                "missingIdentityCredential",
                "/identities",
                "Assigned identity does not declare a consumed field.",
            )]);
        }
    }
    for field in identity_fields {
        let identity = identity.ok_or_else(|| {
            vec![problem(
                "missingIdentity",
                "/groups",
                "Assigned identity is required.",
            )]
        })?;
        required.insert((identity.id.clone(), field));
    }
    for (id, field) in required {
        let reference = doc
            .identities
            .iter()
            .find(|v| v.id == id)
            .and_then(|v| v.credentials.get(&field))
            .expect("validated identity source")
            .clone();
        auth.required_secrets.push(crate::RequiredSecret {
            identity_id: id,
            field,
            reference,
        });
    }
    Ok(Program {
        document: doc.clone(),
        flows,
        reachable: reachable.into_iter().collect(),
        auth,
    })
}
