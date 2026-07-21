#![allow(clippy::unwrap_used)]
// Content addressing: manifest capture, and the rename-safety property that
// stored file_path exists to provide.

mod common;
use common::{insert_chapter, insert_manga, insert_source, test_service};
use kani_app::ids::ChapterId;
use std::io::Write;
use std::path::Path;

fn png_bytes(shade: u8) -> Vec<u8> {
    let mut img = image::GrayImage::new(24, 32);
    for (x, _y, p) in img.enumerate_pixels_mut() {
        *p = image::Luma([shade.wrapping_add((x % 255) as u8)]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn write_cbz(path: &Path, shades: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (i, shade) in shades.iter().enumerate() {
        zip.start_file(format!("{:04}.png", i + 1), opts).unwrap();
        zip.write_all(&png_bytes(*shade)).unwrap();
    }
    zip.finish().unwrap();
}

/// Seeds a downloaded chapter whose CBZ sits at the title-derived path, which
/// is where the pre-backfill derivation expects to find it.
async fn seed_downloaded_chapter(
    svc: &kani_app::service::AppService,
    manga_name: &str,
) -> (ChapterId, std::path::PathBuf) {
    let src = insert_source(&svc.db, "src").await;
    let manga = insert_manga(&svc.db, src, "m1", manga_name).await;
    let chapter = insert_chapter(&svc.db, manga, "c1", 1.0).await;

    sqlx::query("UPDATE chapters SET download_status = 2 WHERE id = ?")
        .bind(chapter)
        .execute(&svc.db)
        .await
        .unwrap();

    // The manga directory has to exist before the path guard will resolve, but
    // the filename comes from the service so this test cannot drift from the
    // real naming scheme.
    let library = { svc.settings.read().await.library_path.clone() };
    std::fs::create_dir_all(library.join(format!(
        "{} - {}",
        kani_core::utilities::sanitize_filename(manga_name),
        manga.0
    )))
    .unwrap();

    let cbz = svc.chapter_cbz_path(chapter).await.unwrap().path;
    write_cbz(&cbz, &[10, 90]);
    (chapter, cbz)
}

#[tokio::test]
async fn recording_a_manifest_populates_every_content_column() {
    let svc = test_service().await;
    let (chapter, _) = seed_downloaded_chapter(&svc, "Test Manga").await;

    let path = svc.chapter_cbz_path(chapter).await.unwrap().path;
    svc.record_chapter_manifest(chapter, path).await;

    let row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<i64>)>(
        "SELECT file_path, content_hash, manifest_json, file_verified_at, quality_long_edge, page_count \
         FROM chapters WHERE id = ?",
    )
    .bind(chapter)
    .fetch_one(&svc.db)
    .await
    .unwrap();

    assert!(row.0.is_some(), "file_path should be stored");
    assert_eq!(
        row.1.as_ref().map(|h| h.len()),
        Some(64),
        "content_hash is blake3 hex"
    );
    assert!(row.2.is_some(), "manifest_json should be stored");
    assert!(row.3.is_some(), "file_verified_at should be set");
    assert_eq!(row.4, Some(32), "long edge of a 24x32 page");
    assert_eq!(row.5, Some(2), "page_count measured from the archive");

    let manifest: kani_core::manifest::ChapterManifest =
        serde_json::from_str(&row.2.unwrap()).unwrap();
    assert_eq!(manifest.page_count, 2);
    assert_eq!(manifest.pages.len(), 2);
}

#[tokio::test]
async fn stored_path_is_relative_to_the_library_root() {
    let svc = test_service().await;
    let (chapter, _) = seed_downloaded_chapter(&svc, "Relative Check").await;

    let path = svc.chapter_cbz_path(chapter).await.unwrap().path;
    svc.record_chapter_manifest(chapter, path).await;

    let stored: Option<String> = sqlx::query_scalar("SELECT file_path FROM chapters WHERE id = ?")
        .bind(chapter)
        .fetch_one(&svc.db)
        .await
        .unwrap();
    let stored = stored.unwrap();

    assert!(
        !stored.starts_with('/') && !stored.contains(':'),
        "path must be library-relative so relocation cannot invalidate it, got {stored}"
    );
    assert!(stored.ends_with(".cbz"), "got {stored}");
}

#[tokio::test]
async fn renaming_a_manga_does_not_orphan_its_chapter() {
    let svc = test_service().await;
    let (chapter, cbz) = seed_downloaded_chapter(&svc, "Original Title").await;

    let path = svc.chapter_cbz_path(chapter).await.unwrap().path;
    svc.record_chapter_manifest(chapter, path).await;

    // Rename the manga. The file stays where it is; without a stored path the
    // read-time derivation would now point at a directory that does not exist.
    sqlx::query("UPDATE manga SET name = 'Completely Different Title' WHERE id = (SELECT manga_id FROM chapters WHERE id = ?)")
        .bind(chapter)
        .execute(&svc.db)
        .await
        .unwrap();

    let resolved = svc.chapter_cbz_path(chapter).await.unwrap().path;
    assert_eq!(resolved, cbz, "must still resolve to the real file");
    assert!(resolved.exists(), "resolved path should exist on disk");
}

#[tokio::test]
async fn a_chapter_without_a_stored_path_still_resolves_by_derivation() {
    let svc = test_service().await;
    let (chapter, cbz) = seed_downloaded_chapter(&svc, "Pre Backfill").await;

    // No record_chapter_manifest call: this is a pre-migration row.
    let resolved = svc.chapter_cbz_path(chapter).await.unwrap().path;
    assert_eq!(
        resolved, cbz,
        "the derivation fallback must keep working for un-backfilled rows"
    );
}

#[tokio::test]
async fn clearing_a_manifest_removes_the_content_columns() {
    let svc = test_service().await;
    let (chapter, _) = seed_downloaded_chapter(&svc, "Clear Me").await;
    let path = svc.chapter_cbz_path(chapter).await.unwrap().path;
    svc.record_chapter_manifest(chapter, path).await;

    svc.clear_chapter_manifest(chapter).await.unwrap();

    let row = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
        "SELECT file_path, content_hash, manifest_json FROM chapters WHERE id = ?",
    )
    .bind(chapter)
    .fetch_one(&svc.db)
    .await
    .unwrap();
    assert_eq!(
        (row.0, row.1, row.2),
        (None, None, None),
        "a deleted file must not leave a stale hash behind"
    );
}

#[tokio::test]
async fn identical_chapters_share_a_content_hash() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;

    let mut hashes = Vec::new();
    for (i, title) in ["Series A", "Series B"].iter().enumerate() {
        let manga = insert_manga(&svc.db, src, &format!("m{i}"), title).await;
        let chapter = insert_chapter(&svc.db, manga, "c1", 1.0).await;
        sqlx::query("UPDATE chapters SET download_status = 2 WHERE id = ?")
            .bind(chapter)
            .execute(&svc.db)
            .await
            .unwrap();

        let library = { svc.settings.read().await.library_path.clone() };
        std::fs::create_dir_all(library.join(format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(title),
            manga.0
        )))
        .unwrap();

        let path = svc.chapter_cbz_path(chapter).await.unwrap().path;
        write_cbz(&path, &[10, 90]);
        svc.record_chapter_manifest(chapter, path).await;

        let h: Option<String> =
            sqlx::query_scalar("SELECT content_hash FROM chapters WHERE id = ?")
                .bind(chapter)
                .fetch_one(&svc.db)
                .await
                .unwrap();
        hashes.push(h.unwrap());
    }

    assert_eq!(
        hashes[0], hashes[1],
        "byte-identical chapters under different manga must hash the same — \
         this is what exact-duplicate detection keys on"
    );
}

