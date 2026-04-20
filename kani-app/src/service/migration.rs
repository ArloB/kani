use super::*;

pub(super) struct MigrationContext {
    pub new_details: wit_types::MangaInfo,
    pub target_chapters: Vec<wit_types::ChapterInfo>,
    pub matched: Vec<(i64, String)>,
    pub orphaned_ids: Vec<i64>,
    pub unmatched_new: Vec<wit_types::ChapterInfo>,
    pub downloaded_orphan_ids: Vec<i64>,
}

impl AppService {
    pub(super) async fn resolve_migration_context(
        &self,
        manga_db_id: i64,
        target_source_id: i64,
        target_source_manga_id: &str,
    ) -> Result<MigrationContext> {
        let conflict = sqlx::query_scalar!(
            "SELECT id FROM manga WHERE source_id = ? AND source_manga_id = ?",
            target_source_id,
            target_source_manga_id
        )
        .fetch_optional(&self.db)
        .await?;

        if conflict.is_some() {
            return Err(ServiceError::Conflict(
                "Target manga is already in your library from this source".to_string(),
            ));
        }

        let raw = self
            .get_manga_details(target_source_id, target_source_manga_id)
            .await?;
        let new_details: wit_types::MangaInfo = serde_json::from_str(&raw)
            .map_err(|e| ServiceError::Internal(format!("Failed to parse manga details: {e}")))?;

        let target_chapters = self
            .fetch_all_chapter_pages(target_source_id, target_source_manga_id)
            .await?;

        let existing_chapters = sqlx::query!(
            "SELECT id, chapter_number, download_status FROM chapters WHERE manga_id = ?",
            manga_db_id
        )
        .fetch_all(&self.db)
        .await?;

        let existing_pairs: Vec<(i64, f64)> = existing_chapters
            .iter()
            .map(|c| (c.id, c.chapter_number))
            .collect();
        let (matched, orphaned_ids, unmatched_new) =
            match_chapters_inner(&existing_pairs, &target_chapters);

        let downloaded_orphan_ids: Vec<i64> = existing_chapters
            .iter()
            .filter(|c| orphaned_ids.contains(&c.id) && c.download_status == 2)
            .map(|c| c.id)
            .collect();

        Ok(MigrationContext {
            new_details,
            target_chapters,
            matched,
            orphaned_ids,
            unmatched_new,
            downloaded_orphan_ids,
        })
    }

    pub async fn preview_migration(
        &self,
        manga_db_id: i64,
        target_source_id: i64,
        target_source_manga_id: String,
    ) -> Result<MigrationPreview> {
        let ctx = self
            .resolve_migration_context(manga_db_id, target_source_id, &target_source_manga_id)
            .await?;

        Ok(MigrationPreview {
            target_title: ctx.new_details.title,
            target_cover_url: ctx.new_details.cover_url,
            chapters_matched: ctx.matched.len(),
            chapters_orphaned: ctx.orphaned_ids.len(),
            chapters_new: ctx.unmatched_new.len(),
            downloaded_chapters_at_risk: ctx.downloaded_orphan_ids.len(),
        })
    }

