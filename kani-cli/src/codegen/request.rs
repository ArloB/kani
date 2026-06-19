//! Emit Rust source for the HTTP request block of an endpoint method.

use crate::yaml::model::{QueryEntry, QueryValue};
use crate::yaml::schema::{ArrayFormat, BoolFormat, FilterFormatCfg, FilterMappingEntry};

pub fn emit_request_block(
    route: &str,
    method: &str,
    headers: &[(String, String)],
    queries: &[QueryEntry],
    filter_mapping: &[(String, FilterMappingEntry)],
    filter_format: Option<&FilterFormatCfg>,
    endpoint_id: Option<&str>,
) -> String {
    let mut lines = Vec::new();

    let ctor = match method.to_uppercase().as_str() {
        "POST" => "post",
        "PUT" => "put",
        "DELETE" => "delete",
        _ => "get",
    };

    let url_expr = emit_route_format(route, "self.base_url");
    let mutability = if filter_mapping.is_empty() {
        ""
    } else {
        "mut "
    };
    lines.push(format!(
        "let {mutability}req = HttpRequest::{ctor}({url_expr})"
    ));

    if let Some(id) = endpoint_id {
        lines.push(format!("    .endpoint_id(\"{id}\")"));
    }

    for (k, v) in headers {
        lines.push(format!("    .header(\"{}\", \"{}\")", k, v));
    }

    for entry in queries {
        let val = match &entry.value {
            QueryValue::Static(s) => format!("\"{}\"", s),
            QueryValue::Arg(name) => name.replace('.', "_"),
        };
        lines.push(format!("    .query(\"{}\", {})", entry.key, val));
    }

    let last = lines.pop().expect("at least one line");
    lines.push(format!("{last};"));

    if !filter_mapping.is_empty() {
        lines.push(String::new());
        lines.push("for f in filters {".into());
        lines.push("    let (group, action) = f.filter_name.split_once(':').unwrap_or((&f.filter_name, \"\"));".into());
        lines.push("    match group {".into());

        let bool_fmt = filter_format.map(|f| f.bool_format).unwrap_or_default();
        let omit_empty = filter_format.map(|f| f.omit_empty).unwrap_or(true);
        let array_fmt = filter_format.map(|f| f.multiselect).unwrap_or_default();
        let array_sep = filter_format
            .map(|f| f.array_separator.as_str())
            .unwrap_or(",");

        for (group, entry) in filter_mapping {
            match entry {
                FilterMappingEntry::Simple(param) => {
                    let true_lit = bool_literal(bool_fmt, true);
                    lines.push(format!("        \"{group}\" => match &f.state {{"));
                    lines.push("            FilterState::Checkbox(c) if *c => {".to_string());
                    lines.push(format!("                req = req.query(\"{param}\", if action.is_empty() {{ \"{true_lit}\" }} else {{ action }});"));
                    lines.push("            }".into());
                    if !omit_empty {
                        let false_lit = bool_literal(bool_fmt, false);
                        lines.push("            FilterState::Checkbox(_) => {".to_string());
                        lines.push(format!(
                            "                req = req.query(\"{param}\", \"{false_lit}\");"
                        ));
                        lines.push("            }".into());
                    }
                    match array_fmt {
                        ArrayFormat::Default | ArrayFormat::Repeated => {
                            lines.push(
                                "            FilterState::Multiselect(values) => {".to_string(),
                            );
                            lines.push(format!("                for v in values {{ req = req.query(\"{param}\", v.as_str()); }}"));
                            lines.push("            }".into());
                        }
                        ArrayFormat::Bracket => {
                            lines.push(
                                "            FilterState::Multiselect(values) => {".to_string(),
                            );
                            lines.push(format!("                for v in values {{ req = req.query(\"{param}[]\", v.as_str()); }}"));
                            lines.push("            }".into());
                        }
                        ArrayFormat::CommaSeparated => {
                            lines.push(
                                "            FilterState::Multiselect(values) => {".to_string(),
                            );
                            lines.push(format!("                req = req.query(\"{param}\", values.join(\"{array_sep}\").as_str());"));
                            lines.push("            }".into());
                        }
                    }
                    lines.push(format!("            FilterState::Selection {{ value, .. }} => req = req.query(\"{param}\", value.as_str()),"));
                    lines.push(format!("            FilterState::TextInput(s) => req = req.query(\"{param}\", s.as_str()),"));
                    lines.push("            _ => {}".into());
                    lines.push("        },".into());
                }
                FilterMappingEntry::SortPair {
                    key_template,
                    direction_param,
                    ..
                } => {
                    lines.push(format!("        \"{group}\" => match &f.state {{"));
                    lines.push("            FilterState::Selection { value, .. } => {".into());
                    lines.push(
                        "                if let Some((key_part, dir)) = value.split_once(':') {"
                            .into(),
                    );
                    lines.push(format!("                    req = req.query(&format!(\"{key_template}\", key_part), dir);"));
                    if let Some(dir_param) = direction_param {
                        lines.push(format!(
                            "                    req = req.query(\"{dir_param}\", dir);"
                        ));
                    }
                    lines.push("                }".into());
                    lines.push("            }".into());
                    lines.push("            _ => {}".into());
                    lines.push("        },".into());
                }
                FilterMappingEntry::TupleSplit {
                    from_param,
                    to_param,
                    ..
                } => {
                    lines.push(format!("        \"{group}\" => match &f.state {{"));
                    lines.push("            FilterState::TextInput(s) => {".into());
                    lines.push(
                        "                if let Some((from, to)) = s.split_once(':') {".into(),
                    );
                    lines.push(format!(
                        "                    req = req.query(\"{from_param}\", from);"
                    ));
                    lines.push(format!(
                        "                    req = req.query(\"{to_param}\", to);"
                    ));
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

fn bool_literal(fmt: BoolFormat, value: bool) -> &'static str {
    match (fmt, value) {
        (BoolFormat::TrueFalse, true) => "true",
        (BoolFormat::TrueFalse, false) => "false",
        (BoolFormat::OneZero, true) => "1",
        (BoolFormat::OneZero, false) => "0",
        (BoolFormat::YesNo, true) => "yes",
        (BoolFormat::YesNo, false) => "no",
    }
}

/// Convert a route with `$var$` placeholders into a `format!(...)` call.
/// A placeholder may contain a single `.` (e.g. `$manga.hid$`) to reference a
/// composite-id subfield; these are sanitized to `manga_hid` since `.` is not
/// a valid Rust identifier character (the matching local is emitted by the
/// decode prologue).
pub fn emit_route_format(route: &str, base_url_expr: &str) -> String {
    let mut vars: Vec<String> = Vec::new();
    let mut fmt = String::new();
    let bytes = route.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                fmt.push_str("{{");
                i += 1;
            }
            b'}' => {
                fmt.push_str("}}");
                i += 1;
            }
            b'$' => {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric()
                        || bytes[end] == b'_'
                        || bytes[end] == b'.')
                {
                    end += 1;
                }
                if end < bytes.len() && bytes[end] == b'$' && end > start {
                    vars.push(route[start..end].replace('.', "_"));
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
