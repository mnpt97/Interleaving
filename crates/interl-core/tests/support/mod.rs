use interl_core::{AuthPlan, Program, WorkflowNode};
pub fn steps(program: &Program) -> Vec<WorkflowNode> {
    let workflow = &program.workflows()[0];
    workflow
        .order()
        .iter()
        .map(|&i| workflow.definition().nodes[i].clone())
        .collect()
}
pub fn signature(program: &Program) -> (Vec<WorkflowNode>, AuthPlan) {
    (steps(program), program.workflows()[0].auth().clone())
}
