use crate::diagnostic::pointer;
use crate::{
    parse_scenario, AuthPlan, Diagnostic, ErrorCategory, ScenarioDocument, ScenarioFormat,
    ValidationError, WorkflowNode,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub valid: bool,
    pub document: Option<ScenarioDocument>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn inspect_scenario(source: &str, format: ScenarioFormat) -> ValidationResult {
    match parse_scenario(source, format) {
        Ok(document) => {
            let diagnostics = validate_scenario(&document).err().unwrap_or_default();
            ValidationResult {
                valid: diagnostics.is_empty(),
                document: Some(document),
                diagnostics,
            }
        }
        Err(error) => ValidationResult {
            valid: false,
            document: None,
            diagnostics: vec![error.diagnostic],
        },
    }
}

#[derive(Default)]
pub(crate) struct Errors(pub(crate) Vec<Diagnostic>);
impl Errors {
    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        if self.0.len() < 100 {
            self.0.push(diagnostic);
        }
    }
    pub(crate) fn add(&mut self, code: &str, path: &str, message: &str, node: Option<&str>) {
        if self.0.len() < 100 {
            let mut diagnostic = Diagnostic::new(ErrorCategory::Validation, code, path, message);
            diagnostic.node_id = node.map(str::to_owned);
            self.0.push(diagnostic);
        }
    }
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

pub fn validate_scenario(scenario: &ScenarioDocument) -> Result<(), Vec<ValidationError>> {
    validate_and_compile_auth(scenario).map(|_| ())
}

pub(crate) fn validate_and_compile_auth(
    scenario: &ScenarioDocument,
) -> Result<AuthPlan, Vec<ValidationError>> {
    let mut errors = Errors::default();
    if scenario.schema_version != 1 {
        errors.add(
            "unsupportedVersion",
            "/schemaVersion",
            "Only schemaVersion 1 is supported.",
            None,
        );
    }
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
        if group.count == 0 {
            errors.add(
                "invalidUserCount",
                &format!("{path}/count"),
                "User count must be greater than zero.",
                None,
            );
        }
    }
    let nodes = &scenario.workflow.nodes;
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
        validate_graph(scenario, &mut errors);
    }
    if let Some(layout) = &scenario.layout {
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
        crate::auth_validation::validate_auth(scenario)
    } else {
        Err(errors.0)
    }
}

fn validate_graph(scenario: &ScenarioDocument, errors: &mut Errors) {
    let nodes = &scenario.workflow.nodes;
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
    if !nodes
        .iter()
        .any(|node| matches!(node, WorkflowNode::Http { .. }))
    {
        errors.add(
            "missingHttp",
            "/workflow/nodes",
            "Workflow requires one or more HTTP nodes.",
            None,
        );
    }
    let mut outgoing = vec![Vec::new(); nodes.len()];
    let mut incoming = vec![0usize; nodes.len()];
    let mut edges = BTreeSet::new();
    let mut dangling = false;
    for (index, edge) in scenario.workflow.edges.iter().enumerate() {
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
        if !edges.insert((from, to)) {
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
        let expected = match node {
            WorkflowNode::Start { .. } => (0, 1),
            WorkflowNode::End { .. } => (1, 0),
            WorkflowNode::Http { .. }
            | WorkflowNode::Extract { .. }
            | WorkflowNode::SetSession { .. } => (1, 1),
        };
        if (incoming[index], outgoing[index].len()) != expected {
            errors.add(
                "nonLinearGraph",
                &format!("/workflow/nodes/{index}"),
                "Node connections must form a single sequential start → workflow steps → end chain.",
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
