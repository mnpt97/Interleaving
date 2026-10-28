//! Bound call-tree expansion and compute lifecycle reachability without executing it.

use super::problem;
use crate::{Diagnostic, Lifecycle, ProgramNode};
use std::collections::{BTreeMap, BTreeSet};

/// Cached cost of invoking a workflow once, including all of its child calls.
/// `Copy` is appropriate because these three counters are small integer values.
#[derive(Clone, Copy)]
struct CallSummary {
    depth: usize,
    invocations: usize,
    node_visits: usize,
}

/// Validate every call graph, then return the indices reachable from lifecycle roots.
/// All definitions participate in recursion/depth validation, including unused ones;
/// only initialize/iteration/cleanup roots contribute runtime reachability.
///
/// Missing references return safe diagnostics even if an earlier compiler pass
/// changes; they never turn into a production panic.
pub(super) fn reachable_workflows(
    lifecycle: &Lifecycle,
    flows: &[super::WorkflowAnalysis<'_>],
) -> Result<BTreeSet<usize>, Vec<Diagnostic>> {
    /// Summarize one workflow with depth-first search and memoization.
    /// `active` contains the current DFS path, so revisiting it detects recursion;
    /// `memo` reuses completed summaries without expanding repeated calls. Saturating
    /// addition prevents integer overflow before oversized call trees are rejected.
    fn visit(
        i: usize,
        flows: &[super::WorkflowAnalysis<'_>],
        indices: &BTreeMap<&str, usize>,
        active: &mut BTreeSet<usize>,
        memo: &mut BTreeMap<usize, CallSummary>,
    ) -> Result<CallSummary, Diagnostic> {
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
                let child = indices
                    .get(config.workflow_id.as_str())
                    .copied()
                    .ok_or_else(|| {
                        problem(
                            "missingWorkflow",
                            "/workflows",
                            "Called workflow does not exist.",
                        )
                    })?;
                let child = visit(child, flows, indices, active, memo)?;
                depth = depth.max(child.depth + 1);
                calls = calls.saturating_add(child.invocations);
                nodes = nodes.saturating_add(child.node_visits);
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
        let summary = CallSummary {
            depth,
            invocations: calls,
            node_visits: nodes,
        };
        memo.insert(i, summary);
        Ok(summary)
    }
    let indices: BTreeMap<_, _> = flows
        .iter()
        .enumerate()
        .map(|(index, flow)| (flow.definition.id.as_str(), index))
        .collect();
    let mut memo = BTreeMap::new();
    for i in 0..flows.len() {
        visit(i, flows, &indices, &mut BTreeSet::new(), &mut memo).map_err(|e| vec![e])?;
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
            indices
                .get(b.workflow_id.as_str())
                .copied()
                .ok_or_else(|| {
                    vec![problem(
                        "missingWorkflow",
                        "/groups",
                        "Referenced workflow does not exist.",
                    )]
                })?,
        );
    }
    if roots.iter().map(|i| memo[i].invocations).sum::<usize>() > 128
        || roots.iter().map(|i| memo[i].node_visits).sum::<usize>() > 1024
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
                        indices
                            .get(config.workflow_id.as_str())
                            .copied()
                            .ok_or_else(|| {
                                vec![problem(
                                    "missingWorkflow",
                                    "/workflows",
                                    "Called workflow does not exist.",
                                )]
                            })?,
                    );
                }
            }
        }
    }
    Ok(reachable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_document, AuthPlan, ScenarioFormat, WorkflowBinding};

    #[test]
    fn reachable_workflows_should_report_a_missing_callee() {
        let mut document = parse_document(
            include_str!("../../../../scenarios/examples/hello-api.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        document.workflows[0].nodes[1] = ProgramNode::Call {
            name: None,
            id: "call".into(),
            config: WorkflowBinding {
                workflow_id: "missing".into(),
                inputs: BTreeMap::new(),
            },
        };
        let analysis = super::super::WorkflowAnalysis {
            definition: &document.workflows[0],
            order: vec![0, 1, 2],
            auth: AuthPlan::default(),
            predicates: BTreeMap::new(),
            last_uses: BTreeMap::new(),
        };

        let errors = reachable_workflows(&document.groups[0].lifecycle, &[analysis]).unwrap_err();

        assert_eq!(errors[0].code, "missingWorkflow");
    }

    #[test]
    fn reachable_workflows_should_report_a_missing_lifecycle_root() {
        let mut document = parse_document(
            include_str!("../../../../scenarios/examples/hello-api.json"),
            ScenarioFormat::Json,
        )
        .unwrap();
        document.groups[0].lifecycle.iteration.workflow_id = "missing".into();
        let analysis = super::super::WorkflowAnalysis {
            definition: &document.workflows[0],
            order: vec![0, 1, 2],
            auth: AuthPlan::default(),
            predicates: BTreeMap::new(),
            last_uses: BTreeMap::new(),
        };

        let errors = reachable_workflows(&document.groups[0].lifecycle, &[analysis]).unwrap_err();

        assert_eq!(errors[0].code, "missingWorkflow");
    }
}
