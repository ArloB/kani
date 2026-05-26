use super::*;

impl AppService {
    pub async fn download_chapter(&self, chapter_id: i64) -> Result<()> {
        let claimed = sqlx::query!(
            "UPDATE chapters SET download_status = 1 \
             WHERE id = ? AND download_status = 0",
            chapter_id
        )
        .execute(&self.db)
        .await?;

        if claimed.rows_affected() == 0 {
            return Err(ServiceError::Internal(format!(
                "Chapter {chapter_id} is already downloaded or in progress."
            )));
        }

        self.enqueue_claimed_chapter(chapter_id).await
    }

    pub async fn enqueue_claimed_chapter(&self, chapter_id: i64) -> Result<()> {
        let result = async {
            let task = self.build_download_task(chapter_id).await?;
            self.downloader
                .queue_chapter(task)
                .await
                .map_err(ServiceError::Core)?;
            Ok(())
        }
        .await;

        if result.is_err() {
            let _ = sqlx::query!(
                "UPDATE chapters SET download_status = 0 WHERE id = ?",
                chapter_id
            )
            .execute(&self.db)
            .await;
        }

        result
    }

    pub async fn download_all_chapters(&self, manga_id: i64) -> Result<()> {
        let candidate_ids: Vec<i64> = sqlx::query_scalar!(
            "SELECT id FROM chapters \
             WHERE manga_id = ? AND download_status = 0 AND is_orphaned = 0",
            manga_id
        )
        .fetch_all(&self.db)
        .await?;

        if candidate_ids.is_empty() {
            tracing::info!(
                "download_all_chapters: no undownloaded chapters for manga {}",
                manga_id
            );
            return Ok(());
        }

        let preferred_only = sqlx::query_scalar!(
            "SELECT download_all_preferred_only FROM manga WHERE id = ?",
            manga_id
        )
        .fetch_optional(&self.db)
        .await?
        .unwrap_or(true);

        let preferred_ids = if preferred_only {
            let filtered = self.filter_chapters_by_rules(manga_id, candidate_ids).await;
            if filtered.is_empty() {
                tracing::info!(
                    "download_all_chapters: all candidates were filtered out for manga {}",
                    manga_id
                );
                return Ok(());
            }
            filtered
        } else {
            candidate_ids
        };

        // Claim only the preferred chapters atomically; skip any that were
        // already grabbed by a concurrent request.
        let mut claimed_ids = Vec::with_capacity(preferred_ids.len());
        for id in preferred_ids {
            let res = sqlx::query_scalar!(
                "UPDATE chapters SET download_status = 1 \
                 WHERE id = ? AND download_status = 0 \
                 RETURNING id",
                id
            )
            .fetch_optional(&self.db)
            .await?;
            if let Some(claimed_id) = res {
                claimed_ids.push(claimed_id);
            }
        }

        if claimed_ids.is_empty() {
            tracing::info!(
                "download_all_chapters: no undownloaded chapters for manga {}",
                manga_id
            );
            return Ok(());
        }

        let mut tasks: FuturesUnordered<_> = claimed_ids
            .into_iter()
            .map(|chapter_id| {
                let this = self.clone();
                async move { (chapter_id, this.enqueue_claimed_chapter(chapter_id).await) }
            })
            .collect();

        while let Some((chapter_id, result)) = tasks.next().await {
            if let Err(e) = result {
                tracing::error!(
                    "Failed to enqueue chapter {} (manga {}): {}",
                    chapter_id,
                    manga_id,
                    e
                );
            }
        }

        Ok(())
    }

    pub async fn delete_downloaded(&self, chapter_id: i64) -> Result<()> {
        let (cbz_path, ..) = self.chapter_cbz_path(chapter_id).await?;

        if let Err(e) = tokio::fs::remove_file(&cbz_path).await {
            tracing::error!("Failed to remove chapter file: {}", e);
        }

        let is_orphaned: bool =
            sqlx::query_scalar!("SELECT is_orphaned FROM chapters WHERE id = ?", chapter_id)
                .fetch_one(&self.db)
                .await?;

        if is_orphaned {
            let _ = sqlx::query!("DELETE FROM chapters WHERE id = ?", chapter_id)
                .execute(&self.db)
                .await;
        } else {
            let _ = sqlx::query!(
                "UPDATE chapters SET download_status = 0 WHERE id = ?",
                chapter_id
            )
            .execute(&self.db)
            .await;
        }

        Ok(())
    }

    /// Cancels an in-progress download and resets the chapter's download_status.
    pub async fn cancel_download(&self, chapter_id: i64) -> Result<()> {
        let was_cancelled = self.downloader.cancel_download(chapter_id).await;
        if was_cancelled {
            sqlx::query!(
                "UPDATE chapters SET download_status = 0 WHERE id = ? AND download_status = 1",
                chapter_id
            )
            .execute(&self.db)
            .await?;
        }
        Ok(())
    }

