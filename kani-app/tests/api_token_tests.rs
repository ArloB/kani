#![allow(clippy::unwrap_used)]

mod common;
use common::{insert_user, test_service};
use kani_app::permissions::{Opds, Permission};

#[tokio::test]
async fn create_then_authenticate_succeeds() {
    let svc = test_service().await;
    let user_id = insert_user(&svc.db, "alice").await;

    let created = svc
        .create_api_token(user_id, "my reader", None)
        .await
        .unwrap();
    assert!(created.raw_token.starts_with("kani_"));
    assert_eq!(created.token.name, "my reader");

    let auth = svc
        .authenticate_api_token(&created.raw_token)
        .await
        .unwrap()
        .expect("token should authenticate");
    assert_eq!(auth.user_id, user_id);
    assert_eq!(
        auth.scopes,
        vec![
            Permission::Opds(Opds::Read),
            Permission::Opds(Opds::Progress)
        ]
    );
}

#[tokio::test]
async fn list_never_exposes_raw_token_or_hash() {
    let svc = test_service().await;
    let user_id = insert_user(&svc.db, "bob").await;

    let created = svc.create_api_token(user_id, "reader", None).await.unwrap();

    let tokens = svc.list_api_tokens(user_id).await.unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "reader");
    assert_eq!(tokens[0].id, created.token.id);
    // ApiToken carries no raw token or hash field — nothing to leak by construction.
}

#[tokio::test]
async fn revoke_then_authenticate_fails() {
    let svc = test_service().await;
    let user_id = insert_user(&svc.db, "carol").await;

    let created = svc.create_api_token(user_id, "reader", None).await.unwrap();
    svc.revoke_api_token(user_id, &created.token.id)
        .await
        .unwrap();

    let auth = svc
        .authenticate_api_token(&created.raw_token)
        .await
        .unwrap();
    assert!(auth.is_none());

    let tokens = svc.list_api_tokens(user_id).await.unwrap();
    assert!(tokens.is_empty());
}

#[tokio::test]
async fn expired_token_fails() {
    let svc = test_service().await;
    let user_id = insert_user(&svc.db, "dave").await;

    let created = svc
        .create_api_token(user_id, "reader", Some(30))
        .await
        .unwrap();

    sqlx::query("UPDATE api_tokens SET expires_at = unixepoch() - 10 WHERE id = ?")
        .bind(&created.token.id)
        .execute(&svc.db)
        .await
        .unwrap();

    let auth = svc
        .authenticate_api_token(&created.raw_token)
        .await
        .unwrap();
    assert!(auth.is_none());
}

#[tokio::test]
async fn wrong_user_revoke_is_not_found() {
    let svc = test_service().await;
    let owner = insert_user(&svc.db, "erin").await;
    let other = insert_user(&svc.db, "frank").await;

    let created = svc.create_api_token(owner, "reader", None).await.unwrap();

    let err = svc
        .revoke_api_token(other, &created.token.id)
        .await
        .unwrap_err();
    assert!(matches!(err, kani_app::error::ServiceError::NotFound(_)));

    // Owner's token still authenticates.
    assert!(
        svc.authenticate_api_token(&created.raw_token)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn garbage_token_returns_ok_none() {
    let svc = test_service().await;
    assert!(
        svc.authenticate_api_token("not-a-kani-token")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        svc.authenticate_api_token("kani_deadbeef")
            .await
            .unwrap()
            .is_none()
    );
}
