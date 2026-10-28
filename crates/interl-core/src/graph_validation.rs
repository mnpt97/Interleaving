//! Validate transport declarations and the connected, structured workflow graph.
//! This pass establishes traversal invariants used by reference and auth analysis.

use crate::diagnostic::pointer;
use crate::{
    AuthPlan, Diagnostic, ErrorCategory, ScenarioDocument, ValidationError, WorkflowDefinition,
    WorkflowNode,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A capped diagnostic accumulator shared by graph and authentication validators.
/// The tuple-struct `.0` stores the vector without introducing another collection abstraction.
#[derive(Default)]
pub(crate) struct Errors(pub(crate) Vec<Diagnostic>);
impl Errors {
    /// Retain an existing diagnostic unless the 100-item reporting cap was reached.
    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        if self.0.len() < 100 {
            self.0.push(diagnostic);
        }
    }
    /// Create a semantic diagnostic and attach optional local-node context.
    pub(crate) fn add(&mut self, code: &str, path: &str, message: &str, node: Option<&str>) {
        if self.0.len() < 100 {
            let mut diagnostic = Diagnostic::new(ErrorCategory::Validation, code, path, message);
            diagnostic.node_id = node.map(str::to_owned);
            self.0.push(diagnostic);
        }
    }
    /// Check nonempty, unique IDs from any iterator of borrowed string slices.
    /// `'a` ties each yielded `&str` to its owner without cloning IDs into the set.
    pub(crate) fn ids<'a>(&mut self, items: impl Iterator<Item = &'a str>, path: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut valid = true;
        for (index, id) in items.enumerate() {
            let field = format!("{path}/{index}/id");
            if id.trim().is_empty() {
                self.add("emptyId", &field, "IDs must not be empty.", None);
                valid = false;
            }
            if !seen.insert(id) {
                self.add(
                    "duplicateId",
                    &field,
                    "IDs must be unique within their collection.",
                    None,
                );
                valid = false;
            }
        }
        valid
    }
    /// Validate literal HTTP header names and reject line breaks in their values.
    fn headers(&mut self, headers: &BTreeMap<String, String>, path: &str, node: Option<&str>) {
        for (name, value) in headers {
            let token = !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
            if !token || value.contains(['\r', '\n']) {
                self.add(
                    "invalidHeader",
                    &pointer(path, name),
                    "Use a valid HTTP header name and a value without line breaks.",
                    node,
                );
            }
        }
    }
}

