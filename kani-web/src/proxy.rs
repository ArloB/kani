//! Cryptographic helpers for the opaque image proxy.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const NONCE_LEN: usize = 12;
const TOKEN_TTL_SECS: i64 = 3600;

/// Load the proxy secret from `KANI_PROXY_SECRET` (base64-encoded 32 bytes),
/// or generate a random one if the env var is absent.
pub fn load_or_generate_secret() -> [u8; 32] {
    if let Ok(val) = std::env::var("KANI_PROXY_SECRET") {
        let decoded = URL_SAFE_NO_PAD.decode(val.trim()).unwrap_or_default();
        if decoded.len() == 32 {
            let mut secret = [0u8; 32];
            secret.copy_from_slice(&decoded);
            tracing::info!("Loaded proxy secret from KANI_PROXY_SECRET");
            return secret;
        }
        tracing::warn!(
            "KANI_PROXY_SECRET was set but not 32 bytes after decoding — \
             generating a random secret instead"
        );
    }
    let secret: [u8; 32] = rand::random();
    tracing::info!(
        "Generated ephemeral proxy secret. Set KANI_PROXY_SECRET to persist \
         tokens across restarts."
    );
    secret
}

/// Seal a (url, referer) pair into an opaque, time-limited token.
pub fn seal_proxy_token(url: &str, referer: &str, secret: &[u8; 32]) -> String {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let expiry = ((now / TOKEN_TTL_SECS) + 1) * TOKEN_TTL_SECS;
    let plaintext = format!("{}|{}|{}", url, referer, expiry);

    let mut mac =
        <HmacSha256 as hmac::Mac>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(b"nonce|");
    mac.update(plaintext.as_bytes());
    let digest = mac.finalize().into_bytes();
    let nonce_bytes: [u8; NONCE_LEN] = digest[..NONCE_LEN]
        .try_into()
        .expect("HMAC-SHA256 output is >= 12 bytes");

    let nonce = Nonce::from_slice(&nonce_bytes);
    let cipher = ChaCha20Poly1305::new(secret.into());
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .expect("ChaCha20Poly1305 encryption is infallible for valid key/nonce");

    let mut token = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    token.extend_from_slice(&nonce_bytes);
    token.extend_from_slice(&ciphertext);
    URL_SAFE_NO_PAD.encode(token)
}

/// Unseal a token, returning `(url, referer)` if it is valid and unexpired.
pub fn unseal_proxy_token(token: &str, secret: &[u8; 32]) -> Option<(String, String)> {
    let raw = URL_SAFE_NO_PAD.decode(token).ok()?;
    if raw.len() <= NONCE_LEN {
        return None;
    }

    let (nonce_bytes, ciphertext) = raw.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = ChaCha20Poly1305::new(secret.into());

    let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;
    let s = String::from_utf8(plaintext).ok()?;

    let mut parts = s.splitn(2, '|');
    let url = parts.next()?.to_string();
    let tail = parts.next()?;
    let sep = tail.rfind('|')?;
    let referer = tail[..sep].to_string();
    let expiry: i64 = tail[sep + 1..].parse().ok()?;

    if time::OffsetDateTime::now_utc().unix_timestamp() > expiry {
        return None;
    }

    Some((url, referer))
}

/// Compute a stable, server-signed ETag for a (url, referer) pair.
pub fn compute_etag(url: &str, referer: &str, secret: &[u8; 32]) -> String {
    let mut mac =
        <HmacSha256 as hmac::Mac>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(b"etag|");
    mac.update(url.as_bytes());
    mac.update(b"|");
    mac.update(referer.as_bytes());
    format!("\"{}\"", hex::encode(mac.finalize().into_bytes()))
}

