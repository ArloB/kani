use std::io::{Cursor, Read as _, Write as _};

use serde::{Deserialize, Serialize};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use super::AppService;
use crate::error::{Result, ServiceError};
use crate::events::AppEvent;

// ── Serialisable backup structs ───────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupData {
    pub version: u32,
    pub exported_at: String,
    pub manga: Vec<BackupManga>,
    pub categories: Vec<BackupCategory>,
    pub settings: Option<BackupSettings>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupManga {
    pub source_name: String,
    pub source_manga_id: String,
    pub name: String,
    pub status: Option<i64>,
    pub auto_download: bool,
    pub auto_scan: bool,
    pub scanlator_mode: String,
    pub categories: Vec<String>,
    pub tracking: Option<BackupMangaTracking>,
    pub download_rules: Vec<BackupDownloadRule>,
    pub chapter_progress: Vec<BackupChapterProgress>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupMangaTracking {
    pub status: i64,
    pub score: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupChapterProgress {
    pub source_chapter_id: String,
    pub is_read: bool,
    pub last_page_read: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupDownloadRule {
    pub rule_type: String,
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupCategory {
    pub name: String,
    pub sort_order: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupSettings {
    pub scan_interval_minutes: i64,
    pub auto_scan: bool,
    pub concurrent_page_downloads: i64,
    pub concurrent_manga_downloads: i64,
}

// ── Preview (read-only, no DB writes) ────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct BackupPreview {
    pub version: u32,
    pub exported_at: String,
    pub manga_count: u32,
    pub category_count: u32,
    pub download_rule_count: u32,
    pub has_tracking: bool,
    pub has_chapter_progress: bool,
    pub has_settings: bool,
    pub sources: Vec<BackupSourceSummary>,
}

#[derive(Debug, Serialize)]
pub struct BackupSourceSummary {
    pub source_name: String,
    pub manga_count: u32,
    pub found: bool,
}

// ── Restore options + result ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RestoreOptions {
    pub merge: bool,
    pub import_manga: bool,
    pub import_categories: bool,
    pub import_download_rules: bool,
    pub import_tracking: bool,
    pub import_chapter_progress: bool,
    pub import_settings: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            merge: false,
            import_manga: true,
            import_categories: true,
            import_download_rules: true,
            import_tracking: true,
            import_chapter_progress: false,
            import_settings: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub imported_manga: u32,
    pub skipped_manga: u32,
    pub possible_duplicates: u32,
    pub imported_categories: u32,
    pub pending_imports_added: u32,
    pub warnings: Vec<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_zip_backup(data: &[u8]) -> Result<BackupData> {
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| ServiceError::Validation(format!("Not a valid ZIP file: {e}")))?;

    let mut json_bytes = Vec::new();
    {
        let mut file = archive.by_name("backup.json").map_err(|_| {
            ServiceError::Validation("ZIP does not contain backup.json".into())
        })?;
        file.read_to_end(&mut json_bytes)
            .map_err(ServiceError::Io)?;
    }

    let data: BackupData = serde_json::from_slice(&json_bytes)
        .map_err(|e| ServiceError::Validation(format!("Invalid backup JSON: {e}")))?;

    if data.version != 1 {
        return Err(ServiceError::Validation(format!(
            "Unsupported backup version {}",
            data.version
        )));
    }

    Ok(data)
}

fn build_zip(backup: &BackupData) -> Result<Vec<u8>> {
    let buf = Vec::new();
    let cursor = Cursor::new(buf);
    let mut zip = ZipWriter::new(cursor);

    let opts = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("backup.json", opts)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    let json = serde_json::to_vec_pretty(backup)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    zip.write_all(&json)
        .map_err(ServiceError::Io)?;

    zip.start_file("VERSION", opts)
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    zip.write_all(b"1").map_err(ServiceError::Io)?;

    let cursor = zip
        .finish()
        .map_err(|e| ServiceError::Internal(e.to_string()))?;
    Ok(cursor.into_inner())
}

// ── AppService methods ────────────────────────────────────────────────────────

impl AppService {
    /// Parse the backup ZIP and return metadata without writing to the DB.
    pub async fn preview_backup(&self, data: &[u8]) -> Result<BackupPreview> {
        let backup = parse_zip_backup(data)?;

        let mut source_counts: indexmap::IndexMap<String, u32> = Default::default();
        let mut download_rule_count: u32 = 0;
        let mut has_tracking = false;
        let mut has_chapter_progress = false;

        for m in &backup.manga {
            *source_counts.entry(m.source_name.clone()).or_insert(0) += 1;
            download_rule_count += m.download_rules.len() as u32;
            if m.tracking.is_some() {
                has_tracking = true;
            }
            if !m.chapter_progress.is_empty() {
                has_chapter_progress = true;
            }
        }

        let mut sources = Vec::new();
        for (name, count) in source_counts {
            let found = sqlx::query_scalar!(
                "SELECT id FROM sources WHERE name = ? AND deleted_at IS NULL",
                name
            )
            .fetch_optional(&self.db)
            .await?
            .is_some();
            sources.push(BackupSourceSummary {
                source_name: name,
                manga_count: count,
                found,
            });
        }

        Ok(BackupPreview {
            version: backup.version,
            exported_at: backup.exported_at,
            manga_count: backup.manga.len() as u32,
            category_count: backup.categories.len() as u32,
            download_rule_count,
            has_tracking,
            has_chapter_progress,
            has_settings: backup.settings.is_some(),
            sources,
        })
    }

    /// Export the library to a ZIP archive of JSON.
    pub async fn export_backup(
        &self,
        user_id: i64,
        include_chapter_progress: bool,
    ) -> Result<Vec<u8>> {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| "unknown".into());

        // All manga with source names
        let rows = sqlx::query!(
            "SELECT m.id, m.source_manga_id, m.name, m.status, m.auto_download, m.auto_scan,
                    m.scanlator_mode, s.name AS source_name
             FROM manga m
             JOIN sources s ON s.id = m.source_id",
        )
        .fetch_all(&self.db)
        .await?;

        let mut manga_out = Vec::with_capacity(rows.len());

        for row in rows {
            let categories: Vec<String> = sqlx::query_scalar!(
                "SELECT c.name FROM manga_categories mc \
                 JOIN categories c ON c.id = mc.category_id \
                 WHERE mc.manga_id = ?",
                row.id
            )
            .fetch_all(&self.db)
            .await?;

            let tracking = sqlx::query!(
                "SELECT status, score FROM user_manga_tracking \
                 WHERE manga_id = ? AND user_id = ?",
                row.id,
                user_id
            )
            .fetch_optional(&self.db)
            .await?
            .map(|r| BackupMangaTracking {
                status: r.status,
                score: r.score,
            });

            let download_rules: Vec<BackupDownloadRule> = sqlx::query!(
                "SELECT rule_type, value FROM download_rules WHERE manga_id = ?",
                row.id
            )
            .fetch_all(&self.db)
            .await?
            .into_iter()
            .map(|r| BackupDownloadRule {
                rule_type: r.rule_type,
                value: r.value,
            })
            .collect();

            let chapter_progress = if include_chapter_progress {
                sqlx::query!(
                    "SELECT c.source_chapter_id, uct.is_read, uct.last_page_read \
                     FROM user_chapter_tracking uct \
                     JOIN chapters c ON c.id = uct.chapter_id \
                     WHERE c.manga_id = ? AND uct.user_id = ?",
                    row.id,
                    user_id
                )
                .fetch_all(&self.db)
                .await?
                .into_iter()
                .map(|r| BackupChapterProgress {
                    source_chapter_id: r.source_chapter_id,
                    is_read: r.is_read,
                    last_page_read: r.last_page_read,
                })
                .collect()
            } else {
                vec![]
            };

            manga_out.push(BackupManga {
                source_name: row.source_name,
                source_manga_id: row.source_manga_id,
                name: row.name,
                status: Some(row.status),
                auto_download: row.auto_download,
                auto_scan: row.auto_scan,
                scanlator_mode: row.scanlator_mode,
                categories,
                tracking,
                download_rules,
                chapter_progress,
            });
        }

        let categories: Vec<BackupCategory> = sqlx::query!(
            "SELECT name, sort_order FROM categories ORDER BY sort_order"
        )
        .fetch_all(&self.db)
        .await?
        .into_iter()
        .map(|r| BackupCategory {
            name: r.name,
            sort_order: r.sort_order,
        })
        .collect();

        let s = self.settings.read().await;
        let settings = Some(BackupSettings {
            scan_interval_minutes: s.scan_interval_minutes,
            auto_scan: s.auto_scan,
            concurrent_page_downloads: s.concurrent_page_downloads,
            concurrent_manga_downloads: s.concurrent_manga_downloads,
        });
        drop(s);

        let backup = BackupData {
            version: 1,
            exported_at: now,
            manga: manga_out,
            categories,
            settings,
        };

        let bytes = build_zip(&backup)?;
        self.audit(Some(user_id), "backup.export", None, None).await;
        Ok(bytes)
    }

    /// Restore from a backup ZIP. Honours `opts` to select which sections to apply.
    pub async fn restore_backup(
        &self,
        user_id: i64,
        data: &[u8],
        opts: RestoreOptions,
    ) -> Result<RestoreResult> {
        let backup = parse_zip_backup(data)?;

        let mut result = RestoreResult {
            imported_manga: 0,
            skipped_manga: 0,
            possible_duplicates: 0,
            imported_categories: 0,
            pending_imports_added: 0,
            warnings: vec![],
        };

        let mut tx = self.db.begin().await?;

        if !opts.merge {
            sqlx::query!("DELETE FROM user_chapter_tracking WHERE user_id = ?", user_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query!("DELETE FROM user_manga_tracking WHERE user_id = ?", user_id)
                .execute(&mut *tx)
                .await?;
            if opts.import_manga {
                sqlx::query!("DELETE FROM manga_categories").execute(&mut *tx).await?;
                sqlx::query!("DELETE FROM download_rules").execute(&mut *tx).await?;
                sqlx::query!("DELETE FROM manga").execute(&mut *tx).await?;
            }
            if opts.import_categories {
                sqlx::query!("DELETE FROM categories").execute(&mut *tx).await?;
            }
        }

        // Insert / ensure categories exist, build name→id map
        let mut cat_map: std::collections::HashMap<String, i64> = Default::default();
        if opts.import_categories {
            for cat in &backup.categories {
                sqlx::query!(
                    "INSERT OR IGNORE INTO categories (name, sort_order) VALUES (?, ?)",
                    cat.name,
                    cat.sort_order
                )
                .execute(&mut *tx)
                .await?;
                result.imported_categories += 1;
            }
        }
        // Build map from existing categories (may include pre-existing ones in merge mode)
        let cats = sqlx::query!("SELECT id, name FROM categories")
            .fetch_all(&mut *tx)
            .await?;
        for c in cats {
            cat_map.insert(c.name, c.id);
        }

        tx.commit().await?;

        let mut new_manga_ids: Vec<i64> = Vec::new();

        let total_manga = if opts.import_manga { backup.manga.len() as u32 } else { 0 };
        if total_manga > 0 {
            let _ = self.refresh_tx.send(AppEvent::ImportStarted {
                origin: "kani_backup".into(),
                total: total_manga,
            });
        }
        // Process manga one at a time (each may do a fuzzy scan)
        for (processed, m) in (1_u32..).zip(backup.manga.iter()) {
            if !opts.import_manga {
                break;
            }

            let source_id: Option<i64> = sqlx::query_scalar!(
                "SELECT id FROM sources WHERE name = ? AND deleted_at IS NULL",
                m.source_name
            )
            .fetch_optional(&self.db)
            .await?;

            let source_id = match source_id {
                Some(id) => id,
                None => {
                    result.warnings.push(format!(
                        "Source '{}' not installed — '{}' saved to pending imports",
                        m.source_name, m.name
                    ));
                    self.save_pending_import_for_user(user_id, "kani_backup", m, None, None)
                        .await?;
                    result.pending_imports_added += 1;
                    result.skipped_manga += 1;
                    let _ = self.refresh_tx.send(AppEvent::ImportProgress {
                        origin: "kani_backup".into(),
                        completed: processed,
                        total: total_manga,
                        title: m.name.clone(),
                    });
                    continue;
                }
            };

            // Exact match check
            let existing_id: Option<i64> = sqlx::query_scalar!(
                "SELECT id FROM manga WHERE source_id = ? AND source_manga_id = ?",
                source_id,
                m.source_manga_id
            )
            .fetch_optional(&self.db)
            .await?;

            let manga_id = if let Some(id) = existing_id {
                id
            } else {
                // Fuzzy duplicate check
                let authors: Vec<String> = vec![];
                let hits = crate::service::dedup::find_similar_manga(
                    &self.db,
                    &m.name,
                    &authors,
                    None,
                )
                .await?;
                if !hits.is_empty() {
                    self.save_pending_import_for_user(
                        user_id,
                        "kani_backup",
                        m,
                        Some(hits[0].id),
                        Some(hits[0].similarity),
                    )
                    .await?;
                    result.possible_duplicates += 1;
                    result.pending_imports_added += 1;
                    let _ = self.refresh_tx.send(AppEvent::ImportProgress {
                        origin: "kani_backup".into(),
                        completed: processed,
                        total: total_manga,
                        title: m.name.clone(),
                    });
                    continue;
                }

                let mut tx = self.db.begin().await?;
                let status = m.status.unwrap_or(0);
                let id = sqlx::query_scalar!(
                    "INSERT INTO manga (source_id, source_manga_id, name, status, auto_download, auto_scan, scanlator_mode) \
                     VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
                    source_id,
                    m.source_manga_id,
                    m.name,
                    status,
                    m.auto_download,
                    m.auto_scan,
                    m.scanlator_mode
                )
                .fetch_one(&mut *tx)
                .await?;

                // manga_categories
                for cat_name in &m.categories {
                    if let Some(&cat_id) = cat_map.get(cat_name) {
                        sqlx::query!(
                            "INSERT OR IGNORE INTO manga_categories (manga_id, category_id) VALUES (?, ?)",
                            id,
                            cat_id
                        )
                        .execute(&mut *tx)
                        .await?;
                    }
                }

                // download_rules
                if opts.import_download_rules {
                    for rule in &m.download_rules {
                        sqlx::query!(
                            "INSERT INTO download_rules (manga_id, rule_type, value) VALUES (?, ?, ?)",
                            id,
                            rule.rule_type,
                            rule.value
                        )
                        .execute(&mut *tx)
                        .await?;
                    }
                }

                tx.commit().await?;
                id
            };

            // user_manga_tracking
            if opts.import_tracking
                && let Some(ref tr) = m.tracking {
                    sqlx::query!(
                        "INSERT OR REPLACE INTO user_manga_tracking (user_id, manga_id, status, score) \
                         VALUES (?, ?, ?, ?)",
                        user_id,
                        manga_id,
                        tr.status,
                        tr.score
                    )
                    .execute(&self.db)
                    .await?;
                }

            // user_chapter_tracking
            if opts.import_chapter_progress && !m.chapter_progress.is_empty() {
                for cp in &m.chapter_progress {
                    let chapter_id: Option<i64> = sqlx::query_scalar!(
                        "SELECT id FROM chapters WHERE manga_id = ? AND source_chapter_id = ?",
                        manga_id,
                        cp.source_chapter_id
                    )
                    .fetch_optional(&self.db)
                    .await?;

                    if let Some(ch_id) = chapter_id {
                        sqlx::query!(
                            "INSERT OR REPLACE INTO user_chapter_tracking \
                             (user_id, chapter_id, is_read, last_page_read) \
                             VALUES (?, ?, ?, ?)",
                            user_id,
                            ch_id,
                            cp.is_read,
                            cp.last_page_read
                        )
                        .execute(&self.db)
                        .await?;
                    }
                }
            }

            new_manga_ids.push(manga_id);
            result.imported_manga += 1;
            let _ = self.refresh_tx.send(AppEvent::ImportProgress {
                origin: "kani_backup".into(),
                completed: processed,
                total: total_manga,
                title: m.name.clone(),
            });
        }

        if total_manga > 0 {
            let _ = self.refresh_tx.send(AppEvent::ImportCompleted {
                origin: "kani_backup".into(),
                imported: result.imported_manga,
                skipped: result.skipped_manga,
                pending: result.pending_imports_added,
            });
        }

        // Settings
        if opts.import_settings
            && let Some(ref s) = backup.settings {
                sqlx::query!(
                    "UPDATE settings SET scan_interval_minutes = ?, auto_scan = ?, \
                     concurrent_page_downloads = ?, concurrent_manga_downloads = ? \
                     WHERE id = 'singleton'",
                    s.scan_interval_minutes,
                    s.auto_scan,
                    s.concurrent_page_downloads,
                    s.concurrent_manga_downloads
                )
                .execute(&self.db)
                .await?;
            }

        if !new_manga_ids.is_empty() {
            let pool = self.db.clone();
            tokio::spawn(async move {
                for id in new_manga_ids {
                    if let Err(e) =
                        crate::service::dedup::record_duplicates_for_manga(&pool, id).await
                    {
                        tracing::warn!("Duplicate recording failed for manga {id}: {e}");
                    }
                }
            });
        }

        self.cache.invalidate_stats(user_id);
        self.audit(
            Some(user_id),
            "backup.restore",
            None,
            Some(serde_json::json!({
                "imported": result.imported_manga,
                "skipped": result.skipped_manga,
                "pending": result.pending_imports_added,
            })),
        )
        .await;

        Ok(result)
    }

    /// Save a manga that couldn't be matched to the pending imports queue.
    pub async fn save_pending_import_for_user(
        &self,
        user_id: i64,
        origin: &str,
        m: &BackupManga,
        duplicate_of: Option<i64>,
        similarity: Option<f64>,
    ) -> Result<()> {
        let tracking = m
            .tracking
            .as_ref()
            .and_then(|t| serde_json::to_string(t).ok());
        let chapter_progress = if m.chapter_progress.is_empty() {
            None
        } else {
            serde_json::to_string(&m.chapter_progress).ok()
        };

        sqlx::query!(
            "INSERT INTO pending_imports \
             (user_id, origin, title, source_hint, source_manga_id, tracking, chapter_progress, \
              possible_duplicate_of, duplicate_similarity) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            user_id,
            origin,
            m.name,
            m.source_name,
            m.source_manga_id,
            tracking,
            chapter_progress,
            duplicate_of,
            similarity
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }

}
