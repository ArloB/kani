#![allow(clippy::unwrap_used)]

//! Group D — source lifecycle. D5 (disabled vs not-found) and D6 (uninstall
//! removes the backend and the artifact) are the DB/registry-driven cases; the
//! in-flight / drain / hot-swap cases (D1–D4) need parked concurrent calls and
//! are tracked separately.

mod common;
use common::{insert_source, test_service};

use std::collections::HashMap;
use std::sync::Arc;

use kani_app::error::ServiceError;
use kani_app::service::AppService;
use kani_app::source::{SourceBackend, YamlSource};
use kani_shared_test::insert_user;
use kani_yaml::yaml::model::ValidatedExtension;

fn yaml_backend(name: &str) -> SourceBackend {
    let ext = ValidatedExtension {
        id: name.into(),
        name: name.into(),
        version: "1.0.0".into(),
        base_url: "http://127.0.0.1:1".into(),
        language: "en".into(),
        unrestricted_http: true,
        ..Default::default()
    };
    SourceBackend::Yaml(Box::new(YamlSource::new(
        Arc::new(ext),
        kani_core::http::SmartClient::new(None).unwrap(),
        Arc::new(kani_core::cache::InMemoryCache::new()),
        "test:".into(),
        HashMap::new(),
        true,
    )))
}

// D5 — a disabled source reports "disabled", a missing one reports "not found".
// `require_source_active` distinguishes them, but only once the backend is no
// longer in the in-memory registry (it short-circuits Ok while present).
#[tokio::test]
async fn a_disabled_source_reports_disabled_not_not_found() {
    let svc = test_service().await;
    let source_id = insert_source(&svc.db, "disabled-src").await;
    sqlx::query("UPDATE sources SET enabled = 0 WHERE id = ?")
        .bind(source_id)
        .execute(&svc.db)
        .await
        .unwrap();
    // Not inserted into svc.sources — mirrors a disabled source after its backend
    // was dropped from the registry.

    match svc.get_metadata(source_id).await {
        Err(ServiceError::SourceDisabled(id)) => assert_eq!(id, source_id),
        other => panic!("expected SourceDisabled, got {other:?}"),
    }

    match svc.get_metadata(999_999).await {
        Err(ServiceError::NotFound(_)) => {}
        other => panic!("expected NotFound for an unknown source, got {other:?}"),
    }
}

// D6 — uninstalling a source drops its backend from the registry, soft-deletes
// the row, and removes the on-disk .wasm artifact.
#[tokio::test]
async fn uninstall_removes_the_backend_and_the_artifact() {
    let svc = test_service().await;
    let user = insert_user(&svc.db, "admin").await;
    let name = "uninstall-me";
    let source_id = insert_source(&svc.db, name).await;
    svc.sources.insert(source_id, yaml_backend(name));

    // Plant the artifact the uninstall must remove.
    let storage = svc.settings.read().await.wasm_storage_path.clone();
    let artifact = storage.join(format!("{name}.wasm"));
    tokio::fs::write(&artifact, b"fake wasm bytes")
        .await
        .unwrap();
    assert!(artifact.exists(), "precondition: the artifact was planted");
    assert!(
        svc.sources.contains_key(source_id),
        "precondition: backend registered"
    );

    svc.delete_source(source_id, user).await.unwrap();

    assert!(
        !svc.sources.contains_key(source_id),
        "the backend was removed from the registry"
    );
    assert!(!artifact.exists(), "the .wasm artifact was deleted");
    let deleted_at: Option<String> =
        sqlx::query_scalar("SELECT deleted_at FROM sources WHERE id = ?")
            .bind(source_id)
            .fetch_one(&svc.db)
            .await
            .unwrap();
    assert!(deleted_at.is_some(), "the row was soft-deleted");
}

// Compile guard: keep AppService imported even if the helpers change.
#[allow(dead_code)]
fn _uses_service(_: &AppService) {}
