// Emit Rust source for a BlueprintBuilder chain from a ValidatedEndpoint.

use super::expr::emit_expr;
use crate::yaml::model::{FieldSource, ValidatedEndpoint, ValidatedHnp};
use crate::yaml::schema::YamlOffsetType;
use kani_shared::ast::{BlueprintBuilder, OffsetType};

pub fn emit_blueprint_chain(ep: &ValidatedEndpoint) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "let bp = BlueprintBuilder::new(\"{}\")",
        ep.container
    ));

    // Attach request (replaced by `let mut req` from emit_request_block and then `.request(req)`)
    lines.push("    .request(req)".into());

    // Bindings
    for b in &ep.bindings {
        lines.push(format!("    .bind(\"{}\", {})", b.name, emit_expr(&b.expr)));
    }

    // Fields
    for f in &ep.fields {
        match &f.source {
            FieldSource::Blueprint(expr) => {
                let method = if f.optional { "field_opt" } else { "field" };
                lines.push(format!(
                    "    .{method}(\"{}\", {})",
                    f.name,
                    emit_expr(expr)
                ));
            }
            FieldSource::FnArg(_) => { /* not added to blueprint */ }
        }
    }

    // Scalars
    for s in &ep.scalars {
        match &s.source {
            FieldSource::Blueprint(expr) => {
                let method = if s.optional { "scalar_opt" } else { "scalar" };
                lines.push(format!(
                    "    .{method}(\"{}\", {})",
                    s.name,
                    emit_expr(expr)
                ));
            }
            FieldSource::FnArg(_) => {}
        }
    }

    // has_next_page scalar (if DSL-driven)
    if let ValidatedHnp::Scalar(expr) = &ep.has_next_page {
        lines.push(format!(
            "    .scalar(\"has_next_page\", {})",
            emit_expr(expr)
        ));
    }

    // Pagination
    if let Some(pag) = &ep.pagination {
        let offset_type = match pag.offset_type {
            YamlOffsetType::Item => "OffsetType::ItemOffset".into(),
            YamlOffsetType::Page => {
                format!("OffsetType::PageNumber {{ start: {} }}", pag.page_start)
            }
        };
        lines.push(format!(
            "    .paginated({}, \"{}\", {})",
            pag.native_page_size, pag.offset_param, offset_type
        ));
    }

    lines.push("    .build();".into());
    lines.join("\n")
}

/// Emit only the `BlueprintBuilder::new(...)...build()` without the `.request()` line.
/// Used for endpoints where the request is not needed (e.g. doc handle passed externally).
#[allow(dead_code)]
pub fn emit_blueprint_chain_no_request(ep: &ValidatedEndpoint) -> String {
    let src = emit_blueprint_chain(ep);
    // Remove the `.request(req)` line
    src.lines()
        .filter(|l| !l.trim().starts_with(".request(req)"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a Blueprint from a ValidatedEndpoint at codegen time (no request attached),
/// serialize it to postcard bytes, and return a Rust `const` declaration.
///
/// The generated constant looks like:
/// ```ignore
/// const BP: &[u8] = &[0x01, 0x02, ...];
/// ```
pub fn emit_blueprint_bytes(ep: &ValidatedEndpoint) -> String {
    let mut builder = BlueprintBuilder::new(&ep.container);

    for b in &ep.bindings {
        builder = builder.bind(&b.name, b.expr.clone());
    }

    for f in &ep.fields {
        if let FieldSource::Blueprint(expr) = &f.source {
            if f.optional {
                builder = builder.field_opt(&f.name, expr.clone());
            } else {
                builder = builder.field(&f.name, expr.clone());
            }
        }
    }

    for s in &ep.scalars {
        if let FieldSource::Blueprint(expr) = &s.source {
            if s.optional {
                builder = builder.scalar_opt(&s.name, expr.clone());
            } else {
                builder = builder.scalar(&s.name, expr.clone());
            }
        }
    }

    if let ValidatedHnp::Scalar(expr) = &ep.has_next_page {
        builder = builder.scalar("has_next_page", expr.clone());
    }

    if let Some(pag) = &ep.pagination {
        let offset_type = match pag.offset_type {
            YamlOffsetType::Item => OffsetType::ItemOffset,
            YamlOffsetType::Page => OffsetType::PageNumber {
                start: pag.page_start,
            },
        };
        builder = builder.paginated(pag.native_page_size, &pag.offset_param, offset_type);
    }

    let bytes = builder.build().to_bytes();
    let hex: Vec<String> = bytes.iter().map(|b| format!("0x{b:02x}")).collect();
    format!("const BP: &[u8] = &[{}];", hex.join(", "))
}
