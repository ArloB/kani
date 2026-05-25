use crate::dsl;
use crate::error::CliError;

pub fn run(expression: &str) -> Result<(), CliError> {
    let expr = dsl::parse(expression).map_err(|e| CliError::Other(e.to_string()))?;
    println!("{expr:#?}");
    Ok(())
}