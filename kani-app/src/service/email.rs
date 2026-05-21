use std::sync::Arc;

use async_trait::async_trait;
use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};

use crate::models::Settings;

/// Provider-agnostic email sending interface.
/// Implement this trait to add a new email provider.
#[async_trait]
pub trait EmailTransport: Send + Sync {
    async fn send(&self, from: &str, to: &str, subject: &str, html_body: &str) -> Result<(), String>;
}

/// General-purpose email service. Holds a provider backend and the configured from-address.
/// Cheap to clone — the transport is behind an Arc.
#[derive(Clone)]
pub struct EmailService {
    transport: Arc<dyn EmailTransport>,
    pub from_address: String,
}

impl EmailService {
    /// Constructs from current settings. Returns `None` if email is disabled or unconfigured.
    pub fn from_settings(s: &Settings) -> Option<Self> {
        if !s.email_enabled || s.email_from_address.is_empty() {
            return None;
        }
        let config: serde_json::Value =
            serde_json::from_str(&s.email_provider_config).unwrap_or_default();
        let transport: Arc<dyn EmailTransport> = match s.email_provider.as_str() {
            "smtp" | "" => Arc::new(SmtpEmailTransport::from_config(&config)?),
            unknown => {
                tracing::warn!("Unknown email provider: {unknown}");
                return None;
            }
        };
        Some(Self { transport, from_address: s.email_from_address.clone() })
    }

    pub async fn send(&self, to: &str, subject: &str, html: &str) -> Result<(), String> {
        self.transport.send(&self.from_address, to, subject, html).await
    }
}

// ── SMTP backend ──────────────────────────────────────────────────────────────

struct SmtpEmailTransport {
    inner: AsyncSmtpTransport<Tokio1Executor>,
}

impl SmtpEmailTransport {
    fn from_config(cfg: &serde_json::Value) -> Option<Self> {
        let host = cfg.get("host").and_then(|v| v.as_str()).filter(|h| !h.is_empty())?;
        let port = cfg.get("port").and_then(|v| v.as_u64()).unwrap_or(587) as u16;
        let username = cfg.get("username").and_then(|v| v.as_str()).unwrap_or_default();
        let password = cfg.get("password").and_then(|v| v.as_str()).unwrap_or_default();
        let tls_mode = cfg.get("tls_mode").and_then(|v| v.as_str()).unwrap_or("starttls");

        let creds = if !username.is_empty() {
            Some(Credentials::new(username.to_string(), password.to_string()))
        } else {
            None
        };

        let transport = match tls_mode {
            "tls" => {
                let mut b = AsyncSmtpTransport::<Tokio1Executor>::relay(host).ok()?;
                b = b.port(port);
                if let Some(c) = creds {
                    b = b.credentials(c);
                }
                b.build()
            }
            "none" | "plain" => {
                let mut b = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host);
                b = b.port(port);
                if let Some(c) = creds {
                    b = b.credentials(c);
                }
                b.build()
            }
            _ => {
                let mut b =
                    AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host).ok()?;
                b = b.port(port);
                if let Some(c) = creds {
                    b = b.credentials(c);
                }
                b.build()
            }
        };

        Some(Self { inner: transport })
    }
}

#[async_trait]
impl EmailTransport for SmtpEmailTransport {
    async fn send(
        &self,
        from: &str,
        to: &str,
        subject: &str,
        html_body: &str,
    ) -> Result<(), String> {
        let from_mb: Mailbox =
            from.parse().map_err(|e| format!("Invalid from address: {e}"))?;
        let to_mb: Mailbox = to.parse().map_err(|e| format!("Invalid to address: {e}"))?;

        let message = lettre::Message::builder()
            .from(from_mb)
            .to(to_mb)
            .subject(subject)
            .header(ContentType::TEXT_HTML)
            .body(html_body.to_string())
            .map_err(|e| format!("Failed to build email: {e}"))?;

        self.inner.send(message).await.map(|_| ()).map_err(|e| e.to_string())
    }
}

// ── Token generation (shared by password_reset and email_verification) ────────

/// Generates a (raw_token, sha256_hash) pair.
/// `raw_token` is sent to the user; only `sha256_hash` is stored in the DB.
pub(crate) fn generate_token() -> (String, String) {
    use sha2::{Digest, Sha256};

    let bytes: [u8; 32] = rand::random();
    let raw = hex::encode(bytes);
    let hash = hex::encode(Sha256::digest(raw.as_bytes()));
    (raw, hash)
}

/// SHA-256 hex of the given raw token string.
pub(crate) fn hash_token(raw: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(raw.as_bytes()))
}
