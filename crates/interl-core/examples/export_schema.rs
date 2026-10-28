//! Export the structural file contract directly from the persisted Rust model.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schemars::schema_for!(interl_core::ScenarioDocument);
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