/// Check shared declarations, local node requests, graph/layout, and authentication uses.
/// Returns the workflow's capture/extraction plan only after all checks succeed.
pub(crate) fn validate_workflow(
    scenario: &ScenarioDocument,
    workflow: &WorkflowDefinition,
) -> Result<AuthPlan, Vec<ValidationError>> {
    let mut errors = Errors::default();
    if scenario.id.trim().is_empty() {
        errors.add("emptyId", "/id", "Scenario ID must not be empty.", None);
    }
    if scenario.name.trim().is_empty() {
        errors.add(
            "emptyName",
            "/name",
            "Scenario name must not be empty.",
            None,
        );
    }
    errors.ids(
        scenario
            .client_profiles
            .iter()
            .map(|profile| profile.id.as_str()),
        "/clientProfiles",
    );
    errors.ids(
        scenario.groups.iter().map(|group| group.id.as_str()),
        "/groups",
    );
    if scenario.client_profiles.is_empty() {
        errors.add(
            "missingProfile",
            "/clientProfiles",
            "Declare at least one client profile.",
            None,
        );
    }
    if scenario.groups.is_empty() {
        errors.add(
            "missingGroup",
            "/groups",
            "Declare at least one virtual-user group.",
            None,
        );
    }
    let profiles: BTreeSet<_> = scenario
        .client_profiles
        .iter()
        .map(|profile| profile.id.as_str())
        .collect();
    for (index, profile) in scenario.client_profiles.iter().enumerate() {
        let path = format!("/clientProfiles/{index}");
        let valid_url = url::Url::parse(&profile.base_url).is_ok_and(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && profile.base_url.len() <= 8192
        });
        if !valid_url {
            errors.add(
                "invalidBaseUrl",
                &format!("{path}/baseUrl"),
                "Use an absolute HTTP(S) URL without embedded credentials.",
                None,
            );
        }
        if profile.timeout_ms == 0 {
            errors.add(
                "invalidTimeout",
                &format!("{path}/timeoutMs"),
                "Timeout must be greater than zero.",
                None,
            );
        }
        errors.headers(&profile.headers, &format!("{path}/headers"), None);
    }
    for (index, group) in scenario.groups.iter().enumerate() {
        let path = format!("/groups/{index}");
        if !profiles.contains(group.client_profile_id.as_str()) {
            errors.add(
                "missingProfile",
                &format!("{path}/clientProfileId"),
                "Referenced client profile does not exist.",
                None,
            );
        }
    }
    let nodes = &workflow.nodes;
    let unique = errors.ids(nodes.iter().map(WorkflowNode::id), "/workflow/nodes");
    for (index, node) in nodes.iter().enumerate() {
        if let WorkflowNode::Http { config, .. } = node {
            let path = format!("/workflow/nodes/{index}/config");
            if !config.path.starts_with('/')
                || config.path.starts_with("//")
                || config.path.contains(['\r', '\n'])
            {
                errors.add(
                    "invalidRequestPath",
                    &format!("{path}/path"),
                    "Use a relative request path starting with a single slash.",
                    Some(node.id()),
                );
            }
            if config.timeout_ms == Some(0) {
                errors.add(
                    "invalidTimeout",
                    &format!("{path}/timeoutMs"),
                    "Timeout must be greater than zero.",
                    Some(node.id()),
                );
            }
            errors.headers(&config.headers, &format!("{path}/headers"), Some(node.id()));
        }
    }
    if unique {
        validate_graph(workflow, &mut errors);
    }
    if let Some(layout) = &workflow.layout {
        for id in layout.connections.keys() {
            if !workflow.edges.iter().any(|edge| edge.id == *id) {
                errors.add(
                    "unknownLayoutEdge",
                    &pointer("/layout/connections", id),
                    "Connection layout refers to a missing edge.",
                    None,
                );
            }
        }
        let ids: BTreeSet<_> = nodes.iter().map(WorkflowNode::id).collect();
        for (id, position) in &layout.positions {
            let path = pointer("/layout/positions", id);
            if !ids.contains(id.as_str()) {
                errors.add(
                    "unknownLayoutNode",
                    &path,
                    "Position refers to a missing workflow node.",
                    Some(id),
                );
            }
            if !position.x.is_finite() || !position.y.is_finite() {
                errors.add(
                    "invalidPosition",
                    &path,
                    "Position coordinates must be finite.",
                    Some(id),
                );
            }
        }
    }
    if errors.0.is_empty() {
        crate::auth_validation::validate_auth(scenario, workflow)
    } else {
        Err(errors.0)
    }
}

