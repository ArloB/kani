use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::SqlitePool;
use time::format_description::well_known::Rfc3339;

use crate::error::{Result, ServiceError};
use crate::ids::{ChapterId, MangaId};

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

// ── Service ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WebhookService {
    pub db: SqlitePool,
    pub http: rquest::Client,
}

impl WebhookService {
    pub fn new(db: SqlitePool) -> Self {
        Self {
            db,
            http: rquest::Client::new(),
        }
    }
}

// ── Payload types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookPayload {
    ChapterNew {
        manga_id: MangaId,
        manga_name: String,
        chapter_count: usize,
        chapter_ids: Vec<i64>,
        chapter_names: Vec<String>,
    },
    MangaAdded {
        manga_id: MangaId,
        manga_name: String,
        source_id: i64,
    },
    MangaDeleted {
        manga_id: MangaId,
        manga_name: String,
    },
    ChapterDownloaded {
        chapter_id: ChapterId,
        manga_id: MangaId,
        manga_name: String,
        chapter_name: String,
    },
    ScanCompleted {
        total_scanned: usize,
        failed_count: usize,
    },
}

impl WebhookPayload {
    fn event_type(&self) -> &'static str {
        match self {
            Self::ChapterNew { .. } => "chapter.new",
            Self::MangaAdded { .. } => "manga.added",
            Self::MangaDeleted { .. } => "manga.deleted",
            Self::ChapterDownloaded { .. } => "chapter.downloaded",
            Self::ScanCompleted { .. } => "scan.completed",
        }
    }

    fn manga_id(&self) -> Option<i64> {
        match self {
            Self::ChapterNew { manga_id, .. }
            | Self::MangaAdded { manga_id, .. }
            | Self::MangaDeleted { manga_id, .. }
            | Self::ChapterDownloaded { manga_id, .. } => Some(manga_id.0),
            Self::ScanCompleted { .. } => None,
        }
    }
}

#[derive(Serialize)]
struct Envelope<'a> {
    event: &'static str,
    timestamp: String,
    data: &'a WebhookPayload,
}

// ── Row types ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct WebhookRow {
    pub id: i64,
    pub url: String,
    #[serde(skip)]
    pub secret: Option<String>,
    pub events: String,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Serialize)]
pub struct DeliveryRow {
    pub id: i64,
    pub webhook_id: i64,
    pub event_type: String,
    pub payload: String,
    pub http_status: Option<i64>,
    pub error: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub delivered_at: time::OffsetDateTime,
}

// ── Request/response types for the REST layer ─────────────────────────────────

#[derive(Deserialize)]
pub struct CreateWebhookBody {
    pub url: String,
    pub secret: Option<String>,
    /// JSON array of event type strings, or `["*"]` for all. Defaults to `["*"]`.
    pub events: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateWebhookBody {
    pub url: Option<String>,
    pub secret: Option<String>,
    pub events: Option<String>,
    pub enabled: Option<bool>,
}

// ── Core firing logic ─────────────────────────────────────────────────────────

impl WebhookService {
    /// Fire a webhook event. Spawns one background task per applicable webhook.
    /// Never blocks or fails the caller — errors are logged and recorded in webhook_deliveries.
    pub async fn fire(&self, payload: WebhookPayload) {
        let event_type = payload.event_type();
        let manga_id = payload.manga_id();

        let envelope = Envelope {
            event: event_type,
            timestamp: now_rfc3339(),
            data: &payload,
        };
        let body = match serde_json::to_string(&envelope) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("Webhook: failed to serialize payload: {e}");
                return;
            }
        };

        let webhooks = match self.list_applicable(event_type, manga_id).await {
            Ok(wh) => wh,
            Err(e) => {
                tracing::error!("Webhook: failed to query applicable webhooks: {e}");
                return;
            }
        };

