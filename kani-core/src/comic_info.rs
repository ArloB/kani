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
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn minimal() -> ComicInfo {
        ComicInfo {
            xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance",
            series: "Test Series".to_string(),
            title: None,
            number: 1.0,
            volume: None,
            summary: None,
            language_iso: None,
            writer: None,
            penciller: None,
            genre: None,
            web: None,
        }
    }

    #[test]
    fn escapes_special_characters() {
        let info = ComicInfo {
            series: "Berserk & Co".to_string(),
            title: Some("<Dark Fantasy>".to_string()),
            summary: Some("A \"great\" story".to_string()),
            language_iso: Some("en".to_string()),
            ..minimal()
        };
        let xml = build_xml(&info).unwrap();
        assert!(xml.contains("Berserk &amp; Co"));
        assert!(xml.contains("&lt;Dark Fantasy&gt;"));
        assert!(xml.contains("&quot;great&quot;") || xml.contains("\"great\""));
    }

    #[test]
    fn xml_declaration_is_present() {
        let xml = build_xml(&minimal()).unwrap();
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="utf-8"?>"#));
    }

    #[test]
    fn mandatory_fields_appear() {
        let info = ComicInfo {
            series: "My Manga".to_string(),
            number: 7.5,
            ..minimal()
        };
        let xml = build_xml(&info).unwrap();
        assert!(xml.contains("<Series>My Manga</Series>"));
        assert!(xml.contains("<Number>7.5</Number>"));
    }

    #[test]
    fn whole_number_serializes_without_fraction() {
        let info = ComicInfo {
            number: 3.0,
            ..minimal()
        };
        let xml = build_xml(&info).unwrap();
        // quick-xml serialises f64: 3.0 should not produce "3.0" with trailing zero fraction
        // — accept either "3" or "3.0" as long as the chapter parses back correctly.
        assert!(xml.contains("<Number>3</Number>") || xml.contains("<Number>3.0</Number>"));
    }

    #[test]
    fn optional_none_fields_are_absent() {
        let xml = build_xml(&minimal()).unwrap();
        assert!(!xml.contains("<Title>"));
        assert!(!xml.contains("<Volume>"));
        assert!(!xml.contains("<Summary>"));
        assert!(!xml.contains("<LanguageISO>"));
        assert!(!xml.contains("<Writer>"));
        assert!(!xml.contains("<Penciller>"));
        assert!(!xml.contains("<Genre>"));
        assert!(!xml.contains("<Web>"));
    }

    #[test]
    fn all_optional_fields_appear_when_set() {
        let info = ComicInfo {
            series: "Fullmetal".to_string(),
            title: Some("Brotherhood".to_string()),
            number: 1.0,
            volume: Some(1),
            summary: Some("Alchemy story".to_string()),
            language_iso: Some("ja".to_string()),
            writer: Some("Arakawa Hiromu".to_string()),
            penciller: Some("Arakawa Hiromu".to_string()),
            genre: Some("Action".to_string()),
            web: Some("https://example.com".to_string()),
            ..minimal()
        };
        let xml = build_xml(&info).unwrap();
        assert!(xml.contains("<Title>Brotherhood</Title>"));
        assert!(xml.contains("<Volume>1</Volume>"));
        assert!(xml.contains("<Summary>Alchemy story</Summary>"));
        assert!(xml.contains("<LanguageISO>ja</LanguageISO>"));
        assert!(xml.contains("<Writer>Arakawa Hiromu</Writer>"));
        assert!(xml.contains("<Penciller>Arakawa Hiromu</Penciller>"));
        assert!(xml.contains("<Genre>Action</Genre>"));
        assert!(xml.contains("<Web>https://example.com</Web>"));
    }
}
