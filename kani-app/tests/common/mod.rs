#![allow(clippy::unwrap_used, dead_code)]

use kani_app::service::AppService;
use sqlx::SqlitePool;

pub async fn test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("../migrations").run(&pool).await.unwrap();
    pool
}

pub async fn test_service() -> AppService {
    AppService::new_for_test(test_db().await).await
}

/// Inserts a minimal source row; returns its id.
pub async fn insert_source(pool: &SqlitePool, name: &str) -> i64 {
    sqlx::query_scalar("INSERT INTO sources (name, version) VALUES (?, '0.1') RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Inserts a minimal manga row (status 0 = Unknown); returns its id.
pub async fn insert_manga(
    pool: &SqlitePool,
    source_id: i64,
    source_manga_id: &str,
    name: &str,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO manga (source_id, source_manga_id, name, status) \
         VALUES (?, ?, ?, 0) RETURNING id",
    )
    .bind(source_id)
    .bind(source_manga_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Inserts a chapter row; returns its id.
pub async fn insert_chapter(
    pool: &SqlitePool,
    manga_id: i64,
    source_chapter_id: &str,
    number: f64,
) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO chapters (manga_id, source_chapter_id, chapter_number, language) \
         VALUES (?, ?, ?, 'en') RETURNING id",
    )
    .bind(manga_id)
    .bind(source_chapter_id)
    .bind(number)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Inserts a minimal user row; returns its id.
pub async fn insert_user(pool: &SqlitePool, username: &str) -> i64 {
    sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, change_id) \
         VALUES (?, ?, 'hash', randomblob(16)) RETURNING id",
    )
    .bind(username)
    .bind(format!("{}@test.local", username))
    .fetch_one(pool)
    .await
    .unwrap()
}
