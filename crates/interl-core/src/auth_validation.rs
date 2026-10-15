use crate::compile::{ordered_indices, AuthPlan, CompiledExtraction, RequiredSecret};
use crate::diagnostic::pointer;
use crate::expressions::compile_expression;
use crate::validate::Errors;
use crate::{
    Diagnostic, ErrorCategory, IdentityAssignment, RequestBody, ScenarioDocument, SecretReference,
    SessionHeaderValue, TemplateValue, ValidationError, WorkflowNode,
};
use std::collections::{BTreeMap, BTreeSet};

struct Check<'a> {
    scenario: &'a ScenarioDocument,
    positions: BTreeMap<&'a str, usize>,
    indices: BTreeMap<&'a str, usize>,
    errors: Errors,
    plan: AuthPlan,
    identity_fields: BTreeSet<String>,
    output_uses: BTreeMap<String, String>,
}

impl Check<'_> {
    fn name(&mut self, name: &str, path: &str, node: Option<&str>) {
        if name.trim().is_empty() || name.len() > 128 {
            self.errors.add(
                "invalidAuthName",
                path,
                "Authentication identifiers must be nonempty and at most 128 UTF-8 bytes.",
                node,
            );
        }
    }
    fn limit(&mut self, path: &str, node: &str) {
        let mut error = Diagnostic::new(ErrorCategory::ResourceLimit, "templateLimit", path, "JSON templates allow at most 256 composition nodes and depth 32, including the body object.");
        error.node_id = Some(node.into());
        self.errors.push(error);
    }
    fn identity_source(&mut self, field: &str, path: &str, node: &str) {
        self.name(field, &pointer(path, "field"), Some(node));
        self.identity_fields.insert(field.to_owned());
    }
    fn earlier_source(&mut self, source: &str, path: &str, node: &str) -> Option<usize> {
        self.name(source, path, Some(node));
        let Some(&index) = self.indices.get(source) else {
            self.errors.add(
                "missingSourceNode",
                path,
                "Referenced source node does not exist.",
                Some(node),
            );
            return None;
        };
        if self.positions[source] >= self.positions[node] {
            self.errors.add("forwardReference", path, "Source must precede this node in workflow connectivity; self and forward references are unavailable.", Some(node));
            return None;
        }
        Some(index)
    }
    fn output_source(&mut self, source: &str, name: &str, path: &str, node: &str) {
        self.name(name, &pointer(path, "name"), Some(node));
        if let Some(index) = self.earlier_source(source, &pointer(path, "nodeId"), node) {
            match &self.scenario.workflow.nodes[index] {
                WorkflowNode::Extract { config, .. } if config.output == name => {
                    self.output_uses.insert(source.to_owned(), node.to_owned());
                }
                WorkflowNode::Extract { .. } => self.errors.add(
                    "missingOutput",
                    &pointer(path, "name"),
                    "Referenced extraction does not declare this output.",
                    Some(node),
                ),
                _ => self.errors.add(
                    "invalidSourceType",
                    &pointer(path, "nodeId"),
                    "Output references must point to an earlier extraction node.",
                    Some(node),
                ),
            }
        }
    }
    fn template(
        &mut self,
        fields: &BTreeMap<String, TemplateValue>,
        path: &str,
        node: &str,
    ) -> bool {
        if fields.len() > 255 {
            self.limit(path, node);
            return false;
        }
        let mut pending: Vec<_> = fields
            .iter()
            .map(|(key, value)| (value, pointer(&pointer(path, "fields"), key), 2))
            .collect();
        let mut visited = 1;
        let mut sensitive = false;
        while let Some((value, path, depth)) = pending.pop() {
            visited += 1;
            if depth > 32 || visited > 256 {
                self.limit(&path, node);
                break;
            }
            match value {
                TemplateValue::Literal { .. } => {}
                TemplateValue::Identity { field } => {
                    self.identity_source(field, &path, node);
                    sensitive = true;
                }
                TemplateValue::Extraction { node_id, name } => {
                    self.output_source(node_id, name, &path, node);
                    sensitive = true;
                }
                TemplateValue::Object { fields } => {
                    if visited + pending.len() + fields.len() > 256 {
                        self.limit(&path, node);
                        break;
                    }
                    pending.extend(fields.iter().map(|(key, value)| {
                        (value, pointer(&pointer(&path, "fields"), key), depth + 1)
                    }));
                }
                TemplateValue::Array { items } => {
                    if visited + pending.len() + items.len() > 256 {
                        self.limit(&path, node);
                        break;
                    }
                    pending.extend(items.iter().enumerate().map(|(index, value)| {
                        (
                            value,
                            pointer(&pointer(&path, "items"), &index.to_string()),
                            depth + 1,
                        )
                    }));
                }
            }
        }
        sensitive
    }
    fn header_name(&mut self, name: &str, path: &str, node: &str) {
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        {
            self.errors.add(
                "invalidHeader",
                path,
                "Use an HTTP token header name of at most 128 bytes.",
                Some(node),
            );
        }
    }
    fn header_value(&mut self, value: &SessionHeaderValue, path: &str, node: &str) {
        let prefix = match value {
            SessionHeaderValue::Literal { value, prefix } => {
                if value.len().saturating_add(prefix.len()) > 8192 || invalid_header_text(value) {
                    self.errors.add("invalidHeaderValue", &pointer(path, "value"), "Rendered literal header values must be at most 8192 bytes and contain no prohibited control characters.", Some(node));
                }
                prefix
            }
            SessionHeaderValue::Identity { field, prefix } => {
                self.identity_source(field, path, node);
                prefix
            }
            SessionHeaderValue::Extraction {
                node_id,
                name,
                prefix,
            } => {
                self.output_source(node_id, name, path, node);
                prefix
            }
        };
        if prefix.len() > 8192 || invalid_header_text(prefix) {
            self.errors.add("invalidHeaderValue", &pointer(path, "prefix"), "Header prefixes must be at most 8192 bytes and contain no prohibited control characters.", Some(node));
        }
    }
}

