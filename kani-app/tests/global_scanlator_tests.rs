#![allow(clippy::unwrap_used)]
// Library-wide scanlator defaults: per-manga still wins, and the defaults must
// actually reach the code that decides what to download.

mod common;
use common::{insert_chapter, insert_manga, insert_source, test_service};

#[tokio::test]
async fn a_global_default_applies_where_no_per_manga_rule_exists() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "s").await;
    let manga = insert_manga(&svc.db, src, "m", "M").await;

    svc.set_global_scanlator_pref("Good", 10, false)
        .await
        .unwrap();
    svc.set_global_scanlator_pref("Bad", 0, true).await.unwrap();

    let eff = svc.effective_scanlator_prefs(manga).await.unwrap();
    assert_eq!(eff.len(), 2);
    assert_eq!(eff[0].scanlator, "Good");
    assert!(eff.iter().all(|p| p.manga_id.is_none()));
}

#[tokio::test]
async fn a_per_manga_rule_overrides_the_global_one() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "s").await;
    let manga = insert_manga(&svc.db, src, "m", "M").await;

    svc.set_global_scanlator_pref("Group", 10, false)
        .await
        .unwrap();
    svc.set_scanlator_pref(manga, "Group", 1, true)
        .await
        .unwrap();

    let eff = svc.effective_scanlator_prefs(manga).await.unwrap();
    assert_eq!(
        eff.len(),
        1,
        "the global must not appear alongside its override"
    );
    assert_eq!(eff[0].manga_id, Some(manga.0));
    assert!(
        eff[0].blocked,
        "the per-manga block wins over a global preference"
    );
}

#[tokio::test]
async fn globals_and_per_manga_rules_coexist_for_different_groups() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "s").await;
    let manga = insert_manga(&svc.db, src, "m", "M").await;

    svc.set_global_scanlator_pref("Global", 5, false)
        .await
        .unwrap();
    svc.set_scanlator_pref(manga, "Local", 9, false)
        .await
        .unwrap();

    let eff = svc.effective_scanlator_prefs(manga).await.unwrap();
    assert_eq!(eff.len(), 2);
    assert_eq!(eff[0].scanlator, "Local", "ordered by priority");
    assert_eq!(eff[1].scanlator, "Global");
}

#[tokio::test]
async fn setting_the_same_global_twice_updates_rather_than_duplicating() {
    let svc = test_service().await;
    svc.set_global_scanlator_pref("Group", 1, false)
        .await
        .unwrap();
    svc.set_global_scanlator_pref("Group", 8, true)
        .await
        .unwrap();

    let globals = svc.get_global_scanlator_prefs().await.unwrap();
    assert_eq!(
        globals.len(),
        1,
        "SQLite treats NULLs as distinct, so a plain ON CONFLICT would have \
         inserted a second row here"
    );
    assert_eq!(globals[0].priority, 8);
    assert!(globals[0].blocked);
}

#[tokio::test]
async fn an_empty_global_name_is_rejected() {
    let svc = test_service().await;
    assert!(
        svc.set_global_scanlator_pref("   ", 1, false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_global_default_reaches_upgrade_detection() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "s").await;
    let manga = insert_manga(&svc.db, src, "m", "M").await;

    let held = insert_chapter(&svc.db, manga, "held", 1.0).await;
    sqlx::query(
        "UPDATE chapters SET download_status = 2, scanlator = 'Low', page_count = 3 WHERE id = ?",
    )
    .bind(held)
    .execute(&svc.db)
    .await
    .unwrap();
    let rival = insert_chapter(&svc.db, manga, "rival", 1.0).await;
    sqlx::query("UPDATE chapters SET scanlator = 'High', page_count = 3 WHERE id = ?")
        .bind(rival)
        .execute(&svc.db)
        .await
        .unwrap();

    // Only global preferences exist — no per-manga rules at all.
    svc.set_global_scanlator_pref("Low", 1, false)
        .await
        .unwrap();
    svc.set_global_scanlator_pref("High", 10, false)
        .await
        .unwrap();

    let found = svc.evaluate_upgrades(manga).await.unwrap();
    assert!(
        found
            .iter()
            .any(|c| c.candidate_scanlator.as_deref() == Some("High")),
        "a library-wide preference that upgrade detection ignores is a \
         preference in name only"
    );
}

#[tokio::test]
async fn scanlators_by_usage_ranks_by_how_much_you_hold() {
    let svc = test_service().await;
    let src = insert_source(&svc.db, "s").await;
    let manga = insert_manga(&svc.db, src, "m", "M").await;

    for (i, group) in ["A", "A", "A", "B"].iter().enumerate() {
        let c = insert_chapter(&svc.db, manga, &format!("c{i}"), i as f64).await;
        sqlx::query("UPDATE chapters SET scanlator = ? WHERE id = ?")
            .bind(group)
            .bind(c)
            .execute(&svc.db)
            .await
            .unwrap();
    }

    let ranked = svc.scanlators_by_usage().await.unwrap();
    assert_eq!(ranked[0], ("A".to_string(), 3));
    assert_eq!(ranked[1], ("B".to_string(), 1));
}
