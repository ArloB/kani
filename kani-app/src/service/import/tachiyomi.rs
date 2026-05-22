use std::io::Read as _;

use flate2::read::GzDecoder;
use prost::Message as _;
use serde::{Deserialize, Serialize};

use crate::error::{Result, ServiceError};
use crate::events::AppEvent;
use crate::service::AppService;
use crate::service::backup::BackupManga as KaniBackupManga;
use crate::service::backup::{BackupChapterProgress, BackupMangaTracking};

use super::tachiyomi_sources::{tachiyomi_source_to_kani_name, tachiyomi_sync_id_to_tracker_name};

// ── Generated protobuf types ──────────────────────────────────────────────────

include!(concat!(env!("OUT_DIR"), "/tachiyomi.rs"));

// ── Preview ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TachiyomiPreview {
    pub total_manga: u32,
    pub category_count: u32,
    pub has_tracking: bool,
    pub has_chapter_progress: bool,
    pub sources: Vec<TachiyomiSourceSummary>,
    pub pending_import_estimate: u32,
}

#[derive(Debug, Serialize)]
pub struct TachiyomiSourceSummary {
    pub source_id: i64,
    pub source_name: String,
    pub manga_count: u32,
    pub found: bool,
}

// ── Import options + result ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TachiyomiImportOptions {
    pub import_manga: bool,
    pub import_categories: bool,
    pub import_tracking: bool,
    pub import_chapter_progress: bool,
}

