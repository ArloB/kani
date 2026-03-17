//! Cryptographic helpers for the opaque image proxy.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::Aead,
};
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
    let expiry = chrono::Utc::now().timestamp() + TOKEN_TTL_SECS;
    let plaintext = format!("{}|{}|{}", url, referer, expiry);

    let nonce_bytes: [u8; NONCE_LEN] = rand::random();
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

    if chrono::Utc::now().timestamp() > expiry {
        return None;
    }

    Some((url, referer))
}

/// Compute a stable, server-signed ETag for a (url, referer) pair.
pub fn compute_etag(url: &str, referer: &str, secret: &[u8; 32]) -> String {
    let mut mac = <HmacSha256 as hmac::Mac>::new_from_slice(secret)
        .expect("HMAC accepts any key length");
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