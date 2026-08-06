#![allow(clippy::unwrap_used)]

mod common;

use kani_app::ids::UserId;
use kani_app::service::AppService;
use kani_app::service::import::tachiyomi::{Backup, TachiyomiImportOptions};
use prost::Message as _;
use sqlx::SqlitePool;
use std::io::Read as _;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mihon")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn decode(data: &[u8]) -> Backup {
    let mut gz = flate2::read::GzDecoder::new(data);
    let mut buf = Vec::new();
    gz.read_to_end(&mut buf).unwrap();
    Backup::decode(buf.as_slice()).unwrap()
}

async fn register_source(pool: &SqlitePool, name: &str, mihon_id: i64) -> i64 {
    let id = common::insert_source(pool, name).await;
    sqlx::query("UPDATE sources SET mihon_source_id = ? WHERE id = ?")
        .bind(mihon_id)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn register_every_source(pool: &SqlitePool, backup: &Backup) {
    let mut seen = std::collections::BTreeSet::new();
    for m in &backup.backup_manga {
        if seen.insert(m.source) {
            register_source(pool, &format!("Source {}", m.source), m.source).await;
        }
    }
}

fn options(chapter_progress: bool) -> TachiyomiImportOptions {
    TachiyomiImportOptions {
        import_manga: true,
        import_categories: true,
        import_tracking: true,
        import_chapter_progress: chapter_progress,
    }
}

async fn user(svc: &AppService) -> UserId {
    common::insert_user(&svc.db, "reader").await
}

#[tokio::test]
async fn a_real_mihon_backup_imports_its_series() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    assert!(
        backup.backup_manga.len() >= 4,
        "the fixture must carry several series"
    );

    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    let result = svc
        .import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();

    assert_eq!(result.imported_manga as usize, backup.backup_manga.len());
    assert_eq!(result.skipped_manga, 0);
    assert_eq!(result.pending_imports_added, 0);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);

    for m in &backup.backup_manga {
        let row = sqlx::query_as::<_, (String, Option<String>, Option<String>, i64)>(
            "SELECT name, description, cover_url, status FROM manga WHERE source_manga_id = ?",
        )
        .bind(&m.url)
        .fetch_one(&svc.db)
        .await
        .unwrap_or_else(|e| panic!("series '{}' was not stored: {e}", m.title));
        assert_eq!(row.0, m.title);
        assert_eq!(row.1.as_deref().unwrap_or_default(), m.description);
        assert_eq!(row.2.as_deref().unwrap_or_default(), m.thumbnail_url);
        let expected_status = match m.status {
            1 => 1,
            2 => 2,
            _ => 0,
        };
        assert_eq!(row.3, expected_status, "status drifted for '{}'", m.title);
    }

    let first = &backup.backup_manga[0];
    let genres: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM tags t \
         JOIN manga_tags mt ON mt.tag_id = t.id \
         JOIN manga m ON m.id = mt.manga_id \
         WHERE m.source_manga_id = ? ORDER BY t.name",
    )
    .bind(&first.url)
    .fetch_all(&svc.db)
    .await
    .unwrap();
    let mut expected = first.genre.clone();
    expected.sort();
    assert_eq!(genres, expected, "genres did not survive");

    let authors: Vec<String> = sqlx::query_scalar(
        "SELECT p.name FROM people p \
         JOIN manga_people mp ON mp.person_id = p.id \
         JOIN manga m ON m.id = mp.manga_id \
         WHERE m.source_manga_id = ? AND mp.role = 'author'",
    )
    .bind(&first.url)
    .fetch_all(&svc.db)
    .await
    .unwrap();
    assert_eq!(authors, vec![first.author.clone()]);
}

