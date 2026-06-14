use crate::error::CliError;
use crate::repl::{har, test_cmd};
use reqwest::blocking::Client;

pub fn run(file: &str, endpoint: &str, args: &[String], output: &str) -> Result<(), CliError> {
    let ep = test_cmd::load_endpoint(file, endpoint)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| CliError::Other(format!("HTTP client error: {e}")))?;

    let base_url = extract_base_url(file)?;
    let route = build_route(&ep.route, args)?;
    let mut url = reqwest::Url::parse(&format!("{base_url}{route}"))
        .map_err(|e| CliError::Other(format!("invalid URL: {e}")))?;

    for qe in &ep.queries {
        let value = match &qe.value {
            crate::yaml::model::QueryValue::Static(v) => v.clone(),
            crate::yaml::model::QueryValue::Arg(name) => args
                .iter()
                .find(|a| a.starts_with(&format!("{name}=")))
                .and_then(|a| a.split_once('=').map(|x| x.1))
                .unwrap_or("")
                .to_owned(),
        };
        url.query_pairs_mut().append_pair(&qe.key, &value);
    }

    let url_str = url.to_string();
    println!("Fetching: {url_str}");

    let mut req = match ep.method.to_uppercase().as_str() {
        "POST" => client.post(&url_str),
        "PUT" => client.put(&url_str),
        "DELETE" => client.delete(&url_str),
        _ => client.get(&url_str),
    };

    for (key, value) in &ep.headers {
        req = req.header(key.as_str(), value.as_str());
    }

    let response = req
        .send()
        .map_err(|e| CliError::Other(format!("HTTP error: {e}")))?;
    let status = response.status().as_u16();
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_owned();
    let body = response
        .text()
        .map_err(|e| CliError::Other(format!("response read error: {e}")))?;

    println!("Status: {status}  Content-Type: {mime}");

    let har_doc = har::Har {
        log: har::HarLog {
            entries: vec![har::HarEntry {
                request: har::HarRequest {
                    method: ep.method.to_uppercase(),
                    url: url_str,
                },
                response: har::HarResponse {
                    status,
                    content: har::HarContent {
                        mime_type: mime,
                        text: Some(body),
                    },
                },
            }],
        },
    };

    let json =
        serde_json::to_string_pretty(&har_doc).map_err(|e| CliError::Other(e.to_string()))?;
    std::fs::write(output, json)?;
    println!("Recorded to {output}");
    Ok(())
}

fn extract_base_url(file: &str) -> Result<String, CliError> {
    let src = std::fs::read_to_string(file)?;
    let ext: crate::yaml::schema::YamlExtension = serde_yaml::from_str(&src)
        .map_err(|e| CliError::Other(format!("YAML parse error: {e}")))?;
    Ok(ext.base_url.trim_end_matches('/').to_string())
}

fn build_route(route: &str, args: &[String]) -> Result<String, CliError> {
    let mut result = route.to_owned();
    for arg in args {
        if let Some((key, value)) = arg.split_once('=') {
            result = result.replace(&format!("${key}$"), value);
        }
    }
    if result.contains('$') {
        return Err(CliError::Other(format!(
            "route {route:?} has unresolved placeholders; provide them as key=value args"
        )));
    }
    Ok(result)
}
