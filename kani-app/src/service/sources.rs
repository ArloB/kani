use super::*;
use crate::models::SourceHealthRow;

impl AppService {
    pub async fn get_source(&self, id: i64) -> Result<Source> {
        let source = sqlx::query_as!(
            Source,
            "SELECT id, name, version, base_url, enabled, favourited, unrestricted_http \
             FROM sources WHERE id = ? AND deleted_at IS NULL",
            id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or_else(|| ServiceError::NotFound("Source not found".into()))?;

        Ok(source)
    }

    pub async fn list_sources(&self) -> Result<Vec<Source>> {
        sqlx::query_as!(
            Source,
            "SELECT id, name, version, base_url, enabled, favourited, unrestricted_http \
             FROM sources WHERE deleted_at IS NULL LIMIT 1000"
        )
        .fetch_all(&self.db)
        .await
        .map_err(Into::into)
    }

    /// Inserts a new source row with a default version and returns its id.
    pub async fn add_source(&self, name: &str, user_id: i64) -> Result<i64> {
        let id = sqlx::query_scalar!(
            "INSERT INTO sources (name, version) VALUES (?, '0.1') RETURNING id",
            name
        )
        .fetch_one(&self.db)
        .await?;

        self.audit(Some(user_id), "source.install", Some(name), None)
            .await;
        Ok(id)
    }

    /// Updates the name and/or version of an existing source.
    pub async fn update_source(
        &self,
        id: i64,
        name: Option<String>,
        version: Option<String>,
    ) -> Result<()> {
        sqlx::query!(
            "UPDATE sources SET name = COALESCE(?, name), version = COALESCE(?, version) WHERE id = ?",
            name,
            version,
            id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// Soft-deletes a source: marks manga as orphaned, sets deleted_at on the source
    /// row, removes the WASM file, and evicts the source from the runtime. The source
    /// row is kept so manga.source_id FKs remain valid.
    pub async fn delete_source(&self, id: i64, user_id: i64) -> Result<()> {
        let row = sqlx::query!(
            "SELECT name FROM sources WHERE id = ? AND deleted_at IS NULL",
            id
        )
        .fetch_optional(&self.db)
        .await?;

        if let Some(row) = row {
            let mut tx = self.db.begin().await?;

            let orphaned = sqlx::query_scalar!(
                "UPDATE manga SET is_orphaned = TRUE WHERE source_id = ? RETURNING id",
                id
            )
            .fetch_all(&mut *tx)
            .await?
            .len() as i64;

            sqlx::query!(
                "UPDATE sources SET deleted_at = datetime('now'), enabled = FALSE WHERE id = ?",
                id
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            let wasm_path = self.settings.read().await.wasm_storage_path.clone();
            let storage = wasm_path
                .to_str()
                .ok_or_else(|| {
                    ServiceError::Internal("wasm_storage_path is not valid UTF-8".into())
                })?
                .to_owned();
            kani_core::file_storage::delete_wasm_file(&storage, &row.name)
                .await
                .map_err(ServiceError::Core)?;

            self.sources.write().await.remove(&id);
            self.cache.invalidate_source(id);
            self.audit(
                Some(user_id),
                "source.delete",
                Some(&row.name),
                Some(serde_json::json!({ "manga_orphaned": orphaned })),
            )
            .await;
        }

        Ok(())
    }

    pub async fn toggle_source_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        sqlx::query!("UPDATE sources SET enabled = ? WHERE id = ?", enabled, id)
            .execute(&self.db)
            .await?;
        Ok(())
    }

    pub async fn toggle_source_favourite(&self, id: i64, favourited: bool) -> Result<()> {
        sqlx::query!(
            "UPDATE sources SET favourited = ? WHERE id = ?",
            favourited,
            id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

    /// Returns the base URL of a source, or an empty string if the source has none.
    pub async fn get_source_base_url(&self, id: i64) -> Result<String> {
        Ok(sqlx::query_scalar!(
            "SELECT base_url FROM sources WHERE id = ? AND deleted_at IS NULL",
            id
        )
        .fetch_optional(&self.db)
        .await?
        .unwrap_or_default())
    }

    pub async fn get_metadata(&self, id: i64) -> Result<String> {
        self.require_source_active(id).await?;
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
        };

        let t0 = std::time::Instant::now();
        let result = source_manager.lease_instance().await?.get_metadata().await;
        let elapsed = t0.elapsed().as_millis() as u64;
        match result {
            Ok(r) => {
                self.record_source_success(id, elapsed).await;
                serde_json::to_string(&r).map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            }
            Err(e) => {
                self.record_source_error(id).await;
                Err(ServiceError::Core(e))
            }
        }
    }

    pub async fn get_popular_manga(
        &self,
        id: i64,
        page: i32,
        page_size: i32,
        filters: Option<String>,
    ) -> Result<String> {
        self.require_source_active(id).await?;
        let filters_key = filters.clone().unwrap_or_default();
        let active_filters: Vec<kani_shared::types::ActiveFilter> = filters
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let sources = self.sources.clone();

        self.cache
            .get_or_fetch_popular_manga(id, page, page_size, filters_key, async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .get_popular_manga(page, page_size, &active_filters)
                    .await?;
                serde_json::to_string(&result)
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn search_manga(
        &self,
        id: i64,
        query: &str,
        page: i32,
        page_size: i32,
        filters: Option<String>,
    ) -> Result<String> {
        self.require_source_active(id).await?;
        let filters_key = filters.clone().unwrap_or_default();
        let active_filters: Vec<kani_shared::types::ActiveFilter> = filters
            .as_deref()
            .filter(|s| !s.is_empty())
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        let sources = self.sources.clone();
        let q = query.to_string();
        self.cache
            .get_or_fetch_search_results(id, query, page, page_size, filters_key, async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .search_manga(&q, page, page_size, &active_filters)
                    .await?;
                serde_json::to_string(&result)
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_filter_list(&self, id: i64) -> Result<kani_core::WitFilterList> {
        self.require_source_active(id).await?;
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
        };
        let t0 = std::time::Instant::now();
        let result = source_manager
            .lease_instance()
            .await?
            .get_filter_list()
            .await;
        let elapsed = t0.elapsed().as_millis() as u64;
        match result {
            Ok(r) => {
                self.record_source_success(id, elapsed).await;
                Ok(r)
            }
            Err(e) => {
                self.record_source_error(id).await;
                Err(ServiceError::Core(e))
            }
        }
    }

    pub async fn get_source_url(&self, id: i64, manga_id: &str) -> Result<String> {
        self.require_source_active(id).await?;
        let manga_id_d = decode_manga_id(manga_id);
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
        };
        source_manager
            .lease_instance()
            .await?
            .get_url(&manga_id_d)
            .await
            .map_err(ServiceError::Core)
    }

    pub async fn get_manga_details(&self, id: i64, manga_id: &str) -> Result<String> {
        self.require_source_active(id).await?;
        let sources = self.sources.clone();
        let manga_id_d = decode_manga_id(manga_id);

        self.cache
            .get_or_fetch_manga_details(id, &manga_id_d.clone(), async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .get_manga_details(&manga_id_d)
                    .await?;
                serde_json::to_string(&convert_to_shared_manga_info(result))
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_pages(&self, id: i64, manga_id: &str, chapter_id: &str) -> Result<String> {
        self.require_source_active(id).await?;
        let sources = self.sources.clone();
        let manga_id_d = decode_manga_id(manga_id);
        let chapter_id_d = decode_manga_id(chapter_id);

        self.cache
            .get_or_fetch_pages(id, &manga_id_d.clone(), &chapter_id_d.clone(), async move {
                let source_manager = {
                    let sources = sources.read().await;
                    sources
                        .get(&id)
                        .cloned()
                        .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
                };
                let result = source_manager
                    .lease_instance()
                    .await?
                    .get_pages(&manga_id_d, &chapter_id_d)
                    .await?;
                serde_json::to_string(&result)
                    .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
            })
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_chapter_list_paged(
        &self,
        id: i64,
        manga_id: &str,
        page: i32,
        page_size: i32,
        sort: Option<String>,
    ) -> Result<String> {
        self.require_source_active(id).await?;
        let sources = self.sources.clone();
        let manga_id_d = decode_manga_id(manga_id);
        let sort_key = sort.clone().unwrap_or_default();

        self.cache
            .get_or_fetch_chapter_list(
                id,
                &manga_id_d.clone(),
                page,
                page_size,
                &sort_key,
                async move {
                    let source_manager = {
                        let sources = sources.read().await;
                        sources.get(&id).cloned().ok_or_else(|| {
                            ServiceError::NotFound(format!("Source {id} not found"))
                        })?
                    };
                    let result = source_manager
                        .lease_instance()
                        .await?
                        .get_chapter_list(&manga_id_d, page, Some(page_size), sort)
                        .await?;
                    serde_json::to_string(&result)
                        .map_err(|e| ServiceError::Core(kani_core::Error::Json(e)))
                },
            )
            .await
            .map_err(unwrap_cache_err)
    }

    pub async fn get_chapter_sort_list(
        &self,
        id: i64,
    ) -> Result<Vec<kani_shared::types::ChapterSortOption>> {
        self.require_source_active(id).await?;
        let source_manager = {
            let sources = self.sources.read().await;
            sources
                .get(&id)
                .cloned()
                .ok_or_else(|| ServiceError::NotFound(format!("Source {id} not found")))?
        };
        let wit_opts = source_manager
            .lease_instance()
            .await?
            .get_chapter_sort_list()
            .await?;
        Ok(wit_opts
            .into_iter()
            .map(|o| kani_shared::types::ChapterSortOption {
                id: o.id,
                name: o.name,
            })
            .collect())
    }

    async fn require_source_active(&self, id: i64) -> Result<()> {
        {
            let sources = self.sources.read().await;
            if sources.contains_key(&id) {
                return Ok(());
            }
        }
        let row = sqlx::query!(
            "SELECT enabled FROM sources WHERE id = ? AND deleted_at IS NULL",
            id
        )
        .fetch_optional(&self.db)
        .await?;
        match row {
            None => Err(ServiceError::NotFound(format!("Source {id} not found"))),
            Some(r) if !r.enabled => Err(ServiceError::SourceDisabled(id)),
            Some(_) => Err(ServiceError::NotFound(format!("Source {id} not found"))),
        }
    }

    pub async fn global_search(
        &self,
        query: &str,
        scope: SearchScope,
        page: i32,
        page_size: i32,
    ) -> Result<Vec<GlobalSearchResult>> {
        let favourited_only = matches!(scope, SearchScope::FavouritedOnly) as i64;

        let ids_to_search: IndexMap<i64, String> = sqlx::query!(
            "SELECT id, name FROM sources WHERE enabled = 1 AND deleted_at IS NULL AND (favourited = 1 OR ? = 0)",
            favourited_only
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .filter(|r| match &scope {
            SearchScope::Sources(ids) => ids.contains(&r.id),
            _ => true,
        })
        .map(|r| (r.id, r.name))
        .collect();

        let tasks: Vec<_> = ids_to_search
            .iter()
            .map(|(&source_id, source_name)| {
                let state = self.clone();
                let q = query.to_string();
                let source_name = source_name.clone();

                tokio::spawn(async move {
                    let result = state
                        .search_manga(source_id, &q, page, page_size, None)
                        .await;
                    (source_id, source_name, result)
                })
            })
            .collect();

        let outcomes = futures::future::join_all(tasks).await;

        let mut per_source_results: Vec<GlobalSearchResult> = Vec::new();

        for outcome in outcomes {
            match outcome {
                Ok((source_id, source_name, Ok(json))) => {
                    match serde_json::from_str::<MangaList>(&json) {
                        Ok(manga_list) => {
                            per_source_results.push(GlobalSearchResult {
                                source_id,
                                source_name,
                                has_next_page: manga_list.has_next_page,
                                manga: manga_list.manga,
                            });
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse results for source {source_id}: {e}");
                            return Err(ServiceError::Core(kani_core::Error::Json(e)));
                        }
                    }
                }
                Ok((source_id, _, Err(e))) => {
                    tracing::warn!("Search failed for source {source_id}: {e}");
                    per_source_results.push(GlobalSearchResult {
                        source_id,
                        source_name: ids_to_search.get(&source_id).cloned().unwrap_or_default(),
                        has_next_page: false,
                        manga: vec![],
                    });
                }
                Err(join_err) => {
                    tracing::error!("Task panicked: {join_err}");
                }
            }
        }

        Ok(per_source_results)
    }

    pub async fn get_source_health(&self) -> Result<Vec<SourceHealthRow>> {
        let rows = sqlx::query_as::<_, SourceHealthRow>(
            r#"SELECT
                s.id AS source_id,
                s.name AS source_name,
                sh.last_success_at,
                sh.last_error_at,
                COALESCE(sh.consecutive_error_count, 0) AS consecutive_error_count,
                sh.avg_response_ms
            FROM sources s
            LEFT JOIN source_health sh ON sh.source_id = s.id
            WHERE s.deleted_at IS NULL
            ORDER BY s.name"#,
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows)
    }

    pub async fn record_source_success(&self, source_id: i64, elapsed_ms: u64) {
        let ms = elapsed_ms as f64;
        let _ = sqlx::query(
            r#"INSERT INTO source_health (source_id, last_success_at, consecutive_error_count, avg_response_ms)
               VALUES (?, datetime('now'), 0, ?)
               ON CONFLICT(source_id) DO UPDATE SET
                 last_success_at = datetime('now'),
                 consecutive_error_count = 0,
                 avg_response_ms = CASE
                   WHEN avg_response_ms IS NULL THEN excluded.avg_response_ms
                   ELSE (avg_response_ms * 0.8 + excluded.avg_response_ms * 0.2)
                 END"#,
        )
        .bind(source_id)
        .bind(ms)
        .execute(&self.db)
        .await;
    }

    pub async fn record_source_error(&self, source_id: i64) {
        let _ = sqlx::query(
            r#"INSERT INTO source_health (source_id, last_error_at, consecutive_error_count)
               VALUES (?, datetime('now'), 1)
               ON CONFLICT(source_id) DO UPDATE SET
                 last_error_at = datetime('now'),
                 consecutive_error_count = consecutive_error_count + 1"#,
        )
        .bind(source_id)
        .execute(&self.db)
        .await;
    }

    pub(crate) async fn scan_and_register_sources(
        db: &SqlitePool,
        wasm_storage_path: &std::path::Path,
        smart_client: kani_core::http::SmartClient,
        wasm_runtime: &WasmRuntime,
        preference_schemas: &DashMap<i64, Vec<kani_core::PreferenceSpec>>,
    ) -> Result<()> {
        tracing::info!(
            "Scanning and registering sources in {:?}",
            wasm_storage_path
        );

        let mut entries = tokio::fs::read_dir(wasm_storage_path)
            .await
            .map_err(|e| ServiceError::Core(kani_core::Error::Io(e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ServiceError::Core(kani_core::Error::Io(e)))?
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                let filename = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| ServiceError::Internal("Invalid filename".to_string()))?
                    .to_owned();

                let bytes = tokio::fs::read(&path)
                    .await
                    .map_err(|e| ServiceError::Core(kani_core::Error::Io(e)))?;

                let component = wasm_runtime
                    .compile_component(&bytes)
                    .map_err(ServiceError::Core)?;

                let (raw_meta, schema) = {
                    let mut inst =
                        kani_core::sources::SourceInstance::new(smart_client.clone(), None, false);
                    inst.load(wasm_runtime.engine(), &component, wasm_runtime.linker())
                        .await
                        .map_err(ServiceError::Core)?;

                    let meta = inst.get_metadata().await.map_err(ServiceError::Core)?;
                    let schema = inst.get_preferences().await.ok();
                    (meta, schema)
                };

                let metadata = match serde_json::to_value(&raw_meta)
                    .and_then(serde_json::from_value::<kani_shared::ExtensionMetadata>)
                {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::error!("Failed to convert metadata for {}: {}", filename, e);
                        continue;
                    }
                };

                let canonical_id = metadata.id.clone();
                let initially_enabled = if metadata.unrestricted_http {
                    0i64
                } else {
                    1i64
                };

                let rename_file = |current_filename: &str| {
                    let src = wasm_storage_path.join(format!("{current_filename}.wasm"));
                    let dst = wasm_storage_path.join(format!("{canonical_id}.wasm"));
                    (src, dst, current_filename != canonical_id.as_str())
                };

                // Already registered under the canonical id — sync metadata, re-activate if
                // soft-deleted, and ensure the file is named correctly.
                let by_id = sqlx::query!(
                    "SELECT id, version, base_url, unrestricted_http, mihon_source_id, deleted_at FROM sources WHERE name = ?",
                    canonical_id
                )
                .fetch_optional(db)
                .await?;

                if let Some(existing) = by_id {
                    if existing.version != metadata.version {
                        tracing::warn!(
                            "Source '{}' version changed: DB has '{}', loaded '{}'",
                            canonical_id,
                            existing.version,
                            metadata.version
                        );
                    }
                    let version_changed = existing.version != metadata.version;
                    let base_url_changed = existing.base_url != metadata.base_url;
                    let http_changed = existing.unrestricted_http != metadata.unrestricted_http;
                    let mihon_changed = existing.mihon_source_id != metadata.mihon_source_id;
                    let was_deleted = existing.deleted_at.is_some();
                    if version_changed
                        || base_url_changed
                        || http_changed
                        || mihon_changed
                        || was_deleted
                    {
                        sqlx::query!(
                            "UPDATE sources SET version = ?, base_url = ?, unrestricted_http = ?, mihon_source_id = ?, deleted_at = NULL WHERE id = ?",
                            metadata.version,
                            metadata.base_url,
                            metadata.unrestricted_http,
                            metadata.mihon_source_id,
                            existing.id
                        )
                        .execute(db)
                        .await?;
                        if was_deleted {
                            sqlx::query!(
                                "UPDATE manga SET is_orphaned = FALSE WHERE source_id = ?",
                                existing.id
                            )
                            .execute(db)
                            .await?;
                            tracing::info!(
                                "Re-activated previously deleted source '{}'",
                                canonical_id
                            );
                        } else {
                            tracing::debug!("Synced metadata for source '{}'", canonical_id);
                        }
                    }
                    let (src, dst, needs_rename) = rename_file(&filename);
                    if needs_rename
                        && src.exists()
                        && !dst.exists()
                        && let Err(e) = tokio::fs::rename(&src, &dst).await
                    {
                        tracing::warn!(
                            "Failed to rename {} → {}: {}",
                            src.display(),
                            dst.display(),
                            e
                        );
                    }
                    continue;
                }

                let legacy = sqlx::query!(
                    "SELECT id FROM sources WHERE name = ? OR name = ? LIMIT 1",
                    filename,
                    metadata.name
                )
                .fetch_optional(db)
                .await?;

                if let Some(row) = legacy {
                    sqlx::query!(
                        "UPDATE sources SET name = ?, version = ?, base_url = ?, unrestricted_http = ?, mihon_source_id = ? WHERE id = ?",
                        canonical_id,
                        metadata.version,
                        metadata.base_url,
                        metadata.unrestricted_http,
                        metadata.mihon_source_id,
                        row.id
                    )
                    .execute(db)
                    .await?;

                    let (src, dst, needs_rename) = rename_file(&filename);
                    if needs_rename
                        && src.exists()
                        && !dst.exists()
                        && let Err(e) = tokio::fs::rename(&src, &dst).await
                    {
                        tracing::warn!(
                            "Failed to rename {} → {}: {}",
                            src.display(),
                            dst.display(),
                            e
                        );
                    }

                    tracing::info!("Migrated source '{}' → '{}'", filename, canonical_id);
                    continue;
                }

                let result = sqlx::query!(
                    "INSERT INTO sources (name, version, base_url, enabled, unrestricted_http, mihon_source_id) VALUES (?, ?, ?, ?, ?, ?)",
                    canonical_id,
                    metadata.version,
                    metadata.base_url,
                    initially_enabled,
                    metadata.unrestricted_http,
                    metadata.mihon_source_id,
                )
                .execute(db)
                .await?;

                if let Some(schema) = schema {
                    preference_schemas.insert(result.last_insert_rowid(), schema);
                }

                let (src, dst, needs_rename) = rename_file(&filename);
                if needs_rename
                    && src.exists()
                    && !dst.exists()
                    && let Err(e) = tokio::fs::rename(&src, &dst).await
                {
                    tracing::warn!(
                        "Failed to rename {} → {}: {}",
                        src.display(),
                        dst.display(),
                        e
                    );
                }

                tracing::info!("Registered new source: {}", canonical_id);
            }
        }
        Ok(())
    }
}
