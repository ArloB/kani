#![allow(clippy::unwrap_used)]
// Upgrade detection: what gets flagged, what deliberately does not, and the
// reversibility of applying one.

mod common;
use common::{insert_chapter, insert_manga, insert_source, test_service};
use kani_app::ids::{ChapterId, MangaId};
use kani_app::service::quality::UpgradeKind;
use std::io::Write;
use std::path::Path;

fn png_bytes(shade: u8) -> Vec<u8> {
    let mut img = image::GrayImage::new(16, 24);
    for (x, _y, p) in img.enumerate_pixels_mut() {
        *p = image::Luma([shade.wrapping_add((x % 255) as u8)]);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageLuma8(img)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn write_cbz(path: &Path, pages: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    for i in 0..pages {
        zip.start_file(format!("{:04}.png", i + 1), opts).unwrap();
        zip.write_all(&png_bytes(i as u8 * 9)).unwrap();
    }
    zip.finish().unwrap();
}

/// A downloaded chapter whose manifest records `pages` pages.
async fn held_chapter(
    svc: &kani_app::service::AppService,
    manga: MangaId,
    title: &str,
    num: f64,
    pages: usize,
) -> ChapterId {
    let chapter = insert_chapter(&svc.db, manga, &format!("src-{num}"), num).await;
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
    let cbz = svc.chapter_cbz_path(chapter).await.unwrap().path;
    write_cbz(&cbz, pages);
    svc.record_chapter_manifest(chapter, cbz).await;
    chapter
}

async fn seed_manga(svc: &kani_app::service::AppService, title: &str) -> MangaId {
    let src = insert_source(&svc.db, &format!("s-{title}")).await;
    insert_manga(&svc.db, src, "m1", title).await
}

#[tokio::test]
async fn a_longer_relisting_is_flagged_as_a_reupload() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Reupload").await;
    let chapter = held_chapter(&svc, manga, "Reupload", 1.0, 2).await;

    // The source now advertises more pages than the copy on disk.
    sqlx::query("UPDATE chapters SET page_count = 5 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, UpgradeKind::QualityReupload);
    assert_eq!(found[0].held_page_count, Some(2));
    assert_eq!(found[0].candidate_page_count, Some(5));
}

#[tokio::test]
async fn a_shorter_relisting_is_reassurance_not_a_nag() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Downgrade").await;
    let chapter = held_chapter(&svc, manga, "Downgrade", 1.0, 5).await;

    sqlx::query("UPDATE chapters SET page_count = 2 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].kind,
        UpgradeKind::SourceDowngraded,
        "yours is the better copy; this must never read as 'replace me'"
    );
    assert_eq!(found[0].reason_key, "upgrade.reason.source_downgraded");
}

#[tokio::test]
async fn a_matching_relisting_flags_nothing() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Stable").await;
    let chapter = held_chapter(&svc, manga, "Stable", 1.0, 3).await;
    sqlx::query("UPDATE chapters SET page_count = 3 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();

    assert!(svc.evaluate_upgrades(manga).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_better_ranked_scanlator_is_flagged() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Groups").await;
    let held = held_chapter(&svc, manga, "Groups", 1.0, 3).await;
    sqlx::query("UPDATE chapters SET scanlator = 'Low', page_count = 3 WHERE id = ?")
        .bind(held.0)
        .execute(&svc.db)
        .await
        .unwrap();

    // A sibling at the same chapter number from another group.
    let rival = insert_chapter(&svc.db, manga, "rival", 1.0).await;
    sqlx::query("UPDATE chapters SET scanlator = 'High', page_count = 3 WHERE id = ?")
        .bind(rival)
        .execute(&svc.db)
        .await
        .unwrap();

    svc.set_scanlator_pref(manga, "Low", 1, false)
        .await
        .unwrap();
    svc.set_scanlator_pref(manga, "High", 10, false)
        .await
        .unwrap();

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    let pref: Vec<_> = found
        .iter()
        .filter(|c| c.kind == UpgradeKind::PreferredScanlator)
        .collect();
    assert_eq!(pref.len(), 1);
    assert_eq!(pref[0].held_chapter_id, held.0);
    assert_eq!(pref[0].candidate_scanlator.as_deref(), Some("High"));
}

#[tokio::test]
async fn a_worse_ranked_scanlator_is_not_flagged() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Ranked").await;
    let held = held_chapter(&svc, manga, "Ranked", 1.0, 3).await;
    sqlx::query("UPDATE chapters SET scanlator = 'High', page_count = 3 WHERE id = ?")
        .bind(held.0)
        .execute(&svc.db)
        .await
        .unwrap();
    let rival = insert_chapter(&svc.db, manga, "rival", 1.0).await;
    sqlx::query("UPDATE chapters SET scanlator = 'Low', page_count = 3 WHERE id = ?")
        .bind(rival)
        .execute(&svc.db)
        .await
        .unwrap();

    svc.set_scanlator_pref(manga, "Low", 1, false)
        .await
        .unwrap();
    svc.set_scanlator_pref(manga, "High", 10, false)
        .await
        .unwrap();

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert!(
        !found
            .iter()
            .any(|c| c.kind == UpgradeKind::PreferredScanlator),
        "you already hold the preferred group's release"
    );
}

#[tokio::test]
async fn a_blocked_scanlator_is_never_a_candidate() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Blocked").await;
    let held = held_chapter(&svc, manga, "Blocked", 1.0, 3).await;
    sqlx::query("UPDATE chapters SET scanlator = 'Low', page_count = 3 WHERE id = ?")
        .bind(held.0)
        .execute(&svc.db)
        .await
        .unwrap();
    let rival = insert_chapter(&svc.db, manga, "rival", 1.0).await;
    sqlx::query("UPDATE chapters SET scanlator = 'Banned', page_count = 3 WHERE id = ?")
        .bind(rival)
        .execute(&svc.db)
        .await
        .unwrap();

    svc.set_scanlator_pref(manga, "Low", 1, false)
        .await
        .unwrap();
    svc.set_scanlator_pref(manga, "Banned", 99, true)
        .await
        .unwrap();

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert!(
        !found
            .iter()
            .any(|c| c.kind == UpgradeKind::PreferredScanlator),
        "a blocked group outranking everything would otherwise be recommended"
    );
}

