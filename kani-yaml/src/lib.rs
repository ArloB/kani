pub mod dsl;
pub mod error;
pub mod yaml;

pub use error::YamlError;
pub use yaml::model::{ValidatedEndpoint, ValidatedExtension};
pub use yaml::schema::YamlExtension;

/// Parse and validate a YAML extension from source text.
pub fn parse_and_validate(
    text: &str,
    path: &std::path::Path,
) -> Result<ValidatedExtension, Vec<YamlError>> {
    let ext: YamlExtension = serde_yaml::from_str(text)
        .map_err(|e| vec![YamlError::Validation(format!("YAML parse error: {e}"))])?;

    yaml::validate::validate(&ext, text, path)
}

/// Build a `Blueprint` from a validated endpoint at runtime, with a request attached.
///
/// Used by the interpreted YAML tier, which always has a live `RequestDef` in hand.
/// Codegen (`kani-cli`) uses [`build_blueprint_core`] directly instead, since its
/// `emit_blueprint_bytes` path serialises the blueprint at build time with no request
/// attached (the request is built and attached separately in the generated Rust).
pub fn build_blueprint(
    ep: &yaml::model::ValidatedEndpoint,
    ext: &yaml::model::ValidatedExtension,
    endpoint_name: &str,
    req: kani_shared::ast::RequestDef,
) -> kani_shared::ast::Blueprint {
    build_blueprint_core(ep, ext, endpoint_name)
        .with_request(req)
        .build()
}