    /// Cancels all queued or in-progress downloads for a manga.
    pub async fn cancel_all_downloads(&self, manga_id: i64) -> Result<()> {
        let chapter_ids: Vec<i64> = sqlx::query_scalar!(
            "SELECT id FROM chapters WHERE manga_id = ? AND download_status = 1",
            manga_id
        )
        .fetch_all(&self.db)
        .await?;

        for id in chapter_ids {
            let was_cancelled = self.downloader.cancel_download(id).await;
            if was_cancelled {
                let _ = sqlx::query!(
                    "UPDATE chapters SET download_status = 0 WHERE id = ? AND download_status = 1",
                    id
                )
                .execute(&self.db)
                .await;
            }
        }
        Ok(())
    }

    /// Cancels all in-progress downloads across all manga.
    pub async fn cancel_all_global_downloads(&self) -> Result<()> {
        let chapter_ids: Vec<i64> =
            sqlx::query_scalar!("SELECT id FROM chapters WHERE download_status = 1")
                .fetch_all(&self.db)
                .await?;

        for id in chapter_ids {
            let was_cancelled = self.downloader.cancel_download(id).await;
            if was_cancelled {
                let _ = sqlx::query!(
                    "UPDATE chapters SET download_status = 0 WHERE id = ? AND download_status = 1",
                    id
                )
                .execute(&self.db)
                .await;
            }
        }
        Ok(())
    }

