use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename = "ComicInfo")]
pub struct ComicInfo {
    #[serde(rename = "@xmlns:xsi")]
    pub xmlns_xsi: &'static str,
    #[serde(rename = "Series")]
    pub series: String,
    #[serde(rename = "Title", skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "Number")]
    pub number: f64,
    #[serde(rename = "Volume", skip_serializing_if = "Option::is_none")]
    pub volume: Option<i64>,
    #[serde(rename = "Summary", skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(rename = "LanguageISO", skip_serializing_if = "Option::is_none")]
    pub language_iso: Option<String>,
    #[serde(rename = "Writer", skip_serializing_if = "Option::is_none")]
    pub writer: Option<String>,
    #[serde(rename = "Penciller", skip_serializing_if = "Option::is_none")]
    pub penciller: Option<String>,
    #[serde(rename = "Genre", skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(rename = "Web", skip_serializing_if = "Option::is_none")]
    pub web: Option<String>,
}

/// Serialise a `ComicInfo` to a UTF-8 XML string with a proper declaration.
pub fn build_xml(info: &ComicInfo) -> crate::error::Result<String> {
    let mut buf = String::from(r#"<?xml version="1.0" encoding="utf-8"?>"#);
    buf.push('\n');
    let serialized = quick_xml::se::to_string(info).map_err(|e| {
        tracing::error!("Failed to serialize ComicInfo: {}", e);
        crate::error::Error::Internal(format!("Failed to serialize ComicInfo: {}", e))
    })?;
    buf.push_str(&serialized);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_special_characters() {
        let info = ComicInfo {
            xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance",
            series: "Berserk & Co".to_string(),
            title: Some("<Dark Fantasy>".to_string()),
            number: 1.0,
            volume: None,
            summary: Some("A \"great\" story".to_string()),
            language_iso: Some("en".to_string()),
            writer: None,
            penciller: None,
            genre: None,
            web: None,
        };
        let xml = build_xml(&info).unwrap();
        assert!(xml.contains("Berserk &amp; Co"));
        assert!(xml.contains("&lt;Dark Fantasy&gt;"));
        assert!(xml.contains("&quot;great&quot;") || xml.contains("\"great\""));
    }
}
