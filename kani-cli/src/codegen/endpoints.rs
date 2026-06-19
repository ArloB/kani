//! Emit per-endpoint MangaExtension trait method bodies.

use super::blueprint::{emit_blueprint_bytes, emit_blueprint_chain};
use super::request::emit_request_block;
use crate::yaml::model::{
    FieldSource, ValidatedEndpoint, ValidatedExtension, ValidatedHnp, ValidatedPopular,
    ValidatedTotalPages,
};
use crate::yaml::schema::{EndpointVia, ResponseType, YamlIdEncoding};

/// Emits `let` bindings that decode each composite ID referenced by this
/// endpoint's route/queries (e.g. `$manga.hid$`) into local `&str`s
/// (`manga_hid`) before the request is built.
fn emit_composite_id_decode_prologue(ep: &ValidatedEndpoint) -> String {
    let mut out = String::new();
    for decode in &ep.composite_id_decodes {
        if decode.referenced_fields.is_empty() {
            continue;
        }
        let encoding_str = match decode.encoding {
            YamlIdEncoding::Base64Url => "kani_shared::ast::IdEncoding::Base64Url",
            YamlIdEncoding::Base64 => "kani_shared::ast::IdEncoding::Base64",
            YamlIdEncoding::Passthrough => "kani_shared::ast::IdEncoding::Passthrough",
            YamlIdEncoding::Hex => "kani_shared::ast::IdEncoding::Hex",
        };
        let field_names = decode
            .fields
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let local = format!("__{}_decoded", decode.role);
        out.push_str(&format!(
            "let {local} = kani_shared::encoding::decode_composite({arg}, \"{delim}\", &{encoding_str}, &[{field_names}]).map_err(kani_shared::ExtensionError::parse)?;\n",
            arg = decode.fn_arg,
            delim = decode.delimiter,
        ));
        for (idx, field) in decode.fields.iter().enumerate() {
            if !decode.referenced_fields.iter().any(|f| f == field) {
                continue;
            }
            out.push_str(&format!(
                "let {role}_{field} = {local}[{idx}].1.as_str();\n",
                role = decode.role,
            ));
        }
    }
    out
}

/// Returns `Some(code)` when `ep.via` is `BrowserPayload`, where `code`
/// emits a `capture_page_payload` call. The extraction step is a stub:
/// the browser *runtime* (Chromium lifecycle + payload injection) is
/// deferred and tracked in EXT_BROWSER_PAYLOAD_FEATURE_OVERVIEW.md.
fn try_emit_browser_fetch(ep: &ValidatedEndpoint) -> Option<String> {
    let EndpointVia::BrowserPayload = ep.via?;
    let page_url = ep.page_url.as_deref().unwrap_or("");
    let script_name = ep.script_name.as_deref().unwrap_or("");
    let const_name = format!(
        "SCRIPT_{}",
        script_name.to_uppercase().replace(['-', '.'], "_")
    );
    let rust_page_url = page_url
        .replace("$manga_id$", "{manga_id}")
        .replace("$chapter_id$", "{chapter_id}");
    let page_url_expr = if rust_page_url.contains('{') {
        format!("&format!(\"{rust_page_url}\")")
    } else {
        format!("\"{}\"", rust_page_url)
    };
    let timeout = ep.timeout_ms;
    Some(format!(
        "let _payload = kani_shared::host_abi::capture_page_payload({page_url_expr}, {const_name}, {timeout})?;\n\
         #[allow(unused_variables)]\n\
         let rows = unimplemented!(\"browser_payload extraction requires the browser runtime (deferred)\");"
    ))
}

