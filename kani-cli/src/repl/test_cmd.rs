use crate::error::CliError;
use crate::repl::har;
use crate::yaml::{
    model::{FieldSource, ValidatedEndpoint, ValidatedPopular},
    schema::{ResponseType, YamlExtension},
    validate,
};
use std::path::Path;

pub fn run_test(
    file: &str,
    har_path: &str,
    endpoint: &str,
    expected_count: usize,
) -> Result<(), CliError> {
    let actual = evaluate_endpoint_count(file, har_path, endpoint)?;
    if actual == expected_count {
        println!("ok — {actual} row(s)");
        Ok(())
    } else {
        Err(CliError::Other(format!(
            "row count mismatch: expected {expected_count}, got {actual}"
        )))
    }
}

pub fn run_replay(
    file: &str,
    har_path: &str,
    endpoint: &str,
    expected_path: &str,
) -> Result<(), CliError> {
    let ep = load_endpoint(file, endpoint)?;
    let har = har::load(har_path)?;
    let body = find_response_body(&har, file, endpoint)?;
    let actual = extract_rows(&ep, &body)?;

    let expected_src = std::fs::read_to_string(expected_path)?;
    let expected: serde_json::Value = serde_json::from_str(&expected_src)
        .map_err(|e| CliError::Other(format!("expected JSON parse error: {e}")))?;

    if actual == expected {
        println!("ok — replay matches expected output");
        Ok(())
    } else {
        let actual_pretty =
            serde_json::to_string_pretty(&actual).unwrap_or_else(|_| format!("{actual:?}"));
        let expected_pretty =
            serde_json::to_string_pretty(&expected).unwrap_or_else(|_| format!("{expected:?}"));
        eprintln!("--- expected ---");
        eprintln!("{expected_pretty}");
        eprintln!("--- actual ---");
        eprintln!("{actual_pretty}");
        Err(CliError::Other(
            "replay output does not match expected".into(),
        ))
    }
}

fn evaluate_endpoint_count(file: &str, har_path: &str, endpoint: &str) -> Result<usize, CliError> {
    let ep = load_endpoint(file, endpoint)?;
    let har = har::load(har_path)?;
    let body = find_response_body(&har, file, endpoint)?;
    let rows = extract_rows(&ep, &body)?;
    match rows {
        serde_json::Value::Array(arr) => Ok(arr.len()),
        _ => Ok(1),
    }
}

pub fn load_endpoint(file: &str, endpoint: &str) -> Result<ValidatedEndpoint, CliError> {
    let path = Path::new(file);
    let src = std::fs::read_to_string(path)?;
    let ext: YamlExtension = serde_yaml::from_str(&src)
        .map_err(|e| CliError::Other(format!("YAML parse error: {e}")))?;
    let validated = validate::validate(&ext, &src, path).map_err(|errors| {
        for e in &errors {
            eprintln!("  {e}");
        }
        CliError::Other(format!("{} validation error(s)", errors.len()))
    })?;

    match endpoint {
        "popular" => match validated.popular {
            Some(ValidatedPopular::Full(ep)) => Ok(*ep),
            Some(ValidatedPopular::Delegated { delegate_to, .. }) => Err(CliError::Other(format!(
                "popular endpoint delegates to {delegate_to}, not a direct fetch"
            ))),
            None => Err(CliError::Other("no popular endpoint defined".into())),
        },
        "search" => validated
            .search
            .ok_or_else(|| CliError::Other("no search endpoint defined".into())),
        "manga_details" | "details" => validated
            .manga_details
            .ok_or_else(|| CliError::Other("no manga_details endpoint defined".into())),
        "chapter_list" | "chapters" => validated
            .chapter_list
            .ok_or_else(|| CliError::Other("no chapter_list endpoint defined".into())),
        "pages" => validated
            .pages
            .ok_or_else(|| CliError::Other("no pages endpoint defined".into())),
        other => Err(CliError::Other(format!("unknown endpoint: {other}"))),
    }
}

fn find_response_body(har: &har::Har, _file: &str, _endpoint: &str) -> Result<String, CliError> {
    har.log
        .entries
        .iter()
        .find(|e| e.response.status < 400)
        .and_then(|e| e.response.content.text.clone())
        .ok_or_else(|| CliError::Other("no successful response found in HAR".into()))
}

fn extract_rows(ep: &ValidatedEndpoint, body: &str) -> Result<serde_json::Value, CliError> {
    match ep.response_type {
        ResponseType::Json => extract_json_rows(ep, body),
        ResponseType::Html => Err(CliError::Other(
            "HTML endpoint evaluation requires the full runtime; use JSON endpoints for CLI test/replay".into()
        )),
    }
}

fn extract_json_rows(ep: &ValidatedEndpoint, body: &str) -> Result<serde_json::Value, CliError> {
    let doc: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| CliError::Other(format!("JSON parse error: {e}")))?;

    let container = if ep.container.is_empty() {
        &doc
    } else {
        doc.pointer(&ep.container).ok_or_else(|| {
            CliError::Other(format!(
                "container {:?} not found in response",
                ep.container
            ))
        })?
    };

    let items: Vec<&serde_json::Value> = match container {
        serde_json::Value::Array(arr) => arr.iter().collect(),
        single => vec![single],
    };

    let field_names: Vec<&str> = ep
        .fields
        .iter()
        .filter(|f| matches!(f.source, FieldSource::Blueprint(_)))
        .map(|f| f.name.as_str())
        .collect();

    let rows: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            let mut obj = serde_json::Map::new();
            for name in &field_names {
                obj.insert((*name).to_string(), item[*name].clone());
            }
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(serde_json::Value::Array(rows))
}