/// Source migration is the one flow that physically moves files, so a stored
/// path must follow the rename. Without the repoint this resolves to the old
/// directory, which no longer exists — the regression that preferring
/// file_path over title derivation would otherwise introduce.
#[tokio::test]
async fn migrating_a_manga_repoints_stored_chapter_paths() {
    let svc = test_service().await;
    let (chapter, cbz) = seed_downloaded_chapter(&svc, "Old Source Title").await;

    let path = svc.chapter_cbz_path(chapter).await.unwrap().path;
    svc.record_chapter_manifest(chapter, path).await;

    let manga_id: i64 = sqlx::query_scalar("SELECT manga_id FROM chapters WHERE id = ?")
        .bind(chapter)
        .fetch_one(&svc.db)
        .await
        .unwrap();
    let library = { svc.settings.read().await.library_path.clone() };
    let old_dir = library.join(format!("Old Source Title - {manga_id}"));
    let new_dir = library.join(format!("New Source Title - {manga_id}"));

    // Simulate what migrate_manga does: rename the row and move the directory.
    std::fs::rename(&old_dir, &new_dir).unwrap();
    sqlx::query("UPDATE manga SET name = 'New Source Title' WHERE id = ?")
        .bind(manga_id)
        .execute(&svc.db)
        .await
        .unwrap();

    let old_prefix = format!("Old Source Title - {manga_id}/");
    let new_prefix = format!("New Source Title - {manga_id}/");
    sqlx::query(
        "UPDATE chapters SET file_path = ? || substr(file_path, length(?) + 1) \
         WHERE manga_id = ? AND substr(file_path, 1, length(?)) = ?",
    )
    .bind(&new_prefix)
    .bind(&old_prefix)
    .bind(manga_id)
    .bind(&old_prefix)
    .bind(&old_prefix)
    .execute(&svc.db)
    .await
    .unwrap();

    let resolved = svc.chapter_cbz_path(chapter).await.unwrap().path;
    assert_ne!(
        resolved, cbz,
        "the file moved, so the path must have changed"
    );
    assert!(
        resolved.starts_with(&new_dir),
        "stored path should follow the directory rename, got {resolved:?}"
    );
    assert!(resolved.exists(), "repointed path must exist on disk");
}