#[tokio::test]
async fn the_preview_describes_the_backup_without_importing_it() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    register_every_source(&svc.db, &backup).await;

    let preview = svc.preview_tachiyomi_backup(&data).await.unwrap();

    assert_eq!(preview.total_manga as usize, backup.backup_manga.len());
    assert_eq!(
        preview.category_count as usize,
        backup.backup_categories.len()
    );
    assert!(
        preview.has_chapter_progress,
        "the fixture has read chapters"
    );
    assert!(preview.has_tracking, "the fixture has a tracking entry");
    assert_eq!(preview.pending_import_estimate, 0);
    assert_eq!(
        preview.sources.iter().map(|s| s.manga_count).sum::<u32>() as usize,
        backup.backup_manga.len()
    );
    assert!(preview.sources.iter().all(|s| s.found));

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(stored, 0, "a preview must not write to the library");
}

#[tokio::test]
async fn categories_survive_the_import() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    svc.import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();

    for cat in &backup.backup_categories {
        let found: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories WHERE name = ?")
            .bind(&cat.name)
            .fetch_one(&svc.db)
            .await
            .unwrap();
        assert_eq!(found, 1, "category '{}' was not created", cat.name);
    }

    let assigned = backup
        .backup_manga
        .iter()
        .find(|m| !m.categories.is_empty())
        .expect("the fixture must have a series in a category");
    let expected_name = &backup.backup_categories[assigned.categories[0] as usize].name;

    let names: Vec<String> = sqlx::query_scalar(
        "SELECT c.name FROM categories c \
         JOIN manga_categories mc ON mc.category_id = c.id \
         JOIN manga m ON m.id = mc.manga_id \
         WHERE m.source_manga_id = ?",
    )
    .bind(&assigned.url)
    .fetch_all(&svc.db)
    .await
    .unwrap();
    assert_eq!(names, vec![expected_name.clone()], "membership was lost");
}

#[tokio::test]
async fn tracker_links_survive_the_import() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    svc.import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();

    let tracked = backup
        .backup_manga
        .iter()
        .find(|m| !m.tracking.is_empty())
        .expect("the fixture must have a tracked series");
    let entry = &tracked.tracking[0];

    let (tracker_name, remote_id): (String, String) = sqlx::query_as(
        "SELECT t.name, tmm.tracker_manga_id FROM tracker_manga_mappings tmm \
         JOIN trackers t ON t.id = tmm.tracker_id \
         JOIN manga m ON m.id = tmm.manga_id \
         WHERE m.source_manga_id = ?",
    )
    .bind(&tracked.url)
    .fetch_one(&svc.db)
    .await
    .unwrap();
    assert_eq!(tracker_name, "AniList", "syncId 2 is AniList");
    assert_eq!(remote_id, entry.media_id.to_string());

    let status: i64 = sqlx::query_scalar(
        "SELECT umt.status FROM user_manga_tracking umt \
         JOIN manga m ON m.id = umt.manga_id \
         WHERE m.source_manga_id = ? AND umt.user_id = ?",
    )
    .bind(&tracked.url)
    .bind(uid)
    .fetch_one(&svc.db)
    .await
    .unwrap();
    assert_eq!(status, 0, "Tachiyomi status 1 (Reading) maps to Kani 0");
}

#[tokio::test]
async fn read_progress_and_chapter_state_survive() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    let series = backup
        .backup_manga
        .iter()
        .find(|m| m.chapters.iter().any(|c| c.read || c.last_page_read > 0))
        .expect("the fixture must carry read state");

    let source_id: i64 = sqlx::query_scalar("SELECT id FROM sources WHERE mihon_source_id = ?")
        .bind(series.source)
        .fetch_one(&svc.db)
        .await
        .unwrap();
    let manga_id = common::insert_manga(&svc.db, source_id, &series.url, &series.title).await;
    for (idx, ch) in series.chapters.iter().enumerate() {
        common::insert_chapter(&svc.db, manga_id, &ch.url, idx as f64 + 1.0).await;
    }

    svc.import_tachiyomi_backup(uid, &data, options(true))
        .await
        .unwrap();

    let mut checked = 0;
    for ch in &series.chapters {
        let row: Option<(bool, i64)> = sqlx::query_as(
            "SELECT uct.is_read, uct.last_page_read FROM user_chapter_tracking uct \
             JOIN chapters c ON c.id = uct.chapter_id \
             WHERE c.manga_id = ? AND c.source_chapter_id = ? AND uct.user_id = ?",
        )
        .bind(manga_id)
        .bind(&ch.url)
        .bind(uid)
        .fetch_optional(&svc.db)
        .await
        .unwrap();

        if ch.read || ch.last_page_read > 0 {
            let (is_read, last_page) =
                row.unwrap_or_else(|| panic!("no progress row for '{}'", ch.name));
            assert_eq!(is_read, ch.read, "read flag drifted for '{}'", ch.name);
            assert_eq!(
                last_page,
                i64::from(ch.last_page_read),
                "last page drifted for '{}'",
                ch.name
            );
            checked += 1;
        } else {
            assert!(
                row.is_none(),
                "an unread chapter must not gain a progress row: '{}'",
                ch.name
            );
        }
    }
    assert!(checked > 0, "the assertions above never ran");
}