impl Default for TachiyomiImportOptions {
    fn default() -> Self {
        Self {
            import_manga: true,
            import_categories: true,
            import_tracking: true,
            import_chapter_progress: false,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TachiyomiImportResult {
    pub imported_manga: u32,
    pub skipped_manga: u32,
    pub imported_categories: u32,
    pub pending_imports_added: u32,
    pub possible_duplicates: u32,
    pub warnings: Vec<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn decode_backup(data: &[u8]) -> Result<Backup> {
    let mut gz = GzDecoder::new(data);
    let mut buf = Vec::new();
    gz.read_to_end(&mut buf)
        .map_err(|e| ServiceError::Validation(format!("Failed to decompress .tachibk: {e}")))?;

    Backup::decode(buf.as_slice())
        .map_err(|e| ServiceError::Validation(format!("Failed to decode Tachiyomi proto: {e}")))
}

/// Map Tachiyomi manga publication status (BackupManga.status) to Kani's manga.status.
fn map_publication_status(tachi_status: i32) -> i64 {
    match tachi_status {
        1 => 1, // Ongoing
        2 => 2, // Completed
        _ => 0, // Unknown
    }
}

/// Map Tachiyomi tracker reading status (BackupTracking.status) to Kani's user_manga_tracking.status.
/// Tachiyomi: 1=Reading, 2=Completed, 3=OnHold, 4=Dropped, 5=PlanToRead, 6=Rereading
/// Kani:      0=Reading, 4=Completed, 1=OnHold, 2=Dropped, 3=PlanToRead, 5=Rereading
fn map_reading_status(tachi_status: i32) -> i64 {
    match tachi_status {
        1 => 0,
        2 => 4,
        3 => 1,
        4 => 2,
        5 => 3,
        6 => 5,
        _ => 0,
    }
}

// ── AppService methods ────────────────────────────────────────────────────────

impl AppService {
    pub async fn preview_tachiyomi_backup(&self, data: &[u8]) -> Result<TachiyomiPreview> {
        let backup = decode_backup(data)?;

        let mut source_counts: std::collections::HashMap<i64, u32> = Default::default();
        let mut has_tracking = false;
        let mut has_chapter_progress = false;
        let mut pending_estimate: u32 = 0;

        for m in &backup.backup_manga {
            *source_counts.entry(m.source).or_insert(0) += 1;
            if !m.tracking.is_empty() {
                has_tracking = true;
            }
            if m.chapters.iter().any(|c| c.read || c.last_page_read > 0) {
                has_chapter_progress = true;
            }
        }

        let mut sources = Vec::new();
        for (&source_id, &count) in &source_counts {
            let kani_name = tachiyomi_source_to_kani_name(source_id);
            let found = if let Some(name) = kani_name {
                sqlx::query_scalar!(
                    "SELECT id FROM sources WHERE name = ? AND deleted_at IS NULL",
                    name
                )
                .fetch_optional(&self.db)
                .await?
                .is_some()
            } else {
                false
            };

            if !found {
                pending_estimate += count;
            }

            sources.push(TachiyomiSourceSummary {
                source_id,
                source_name: kani_name.unwrap_or("Unknown").to_string(),
                manga_count: count,
                found,
            });
        }

        sources.sort_by(|a, b| b.manga_count.cmp(&a.manga_count));

        Ok(TachiyomiPreview {
            total_manga: backup.backup_manga.len() as u32,
            category_count: backup.backup_categories.len() as u32,
            has_tracking,
            has_chapter_progress,
            sources,
            pending_import_estimate: pending_estimate,
        })
    }

    pub async fn import_tachiyomi_backup(
        &self,
        user_id: i64,
        data: &[u8],
        opts: TachiyomiImportOptions,
    ) -> Result<TachiyomiImportResult> {
        let backup = decode_backup(data)?;

        let mut result = TachiyomiImportResult {
            imported_manga: 0,
            skipped_manga: 0,
            imported_categories: 0,
            pending_imports_added: 0,
            possible_duplicates: 0,
            warnings: vec![],
        };

        // Build Tachiyomi category index → Kani category id map
        let mut tachi_cat_map: std::collections::HashMap<i32, i64> = Default::default();

        if opts.import_categories {
            for (idx, cat) in backup.backup_categories.iter().enumerate() {
                sqlx::query!(
                    "INSERT OR IGNORE INTO categories (name, sort_order) VALUES (?, ?)",
                    cat.name,
                    cat.order
                )
                .execute(&self.db)
                .await?;
                result.imported_categories += 1;

                let cat_id: Option<i64> =
                    sqlx::query_scalar!("SELECT id FROM categories WHERE name = ?", cat.name)
                        .fetch_optional(&self.db)
                        .await?;

                if let Some(id) = cat_id {
                    tachi_cat_map.insert(idx as i32, id);
                }
            }
        }

        let mut new_manga_ids: Vec<i64> = Vec::new();

        let total_manga = if opts.import_manga {
            backup.backup_manga.len() as u32
        } else {
            0
        };
        if total_manga > 0 {
            let _ = self.refresh_tx.send(AppEvent::ImportStarted {
                origin: "tachiyomi".into(),
                total: total_manga,
            });
        }
        for (processed, m) in (1_u32..).zip(backup.backup_manga.iter()) {
            if !opts.import_manga {
                break;
            }

            let kani_name = tachiyomi_source_to_kani_name(m.source);

            let source_id: Option<i64> = if let Some(name) = kani_name {
                sqlx::query_scalar!(
                    "SELECT id FROM sources WHERE name = ? AND deleted_at IS NULL",
                    name
                )
                .fetch_optional(&self.db)
                .await?
            } else {
                None
            };

            let source_id = match source_id {
                Some(id) => id,
                None => {
                    let kani_hint = kani_name.unwrap_or("Unknown");
                    result.warnings.push(format!(
                        "Source '{}' (Tachiyomi ID {}) not installed — '{}' saved to pending imports",
                        kani_hint, m.source, m.title
                    ));
                    let proxy = self.make_tachi_backup_manga(m);
                    let source_hint = kani_name
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("Tachiyomi:{}", m.source));
                    self.save_pending_import_tachiyomi(user_id, &proxy, &source_hint, None, None)
                        .await?;
                    result.pending_imports_added += 1;
                    result.skipped_manga += 1;
                    let _ = self.refresh_tx.send(AppEvent::ImportProgress {
                        origin: "tachiyomi".into(),
                        completed: processed,
                        total: total_manga,
                        title: m.title.clone(),
                    });
                    continue;
                }
            };

            // Exact match check via URL (used as source_manga_id)
            let existing_id: Option<i64> = sqlx::query_scalar!(
                "SELECT id FROM manga WHERE source_id = ? AND source_manga_id = ?",
                source_id,
                m.url
            )
            .fetch_optional(&self.db)
            .await?;

            let (manga_id, is_new) = if let Some(id) = existing_id {
                (id, false)
            } else {
                let authors = if m.author.is_empty() {
                    vec![]
                } else {
                    vec![m.author.clone()]
                };
                let hits =
                    crate::service::dedup::find_similar_manga(&self.db, &m.title, &authors, None)
                        .await?;

                if !hits.is_empty() {
                    let proxy = self.make_tachi_backup_manga(m);
                    let source_hint = kani_name.unwrap_or("Unknown").to_string();
                    self.save_pending_import_tachiyomi(
                        user_id,
                        &proxy,
                        &source_hint,
                        Some(hits[0].id),
                        Some(hits[0].similarity),
                    )
                    .await?;
                    result.possible_duplicates += 1;
                    result.pending_imports_added += 1;
                    let _ = self.refresh_tx.send(AppEvent::ImportProgress {
                        origin: "tachiyomi".into(),
                        completed: processed,
                        total: total_manga,
                        title: m.title.clone(),
                    });
                    continue;
                }

                let mut tx = self.db.begin().await?;

                let status = map_publication_status(m.status);
                let description = if m.description.is_empty() {
                    None
                } else {
                    Some(&m.description)
                };
                let cover_url = if m.thumbnail_url.is_empty() {
                    None
                } else {
                    Some(&m.thumbnail_url)
                };

                let id = sqlx::query_scalar!(
                    "INSERT INTO manga (source_id, source_manga_id, name, description, cover_url, status) \
                     VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
                    source_id,
                    m.url,
                    m.title,
                    description,
                    cover_url,
                    status
                )
                .fetch_one(&mut *tx)
                .await?;

                // manga_categories
                for &cat_idx in &m.categories {
                    if let Some(&cat_id) = tachi_cat_map.get(&cat_idx) {
                        sqlx::query!(
                            "INSERT OR IGNORE INTO manga_categories (manga_id, category_id) \
                             VALUES (?, ?)",
                            id,
                            cat_id
                        )
                        .execute(&mut *tx)
                        .await?;
                    }
                }

                // authors / artists → manga_people
                for (name, role) in [(&m.author, "author"), (&m.artist, "artist")] {
                    if name.is_empty() {
                        continue;
                    }
                    sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", name)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query!(
                        "INSERT OR IGNORE INTO manga_people (manga_id, role, person_id) \
                         SELECT ?, ?, id FROM people WHERE name = ?",
                        id,
                        role,
                        name
                    )
                    .execute(&mut *tx)
                    .await?;
                }

                // genre → tags + manga_tags
                for genre in &m.genre {
                    if genre.is_empty() {
                        continue;
                    }
                    sqlx::query!("INSERT OR IGNORE INTO tags (name) VALUES (?)", genre)
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query!(
                        "INSERT OR IGNORE INTO manga_tags (manga_id, tag_id) \
                         SELECT ?, id FROM tags WHERE name = ?",
                        id,
                        genre
                    )
                    .execute(&mut *tx)
                    .await?;
                }

                tx.commit().await?;
                (id, true)
            };

            // user_manga_tracking
            if opts.import_tracking
                && let Some(t) = m.tracking.first()
            {
                let kani_status = map_reading_status(t.status);
                let score: Option<f64> = if t.score > 0.0 {
                    Some(t.score as f64)
                } else {
                    None
                };
                sqlx::query!(
                    "INSERT OR REPLACE INTO user_manga_tracking \
                         (user_id, manga_id, status, score) VALUES (?, ?, ?, ?)",
                    user_id,
                    manga_id,
                    kani_status,
                    score
                )
                .execute(&self.db)
                .await?;
            }

            // tracker_manga_mappings — link AniList / MAL entries
            for t in &m.tracking {
                let Some(tracker_name) = tachiyomi_sync_id_to_tracker_name(t.sync_id) else {
                    continue;
                };

                // Check the new 'media_id' first, fallback to the deprecated 'media_id_int' if it's 0
                let remote_id_val = if t.media_id != 0 {
                    t.media_id
                } else {
                    t.media_id_int as i64
                };

                let remote_id = if remote_id_val != 0 {
                    remote_id_val.to_string()
                } else {
                    // Fall back to last path segment of tracking_url (renamed from track_url)
                    t.tracking_url
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                };

                if remote_id.is_empty() {
                    continue;
                }

                // Ensure the tracker row exists (it may not if the admin hasn't configured it)
                sqlx::query!(
                    "INSERT OR IGNORE INTO trackers (name) VALUES (?)",
                    tracker_name
                )
                .execute(&self.db)
                .await?;

                let tracker_id: Option<i64> =
                    sqlx::query_scalar!("SELECT id FROM trackers WHERE name = ?", tracker_name)
                        .fetch_optional(&self.db)
                        .await?
                        .flatten();

                if let Some(tid) = tracker_id {
                    sqlx::query!(
                        "INSERT OR IGNORE INTO tracker_manga_mappings \
                         (user_id, tracker_id, manga_id, tracker_manga_id) VALUES (?, ?, ?, ?)",
                        user_id,
                        tid,
                        manga_id,
                        remote_id
                    )
                    .execute(&self.db)
                    .await?;
                }
            }

            // reading direction: viewer_flags (field 103, Mihon) takes priority over viewer (field 14, legacy)
            // Lower 3 bits of viewer_flags: 1=LTR, 2=RTL. viewer field: 1=LTR, 2=RTL.
            let reading_dir = if m.viewer_flags != 0 {
                match m.viewer_flags & 0x07 {
                    1 => Some("ltr"),
                    2 => Some("rtl"),
                    _ => None,
                }
            } else {
                match m.viewer {
                    1 => Some("ltr"),
                    2 => Some("rtl"),
                    _ => None,
                }
            };
            if let Some(dir) = reading_dir {
                sqlx::query!(
                    "INSERT INTO user_manga_tracking (user_id, manga_id, reading_direction) \
                     VALUES (?, ?, ?) \
                     ON CONFLICT (user_id, manga_id) DO UPDATE SET reading_direction = excluded.reading_direction",
                    user_id, manga_id, dir
                )
                .execute(&self.db)
                .await?;
            }

            // user_chapter_tracking
            if opts.import_chapter_progress {
                // For newly inserted manga, fetch chapters from the source first so that
                // source_chapter_id values exist in the chapters table for progress matching.
                if is_new && let Err(e) = self.fetch_and_store_chapters_silent(manga_id).await {
                    result
                        .warnings
                        .push(format!("Could not fetch chapters for '{}': {}", m.title, e));
                }
                for ch in &m.chapters {
                    if !ch.read && ch.last_page_read == 0 {
                        continue;
                    }
                    let chapter_id: Option<i64> = sqlx::query_scalar!(
                        "SELECT id FROM chapters WHERE manga_id = ? AND source_chapter_id = ?",
                        manga_id,
                        ch.url
                    )
                    .fetch_optional(&self.db)
                    .await?;

                    if let Some(ch_id) = chapter_id {
                        sqlx::query!(
                            "INSERT OR REPLACE INTO user_chapter_tracking \
                             (user_id, chapter_id, is_read, last_page_read) VALUES (?, ?, ?, ?)",
                            user_id,
                            ch_id,
                            ch.read,
                            ch.last_page_read
                        )
                        .execute(&self.db)
                        .await?;
                    }
                }
            }

            new_manga_ids.push(manga_id);
            result.imported_manga += 1;
            let _ = self.refresh_tx.send(AppEvent::ImportProgress {
                origin: "tachiyomi".into(),
                completed: processed,
                total: total_manga,
                title: m.title.clone(),
            });
        }

        if total_manga > 0 {
            let _ = self.refresh_tx.send(AppEvent::ImportCompleted {
                origin: "tachiyomi".into(),
                imported: result.imported_manga,
                skipped: result.skipped_manga,
                pending: result.pending_imports_added,
            });
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
            "import.tachiyomi",
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

    fn make_tachi_backup_manga(&self, m: &BackupManga) -> KaniBackupManga {
        let chapter_progress: Vec<BackupChapterProgress> = m
            .chapters
            .iter()
            .filter(|c| c.read || c.last_page_read > 0)
            .map(|c| BackupChapterProgress {
                source_chapter_id: c.url.clone(),
                is_read: c.read,
                last_page_read: c.last_page_read as i64,
            })
            .collect();

        let tracking = m.tracking.first().map(|t| BackupMangaTracking {
            status: map_reading_status(t.status),
            score: if t.score > 0.0 {
                Some(t.score as f64)
            } else {
                None
            },
        });

        KaniBackupManga {
            source_name: String::new(),
            source_manga_id: m.url.clone(),
            name: m.title.clone(),
            status: Some(map_publication_status(m.status)),
            auto_download: false,
            auto_scan: false,
            scanlator_mode: String::new(),
            categories: vec![],
            tracking,
            download_rules: vec![],
            chapter_progress,
        }
    }

    async fn save_pending_import_tachiyomi(
        &self,
        user_id: i64,
        m: &KaniBackupManga,
        source_hint: &str,
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
             VALUES (?, 'tachiyomi', ?, ?, ?, ?, ?, ?, ?)",
            user_id,
            m.name,
            source_hint,
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
