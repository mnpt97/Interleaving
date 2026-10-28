use interl_core::{compile_document, parse_document, ScenarioFormat};

const SOURCE: &str = include_str!("../../../scenarios/examples/hello-api.json");

#[test]
fn compile_should_keep_its_snapshot_when_the_source_document_changes() {
    let mut source = parse_document(SOURCE, ScenarioFormat::Json).unwrap();
    let expected = source.clone();
    let plan = compile_document(&source).unwrap();

    source.name = "Changed scenario".into();
    source.workflows[0].name = "Changed workflow".into();
    source.workflows[0].nodes.clear();

    assert_eq!(
        (plan.document(), plan.workflows()[0].definition()),
        (&expected, &expected.workflows[0])
    );
}

#[test]
fn compiled_workflow_should_remain_usable_after_its_parent_program_is_dropped() {
    let source = parse_document(SOURCE, ScenarioFormat::Json).unwrap();
    let expected = source.workflows[0].clone();
    let plan = compile_document(&source).unwrap();
    let workflow = plan.workflows()[0].clone();

    drop(source);
    drop(plan);

    assert_eq!(workflow.definition(), &expected);
}

#[test]
fn compiled_workflow_should_share_the_programs_document_snapshot() {
    let source = parse_document(SOURCE, ScenarioFormat::Json).unwrap();
    let plan = compile_document(&source).unwrap();

    assert!(std::ptr::eq(
        plan.workflows()[0].definition(),
        &plan.document().workflows[0],
    ));
}

#[test]
fn expression_errors_should_support_standard_error_propagation() {
    fn compile_invalid_expression() -> Result<(), Box<dyn std::error::Error>> {
        interl_core::compile_expression("response.body.token + unsupported")?;
        Ok(())
    }

    let error = compile_invalid_expression().unwrap_err();

    assert!(error
        .downcast_ref::<interl_core::ExpressionError>()
        .is_some());
}
