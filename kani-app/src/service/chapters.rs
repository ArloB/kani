use super::*;

impl AppService {
    /// Returns a paginated chapter list for a manga. Returns (chapters, has_next_page, total_pages).
    #[allow(clippy::too_many_arguments)]
    pub async fn get_local_chapters(
        &self,
        manga_id: i64,
        page: i32,
        page_size: i32,
        sort_order: kani_shared::types::ChapterSortOrder,
        user_id: i64,
        filter_downloaded: Option<bool>,
        filter_unread: Option<bool>,
        filter_scanlator: Option<String>,
    ) -> Result<(Vec<kani_shared::types::Chapter>, bool, Option<u32>)> {
        let limit = (page_size as i64) + 1;
        let offset = ((page - 1).max(0) as i64) * (page_size as i64);

        let mut extra = String::new();
        match filter_downloaded {
            Some(true) => extra.push_str(" AND c.download_status = 2"),
            Some(false) => extra.push_str(" AND c.download_status != 2"),
            None => {}
        }
        if filter_unread == Some(true) {
            extra.push_str(" AND (uct.is_read IS NULL OR uct.is_read = 0)");
        }

        let sql = format!(
            r#"SELECT c.id, c.source_chapter_id, c.name, c.chapter_number, c.language,
                      c.volume, c.scanlator, c.uploaded_at, c.download_status, c.is_orphaned,
                      c.page_count,
                      uct.is_read, uct.last_page_read
               FROM chapters c
               LEFT JOIN scanlator_preferences sp
                   ON sp.manga_id = c.manga_id AND sp.manga_id = ?
                   AND (c.scanlator = sp.scanlator OR (c.scanlator IS NULL AND sp.scanlator = 'Unknown'))
               LEFT JOIN user_chapter_tracking uct
                   ON uct.chapter_id = c.id AND uct.user_id = ?
               WHERE c.manga_id = ?{extra}
                 AND c.is_orphaned = false
                 AND (? IS NULL OR c.scanlator = ?)
               ORDER BY {}, COALESCE(sp.priority, -1) DESC
               LIMIT ? OFFSET ?"#,
            sort_order.to_sql_order()
        );

        let scanlator_for_count = filter_scanlator.clone();
        let mut rows = sqlx::query_as::<_, crate::models::ChapterRow>(&sql)
            .bind(manga_id)
            .bind(user_id)
            .bind(manga_id)
            .bind(filter_scanlator.clone())
            .bind(filter_scanlator)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.db)
            .await?;

        let has_next_page = rows.len() == limit as usize;
        if has_next_page {
            rows.pop();
        }

        let chapters = rows
            .into_iter()
            .map(|c| kani_shared::types::Chapter {
                id: c.id.to_string(),
                title: c.name,
                number: c.chapter_number,
                volume: c.volume,
                language: c.language,
                scanlator: c.scanlator,
                date_uploaded: c.uploaded_at,
                download_status: c.download_status,
                is_orphaned: c.is_orphaned,
                page_count: c.page_count,
                is_read: c.is_read.unwrap_or(false),
                last_page_read: c.last_page_read,
            })
            .collect();

        let count_sql = format!(
            "SELECT COUNT(*) FROM chapters c \
             LEFT JOIN user_chapter_tracking uct ON uct.chapter_id = c.id AND uct.user_id = ? \
             WHERE c.manga_id = ?{extra} AND (? IS NULL OR c.scanlator = ?)"
        );
        let total_count: i64 = sqlx::query_scalar(&count_sql)
            .bind(user_id)
            .bind(manga_id)
            .bind(scanlator_for_count.clone())
            .bind(scanlator_for_count)
            .fetch_one(&self.db)
            .await?;
        let ps = page_size as i64;
        let total_pages = Some(((total_count + ps - 1) / ps).max(0) as u32);

