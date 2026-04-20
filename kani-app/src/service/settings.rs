use super::*;

impl AppService {
    /// Returns the public-facing settings snapshot (omits internal paths).
    pub async fn get_settings(&self) -> kani_shared::types::AppSettings {
        let s = self.settings.read().await;
        kani_shared::types::AppSettings {
            flaresolverr_url: s.flaresolverr_url.clone(),
            library_path: s.library_path.to_string_lossy().into_owned(),
            concurrent_page_downloads: s.concurrent_page_downloads,
            concurrent_manga_downloads: s.concurrent_manga_downloads,
            chapter_queue_size: s.chapter_queue_size,
            max_retries: s.max_retries,
            initial_retry_delay_ms: s.initial_retry_delay_ms,
            max_wasm_instances: s.max_wasm_instances,
            auto_scan: s.auto_scan,
            scan_interval_minutes: s.scan_interval_minutes,
            default_tracking_enabled: s.default_tracking_enabled,
        }
    }

    /// Validates, persists, and applies a settings update. Audits the action.
    pub async fn update_settings(
        &self,
        update: kani_shared::types::SettingsUpdate,
        user_id: i64,
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
                sqlx::query!(
                    "UPDATE settings SET concurrent_page_downloads=?, concurrent_manga_downloads=?, \
                     chapter_queue_size=?, max_retries=?, initial_retry_delay_ms=? WHERE id='singleton'",
                    s.concurrent_page_downloads,
                    s.concurrent_manga_downloads,
                    s.chapter_queue_size,
                    s.max_retries,
                    s.initial_retry_delay_ms
                )
                .execute(&self.db)
                .await?;
                let mut settings = self.settings.write().await;
                settings.concurrent_page_downloads = s.concurrent_page_downloads;
                settings.concurrent_manga_downloads = s.concurrent_manga_downloads;
                settings.chapter_queue_size = s.chapter_queue_size;
                settings.max_retries = s.max_retries;
                settings.initial_retry_delay_ms = s.initial_retry_delay_ms;
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
                    "UPDATE settings SET auto_scan=?, scan_interval_minutes=? WHERE id='singleton'",
                    s.auto_scan,
                    s.scan_interval_minutes
                )
                .execute(&self.db)
                .await?;
                let mut settings = self.settings.write().await;
                settings.auto_scan = s.auto_scan;
                settings.scan_interval_minutes = s.scan_interval_minutes;
                self.audit(Some(user_id), "settings.update.scan", None, None)
                    .await;
            }
            SettingsUpdate::Tracking(s) => {
                sqlx::query!(
                    "UPDATE settings SET default_tracking_enabled=? WHERE id='singleton'",
                    s.default_tracking_enabled
                )
                .execute(&self.db)
                .await?;
                self.settings.write().await.default_tracking_enabled =
                    s.default_tracking_enabled;
                self.audit(Some(user_id), "settings.update.tracking", None, None)
                    .await;
            }
            SettingsUpdate::Advanced(s) => {
                sqlx::query!(
                    "UPDATE settings SET flaresolverr_url=?, library_path=?, max_wasm_instances=? \
                     WHERE id='singleton'",
                    s.flaresolverr_url,
                    s.library_path,
                    s.max_wasm_instances
                )
                .execute(&self.db)
                .await?;
                {
                    let mut settings = self.settings.write().await;
                    settings.flaresolverr_url = s.flaresolverr_url.clone();
                    settings.library_path = s.library_path.clone().into();
                    settings.max_wasm_instances = s.max_wasm_instances;
                }
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

    pub async fn toggle_auto_download(&self, manga_id: i64, enabled: bool) -> Result<()> {
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
        manga_id: i64,
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