#[tokio::test]
async fn progress_that_cannot_be_applied_yet_is_reported() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    let result = svc
        .import_tachiyomi_backup(uid, &data, options(true))
        .await
        .unwrap();

    assert_eq!(result.imported_manga as usize, backup.backup_manga.len());
    let progress_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_chapter_tracking")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(
        progress_rows, 0,
        "there are no chapter rows to attach progress to yet"
    );

    let with_progress: Vec<&String> = backup
        .backup_manga
        .iter()
        .filter(|m| m.chapters.iter().any(|c| c.read || c.last_page_read > 0))
        .map(|m| &m.title)
        .collect();
    assert!(!with_progress.is_empty());
    for title in with_progress {
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains(title) && w.contains("chapters")),
            "progress for '{title}' was dropped without a warning: {:?}",
            result.warnings
        );
    }
}

#[tokio::test]
async fn a_series_resembling_one_already_in_the_library_is_parked_for_review() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    let incoming = &backup.backup_manga[0];
    let other_source = common::insert_source(&svc.db, "Existing Source").await;
    let existing =
        common::insert_manga(&svc.db, other_source, "already-here", &incoming.title).await;

    let result = svc
        .import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();

    assert_eq!(result.possible_duplicates, 1, "{:?}", result.warnings);
    assert_eq!(
        result.imported_manga as usize,
        backup.backup_manga.len() - 1
    );

    let (title, duplicate_of): (String, Option<i64>) = sqlx::query_as(
        "SELECT title, possible_duplicate_of FROM pending_imports WHERE source_manga_id = ?",
    )
    .bind(&incoming.url)
    .fetch_one(&svc.db)
    .await
    .unwrap();
    assert_eq!(title, incoming.title);
    assert_eq!(
        duplicate_of,
        Some(existing.0),
        "the pending row must point at the series it resembles"
    );

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga WHERE source_manga_id = ?")
        .bind(&incoming.url)
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(stored, 0, "a parked series must not also be imported");
}

#[tokio::test]
async fn an_unknown_source_id_is_reported_not_silently_dropped() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;

    let known = backup.backup_manga[0].source;
    register_source(&svc.db, "Known Source", known).await;
    let unknown_count = backup
        .backup_manga
        .iter()
        .filter(|m| m.source != known)
        .count();
    assert!(unknown_count > 0, "the fixture must span several sources");

    let result = svc
        .import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();

    assert_eq!(result.skipped_manga as usize, unknown_count);
    assert_eq!(result.pending_imports_added as usize, unknown_count);
    assert_eq!(result.warnings.len(), unknown_count);

    for m in backup.backup_manga.iter().filter(|m| m.source != known) {
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pending_imports \
             WHERE origin = 'tachiyomi' AND source_manga_id = ? AND title = ?",
        )
        .bind(&m.url)
        .bind(&m.title)
        .fetch_one(&svc.db)
        .await
        .unwrap();
        assert_eq!(pending, 1, "'{}' was dropped instead of parked", m.title);
        assert!(
            result.warnings.iter().any(|w| w.contains(&m.title)),
            "no warning named '{}': {:?}",
            m.title,
            result.warnings
        );
    }

    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(
        stored as usize,
        backup.backup_manga.len() - unknown_count,
        "only the resolvable series belong in the library"
    );
}