#[tokio::test]
async fn dismissing_prevents_the_same_candidate_returning() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Dismissed").await;
    let chapter = held_chapter(&svc, manga, "Dismissed", 1.0, 2).await;
    sqlx::query("UPDATE chapters SET page_count = 6 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();

    assert_eq!(svc.evaluate_upgrades(manga).await.unwrap().len(), 1);
    svc.dismiss_upgrade(chapter).await.unwrap();
    assert!(svc.get_upgrades(manga).await.unwrap().is_empty());

    // The whole point: a re-scan must not raise it again.
    assert!(
        svc.evaluate_upgrades(manga).await.unwrap().is_empty(),
        "a dismissed candidate that returns on the next scan is worse than \
         never offering the button"
    );
}

#[tokio::test]
async fn detection_can_be_turned_off_entirely() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Off").await;
    let chapter = held_chapter(&svc, manga, "Off", 1.0, 2).await;
    sqlx::query("UPDATE chapters SET page_count = 9 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();
    {
        let mut s = svc.settings.write().await;
        s.upgrade_detection_enabled = false;
    }

    assert!(svc.evaluate_upgrades(manga).await.unwrap().is_empty());
}

#[tokio::test]
async fn applying_an_upgrade_keeps_the_old_file() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Applied").await;
    let chapter = held_chapter(&svc, manga, "Applied", 1.0, 3).await;
    let old_path = svc.chapter_cbz_path(chapter).await.unwrap().path;
    let library = { svc.settings.read().await.library_path.clone() };

    // The download will fail (no real source), but the file must already be
    // preserved by then.
    let _ = svc.apply_upgrade(chapter, kani_app::ids::UserId(1)).await;

    assert!(!old_path.exists(), "the held file is moved out of the way");
    let replaced: Vec<_> = std::fs::read_dir(library.join(".replaced"))
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(
        replaced.len(),
        1,
        "the old copy must survive in .replaced — this is the only thing \
         making an upgrade reversible"
    );
    assert!(
        replaced[0]
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{}-", chapter.0)),
        "the holding file must name the chapter it came from"
    );
}

#[tokio::test]
async fn applying_an_upgrade_clears_the_stale_hashes() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Cleared").await;
    let chapter = held_chapter(&svc, manga, "Cleared", 1.0, 3).await;

    let _ = svc.apply_upgrade(chapter, kani_app::ids::UserId(1)).await;

    let (status, hash, flag): (i64, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT download_status, content_hash, upgrade_available FROM chapters WHERE id = ?",
    )
    .bind(chapter.0)
    .fetch_one(&svc.db)
    .await
    .unwrap();

    assert_ne!(status, 2, "the chapter is no longer downloaded");
    assert!(
        hash.is_none(),
        "the old hash would make the next scrub call the new file corrupt"
    );
    assert!(flag.is_none(), "the acted-on flag must clear");
}

#[tokio::test]
async fn the_replaced_sweep_respects_the_retention_window() {
    let svc = test_service().await;
    let library = { svc.settings.read().await.library_path.clone() };
    let dir = library.join(".replaced");
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("1-abcdef.cbz");
    std::fs::write(&f, b"old").unwrap();

    assert_eq!(
        svc.purge_replaced(30).await.unwrap(),
        0,
        "a file inside the window must stay recoverable"
    );
    assert!(f.exists());

    assert_eq!(svc.purge_replaced(0).await.unwrap(), 1);
    assert!(!f.exists());
}

#[tokio::test]
async fn a_zero_confirmation_budget_falls_back_to_page_count() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "NoBudget").await;
    let chapter = held_chapter(&svc, manga, "NoBudget", 1.0, 2).await;
    sqlx::query("UPDATE chapters SET page_count = 5 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();
    {
        let mut s = svc.settings.write().await;
        s.upgrade_confirm_fetches = 0;
    }

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].kind,
        UpgradeKind::QualityReupload,
        "with no budget to confirm, a longer listing is still the only signal \
         available and must not be silently discarded"
    );
}

#[tokio::test]
async fn a_confirmation_that_cannot_reach_the_source_keeps_the_count_verdict() {
    let svc = test_service().await;
    let manga = seed_manga(&svc, "Unreachable").await;
    let chapter = held_chapter(&svc, manga, "Unreachable", 1.0, 2).await;
    sqlx::query("UPDATE chapters SET page_count = 5 WHERE id = ?")
        .bind(chapter.0)
        .execute(&svc.db)
        .await
        .unwrap();
    {
        let mut s = svc.settings.write().await;
        s.upgrade_confirm_fetches = 3;
    }

    // The seeded source has no backend, so the probe cannot run.
    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert_eq!(
        found.len(),
        1,
        "a source that cannot be reached must not make detection fail or drop \
         the candidate"
    );
    assert_eq!(found[0].kind, UpgradeKind::QualityReupload);
}
