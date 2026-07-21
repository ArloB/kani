use super::*;
use crate::ids::{MangaId, UserId};

impl AppService {
    /// Returns the public-facing settings snapshot (omits internal paths).
    pub async fn get_settings(&self) -> kani_shared::types::AppSettings {
        let s = self.settings.read().await;
        kani_shared::types::AppSettings {
            flaresolverr_url: s.flaresolverr_url.clone(),
            library_path: s.library_path.to_string_lossy().into_owned(),
            wasm_storage_path: s.wasm_storage_path.to_string_lossy().into_owned(),
            concurrent_page_downloads: s.concurrent_page_downloads,
            concurrent_manga_downloads: s.concurrent_manga_downloads,
            chapter_queue_size: s.chapter_queue_size,
            max_retries: s.max_retries,
            initial_retry_delay_ms: s.initial_retry_delay_ms,
            max_wasm_instances: s.max_wasm_instances,
            auto_scan: s.auto_scan,
            scan_interval_minutes: s.scan_interval_minutes,
            scan_exclude_completed: s.scan_exclude_completed,
            auto_download_category_id: s.auto_download_category_id,
            auto_download_category_ids: serde_json::from_str(&s.auto_download_category_ids)
                .unwrap_or_default(),
            default_tracking_enabled: s.default_tracking_enabled,
            http_request_logging: s.http_request_logging,
            browser_debug_logging: s.browser_debug_logging,
            registration_enabled: s.registration_enabled,
            cover_max_dimension: s.cover_max_dimension,
            email_enabled: s.email_enabled,
            email_provider: s.email_provider.clone(),
            email_provider_config: mask_email_config(&s.email_provider_config),
            email_from_address: s.email_from_address.clone(),
            app_url: s.app_url.clone(),
            password_reset_enabled: s.password_reset_enabled,
            email_verification_required: s.email_verification_required,
            first_run_complete: s.first_run_complete,
            scan_concurrency: s.scan_concurrency,
            per_source_download_concurrency: s.per_source_download_concurrency,
            trash_retention_days: s.trash_retention_days,
            audit_retention_days: s.audit_retention_days,
            audit_security_retention_days: s.audit_security_retention_days,
            disk_warn_threshold: s.disk_warn_threshold,
            thumbnail_formats: s.thumbnail_formats.clone(),
            max_login_attempts: s.max_login_attempts,
            max_ip_attempts: s.max_ip_attempts,
            login_lockout_seconds: s.login_lockout_seconds,
            session_timeout_secs: s.session_timeout_secs,
            tracker_auto_sync_enabled: s.tracker_auto_sync_enabled,
            tracker_sync_interval_hours: s.tracker_sync_interval_hours,
            max_concurrent_jobs: s.max_concurrent_jobs,
            db_maintenance_interval_hours: s.db_maintenance_interval_hours,
            db_vacuum_interval_hours: s.db_vacuum_interval_hours,
            audit_prune_interval_hours: s.audit_prune_interval_hours,
            trash_purge_interval_hours: s.trash_purge_interval_hours,
            integrity_quick_scrub_interval_hours: s.integrity_quick_scrub_interval_hours,
            integrity_deep_scrub_interval_hours: s.integrity_deep_scrub_interval_hours,
            scrub_on_startup: s.scrub_on_startup,
            browser_max_memory_mb: s.browser_max_memory_mb,
            browser_max_instances: s.browser_max_instances,
            update_check_enabled: s.update_check_enabled,
            error_reporting_enabled: s.error_reporting_enabled,
            browser_idle_timeout_s: s.browser_idle_timeout_s,
        }
    }

    pub async fn mark_first_run_complete(&self, user_id: UserId) -> Result<()> {
        sqlx::query!("UPDATE settings SET first_run_complete = 1 WHERE id = 'singleton'")
            .execute(&self.db)
            .await?;
        self.settings.write().await.first_run_complete = true;
        self.audit(Some(user_id), "settings.first_run_complete", None, None)
            .await;
        Ok(())
    }

