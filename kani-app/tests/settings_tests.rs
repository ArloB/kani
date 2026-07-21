#![allow(clippy::unwrap_used)]

mod common;
use common::test_service;
use kani_app::ids::UserId;
use kani_shared::types::{AdvancedSettings, DownloadSettings, ScanSettings, SettingsUpdate};

#[tokio::test]
async fn get_settings_reflects_initial_values() {
    let svc = test_service().await;
    let s = svc.get_settings().await;
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
            scan_concurrency: 4,
            per_source_download_concurrency: 2,
        }),
        UserId(1),
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

fn advanced_settings() -> AdvancedSettings {
    AdvancedSettings {
        flaresolverr_url: String::new(),
        library_path: "/data/library".into(),
        wasm_storage_path: "/data/wasm".into(),
        max_wasm_instances: 4,
        http_request_logging: false,
        browser_debug_logging: false,
        registration_enabled: true,
        cover_max_dimension: Some(512),
        browser_max_memory_mb: 512,
        browser_max_instances: 2,
        browser_idle_timeout_s: 300,
        update_check_enabled: true,
    }
}

#[tokio::test]
async fn update_advanced_settings_round_trips_browser_caps() {
    let svc = test_service().await;

    svc.update_settings(
        SettingsUpdate::Advanced(AdvancedSettings {
            browser_max_memory_mb: 1024,
            browser_max_instances: 4,
            browser_idle_timeout_s: 120,
            ..advanced_settings()
        }),
        UserId(1),
    )
    .await
    .unwrap();

    let s = svc.get_settings().await;
    assert_eq!(s.browser_max_memory_mb, 1024);
    assert_eq!(s.browser_max_instances, 4);
    assert_eq!(s.browser_idle_timeout_s, 120);
}

#[tokio::test]
async fn update_advanced_settings_rejects_invalid_browser_caps() {
    let svc = test_service().await;

    let result = svc
        .update_settings(
            SettingsUpdate::Advanced(AdvancedSettings {
                browser_max_instances: 0,
                ..advanced_settings()
            }),
            UserId(1),
        )
        .await;
    assert!(result.is_err());
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
        UserId(1),
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
            UserId(1),
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
                scan_concurrency: 4,
                per_source_download_concurrency: 2,
            }),
            UserId(1),
        )
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn update_settings_does_not_affect_unrelated_fields() {
    let svc = test_service().await;

    svc.update_settings(
        SettingsUpdate::Scan(ScanSettings {
            auto_scan: true,
            scan_interval_minutes: 30,
            scan_exclude_completed: false,
        }),
        UserId(1),
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
            scan_concurrency: 4,
            per_source_download_concurrency: 2,
        }),
        UserId(1),
    )
    .await
    .unwrap();

    let s = svc.get_settings().await;
    assert!(s.auto_scan, "scan setting should be unchanged");
    assert_eq!(
        s.scan_interval_minutes, 30,
        "scan interval should be unchanged"
    );
    assert_eq!(
        s.concurrent_page_downloads, 8,
        "download setting should be updated"
    );
}

#[tokio::test]
async fn update_check_enabled_round_trips_and_defaults_on() {
    let svc = test_service().await;

    assert!(
        svc.get_settings().await.update_check_enabled,
        "update checking should be on by default"
    );

    let mut advanced = advanced_settings();
    advanced.update_check_enabled = false;
    svc.update_settings(SettingsUpdate::Advanced(advanced), UserId(1))
        .await
        .unwrap();

    assert!(
        !svc.get_settings().await.update_check_enabled,
        "the toggle must persist through update_settings"
    );
}
