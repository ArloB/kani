#![allow(clippy::unwrap_used)]

mod common;
use common::test_service;
use kani_app::ids::UserId;

#[tokio::test]
async fn add_source_returns_positive_id() {
    let svc = test_service().await;
    let id = svc.add_source("test-source", UserId(1)).await.unwrap();
    assert!(id > 0);
}

#[tokio::test]
async fn list_sources_empty_on_fresh_db() {
    let svc = test_service().await;
    let sources = svc.list_sources().await.unwrap();
    assert!(sources.is_empty());
}

#[tokio::test]
async fn list_sources_returns_added_sources() {
    let svc = test_service().await;
    svc.add_source("source-a", UserId(1)).await.unwrap();
    svc.add_source("source-b", UserId(1)).await.unwrap();

    let sources = svc.list_sources().await.unwrap();
    assert_eq!(sources.len(), 2);
    let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"source-a"));
    assert!(names.contains(&"source-b"));
}

#[tokio::test]
async fn get_source_returns_correct_row() {
    let svc = test_service().await;
    let id = svc.add_source("my-source", UserId(1)).await.unwrap();

    let source = svc.get_source(id).await.unwrap();
    assert_eq!(source.id, id);
    assert_eq!(source.name, "my-source");
}

#[tokio::test]
async fn get_source_returns_error_for_missing_id() {
    let svc = test_service().await;
    let result = svc.get_source(99999).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn delete_source_removes_it_from_list() {
    let svc = test_service().await;
    let id = svc.add_source("to-delete", UserId(1)).await.unwrap();
    assert_eq!(svc.list_sources().await.unwrap().len(), 1);

    svc.delete_source(id, UserId(1)).await.unwrap();
    assert!(svc.list_sources().await.unwrap().is_empty());
}

#[tokio::test]
async fn delete_source_is_idempotent() {
    let svc = test_service().await;
    let id = svc.add_source("del", UserId(1)).await.unwrap();
    svc.delete_source(id, UserId(1)).await.unwrap();
    // delete_source is a no-op when the source is already soft-deleted
    svc.delete_source(id, UserId(1)).await.unwrap();
    assert!(svc.list_sources().await.unwrap().is_empty());
}

#[tokio::test]
async fn set_and_get_preference_round_trips() {
    let svc = test_service().await;
    let id = svc.add_source("pref-src", UserId(1)).await.unwrap();

    svc.set_preference(id, "lang", "en").await.unwrap();
    let val = svc.get_preference(id, "lang").await.unwrap();
    assert_eq!(val, Some("en".to_string()));
}

#[tokio::test]
async fn get_preference_returns_none_for_missing_key() {
    let svc = test_service().await;
    let id = svc.add_source("pref-src", UserId(1)).await.unwrap();

    let val = svc.get_preference(id, "no-such-key").await.unwrap();
    assert_eq!(val, None);
}

#[tokio::test]
async fn get_all_preferences_returns_all_set_values() {
    let svc = test_service().await;
    let id = svc.add_source("pref-src", UserId(1)).await.unwrap();
    svc.set_preference(id, "a", "1").await.unwrap();
    svc.set_preference(id, "b", "2").await.unwrap();

    let prefs = svc.get_all_preferences(id).await.unwrap();
    assert_eq!(prefs.len(), 2);
    let map: std::collections::HashMap<&str, &str> = prefs
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(map.get("a"), Some(&"1"));
    assert_eq!(map.get("b"), Some(&"2"));
}

#[tokio::test]
async fn set_preference_overwrites_existing_value() {
    let svc = test_service().await;
    let id = svc.add_source("pref-src", UserId(1)).await.unwrap();

    svc.set_preference(id, "key", "old").await.unwrap();
    svc.set_preference(id, "key", "new").await.unwrap();

    let val = svc.get_preference(id, "key").await.unwrap();
    assert_eq!(val, Some("new".to_string()));
}