    pub async fn migrate_manga(
        &self,
        manga_db_id: i64,
        target_source_id: i64,
        target_source_manga_id: String,
        keep_orphaned_downloads: bool,
    ) -> Result<MigrationResult> {
        let old_manga = sqlx::query!("SELECT name FROM manga WHERE id = ?", manga_db_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Manga {manga_db_id} not found")))?;
        let old_manga_name = old_manga.name;

        let ctx = self
            .resolve_migration_context(manga_db_id, target_source_id, &target_source_manga_id)
            .await?;

        let MigrationContext {
            new_details,
            target_chapters,
            matched,
            orphaned_ids,
            unmatched_new,
            downloaded_orphan_ids,
        } = ctx;

        let new_count = unmatched_new.len();

        let library_path = self.settings.read().await.library_path.clone();
        let old_dir_name = format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&old_manga_name),
            manga_db_id
        );
        let new_dir_name = format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&new_details.title),
            manga_db_id
        );

        let non_downloaded_orphan_ids: Vec<i64> = orphaned_ids
            .iter()
            .copied()
            .filter(|id| !downloaded_orphan_ids.contains(id))
            .collect();

        if !keep_orphaned_downloads {
            for orphan_id in &downloaded_orphan_ids {
                let ch = sqlx::query!(
                    "SELECT name, chapter_number, volume FROM chapters WHERE id = ?",
                    orphan_id
                )
                .fetch_optional(&self.db)
                .await?;

                if let Some(ch) = ch {
                    let ch_name = chapter_name(ch.volume, ch.chapter_number, ch.name);
                    let cbz_path = library_path.join(&old_dir_name).join(format!(
                        "{}.cbz",
                        kani_core::utilities::sanitize_filename(&ch_name)
                    ));
                    if cbz_path.exists()
                        && let Err(e) = tokio::fs::remove_file(&cbz_path).await
                    {
                        tracing::warn!("Failed to delete orphaned CBZ {:?}: {}", cbz_path, e);
                    }
                }
            }
        }

        let mut tx = self.db.begin().await?;
        let status: i64 = new_details.status.into();

        sqlx::query!(
            "UPDATE manga SET source_id = ?, source_manga_id = ?, name = ?,
            cover_url = ?, description = ?, status = ? WHERE id = ?",
            target_source_id,
            target_source_manga_id,
            new_details.title,
            new_details.cover_url,
            new_details.description,
            status,
            manga_db_id
        )
        .execute(&mut *tx)
        .await?;

        for (existing_id, new_source_chapter_id) in &matched {
            let target_ch = target_chapters
                .iter()
                .find(|c| c.id == *new_source_chapter_id)
                .ok_or_else(|| {
                    ServiceError::Internal("Chapter match inconsistency during migration".into())
                })?;

            let vol: Option<i64> = target_ch.volume.map(|v| v as i64);
            sqlx::query!(
                "UPDATE chapters SET source_chapter_id = ?, name = ?, language = ?,
                scanlator = ?, uploaded_at = ?, volume = ? WHERE id = ?",
                new_source_chapter_id,
                target_ch.title,
                target_ch.language,
                target_ch.scanlator,
                target_ch.date_uploaded,
                vol,
                existing_id
            )
            .execute(&mut *tx)
            .await?;
        }
        if keep_orphaned_downloads {
            for orphan_id in &downloaded_orphan_ids {
                sqlx::query!(
                    "UPDATE chapters SET is_orphaned = 1 WHERE id = ?",
                    orphan_id
                )
                .execute(&mut *tx)
                .await?;
            }

            for orphan_id in &non_downloaded_orphan_ids {
                sqlx::query!("DELETE FROM chapters WHERE id = ?", orphan_id)
                    .execute(&mut *tx)
                    .await?;
            }
        } else {
            for orphan_id in &orphaned_ids {
                sqlx::query!("DELETE FROM chapters WHERE id = ?", orphan_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        for ch in &unmatched_new {
            let vol: Option<i64> = ch.volume.map(|v| v as i64);
            sqlx::query!(
                "INSERT OR IGNORE INTO chapters
                (manga_id, source_chapter_id, name, chapter_number, language,
                volume, scanlator, uploaded_at, discovered_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)",
                manga_db_id,
                ch.id,
                ch.title,
                ch.number,
                ch.language,
                vol,
                ch.scanlator,
                ch.date_uploaded
            )
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query!("DELETE FROM manga_people WHERE manga_id = ?", manga_db_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query!("DELETE FROM manga_tags WHERE manga_id = ?", manga_db_id)
            .execute(&mut *tx)
            .await?;
        Self::sync_manga_metadata(&mut tx, manga_db_id, &new_details).await?;

        tx.commit().await?;

        if old_dir_name != new_dir_name {
            let old_path = library_path.join(&old_dir_name);
            let new_path = library_path.join(&new_dir_name);
            if old_path.exists()
                && let Err(e) = tokio::fs::rename(&old_path, &new_path).await
            {
                tracing::warn!(
                    "Failed to rename library directory {:?} → {:?}: {}",
                    old_path,
                    new_path,
                    e
                );
            }
        }

        let kept_count = if keep_orphaned_downloads {
            downloaded_orphan_ids.len()
        } else {
            0
        };
        let removed_count = if keep_orphaned_downloads {
            non_downloaded_orphan_ids.len()
        } else {
            orphaned_ids.len()
        };

        Ok(MigrationResult {
            chapters_matched: matched.len(),
            chapters_orphaned: removed_count,
            chapters_new: new_count,
            chapters_kept: kept_count,
        })
    }
}