    /// Validates, persists, and applies a settings update. Audits the action.
    pub async fn update_settings(
        &self,
        update: kani_shared::types::SettingsUpdate,
        user_id: UserId,
    ) -> Result<()> {
        use kani_shared::types::SettingsUpdate;
        match update {
            SettingsUpdate::Download(s) => {
                if s.concurrent_page_downloads < 1 || s.concurrent_page_downloads > 32 {
                    return Err(ServiceError::Validation(
                        "concurrent_page_downloads must be 1-32".into(),
                    ));
                }
                if s.concurrent_manga_downloads < 1 || s.concurrent_manga_downloads > 16 {
                    return Err(ServiceError::Validation(
                        "concurrent_manga_downloads must be 1-16".into(),
                    ));
                }
                if s.scan_concurrency < 1 || s.scan_concurrency > 32 {
                    return Err(ServiceError::Validation(
                        "scan_concurrency must be 1-32".into(),
                    ));
                }
                if s.per_source_download_concurrency < 1 || s.per_source_download_concurrency > 16 {
                    return Err(ServiceError::Validation(
                        "per_source_download_concurrency must be 1-16".into(),
                    ));
                }
                let cat_ids_json = serde_json::to_string(&s.auto_download_category_ids)
                    .unwrap_or_else(|_| "[]".to_string());
                sqlx::query!(
                    "UPDATE settings SET concurrent_page_downloads=?, concurrent_manga_downloads=?, \
                     chapter_queue_size=?, max_retries=?, initial_retry_delay_ms=?, \
                     auto_download_category_ids=?, scan_concurrency=?, \
                     per_source_download_concurrency=? WHERE id='singleton'",
                    s.concurrent_page_downloads,
                    s.concurrent_manga_downloads,
                    s.chapter_queue_size,
                    s.max_retries,
                    s.initial_retry_delay_ms,
                    cat_ids_json,
                    s.scan_concurrency,
                    s.per_source_download_concurrency,
                )
                .execute(&self.db)
                .await?;
                {
                    let mut settings = self.settings.write().await;
                    settings.concurrent_page_downloads = s.concurrent_page_downloads;
                    settings.concurrent_manga_downloads = s.concurrent_manga_downloads;
                    settings.chapter_queue_size = s.chapter_queue_size;
                    settings.max_retries = s.max_retries;
                    settings.initial_retry_delay_ms = s.initial_retry_delay_ms;
                    settings.auto_download_category_ids = cat_ids_json;
                    settings.scan_concurrency = s.scan_concurrency;
                    settings.per_source_download_concurrency = s.per_source_download_concurrency;
                }
                self.job_manager.invalidate_all_source_semaphores();
                self.audit(Some(user_id), "settings.update.download", None, None)
                    .await;
            }
            SettingsUpdate::Scan(s) => {
                if s.scan_interval_minutes < 5 {
                    return Err(ServiceError::Validation(
                        "scan_interval_minutes must be >= 5".into(),
                    ));
                }
                sqlx::query!(
                    "UPDATE settings SET auto_scan=?, scan_interval_minutes=?, scan_exclude_completed=? WHERE id='singleton'",
                    s.auto_scan,
                    s.scan_interval_minutes,
                    s.scan_exclude_completed
                )
                .execute(&self.db)
                .await?;
                let mut settings = self.settings.write().await;
                settings.auto_scan = s.auto_scan;
                settings.scan_interval_minutes = s.scan_interval_minutes;
                settings.scan_exclude_completed = s.scan_exclude_completed;
                self.audit(Some(user_id), "settings.update.scan", None, None)
                    .await;
            }
            SettingsUpdate::Tracking(s) => {
                if s.tracker_sync_interval_hours < 1 {
                    return Err(ServiceError::Validation(
                        "tracker_sync_interval_hours must be >= 1".into(),
                    ));
                }
                sqlx::query!(
                    "UPDATE settings SET default_tracking_enabled=?, tracker_auto_sync_enabled=?, \
                     tracker_sync_interval_hours=? WHERE id='singleton'",
                    s.default_tracking_enabled,
                    s.tracker_auto_sync_enabled,
                    s.tracker_sync_interval_hours
                )
                .execute(&self.db)
                .await?;
                {
                    let mut settings = self.settings.write().await;
                    settings.default_tracking_enabled = s.default_tracking_enabled;
                    settings.tracker_auto_sync_enabled = s.tracker_auto_sync_enabled;
                    settings.tracker_sync_interval_hours = s.tracker_sync_interval_hours;
                }
                self.audit(Some(user_id), "settings.update.tracking", None, None)
                    .await;
            }
            SettingsUpdate::Advanced(s) => {
                if s.browser_max_memory_mb < 64 || s.browser_max_memory_mb > 8192 {
                    return Err(ServiceError::Validation(
                        "browser_max_memory_mb must be 64-8192".into(),
                    ));
                }
                if s.browser_max_instances < 1 || s.browser_max_instances > 16 {
                    return Err(ServiceError::Validation(
                        "browser_max_instances must be 1-16".into(),
                    ));
                }
                if s.browser_idle_timeout_s < 10 || s.browser_idle_timeout_s > 3600 {
                    return Err(ServiceError::Validation(
                        "browser_idle_timeout_s must be 10-3600".into(),
                    ));
                }
                sqlx::query!(
                    "UPDATE settings SET flaresolverr_url=?, library_path=?, wasm_storage_path=?, \
                     max_wasm_instances=?, http_request_logging=?, browser_debug_logging=?, \
                     registration_enabled=?, cover_max_dimension=?, browser_max_memory_mb=?, \
                     browser_max_instances=?, browser_idle_timeout_s=?, \
                     update_check_enabled=?, error_reporting_enabled=? WHERE id='singleton'",
                    s.flaresolverr_url,
                    s.library_path,
                    s.wasm_storage_path,
                    s.max_wasm_instances,
                    s.http_request_logging,
                    s.browser_debug_logging,
                    s.registration_enabled,
                    s.cover_max_dimension,
                    s.browser_max_memory_mb,
                    s.browser_max_instances,
                    s.browser_idle_timeout_s,
                    s.update_check_enabled,
                    s.error_reporting_enabled,
                )
                .execute(&self.db)
                .await?;
                {
                    let mut settings = self.settings.write().await;
                    settings.flaresolverr_url = s.flaresolverr_url.clone();
                    settings.library_path = s.library_path.clone().into();
                    settings.wasm_storage_path = s.wasm_storage_path.clone().into();
                    settings.max_wasm_instances = s.max_wasm_instances;
                    settings.http_request_logging = s.http_request_logging;
                    settings.browser_debug_logging = s.browser_debug_logging;
                    settings.registration_enabled = s.registration_enabled;
                    settings.cover_max_dimension = s.cover_max_dimension;
                    settings.browser_max_memory_mb = s.browser_max_memory_mb;
                    settings.browser_max_instances = s.browser_max_instances;
                    settings.browser_idle_timeout_s = s.browser_idle_timeout_s;
                    settings.update_check_enabled = s.update_check_enabled;
                    settings.error_reporting_enabled = s.error_reporting_enabled;
                }
                kani_core::v8_process::set_v8_debug_logging(s.browser_debug_logging);
                kani_core::v8_process::set_v8_config(kani_core::v8_process::V8Config {
                    max_memory_mb: s.browser_max_memory_mb as u32,
                    max_instances: s.browser_max_instances as u32,
                    idle_timeout_s: s.browser_idle_timeout_s as u32,
                });
                let new_solver = if s.flaresolverr_url.is_empty() {
                    None
                } else {
                    Some(s.flaresolverr_url)
                };
                self.smart_client.update_solver_url(new_solver.clone());
                self.proxy_client.update_solver_url(new_solver);
                self.audit(Some(user_id), "settings.update.advanced", None, None)
                    .await;
            }
            SettingsUpdate::Email(s) => {
                // If the incoming config contains placeholder values, restore from DB.
                let config_plain = restore_masked_email_config(
                    &s.email_provider_config,
                    &self.db,
                    self.encryption.as_deref(),
                )
                .await?;
                // Encrypt before writing to DB; keep plaintext in memory.
                let config_to_db = crate::service::encryption::maybe_encrypt(
                    self.encryption.as_deref(),
                    &config_plain,
                );

                sqlx::query!(
                    "UPDATE settings SET email_enabled=?, email_provider=?, email_provider_config=?, \
                     email_from_address=?, app_url=?, password_reset_enabled=?, \
                     email_verification_required=? WHERE id='singleton'",
                    s.email_enabled,
                    s.email_provider,
                    config_to_db,
                    s.email_from_address,
                    s.app_url,
                    s.password_reset_enabled,
                    s.email_verification_required,
                )
                .execute(&self.db)
                .await?;

                {
                    let mut settings = self.settings.write().await;
                    settings.email_enabled = s.email_enabled;
                    settings.email_provider = s.email_provider.clone();
                    settings.email_provider_config = config_plain; // always plaintext in memory
                    settings.email_from_address = s.email_from_address;
                    settings.app_url = s.app_url;
                    settings.password_reset_enabled = s.password_reset_enabled;
                    settings.email_verification_required = s.email_verification_required;
                }

                self.rebuild_email_service().await;
                self.audit(Some(user_id), "settings.update.email", None, None)
                    .await;
            }
            SettingsUpdate::Maintenance(s) => {
                if s.trash_retention_days < 0
                    || s.audit_retention_days < 0
                    || s.audit_security_retention_days < 0
                {
                    return Err(ServiceError::Validation(
                        "retention days must be >= 0".into(),
                    ));
                }
                if !(0.0..=1.0).contains(&s.disk_warn_threshold) {
                    return Err(ServiceError::Validation(
                        "disk_warn_threshold must be between 0.0 and 1.0".into(),
                    ));
                }
                if s.integrity_quick_scrub_interval_hours < 1
                    || s.integrity_deep_scrub_interval_hours < 1
                {
                    return Err(ServiceError::Validation(
                        "scrub interval hours must be >= 1".into(),
                    ));
                }
                sqlx::query!(
                    "UPDATE settings SET trash_retention_days=?, audit_retention_days=?, \
                     audit_security_retention_days=?, disk_warn_threshold=?, thumbnail_formats=?, \
                     integrity_quick_scrub_interval_hours=?, \
                     integrity_deep_scrub_interval_hours=?, scrub_on_startup=? \
                     WHERE id='singleton'",
                    s.trash_retention_days,
                    s.audit_retention_days,
                    s.audit_security_retention_days,
                    s.disk_warn_threshold,
                    s.thumbnail_formats,
                    s.integrity_quick_scrub_interval_hours,
                    s.integrity_deep_scrub_interval_hours,
                    s.scrub_on_startup,
                )
                .execute(&self.db)
                .await?;
                {
                    let mut settings = self.settings.write().await;
                    settings.trash_retention_days = s.trash_retention_days;
                    settings.audit_retention_days = s.audit_retention_days;
                    settings.audit_security_retention_days = s.audit_security_retention_days;
                    settings.disk_warn_threshold = s.disk_warn_threshold;
                    settings.thumbnail_formats = s.thumbnail_formats;
                    settings.integrity_quick_scrub_interval_hours =
                        s.integrity_quick_scrub_interval_hours;
                    settings.integrity_deep_scrub_interval_hours =
                        s.integrity_deep_scrub_interval_hours;
                    settings.scrub_on_startup = s.scrub_on_startup;
                }
                self.audit(Some(user_id), "settings.update.maintenance", None, None)
                    .await;
            }
            SettingsUpdate::Security(s) => {
                if s.max_login_attempts < 1 || s.max_ip_attempts < 1 {
                    return Err(ServiceError::Validation(
                        "login attempt limits must be >= 1".into(),
                    ));
                }
                if s.login_lockout_seconds < 1 {
                    return Err(ServiceError::Validation(
                        "login_lockout_seconds must be >= 1".into(),
                    ));
                }
                if s.session_timeout_secs < 60 {
                    return Err(ServiceError::Validation(
                        "session_timeout_secs must be >= 60".into(),
                    ));
                }
                sqlx::query!(
                    "UPDATE settings SET max_login_attempts=?, max_ip_attempts=?, \
                     login_lockout_seconds=?, session_timeout_secs=? WHERE id='singleton'",
                    s.max_login_attempts,
                    s.max_ip_attempts,
                    s.login_lockout_seconds,
                    s.session_timeout_secs,
                )
                .execute(&self.db)
                .await?;
                {
                    let mut settings = self.settings.write().await;
                    settings.max_login_attempts = s.max_login_attempts;
                    settings.max_ip_attempts = s.max_ip_attempts;
                    settings.login_lockout_seconds = s.login_lockout_seconds;
                    settings.session_timeout_secs = s.session_timeout_secs;
                }
                self.audit(Some(user_id), "settings.update.security", None, None)
                    .await;
            }
            SettingsUpdate::Performance(s) => {
                if s.max_concurrent_jobs < 1 {
                    return Err(ServiceError::Validation(
                        "max_concurrent_jobs must be >= 1".into(),
                    ));
                }
                if s.db_maintenance_interval_hours < 1
                    || s.db_vacuum_interval_hours < 1
                    || s.audit_prune_interval_hours < 1
                    || s.trash_purge_interval_hours < 1
                {
                    return Err(ServiceError::Validation(
                        "interval hours must be >= 1".into(),
                    ));
                }
                sqlx::query!(
                    "UPDATE settings SET max_concurrent_jobs=?, db_maintenance_interval_hours=?, \
                     db_vacuum_interval_hours=?, audit_prune_interval_hours=?, \
                     trash_purge_interval_hours=? WHERE id='singleton'",
                    s.max_concurrent_jobs,
                    s.db_maintenance_interval_hours,
                    s.db_vacuum_interval_hours,
                    s.audit_prune_interval_hours,
                    s.trash_purge_interval_hours,
                )
                .execute(&self.db)
                .await?;
                {
                    let mut settings = self.settings.write().await;
                    settings.max_concurrent_jobs = s.max_concurrent_jobs;
                    settings.db_maintenance_interval_hours = s.db_maintenance_interval_hours;
                    settings.db_vacuum_interval_hours = s.db_vacuum_interval_hours;
                    settings.audit_prune_interval_hours = s.audit_prune_interval_hours;
                    settings.trash_purge_interval_hours = s.trash_purge_interval_hours;
                }
                self.audit(Some(user_id), "settings.update.performance", None, None)
                    .await;
            }
        }
        Ok(())
    }

