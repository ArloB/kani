#![allow(clippy::unwrap_used)]

mod common;
use common::{insert_chapter, insert_manga, insert_source, test_service};
use kani_app::ids::UserId;
use kani_shared::types::ChapterSortOrder;

#[tokio::test]
async fn get_local_chapters_empty_returns_empty_list() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Manga").await;

    let (chapters, has_next, _, _) = svc
        .get_local_chapters(
            manga_id,
            1,
            20,
            ChapterSortOrder::ChapterDesc,
            UserId(1),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(chapters.is_empty());
    assert!(!has_next);
}

#[tokio::test]
async fn get_local_chapters_returns_inserted_chapters() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Manga").await;
    insert_chapter(&svc.db, manga_id, "ch1", 1.0).await;
    insert_chapter(&svc.db, manga_id, "ch2", 2.0).await;

    let (chapters, _, _, total) = svc
        .get_local_chapters(
            manga_id,
            1,
            20,
            ChapterSortOrder::ChapterDesc,
            UserId(1),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(chapters.len(), 2);
    assert_eq!(total, 2);
    assert!((chapters[0].number - 2.0).abs() < f64::EPSILON);
    assert!((chapters[1].number - 1.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn get_local_chapters_paging_works() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Manga").await;
    for i in 1..=5u32 {
        insert_chapter(&svc.db, manga_id, &format!("ch{i}"), f64::from(i)).await;
    }

    let (page1, has_next1, total1, _) = svc
        .get_local_chapters(
            manga_id,
            1,
            3,
            ChapterSortOrder::ChapterAsc,
            UserId(1),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(page1.len(), 3);
    assert!(has_next1);
    assert_eq!(total1, Some(2)); // ceil(5/3) = 2

    let (page2, has_next2, _, _) = svc
        .get_local_chapters(
            manga_id,
            2,
            3,
            ChapterSortOrder::ChapterAsc,
            UserId(1),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(page2.len(), 2);
    assert!(!has_next2);
}

#[tokio::test]
async fn download_status_filter_works() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Manga").await;
    let ch_id = insert_chapter(&svc.db, manga_id, "ch1", 1.0).await;

    // Set download_status = 2 (downloaded)
    sqlx::query("UPDATE chapters SET download_status = 2 WHERE id = ?")
        .bind(ch_id)
        .execute(&svc.db)
        .await
        .unwrap();

    let (downloaded, _, _, _) = svc
        .get_local_chapters(
            manga_id,
            1,
            20,
            ChapterSortOrder::ChapterDesc,
            UserId(1),
            Some(true),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(downloaded.len(), 1);

    let (not_downloaded, _, _, _) = svc
        .get_local_chapters(
            manga_id,
            1,
            20,
            ChapterSortOrder::ChapterDesc,
            UserId(1),
            Some(false),
            None,
            None,
        )
        .await
        .unwrap();
    assert!(not_downloaded.is_empty());
}

#[tokio::test]
async fn orphaned_chapters_are_excluded() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "src").await;
    let manga_id = insert_manga(&svc.db, src, "m1", "Manga").await;
    let ch_id = insert_chapter(&svc.db, manga_id, "ch1", 1.0).await;

    sqlx::query("UPDATE chapters SET is_orphaned = TRUE WHERE id = ?")
        .bind(ch_id)
        .execute(&svc.db)
        .await
        .unwrap();

    let (chapters, _, _, _) = svc
        .get_local_chapters(
            manga_id,
            1,
            20,
            ChapterSortOrder::ChapterDesc,
            UserId(1),
            None,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(chapters.is_empty(), "orphaned chapters should not appear");
}
