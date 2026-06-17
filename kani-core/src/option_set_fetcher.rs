use crate::error::{Error, Result};
use crate::http::SmartClient;
use kani_shared::FilterFetchDef;

/// Fetches and parses a `FilterFetchDef`, returning `(name, value)` pairs.
/// Options with `nsfw=true` are excluded.
pub async fn fetch_option_set(
    client: &SmartClient,
    def: &FilterFetchDef,
) -> Result<Vec<(String, String)>> {
    let response = client.get(&def.route).await?.text().await?;

    match def.response_type.as_str() {
        "json" => parse_json_options(&response, def),
        _ => parse_html_options(&response, def),
    }
}

fn parse_html_options(html: &str, def: &FilterFetchDef) -> Result<Vec<(String, String)>> {
    use scraper::{Html, Selector};

    let doc = Html::parse_document(html);
    let container_sel = def.container.as_deref().unwrap_or(":root");
    let sel = Selector::parse(container_sel)
        .map_err(|_| Error::Internal(format!("invalid CSS selector: {container_sel}")))?;

    let name_expr = def.fields.get("name").map(String::as_str).unwrap_or("*");
    let value_expr = def.fields.get("value").map(String::as_str).unwrap_or("*");
    let nsfw_expr = def
        .nsfw_field
        .as_ref()
        .and_then(|f| def.fields.get(f.as_str()));

    let mut options = Vec::new();
    for element in doc.select(&sel) {
        let name = extract_html_value(element, name_expr);
        let value = extract_html_value(element, value_expr);
        if name.is_empty() || value.is_empty() {
            continue;
        }
        if let Some(nsfw_sel) = nsfw_expr {
            let nsfw_str = extract_html_value(element, nsfw_sel);
            if nsfw_str == "true" || nsfw_str == "1" {
                continue;
            }
        }
        options.push((name, value));
    }
    Ok(options)
}

/// Extracts a value from an HTML element. Format: `"sel"` (text), `"sel|attr"` (attribute),
/// `"self"` (element text), `"self|attr"` (element attribute).
fn extract_html_value(element: scraper::ElementRef<'_>, expr: &str) -> String {
    use scraper::Selector;

    if let Some((sel_part, attr)) = expr.split_once('|') {
        let target = if sel_part.is_empty() || sel_part == "self" {
            Some(element)
        } else {
            Selector::parse(sel_part)
                .ok()
                .and_then(|s| element.select(&s).next())
        };
        target
            .and_then(|el| el.attr(attr).map(str::to_owned))
            .unwrap_or_default()
    } else if expr.is_empty() || expr == "self" {
        element.text().collect::<String>().trim().to_owned()
    } else {
        match Selector::parse(expr) {
            Ok(s) => element
                .select(&s)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_owned())
                .unwrap_or_default(),
            Err(_) => String::new(),
        }
    }
}

fn parse_json_options(json: &str, def: &FilterFetchDef) -> Result<Vec<(String, String)>> {
    let root: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| Error::Internal(format!("JSON parse error: {e}")))?;

    let items = if let Some(container) = &def.container {
        root.pointer(container)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    } else {
        root.as_array().cloned().unwrap_or_default()
    };

    let name_ptr = def
        .fields
        .get("name")
        .map(String::as_str)
        .unwrap_or("/name");
    let value_ptr = def
        .fields
        .get("value")
        .map(String::as_str)
        .unwrap_or("/value");
    let nsfw_ptr = def
        .nsfw_field
        .as_ref()
        .and_then(|f| def.fields.get(f.as_str()));

    let mut options = Vec::new();
    for item in &items {
        let name = item
            .pointer(name_ptr)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let value = item
            .pointer(value_ptr)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        if name.is_empty() || value.is_empty() {
            continue;
        }
        if let Some(np) = nsfw_ptr {
            let nsfw = item.pointer(np).and_then(|v| v.as_bool()).unwrap_or(false);
            if nsfw {
                continue;
            }
        }
        options.push((name, value));
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn make_def(
        response_type: &str,
        container: Option<&str>,
        fields: Vec<(&str, &str)>,
        nsfw_field: Option<&str>,
    ) -> FilterFetchDef {
        FilterFetchDef {
            filter_id: "test".to_string(),
            option_set_name: "test_set".to_string(),
            route: "https://example.com".to_string(),
            response_type: response_type.to_string(),
            container: container.map(str::to_owned),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            nsfw_field: nsfw_field.map(str::to_owned),
            cache_key: None,
            cache_ttl: 300,
        }
    }

    #[test]
    fn html_text_extraction() {
        let html = r#"<ul><li><span class="name">Action</span><a data-id="action">action</a></li><li><span class="name">Comedy</span><a data-id="comedy">comedy</a></li></ul>"#;
        let def = make_def(
            "html",
            Some("li"),
            vec![("name", "span.name"), ("value", "a|data-id")],
            None,
        );
        let opts = parse_html_options(html, &def).unwrap();
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0], ("Action".to_string(), "action".to_string()));
        assert_eq!(opts[1], ("Comedy".to_string(), "comedy".to_string()));
    }

    #[test]
    fn html_nsfw_filtering() {
        let html_full = r#"<ul>
            <li><span class="name">Action</span><span class="nsfw">false</span><a data-id="1">1</a></li>
            <li><span class="name">Adult</span><span class="nsfw">true</span><a data-id="2">2</a></li>
        </ul>"#;
        let def = make_def(
            "html",
            Some("li"),
            vec![
                ("name", "span.name"),
                ("value", "a|data-id"),
                ("nsfw", "span.nsfw"),
            ],
            Some("nsfw"),
        );
        let opts = parse_html_options(html_full, &def).unwrap();
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].0, "Action");
    }

    #[test]
    fn json_extraction() {
        let json = r#"[{"name":"Action","id":"action"},{"name":"Comedy","id":"comedy"}]"#;
        let def = make_def(
            "json",
            None,
            vec![("name", "/name"), ("value", "/id")],
            None,
        );
        let opts = parse_json_options(json, &def).unwrap();
        assert_eq!(opts.len(), 2);
        assert_eq!(opts[0], ("Action".to_string(), "action".to_string()));
    }

    #[test]
    fn json_container_path() {
        let json = r#"{"tags":[{"name":"Action","id":"action"}]}"#;
        let def = make_def(
            "json",
            Some("/tags"),
            vec![("name", "/name"), ("value", "/id")],
            None,
        );
        let opts = parse_json_options(json, &def).unwrap();
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].0, "Action");
    }

    #[test]
    fn json_nsfw_filtered() {
        let json =
            r#"[{"name":"Safe","id":"s","adult":false},{"name":"NSFW","id":"n","adult":true}]"#;
        let def = make_def(
            "json",
            None,
            vec![("name", "/name"), ("value", "/id"), ("nsfw", "/adult")],
            Some("nsfw"),
        );
        let opts = parse_json_options(json, &def).unwrap();
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].0, "Safe");
    }
}
