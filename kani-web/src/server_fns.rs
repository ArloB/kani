use crate::types::{ChapterList, MangaInfo, MangaList};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub base_url: String,
}

/// Converts any `Display`-able error into a `ServerFnError`.
#[cfg(feature = "ssr")]
fn to_server_err(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

#[server]
pub async fn fetch_sources() -> Result<Vec<Source>, ServerFnError> {
    use crate::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query_as!(Source, "SELECT id, name, version, base_url FROM sources LIMIT 1000")
        .fetch_all(&state.db)
        .await
        .map_err(to_server_err)
}

#[server]
pub async fn get_popular_manga(source_id: i64, page: i32) -> Result<MangaList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let json = state
        .get_popular_manga(source_id, page)
        .await
        .map_err(to_server_err)?;
    serde_json::from_str(&json).map_err(to_server_err)
}

#[server]
pub async fn search_manga(
    source_id: i64,
    query: String,
    page: i32,
) -> Result<MangaList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let json = state
        .search_manga(source_id, &query, page)
        .await
        .map_err(to_server_err)?;
    serde_json::from_str(&json).map_err(to_server_err)
}

#[server]
pub async fn get_manga_details(
    source_id: i64,
    manga_id: String,
) -> Result<MangaInfo, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let json = state
        .get_manga_details(source_id, &manga_id)
        .await
        .map_err(to_server_err)?;
    serde_json::from_str(&json).map_err(to_server_err)
}

#[server]
pub async fn get_chapter_list(
    source_id: i64,
    manga_id: String,
    page: i32,
) -> Result<ChapterList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let json = state
        .get_chapter_list_paged(source_id, &manga_id, page)
        .await
        .map_err(to_server_err)?;
    serde_json::from_str(&json).map_err(to_server_err)
}

#[server]
pub async fn save_to_library(source_id: i64, manga_id: String) -> Result<i64, ServerFnError> {
    use kani_shared::{ChapterList as KaniChapterList, MangaInfo as KaniMangaInfo};

    let state = expect_context::<crate::state::AppState>();

    let exists: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM manga WHERE source_manga_id = ? AND source_id = ?",
        manga_id,
        source_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(to_server_err)?
    .flatten();

    if let Some(id) = exists {
        return Ok(id);
    }

    let manga: KaniMangaInfo = serde_json::from_str(
        &state
            .get_manga_details(source_id, &manga_id)
            .await
            .map_err(to_server_err)?,
    )
    .map_err(to_server_err)?;

    let chapters: KaniChapterList = serde_json::from_str(
        &state
            .get_chapter_list(source_id, &manga_id)
            .await
            .map_err(to_server_err)?,
    )
    .map_err(to_server_err)?;

    let mut tx = state.db.begin().await.map_err(to_server_err)?;
    let status: i64 = manga.status.into();

    let result = sqlx::query!(
        "INSERT INTO manga (source_manga_id, source_id, name, cover_url, description, status) \
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        manga.id,
        source_id,
        manga.title,
        manga.cover_url,
        manga.description,
        status
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(to_server_err)?;

    let manga_row_id = result
        .id
        .ok_or_else(|| ServerFnError::new("Failed to get manga id"))?;

    for author in &manga.authors {
        sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", author)
            .execute(&mut *tx)
            .await
            .map_err(to_server_err)?;
        sqlx::query!(
            "INSERT OR IGNORE INTO manga_authors (manga_id, person_id) \
             SELECT ?, id FROM people WHERE name = ?",
            manga_row_id,
            author
        )
        .execute(&mut *tx)
        .await
        .map_err(to_server_err)?;
    }

    for artist in &manga.artists {
        sqlx::query!("INSERT OR IGNORE INTO people (name) VALUES (?)", artist)
            .execute(&mut *tx)
            .await
            .map_err(to_server_err)?;
        sqlx::query!(
            "INSERT OR IGNORE INTO manga_artists (manga_id, person_id) \
             SELECT ?, id FROM people WHERE name = ?",
            manga_row_id,
            artist
        )
        .execute(&mut *tx)
        .await
        .map_err(to_server_err)?;
    }

    for tag in &manga.tags {
        sqlx::query!("INSERT OR IGNORE INTO tags (name) VALUES (?)", tag)
            .execute(&mut *tx)
            .await
            .map_err(to_server_err)?;
        sqlx::query!(
            "INSERT OR IGNORE INTO manga_tags (manga_id, tag_id) \
             SELECT ?, id FROM tags WHERE name = ?",
            manga_row_id,
            tag
        )
        .execute(&mut *tx)
        .await
        .map_err(to_server_err)?;
    }

    for chapter in chapters.chapters {
        sqlx::query!(
            "INSERT INTO chapters \
             (manga_id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            manga_row_id,
            chapter.id,
            chapter.title,
            chapter.number,
            chapter.language,
            chapter.volume,
            chapter.scanlator,
            chapter.date_uploaded
        )
        .execute(&mut *tx)
        .await
        .map_err(to_server_err)?;
    }

    tx.commit().await.map_err(to_server_err)?;
    Ok(manga_row_id)
}

#[server]
pub async fn start_download(chapter_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    state
        .start_download(chapter_id)
        .await
        .map_err(to_server_err)
}

pub fn proxy_url(url: &str, referer: &str) -> String {
    format!(
        "/api/image_proxy?url={}&referer={}",
        urlencoding::encode(url),
        referer
    )
}
