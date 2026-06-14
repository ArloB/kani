use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Har {
    pub log: HarLog,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HarLog {
    pub entries: Vec<HarEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HarEntry {
    pub request: HarRequest,
    pub response: HarResponse,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HarRequest {
    pub method: String,
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HarResponse {
    pub status: u16,
    pub content: HarContent,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HarContent {
    #[serde(rename = "mimeType", default = "default_mime")]
    pub mime_type: String,
    pub text: Option<String>,
}

fn default_mime() -> String {
    "application/json".to_string()
}

pub fn load(path: &str) -> Result<Har, crate::error::CliError> {
    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|e| crate::error::CliError::Other(format!("HAR parse error in {path}: {e}")))
}

pub fn find_entry<'a>(har: &'a Har, url_fragment: &str) -> Option<&'a HarEntry> {
    har.log
        .entries
        .iter()
        .find(|e| e.request.url.contains(url_fragment))
}