        Ok((chapters, has_next_page, total_pages))
    }

    /// Returns all chapter IDs for a manga matching the given filters (no pagination).
    /// When `preferred_only` is true, applies scanlator preferences and download rules
    /// to return one preferred version per chapter number (undownloaded chapters only).
    #[allow(clippy::too_many_arguments)]
    pub async fn get_chapter_ids(
        &self,
        manga_id: i64,
        user_id: i64,
        sort_order: kani_shared::types::ChapterSortOrder,
        filter_downloaded: Option<bool>,
        filter_unread: Option<bool>,
        filter_scanlator: Option<String>,
        preferred_only: bool,
    ) -> Result<Vec<i64>> {
        // When preferred_only is set, restrict to undownloaded chapters and
        // apply filter_chapters_by_rules afterwards.
        let effective_filter_downloaded = if preferred_only {
            Some(false)
        } else {
            filter_downloaded
        };

        let mut extra = String::new();
        match effective_filter_downloaded {
            Some(true) => extra.push_str(" AND c.download_status = 2"),
            Some(false) => extra.push_str(" AND c.download_status != 2"),
            None => {}
        }
        if filter_unread == Some(true) {
            extra.push_str(" AND (uct.is_read IS NULL OR uct.is_read = 0)");
        }

        let sql = format!(
            r#"SELECT c.id
               FROM chapters c
               LEFT JOIN scanlator_preferences sp
                   ON sp.manga_id = c.manga_id AND sp.manga_id = ?
                   AND (c.scanlator = sp.scanlator OR (c.scanlator IS NULL AND sp.scanlator = 'Unknown'))
               LEFT JOIN user_chapter_tracking uct
                   ON uct.chapter_id = c.id AND uct.user_id = ?
               WHERE c.manga_id = ?{extra}
                 AND c.is_orphaned = false
                 AND (? IS NULL OR c.scanlator = ?)
               ORDER BY {}, COALESCE(sp.priority, -1) DESC"#,
            sort_order.to_sql_order()
        );

        let ids: Vec<i64> = sqlx::query_scalar::<_, i64>(&sql)
            .bind(manga_id)
            .bind(user_id)
            .bind(manga_id)
            .bind(filter_scanlator.clone())
            .bind(filter_scanlator)
            .fetch_all(&self.db)
            .await?;

        if preferred_only {
            Ok(self.filter_chapters_by_rules(manga_id, ids).await)
        } else {
            Ok(ids)
        }
    }

    /// Resolves the on-disk CBZ path for a downloaded chapter and returns its
    /// metadata. Returns an error if the chapter is not in downloaded state.
    pub async fn chapter_cbz_path(
        &self,
        chapter_id: i64,
    ) -> Result<(std::path::PathBuf, String, i64, f64)> {
        let rec = sqlx::query!(
            "SELECT c.download_status, c.volume, c.chapter_number, c.name,
                    m.id as manga_id, m.name as manga_name
             FROM chapters c JOIN manga m ON c.manga_id = m.id
             WHERE c.id = ?",
            chapter_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| ServiceError::NotFound(format!("Chapter {chapter_id} not found")))?;

        if rec.download_status != 2 {
            return Err(ServiceError::NotFound(format!(
                "Chapter {chapter_id} is not downloaded"
            )));
        }

        let chapter_title = chapter_name(rec.volume, rec.chapter_number, rec.name);
        let library_path = self.settings.read().await.library_path.clone();
        let safe_manga_dir = format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&rec.manga_name),
            rec.manga_id,
        );
        let safe_chapter = kani_core::utilities::sanitize_filename(&chapter_title);
        let cbz_path = library_path
            .join(safe_manga_dir)
            .join(format!("{safe_chapter}.cbz"));

        // Defence-in-depth: even though sanitize_filename strips traversal
        // characters, canonicalise and verify containment to guard against
        // future weakening of the sanitisation logic.
        let safe_cbz = kani_core::utilities::assert_within_root(&library_path, &cbz_path)
            .map_err(|e| ServiceError::Internal(format!("Path traversal blocked: {e}")))?;

        Ok((safe_cbz, chapter_title, rec.manga_id, rec.chapter_number))
    }

    /// Returns the page manifest for a downloaded chapter, including adjacent
    /// chapter IDs for navigation.
    pub async fn get_chapter_page_manifest(
        &self,
        chapter_id: i64,
        user_id: i64,
    ) -> Result<crate::models::ChapterPageManifest> {
        let (cbz_path, chapter_title, manga_id, chapter_number) =
            self.chapter_cbz_path(chapter_id).await?;

        let manga_title = sqlx::query_scalar!("SELECT name FROM manga WHERE id = ?", manga_id)
            .fetch_one(&self.db)
            .await?;

        let path_clone = cbz_path.clone();
        // Read page list and double-page flags in a single blocking task so
        // we open the CBZ archive only once (the `zip` crate is sync).
        let (names, double_page_flags, spread_analysed) = tokio::task::spawn_blocking(move || {
            let names = kani_core::cbz::list_cbz_pages(&path_clone)?;
            let (flags, analysed) = kani_core::cbz::read_double_page_flags(&path_clone);
            Ok::<_, kani_core::error::Error>((names, flags, analysed))
        })
        .await
        .map_err(|e| ServiceError::Internal(format!("Task join error: {e}")))?
        .map_err(ServiceError::Core)?;

        let pages = names
            .into_iter()
            .enumerate()
            .map(|(index, filename)| crate::models::PageInfo {
                double_page: double_page_flags.contains(&index),
                index,
                filename,
            })
            .collect::<Vec<_>>();
        let page_count = pages.len();

        let db_clone = self.db.clone();
        let pc_i64 = page_count as i64;

        tokio::spawn(async move {
            let result = sqlx::query!(
                "UPDATE chapters SET page_count = ? WHERE id = ? AND page_count IS NULL",
                pc_i64,
                chapter_id
            )
            .execute(&db_clone)
            .await;

            if let Err(e) = result {
                tracing::warn!(
                    "Opportunistic page_count update failed for chapter {}: {}",
                    chapter_id,
                    e
                );
            }
        });

        let prev_chapter_id = self
            .adjacent_chapter_id(manga_id, chapter_number, false)
            .await?;
        let next_chapter_id = self
            .adjacent_chapter_id(manga_id, chapter_number, true)
            .await?;

        let last_page_read = self
            .get_chapter_progress(user_id, chapter_id)
            .await?
            .map(|(page, _)| page);

        Ok(crate::models::ChapterPageManifest {
            chapter_id,
            chapter_title,
            manga_id,
            manga_title,
            page_count,
            pages,
            prev_chapter_id,
            next_chapter_id,
            last_page_read,
            spread_analysed,
        })
    }

    /// Returns the id of the nearest chapter before (`next=false`) or after
    /// (`next=true`) `chapter_number`, respecting scanlator preferences.
    /// Does not filter by download status — the reader handles downloading.
    async fn adjacent_chapter_id(
        &self,
        manga_id: i64,
        chapter_number: f64,
        next: bool,
    ) -> Result<Option<i64>> {
        let scanlator_mode = self.get_scanlator_mode(manga_id).await?;
        let scanlator_filter = match scanlator_mode.as_str() {
            "whitelist" => {
                " AND EXISTS (SELECT 1 FROM scanlator_preferences sp WHERE sp.manga_id = c.manga_id AND sp.scanlator = c.scanlator)"
            }
            _ => {
                " AND NOT EXISTS (SELECT 1 FROM scanlator_preferences sp WHERE sp.manga_id = c.manga_id AND sp.scanlator = c.scanlator AND sp.blocked = 1)"
            }
        };
        let (cmp, order) = if next { (">", "ASC") } else { ("<", "DESC") };
        let sql = format!(
            "SELECT c.id FROM chapters c
             LEFT JOIN scanlator_preferences sp ON sp.manga_id = c.manga_id AND sp.scanlator = c.scanlator
             WHERE c.manga_id = ? AND c.chapter_number {cmp} ?{scanlator_filter}
             ORDER BY c.chapter_number {order}, COALESCE(sp.priority, -1) DESC LIMIT 1"
        );
        let id: Option<i64> = sqlx::query_scalar(&sql)
            .bind(manga_id)
            .bind(chapter_number)
            .fetch_optional(&self.db)
            .await?;
        Ok(id)
    }

    /// Reads a single page image from a downloaded chapter's CBZ archive.
    ///
    /// Returns the raw bytes and lowercase file extension (e.g. `"jpg"`).
    pub async fn read_chapter_page(
        &self,
        chapter_id: i64,
        page_num: usize,
    ) -> Result<(Vec<u8>, String)> {
        let (cbz_path, ..) = self.chapter_cbz_path(chapter_id).await?;
        tokio::task::spawn_blocking(move || kani_core::cbz::read_cbz_page(&cbz_path, page_num))
            .await
            .map_err(|e| ServiceError::Internal(format!("Task join error: {e}")))?
            .map_err(ServiceError::Core)
    }

    pub(super) async fn fetch_all_chapter_pages(
        &self,
        source_id: i64,
        source_manga_id: &str,
    ) -> Result<Vec<wit_types::ChapterInfo>> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&source_id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {source_id} not found")))?
        };

        let mut all: Vec<wit_types::ChapterInfo> = Vec::new();
        let mut page = 1i32;
        loop {
            let res = source_manager
                .lease_instance()
                .await?
                .get_chapter_list(source_manga_id, page, None, None)
                .await?;
            let json = serde_json::to_string(&res)
                .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))?;
            let list: wit_types::ChapterList = serde_json::from_str(&json).map_err(|e| {
                ServiceError::Internal(format!("Failed to parse chapter list: {e}"))
            })?;
            all.extend(list.chapters);
            if !list.has_next_page {
                break;
            }
            page += 1;
        }
        Ok(all)
    }
}