    pub async fn build_download_task(&self, chapter_id: i64) -> Result<DownloadTask> {
        let record = sqlx::query!(
            "SELECT
                c.source_chapter_id, c.name, c.chapter_number, c.volume, c.language,
                m.id as manga_id, m.source_id, m.source_manga_id, m.name as manga_name,
                m.description, s.base_url,
                (SELECT GROUP_CONCAT(p.name, ', ')
                FROM manga_people mp JOIN people p ON mp.person_id = p.id
                WHERE mp.manga_id = m.id and role = 'author') as authors,
                (SELECT GROUP_CONCAT(p.name, ', ')
                FROM manga_people mp JOIN people p ON mp.person_id = p.id
                WHERE mp.manga_id = m.id and role = 'artist') as artists,
                (SELECT GROUP_CONCAT(t.name, ', ')
                FROM manga_tags mt JOIN tags t ON mt.tag_id = t.id
                WHERE mt.manga_id = m.id) as tags
            FROM chapters c
            JOIN manga m ON c.manga_id = m.id
            JOIN sources s ON m.source_id = s.id
            WHERE c.id = ?",
            chapter_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| {
            ServiceError::NotFound(format!(
                "Chapter {chapter_id} not found (deleted after claim)"
            ))
        })?;

        let source_manager = {
            let sources = self.sources.read().await;
            sources.get(&record.source_id).cloned().ok_or_else(|| {
                ServiceError::NotFound(format!("Source {} not found", record.source_id))
            })?
        };

        let library_path = self.settings.read().await.library_path.clone();
        let name = chapter_name(record.volume, record.chapter_number, record.name.clone());
        let save_path = library_path.join(format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&record.manga_name),
            record.manga_id
        ));

        let comic_info = kani_core::comic_info::ComicInfo {
            xmlns_xsi: "http://www.w3.org/2001/XMLSchema-instance",
            series: record.manga_name.clone(),
            title: record.name,
            number: record.chapter_number,
            volume: record.volume,
            summary: record.description,
            language_iso: Some(record.language),
            writer: record.authors,
            penciller: record.artists,
            genre: record.tags,
            web: Some(format!("{}/{}", record.base_url, record.source_manga_id)),
            pages: None, // populated by create_cbz after spread detection
        };

        Ok(DownloadTask {
            chapter_id,
            manga_id: record.manga_id,
            manga_title: record.manga_name.clone(),
            source_manager,
            source_manga_id: record.source_manga_id,
            source_chapter_id: record.source_chapter_id,
            name,
            library_path,
            save_path,
            comic_info: Some(comic_info),
        })
    }

    pub async fn get_download_rules(
        &self,
        manga_id: i64,
    ) -> Result<Vec<kani_shared::types::DownloadRule>> {
        let rows = sqlx::query_as::<_, crate::models::DownloadRuleRow>(
            "SELECT id, manga_id, rule_type, value FROM download_rules WHERE manga_id=? ORDER BY priority, id",
        )
        .bind(manga_id)
        .fetch_all(&self.db)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|r| kani_shared::types::DownloadRule::try_from(r).ok())
            .collect())
    }

    /// Inserts a download rule and returns the new row id.
    pub async fn add_download_rule(
        &self,
        manga_id: i64,
        kind: kani_shared::types::DownloadRuleKind,
    ) -> Result<i64> {
        use kani_shared::types::DownloadRuleKind::*;
        // Rules with a string value need validation; others use a derived string.
        let (rule_type, value): (&str, String) = match &kind {
            LanguageInclude(v) => ("language_include", v.clone()),
            LanguageExclude(v) => ("language_exclude", v.clone()),
            TitleContains(v) => ("title_contains", v.clone()),
            TitleExcludes(v) => ("title_excludes", v.clone()),
            ChapterNumberMin(n) => ("chapter_number_min", n.to_string()),
            ChapterNumberMax(n) => ("chapter_number_max", n.to_string()),
            ExcludeFractional => ("exclude_fractional", String::new()),
            MaxAgeDays(n) => ("max_age_days", n.to_string()),
            PublishedAfter(ts) => ("published_after", ts.to_string()),
        };
        // String-valued rules must be non-empty.
        match &kind {
            LanguageInclude(_) | LanguageExclude(_) | TitleContains(_) | TitleExcludes(_)
                if value.trim().is_empty() =>
            {
                return Err(ServiceError::Validation(
                    "Rule value cannot be empty".into(),
                ));
            }
            _ => {}
        }
        let id = sqlx::query_scalar!(
            "INSERT INTO download_rules (manga_id, rule_type, value) VALUES (?,?,?) RETURNING id",
            manga_id,
            rule_type,
            value
        )
        .fetch_one(&self.db)
        .await?;
        Ok(id)
    }

    pub async fn delete_download_rule(&self, id: i64) -> Result<()> {
        sqlx::query!("DELETE FROM download_rules WHERE id=?", id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn update_download_rule(
        &self,
        id: i64,
        kind: kani_shared::types::DownloadRuleKind,
    ) -> Result<()> {
        use kani_shared::types::DownloadRuleKind::*;
        let (rule_type, value): (&str, String) = match &kind {
            LanguageInclude(v) => ("language_include", v.clone()),
            LanguageExclude(v) => ("language_exclude", v.clone()),
            TitleContains(v) => ("title_contains", v.clone()),
            TitleExcludes(v) => ("title_excludes", v.clone()),
            ChapterNumberMin(n) => ("chapter_number_min", n.to_string()),
            ChapterNumberMax(n) => ("chapter_number_max", n.to_string()),
            ExcludeFractional => ("exclude_fractional", String::new()),
            MaxAgeDays(n) => ("max_age_days", n.to_string()),
            PublishedAfter(ts) => ("published_after", ts.to_string()),
        };
        match &kind {
            LanguageInclude(_) | LanguageExclude(_) | TitleContains(_) | TitleExcludes(_)
                if value.trim().is_empty() =>
            {
                return Err(ServiceError::Validation(
                    "Rule value cannot be empty".into(),
                ));
            }
            _ => {}
        }
        sqlx::query("UPDATE download_rules SET rule_type=?, value=? WHERE id=?")
            .bind(rule_type)
            .bind(value)
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn reorder_download_rules(&self, manga_id: i64, ordered_ids: Vec<i64>) -> Result<()> {
        for (priority, id) in ordered_ids.iter().enumerate() {
            let p = priority as i64;
            sqlx::query("UPDATE download_rules SET priority=? WHERE id=? AND manga_id=?")
                .bind(p)
                .bind(id)
                .bind(manga_id)
                .execute(&self.db)
                .await?;
        }
        Ok(())
    }

    /// Returns the N most recently downloaded chapters (download_status = 2).
    pub async fn get_download_history(&self, limit: i64) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query!(
            "SELECT c.id, c.name, c.chapter_number, c.volume, c.manga_id, m.name as manga_title, c.downloaded_at
             FROM chapters c
             JOIN manga m ON m.id = c.manga_id
             WHERE c.download_status = 2
             ORDER BY c.downloaded_at DESC
             LIMIT ?",
            limit
        )
        .fetch_all(&self.db)
        .await?;

        let items = rows
            .into_iter()
            .map(|r| {
                let formatted_name = super::chapter_name(r.volume, r.chapter_number, r.name);
                serde_json::json!({
                    "id":           r.id,
                    "name":         formatted_name,
                    "chapterNumber": r.chapter_number,
                    "mangaId":      r.manga_id,
                    "mangaTitle":   r.manga_title,
                    "downloadedAt": r.downloaded_at.map(|t| t.unix_timestamp_nanos() / 1_000_000),
                    "status":       "completed",
                })
            })
            .collect();
        Ok(items)
    }
}