fn invalid_header_text(value: &str) -> bool {
    value.bytes().any(|b| (b < 32 && b != b'\t') || b == 127)
}

pub(crate) fn validate_auth(scenario: &ScenarioDocument) -> Result<AuthPlan, Vec<ValidationError>> {
    // Base graph validation has already established a single connected chain.
    let ordered = ordered_indices(scenario);
    let mut check = Check {
        scenario,
        positions: ordered
            .iter()
            .enumerate()
            .map(|(position, &index)| (scenario.workflow.nodes[index].id(), position))
            .collect(),
        indices: scenario
            .workflow
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id(), index))
            .collect(),
        errors: Errors::default(),
        plan: AuthPlan::default(),
        identity_fields: BTreeSet::new(),
        output_uses: BTreeMap::new(),
    };
    check.errors.ids(
        scenario
            .identities
            .iter()
            .map(|identity| identity.id.as_str()),
        "/identities",
    );
    let identities: BTreeMap<_, _> = scenario
        .identities
        .iter()
        .map(|identity| (identity.id.as_str(), identity))
        .collect();
    for (index, identity) in scenario.identities.iter().enumerate() {
        let path = format!("/identities/{index}");
        check.name(&identity.id, &pointer(&path, "id"), None);
        for (field, reference) in &identity.credentials {
            let path = pointer(&pointer(&path, "credentials"), field);
            check.name(field, &path, None);
            let SecretReference::Env { name } = reference;
            let mut bytes = name.bytes();
            if name.len() > 128
                || !bytes
                    .next()
                    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                check.errors.add(
                    "invalidSecretReference",
                    &pointer(&path, "name"),
                    "Environment references require an ASCII variable name of at most 128 bytes.",
                    None,
                );
            }
        }
    }
    for (index, group) in scenario.groups.iter().enumerate() {
        let IdentityAssignment::Shared { identity_id } = &group.identity_assignment;
        if let Some(id) = identity_id {
            let path = format!("/groups/{index}/identityAssignment/identityId");
            check.name(id, &path, None);
            if !identities.contains_key(id.as_str()) {
                check.errors.add(
                    "missingIdentity",
                    &path,
                    "Assigned identity does not exist.",
                    None,
                );
            }
        }
    }
    // Sources are known before HTTP capture: extraction may run much later.
    let extraction_sources: BTreeSet<_> = scenario
        .workflow
        .nodes
        .iter()
        .filter_map(|node| match node {
            WorkflowNode::Extract { config, .. } => Some(config.source_node_id.as_str()),
            _ => None,
        })
        .collect();
    let mut sensitive_session = false;
    for &index in &ordered {
        let node = &scenario.workflow.nodes[index];
        let node_id = node.id();
        let path = format!("/workflow/nodes/{index}/config");
        match node {
            WorkflowNode::Http { config, .. } => {
                let dynamic = match &config.body {
                    Some(RequestBody::JsonTemplate { fields }) => {
                        check.template(fields, &pointer(&path, "body"), node_id)
                    }
                    _ => false,
                };
                // Cookies and server echoes can outlive the operation that
                // introduced a secret. Capture remains conservative thereafter.
                sensitive_session |= dynamic || extraction_sources.contains(node_id);
                if sensitive_session {
                    check.plan.sensitive_http_nodes.insert(node_id.into());
                }
            }
            WorkflowNode::Extract { config, .. } => {
                check.name(
                    node_id,
                    &format!("/workflow/nodes/{index}/id"),
                    Some(node_id),
                );
                check.name(&config.output, &pointer(&path, "output"), Some(node_id));
                if let Some(source_index) = check.earlier_source(
                    &config.source_node_id,
                    &pointer(&path, "sourceNodeId"),
                    node_id,
                ) {
                    if matches!(
                        scenario.workflow.nodes[source_index],
                        WorkflowNode::Http { .. }
                    ) {
                        check
                            .plan
                            .response_last_uses
                            .insert(config.source_node_id.clone(), node_id.into());
                    } else {
                        check.errors.add(
                            "invalidSourceType",
                            &pointer(&path, "sourceNodeId"),
                            "Extraction sources must be earlier HTTP nodes.",
                            Some(node_id),
                        );
                    }
                }
                match compile_expression(&config.expression) {
                    Ok(expression) => {
                        check.plan.extractions.insert(
                            node_id.into(),
                            CompiledExtraction {
                                source_node_id: config.source_node_id.clone(),
                                output: config.output.clone(),
                                expression,
                                last_use_node_id: node_id.into(),
                            },
                        );
                    }
                    Err(error) => {
                        let category = if error.code == "expressionLimit" {
                            ErrorCategory::ResourceLimit
                        } else {
                            ErrorCategory::Validation
                        };
                        let mut diagnostic = Diagnostic::new(
                            category,
                            error.code,
                            &pointer(&path, "expression"),
                            error.message,
                        );
                        diagnostic.node_id = Some(node_id.into());
                        let mut line = 1;
                        let mut column = 1;
                        for (_, ch) in config
                            .expression
                            .char_indices()
                            .take_while(|(index, _)| *index < error.offset)
                        {
                            if ch == '\n' {
                                line += 1;
                                column = 1;
                            } else {
                                column += 1;
                            }
                        }
                        diagnostic.expression_line = Some(line);
                        diagnostic.expression_column = Some(column);
                        check.errors.push(diagnostic);
                    }
                }
            }
            WorkflowNode::SetSession { config, .. } => {
                check.name(
                    node_id,
                    &format!("/workflow/nodes/{index}/id"),
                    Some(node_id),
                );
                if config.set_headers.is_empty() && config.remove_headers.is_empty() {
                    check.errors.add(
                        "emptySessionUpdate",
                        &path,
                        "Declare at least one header set or removal operation.",
                        Some(node_id),
                    );
                }
                let mut set = BTreeSet::new();
                for (name, value) in &config.set_headers {
                    let path = pointer(&pointer(&path, "setHeaders"), name);
                    check.header_name(name, &path, node_id);
                    if !set.insert(name.to_ascii_lowercase()) {
                        check.errors.add(
                            "duplicateHeaderOperation",
                            &path,
                            "Header names must be unique ignoring case.",
                            Some(node_id),
                        );
                    }
                    check.header_value(value, &path, node_id);
                }
                let mut removed = BTreeSet::new();
                for (index, name) in config.remove_headers.iter().enumerate() {
                    let path = pointer(&pointer(&path, "removeHeaders"), &index.to_string());
                    check.header_name(name, &path, node_id);
                    let normalized = name.to_ascii_lowercase();
                    if !removed.insert(normalized.clone()) {
                        check.errors.add(
                            "duplicateHeaderOperation",
                            &path,
                            "Header removals must be unique ignoring case.",
                            Some(node_id),
                        );
                    }
                    if set.contains(&normalized) {
                        check.errors.add(
                            "conflictingHeaderOperation",
                            &path,
                            "A header cannot be set and removed in the same update.",
                            Some(node_id),
                        );
                    }
                }
                sensitive_session |= !config.set_headers.is_empty();
            }
            _ => {}
        }
    }
    let mut required = BTreeMap::new();
    for (index, group) in scenario.groups.iter().enumerate() {
        let IdentityAssignment::Shared { identity_id } = &group.identity_assignment;
        let path = format!("/groups/{index}/identityAssignment/identityId");
        let Some(id) = identity_id else {
            if !check.identity_fields.is_empty() {
                check.errors.add("missingAssignedIdentity", &path, "This workflow consumes identity fields; each group must have an assigned identity.", None);
            }
            continue;
        };
        if let Some(identity) = identities.get(id.as_str()) {
            for field in &check.identity_fields {
                if let Some(reference) = identity.credentials.get(field) {
                    required.insert((id.clone(), field.clone()), reference.clone());
                } else {
                    check.errors.add("missingCredentialField", &path, "Assigned identity does not declare every credential field consumed by the workflow.", None);
                }
            }
        }
    }
    check.plan.required_secrets = required
        .into_iter()
        .map(|((identity_id, field), reference)| RequiredSecret {
            identity_id,
            field,
            reference,
        })
        .collect();
    for (node_id, extraction) in &mut check.plan.extractions {
        if let Some(last) = check.output_uses.get(node_id) {
            extraction.last_use_node_id = last.clone();
        }
    }
    if check.errors.0.is_empty() {
        Ok(check.plan)
    } else {
        Err(check.errors.0)
    }
}