/// Build an opaque proxy URL for use in `src` attributes.
pub fn make_proxy_url(url: &str, referer: &str, secret: &[u8; 32]) -> String {
    let token = seal_proxy_token(url, referer, secret);
    format!("/rest/image_proxy?token={}", token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

    fn secret() -> [u8; 32] {
        [0xABu8; 32]
    }
    fn other_secret() -> [u8; 32] {
        [0xCDu8; 32]
    }

    #[test]
    fn roundtrip_basic() {
        let s = secret();
        let token = seal_proxy_token("https://img.example.com/a.jpg", "https://example.com", &s);
        assert_eq!(
            unseal_proxy_token(&token, &s),
            Some((
                "https://img.example.com/a.jpg".into(),
                "https://example.com".into()
            ))
        );
    }

    #[test]
    fn roundtrip_empty_referer() {
        let s = secret();
        let token = seal_proxy_token("https://img.example.com/a.jpg", "", &s);
        assert_eq!(
            unseal_proxy_token(&token, &s),
            Some(("https://img.example.com/a.jpg".into(), "".into()))
        );
    }

    #[test]
    fn roundtrip_special_chars_in_url() {
        let s = secret();
        let url = "https://cdn.example.com/path?size=800&quality=90";
        let referer = "https://example.com/manga/chapter/1";
        let token = seal_proxy_token(url, referer, &s);
        assert_eq!(
            unseal_proxy_token(&token, &s),
            Some((url.into(), referer.into()))
        );
    }

    #[test]
    fn roundtrip_unicode_in_url() {
        let s = secret();
        let url = "https://example.com/\u{753b}\u{50cf}/test.jpg";
        let referer = "https://example.com/\u{6f2b}\u{753b}/1";
        let token = seal_proxy_token(url, referer, &s);
        assert_eq!(
            unseal_proxy_token(&token, &s),
            Some((url.into(), referer.into()))
        );
    }

    #[test]
    fn roundtrip_pipe_in_referer() {
        let s = secret();
        let url = "https://cdn.example.com/img.jpg";
        let referer = "https://example.com/page|with|pipes";
        let token = seal_proxy_token(url, referer, &s);
        assert_eq!(
            unseal_proxy_token(&token, &s),
            Some((url.into(), referer.into()))
        );
    }

    #[test]
    fn wrong_secret_returns_none() {
        let token = seal_proxy_token("https://img.example.com/a.jpg", "ref", &secret());
        assert_eq!(unseal_proxy_token(&token, &other_secret()), None);
    }

    #[test]
    fn different_secrets_produce_different_tokens() {
        let url = "https://img.example.com/a.jpg";
        let t1 = seal_proxy_token(url, "ref", &secret());
        let t2 = seal_proxy_token(url, "ref", &other_secret());
        assert_ne!(t1, t2);
    }

    #[test]
    fn empty_token_returns_none() {
        assert_eq!(unseal_proxy_token("", &secret()), None);
    }

    #[test]
    fn garbage_token_returns_none() {
        assert_eq!(unseal_proxy_token("not-a-real-token!!", &secret()), None);
    }

    #[test]
    fn truncated_token_returns_none() {
        let token = seal_proxy_token("https://img.example.com/a.jpg", "ref", &secret());
        let truncated = &token[..token.len() / 2];
        assert_eq!(unseal_proxy_token(truncated, &secret()), None);
    }

    #[test]
    fn modified_ciphertext_returns_none() {
        let s = secret();
        let token = seal_proxy_token("https://img.example.com/a.jpg", "ref", &s);
        let mut raw = URL_SAFE_NO_PAD.decode(&token).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;
        let bad_token = URL_SAFE_NO_PAD.encode(&raw);
        assert_eq!(unseal_proxy_token(&bad_token, &s), None);
    }

    #[test]
    fn modified_nonce_returns_none() {
        let s = secret();
        let token = seal_proxy_token("https://img.example.com/a.jpg", "ref", &s);
        let mut raw = URL_SAFE_NO_PAD.decode(&token).unwrap();
        raw[0] ^= 0xFF;
        let bad_token = URL_SAFE_NO_PAD.encode(&raw);
        assert_eq!(unseal_proxy_token(&bad_token, &s), None);
    }

    #[test]
    fn token_contains_only_base64url_chars() {
        let token = seal_proxy_token("https://img.example.com/a.jpg", "ref", &secret());
        assert!(
            token
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        );
    }

    #[test]
    fn fresh_token_is_valid() {
        let s = secret();
        let token = seal_proxy_token("https://img.example.com/a.jpg", "ref", &s);
        assert!(unseal_proxy_token(&token, &s).is_some());
    }

    #[test]
    fn seal_empty_url() {
        let s = secret();
        let token = seal_proxy_token("", "ref", &s);
        let result = unseal_proxy_token(&token, &s);
        assert_eq!(result, Some(("".into(), "ref".into())));
    }

    #[test]
    fn seal_long_url() {
        let s = secret();
        let url = format!("https://example.com/{}", "a".repeat(1000));
        let token = seal_proxy_token(&url, "ref", &s);
        assert_eq!(unseal_proxy_token(&token, &s), Some((url, "ref".into())));
    }

    #[test]
    fn etag_is_deterministic() {
        let s = secret();
        let e1 = compute_etag("https://img.example.com/a.jpg", "ref", &s);
        let e2 = compute_etag("https://img.example.com/a.jpg", "ref", &s);
        assert_eq!(e1, e2);
    }

    #[test]
    fn etag_differs_by_url() {
        let s = secret();
        let e1 = compute_etag("https://img1.example.com/a.jpg", "ref", &s);
        let e2 = compute_etag("https://img2.example.com/b.jpg", "ref", &s);
        assert_ne!(e1, e2);
    }

    #[test]
    fn etag_differs_by_referer() {
        let s = secret();
        let e1 = compute_etag("https://img.example.com/a.jpg", "ref1", &s);
        let e2 = compute_etag("https://img.example.com/a.jpg", "ref2", &s);
        assert_ne!(e1, e2);
    }

    #[test]
    fn etag_is_quoted_hex() {
        let etag = compute_etag("https://img.example.com/a.jpg", "ref", &secret());
        assert!(etag.starts_with('"') && etag.ends_with('"'));
        let inner = &etag[1..etag.len() - 1];
        assert!(!inner.is_empty());
        assert!(inner.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
