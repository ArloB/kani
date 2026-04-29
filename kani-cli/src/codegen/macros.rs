// Emit `filter_list!` and `preference_list!` macro invocations from schema entries.

use crate::yaml::schema::{
    FilterDefault, FilterEntry, FilterKind, FilterOption, FilterSemantic,
    PreferenceEntry, PreferenceKind, PrefOption,
};

pub fn emit_filter_list(filters: &[FilterEntry]) -> String {
    if filters.is_empty() {
        return "Ok(filter_list!{})".to_string();
    }
    let mut entries = Vec::new();
    for f in filters {
        entries.push(emit_filter_entry(f));
    }
    format!("Ok(filter_list!{{\n{}\n}})", entries.join(";\n"))
}

fn emit_filter_entry(f: &FilterEntry) -> String {
    let id = escape_str(&f.id);
    let name = escape_str(&f.name);
    let semantic_suffix = f.semantic.as_ref().map(|s| {
        let tag = match s {
            FilterSemantic::Author => "kani_shared::wit_types::FilterSemantic::Author",
            FilterSemantic::Artist => "kani_shared::wit_types::FilterSemantic::Artist",
            FilterSemantic::Tag    => "kani_shared::wit_types::FilterSemantic::Tag",
        };
        format!(", semantic: {tag}")
    }).unwrap_or_default();

    match f.kind {
        FilterKind::Checkbox => {
            let def = match &f.default {
                Some(FilterDefault::Bool(b)) => format!(", default: {b}"),
                _ => String::new(),
            };
            format!("    \"{id}\", \"{name}\", Checkbox{def}{semantic_suffix}")
        }

        FilterKind::Select | FilterKind::Sort => {
            let kind = if f.kind == FilterKind::Sort { "Sort" } else { "Select" };
            let opts = emit_opts_with_values(&f.id, &f.options);
            let def = match &f.default {
                Some(FilterDefault::Option { name: n, value: v }) =>
                    format!(", default: (\"{}\", \"{}\")", escape_str(n), escape_str(v)),
                Some(FilterDefault::Text(s)) =>
                    format!(", default: (\"{}\")", escape_str(s)),
                _ => String::new(),
            };
            format!("    \"{id}\", \"{name}\", {kind}, [{opts}]{def}{semantic_suffix}")
        }

        FilterKind::Multiselect => {
            let opts = emit_opts_multiselect(&f.id, &f.options);
            format!("    \"{id}\", \"{name}\", Multiselect, [{opts}]{semantic_suffix}")
        }

        FilterKind::TextInput => {
            let def = match &f.default {
                Some(FilterDefault::Text(s)) => format!(", default: \"{}\"", escape_str(s)),
                _ => String::new(),
            };
            format!("    \"{id}\", \"{name}\", TextInput{def}{semantic_suffix}")
        }
    }
}

/// Emit Select/Sort options as `("Name", "value")` tuples (always tuple form).
fn emit_opts_with_values(_filter_id: &str, opts: &[FilterOption]) -> String {
    opts.iter().map(|o| {
        format!("(\"{}\", \"{}\")", escape_str(&o.name), escape_str(&o.value))
    }).collect::<Vec<_>>().join(", ")
}

/// Emit Multiselect options: bare string if name==value, else tuple.
fn emit_opts_multiselect(_filter_id: &str, opts: &[FilterOption]) -> String {
    opts.iter().map(|o| {
        if o.name == o.value {
            format!("\"{}\"", escape_str(&o.name))
        } else {
            format!("(\"{}\", \"{}\")", escape_str(&o.name), escape_str(&o.value))
        }
    }).collect::<Vec<_>>().join(", ")
}

pub fn emit_preference_list(prefs: &[PreferenceEntry]) -> String {
    if prefs.is_empty() {
        return "Ok(vec![])".to_string();
    }
    let mut entries = Vec::new();
    for p in prefs {
        entries.push(emit_pref_entry(p));
    }
    format!("Ok(preference_list![\n{}\n])", entries.join(";\n"))
}

fn emit_pref_entry(p: &PreferenceEntry) -> String {
    let key = escape_str(&p.key);
    let label = escape_str(&p.label);
    let desc_suffix = p.description.as_ref().map(|d| {
        format!(", description: \"{}\"", escape_str(d))
    }).unwrap_or_default();

    match p.kind {
        PreferenceKind::Toggle => {
            let def = p.default.as_str() == "true";
            format!("    \"{key}\", \"{label}\", Toggle, default: {def}{desc_suffix}")
        }

        PreferenceKind::Select => {
            let opts = emit_pref_opts(&p.options);
            let def = escape_str(&p.default);
            format!("    \"{key}\", \"{label}\", Select, [{opts}], default: \"{def}\"{desc_suffix}")
        }

        PreferenceKind::Text => {
            let def = escape_str(&p.default);
            let secret_suffix = if p.secret { ", secret: true" } else { "" };
            format!("    \"{key}\", \"{label}\", Text, default: \"{def}\"{desc_suffix}{secret_suffix}")
        }

        PreferenceKind::MultiValueList => {
            format!("    \"{key}\", \"{label}\", MultiValueList{desc_suffix}")
        }
    }
}

fn emit_pref_opts(opts: &[PrefOption]) -> String {
    opts.iter().map(|o| {
        format!("(\"{}\", \"{}\")", escape_str(&o.name), escape_str(&o.value))
    }).collect::<Vec<_>>().join(", ")
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"', "\\\"")
}
