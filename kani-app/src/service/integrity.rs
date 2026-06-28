use crate::error::Result;
use crate::service::{AppService, chapter_name};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Serialize)]
pub struct IntegrityReport {
    pub orphaned_files: Vec<String>,
    pub missing_files: Vec<String>,
    pub cover_mismatches: Vec<String>,
    pub db_chapter_count: u64,
    pub disk_file_count: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct CleanupResult {
    pub removed_count: u64,
    pub failed_count: u64,
    pub dry_run: bool,
}

impl AppService {
    pub async fn check_library(&self) -> Result<IntegrityReport> {
        let library_path = self.settings.read().await.library_path.clone();

        let disk_files = collect_cbz_files(&library_path).await;
        let disk_file_count = disk_files.len() as u64;
        let disk_set: HashSet<PathBuf> = disk_files.iter().cloned().collect();

        let rows = sqlx::query!(
            "SELECT c.id, c.manga_id, m.name AS manga_name, c.name AS chapter_name, \
             c.chapter_number, c.volume \
             FROM chapters c \
             JOIN manga m ON m.id = c.manga_id \
             WHERE c.download_status = 2 AND m.deleted_at IS NULL"
        )
        .fetch_all(&self.db_read)
        .await?;

        let db_chapter_count = rows.len() as u64;
        let mut expected_set: HashSet<PathBuf> = HashSet::new();
        let mut missing_files: Vec<String> = Vec::new();

        for row in rows {
            let safe_manga = format!(
                "{} - {}",
                kani_core::utilities::sanitize_filename(&row.manga_name),
                row.manga_id
            );
            let title = chapter_name(row.volume, row.chapter_number, row.chapter_name);
            let path = library_path.join(&safe_manga).join(format!(
                "{}.cbz",
                kani_core::utilities::sanitize_filename(&title)
            ));
            if !disk_set.contains(&path) {
                missing_files.push(path.to_string_lossy().into_owned());
            }
            expected_set.insert(path);
        }

        let orphaned_files: Vec<String> = disk_files
            .into_iter()
            .filter(|p| !expected_set.contains(p))
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        let cover_rows = sqlx::query!(
            "SELECT local_cover_path FROM manga WHERE local_cover_path IS NOT NULL AND deleted_at IS NULL"
        )
        .fetch_all(&self.db_read)
        .await?;

        let mut cover_mismatches: Vec<String> = Vec::new();
        for row in cover_rows {
            if let Some(rel_path) = row.local_cover_path {
                let full_path = library_path.join(&rel_path);
                if tokio::fs::metadata(&full_path).await.is_err() {
                    cover_mismatches.push(full_path.to_string_lossy().into_owned());
                }
            }
        }

        Ok(IntegrityReport {
            orphaned_files,
            missing_files,
            cover_mismatches,
            db_chapter_count,
            disk_file_count,
        })
    }

    pub async fn cleanup_orphans(&self, dry_run: bool) -> Result<CleanupResult> {
        let report = self.check_library().await?;
        let mut removed_count = 0u64;
        let mut failed_count = 0u64;

        if dry_run {
            removed_count = report.orphaned_files.len() as u64;
        } else {
            for path_str in &report.orphaned_files {
                match tokio::fs::remove_file(path_str).await {
                    Ok(_) => removed_count += 1,
                    Err(e) => {
                        tracing::warn!("cleanup_orphans: failed to remove {path_str}: {e}");
                        failed_count += 1;
                    }
                }
            }
        }

        Ok(CleanupResult {
            removed_count,
            failed_count,
            dry_run,
        })
    }
}

async fn collect_cbz_files(library_path: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let Ok(mut manga_dirs) = tokio::fs::read_dir(library_path).await else {
        return result;
    };
    while let Ok(Some(entry)) = manga_dirs.next_entry().await {
        let Ok(ft) = entry.file_type().await else {
            continue;
        };
        if !ft.is_dir() {
            continue;
        }
        if entry.file_name() == "covers" {
            continue;
        }
        let manga_dir = entry.path();
        let Ok(mut chapter_files) = tokio::fs::read_dir(&manga_dir).await else {
            continue;
        };
        while let Ok(Some(f)) = chapter_files.next_entry().await {
            let name = f.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".cbz") {
                result.push(f.path());
            }
        }
    }
    result
}
