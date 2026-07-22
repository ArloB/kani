#![allow(clippy::unwrap_used)]

mod common;
use common::{insert_manga, insert_source, test_service};
use kani_app::ids::{MangaId, UserId};
use kani_app::service::library::LibraryFilter;
use kani_shared::types::MangaSortOrder;

#[tokio::test]
async fn get_manga_by_id_returns_not_found_for_missing_id() {
    let svc = test_service().await;
    let result = svc.get_manga_by_id(MangaId(99999)).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_manga_by_id_returns_manga_after_insert() {
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, source_id, "m1", "Test Manga").await;

    let manga = svc.get_manga_by_id(manga_id).await.unwrap();
    assert_eq!(manga.name, "Test Manga");
    assert_eq!(manga.id, manga_id);
    assert_eq!(manga.source_id, source_id);
}

#[tokio::test]
async fn get_library_returns_empty_list_on_fresh_db() {
    let svc = test_service().await;
    let list = svc.get_library(1, 0).await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn get_library_returns_inserted_manga() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    insert_manga(&svc.db, src, "m1", "Alpha").await;
    insert_manga(&svc.db, src, "m2", "Beta").await;

    let list = svc.get_library(1, 0).await.unwrap();
    assert_eq!(list.len(), 2);
}

#[tokio::test]
async fn get_library_filtered_empty_db_returns_empty() {
    let svc = test_service().await;
    let (rows, has_next, _total) = svc
        .get_library_filtered(
            UserId(1),
            &LibraryFilter {
                page: 1,
                page_size: 20,
                sort_by: MangaSortOrder::UpdatedAsc,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(rows.is_empty());
    assert!(!has_next);
}

#[tokio::test]
async fn get_library_filtered_returns_matching_manga() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    insert_manga(&svc.db, src, "m1", "Dragon Quest").await;
    insert_manga(&svc.db, src, "m2", "Naruto").await;

    let (rows, _, _) = svc
        .get_library_filtered(
            UserId(1),
            &LibraryFilter {
                page: 1,
                page_size: 20,
                search: Some("Dragon".to_string()),
                sort_by: MangaSortOrder::UpdatedAsc,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].name, "Dragon Quest");
}

#[tokio::test]
async fn delete_manga_removes_row() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "To Delete").await;

    svc.delete_manga(manga_id, UserId(1)).await.unwrap();

    let result = svc.get_manga_by_id(manga_id).await;
    assert!(result.is_err(), "manga should be gone after delete");
}

#[tokio::test]
async fn delete_manga_returns_not_found_for_missing_id() {
    let svc = test_service().await;
    let result = svc.delete_manga(MangaId(99999), UserId(1)).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn a_cover_whose_directory_is_missing_is_not_found_not_a_server_error() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Gone Cover").await;

    // A row pointing at a cover under a directory that does not exist — the
    // shape left behind by a library-path change or a partial restore.
    sqlx::query("UPDATE manga SET local_cover_path = 'no-such-dir/cover.jpg' WHERE id = ?")
        .bind(manga_id)
        .execute(&svc.db)
        .await
        .unwrap();

    let err = svc.get_manga_cover_path(manga_id).await.unwrap_err();
    assert!(
        matches!(err, kani_app::ServiceError::NotFound(_)),
        "a missing cover directory must be NotFound (404), not Internal (500) — \
         got {err:?}"
    );
}
