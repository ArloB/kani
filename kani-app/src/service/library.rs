use super::*;

impl AppService {
    pub async fn get_manga_by_id(&self, id: i64) -> Result<crate::models::Manga> {
        sqlx::query_as!(crate::models::Manga, "SELECT * FROM manga WHERE id = ?", id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Manga {id} not found")))
    }

    /// Returns one page (20 rows) of the library ordered by id ASC (order=0) or DESC (order=1).
    pub async fn get_library(&self, page: i32, order: i32) -> Result<Vec<crate::models::Manga>> {
        let order_sql = if order == 1 { "id DESC" } else { "id ASC" };
        let offset = (page - 1).max(0) * 20;
        // sqlx macro can't bind ORDER BY; use query_as with a runtime-built SQL.
        let sql = format!("SELECT * FROM manga ORDER BY {order_sql} LIMIT 20 OFFSET {offset}");
        sqlx::query_as::<_, crate::models::Manga>(&sql)
            .fetch_all(&self.db)
            .await
            .map_err(Into::into)
    }

    /// Deletes a manga entry together with its directory on disk (best-effort) and
    /// its cover file. Succeeds even if the files are already absent.
    pub async fn delete_manga(&self, id: i64, user_id: i64) -> Result<()> {
        let row = sqlx::query!("SELECT name, local_cover_path FROM manga WHERE id = ?", id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Manga {id} not found")))?;

        let library_path = self.settings.read().await.library_path.clone();

        let safe_name = format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&row.name),
            id
        );
        let dir_path = library_path.join(&safe_name);
        match tokio::fs::remove_dir_all(&dir_path).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(ServiceError::Io(e)),
        }

