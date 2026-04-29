use chumsky::Parser;
use kani_shared::ast::Expr;
use crate::dsl::parser;
use crate::error::{report_custom_error, report_errors, CliError};

pub fn run(expression: &str) -> Result<(), CliError> {
    let result = parser().parse(expression);

    if result.has_errors() {
        let errs: Vec<_> = result.errors().cloned().collect();
        report_errors("<stdin>", expression, errs);
        return Err(CliError::Other("DSL parsing failed (see above)".to_string()));
    }

    let parse_ast = result.into_result().unwrap();

    let ast_raw: Result<Expr, Vec<CliError>> = parse_ast.clone().try_into();

    if let Err(item) = ast_raw {
        for error in item {
            match error {
                CliError::DslConversion { message, span } => {
                    report_custom_error("<stdin>", expression, &message, span);
                }
                e => println!("err when validating: {}", e),
            }
        }

        return Err(CliError::Other("Validation Error (see above)".to_string()));
    }
    
    let expr: Expr = ast_raw.unwrap();

    println!("{expr:#?}");
    Ok(())
}