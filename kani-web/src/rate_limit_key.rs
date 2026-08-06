use std::hash::{Hash, Hasher};

use axum::http::{Request, header::AUTHORIZATION};
use tower_governor::{GovernorError, key_extractor::KeyExtractor};

/// Rate-limit bucket identity.
///
/// Bearer-authenticated traffic is bucketed per token rather than per peer IP.
/// Sharing a bucket would mean a busy integration spends its owner's browsing
/// budget — the owner's UI would stall with no visible cause, and the fix
/// (revoking the token) would not be discoverable from the symptom.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RateKey {
    /// Hash of the presented bearer, never the token itself: this value is used
    /// as a map key and appears in error paths.
    Token(u64),
    Peer(std::net::IpAddr),
}

#[derive(Clone, Copy)]
pub struct TokenOrPeerIp;

fn hash_bearer(raw: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    hasher.finish()
}

impl KeyExtractor for TokenOrPeerIp {
    type Key = RateKey;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        if let Some(raw) = req
            .headers()
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|v| !v.is_empty())
        {
            return Ok(RateKey::Token(hash_bearer(raw)));
        }

        req.extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|info| RateKey::Peer(info.0.ip()))
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn req_with_auth(value: Option<&str>) -> Request<()> {
        let mut b = Request::builder().uri("/rest/sources");
        if let Some(v) = value {
            b = b.header(AUTHORIZATION, v);
        }
        b.body(()).unwrap()
    }

    #[test]
    fn distinct_tokens_get_distinct_buckets() {
        let a = TokenOrPeerIp.extract(&req_with_auth(Some("Bearer kani_aaa")));
        let b = TokenOrPeerIp.extract(&req_with_auth(Some("Bearer kani_bbb")));
        assert!(a.is_ok() && b.is_ok());
        assert_ne!(a.unwrap(), b.unwrap());
    }

    #[test]
    fn the_same_token_reuses_one_bucket() {
        let a = TokenOrPeerIp.extract(&req_with_auth(Some("Bearer kani_aaa")));
        let b = TokenOrPeerIp.extract(&req_with_auth(Some("Bearer kani_aaa")));
        assert_eq!(a.unwrap(), b.unwrap());
    }

    #[test]
    fn the_raw_token_is_never_the_key() {
        let key = TokenOrPeerIp
            .extract(&req_with_auth(Some("Bearer kani_supersecret")))
            .unwrap();
        assert!(
            !format!("{key:?}").contains("supersecret"),
            "the bucket key must not carry the credential"
        );
    }

    #[test]
    fn a_request_without_a_bearer_falls_back_to_peer_ip() {
        // No ConnectInfo in a synthetic request, so extraction fails rather than
        // silently lumping every anonymous caller into one shared bucket.
        assert!(TokenOrPeerIp.extract(&req_with_auth(None)).is_err());
        assert!(
            TokenOrPeerIp
                .extract(&req_with_auth(Some("Basic abc")))
                .is_err(),
            "only Bearer opts into token bucketing"
        );
    }
}
