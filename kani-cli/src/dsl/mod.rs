// DSL parser: converts expression strings into kani_shared::ast::Expr trees.
//
// Called by:
//   - yaml codegen (src/codegen/mod.rs) to compile DSL fields in YAML blueprints
//   - `kani-cli dsl "<expr>"` for interactive inspection
//
// Implementation: use chumsky combinators to build the parser.
// Entry points are `parse` (used programmatically) and the `parser()` combinator
// (composable, useful for embedding in a larger grammar).
//
// Example DSL input:
//   self_ref().ptr("/attributes/title").str_val().fallback("Unknown")

use kani_shared::ast::Expr;

#[derive(Debug, thiserror::Error)]
pub enum DslError {
    #[error("parse error: {0}")]
    Parse(String),
}

/// Parse a DSL expression string into an `Expr` AST node.
pub fn parse(input: &str) -> Result<Expr, DslError> {
    // TODO: implement with chumsky
    //
    // let (expr, errs) = parser().parse_recovery(input);
    // if !errs.is_empty() {
    //     return Err(DslError::Parse(
    //         errs.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; ")
    //     ));
    // }
    // expr.ok_or_else(|| DslError::Parse("empty input".into()))
    let _ = input;
    Err(DslError::Parse("DSL parser not yet implemented".into()))
}