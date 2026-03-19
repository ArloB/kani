use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

pub fn encode_manga_id(raw: &str) -> String {
    URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

pub fn decode_manga_id(encoded: &str) -> String {
    URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_else(|| encoded.to_string())
}