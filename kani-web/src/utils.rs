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

pub fn render_description(raw: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);

    let parser = Parser::new_ext(raw, opts);
    let mut html_output = String::with_capacity(raw.len() * 2);
    html::push_html(&mut html_output, parser);

    ammonia::clean(&html_output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

    fn encode(s: &str) -> String {
        URL_SAFE_NO_PAD.encode(s.as_bytes())
    }

    #[test]
    fn roundtrip_encode_decode() {
        let original = "https://example.com/manga/12345";
        assert_eq!(decode_manga_id(&encode(original)), original);
    }

    #[test]
    fn plain_id_falls_back_to_original() {
        // A simple slug that is not valid base64url decodes — returned as-is.
        assert_eq!(decode_manga_id("my-manga-slug"), "my-manga-slug");
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(decode_manga_id(""), "");
    }

    #[test]
    fn invalid_base64_returns_original() {
        let input = "not-base64!!!";
        assert_eq!(decode_manga_id(input), input);
    }

    #[test]
    fn valid_base64_non_utf8_falls_back() {
        // Bytes that are valid base64url but produce invalid UTF-8.
        let bad_utf8 = URL_SAFE_NO_PAD.encode([0xFF, 0xFE]);
        assert_eq!(decode_manga_id(&bad_utf8), bad_utf8);
    }

    #[test]
    fn unicode_manga_id_roundtrips() {
        let original = "manga/進撃の巨人/ch1";
        assert_eq!(decode_manga_id(&encode(original)), original);
    }
}
