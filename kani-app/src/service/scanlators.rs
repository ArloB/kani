use super::*;

impl AppService {
    pub async fn get_scanlator_prefs(
        &self,
        manga_id: i64,
    ) -> Result<Vec<kani_shared::types::ScanlatorPreference>> {
        // SQLite stores bool as INTEGER; we map manually to avoid trait bound issues.
        let rows = sqlx::query!(
            "SELECT id, manga_id, scanlator, priority, blocked FROM scanlator_preferences \
             WHERE manga_id=? ORDER BY priority DESC",
            manga_id
        )
        .fetch_all(&self.db)
        .await
        .map_err(ServiceError::Db)?;
        let prefs = rows
            .into_iter()
            .map(|r| kani_shared::types::ScanlatorPreference {
                id: r.id,
                manga_id: r.manga_id,
                scanlator: r.scanlator,
                priority: r.priority,
                blocked: r.blocked != 0,
            })
            .collect();
        Ok(prefs)
    }

    pub async fn set_scanlator_pref(
        &self,
        manga_id: i64,
        scanlator: &str,
        priority: i64,
        blocked: bool,
    ) -> Result<()> {
        let blocked_int = blocked as i64;
        sqlx::query!(
            "INSERT INTO scanlator_preferences (manga_id, scanlator, priority, blocked) VALUES (?,?,?,?) \
             ON CONFLICT (manga_id, scanlator) DO UPDATE SET priority = excluded.priority, blocked = excluded.blocked",
            manga_id,
            scanlator,
            priority,
            blocked_int
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn delete_scanlator_pref(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM scanlator_preferences WHERE id=?", id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    /// Returns the distinct scanlator names for chapters of a manga.
    pub async fn get_chapter_scanlators(&self, manga_id: i64) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar!(
            "SELECT DISTINCT scanlator FROM chapters WHERE manga_id = ? AND scanlator IS NOT NULL ORDER BY scanlator",
            manga_id
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows.into_iter().flatten().collect())
    }

    /// Returns the distinct language codes for chapters of a manga.
    pub async fn get_chapter_languages(&self, manga_id: i64) -> Result<Vec<String>> {
        let rows = sqlx::query_scalar!(
            "SELECT DISTINCT language FROM chapters WHERE manga_id = ? ORDER BY language",
            manga_id
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows)
    }

    /// Returns the scanlator mode for a manga, defaulting to `'priority'`.
    pub async fn get_scanlator_mode(&self, manga_id: i64) -> Result<String> {
        let mode = sqlx::query_scalar!(
            "SELECT COALESCE(scanlator_mode, 'priority') FROM manga WHERE id = ?",
            manga_id
        )
        .fetch_optional(&self.db)
        .await?
        .unwrap_or_else(|| "priority".into());
        Ok(mode)
    }

    /// Sets the scanlator mode for a manga ('priority' or 'whitelist').
    pub async fn set_scanlator_mode(&self, manga_id: i64, mode: &str) -> Result<()> {
        if mode != "priority" && mode != "whitelist" {
            return Err(ServiceError::Validation(
                "scanlator_mode must be 'priority' or 'whitelist'".into(),
            ));
        }
        sqlx::query!(
            "UPDATE manga SET scanlator_mode = ? WHERE id = ?",
            mode,
            manga_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
}