        for wh in webhooks {
            let client = self.http.clone();
            let db = self.db.clone();
            let body = body.clone();
            let event_str = event_type.to_owned();

            tokio::spawn(async move {
                let sig = wh.secret.as_deref().map(|s| {
                    let mut mac = Hmac::<Sha256>::new_from_slice(s.as_bytes())
                        .expect("HMAC accepts any key size");
                    mac.update(body.as_bytes());
                    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
                });

                let mut builder = client
                    .post(&wh.url)
                    .header("Content-Type", "application/json")
                    .header("User-Agent", "Kani-Webhook/1.0")
                    .body(body.clone());
                if let Some(ref sig) = sig {
                    builder = builder.header("X-Kani-Signature", sig.as_str());
                }

                let (http_status, error) = match builder.send().await {
                    Ok(r) => (Some(r.status().as_u16() as i64), None),
                    Err(e) => {
                        tracing::warn!("Webhook delivery to {} failed: {e}", wh.url);
                        (None, Some(e.to_string()))
                    }
                };

                let _ = sqlx::query!(
                    "INSERT INTO webhook_deliveries \
                     (webhook_id, event_type, payload, http_status, error) \
                     VALUES (?, ?, ?, ?, ?)",
                    wh.id,
                    event_str,
                    body,
                    http_status,
                    error,
                )
                .execute(&db)
                .await;
            });
        }
    }

    /// Send a synchronous test event to a single webhook and return success/error.
    pub async fn send_test(&self, id: i64) -> Result<()> {
        let wh = self.get_by_id(id).await?;

        let body = serde_json::to_string(&serde_json::json!({
            "event": "test",
            "timestamp": now_rfc3339(),
            "data": {},
        }))
        .map_err(|e| ServiceError::Internal(e.to_string()))?;

        let sig = wh.secret.as_deref().map(|s| {
            let mut mac =
                Hmac::<Sha256>::new_from_slice(s.as_bytes()).expect("HMAC accepts any key size");
            mac.update(body.as_bytes());
            format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
        });

        let mut builder = self
            .http
            .post(&wh.url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "Kani-Webhook/1.0")
            .body(body.clone());
        if let Some(ref sig) = sig {
            builder = builder.header("X-Kani-Signature", sig.as_str());
        }

        let resp = builder.send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(ServiceError::Internal(format!(
                "Webhook returned HTTP {}",
                resp.status()
            )))
        }
    }

    async fn list_applicable(
        &self,
        event_type: &str,
        manga_id: Option<i64>,
    ) -> Result<Vec<WebhookRow>> {
        let webhooks = sqlx::query_as!(
            WebhookRow,
            r#"SELECT id, url, secret, events, enabled AS "enabled: bool", created_at
               FROM webhooks
               WHERE enabled = TRUE
                 AND (
                     events = '["*"]'
                     OR EXISTS (SELECT 1 FROM json_each(events) WHERE value = ?)
                 )"#,
            event_type,
        )
        .fetch_all(&self.db)
        .await?;

        let Some(mid) = manga_id else {
            return Ok(webhooks);
        };

        // Fetch opt-out rows for this manga (webhook_id = 0 = global opt-out).
        let opted_out: Vec<i64> = sqlx::query_scalar!(
            "SELECT webhook_id FROM webhook_manga_overrides \
             WHERE manga_id = ? AND enabled = FALSE",
            mid,
        )
        .fetch_all(&self.db)
        .await?;

        let opted_out_set: std::collections::HashSet<i64> = opted_out.into_iter().collect();
        if opted_out_set.contains(&0) {
            // Global opt-out — suppress all webhooks for this manga.
            return Ok(vec![]);
        }

        Ok(webhooks
            .into_iter()
            .filter(|w| !opted_out_set.contains(&w.id))
            .collect())
    }

    // ── CRUD ──────────────────────────────────────────────────────────────────

    pub async fn list_webhooks(&self) -> Result<Vec<WebhookRow>> {
        let rows = sqlx::query_as!(
            WebhookRow,
            r#"SELECT id, url, secret, events, enabled AS "enabled: bool", created_at
               FROM webhooks ORDER BY id ASC"#,
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows)
    }

    pub async fn get_by_id(&self, id: i64) -> Result<WebhookRow> {
        sqlx::query_as!(
            WebhookRow,
            r#"SELECT id, url, secret, events, enabled AS "enabled: bool", created_at
               FROM webhooks WHERE id = ?"#,
            id,
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| ServiceError::NotFound(format!("Webhook {id} not found")))
    }

    pub async fn create_webhook(&self, body: CreateWebhookBody) -> Result<WebhookRow> {
        validate_url(&body.url)?;
        let events = body.events.unwrap_or_else(|| r#"["*"]"#.to_owned());
        validate_events_json(&events)?;

        let id = sqlx::query_scalar!(
            "INSERT INTO webhooks (url, secret, events) VALUES (?, ?, ?) RETURNING id",
            body.url,
            body.secret,
            events,
        )
        .fetch_one(&self.db)
        .await?;

        self.get_by_id(id).await
    }

    pub async fn update_webhook(&self, id: i64, body: UpdateWebhookBody) -> Result<WebhookRow> {
        self.get_by_id(id).await?;

        if let Some(ref url) = body.url {
            validate_url(url)?;
        }
        if let Some(ref events) = body.events {
            validate_events_json(events)?;
        }

        if let Some(url) = &body.url {
            sqlx::query!("UPDATE webhooks SET url = ? WHERE id = ?", url, id)
                .execute(&self.db)
                .await?;
        }
        if body.secret.is_some() {
            sqlx::query!(
                "UPDATE webhooks SET secret = ? WHERE id = ?",
                body.secret,
                id
            )
            .execute(&self.db)
            .await?;
        }
        if let Some(events) = &body.events {
            sqlx::query!("UPDATE webhooks SET events = ? WHERE id = ?", events, id)
                .execute(&self.db)
                .await?;
        }
        if let Some(enabled) = body.enabled {
            sqlx::query!("UPDATE webhooks SET enabled = ? WHERE id = ?", enabled, id)
                .execute(&self.db)
                .await?;
        }

        self.get_by_id(id).await
    }

    pub async fn delete_webhook(&self, id: i64) -> Result<()> {
        let affected = sqlx::query!("DELETE FROM webhooks WHERE id = ?", id)
            .execute(&self.db)
            .await?
            .rows_affected();
        if affected == 0 {
            return Err(ServiceError::NotFound(format!("Webhook {id} not found")));
        }
        Ok(())
    }

    pub async fn list_deliveries(&self, webhook_id: i64) -> Result<Vec<DeliveryRow>> {
        let rows = sqlx::query_as!(
            DeliveryRow,
            "SELECT id, webhook_id, event_type, payload, http_status, error, delivered_at \
             FROM webhook_deliveries \
             WHERE webhook_id = ? \
             ORDER BY delivered_at DESC \
             LIMIT 50",
            webhook_id,
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows)
    }

    // ── Per-manga notify flag ─────────────────────────────────────────────────

    /// Returns whether webhook notifications are enabled globally for a manga.
    /// True = enabled (no global opt-out row), False = opted out.
    pub async fn get_manga_notify(&self, manga_id: MangaId) -> Result<bool> {
        // An opt-out row (webhook_id=0, enabled=FALSE) means notifications are disabled.
        let opted_out: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM webhook_manga_overrides \
             WHERE webhook_id = 0 AND manga_id = ? AND enabled = FALSE",
            manga_id,
        )
        .fetch_one(&self.db)
        .await?;
        Ok(opted_out == 0)
    }

    /// Set the global per-manga webhook notify flag.
    pub async fn set_manga_notify(&self, manga_id: MangaId, enabled: bool) -> Result<()> {
        if enabled {
            // Remove the opt-out row so the default (enabled) applies.
            sqlx::query!(
                "DELETE FROM webhook_manga_overrides WHERE webhook_id = 0 AND manga_id = ?",
                manga_id,
            )
            .execute(&self.db)
            .await?;
        } else {
            sqlx::query!(
                "INSERT INTO webhook_manga_overrides (webhook_id, manga_id, enabled) \
                 VALUES (0, ?, FALSE) \
                 ON CONFLICT (webhook_id, manga_id) DO UPDATE SET enabled = FALSE",
                manga_id,
            )
            .execute(&self.db)
            .await?;
        }
        Ok(())
    }
}

// ── Validation helpers ────────────────────────────────────────────────────────

fn validate_url(url: &str) -> Result<()> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(ServiceError::Validation(
            "Webhook URL must start with http:// or https://".to_owned(),
        ));
    }
    Ok(())
}

fn validate_events_json(events: &str) -> Result<()> {
    let arr: serde_json::Value = serde_json::from_str(events)
        .map_err(|_| ServiceError::Validation("events must be a JSON array".to_owned()))?;
    if !arr.is_array() {
        return Err(ServiceError::Validation(
            "events must be a JSON array".to_owned(),
        ));
    }
    Ok(())
}
