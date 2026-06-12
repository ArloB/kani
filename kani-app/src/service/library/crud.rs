use super::super::*;
use crate::ids::UserId;

// Library CRUD: fetch, list, filter, delete and local-metadata updates.

impl AppService {
    pub async fn get_manga_by_id(&self, id: MangaId) -> Result<crate::models::Manga> {
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
    pub async fn delete_manga(&self, id: MangaId, user_id: UserId) -> Result<()> {
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
        user_id: UserId,
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

    /// Returns full manga details including parsed authors, artists, and tags.
    /// URL signing and markdown rendering are the caller's responsibility.
    pub async fn get_local_manga_details(
        &self,
        id: MangaId,
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

    /// Updates the local metadata override columns for a manga.
    /// Pass `None` for any field to leave it unchanged.
    /// For people/tags, pass `Some(vec![])` to clear the override entirely.
    pub async fn update_local_metadata(
        &self,
        manga_id: MangaId,
        update: crate::models::LocalMetadataUpdate,
        user_id: UserId,
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
}