pub fn emit_browser_script_statics(scripts: &std::collections::BTreeMap<String, String>) -> String {
    scripts
        .keys()
        .map(|name| {
            let const_name = format!("SCRIPT_{}", name.to_uppercase().replace(['-', '.'], "_"));
            format!("static {const_name}: &str = include_str!(\"scripts/{name}.js\");")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn emit_popular(
    popular: &ValidatedPopular,
    ext: &ValidatedExtension,
    embedded_bytes: bool,
) -> String {
    match popular {
        ValidatedPopular::Delegated {
            delegate_to,
            empty_without_filters,
        } => {
            let guard = if *empty_without_filters {
                "    if filters.is_empty() {\n        return Ok(MangaList { manga: vec![], has_next_page: false, total_pages: None });\n    }\n"
            } else {
                ""
            };
            let delegate_call = match delegate_to.as_str() {
                "search" => "self.search_manga(\"\", page, page_size, filters)",
                other => &format!("self.{other}(page, page_size, filters)"),
            };
            format!(
                "fn get_popular_manga(&self, page: i32, page_size: i32, filters: &[ActiveFilter]) -> ExtensionResult<MangaList> {{\n\
                 {guard}\
                 {delegate_call}\n\
                 }}"
            )
        }
        ValidatedPopular::Full(ep) => emit_manga_list_method(
            "get_popular_manga",
            "page: i32, page_size: i32, filters: &[ActiveFilter]",
            ep,
            ext,
            embedded_bytes,
            "popular",
        ),
    }
}

pub fn emit_search(
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    embedded_bytes: bool,
) -> String {
    emit_manga_list_method(
        "search_manga",
        "query: &str, page: i32, page_size: i32, filters: &[ActiveFilter]",
        ep,
        ext,
        embedded_bytes,
        "search",
    )
}

pub fn emit_manga_details(
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    embedded_bytes: bool,
) -> String {
    let decode_prologue = emit_composite_id_decode_prologue(ep);
    let row_assembly = emit_manga_info_assembly(ep);
    let bp_chain = emit_blueprint_chain(ep, ext, "manga_details");

    if let Some(browser_fetch) = try_emit_browser_fetch(ep) {
        return format!(
            "fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo> {{\n\
             {decode_prologue}\
             {bp_chain}\n\
             {browser_fetch}\n\
             let row = rows.rows_get(0).map_err(|_| kani_shared::ExtensionError::parse(\"no details row\".into()))?;\n\
             {row_assembly}\n\
             }}"
        );
    }

    let req_block = emit_request_block(
        &ep.route,
        &ep.method,
        &ep.headers,
        &ep.queries,
        &[],
        None,
        Some("manga_details"),
    );

    if embedded_bytes {
        let bp_bytes = emit_blueprint_bytes(ep, ext, "manga_details");
        let fetch = match ep.response_type {
            ResponseType::Html => {
                "let _doc = req.send_html()?;\nlet rows = extract_raw::html(Some(_doc.handle()), BP)?;"
            }
            ResponseType::Json => {
                "let _json = req.send_json_handle()?;\nlet rows = extract_raw::json(Some(_json.raw_handle()), BP)?;"
            }
        };
        format!(
            "fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo> {{\n\
             {decode_prologue}\
             {bp_bytes}\n\
             {req_block}\n\
             {fetch}\n\
             let row = rows.rows_get(0).map_err(|_| kani_shared::ExtensionError::parse(\"no details row\".into()))?;\n\
             {row_assembly}\n\
             }}"
        )
    } else {
        let extract_call = match ep.response_type {
            ResponseType::Json => "extract::json(None, &bp)?",
            ResponseType::Html => "extract::html(None, &bp)?",
        };
        format!(
            "fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo> {{\n\
             {decode_prologue}\
             {req_block}\n\
             {bp_chain}\n\
             let rows = {extract_call};\n\
             let row = rows.rows_get(0).map_err(|_| kani_shared::ExtensionError::parse(\"no details row\".into()))?;\n\
             {row_assembly}\n\
             }}"
        )
    }
}

pub fn emit_chapter_list(
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    embedded_bytes: bool,
) -> String {
    let decode_prologue = emit_composite_id_decode_prologue(ep);
    let req_block = emit_request_block(
        &ep.route,
        &ep.method,
        &ep.headers,
        &ep.queries,
        &[],
        None,
        Some("chapter_list"),
    );

    let hnp = emit_hnp_expr_static(&ep.has_next_page);
    let tp = emit_total_pages_static(&ep.total_pages);
    let row_assembly = emit_chapter_info_assembly(ep);

    if embedded_bytes {
        let bp_bytes = emit_blueprint_bytes(ep, ext, "chapter_list");
        let fetch = match ep.response_type {
            ResponseType::Html => {
                "let _doc = req.send_html()?;\nlet rows = extract_raw::html(Some(_doc.handle()), BP)?;"
            }
            ResponseType::Json => {
                "let _json = req.send_json_handle()?;\nlet rows = extract_raw::json(Some(_json.raw_handle()), BP)?;"
            }
        };
        format!(
            "fn get_chapter_list(&self, manga_id: &str, _page: i32, _page_size: Option<i32>, _sort: Option<String>) -> ExtensionResult<ChapterList> {{\n\
             {decode_prologue}\
             {bp_bytes}\n\
             {req_block}\n\
             {fetch}\n\
             let count = rows.rows_len();\n\
             let chapters = (0..count).filter_map(|i| {{\n\
                 let row = rows.rows_get(i).ok()?;\n\
                 {row_assembly}\n\
             }}).collect();\n\
             let has_next_page = {hnp};\n\
             let total_pages = {tp};\n\
             Ok(ChapterList {{ chapters, has_next_page, total_pages }})\n\
             }}"
        )
    } else {
        let bp_chain = emit_blueprint_chain(ep, ext, "chapter_list");
        let extract_call = match ep.response_type {
            ResponseType::Json => "extract::json(None, &bp)?",
            ResponseType::Html => "extract::html(None, &bp)?",
        };
        format!(
            "fn get_chapter_list(&self, manga_id: &str, _page: i32, _page_size: Option<i32>, _sort: Option<String>) -> ExtensionResult<ChapterList> {{\n\
             {decode_prologue}\
             {req_block}\n\
             {bp_chain}\n\
             let rows = {extract_call};\n\
             let count = rows.rows_len();\n\
             let chapters = (0..count).filter_map(|i| {{\n\
                 let row = rows.rows_get(i).ok()?;\n\
                 {row_assembly}\n\
             }}).collect();\n\
             let has_next_page = {hnp};\n\
             let total_pages = {tp};\n\
             Ok(ChapterList {{ chapters, has_next_page, total_pages }})\n\
             }}"
        )
    }
}

pub fn emit_pages(
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    embedded_bytes: bool,
) -> String {
    let decode_prologue = emit_composite_id_decode_prologue(ep);
    let req_block = emit_request_block(
        &ep.route,
        &ep.method,
        &ep.headers,
        &ep.queries,
        &[],
        None,
        Some("pages"),
    );

    let row_assembly = emit_pages_assembly(ep);

    let manga_param = if ep.composite_id_decodes.iter().any(|d| d.role == "manga") {
        "manga_id"
    } else {
        "_manga_id"
    };

    if embedded_bytes {
        let bp_bytes = emit_blueprint_bytes(ep, ext, "pages");
        let fetch = match ep.response_type {
            ResponseType::Html => {
                "let _doc = req.send_html()?;\nlet rows = extract_raw::html(Some(_doc.handle()), BP)?;"
            }
            ResponseType::Json => {
                "let _json = req.send_json_handle()?;\nlet rows = extract_raw::json(Some(_json.raw_handle()), BP)?;"
            }
        };
        format!(
            "fn get_pages(&self, {manga_param}: &str, chapter_id: &str) -> ExtensionResult<Chapter> {{\n\
             {decode_prologue}\
             {bp_bytes}\n\
             {req_block}\n\
             {fetch}\n\
             let count = rows.rows_len();\n\
             Ok(Chapter {{\n\
                 pages: (0..count).filter_map(|i| {{\n\
                     let row = rows.rows_get(i).ok()?;\n\
                     {row_assembly}\n\
                 }}).collect(),\n\
             }})\n\
             }}"
        )
    } else {
        let bp_chain = emit_blueprint_chain(ep, ext, "pages");
        let extract_call = match ep.response_type {
            ResponseType::Json => "extract::json(None, &bp)?",
            ResponseType::Html => "extract::html(None, &bp)?",
        };
        format!(
            "fn get_pages(&self, {manga_param}: &str, chapter_id: &str) -> ExtensionResult<Chapter> {{\n\
             {decode_prologue}\
             {req_block}\n\
             {bp_chain}\n\
             let rows = {extract_call};\n\
             let count = rows.rows_len();\n\
             Ok(Chapter {{\n\
                 pages: (0..count).filter_map(|i| {{\n\
                     let row = rows.rows_get(i).ok()?;\n\
                     {row_assembly}\n\
                 }}).collect(),\n\
             }})\n\
             }}"
        )
    }
}

// ── Result assembly helpers ──────────────────────────────────────────────────

fn emit_manga_list_method(
    method_name: &str,
    params: &str,
    ep: &ValidatedEndpoint,
    ext: &ValidatedExtension,
    embedded_bytes: bool,
    endpoint_id: &str,
) -> String {
    let req_block = emit_request_block(
        &ep.route,
        &ep.method,
        &ep.headers,
        &ep.queries,
        &ep.filter_mapping,
        ep.filter_format.as_ref(),
        Some(endpoint_id),
    );
    let row_assembly = emit_manga_list_item_assembly(ep);

    if embedded_bytes {
        let bp_bytes = emit_blueprint_bytes(ep, ext, endpoint_id);
        let (fetch, hnp_line, tp_line) = if ep.pagination.is_some() {
            (
                "let rows = extract_raw::paginated_html(page, page_size, req, BP)?;".to_string(),
                emit_hnp_scalar(&ep.has_next_page),
                emit_total_pages_scalar(&ep.total_pages),
            )
        } else {
            let fetch = match ep.response_type {
                ResponseType::Html => {
                    "let _doc = req.send_html()?;\nlet rows = extract_raw::html(Some(_doc.handle()), BP)?;"
                }
                ResponseType::Json => {
                    "let _json = req.send_json_handle()?;\nlet rows = extract_raw::json(Some(_json.raw_handle()), BP)?;"
                }
            };
            (
                fetch.to_string(),
                emit_hnp_expr_static(&ep.has_next_page),
                emit_total_pages_static(&ep.total_pages),
            )
        };
        format!(
            "fn {method_name}(&self, {params}) -> ExtensionResult<MangaList> {{\n\
             {bp_bytes}\n\
             {req_block}\n\
             {fetch}\n\
             let has_next_page = {hnp_line};\n\
             let total_pages = {tp_line};\n\
             let manga = rows.rows_iter().filter_map(|row| {{\n\
                 {row_assembly}\n\
             }}).collect();\n\
             Ok(MangaList {{ manga, has_next_page, total_pages }})\n\
             }}"
        )
    } else {
        let bp_chain = emit_blueprint_chain(ep, ext, endpoint_id);
        let (extract_call, hnp_line, tp_line): (String, String, String) = if ep.pagination.is_some()
        {
            (
                "extract::paginated_html(page, page_size, &bp)?".to_string(),
                emit_hnp_scalar(&ep.has_next_page),
                emit_total_pages_scalar(&ep.total_pages),
            )
        } else {
            let fn_name = match ep.response_type {
                ResponseType::Json => "extract::json(None, &bp)?",
                ResponseType::Html => "extract::html(None, &bp)?",
            };
            (
                fn_name.to_string(),
                emit_hnp_expr_static(&ep.has_next_page),
                emit_total_pages_static(&ep.total_pages),
            )
        };
        format!(
            "fn {method_name}(&self, {params}) -> ExtensionResult<MangaList> {{\n\
             {req_block}\n\
             {bp_chain}\n\
             let rows = {extract_call};\n\
             let has_next_page = {hnp_line};\n\
             let total_pages = {tp_line};\n\
             let manga = rows.rows_iter().filter_map(|row| {{\n\
                 {row_assembly}\n\
             }}).collect();\n\
             Ok(MangaList {{ manga, has_next_page, total_pages }})\n\
             }}"
        )
    }
}

/// Build `Some(MangaListItem { ... })` from the ValidatedFields.
fn emit_manga_list_item_assembly(ep: &ValidatedEndpoint) -> String {
    let mut fields = Vec::new();
    for f in &ep.fields {
        let accessor = match &f.source {
            FieldSource::FnArg(name) => format!("{name}.to_string()"),
            FieldSource::Blueprint(_) => manga_list_item_accessor(&f.name, f.optional),
        };
        fields.push(format!("            {}: {accessor}", f.name));
    }
    format!("Some(MangaListItem {{\n{}\n        }})", fields.join(",\n"))
}

fn manga_list_item_accessor(name: &str, optional: bool) -> String {
    match name {
        "id" | "title" => {
            if optional {
                format!("row.get_str(\"/{name}\")")
            } else {
                format!("row.require_str(\"/{name}\").ok()?")
            }
        }
        "cover_url" => "row.get_str(\"/cover_url\")".into(),
        other => {
            if optional {
                format!("row.get_str(\"/{other}\")")
            } else {
                format!("row.require_str(\"/{other}\").ok()?")
            }
        }
    }
}

/// Build `Ok(MangaInfo { ... })` from ValidatedFields.
fn emit_manga_info_assembly(ep: &ValidatedEndpoint) -> String {
    let mut fields = Vec::new();
    let mut declared: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for f in &ep.fields {
        let accessor = match &f.source {
            FieldSource::FnArg(name) => format!("{name}.to_string()"),
            FieldSource::Blueprint(_) => manga_info_field_accessor(&f.name, f.optional),
        };
        fields.push(format!("    {}: {accessor}", f.name));
        declared.insert(f.name.as_str());
    }
    for (name, default) in [
        ("cover_url", "None"),
        ("description", "None"),
        ("authors", "vec![]"),
        ("artists", "vec![]"),
        ("tags", "vec![]"),
    ] {
        if !declared.contains(name) {
            fields.push(format!("    {name}: {default}"));
        }
    }
    // status is special — always extracted from blueprint then converted to enum
    let has_status = ep.fields.iter().any(|f| f.name == "status");
    let status_conversion = if has_status {
        "let status = match row.get_str(\"/status\").as_deref() {\n\
         Some(\"ongoing\")   => MangaStatus::Ongoing,\n\
         Some(\"completed\") => MangaStatus::Completed,\n\
         Some(\"hiatus\")    => MangaStatus::Hiatus,\n\
         Some(\"cancelled\") => MangaStatus::Cancelled,\n\
         _                   => MangaStatus::Unknown,\n\
     };"
    } else {
        ""
    };

    format!(
        "{status_conversion}\n\
         Ok(MangaInfo {{\n{}\n}})",
        fields.join(",\n")
    )
}

fn manga_info_field_accessor(name: &str, optional: bool) -> String {
    match name {
        "id" | "title" => {
            if optional {
                format!("row.get_str(\"/{name}\")")
            } else {
                format!("row.require_str(\"/{name}\")?")
            }
        }
        "description" | "cover_url" => format!("row.get_str(\"/{name}\")"),
        "status" => "status".into(),
        "authors" | "artists" | "tags" => format!("row.get_array_of_strings(\"/{name}\")"),
        other => {
            if optional {
                format!("row.get_str(\"/{other}\")")
            } else {
                format!("row.require_str(\"/{other}\")?")
            }
        }
    }
}

/// Build `Some(ChapterInfo { ... })` from ValidatedFields.
fn emit_chapter_info_assembly(ep: &ValidatedEndpoint) -> String {
    let mut fields = Vec::new();
    for f in &ep.fields {
        let accessor = match &f.source {
            FieldSource::FnArg(name) => format!("{name}.to_string()"),
            FieldSource::Blueprint(_) => chapter_info_field_accessor(&f.name),
        };
        fields.push(format!("        {}: {accessor}", f.name));
    }
    format!("Some(ChapterInfo {{\n{}\n    }})", fields.join(",\n"))
}

fn chapter_info_field_accessor(name: &str) -> String {
    match name {
        "id" => "row.require_str(\"/id\").ok()?".into(),
        "number" => "row.get_f64(\"/number\").unwrap_or(0.0)".into(),
        "title" | "scanlator" => format!("row.get_str(\"/{name}\")"),
        "volume" => "row.get_i64(\"/volume\").map(|v| v as i32)".into(),
        "date_uploaded" => "row.get_i64(\"/date_uploaded\")".into(),
        "language" => "row.get_str(\"/language\").unwrap_or_else(|| \"en\".to_string())".into(),
        other => format!("row.get_str(\"/{other}\")"),
    }
}

/// Build `Some(Page { ... })` from ValidatedFields.
fn emit_pages_assembly(ep: &ValidatedEndpoint) -> String {
    let mut fields = Vec::new();
    for f in &ep.fields {
        let accessor = match &f.source {
            FieldSource::FnArg(name) => format!("{name}.to_string()"),
            FieldSource::Blueprint(_) => pages_field_accessor(&f.name),
        };
        fields.push(format!("        {}: {accessor}", f.name));
    }
    fields.push("        transform: None".to_string());
    format!("Some(Page {{\n{}\n    }})", fields.join(",\n"))
}

fn pages_field_accessor(name: &str) -> String {
    match name {
        "url" => "row.require_str(\"/url\").ok()?".into(),
        "index" => "row.get_i64(\"/index\").unwrap_or(i as i64) as i32".into(),
        other => format!("row.get_str(\"/{other}\").unwrap_or_default()"),
    }
}

// ── has_next_page helpers ────────────────────────────────────────────────────

/// For paginated endpoints, read `has_next_page` from the scalar output.
fn emit_hnp_scalar(hnp: &ValidatedHnp) -> String {
    match hnp {
        ValidatedHnp::Static(b) => b.to_string(),
        ValidatedHnp::Default | ValidatedHnp::Scalar(_) => {
            "rows.get_scalar_bool(\"has_next_page\")".into()
        }
    }
}

/// For non-paginated endpoints where hnp is a static or default bool.
fn emit_hnp_expr_static(hnp: &ValidatedHnp) -> String {
    match hnp {
        ValidatedHnp::Static(b) => b.to_string(),
        ValidatedHnp::Default => "false".into(),
        ValidatedHnp::Scalar(_) => "rows.get_scalar_bool(\"has_next_page\")".into(),
    }
}

/// For paginated endpoints, read `total_pages` from the scalar output.
fn emit_total_pages_scalar(tp: &ValidatedTotalPages) -> String {
    match tp {
        ValidatedTotalPages::Static(n) => format!("Some({n}u32)"),
        ValidatedTotalPages::None => "None".into(),
        ValidatedTotalPages::Scalar(_) => {
            "rows.get_scalar_i64(\"total_pages\").map(|n| n as u32)".into()
        }
    }
}

/// For non-paginated endpoints, emit a static or scalar total_pages expression.
fn emit_total_pages_static(tp: &ValidatedTotalPages) -> String {
    match tp {
        ValidatedTotalPages::Static(n) => format!("Some({n}u32)"),
        ValidatedTotalPages::None => "None".into(),
        ValidatedTotalPages::Scalar(_) => {
            "rows.get_scalar_i64(\"total_pages\").map(|n| n as u32)".into()
        }
    }
}
