//! Emit Rust source for a BlueprintBuilder chain from a ValidatedEndpoint.

use super::expr::emit_expr;
use crate::yaml::model::{FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedHnp};
use crate::yaml::schema::{ResponseType, YamlOffsetType};
use kani_shared::ast::{Blueprint, BlueprintBuilder, Expr, OffsetType};

/// Build a sub-Blueprint struct from a ValidatedEndpoint (no request, no chaining steps).
/// Used to embed sub-blueprints inside `Expr::Fetch` nodes.
pub fn build_sub_blueprint(ep: &ValidatedEndpoint) -> Blueprint {
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
    builder.build()
}

fn make_fetch_expr(
    url_expr: &Expr,
    sub_ep: &ValidatedEndpoint,
    on_failure: &kani_shared::ast::OnFailurePolicy,
    endpoint_id: Option<String>,
) -> Expr {
    let sub_bp = build_sub_blueprint(sub_ep);
    let fetch = match sub_ep.response_type {
        ResponseType::Html => Expr::fetch_html(url_expr.clone(), sub_bp),
        ResponseType::Json => Expr::fetch_json(url_expr.clone(), sub_bp),
    };
    let fetch = if let Some(id) = endpoint_id {
        fetch.with_endpoint_id(id)
    } else {
        fetch
    };
    fetch.with_on_failure(on_failure.clone())
}

pub fn emit_blueprint_chain(
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    parent_endpoint_name: &str,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "let bp = BlueprintBuilder::new(\"{}\")",
        ep.container
    ));

    lines.push("    .request(req)".into());

    for b in &ep.bindings {
        lines.push(format!("    .bind(\"{}\", {})", b.name, emit_expr(&b.expr)));
    }

    for step in &ep.then_steps {
        if let Some(sub_ep) = ext.endpoint_by_name(&step.endpoint_name) {
            let endpoint_id = Some(format!("{parent_endpoint_name}/{}", step.merge_as));
            let fetch = make_fetch_expr(&step.url_expr, sub_ep, &step.on_failure, endpoint_id);
            lines.push(format!(
                "    .bind(\"{}\", {})",
                step.merge_as,
                emit_expr(&fetch)
            ));
        }
    }

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
            FieldSource::FnArg(_) => {}
        }
    }

    for step in &ep.for_each_steps {
        if let Some(sub_ep) = ext.endpoint_by_name(&step.endpoint_name) {
            let endpoint_id = Some(format!("{parent_endpoint_name}/{}", step.merge_as));
            let fetch = make_fetch_expr(&step.url_expr, sub_ep, &step.on_failure, endpoint_id);
            lines.push(format!(
                "    .field(\"{}\", {})",
                step.merge_as,
                emit_expr(&fetch)
            ));
        }
    }

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

    if let ValidatedHnp::Scalar(expr) = &ep.has_next_page {
        lines.push(format!(
            "    .scalar(\"has_next_page\", {})",
            emit_expr(expr)
        ));
    }

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
pub fn emit_blueprint_chain_no_request(
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    parent_endpoint_name: &str,
) -> String {
    let src = emit_blueprint_chain(ep, ext, parent_endpoint_name);
    src.lines()
        .filter(|l| !l.trim().starts_with(".request(req)"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a Blueprint from a ValidatedEndpoint at codegen time (no request attached),
/// serialize it to postcard bytes, and return a Rust `const` declaration.
pub fn emit_blueprint_bytes(
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    parent_endpoint_name: &str,
) -> String {
    let mut builder = BlueprintBuilder::new(&ep.container);

    for b in &ep.bindings {
        builder = builder.bind(&b.name, b.expr.clone());
    }

    for step in &ep.then_steps {
        if let Some(sub_ep) = ext.endpoint_by_name(&step.endpoint_name) {
            let endpoint_id = Some(format!("{parent_endpoint_name}/{}", step.merge_as));
            let fetch = make_fetch_expr(&step.url_expr, sub_ep, &step.on_failure, endpoint_id);
            builder = builder.bind(&step.merge_as, fetch);
        }
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

    for step in &ep.for_each_steps {
        if let Some(sub_ep) = ext.endpoint_by_name(&step.endpoint_name) {
            let endpoint_id = Some(format!("{parent_endpoint_name}/{}", step.merge_as));
            let fetch = make_fetch_expr(&step.url_expr, sub_ep, &step.on_failure, endpoint_id);
            builder = builder.field(&step.merge_as, fetch);
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
