use super::*;

impl AppService {
    pub async fn set_chapter_progress(&self, user_id: i64, chapter_id: i64, page: i64) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_chapter_tracking (user_id, chapter_id, last_page_read, is_read, last_read_at)
            VALUES (
                ?1,
                ?2,
                ?3,
                COALESCE(?3 >= ((SELECT page_count FROM chapters WHERE id = ?2) - 1), false),
                datetime('now')
            )
            ON CONFLICT (user_id, chapter_id) DO UPDATE SET
                last_page_read = excluded.last_page_read,
                is_read = user_chapter_tracking.is_read OR excluded.is_read,
                last_read_at = datetime('now')
            "#,
            user_id,
            chapter_id,
            page
        )
        .execute(&self.db)
        .await?;

        self.cache.invalidate_stats(user_id);
        Ok(())
    }

    pub async fn set_chapter_read_status(&self, user_id: i64, chapter_ids: Vec<i64>, is_read: bool) -> Result<()> {
        let json_chapter_ids = serde_json::to_string(&chapter_ids)
            .map_err(|e| ServiceError::Internal(e.to_string()))?;

        sqlx::query!(
            r#"
            INSERT INTO user_chapter_tracking (user_id, chapter_id, last_page_read, is_read, last_read_at)
            SELECT ?1, CAST(value AS INTEGER), 0, ?3,
                   CASE WHEN ?3 THEN datetime('now') ELSE NULL END
            FROM json_each(?2)
            WHERE 1=1
            ON CONFLICT (user_id, chapter_id) DO UPDATE SET
                is_read = excluded.is_read,
                last_page_read = CASE WHEN excluded.is_read THEN user_chapter_tracking.last_page_read ELSE 0 END,
                last_read_at = CASE WHEN excluded.is_read THEN datetime('now') ELSE user_chapter_tracking.last_read_at END
            "#,
            user_id,
            json_chapter_ids,
            is_read
        )
        .execute(&self.db)
        .await?;

        self.cache.invalidate_stats(user_id);
        Ok(())
    }

    pub async fn set_manga_status(&self, user_id: i64, manga_id: i64, status: kani_shared::types::MangaTrackingStatus) -> Result<()> {
        let status_int = status as i64;        
        
        sqlx::query!(
            r#"
            INSERT INTO user_manga_tracking (user_id, manga_id, status) 
            VALUES (?1, ?2, ?3)
            ON CONFLICT (user_id, manga_id) DO UPDATE SET 
                status = excluded.status
            "#,
            user_id,
            manga_id,
            status_int
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn get_chapter_progress(&self, user_id: i64, chapter_id: i64) -> Result<Option<(i64, bool)>> {
        let row = sqlx::query!(
            r#"
            SELECT last_page_read, is_read as "is_read: bool"
            FROM user_chapter_tracking
            WHERE user_id = ? AND chapter_id = ?
            "#,
            user_id,
            chapter_id
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(row.map(|r| (r.last_page_read, r.is_read)))
    }

    pub async fn set_manga_tracking_enabled(
        &self,
        user_id: i64,
        manga_id: i64,
        enabled: bool,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_manga_tracking (user_id, manga_id, tracking_enabled)
            VALUES (?1, ?2, ?3)
            ON CONFLICT (user_id, manga_id) DO UPDATE SET
                tracking_enabled = excluded.tracking_enabled
            "#,
            user_id,
            manga_id,
            enabled
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn set_manga_notify(&self, user_id: i64, manga_id: i64, notify: bool) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_manga_tracking (user_id, manga_id, notify_new_chapters)
            VALUES (?1, ?2, ?3)
            ON CONFLICT (user_id, manga_id) DO UPDATE SET
                notify_new_chapters = excluded.notify_new_chapters
            "#,
            user_id,
            manga_id,
            notify
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn set_reading_direction(&self, user_id: i64, manga_id: i64, direction: &str) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_manga_tracking (user_id, manga_id, reading_direction)
            VALUES (?1, ?2, ?3)
            ON CONFLICT (user_id, manga_id) DO UPDATE SET
                reading_direction = excluded.reading_direction
            "#,
            user_id,
            manga_id,
            direction
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn get_manga_tracking(&self, user_id: i64, manga_id: i64) -> Result<kani_shared::types::MangaTracking> {
        let tracking = sqlx::query!(
            r#"
            SELECT status, score,
                   tracking_enabled as "tracking_enabled: bool",
                   COALESCE(notify_new_chapters, TRUE) as "notify_new_chapters: bool",
                   COALESCE(reading_direction, 'rtl') as "reading_direction: String"
            FROM user_manga_tracking
            WHERE user_id = ? AND manga_id = ?
            "#,
            user_id,
            manga_id
        )
        .fetch_optional(&self.db)
        .await?;

        // Count distinct chapter numbers read (coalesces multiple scanlator versions).
        let chapters_read = sqlx::query_scalar!(
            r#"
            SELECT COUNT(DISTINCT c.chapter_number) as "count: i64"
            FROM user_chapter_tracking uct
            JOIN chapters c ON c.id = uct.chapter_id
            WHERE uct.user_id = ? AND c.manga_id = ? AND uct.is_read = true
            "#,
            user_id,
            manga_id
        )
        .fetch_one(&self.db)
        .await?;

        // Count distinct chapter numbers (not rows) so scanlator dupes don't inflate total.
        let total_chapters = sqlx::query_scalar!(
            r#"SELECT COUNT(DISTINCT chapter_number) as "count: i64" FROM chapters WHERE manga_id = ?"#,
            manga_id
        )
        .fetch_one(&self.db)
        .await?;

        let (status, score, tracking_enabled, notify_new_chapters, reading_direction) = match tracking {
            Some(row) => {
                let status = match row.status {
                    0 => Some(kani_shared::types::MangaTrackingStatus::Reading),
                    1 => Some(kani_shared::types::MangaTrackingStatus::OnHold),
                    2 => Some(kani_shared::types::MangaTrackingStatus::Dropped),
                    3 => Some(kani_shared::types::MangaTrackingStatus::PlanToRead),
                    4 => Some(kani_shared::types::MangaTrackingStatus::Completed),
                    5 => Some(kani_shared::types::MangaTrackingStatus::Rereading),
                    _ => None,
                };
                (status, row.score, row.tracking_enabled, row.notify_new_chapters.unwrap_or(true), row.reading_direction)
            }
            None => {
                let default = self.settings.read().await.default_tracking_enabled;
                (None, None, default, true, None)
            }
        };

        Ok(kani_shared::types::MangaTracking {
            status,
            score,
            chapters_read,
            total_chapters,
            tracking_enabled,
            notify_new_chapters,
            reading_direction: reading_direction.unwrap_or_else(|| "rtl".to_string()),
        })
    }

    pub async fn set_manga_score(&self, user_id: i64, manga_id: i64, score: f64) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_manga_tracking (user_id, manga_id, score)
            VALUES (?1, ?2, ?3)
            ON CONFLICT (user_id, manga_id) DO UPDATE SET
                score = excluded.score
            "#,
            user_id,
            manga_id,
            score
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Find the next chapter to read for a manga:
    ///    1. The lowest in-progress chapter (started but not finished).
    ///    2. Otherwise, the lowest unread chapter number, picking the preferred scanlator version.
    ///
    /// Returns `None` when all chapters are read or there are no chapters.
    pub async fn get_continue_reading_chapter(
        &self,
        user_id: i64,
        manga_id: i64,
    ) -> Result<Option<kani_shared::types::ContinueReadingChapter>> {
        // Step 1: in-progress chapter (is_read=false but last_page_read > 0).
        let in_progress = sqlx::query!(
            r#"
            SELECT c.id as "id: i64", c.chapter_number as "chapter_number: f64",
                   uct.last_page_read as "last_page_read: i64"
            FROM chapters c
            JOIN manga m ON m.id = c.manga_id
            JOIN user_chapter_tracking uct ON uct.chapter_id = c.id
            LEFT JOIN scanlator_preferences sp 
                ON sp.manga_id = c.manga_id AND sp.scanlator = c.scanlator
            WHERE c.manga_id = ? AND uct.user_id = ?
              AND uct.is_read = false AND uct.last_page_read > 0
              AND c.download_status = 2
              AND (
                  (m.scanlator_mode = 'priority' AND (sp.blocked IS NULL OR sp.blocked = 0)) OR
                  (m.scanlator_mode = 'whitelist' AND sp.id IS NOT NULL)
              )
            ORDER BY c.chapter_number ASC
            LIMIT 1
            "#,
            manga_id,
            user_id,
        )
        .fetch_optional(&self.db)
        .await?;

        if let Some(row) = in_progress {
            return Ok(Some(kani_shared::types::ContinueReadingChapter {
                chapter_id: row.id,
                chapter_number: row.chapter_number,
                last_page: row.last_page_read,
            }));
        }

        // Step 2: lowest unread chapter number (considering all scanlator versions).
        // A number is "read" if ANY version of it is marked read for this user.
        let next_number = sqlx::query_scalar!(
            r#"
            SELECT MIN(c.chapter_number) as "num: f64"
            FROM chapters c
            JOIN manga m ON m.id = c.manga_id
            LEFT JOIN scanlator_preferences sp 
                ON sp.manga_id = c.manga_id AND sp.scanlator = c.scanlator
            WHERE c.manga_id = ?
              AND c.download_status = 2
              AND (
                  (m.scanlator_mode = 'priority' AND (sp.blocked IS NULL OR sp.blocked = 0)) OR
                  (m.scanlator_mode = 'whitelist' AND sp.id IS NOT NULL)
              )
              AND NOT EXISTS (
                  SELECT 1 FROM chapters c2
                  JOIN user_chapter_tracking uct2 ON uct2.chapter_id = c2.id
                  WHERE c2.manga_id = c.manga_id
                    AND c2.chapter_number = c.chapter_number
                    AND uct2.user_id = ?
                    AND uct2.is_read = true
              )
            "#,
            manga_id,
            user_id,
        )
        .fetch_one(&self.db)
        .await?;

        let Some(chapter_number) = next_number else {
            return Ok(None); // all chapters read
        };

        // Step 3: pick the preferred scanlator version for that number.
        let chapter = sqlx::query!(
            r#"
            SELECT c.id as "id: i64"
            FROM chapters c
            JOIN manga m ON m.id = c.manga_id
            LEFT JOIN scanlator_preferences sp
                ON sp.manga_id = c.manga_id AND sp.scanlator = c.scanlator
            WHERE c.manga_id = ? AND c.chapter_number = ?
              AND c.download_status = 2
              AND (
                  (m.scanlator_mode = 'priority' AND (sp.blocked IS NULL OR sp.blocked = 0)) OR
                  (m.scanlator_mode = 'whitelist' AND sp.id IS NOT NULL)
              )
            ORDER BY COALESCE(sp.priority, -1) DESC
            LIMIT 1
            "#,
            manga_id,
            chapter_number,
        )
        .fetch_optional(&self.db)
        .await?;

        Ok(chapter.map(|r| kani_shared::types::ContinueReadingChapter {
            chapter_id: r.id,
            chapter_number,
            last_page: 0,
        }))
    }

    /// Returns all chapter IDs for a manga where chapter_number <= the given number.
    /// Used by "mark as read/unread up to here".
    pub async fn get_chapters_up_to(&self, manga_id: i64, chapter_number: f64) -> Result<Vec<i64>> {
        let ids = sqlx::query_scalar!(
            r#"SELECT id as "id: i64" FROM chapters WHERE manga_id = ? AND chapter_number <= ?"#,
            manga_id,
            chapter_number,
        )
        .fetch_all(&self.db)
        .await?;
        Ok(ids)
    }
}