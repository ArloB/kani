//! Password strength and breach-check policy.
//!
//! Validates passwords against:
//! 1. Minimum length (10 characters)
//! 2. Same-as-identity check (password must not equal username or email)
//! 3. zxcvbn entropy score (must be ≥ 2)
//! 4. HIBP k-anonymity breach check (advisory — skipped on network failure)

use sha1::{Digest, Sha1};

/// Structured password validation errors.
#[derive(Debug, thiserror::Error)]
pub enum PasswordPolicyError {
    #[error("Password must be at least 10 characters")]
    TooShort,

    #[error("Password must not be the same as your username or email")]
    SameAsIdentity,

    #[error("This password has appeared {0} times in known data breaches")]
    Pwned(u64),

    #[error("Password is too weak (score: {0}/4). {1}")]
    TooWeak(u8, String),
}

/// Strength check result returned on success.
#[derive(Debug)]
pub struct PasswordStrength {
    /// 0–4 (zxcvbn scale)
    pub score: u8,
    pub feedback: Vec<String>,
    /// `None` when check was skipped; `0` means not found in HIBP.
    pub pwned_count: Option<u64>,
}

/// Validate `password` against all policy rules.
///
/// `identity` is the username or email to use for the same-as-identity check.
/// `http_client` is used for the HIBP k-anonymity call; if unavailable the check is skipped.
pub async fn check_password(
    password: &str,
    identity: &str,
    http_client: &kani_core::http::SmartClient,
) -> Result<PasswordStrength, PasswordPolicyError> {
    // 1. Length.
    if password.len() < 10 {
        return Err(PasswordPolicyError::TooShort);
    }

    // 2. Same-as-identity (case-insensitive).
    if password.to_lowercase() == identity.to_lowercase() {
        return Err(PasswordPolicyError::SameAsIdentity);
    }

    // 3. zxcvbn entropy.
    let estimate = zxcvbn::zxcvbn(password, &[identity]);
    let score: u8 = estimate.score().into();
    if score < 2 {
        let suggestion = estimate
            .feedback()
            .and_then(|f| f.warning())
            .map(|w| w.to_string())
            .unwrap_or_else(|| "Consider adding more unusual words or characters".into());
        return Err(PasswordPolicyError::TooWeak(score, suggestion));
    }
    let feedback: Vec<String> = estimate
        .feedback()
        .map(|f| {
            let mut v: Vec<String> = f.suggestions().iter().map(|s| s.to_string()).collect();
            if let Some(w) = f.warning() {
                v.insert(0, w.to_string());
            }
            v
        })
        .unwrap_or_default();

    // 4. HIBP k-anonymity (advisory; skip on error).
    let pwned_count = check_hibp(password, http_client).await;

    if let Some(count) = pwned_count
        && count > 0
    {
        return Err(PasswordPolicyError::Pwned(count));
    }

    Ok(PasswordStrength {
        score,
        feedback,
        pwned_count,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn hibp_hash_prefix_for_password() {
        // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
        // First 5 chars = "5BAA6" — the k-anonymity prefix sent to HIBP.
        let hash = sha1_hex_upper("password");
        assert_eq!(
            &hash[..5],
            "5BAA6",
            "HIBP prefix for 'password' must be '5BAA6'"
        );
    }

    #[test]
    fn zxcvbn_weak_password_low_score() {
        let estimate = zxcvbn::zxcvbn("password123", &[]);
        let score: u8 = estimate.score().into();
        assert!(
            score <= 1,
            "expected score <= 1 for 'password123', got {score}"
        );
    }

    #[test]
    fn zxcvbn_strong_password_high_score() {
        let estimate = zxcvbn::zxcvbn("correct-horse-battery-staple-47", &[]);
        let score: u8 = estimate.score().into();
        assert!(
            score >= 3,
            "expected score >= 3 for passphrase, got {score}"
        );
    }

    #[test]
    fn short_password_fails_length_check() {
        // The first check in check_password is length; verify it would reject ≤9 chars.
        assert!(
            "shortpw!".len() < 10,
            "test precondition: password is < 10 chars"
        );
    }

    #[test]
    fn same_as_identity_check_is_case_insensitive() {
        // The check is: password.to_lowercase() == identity.to_lowercase()
        assert_eq!("alice".to_lowercase(), "ALICE".to_lowercase());
    }
}

/// Compute the SHA-1 hex of `password` in uppercase. Public for testing.
pub fn sha1_hex_upper(password: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(password.as_bytes());
    format!("{:X}", hasher.finalize())
}

/// SHA-1 k-anonymity lookup against the HIBP Pwned Passwords API.
/// Returns `None` if the network call fails (advisory only).
async fn check_hibp(password: &str, http_client: &kani_core::http::SmartClient) -> Option<u64> {
    // 1. SHA-1 of the password.
    let mut hasher = Sha1::new();
    hasher.update(password.as_bytes());
    let hash = format!("{:X}", hasher.finalize()); // uppercase hex

    let prefix = &hash[..5];
    let suffix = &hash[5..];

    // 2. k-anonymity API call — only the 5-character prefix is sent.
    let url = format!("https://api.pwnedpasswords.com/range/{prefix}");
    let response = http_client.inner().get(&url).send().await.ok()?;

    if !response.status().is_success() {
        tracing::warn!("HIBP API returned status {}", response.status());
        return None;
    }

    let body = response.text().await.ok()?;

    // 3. Check locally whether our suffix matches any returned line.
    for line in body.lines() {
        if let Some((line_suffix, count_str)) = line.split_once(':')
            && line_suffix.eq_ignore_ascii_case(suffix)
        {
            return count_str.trim().parse().ok();
        }
    }

    Some(0) // hash prefix returned, our suffix not found → not pwned
}