/// Build the `BlueprintBuilder` for a validated endpoint: bindings, `then`/`for_each`
/// sub-fetches, fields, scalars, `has_next_page`, and pagination — everything except the
/// request, which callers attach (or omit) as needed.
///
/// Shared by the interpreted tier ([`build_blueprint`]) and `kani-cli`'s codegen
/// (`emit_blueprint_bytes`), so the two consumption paths can't silently diverge.
pub fn build_blueprint_core(
    ep: &yaml::model::ValidatedEndpoint,
    ext: &yaml::model::ValidatedExtension,
    endpoint_name: &str,
) -> kani_shared::ast::BlueprintBuilder {
    use kani_shared::ast::{BlueprintBuilder, OffsetType};
    use yaml::model::{FieldSource, ValidatedHnp};
    use yaml::schema::YamlOffsetType;

    let mut builder = BlueprintBuilder::new(&ep.container);

    for b in &ep.bindings {
        builder = builder.bind(&b.name, b.expr.clone());
    }

    for step in &ep.then_steps {
        if let Some(sub_ep) = ext.endpoint_by_name(&step.endpoint_name) {
            let endpoint_id = Some(format!("{endpoint_name}/{}", step.merge_as));
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
            let endpoint_id = Some(format!("{endpoint_name}/{}", step.merge_as));
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

    builder
}

/// Build a sub-`Blueprint` from a `ValidatedEndpoint` (no request, no chaining steps).
/// Used to embed sub-blueprints inside `Expr::Fetch` nodes for `then`/`for_each` steps.
pub fn build_sub_blueprint(ep: &yaml::model::ValidatedEndpoint) -> kani_shared::ast::Blueprint {
    use kani_shared::ast::BlueprintBuilder;
    use yaml::model::FieldSource;

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

/// Build the `Expr::Fetch` node for a `then`/`for_each` step: constructs the sub-endpoint's
/// blueprint, wraps it in a `fetch_html`/`fetch_json` expr, and attaches endpoint-id + on-failure.
pub fn make_fetch_expr(
    url_expr: &kani_shared::ast::Expr,
    sub_ep: &yaml::model::ValidatedEndpoint,
    on_failure: &kani_shared::ast::OnFailurePolicy,
    endpoint_id: Option<String>,
) -> kani_shared::ast::Expr {
    use kani_shared::ast::Expr;
    use yaml::schema::ResponseType;

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

/// Build a URL by substituting `$var$` placeholders in `route` from `args`.
///
/// Composite-id sub-field placeholders (`$manga.hid$`) are looked up by the
/// dot-replaced key (`manga_hid`). The `base_url` is prepended verbatim.
pub fn build_url_with_args(
    base_url: &str,
    route: &str,
    args: &std::collections::HashMap<String, String>,
) -> String {
    let mut result = String::with_capacity(route.len());
    let bytes = route.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'.')
            {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'$' && end > start {
                let placeholder = &route[start..end];
                let key = placeholder.replace('.', "_");
                if let Some(val) = args.get(&key) {
                    result.push_str(val);
                } else {
                    result.push('$');
                    result.push_str(placeholder);
                    result.push('$');
                }
                i = end + 1;
            } else {
                result.push('$');
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    format!("{}{}", base_url.trim_end_matches('/'), result)
}

/// Resolve query parameters from an endpoint's query list and a runtime args map.
pub fn build_queries(
    entries: &[yaml::model::QueryEntry],
    args: &std::collections::HashMap<String, String>,
) -> Vec<(String, String)> {
    use yaml::model::QueryValue;
    entries
        .iter()
        .filter_map(|e| {
            let val = match &e.value {
                QueryValue::Static(s) => s.clone(),
                QueryValue::Arg(name) => args.get(name.as_str())?.clone(),
            };
            Some((e.key.clone(), val))
        })
        .collect()
}

/// Decode composite IDs referenced by `ep` and add the decoded sub-fields to `args`.
pub fn resolve_composite_ids(
    ep: &yaml::model::ValidatedEndpoint,
    args: &mut std::collections::HashMap<String, String>,
) {
    use kani_shared::ast::IdEncoding;
    use yaml::schema::YamlIdEncoding;

    for decode in &ep.composite_id_decodes {
        let raw_id = match args.get(&decode.fn_arg) {
            Some(v) => v.clone(),
            None => continue,
        };
        let encoding = match decode.encoding {
            YamlIdEncoding::Base64Url => IdEncoding::Base64Url,
            YamlIdEncoding::Base64 => IdEncoding::Base64,
            YamlIdEncoding::Passthrough => IdEncoding::Passthrough,
            YamlIdEncoding::Hex => IdEncoding::Hex,
        };
        let field_names: Vec<&str> = decode.fields.iter().map(|f| f.as_str()).collect();
        if let Ok(decoded) = kani_shared::encoding::decode_composite(
            &raw_id,
            &decode.delimiter,
            &encoding,
            &field_names,
        ) {
            for (field, value) in decoded {
                args.insert(format!("{}_{}", decode.role, field), value);
            }
        }
    }
}

/// Maps active filters onto query parameters, per an endpoint's `filter_mapping`
/// and `filter_format`.
///
/// This is the reference implementation for both execution paths. It previously
/// existed only inside `kani-cli`'s codegen, which meant an interpreted YAML
/// source rendered the filter panel, accepted a selection and then sent an
/// unfiltered request — the same `.yaml` behaved differently depending on
/// whether it had been compiled. See `docs/developer/backend-unification-plan.md`.
pub fn apply_filters(
    filter_mapping: &[(String, yaml::schema::FilterMappingEntry)],
    filter_format: Option<&yaml::schema::FilterFormatCfg>,
    filters: &[kani_shared::types::ActiveFilter],
) -> Vec<(String, String)> {
    use kani_shared::types::FilterState;
    use yaml::schema::{ArrayFormat, FilterMappingEntry};

    let bool_fmt = filter_format.map(|f| f.bool_format).unwrap_or_default();
    let omit_empty = filter_format.map(|f| f.omit_empty).unwrap_or(true);
    let array_fmt = filter_format.map(|f| f.multiselect).unwrap_or_default();
    let array_sep = filter_format
        .map(|f| f.array_separator.as_str())
        .unwrap_or(",");

    let mut out: Vec<(String, String)> = Vec::new();

    for f in filters {
        // `group:action` — the action half lets one filter group carry a value
        // in its name, e.g. `genre:include`.
        let (group, action) = f
            .filter_name
            .split_once(':')
            .unwrap_or((f.filter_name.as_str(), ""));

        let Some((_, entry)) = filter_mapping.iter().find(|(k, _)| k == group) else {
            continue;
        };

        match entry {
            FilterMappingEntry::Simple(param) => match &f.state {
                FilterState::Checkbox(true) => {
                    let v = if action.is_empty() {
                        bool_literal(bool_fmt, true).to_string()
                    } else {
                        action.to_string()
                    };
                    out.push((param.clone(), v));
                }
                FilterState::Checkbox(false) if !omit_empty => {
                    out.push((param.clone(), bool_literal(bool_fmt, false).to_string()));
                }
                FilterState::Multiselect(values) => match array_fmt {
                    ArrayFormat::Default | ArrayFormat::Repeated => {
                        for v in values {
                            out.push((param.clone(), v.clone()));
                        }
                    }
                    ArrayFormat::Bracket => {
                        for v in values {
                            out.push((format!("{param}[]"), v.clone()));
                        }
                    }
                    ArrayFormat::CommaSeparated => {
                        out.push((param.clone(), values.join(array_sep)));
                    }
                },
                FilterState::Selection { value, .. } => out.push((param.clone(), value.clone())),
                FilterState::TextInput(s) => out.push((param.clone(), s.clone())),
                _ => {}
            },
            FilterMappingEntry::SortPair {
                key_template,
                direction_param,
                ..
            } => {
                if let FilterState::Selection { value, .. } = &f.state
                    && let Some((key_part, dir)) = value.split_once(':')
                {
                    out.push((key_template.replace("{}", key_part), dir.to_string()));
                    if let Some(dir_param) = direction_param {
                        out.push((dir_param.clone(), dir.to_string()));
                    }
                }
            }
            FilterMappingEntry::TupleSplit {
                from_param,
                to_param,
                ..
            } => {
                if let FilterState::TextInput(s) = &f.state
                    && let Some((from, to)) = s.split_once(':')
                {
                    out.push((from_param.clone(), from.to_string()));
                    out.push((to_param.clone(), to.to_string()));
                }
            }
        }
    }

    out
}

fn bool_literal(fmt: yaml::schema::BoolFormat, value: bool) -> &'static str {
    use yaml::schema::BoolFormat;
    match (fmt, value) {
        (BoolFormat::TrueFalse, true) => "true",
        (BoolFormat::TrueFalse, false) => "false",
        (BoolFormat::OneZero, true) => "1",
        (BoolFormat::OneZero, false) => "0",
        (BoolFormat::YesNo, true) => "yes",
        (BoolFormat::YesNo, false) => "no",
    }
}