    /// Toggles auto-scan on/off and returns the new value.
    pub async fn toggle_auto_scan(&self) -> Result<bool> {
        let new_val = !self.settings.read().await.auto_scan;
        sqlx::query!(
            "UPDATE settings SET auto_scan=? WHERE id='singleton'",
            new_val
        )
        .execute(&self.db)
        .await?;
        self.settings.write().await.auto_scan = new_val;
        Ok(new_val)
    }

    pub async fn toggle_auto_scan_manga(&self, manga_id: MangaId, enabled: bool) -> Result<()> {
        sqlx::query!("UPDATE manga SET auto_scan=? WHERE id=?", enabled, manga_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Runs WAL checkpoint + VACUUM and returns (before_bytes, after_bytes).
    pub async fn run_maintenance(&self) -> Result<(u64, u64)> {
        let db_path = std::path::Path::new("kani.db");
        let before = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.db)
            .await?;
        sqlx::query("VACUUM").execute(&self.db).await?;
        let after = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);
        Ok((before, after))
    }

    pub async fn mark_manga_seen(&self, user_id: UserId, manga_id: MangaId) -> Result<()> {
        sqlx::query!(
            "INSERT INTO user_manga_tracking (user_id, manga_id, last_seen_at) \
             VALUES (?1, ?2, CURRENT_TIMESTAMP) \
             ON CONFLICT (user_id, manga_id) DO UPDATE SET last_seen_at = CURRENT_TIMESTAMP",
            user_id,
            manga_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn update_manga_notes(&self, manga_id: MangaId, notes: Option<String>) -> Result<()> {
        sqlx::query!("UPDATE manga SET notes = ? WHERE id = ?", notes, manga_id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn send_test_email_to(&self, to: &str) -> std::result::Result<(), String> {
        let mailer = self
            .mailer()
            .await
            .ok_or_else(|| "Email is not configured or disabled.".to_string())?;
        let (subject, html) = crate::service::email_templates::test_email();
        mailer.send(to, &subject, &html).await
    }

    pub async fn toggle_auto_download(&self, manga_id: MangaId, enabled: bool) -> Result<()> {
        sqlx::query!(
            "UPDATE manga SET auto_download=? WHERE id=?",
            enabled,
            manga_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn toggle_download_all_preferred(
        &self,
        manga_id: MangaId,
        enabled: bool,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE manga SET download_all_preferred_only=? WHERE id=?",
            enabled,
            manga_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

const PLACEHOLDER: &str = "••••••";
const MASKED_KEYS: &[&str] = &[
    "password",
    "api_key",
    "secret",
    "client_secret",
    "access_token",
    "refresh_token",
    "token",
];

/// Returns a copy of the config JSON with credential values replaced by `PLACEHOLDER`.
pub(crate) fn mask_email_config(config_json: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(config_json) else {
        return config_json.to_string();
    };
    if let Some(obj) = v.as_object_mut() {
        for &key in MASKED_KEYS {
            if let Some(val) = obj.get_mut(key)
                && val.as_str().is_some_and(|s| !s.is_empty())
            {
                *val = serde_json::Value::String(PLACEHOLDER.to_string());
            }
        }
    }
    serde_json::to_string(&v).unwrap_or_else(|_| config_json.to_string())
}

/// If the incoming JSON has `PLACEHOLDER` for any credential key, substitute the stored DB value.
/// `cipher` is used to decrypt the stored value before substitution.
async fn restore_masked_email_config(
    incoming: &str,
    db: &sqlx::SqlitePool,
    cipher: Option<&crate::service::encryption::CredentialCipher>,
) -> Result<String> {
    let Ok(mut incoming_val) = serde_json::from_str::<serde_json::Value>(incoming) else {
        return Ok(incoming.to_string());
    };

    let has_placeholder = incoming_val.as_object().is_some_and(|obj| {
        MASKED_KEYS
            .iter()
            .any(|&k| obj.get(k).and_then(serde_json::Value::as_str) == Some(PLACEHOLDER))
    });

    if !has_placeholder {
        return Ok(incoming.to_string());
    }

    let stored_json: Option<String> =
        sqlx::query_scalar!("SELECT email_provider_config FROM settings WHERE id='singleton'")
            .fetch_optional(db)
            .await?;

    // Decrypt the stored config before doing placeholder substitution.
    let stored_plain = match stored_json.as_deref() {
        None | Some("") => String::new(),
        Some(raw) => crate::service::encryption::maybe_decrypt(cipher, raw).unwrap_or_else(|e| {
            tracing::warn!("Cannot decrypt stored email_provider_config for restore: {e}");
            raw.to_string()
        }),
    };

    let stored_val: serde_json::Value = if stored_plain.is_empty() {
        serde_json::Value::default()
    } else {
        serde_json::from_str(&stored_plain).unwrap_or_default()
    };

    if let (Some(incoming_obj), Some(stored_obj)) =
        (incoming_val.as_object_mut(), stored_val.as_object())
    {
        for &key in MASKED_KEYS {
            let is_placeholder =
                incoming_obj.get(key).and_then(serde_json::Value::as_str) == Some(PLACEHOLDER);
            if is_placeholder {
                let real = stored_obj
                    .get(key)
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::String(String::new()));
                incoming_obj.insert(key.to_string(), real);
            }
        }
    }

    Ok(serde_json::to_string(&incoming_val).unwrap_or_else(|_| incoming.to_string()))
}
