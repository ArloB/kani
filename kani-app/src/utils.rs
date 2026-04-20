/// Decodes a base64url-encoded manga ID back to its original string form.
/// Falls back to returning the input unchanged if decoding fails.
pub fn decode_manga_id(encoded: &str) -> String {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

    URL_SAFE_NO_PAD
        .decode(encoded)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_else(|| encoded.to_string())
}