        if let Some(cover_rel) = row.local_cover_path {
            let cover_path = library_path.join(&cover_rel);
            match kani_core::utilities::assert_within_root(&library_path, &cover_path) {
                Ok(safe_path) => match tokio::fs::remove_file(&safe_path).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => tracing::warn!("Failed to remove cover {:?}: {e}", safe_path),
                },
                Err(e) => tracing::warn!("Cover path traversal blocked: {e}"),
            }
        }

        sqlx::query!("DELETE FROM manga WHERE id = ?", id)
            .execute(&self.db)
            .await?;

        self.audit(Some(user_id), "manga.delete", Some(&row.name), None)
            .await;
        self.webhook_service
            .fire(crate::service::webhooks::WebhookPayload::MangaDeleted {
                manga_id: id,
                manga_name: row.name.clone(),
            })
            .await;
        Ok(())
    }

    /// Filtered/paginated library query. Returns (rows, has_next_page, total_pages).
    #[allow(clippy::too_many_arguments)]
    pub async fn get_library_filtered(
        &self,
        user_id: i64,
        page: i32,
        page_size: i32,
        search: Option<String>,
        status_filter: Option<i64>,
        tag_filter: Option<i64>,
        author_filter: Option<i64>,
        artist_filter: Option<i64>,
        category_filter: Option<i64>,
        reading_status_filter: Option<i64>,
        hide_no_unread: bool,
        hide_completed_status: bool,
        source_id: Option<i64>,
        sort_by: kani_shared::types::MangaSortOrder,
    ) -> Result<(Vec<crate::models::LibraryManga>, bool, Option<u32>)> {
        use kani_shared::types::MangaSortOrder;

        let offset = (page - 1).max(0) * page_size;

        let need_umt = sort_by.needs_tracking_join()
            || reading_status_filter.is_some()
            || hide_completed_status;

        let mut qb = sqlx::QueryBuilder::new(
            "SELECT m.id, COALESCE(m.local_name, m.name) AS name, m.cover_url, m.local_cover_path, s.base_url, \
             m.is_orphaned, \
             COUNT(*) OVER() AS total_count, \
             (SELECT COUNT(*) FROM chapters c2 \
              LEFT JOIN user_manga_tracking umt2 ON umt2.manga_id = m.id AND umt2.user_id = ",
        );
        qb.push_bind(user_id);
        qb.push(
            " WHERE c2.manga_id = m.id \
              AND c2.discovered_at IS NOT NULL \
              AND c2.discovered_at > COALESCE(umt2.last_seen_at, m.created_at)) AS new_chapter_count \
             FROM manga m JOIN sources s ON m.source_id = s.id",
        );

        if need_umt {
            qb.push(" LEFT JOIN user_manga_tracking umt ON umt.manga_id = m.id AND umt.user_id = ");
            qb.push_bind(user_id);
        }

        qb.push(" WHERE 1=1");

        if let Some(s) = search {
            qb.push(" AND LOWER(COALESCE(m.local_name, m.name)) LIKE '%' || LOWER(");
            qb.push_bind(s);
            qb.push(") || '%'");
        }
        if let Some(v) = status_filter {
            qb.push(" AND m.status = ");
            qb.push_bind(v);
        }
        if let Some(v) = tag_filter {
            qb.push(" AND EXISTS (SELECT 1 FROM manga_tags mt WHERE mt.manga_id = m.id AND mt.tag_id = ");
            qb.push_bind(v);
            qb.push(")");
        }
        if let Some(v) = author_filter {
            qb.push(" AND EXISTS (SELECT 1 FROM manga_people mp WHERE mp.manga_id = m.id AND mp.role = 'author' AND mp.person_id = ");
            qb.push_bind(v);
            qb.push(")");
        }
        if let Some(v) = artist_filter {
            qb.push(" AND EXISTS (SELECT 1 FROM manga_people mp WHERE mp.manga_id = m.id AND mp.role = 'artist' AND mp.person_id = ");
            qb.push_bind(v);
            qb.push(")");
        }
        if let Some(v) = category_filter {
            qb.push(" AND EXISTS (SELECT 1 FROM manga_categories mc WHERE mc.manga_id = m.id AND mc.category_id = ");
            qb.push_bind(v);
            qb.push(")");
        }
        if let Some(v) = reading_status_filter {
            qb.push(" AND umt.status = ");
            qb.push_bind(v);
        }
        if hide_completed_status {
            qb.push(" AND (umt.status IS NULL OR umt.status != 4)");
        }
        if hide_no_unread {
            // Keep manga that have at least one chapter number not read by this user.
            qb.push(
                " AND EXISTS (\
                   SELECT 1 FROM chapters c WHERE c.manga_id = m.id \
                   AND NOT EXISTS (\
                       SELECT 1 FROM chapters c2 \
                       JOIN user_chapter_tracking uct ON uct.chapter_id = c2.id \
                       WHERE c2.manga_id = c.manga_id \
                         AND c2.chapter_number = c.chapter_number \
                         AND uct.user_id = ",
            );
            qb.push_bind(user_id);
            qb.push(" AND uct.is_read = true))");
        }

        if let Some(v) = source_id {
            qb.push(" AND m.source_id = ");
            qb.push_bind(v);
        }

        let limit = page_size + 1;

        // ORDER BY — LastReadDesc requires a correlated subquery with a bind parameter.
        if matches!(sort_by, MangaSortOrder::LastReadDesc) {
            qb.push(
                " ORDER BY (SELECT MAX(uct2.last_read_at) \
                   FROM user_chapter_tracking uct2 \
                   JOIN chapters lrc ON lrc.id = uct2.chapter_id \
                   WHERE lrc.manga_id = m.id AND uct2.user_id = ",
            );
            qb.push_bind(user_id);
            qb.push(") DESC NULLS LAST, m.name ASC");
        } else {
            qb.push(format!(" ORDER BY {}", sort_by.to_sql_order()));
        }

        qb.push(format!(" LIMIT {} OFFSET ", limit));
        qb.push_bind(offset);

        let mut records = qb
            .build_query_as::<crate::models::LibraryManga>()
            .fetch_all(&self.db)
            .await?;

        let has_next_page = records.len() == limit as usize;
        let total_count = records.first().map(|r| r.total_count).unwrap_or(0);
        records.truncate(page_size as usize);
        let ps = page_size as i64;
        let total_pages = Some(((total_count + ps - 1) / ps).max(0) as u32);
        Ok((records, has_next_page, total_pages))
    }

    /// Returns the continue-reading shelf: manga the user has started that still have
    /// unread chapters, ordered by most-recently-read first.
    pub async fn get_continue_reading_shelf(
        &self,
        user_id: i64,
        limit: i64,
    ) -> Result<Vec<crate::models::ContinueReadingItem>> {
        let mangas = sqlx::query!(
            r#"
            SELECT m.id, COALESCE(m.local_name, m.name) as "name!: String", m.cover_url, m.local_cover_path, s.base_url
            FROM manga m
            JOIN sources s ON s.id = m.source_id
            JOIN chapters c ON c.manga_id = m.id
            JOIN user_chapter_tracking uct ON uct.chapter_id = c.id AND uct.user_id = ?
            WHERE uct.is_read = true
              AND EXISTS (
                  SELECT 1 FROM chapters c2
                  WHERE c2.manga_id = m.id
                    AND NOT EXISTS (
                        SELECT 1 FROM chapters c3
                        JOIN user_chapter_tracking uct2 ON uct2.chapter_id = c3.id
                        WHERE c3.manga_id = m.id
                          AND c3.chapter_number = c2.chapter_number
                          AND uct2.user_id = ?
                          AND uct2.is_read = true
                    )
              )
            GROUP BY m.id
            ORDER BY MAX(uct.last_read_at) DESC
            LIMIT ?
            "#,
            user_id,
            user_id,
            limit,
        )
        .fetch_all(&self.db)
        .await?;

        let mut items = Vec::new();
        for row in mangas {
            let Ok(Some(next)) = self.get_continue_reading_chapter(user_id, row.id).await else {
                continue;
            };
            items.push(crate::models::ContinueReadingItem {
                manga_id: row.id,
                manga_name: row.name,
                cover_url: row.cover_url,
                local_cover_path: row.local_cover_path,
                base_url: row.base_url,
                chapter_id: next.chapter_id,
                chapter_number: next.chapter_number,
                last_page: next.last_page,
            });
        }
        Ok(items)
    }

    /// Returns full manga details including parsed authors, artists, and tags.
    /// URL signing and markdown rendering are the caller's responsibility.
    pub async fn get_local_manga_details(
        &self,
        id: i64,
    ) -> Result<crate::models::LocalMangaDetails> {
        use kani_shared::types::NamedItem;

        let manga = sqlx::query_as!(crate::models::Manga, "SELECT * FROM manga WHERE id = ?", id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotFound(format!("Manga {id} not found")))?;

        let source = sqlx::query_as!(
            kani_shared::types::Source,
            "SELECT id, name, version, base_url, enabled, favourited, unrestricted_http \
             FROM sources WHERE id = ?",
            manga.source_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| ServiceError::NotFound("Source not found".into()))?;

        let record = sqlx::query!(
            r#"SELECT
                (SELECT json_group_array(json_object('id', p.id, 'name', p.name))
                 FROM manga_people mp JOIN people p ON mp.person_id = p.id
                 WHERE mp.manga_id = m.id AND role = 'author') as "authors!: String",
                (SELECT json_group_array(json_object('id', p.id, 'name', p.name))
                 FROM manga_people mp JOIN people p ON mp.person_id = p.id
                 WHERE mp.manga_id = m.id AND role = 'artist') as "artists!: String",
                (SELECT json_group_array(json_object('id', t.id, 'name', t.name))
                 FROM manga_tags mt JOIN tags t ON mt.tag_id = t.id
                 WHERE mt.manga_id = m.id) as "tags!: String",
                (SELECT json_group_array(name) FROM manga_local_authors
                 WHERE manga_id = m.id AND role = 'author') as "local_authors: String",
                (SELECT json_group_array(name) FROM manga_local_authors
                 WHERE manga_id = m.id AND role = 'artist') as "local_artists: String",
                (SELECT json_group_array(name) FROM manga_local_tags
                 WHERE manga_id = m.id) as "local_tags: String",
                EXISTS(SELECT 1 FROM manga_local_authors WHERE manga_id = m.id) as "has_local_people: bool",
                EXISTS(SELECT 1 FROM manga_local_tags WHERE manga_id = m.id) as "has_local_tags: bool"
               FROM manga m WHERE m.id = ?"#,
            id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| ServiceError::NotFound(format!("Manga {id} not found")))?;

        let src_authors =
            serde_json::from_str::<Vec<NamedItem>>(&record.authors).unwrap_or_default();
        let src_artists =
            serde_json::from_str::<Vec<NamedItem>>(&record.artists).unwrap_or_default();
        let src_tags = serde_json::from_str::<Vec<NamedItem>>(&record.tags).unwrap_or_default();

        let loc_authors: Vec<String> = record
            .local_authors
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let loc_artists: Vec<String> = record
            .local_artists
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let loc_tags: Vec<String> = record
            .local_tags
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let eff_authors = if record.has_local_people {
            loc_authors
                .iter()
                .map(|n| NamedItem {
                    id: 0,
                    name: n.clone(),
                })
                .collect()
        } else {
            src_authors.clone()
        };
        let eff_artists = if record.has_local_people {
            loc_artists
                .iter()
                .map(|n| NamedItem {
                    id: 0,
                    name: n.clone(),
                })
                .collect()
        } else {
            src_artists.clone()
        };
        let eff_tags = if record.has_local_tags {
            loc_tags
                .iter()
                .map(|n| NamedItem {
                    id: 0,
                    name: n.clone(),
                })
                .collect()
        } else {
            src_tags.clone()
        };

        Ok(crate::models::LocalMangaDetails {
            auto_scan: manga.auto_scan,
            manga,
            source,
            authors: eff_authors,
            artists: eff_artists,
            tags: eff_tags,
            source_authors: src_authors,
            source_artists: src_artists,
            source_tags: src_tags,
            local_authors: loc_authors,
            local_artists: loc_artists,
            local_tags: loc_tags,
            has_local_people: record.has_local_people,
            has_local_tags: record.has_local_tags,
        })
    }

    /// Returns the library DB id of a manga from a given source, if it is in the library.
    pub async fn check_in_library(&self, source_id: i64, manga_id: &str) -> Result<Option<i64>> {
        sqlx::query_scalar!(
            "SELECT id FROM manga WHERE source_manga_id = ? AND source_id = ?",
            manga_id,
            source_id
        )
        .fetch_optional(&self.db)
        .await
        .map_err(Into::into)
    }

    /// Returns the 50 most recent chapter updates (paginated). Returns (items, has_next_page, total_pages).
    pub async fn get_recent_updates(
        &self,
        page: i32,
    ) -> Result<(Vec<kani_shared::types::RecentUpdateItem>, bool, Option<u32>)> {
        let offset = (page - 1) * 50;
        let mut items = sqlx::query_as!(
            kani_shared::types::RecentUpdateItem,
            "SELECT m.id as manga_id, COALESCE(m.local_name, m.name) as manga_name, m.cover_url, m.local_cover_path,
                    s.base_url, c.id as chapter_id, c.chapter_number,
                    c.name as chapter_name, c.discovered_at,
                    (c.download_status = 2) as \"is_downloaded: bool\"
             FROM chapters c
             JOIN manga m ON c.manga_id = m.id
             JOIN sources s ON m.source_id = s.id
             WHERE c.discovered_at IS NOT NULL
             ORDER BY DATE(c.discovered_at) DESC, c.chapter_number DESC LIMIT 51 OFFSET ?",
            offset
        )
        .fetch_all(&self.db)
        .await?;

        let has_next_page = items.len() > 50;
        if has_next_page {
            items.truncate(50);
        }

        let total_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM chapters c WHERE c.discovered_at IS NOT NULL"
        )
        .fetch_one(&self.db)
        .await?;
        let total_pages = Some(((total_count as i64 + 49) / 50).max(0) as u32);

        Ok((items, has_next_page, total_pages))
    }

    /// `force` skips the fuzzy duplicate check (used after the user confirms "add anyway").
    pub async fn save_to_library(
        &self,
        source_id: i64,
        manga_id: &str,
        force: bool,
    ) -> Result<i64> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&source_id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {source_id} not found")))?
        };

        let result = source_manager
            .lease_instance()
            .await?
            .get_manga_details(manga_id)
            .await?;

        let manga = convert_to_shared_manga_info(result);

        if !force {
            let hits = crate::service::dedup::find_similar_manga(
                &self.db,
                &manga.title,
                &manga.authors,
                None,
            )
            .await?;
            if !hits.is_empty() {
                return Err(ServiceError::PossibleDuplicate(hits));
            }
        }

        let mut tx = self.db.begin().await?;
        let status: i64 = manga.status.into();

        let decoded_manga_id = decode_manga_id(&manga.id);

        let insert_result = sqlx::query!(
            "INSERT OR IGNORE INTO manga (source_manga_id, source_id, name, cover_url, description, status) \
             VALUES (?, ?, ?, ?, ?, ?)",
            decoded_manga_id,
            source_id,
            manga.title,
            manga.cover_url,
            manga.description,
            status
        )
        .execute(&mut *tx)
        .await?;

        let we_inserted = insert_result.rows_affected() == 1;

        let manga_row_id: i64 = sqlx::query_scalar!(
            "SELECT id FROM manga WHERE source_manga_id = ? AND source_id = ?",
            decoded_manga_id,
            source_id
        )
        .fetch_one(&mut *tx)
        .await?;

        if we_inserted {
            Self::sync_manga_metadata(&mut tx, manga_row_id, &manga).await?;
        }

        tx.commit().await?;

        if we_inserted {
            if let Some(ref url) = manga.cover_url {
                let base_url =
                    sqlx::query_scalar!("SELECT base_url FROM sources WHERE id = ?", source_id)
                        .fetch_optional(&self.db)
                        .await?
                        .unwrap_or_default();

                if let Err(e) = self
                    .download_and_store_cover(manga_row_id, url, &base_url)
                    .await
                {
                    tracing::warn!(
                        "Failed to download cover for manga {}: {} — library entry still saved, scheduling retry",
                        manga_row_id,
                        e
                    );
                    self.schedule_cover_retry(manga_row_id).await;
                }
            }

            let has_next_page = self
                .fetch_and_store_chapter_page(source_id, manga_id, manga_row_id, 1)
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("Failed to fetch initial chapters: {}", e);
                    false
                });

            if has_next_page {
                let bg_self = self.clone();
                let bg_manga_id = manga_id.to_string();
                tokio::spawn(async move {
                    bg_self
                        .fetch_and_store_remaining_chapters(source_id, bg_manga_id, manga_row_id, 2)
                        .await;
                });
            }

            let pool = self.db.clone();
            tokio::spawn(async move {
                if let Err(e) =
                    crate::service::dedup::record_duplicates_for_manga(&pool, manga_row_id).await
                {
                    tracing::warn!("Duplicate recording failed for manga {manga_row_id}: {e}");
                }
            });

            self.webhook_service
                .fire(crate::service::webhooks::WebhookPayload::MangaAdded {
                    manga_id: manga_row_id,
                    manga_name: manga.title.clone(),
                    source_id,
                })
                .await;
        }

        Ok(manga_row_id)
    }

    pub(super) async fn sync_manga_metadata(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        manga_row_id: i64,
        manga: &wit_types::MangaInfo,
    ) -> Result<()> {
        for author in &manga.authors {
            sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", author)
                .execute(&mut **tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_people (manga_id, role, person_id) \
                SELECT ?, 'author', id FROM people WHERE name = ?",
                manga_row_id,
                author
            )
            .execute(&mut **tx)
            .await?;
        }

        for artist in &manga.artists {
            sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", artist)
                .execute(&mut **tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_people (manga_id, role, person_id) \
                SELECT ?, 'artist', id FROM people WHERE name = ?",
                manga_row_id,
                artist
            )
            .execute(&mut **tx)
            .await?;
        }

        for tag in &manga.tags {
            sqlx::query!("INSERT OR IGNORE INTO tags (name) VALUES (?)", tag)
                .execute(&mut **tx)
                .await?;
            sqlx::query!(
                "INSERT OR IGNORE INTO manga_tags (manga_id, tag_id) \
                SELECT ?, id FROM tags WHERE name = ?",
                manga_row_id,
                tag
            )
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    pub async fn refresh_manga(&self, manga_row_id: i64) -> Result<()> {
        let ids = sqlx::query!(
            "SELECT source_id, source_manga_id, s.base_url as base_url, m.cover_overridden
            FROM manga m
            JOIN sources s ON m.source_id = s.id
            WHERE m.id = ?",
            manga_row_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| ServiceError::NotFound(format!("Manga {manga_row_id} not found")))?;

        let manga_info_raw = self
            .get_manga_details(ids.source_id, &ids.source_manga_id)
            .await?;
        let manga_info: wit_types::MangaInfo = serde_json::from_str(&manga_info_raw)
            .map_err(|e| ServiceError::Internal(format!("Failed to parse manga: {}", e)))?;

        let mut tx = self.db.begin().await?;
        let status = manga_info.status as i64;

        sqlx::query!(
            "UPDATE manga SET name = ?, cover_url = ?, description = ?, status = ? WHERE id = ?",
            manga_info.title,
            manga_info.cover_url,
            manga_info.description,
            status,
            manga_row_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!("DELETE FROM manga_people WHERE manga_id = ?", manga_row_id)
            .execute(&mut *tx)
            .await?;

        sqlx::query!("DELETE FROM manga_tags WHERE manga_id = ?", manga_row_id)
            .execute(&mut *tx)
            .await?;

        Self::sync_manga_metadata(&mut tx, manga_row_id, &manga_info).await?;

        tx.commit().await?;

        if !ids.cover_overridden
            && let Some(ref url) = manga_info.cover_url
            && let Err(e) = self
                .download_and_store_cover(manga_row_id, url, &ids.base_url)
                .await
        {
            tracing::warn!(
                "Failed to refresh cover for manga {}: {}, scheduling retry",
                manga_row_id,
                e
            );
            self.schedule_cover_retry(manga_row_id).await;
        }

        let has_next_page = self
            .fetch_and_store_chapter_page(ids.source_id, &ids.source_manga_id, manga_row_id, 1)
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to fetch initial chapters during refresh: {}", e);
                false
            });

        self.cache
            .invalidate_chapter_list_for_manga(ids.source_id, &ids.source_manga_id)
            .await;

        if has_next_page {
            let bg_self = self.clone();
            let bg_manga_id = ids.source_manga_id.clone();
            tokio::spawn(async move {
                bg_self
                    .fetch_and_store_remaining_chapters(ids.source_id, bg_manga_id, manga_row_id, 2)
                    .await;
            });
        }

        Ok(())
    }

    pub(super) async fn insert_chapters_batch(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        manga_row_id: i64,
        chapters: &[wit_types::ChapterInfo],
    ) -> Result<Vec<i64>> {
        let mut ids = Vec::new();
        for chunk in chapters.chunks(100) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT OR IGNORE INTO chapters \
                (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at, discovered_at) ",
            );
            qb.push_values(chunk, |mut b, ch| {
                b.push_bind(manga_row_id)
                    .push_bind(decode_manga_id(&ch.id))
                    .push_bind(ch.title.clone())
                    .push_bind(ch.number)
                    .push_bind(ch.language.clone())
                    .push_bind(ch.volume)
                    .push_bind(ch.scanlator.clone())
                    .push_bind(ch.date_uploaded);
                b.push("CURRENT_TIMESTAMP");
            });
            qb.push(" RETURNING id");
            let mut rows: Vec<i64> = qb.build_query_scalar().fetch_all(&mut **tx).await?;
            ids.append(&mut rows);
        }
        Ok(ids)
    }

    /// Fetches chapters from the source and stores them without broadcasting any SSE events.
    /// Used during bulk import to avoid spamming `NewChapters` notifications.
    /// Returns the IDs of newly inserted chapters.
    pub async fn fetch_and_store_chapters_silent(&self, manga_row_id: i64) -> Result<Vec<i64>> {
        let ids = sqlx::query!(
            "SELECT source_id, source_manga_id FROM manga WHERE id = ?",
            manga_row_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| ServiceError::NotFound(format!("Manga {manga_row_id} not found")))?;

        let source_manager = {
            let sources = self.sources.read().await;
            sources.get(&ids.source_id).cloned().ok_or_else(|| {
                ServiceError::NotFound(format!("Source {} not found", ids.source_id))
            })?
        };

        let mut tx = self.db.begin().await?;
        let mut new_chapter_ids = Vec::new();
        let mut page = 1;

        loop {
            let res = source_manager
                .lease_instance()
                .await?
                .get_chapter_list(&ids.source_manga_id, page, None, None)
                .await?;
            let json = serde_json::to_string(&res)
                .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))?;
            let chapter_list: wit_types::ChapterList =
                serde_json::from_str(&json).map_err(|e| {
                    ServiceError::Internal(format!("Failed to parse chapter list: {}", e))
                })?;

            if chapter_list.chapters.is_empty() {
                break;
            }

            let mut page_new_ids = Vec::new();
            for chunk in chapter_list.chapters.chunks(100) {
                let chunk_ids = self
                    .insert_chapters_batch(&mut tx, manga_row_id, chunk)
                    .await?;
                page_new_ids.extend(chunk_ids);
            }

            let new_on_page = page_new_ids.len();
            new_chapter_ids.extend(page_new_ids);

            if new_on_page == 0 || !chapter_list.has_next_page {
                break;
            }

            page += 1;
        }

        tx.commit().await?;

        if !new_chapter_ids.is_empty() {
            self.cache
                .invalidate_chapter_list_for_manga(ids.source_id, &ids.source_manga_id)
                .await;
        }

        Ok(new_chapter_ids)
    }

    pub async fn scan_for_new_chapters(&self, manga_row_id: i64) -> Result<Vec<i64>> {
        let new_chapter_ids = self.fetch_and_store_chapters_silent(manga_row_id).await?;

        if !new_chapter_ids.is_empty() {
            let manga_name =
                sqlx::query_scalar!("SELECT name FROM manga WHERE id = ?", manga_row_id)
                    .fetch_optional(&self.db)
                    .await
                    .unwrap_or(None)
                    .unwrap_or_default();

            struct ChRow {
                volume: Option<i64>,
                chapter_number: f64,
                name: Option<String>,
            }
            let ids_json = serde_json::to_string(&new_chapter_ids).unwrap_or_default();
            let chapter_rows: Vec<ChRow> = sqlx::query_as!(
                ChRow,
                "SELECT volume, chapter_number, name FROM chapters WHERE id IN (SELECT value FROM json_each(?))",
                ids_json
            )
            .fetch_all(&self.db)
            .await
            .unwrap_or_default();
            let chapter_names: Vec<String> = chapter_rows
                .into_iter()
                .map(|r| crate::service::chapter_name(r.volume, r.chapter_number, r.name))
                .collect();

            let _ = self.refresh_tx.send(AppEvent::NewChapters {
                manga_id: manga_row_id,
                manga_name,
                count: new_chapter_ids.len(),
                chapter_ids: new_chapter_ids.clone(),
                chapter_names,
            });
        }

        Ok(new_chapter_ids)
    }

    /// Returns `(matching_chapters, total_chapters)` for a hypothetical rule set,
    /// without modifying any state.
    pub async fn preview_download_rules(
        &self,
        manga_id: i64,
        kinds: Vec<DownloadRuleKind>,
    ) -> Result<(usize, usize)> {
        let rows = sqlx::query_as::<_, ChapterFilterRow>(
            "SELECT id, scanlator, language, name, chapter_number, uploaded_at
             FROM chapters WHERE manga_id = ?",
        )
        .bind(manga_id)
        .fetch_all(&self.db)
        .await?;

        let total = rows.len();
        if kinds.is_empty() {
            return Ok((total, total));
        }

        let predicate = self.build_chapter_predicate(kinds);
        let matching = rows.iter().filter(|r| predicate(r)).count();
        Ok((matching, total))
    }

    fn build_chapter_predicate(
        &self,
        rules: Vec<DownloadRuleKind>,
    ) -> impl Fn(&ChapterFilterRow) -> bool {
        move |chapter| {
            // Axes 0 (Language) and 1 (Title) use include/exclude semantics:
            // if any include rule exists on the axis, at least one must match;
            // all exclude rules on the axis must pass.
            for axis in 0u8..2 {
                let axis_rules: Vec<_> = rules.iter().filter(|r| r.axis() == axis).collect();
                if axis_rules.is_empty() {
                    continue;
                }
                let includes: Vec<_> = axis_rules.iter().filter(|r| r.is_include()).collect();
                let excludes: Vec<_> = axis_rules.iter().filter(|r| !r.is_include()).collect();
                if !includes.is_empty() && !includes.iter().any(|r| r.passes(chapter)) {
                    return false;
                }
                if !excludes.iter().all(|r| r.passes(chapter)) {
                    return false;
                }
            }
            // All remaining axes (2=range, 3=fractional, 4=time) must all pass.
            for rule in rules.iter().filter(|r| r.axis() >= 2) {
                if !rule.passes(chapter) {
                    return false;
                }
            }
            true
        }
    }

    pub async fn filter_chapters_by_rules(
        &self,
        manga_id: i64,
        candidate_ids: Vec<i64>,
    ) -> Vec<i64> {
        let raw_rules: Vec<DownloadRuleRow> = sqlx::query_as!(
            DownloadRuleRow,
            "SELECT id, manga_id, rule_type, value
                 FROM download_rules
                 WHERE manga_id = ?",
            manga_id
        )
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        let ids_after_rules = if raw_rules.is_empty() {
            candidate_ids.clone()
        } else {
            if candidate_ids.is_empty() {
                return vec![];
            }

            let rules: Vec<DownloadRule> = raw_rules
                .into_iter()
                .filter_map(|row| DownloadRule::try_from(row).ok())
                .collect();

            let predicate =
                self.build_chapter_predicate(rules.into_iter().map(|dr| dr.kind).collect());

            let chapter_map: HashMap<i64, ChapterFilterRow> = {
                let mut qb = sqlx::QueryBuilder::new(
                    "SELECT id, scanlator, language, name, chapter_number, uploaded_at FROM chapters WHERE id IN (",
                );
                let mut sep = qb.separated(", ");
                for id in &candidate_ids {
                    sep.push_bind(id);
                }
                qb.push(")");
                qb.build_query_as::<ChapterFilterRow>()
                    .fetch_all(&self.db)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|row| (row.id, row))
                    .collect()
            };

            candidate_ids
                .iter()
                .copied()
                .filter(|id| chapter_map.get(id).map(&predicate).unwrap_or(false))
                .collect()
        };

        let scanlator_mode = self
            .get_scanlator_mode(manga_id)
            .await
            .unwrap_or_else(|_| "priority".into());

        struct PrefEntry {
            priority: i64,
            blocked: bool,
        }

        let prefs: HashMap<String, PrefEntry> = sqlx::query!(
            "SELECT scanlator, priority, blocked FROM scanlator_preferences WHERE manga_id = ?",
            manga_id
        )
        .fetch_all(&self.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| {
            (
                r.scanlator,
                PrefEntry {
                    priority: r.priority,
                    blocked: r.blocked != 0,
                },
            )
        })
        .collect();

        if prefs.is_empty() {
            return ids_after_rules;
        }

        if ids_after_rules.is_empty() {
            return vec![];
        }

        struct ChapRow {
            id: i64,
            chapter_number: f64,
            scanlator: Option<String>,
        }

        let rows: Vec<ChapRow> = {
            let mut qb = sqlx::QueryBuilder::new(
                "SELECT id, chapter_number, scanlator FROM chapters WHERE id IN (",
            );
            let mut sep = qb.separated(", ");
            for id in &ids_after_rules {
                sep.push_bind(id);
            }
            qb.push(")");
            qb.build_query_as::<(i64, f64, Option<String>)>()
                .fetch_all(&self.db)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(id, chapter_number, scanlator)| ChapRow {
                    id,
                    chapter_number,
                    scanlator,
                })
                .collect()
        };

        // In whitelist mode: only chapters whose scanlator appears in prefs.
        // In priority mode: exclude chapters whose scanlator is explicitly blocked.
        let rows: Vec<ChapRow> = rows
            .into_iter()
            .filter(|row| {
                let scanlator = row.scanlator.as_deref().unwrap_or("");
                match scanlator_mode.as_str() {
                    "whitelist" => prefs.contains_key(scanlator),
                    _ => !prefs.get(scanlator).is_some_and(|e| e.blocked),
                }
            })
            .collect();

        let mut best: HashMap<OrderedFloat<f64>, (i64, i64)> = HashMap::new();

        for row in &rows {
            let prio = row
                .scanlator
                .as_deref()
                .and_then(|s| prefs.get(s).map(|e| e.priority))
                .unwrap_or(-1);
            let key = OrderedFloat(row.chapter_number);
            best.entry(key)
                .and_modify(|(existing_id, existing_prio)| {
                    if prio > *existing_prio {
                        *existing_id = row.id;
                        *existing_prio = prio;
                    }
                })
                .or_insert((row.id, prio));
        }

        let winner_ids: std::collections::HashSet<i64> = best.values().map(|(id, _)| *id).collect();
        ids_after_rules
            .into_iter()
            .filter(|id| winner_ids.contains(id))
            .collect()
    }

    pub async fn start_refresh_all(&self) -> Result<()> {
        let mut task_guard = self.refresh_task.lock().await;

        if let Some(handle) = &*task_guard
            && !handle.is_finished()
        {
            return Err(ServiceError::Internal("Refresh already in progress".into()));
        }

        let ids: Vec<(i64, String)> = sqlx::query!("SELECT id, name FROM manga ORDER BY id")
            .fetch_all(&self.db)
            .await?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect();

        let total = ids.len();
        let state = self.clone();

        let handle = tokio::spawn(async move {
            let manga_ids: Vec<i64> = ids.iter().map(|(id, _)| *id).collect();
            let _ = state
                .refresh_tx
                .send(AppEvent::Refresh(RefreshProgressEvent::Started {
                    total,
                    manga_ids,
                }));

            let mut futures = FuturesUnordered::new();
            for (id, name) in ids {
                let s = state.clone();
                let n = name.clone();
                futures.push(async move {
                    let success = s.refresh_manga(id).await.is_ok();
                    (id, n, success)
                });
            }

            let mut completed = 0usize;
            let mut failed = 0usize;

            while let Some((manga_id, manga_name, success)) = futures.next().await {
                completed += 1;
                if !success {
                    failed += 1;
                }

                let _ = state.refresh_tx.send(AppEvent::Refresh(
                    RefreshProgressEvent::MangaRefreshed {
                        manga_id,
                        manga_name,
                        completed,
                        total,
                        success,
                        // start_refresh_all does metadata refresh, not chapter scanning,
                        // so new_chapters is not meaningful here.
                        new_chapters: 0,
                    },
                ));
            }

            let _ = state
                .refresh_tx
                .send(AppEvent::Refresh(RefreshProgressEvent::Completed {
                    total,
                    failed,
                }));

            *state.refresh_task.lock().await = None;
        })
        .abort_handle();

        *task_guard = Some(handle);
        Ok(())
    }

    pub async fn is_refreshing(&self) -> bool {
        self.refresh_task
            .lock()
            .await
            .as_ref()
            .is_some_and(|h| !h.is_finished())
    }

    /// Aborts the currently running on-demand refresh task, if any.
    pub async fn abort_refresh(&self) {
        if let Some(handle) = self.refresh_task.lock().await.take() {
            handle.abort();
        }
    }

    /// Resize and JPEG-compress cover image bytes. Falls back to raw bytes if decode/encode fails.
    fn compress_cover_bytes(bytes: &[u8], max_dim: u32) -> Vec<u8> {
        let img = match image::load_from_memory(bytes) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("Cover decode failed, storing raw bytes: {e}");
                return bytes.to_vec();
            }
        };
        let img = if img.width() > max_dim || img.height() > max_dim {
            img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };
        let mut out = Vec::new();
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        match img.write_with_encoder(encoder) {
            Ok(()) => out,
            Err(e) => {
                tracing::warn!("Cover JPEG encode failed, storing raw bytes: {e}");
                bytes.to_vec()
            }
        }
    }

    pub(super) async fn download_and_store_cover(
        &self,
        manga_row_id: i64,
        cover_url: &str,
        base_url: &str,
    ) -> Result<()> {
        let library_path = self.settings.read().await.library_path.clone();
        let covers_dir = library_path.join("covers");
        tokio::fs::create_dir_all(&covers_dir).await?;

        let mut headers = rquest::header::HeaderMap::new();
        if let Ok(v) = rquest::header::HeaderValue::from_str(base_url) {
            headers.insert(rquest::header::REFERER, v);
        }

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.proxy_client.safe_get(cover_url, Some(headers)),
        )
        .await
        .map_err(|_| ServiceError::Other("Cover download timed out".into()))??;

        if !response.status().is_success() {
            return Err(ServiceError::Other(format!(
                "Cover download returned {}",
                response.status().as_u16()
            )));
        }

        let content_type = response
            .headers()
            .get(rquest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();

        if !content_type.starts_with("image/") {
            return Err(ServiceError::Other(format!(
                "Expected image for cover, got Content-Type: {}",
                content_type
            )));
        }

        const MAX_COVER_BYTES: usize = 10 * 1024 * 1024;
        let bytes = response
            .bytes_limited(MAX_COVER_BYTES)
            .await
            .map_err(|e| ServiceError::Internal(e.to_string()))?;

        let max_dim = self
            .settings
            .read()
            .await
            .cover_max_dimension
            .unwrap_or(800) as u32;

        // Resize and compress to JPEG on a blocking thread.
        // Falls back to raw bytes if decode/encode fails so the cover is never lost.
        let final_bytes =
            tokio::task::spawn_blocking(move || Self::compress_cover_bytes(&bytes, max_dim))
                .await
                .map_err(|e| {
                    ServiceError::Internal(format!("Cover compress task panicked: {e}"))
                })?;

        // Always store as JPEG after compression.
        let filename = format!("{}.jpg", manga_row_id);
        let cover_path = covers_dir.join(&filename);
        let relative = format!("covers/{}", filename);

        tokio::fs::write(&cover_path, &final_bytes).await?;

        sqlx::query!(
            "UPDATE manga SET local_cover_path = ? WHERE id = ?",
            relative,
            manga_row_id
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Retries downloading the cover for a single manga. Only attempts if
    /// `local_cover_path IS NULL` (already downloaded covers are skipped).
    pub async fn retry_single_cover(&self, manga_id: i64) -> Result<()> {
        struct Row {
            cover_url: String,
            base_url: String,
        }

        let row = sqlx::query_as!(
            Row,
            r#"SELECT m.cover_url as "cover_url!", s.base_url
               FROM manga m JOIN sources s ON s.id = m.source_id
               WHERE m.id = ? AND m.local_cover_path IS NULL
                 AND m.cover_url IS NOT NULL AND m.cover_overridden = FALSE"#,
            manga_id
        )
        .fetch_optional(&self.db)
        .await?;

        if let Some(row) = row {
            self.download_and_store_cover(manga_id, &row.cover_url, &row.base_url)
                .await?;
        }
        Ok(())
    }

    pub async fn retry_missing_covers(&self) {
        struct Row {
            id: i64,
            cover_url: String,
            base_url: String,
        }

        let rows = match sqlx::query_as!(
            Row,
            r#"SELECT m.id, m.cover_url as "cover_url!", s.base_url
               FROM manga m
               JOIN sources s ON s.id = m.source_id
               WHERE m.local_cover_path IS NULL
                 AND m.cover_url IS NOT NULL
                 AND m.cover_overridden = FALSE"#
        )
        .fetch_all(&self.db)
        .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("retry_missing_covers: failed to query manga: {e}");
                return;
            }
        };

        for row in rows {
            match self
                .download_and_store_cover(row.id, &row.cover_url, &row.base_url)
                .await
            {
                Ok(()) => tracing::info!(
                    "retry_missing_covers: downloaded cover for manga {}",
                    row.id
                ),
                Err(e) => tracing::warn!("retry_missing_covers: failed for manga {}: {e}", row.id),
            }
        }
    }

    /// Updates the local metadata override columns for a manga.
    /// Pass `None` for any field to leave it unchanged.
    /// For people/tags, pass `Some(vec![])` to clear the override entirely.
    pub async fn update_local_metadata(
        &self,
        manga_id: i64,
        update: crate::models::LocalMetadataUpdate,
        user_id: i64,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE manga SET local_name = ?, local_description = ?, local_status = ? WHERE id = ?",
            update.local_name,
            update.local_description,
            update.local_status,
            manga_id,
        )
        .execute(&self.db)
        .await?;

        if update.authors.is_some() || update.artists.is_some() {
            sqlx::query!(
                "DELETE FROM manga_local_authors WHERE manga_id = ?",
                manga_id
            )
            .execute(&self.db)
            .await?;

            if let Some(authors) = &update.authors {
                for name in authors {
                    sqlx::query!(
                        "INSERT INTO manga_local_authors (manga_id, name, role) VALUES (?, ?, 'author')",
                        manga_id,
                        name,
                    )
                    .execute(&self.db)
                    .await?;
                }
            }
            if let Some(artists) = &update.artists {
                for name in artists {
                    sqlx::query!(
                        "INSERT INTO manga_local_authors (manga_id, name, role) VALUES (?, ?, 'artist')",
                        manga_id,
                        name,
                    )
                    .execute(&self.db)
                    .await?;
                }
            }
        }

        if let Some(tags) = &update.tags {
            sqlx::query!("DELETE FROM manga_local_tags WHERE manga_id = ?", manga_id)
                .execute(&self.db)
                .await?;
            for name in tags {
                sqlx::query!(
                    "INSERT INTO manga_local_tags (manga_id, name) VALUES (?, ?)",
                    manga_id,
                    name,
                )
                .execute(&self.db)
                .await?;
            }
        }

        self.audit(Some(user_id), "manga.local_metadata.update", None, None)
            .await;
        Ok(())
    }

    /// Accepts a user-uploaded cover image, compresses it, and stores it as the local cover.
    /// Sets `cover_overridden = TRUE` so scans won't replace it.
    pub async fn upload_manga_cover(
        &self,
        manga_id: i64,
        bytes: Vec<u8>,
        content_type: &str,
        user_id: i64,
    ) -> Result<()> {
        if !content_type.starts_with("image/") {
            return Err(ServiceError::Other(format!(
                "Expected image, got Content-Type: {content_type}"
            )));
        }

        let library_path = self.settings.read().await.library_path.clone();
        let covers_dir = library_path.join("covers");
        tokio::fs::create_dir_all(&covers_dir).await?;

        let max_dim = self
            .settings
            .read()
            .await
            .cover_max_dimension
            .unwrap_or(800) as u32;

        let final_bytes =
            tokio::task::spawn_blocking(move || Self::compress_cover_bytes(&bytes, max_dim))
                .await
                .map_err(|e| {
                    ServiceError::Internal(format!("Cover compress task panicked: {e}"))
                })?;

        let filename = format!("{manga_id}_local.jpg");
        let cover_path = covers_dir.join(&filename);
        let relative = format!("covers/{filename}");

        tokio::fs::write(&cover_path, &final_bytes).await?;

        sqlx::query!(
            "UPDATE manga SET local_cover_path = ?, cover_overridden = TRUE WHERE id = ?",
            relative,
            manga_id,
        )
        .execute(&self.db)
        .await?;

        self.audit(Some(user_id), "manga.cover.upload", None, None)
            .await;
        Ok(())
    }

    /// Clears the user-uploaded cover override. If the source cover file still exists on disk
    /// it is restored immediately; otherwise `local_cover_path` is set to NULL so the next
    /// refresh re-downloads it.
    pub async fn clear_manga_cover_override(&self, manga_id: i64, user_id: i64) -> Result<()> {
        let library_path = self.settings.read().await.library_path.clone();

        let current_rel =
            sqlx::query_scalar!("SELECT local_cover_path FROM manga WHERE id = ?", manga_id)
                .fetch_optional(&self.db)
                .await?
                .flatten();

        if let Some(ref rel) = current_rel
            && rel.contains("_local")
        {
            let full = library_path.join(rel);
            match tokio::fs::remove_file(&full).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!("Failed to delete local cover {full:?}: {e}"),
            }
        }

        let source_rel = format!("covers/{manga_id}.jpg");
        let source_path = library_path.join(&source_rel);
        let restored_path: Option<String> = match tokio::fs::metadata(&source_path).await {
            Ok(_) => Some(source_rel),
            Err(_) => None,
        };

        sqlx::query!(
            "UPDATE manga SET local_cover_path = ?, cover_overridden = FALSE WHERE id = ?",
            restored_path,
            manga_id,
        )
        .execute(&self.db)
        .await?;

        self.audit(Some(user_id), "manga.cover.override_cleared", None, None)
            .await;
        Ok(())
    }

    pub async fn fetch_and_store_chapter_page(
        &self,
        source_id: i64,
        manga_id: &str,
        manga_row_id: i64,
        page: i32,
    ) -> Result<bool> {
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&source_id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {} not found", source_id)))?
        };
        let res = source_manager
            .lease_instance()
            .await?
            .get_chapter_list(manga_id, page, None, None)
            .await?;
        let json = serde_json::to_string(&res)
            .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))?;
        let chapter_list: wit_types::ChapterList = serde_json::from_str(&json)
            .map_err(|e| ServiceError::Internal(format!("Failed to parse chapter list: {}", e)))?;

        if chapter_list.chapters.is_empty() {
            return Ok(false);
        }

        for chunk in chapter_list.chapters.chunks(100) {
            let mut query_builder = sqlx::QueryBuilder::new(
                "INSERT OR IGNORE INTO chapters (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at, discovered_at) ",
            );

            query_builder.push_values(chunk, |mut b, chapter| {
                b.push_bind(manga_row_id)
                    .push_bind(decode_manga_id(&chapter.id))
                    .push_bind(chapter.title.clone())
                    .push_bind(chapter.number)
                    .push_bind(chapter.language.clone())
                    .push_bind(chapter.volume)
                    .push_bind(chapter.scanlator.clone())
                    .push_bind(chapter.date_uploaded);
                b.push("NULL");
            });

            query_builder.build().execute(&self.db).await?;
        }

        Ok(chapter_list.has_next_page)
    }

    /// Queues a background scan for every manga in the library.
    /// Returns immediately with the count of manga queued.
    pub async fn scan_all_manga(&self) -> Result<usize> {
        let rows: Vec<(i64, String)> = sqlx::query!("SELECT id, name FROM manga ORDER BY id")
            .fetch_all(&self.db)
            .await?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect();
        let ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();
        let count = ids.len();
        self.scan_manga_ids_inner(ids).await?;
        Ok(count)
    }

    /// Scans a specific list of manga IDs for new chapters, emitting SSE progress
    /// events compatible with the scan-all event stream. The caller receives
    /// `Started`, per-manga `MangaRefreshed`, and `Completed` events.
    pub async fn scan_manga_ids(&self, ids: Vec<i64>) -> Result<()> {
        self.scan_manga_ids_inner(ids).await
    }

    async fn scan_manga_ids_inner(&self, ids: Vec<i64>) -> Result<()> {
        // Resolve manga names for the given IDs.
        let ids_json = serde_json::to_string(&ids).unwrap_or_default();
        let rows: Vec<(i64, String)> = sqlx::query!(
            "SELECT id, name FROM manga WHERE id IN (SELECT value FROM json_each(?)) ORDER BY id",
            ids_json
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect();

        let total = rows.len();
        let manga_ids: Vec<i64> = rows.iter().map(|(id, _)| *id).collect();

        let _ = self
            .refresh_tx
            .send(AppEvent::Refresh(RefreshProgressEvent::Started {
                total,
                manga_ids,
            }));

        let service = self.clone();
        tokio::task::spawn(async move {
            let mut failed = 0usize;
            for (idx, (id, name)) in rows.into_iter().enumerate() {
                let completed = idx + 1;
                let result = service.scan_for_new_chapters(id).await;
                let (success, new_chapters) = match result {
                    Ok(ref ch_ids) => (true, ch_ids.len() as u32),
                    Err(_) => {
                        failed += 1;
                        (false, 0)
                    }
                };
                let _ = service.refresh_tx.send(AppEvent::Refresh(
                    RefreshProgressEvent::MangaRefreshed {
                        manga_id: id,
                        manga_name: name,
                        completed,
                        total,
                        success,
                        new_chapters,
                    },
                ));
            }
            let _ = service
                .refresh_tx
                .send(AppEvent::Refresh(RefreshProgressEvent::Completed {
                    total,
                    failed,
                }));
        });
        Ok(())
    }

    pub async fn fetch_and_store_remaining_chapters(
        &self,
        source_id: i64,
        manga_id: String,
        manga_row_id: i64,
        start_page: i32,
    ) {
        let mut page = start_page;
        loop {
            match self
                .fetch_and_store_chapter_page(source_id, &manga_id, manga_row_id, page)
                .await
            {
                Ok(has_next_page) => {
                    if !has_next_page {
                        break;
                    }
                    page += 1;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to fetch chapter page {} for manga {}: {:?}",
                        page,
                        manga_id,
                        e
                    );
                    break;
                }
            }
        }
    }
}
