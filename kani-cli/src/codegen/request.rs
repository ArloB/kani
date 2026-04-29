// Emit Rust source for the HTTP request block of an endpoint method.

use crate::yaml::model::{QueryEntry, QueryValue};
use crate::yaml::schema::FilterMappingEntry;

pub fn emit_request_block(
    route: &str,
    method: &str,
    headers: &[(String, String)],
    queries: &[QueryEntry],
    filter_mapping: &[(String, FilterMappingEntry)],
) -> String {
    let mut lines = Vec::new();

    let ctor = match method.to_uppercase().as_str() {
        "POST"   => "post",
        "PUT"    => "put",
        "DELETE" => "delete",
        _        => "get",
    };

    let url_expr = emit_route_format(route, "self.base_url");
    let mutability = if filter_mapping.is_empty() { "" } else { "mut " };
    lines.push(format!("let {mutability}req = HttpRequest::{ctor}({url_expr})"));

    for (k, v) in headers {
        lines.push(format!("    .header(\"{}\", \"{}\")", k, v));
    }

    for entry in queries {
        let val = match &entry.value {
            QueryValue::Static(s) => format!("\"{}\"", s),
            QueryValue::Arg(name)  => name.clone(),
        };
        lines.push(format!("    .query(\"{}\", {})", entry.key, val));
    }

    // Terminate the method chain
    let last = lines.pop().expect("at least one line");
    lines.push(format!("{last};"));

    if !filter_mapping.is_empty() {
        lines.push(String::new());
        lines.push("for f in filters {".into());
        lines.push("    let (group, action) = f.filter_name.split_once(':').unwrap_or((&f.filter_name, \"\"));".into());
        lines.push("    match group {".into());

        for (group, entry) in filter_mapping {
            match entry {
                FilterMappingEntry::Simple(param) => {
                    lines.push(format!("        \"{group}\" => match &f.state {{"));
                    lines.push(format!("            FilterState::Checkbox(c) if *c => {{"));
                    lines.push(format!("                req = req.query(\"{param}\", if action.is_empty() {{ \"true\" }} else {{ action }});"));
                    lines.push("            }".into());
                    lines.push(format!("            FilterState::Multiselect(values) => {{"));
                    lines.push(format!("                for v in values {{ req = req.query(\"{param}\", v.as_str()); }}"));
                    lines.push("            }".into());
                    lines.push(format!("            FilterState::Selection {{ value, .. }} => req = req.query(\"{param}\", value.as_str()),"));
                    lines.push(format!("            FilterState::TextInput(s) => req = req.query(\"{param}\", s.as_str()),"));
                    lines.push("            _ => {}".into());
                    lines.push("        },".into());
                }
                FilterMappingEntry::SortPair { key_template, direction_param, .. } => {
                    lines.push(format!("        \"{group}\" => match &f.state {{"));
                    lines.push("            FilterState::Selection { value, .. } => {".into());
                    lines.push("                if let Some((key_part, dir)) = value.split_once(':') {".into());
                    let key_fmt = key_template.replace("{}","{}");
                    lines.push(format!("                    req = req.query(&format!(\"{key_fmt}\", key_part), dir);"));
                    if let Some(dir_param) = direction_param {
                        lines.push(format!("                    req = req.query(\"{dir_param}\", dir);"));
                    }
                    lines.push("                }".into());
                    lines.push("            }".into());
                    lines.push("            _ => {}".into());
                    lines.push("        },".into());
                }
            }
        }

        lines.push("        _ => {}".into());
        lines.push("    }".into());
        lines.push("}".into());
    }

    lines.join("\n")
}

/// Convert a route with `$var$` placeholders into a `format!(...)` call.
pub fn emit_route_format(route: &str, base_url_expr: &str) -> String {
    let mut vars: Vec<String> = Vec::new();
    let mut fmt = String::new();
    let bytes = route.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => { fmt.push_str("{{"); i += 1; }
            b'}' => { fmt.push_str("}}"); i += 1; }
            b'$' => {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                    end += 1;
                }
                if end < bytes.len() && bytes[end] == b'$' && end > start {
                    vars.push(route[start..end].to_string());
                    fmt.push_str("{}");
                    i = end + 1;
                } else {
                    fmt.push('$');
                    i += 1;
                }
            }
            b => {
                fmt.push(b as char);
                i += 1;
            }
        }
    }

    let mut args = vec![format!("{base_url_expr}")];
    args.extend(vars);
    format!("format!(\"{{}}{fmt}\", {})", args.join(", "))
}
