#![allow(clippy::unwrap_used)]

mod common;
use common::test_service;
use kani_shared::types::{DownloadSettings, ScanSettings, SettingsUpdate};

#[tokio::test]
async fn get_settings_reflects_initial_values() {
    let svc = test_service().await;
    let s = svc.get_settings().await;
    // Defaults from new_for_test
    assert_eq!(s.concurrent_page_downloads, 4);
    assert_eq!(s.max_retries, 3);
    assert!(!s.auto_scan);
    assert!(s.registration_enabled);
}

#[tokio::test]
async fn update_download_settings_round_trips() {
    let svc = test_service().await;

    svc.update_settings(
        SettingsUpdate::Download(DownloadSettings {
            concurrent_page_downloads: 8,
            concurrent_manga_downloads: 4,
            chapter_queue_size: 64,
            max_retries: 5,
            initial_retry_delay_ms: 200,
            auto_download_category_ids: vec![],
        }),
        1,
    )
    .await
    .unwrap();

    let s = svc.get_settings().await;
    assert_eq!(s.concurrent_page_downloads, 8);
    assert_eq!(s.concurrent_manga_downloads, 4);
    assert_eq!(s.chapter_queue_size, 64);
    assert_eq!(s.max_retries, 5);
    assert_eq!(s.initial_retry_delay_ms, 200);
}

#[tokio::test]
async fn update_scan_settings_round_trips() {
    let svc = test_service().await;

    svc.update_settings(
        SettingsUpdate::Scan(ScanSettings {
            auto_scan: true,
            scan_interval_minutes: 30,
            scan_exclude_completed: true,
        }),
        1,
    )
    .await
    .unwrap();

    let s = svc.get_settings().await;
    assert!(s.auto_scan);
    assert_eq!(s.scan_interval_minutes, 30);
    assert!(s.scan_exclude_completed);
}

#[tokio::test]
async fn update_scan_settings_rejects_short_interval() {
    let svc = test_service().await;
    let result = svc
        .update_settings(
            SettingsUpdate::Scan(ScanSettings {
                auto_scan: false,
                scan_interval_minutes: 4, // below minimum of 5
                scan_exclude_completed: false,
            }),
            1,
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn update_download_settings_rejects_invalid_page_concurrency() {
    let svc = test_service().await;
    let result = svc
        .update_settings(
            SettingsUpdate::Download(DownloadSettings {
                concurrent_page_downloads: 0, // below minimum of 1
                concurrent_manga_downloads: 2,
                chapter_queue_size: 32,
                max_retries: 3,
                initial_retry_delay_ms: 100,
                auto_download_category_ids: vec![],
            }),
            1,
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn update_settings_does_not_affect_unrelated_fields() {
    let svc = test_service().await;

    // Set scan settings first
    svc.update_settings(
        SettingsUpdate::Scan(ScanSettings {
            auto_scan: true,
            scan_interval_minutes: 30,
            scan_exclude_completed: false,
        }),
        1,
    )
    .await
    .unwrap();

    // Update download settings — scan settings should not change
    svc.update_settings(
        SettingsUpdate::Download(DownloadSettings {
            concurrent_page_downloads: 8,
            concurrent_manga_downloads: 2,
            chapter_queue_size: 32,
            max_retries: 3,
            initial_retry_delay_ms: 100,
            auto_download_category_ids: vec![],
        }),
        1,
    )
    .await
    .unwrap();

    let s = svc.get_settings().await;
    assert!(s.auto_scan, "scan setting should be unchanged");
    assert_eq!(s.scan_interval_minutes, 30, "scan interval should be unchanged");
    assert_eq!(s.concurrent_page_downloads, 8, "download setting should be updated");
}
