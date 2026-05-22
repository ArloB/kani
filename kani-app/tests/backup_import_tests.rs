#![allow(clippy::unwrap_used)]
// Tests for Kani native backup (ZIP + JSON) export / restore round-trips.
// Tachiyomi import (.tachibk gzip+protobuf) is exercised separately once
// a proto fixture helper is available.

mod common;
use common::{insert_manga, insert_source, insert_user, test_service};
use kani_app::RestoreOptions;

#[tokio::test]
async fn preview_backup_shows_correct_manga_count() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    insert_manga(&svc.db, src, "m1", "Dragon Ball").await;
    insert_manga(&svc.db, src, "m2", "Naruto").await;

    let zip = svc.export_backup(1, false).await.unwrap();
    let preview = svc.preview_backup(&zip).await.unwrap();

    assert_eq!(preview.manga_count, 2);
    assert_eq!(preview.version, 1);
    assert_eq!(preview.category_count, 0);
    assert!(!preview.has_tracking);
    assert!(!preview.has_chapter_progress);
}

#[tokio::test]
async fn restore_backup_reimports_manga_after_wipe() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    insert_manga(&svc.db, src, "m1", "Dragon Ball").await;

    let zip = svc.export_backup(1, false).await.unwrap();

    // merge=false → deletes all manga then re-imports from the ZIP.
    let result = svc
        .restore_backup(1, &zip, RestoreOptions::default())
        .await
        .unwrap();

    assert_eq!(result.imported_manga, 1);
    assert_eq!(result.skipped_manga, 0);
    assert_eq!(result.pending_imports_added, 0);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn restore_backup_unknown_source_adds_pending_import() {
    // Export from svc1 (has source "src"), restore to svc2 (fresh, no sources).
    let svc1 = test_service().await;
    let src = insert_source(&svc1.db, "src").await;
    insert_manga(&svc1.db, src, "m1", "Dragon Ball").await;
    let zip = svc1.export_backup(1, false).await.unwrap();

    let svc2 = test_service().await;
    let user2 = insert_user(&svc2.db, "user").await;
    let result = svc2
        .restore_backup(user2, &zip, RestoreOptions::default())
        .await
        .unwrap();

    assert_eq!(
        result.imported_manga, 0,
        "source unknown → should not import"
    );
    assert_eq!(result.pending_imports_added, 1);
    assert_eq!(result.skipped_manga, 1);
    assert!(!result.warnings.is_empty());

    let manga_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga")
        .fetch_one(&svc2.db)
        .await
        .unwrap();
    assert_eq!(manga_count, 0);

    let pending_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_imports")
        .fetch_one(&svc2.db)
        .await
        .unwrap();
    assert_eq!(pending_count, 1);
}

#[tokio::test]
async fn restore_backup_preserves_category_assignment() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Dragon Ball").await;

    let cat_id = svc.create_category("Action", 0).await.unwrap();
    svc.set_manga_categories(manga_id, vec![cat_id])
        .await
        .unwrap();

    let zip = svc.export_backup(1, false).await.unwrap();

    let result = svc
        .restore_backup(1, &zip, RestoreOptions::default())
        .await
        .unwrap();
    assert_eq!(result.imported_manga, 1);
    assert_eq!(result.imported_categories, 1);

    // Find the restored manga's id (may differ from the original).
    let restored_manga_id: i64 =
        sqlx::query_scalar("SELECT id FROM manga WHERE source_manga_id = 'm1'")
            .fetch_one(&svc.db)
            .await
            .unwrap();

    let cats = svc.get_manga_categories(restored_manga_id).await.unwrap();
    assert_eq!(
        cats.len(),
        1,
        "category assignment should survive export+restore"
    );
}

#[tokio::test]
async fn restore_backup_merge_mode_keeps_existing_manga() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    insert_manga(&svc.db, src, "m1", "Dragon Ball").await;

    let zip = svc.export_backup(1, false).await.unwrap();

    // Add a second manga AFTER export (not in the ZIP).
    insert_manga(&svc.db, src, "m2", "Naruto").await;

    // Restore with merge=true → should not delete existing manga.
    let opts = RestoreOptions {
        merge: true,
        import_manga: true,
        import_categories: true,
        import_download_rules: true,
        import_tracking: true,
        import_chapter_progress: false,
        import_settings: false,
    };
    svc.restore_backup(1, &zip, opts).await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "both original and post-export manga should remain"
    );
}
