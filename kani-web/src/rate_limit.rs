//! Per-identity and per-IP rate limiting for auth endpoints.
//!
//! Two-phase design — no recording on pre-check:
//!   1. `check_lockout` — read-only: is this identity/IP already locked?
//!   2. `record_and_check` — write then read: record actual outcome, return new lockout state.
//!
//! This avoids the correctness bug of recording a "failure" before the auth attempt,
//! which would cause lockout after N *successful* logins within the window.
//!
//! Identity is stored as SHA-256 hex — the plaintext username/email is never persisted.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use time::OffsetDateTime;

/// Result of a rate-limit check.
#[derive(Debug, PartialEq)]
pub enum RateLimitResult {
    Allowed,
    LockedOutByIdentity { retry_after_secs: i64 },
    LockedOutByIp { retry_after_secs: i64 },
}

/// Tracks login attempts and enforces per-identity and per-IP lockouts.
#[derive(Clone)]
pub struct AuthRateLimiter {
    db: SqlitePool,
    /// Maximum failed attempts per identity before lockout.
    max_attempts_identity: i64,
    /// Maximum failed attempts per IP before lockout.
    max_attempts_ip: i64,
    /// Lockout window in seconds.
    lockout_seconds: i64,
}

impl AuthRateLimiter {
    pub fn new(db: SqlitePool) -> Self {
        let max_attempts_identity = std::env::var("KANI_MAX_LOGIN_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5i64);
        let max_attempts_ip = std::env::var("KANI_MAX_IP_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20i64);
        let lockout_seconds = std::env::var("KANI_LOGIN_LOCKOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(900i64);
        Self {
            db,
            max_attempts_identity,
            max_attempts_ip,
            lockout_seconds,
        }
    }

    fn hash_identity(identity: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(identity.to_lowercase().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Phase 1 — read-only pre-flight check before performing authentication.
    ///
    /// Does NOT write to the database. Returns `Locked*` if the identity or IP
    /// is already within a lockout window from prior failures.
    pub async fn check_lockout(&self, identity: &str, ip_addr: &str) -> RateLimitResult {
        let identity_hash = Self::hash_identity(identity);
        let window_start = OffsetDateTime::now_utc().unix_timestamp() - self.lockout_seconds;
        self.lockout_state(&identity_hash, ip_addr, window_start)
            .await
    }

    /// Phase 2 — record the actual auth outcome (success or failure) and return
    /// the new lockout state.
    ///
    /// Call this AFTER authentication completes. On success the row is recorded
    /// with `succeeded = 1` and `Allowed` is returned. On failure the row is
    /// recorded with `succeeded = 0` and the lockout state is rechecked.
    pub async fn record_and_check(
        &self,
        identity: &str,
        ip_addr: &str,
        succeeded: bool,
    ) -> RateLimitResult {
        let identity_hash = Self::hash_identity(identity);
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let window_start = now - self.lockout_seconds;

        let succeeded_int = succeeded as i64;
        let _ = sqlx::query(
            "INSERT INTO login_attempts (identity_hash, ip_addr, succeeded, attempted_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(&identity_hash)
        .bind(ip_addr)
        .bind(succeeded_int)
        .bind(now)
        .execute(&self.db)
        .await;

        if succeeded {
            return RateLimitResult::Allowed;
        }

        self.lockout_state(&identity_hash, ip_addr, window_start)
            .await
    }

    /// Shared lockout computation: query failure counts and derive retry-after.
    async fn lockout_state(
        &self,
        identity_hash: &str,
        ip_addr: &str,
        window_start: i64,
    ) -> RateLimitResult {
        let now = OffsetDateTime::now_utc().unix_timestamp();

        // Per-identity lockout.
        let identity_failures: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM login_attempts \
             WHERE identity_hash = ? AND succeeded = 0 AND attempted_at >= ?",
        )
        .bind(identity_hash)
        .bind(window_start)
        .fetch_one(&self.db)
        .await
        .unwrap_or(0);

        if identity_failures >= self.max_attempts_identity {
            let oldest: i64 = sqlx::query_scalar(
                "SELECT MIN(attempted_at) FROM login_attempts \
                 WHERE identity_hash = ? AND succeeded = 0 AND attempted_at >= ?",
            )
            .bind(identity_hash)
            .bind(window_start)
            .fetch_one(&self.db)
            .await
            .unwrap_or(window_start);

            let retry_after_secs = (oldest + self.lockout_seconds - now).max(1);
            return RateLimitResult::LockedOutByIdentity { retry_after_secs };
        }

        // Per-IP lockout.
        let ip_failures: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM login_attempts \
             WHERE ip_addr = ? AND succeeded = 0 AND attempted_at >= ?",
        )
        .bind(ip_addr)
        .bind(window_start)
        .fetch_one(&self.db)
        .await
        .unwrap_or(0);

        if ip_failures >= self.max_attempts_ip {
            let oldest: i64 = sqlx::query_scalar(
                "SELECT MIN(attempted_at) FROM login_attempts \
                 WHERE ip_addr = ? AND succeeded = 0 AND attempted_at >= ?",
            )
            .bind(ip_addr)
            .bind(window_start)
            .fetch_one(&self.db)
            .await
            .unwrap_or(window_start);

            let retry_after_secs = (oldest + self.lockout_seconds - now).max(1);
            return RateLimitResult::LockedOutByIp { retry_after_secs };
        }

        RateLimitResult::Allowed
    }

    /// Prune attempts older than the lockout window. Called by the daily background job.
    pub async fn prune_old_attempts(&self) {
        let cutoff = OffsetDateTime::now_utc().unix_timestamp() - self.lockout_seconds;
        let _ = sqlx::query("DELETE FROM login_attempts WHERE attempted_at < ?")
            .bind(cutoff)
            .execute(&self.db)
            .await;
    }
}
