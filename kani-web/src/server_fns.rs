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
    .map_err(to_server_err)?;

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

    let manga_row_id = result.id;

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

#[server]
pub async fn cancel_download(chapter_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    
    state.downloader.cancel_download(chapter_id).await;

    let _ = sqlx::query!(
        "UPDATE chapters SET download_status = 0 WHERE id = ?",
        chapter_id
    )
    .execute(&state.db)
    .await
    .map_err(to_server_err)?;

    Ok(())
}

#[server]
pub async fn download_all(manga_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    let un_downloaded_chapters = sqlx::query_scalar!(
        "SELECT id FROM chapters WHERE manga_id = ? AND download_status = 0",
        manga_id
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_server_err)?;

    let state_clone = state.clone();
    tokio::spawn(async move {
        for chapter_id in un_downloaded_chapters {
            if let Err(e) = state_clone.start_download(chapter_id).await {
                tracing::error!("Failed to queue download for chapter {}: {}", chapter_id, e);
            }
        }
    });

    Ok(())
}

#[server]
pub async fn delete_downloaded(id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    state
        .delete_downloaded(id)
        .await
        .map_err(to_server_err)
}

#[server]
pub async fn check_in_library(source_id: i64, manga_id: String) -> Result<Option<i64>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let exists: Option<i64> = sqlx::query_scalar!(
        "SELECT id FROM manga WHERE source_manga_id = ? AND source_id = ?",
        manga_id,
        source_id
    )
    .fetch_optional(&state.db)
    .await
    .map_err(to_server_err)?;

    Ok(exists)
}

#[server]
pub async fn get_local_manga(id: i64) -> Result<(MangaInfo, Source), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let manga = sqlx::query_as!(crate::models::Manga, "SELECT * FROM manga WHERE id = ?", id)
        .fetch_optional(&state.db)
        .await
        .map_err(to_server_err)?
        .ok_or_else(|| ServerFnError::new("Manga not found"))?;

    let source = sqlx::query_as!(Source, "SELECT id, name, version, base_url FROM sources WHERE id = ?", manga.source_id)
        .fetch_optional(&state.db)
        .await
        .map_err(to_server_err)?
        .ok_or_else(|| ServerFnError::new("Source not found"))?;

    let authors = sqlx::query_scalar!("SELECT p.name FROM people p JOIN manga_authors ma ON p.id = ma.person_id WHERE ma.manga_id = ?", id)
        .fetch_all(&state.db).await.unwrap_or_default();
    let artists = sqlx::query_scalar!("SELECT p.name FROM people p JOIN manga_artists ma ON p.id = ma.person_id WHERE ma.manga_id = ?", id)
        .fetch_all(&state.db).await.unwrap_or_default();
    let tags = sqlx::query_scalar!("SELECT t.name FROM tags t JOIN manga_tags mt ON t.id = mt.tag_id WHERE mt.manga_id = ?", id)
        .fetch_all(&state.db).await.unwrap_or_default();
    
    let info = MangaInfo {
        id: manga.source_manga_id,
        title: manga.name,
        cover_url: manga.cover_url,
        description: manga.description,
        status: crate::types::MangaStatus::from(i64::from(manga.status)),
        authors,
        artists,
        tags,
    };

    Ok((info, source))
}

#[server]
pub async fn get_local_chapter_list(manga_id: i64, page: i32) -> Result<ChapterList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let limit = 65;
    let offset = ((page - 1).max(0) * limit) as i64;

    let chapters_db = sqlx::query!(
        r#"SELECT id, source_chapter_id, name, chapter_number, language, volume, scanlator, uploaded_at as "uploaded_at: i64", download_status
         FROM chapters WHERE manga_id = ?
         ORDER BY chapter_number DESC
         LIMIT ? OFFSET ?"#,
        manga_id, limit, offset
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_server_err)?;

    let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM chapters WHERE manga_id = ?", manga_id)
        .fetch_one(&state.db)
        .await
        .map_err(to_server_err)?;

    let chapters = chapters_db.into_iter().map(|c| crate::types::Chapter {
        id: c.id.to_string(), 
        title: c.name,
        number: c.chapter_number,
        language: c.language,
        volume: c.volume,
        scanlator: c.scanlator,
        date_uploaded: c.uploaded_at,
        download_status: c.download_status,
    }).collect();

    Ok(ChapterList {
        chapters,
        has_next_page: offset + (limit as i64) < total,
    })
}

#[server]
pub async fn delete_manga(id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    let manga = sqlx::query!("SELECT name FROM manga WHERE id = ?", id)
        .fetch_optional(&state.db)
        .await
        .map_err(to_server_err)?
        .ok_or_else(|| ServerFnError::new("Manga not found"))?;

    let library_path = state.settings.read().await.library_path.clone();
    let safe_manga_name = kani_core::sanitize::sanitize_filename(&manga.name);
    let path = library_path.join(safe_manga_name);

    if path.exists()
        && let Err(e) = tokio::fs::remove_dir_all(&path).await {
            tracing::error!("Failed to remove manga directory {:?}: {}", path, e);
        }

    sqlx::query!("DELETE FROM manga WHERE id = ?", id)
        .execute(&state.db)
        .await
        .map_err(to_server_err)?;
    Ok(())
}

#[server]
pub async fn get_library(page: i32) -> Result<Vec<(crate::types::MangaListItem, String)>, ServerFnError> {
    /*let order = match order {
        1 => "id DESC",
        _ => "id ASC",
    };*/
    
    let state = expect_context::<crate::state::AppState>();
    let offset = (page - 1).max(0) * 20;

    let records = sqlx::query!(
        r#"SELECT m.id, m.name, m.cover_url, s.base_url 
           FROM manga m 
           JOIN sources s ON m.source_id = s.id 
           ORDER BY m.id DESC LIMIT 20 OFFSET ?"#,
        offset
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_server_err)?;

    let library = records.into_iter().map(|r| {
        let item = crate::types::MangaListItem {
            id: r.id.to_string(),
            title: r.name,
            cover_url: r.cover_url,
        };
        (item, r.base_url)
    }).collect();

    Ok(library)
}

pub fn proxy_url(url: &str, referer: &str) -> String {
    format!(
        "/api/image_proxy?url={}&referer={}",
        urlencoding::encode(url),
        referer
    )
}
