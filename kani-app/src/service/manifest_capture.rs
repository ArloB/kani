use std::path::{Path, PathBuf};

use kani_core::manifest::ChapterManifest;

use crate::ids::ChapterId;
use crate::service::AppService;

pub struct CapturedManifest {
    pub manifest: ChapterManifest,
    pub rel_path: Option<String>,
    pub quality_long_edge: i64,
    pub quality_bytes_per_mp: f64,
}

/// Computes a chapter's manifest off the runtime. Hashing and decoding every
/// page is CPU-bound and a large chapter would otherwise stall the executor.
pub async fn capture(cbz_path: PathBuf, library_path: &Path) -> Option<CapturedManifest> {
    let for_blocking = cbz_path.clone();
    let manifest =
        tokio::task::spawn_blocking(move || kani_core::manifest::manifest_for_cbz(&for_blocking))
            .await
            .ok()?
            .inspect_err(|e| {
                tracing::warn!("manifest capture failed for {}: {e}", cbz_path.display())
            })
            .ok()?;

    let score = kani_core::quality::score_from_manifest(&manifest);
    let rel_path = cbz_path
        .strip_prefix(library_path)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"));

    Some(CapturedManifest {
        manifest,
        rel_path,
        quality_long_edge: i64::from(score.median_long_edge_px),
        quality_bytes_per_mp: f64::from(score.bytes_per_megapixel),
    })
}

impl AppService {
    /// Records content-addressing columns for a freshly downloaded chapter.
    ///
    /// Never fails the caller: a completed download that cannot be hashed stays
    /// completed with NULL columns, and the backfill or the next scrub picks it
    /// up. Losing a manifest is recoverable; losing the download is not.
    pub async fn record_chapter_manifest(&self, chapter_id: ChapterId, cbz_path: PathBuf) {
        let library_path = { self.settings.read().await.library_path.clone() };
        let Some(captured) = capture(cbz_path, &library_path).await else {
            return;
        };

        let manifest_json = match serde_json::to_string(&captured.manifest) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!("manifest serialisation failed for chapter {chapter_id}: {e}");
                return;
            }
        };
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let page_count = i64::from(captured.manifest.page_count);
        let archive_hash = captured.manifest.archive_hash.clone();

        if let Err(e) = sqlx::query!(
            "UPDATE chapters SET file_path = ?, content_hash = ?, manifest_json = ?, \
             file_verified_at = ?, quality_long_edge = ?, quality_bytes_per_mp = ?, \
             page_count = ? WHERE id = ?",
            captured.rel_path,
            archive_hash,
            manifest_json,
            now,
            captured.quality_long_edge,
            captured.quality_bytes_per_mp,
            page_count,
            chapter_id,
        )
        .execute(&self.db)
        .await
        {
            tracing::warn!("failed to persist manifest for chapter {chapter_id}: {e}");
        }
    }

    /// Submits a one-off backfill if any downloaded chapter still lacks a stored
    /// path. Guarded so a restart loop cannot queue duplicates.
    pub async fn submit_manifest_backfill_if_needed(&self) {
        let pending: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM chapters WHERE download_status = 2 AND file_path IS NULL"
        )
        .fetch_one(&self.db_read)
        .await
        .unwrap_or(0);
        if pending == 0 {
            return;
        }

        let active: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM jobs WHERE job_type = 'manifest_backfill' \
             AND status IN ('pending', 'running')"
        )
        .fetch_one(&self.db_read)
        .await
        .unwrap_or(0);
        if active > 0 {
            return;
        }

        if let Err(e) = self
            .job_manager
            .submit(crate::jobs::manifest_backfill::ManifestBackfillJob::new())
            .await
        {
            tracing::warn!("failed to submit manifest backfill: {e}");
        }
    }

    /// Clears content addressing when a chapter's file is removed, so a stale
    /// hash can never make a deleted chapter look present to the scrub.
    pub async fn clear_chapter_manifest(&self, chapter_id: ChapterId) -> crate::error::Result<()> {
        sqlx::query!(
            "UPDATE chapters SET file_path = NULL, content_hash = NULL, manifest_json = NULL, \
             file_verified_at = NULL WHERE id = ?",
            chapter_id
        )
        .execute(&self.db)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn relative_path_is_derived_against_the_library_root() {
        let lib = PathBuf::from("/library");
        let cbz = PathBuf::from("/library/Some Manga - 1/0001.cbz");
        let rel = cbz
            .strip_prefix(&lib)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"));
        assert_eq!(rel.as_deref(), Some("Some Manga - 1/0001.cbz"));
    }

    #[test]
    fn a_path_outside_the_library_yields_no_relative_path() {
        let lib = PathBuf::from("/library");
        let cbz = PathBuf::from("/elsewhere/0001.cbz");
        assert!(cbz.strip_prefix(&lib).is_err());
    }
}