#[tokio::test]
async fn a_truncated_backup_is_rejected_cleanly() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    let half = &data[..data.len() / 2];
    let err = svc
        .import_tachiyomi_backup(uid, half, options(false))
        .await
        .expect_err("half a gzip stream must not import");
    let message = err.to_string();
    assert!(
        message.contains("decompress") || message.contains("decode"),
        "unhelpful error: {message}"
    );

    assert!(
        svc.preview_tachiyomi_backup(half).await.is_err(),
        "the preview must reject it too"
    );

    for table in ["manga", "categories", "pending_imports"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&svc.db)
            .await
            .unwrap();
        assert_eq!(count, 0, "a rejected backup wrote to {table}");
    }
}

#[tokio::test]
async fn a_backup_with_hostile_titles_is_stored_intact() {
    let data = fixture("hostile-titles.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    let titles: Vec<String> = backup
        .backup_manga
        .iter()
        .map(|m| m.title.clone())
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("../"))
            && titles.iter().any(|t| t.contains('\0'))
            && titles.iter().any(|t| t.contains('\u{202e}')),
        "the fixture lost its hostile cases: {titles:?}"
    );

    svc.import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();

    for m in &backup.backup_manga {
        let stored: String = sqlx::query_scalar("SELECT name FROM manga WHERE source_manga_id = ?")
            .bind(&m.url)
            .fetch_one(&svc.db)
            .await
            .unwrap_or_else(|e| panic!("series {:?} was not stored: {e}", m.title));
        assert_eq!(stored, m.title, "the title was mangled in the DB");
    }

    let library = svc.settings.read().await.library_path.clone();
    for m in &backup.backup_manga {
        let manga_id: i64 = sqlx::query_scalar("SELECT id FROM manga WHERE source_manga_id = ?")
            .bind(&m.url)
            .fetch_one(&svc.db)
            .await
            .unwrap();
        let safe_name = format!(
            "{} - {}",
            kani_core::utilities::sanitize_filename(&m.title),
            manga_id
        );
        let dir = library.join(&safe_name);
        assert!(
            !safe_name.contains(".."),
            "traversal survived sanitisation: {safe_name:?}"
        );
        assert!(
            !safe_name.contains('\0'),
            "a NUL survived into a path: {safe_name:?}"
        );
        assert!(
            !safe_name.contains('/') && !safe_name.contains('\\'),
            "a separator survived sanitisation: {safe_name:?}"
        );
        assert_eq!(
            dir.parent(),
            Some(library.as_path()),
            "'{}' resolved outside the library: {}",
            m.title,
            dir.display()
        );
        assert!(
            dir.components().count() == library.components().count() + 1,
            "the stored title added a path level: {}",
            dir.display()
        );
    }
}

#[tokio::test]
async fn importing_twice_is_idempotent() {
    let data = fixture("suwayomi-anonymised.tachibk");
    let backup = decode(&data);
    let svc = common::test_service().await;
    let uid = user(&svc).await;
    register_every_source(&svc.db, &backup).await;

    svc.import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();
    let after_first: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga")
        .fetch_one(&svc.db)
        .await
        .unwrap();

    let second = svc
        .import_tachiyomi_backup(uid, &data, options(false))
        .await
        .unwrap();

    let after_second: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM manga")
        .fetch_one(&svc.db)
        .await
        .unwrap();
    assert_eq!(
        after_second, after_first,
        "the second import duplicated rows"
    );
    assert_eq!(second.pending_imports_added, 0);
    assert_eq!(second.possible_duplicates, 0);

    for table in ["categories", "manga_categories", "tracker_manga_mappings"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&svc.db)
            .await
            .unwrap();
        let expected: i64 = match table {
            "categories" => backup.backup_categories.len() as i64,
            "manga_categories" => backup
                .backup_manga
                .iter()
                .map(|m| m.categories.len() as i64)
                .sum(),
            _ => backup
                .backup_manga
                .iter()
                .filter(|m| !m.tracking.is_empty())
                .count() as i64,
        };
        assert_eq!(count, expected, "{table} grew on the second import");
    }
}