/// Reject invalid endpoints, duplicate edges, cycles, disconnected nodes, and non-linear degrees.
/// The queue-based traversal avoids recursively walking untrusted graph structure.
fn validate_graph(workflow: &WorkflowDefinition, errors: &mut Errors) {
    let nodes = &workflow.nodes;
    let indices: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id(), index))
        .collect();
    let starts: Vec<_> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| matches!(node, WorkflowNode::Start { .. }))
        .map(|(index, _)| index)
        .collect();
    let ends = nodes
        .iter()
        .filter(|node| matches!(node, WorkflowNode::End { .. }))
        .count();
    if starts.len() != 1 {
        errors.add(
            "startCount",
            "/workflow/nodes",
            "Workflow requires exactly one start node.",
            None,
        );
    }
    if ends != 1 {
        errors.add(
            "endCount",
            "/workflow/nodes",
            "Workflow requires exactly one end node.",
            None,
        );
    }
    let mut outgoing = vec![Vec::new(); nodes.len()];
    let mut incoming = vec![0usize; nodes.len()];
    let mut edges = BTreeSet::new();
    let mut dangling = false;
    for (index, edge) in workflow.edges.iter().enumerate() {
        let path = format!("/workflow/edges/{index}");
        let (Some(&from), Some(&to)) = (
            indices.get(edge.from.as_str()),
            indices.get(edge.to.as_str()),
        ) else {
            errors.add(
                "danglingEdge",
                &path,
                "Both edge endpoints must reference existing nodes.",
                None,
            );
            dangling = true;
            continue;
        };
        if !edges.insert((from, to)) && !matches!(nodes[from], WorkflowNode::ExclusiveSplit { .. })
        {
            errors.add(
                "duplicateEdge",
                &path,
                "Duplicate workflow edge.",
                Some(nodes[from].id()),
            );
            continue;
        }
        if from == to {
            errors.add(
                "selfLoop",
                &path,
                "Self-loops are unsupported.",
                Some(nodes[from].id()),
            );
        }
        outgoing[from].push(to);
        incoming[to] += 1;
    }
    if dangling {
        return;
    }
    for (index, node) in nodes.iter().enumerate() {
        let degrees = (incoming[index], outgoing[index].len());
        let valid = match node {
            WorkflowNode::Start { .. } => degrees == (0, 1),
            WorkflowNode::End { .. } => degrees == (1, 0),
            WorkflowNode::ExclusiveSplit { .. } => degrees.0 == 1 && (2..=8).contains(&degrees.1),
            WorkflowNode::ExclusiveMerge { .. } => (2..=8).contains(&degrees.0) && degrees.1 == 1,
            _ => degrees == (1, 1),
        };
        if !valid {
            let message = match node {
                WorkflowNode::Start { .. } => "Start requires no inputs and exactly one output.",
                WorkflowNode::End { .. } => "End requires exactly one input and no outputs.",
                WorkflowNode::ExclusiveSplit { .. } => {
                    "An exclusive split requires exactly one input and 2–8 outputs."
                }
                WorkflowNode::ExclusiveMerge { .. } => {
                    "An exclusive merge requires 2–8 inputs and exactly one output."
                }
                _ => "This operation requires exactly one input and exactly one output.",
            };
            errors.add(
                "nonLinearGraph",
                &format!("/workflow/nodes/{index}"),
                message,
                Some(node.id()),
            );
        }
    }

    // Kahn's algorithm finds cycles even in disconnected components.
    let mut degree = incoming;
    let mut queue: VecDeque<_> = degree
        .iter()
        .enumerate()
        .filter(|(_, count)| **count == 0)
        .map(|(index, _)| index)
        .collect();
    let mut visited_count = 0;
    while let Some(index) = queue.pop_front() {
        visited_count += 1;
        for &next in &outgoing[index] {
            degree[next] -= 1;
            if degree[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    if visited_count != nodes.len() {
        errors.add(
            "cycle",
            "/workflow/edges",
            "Cycles are unsupported in the current schema.",
            None,
        );
    }
    if visited_count == nodes.len() && errors.0.is_empty() {
        validate_regions(workflow, &indices, &outgoing, errors);
    }
    if let [start] = starts.as_slice() {
        let mut seen = BTreeSet::new();
        let mut pending = vec![*start];
        while let Some(index) = pending.pop() {
            if seen.insert(index) {
                pending.extend(&outgoing[index]);
            }
        }
        for (index, node) in nodes.iter().enumerate() {
            if !seen.contains(&index) {
                errors.add(
                    "unreachableNode",
                    &format!("/workflow/nodes/{index}"),
                    "Node is unreachable from the start.",
                    Some(node.id()),
                );
            }
        }
    }
}

/// Return a stable topological order of all nodes. Validation must precede use.
/// Kahn's queue handles every arm rather than selecting an arbitrary outgoing edge.
pub(crate) fn ordered_indices(workflow: &WorkflowDefinition) -> Vec<usize> {
    let indices: BTreeMap<_, _> = workflow
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id(), i))
        .collect();
    let mut incoming = vec![0; workflow.nodes.len()];
    let mut outgoing = vec![Vec::new(); workflow.nodes.len()];
    for edge in &workflow.edges {
        if let (Some(&a), Some(&b)) = (
            indices.get(edge.from.as_str()),
            indices.get(edge.to.as_str()),
        ) {
            outgoing[a].push(b);
            incoming[b] += 1;
        }
    }
    let mut queue: VecDeque<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect();
    let mut order = Vec::new();
    while let Some(i) = queue.pop_front() {
        order.push(i);
        for &next in &outgoing[i] {
            incoming[next] -= 1;
            if incoming[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    order
}

/// Compute all-path producer availability once for repeated scalar-reference checks.
/// Topological predecessors have already been processed, so intersection retains
/// only producers present on every incoming path. Sets borrow immutable node IDs.
pub(crate) fn dominators(workflow: &WorkflowDefinition) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut result: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for index in ordered_indices(workflow) {
        let id = workflow.nodes[index].id();
        let mut incoming = workflow
            .edges
            .iter()
            .filter(|edge| edge.to == id)
            .filter_map(|edge| result.get(edge.from.as_str()));
        let mut common = incoming.next().cloned().unwrap_or_default();
        for previous in incoming {
            common.retain(|key| previous.contains(key));
        }
        common.insert(id);
        result.insert(id, common);
    }
    result
}

/// Test all-path availability by trying to reach a consumer while excluding its producer.
/// The graph is bounded and validated; an iterative search avoids untrusted recursion.
pub(crate) fn dominates(workflow: &WorkflowDefinition, producer: &str, consumer: &str) -> bool {
    if producer == consumer {
        return false;
    }
    if !workflow.nodes.iter().any(|n| n.id() == producer) {
        return false;
    }
    let mut pending: Vec<_> = workflow
        .nodes
        .iter()
        .filter(|n| matches!(n, WorkflowNode::Start { .. }))
        .map(WorkflowNode::id)
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if id == producer || !seen.insert(id) {
            continue;
        }
        if id == consumer {
            return false;
        }
        pending.extend(
            workflow
                .edges
                .iter()
                .filter(|e| e.from == id)
                .map(|e| e.to.as_str()),
        );
    }
    true
}

/// Infer each split's nearest common successor on every path (its postdominator).
/// Reverse topological order makes successor sets available before their predecessors.
/// Intersecting sets excludes nodes reachable on only some branches; choosing the
/// first remaining node in forward order handles nested regions without ID hints.
fn inferred_gateway_pairs(
    workflow: &WorkflowDefinition,
    outgoing: &[Vec<usize>],
) -> BTreeMap<usize, usize> {
    let order = ordered_indices(workflow);
    let mut postdominators = vec![BTreeSet::new(); workflow.nodes.len()];
    for &index in order.iter().rev() {
        let mut successors = outgoing[index].iter();
        let mut common = successors
            .next()
            .map(|next| postdominators[*next].clone())
            .unwrap_or_default();
        for next in successors {
            common.retain(|candidate| postdominators[*next].contains(candidate));
        }
        common.insert(index);
        postdominators[index] = common;
    }
    order
        .iter()
        .filter_map(|&split| {
            if !matches!(workflow.nodes[split], WorkflowNode::ExclusiveSplit { .. }) {
                return None;
            }
            let next = order.iter().copied().find(|&candidate| {
                candidate != split && postdominators[split].contains(&candidate)
            })?;
            matches!(workflow.nodes[next], WorkflowNode::ExclusiveMerge { .. })
                .then_some((split, next))
        })
        .collect()
}

/// Check paired, disjoint, properly nested regions and route-only edge annotations.
fn validate_regions(
    workflow: &WorkflowDefinition,
    indices: &BTreeMap<&str, usize>,
    outgoing: &[Vec<usize>],
    errors: &mut Errors,
) {
    let pairs = inferred_gateway_pairs(workflow, outgoing);
    for (index, node) in workflow.nodes.iter().enumerate() {
        let WorkflowNode::ExclusiveSplit { config, .. } = node else {
            if matches!(node, WorkflowNode::ExclusiveMerge { .. })
                && pairs.values().filter(|&&merge| merge == index).count() != 1
            {
                errors.add(
                    "invalidGatewayPair",
                    &format!("/workflow/nodes/{index}"),
                    "Connect this merge to branches from exactly one split.",
                    Some(node.id()),
                );
            }
            continue;
        };
        let Some(&merge) = pairs.get(&index) else {
            errors.add(
                "invalidGatewayPair",
                &format!("/workflow/nodes/{index}"),
                "All split branches must reconverge at one exclusive merge before continuing.",
                Some(node.id()),
            );
            continue;
        };
        if !workflow
            .nodes
            .iter()
            .any(|n| n.id() == config.source_node_id && matches!(n, WorkflowNode::Http { .. }))
            || !dominates(workflow, &config.source_node_id, node.id())
        {
            errors.add(
                "routeSourceUnavailable",
                &format!("/workflow/nodes/{index}/config/sourceNodeId"),
                "Source must be an HTTP node available on every path to the split.",
                Some(node.id()),
            );
        }
        if !dominates(workflow, node.id(), workflow.nodes[merge].id()) {
            errors.add(
                "unstructuredBranch",
                "/workflow/edges",
                "The split must dominate its paired merge.",
                Some(node.id()),
            );
        }
        let mut occupied = BTreeSet::new();
        for &arm in &outgoing[index] {
            let mut seen = BTreeSet::new();
            let mut pending = vec![arm];
            let mut reached = false;
            while let Some(i) = pending.pop() {
                if i == merge {
                    reached = true;
                    continue;
                }
                if !seen.insert(i) {
                    continue;
                }
                if outgoing[i].is_empty() {
                    errors.add(
                        "unstructuredBranch",
                        "/workflow/edges",
                        "Every arm must reach its paired merge before End.",
                        Some(node.id()),
                    );
                }
                pending.extend(&outgoing[i]);
            }
            if !reached || seen.iter().any(|i| occupied.contains(i)) {
                errors.add(
                    "unstructuredBranch",
                    "/workflow/edges",
                    "Branch arms must be disjoint and converge only at their paired merge.",
                    Some(node.id()),
                );
            }
            for &i in &seen {
                if matches!(workflow.nodes[i], WorkflowNode::ExclusiveSplit { .. })
                    && !pairs.get(&i).is_some_and(|m| seen.contains(m))
                {
                    errors.add(
                        "unstructuredBranch",
                        "/workflow/edges",
                        "Nested split/merge regions must be properly contained.",
                        Some(node.id()),
                    );
                }
            }
            occupied.extend(seen);
        }
    }
    for (index, edge) in workflow.edges.iter().enumerate() {
        let split = indices
            .get(edge.from.as_str())
            .is_some_and(|i| matches!(workflow.nodes[*i], WorkflowNode::ExclusiveSplit { .. }));
        let valid = if split {
            (edge.condition.is_some() && edge.is_default.is_none())
                || (edge.condition.is_none() && edge.is_default == Some(true))
        } else {
            edge.condition.is_none() && edge.is_default.is_none()
        };
        if !valid {
            errors.add(
                "invalidRouteEdge",
                &format!("/workflow/edges/{index}"),
                "Split edges require a condition or default:true; ordinary edges allow neither.",
                None,
            );
        }
    }
    for node in &workflow.nodes {
        if matches!(node, WorkflowNode::ExclusiveSplit { .. })
            && workflow
                .edges
                .iter()
                .filter(|e| e.from == node.id() && e.is_default == Some(true))
                .count()
                > 1
        {
            errors.add(
                "multipleDefaultRoutes",
                "/workflow/edges",
                "A split permits at most one default route.",
                Some(node.id()),
            );
        }
    }
}
