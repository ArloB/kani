use crate::types::{ChapterList, GlobalSearchResult, LibraryPage, MangaInfo, MangaList, MangaSortOrder, Source};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
fn to_server_err(e: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::new(e.to_string())
}

#[server]
pub async fn fetch_sources() -> Result<Vec<Source>, ServerFnError> {
    use crate::state::AppState;
    let state = expect_context::<AppState>();
    sqlx::query_as!(Source, "SELECT * FROM sources LIMIT 1000")
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
    let state = expect_context::<crate::state::AppState>();
    state.save_to_library(source_id, &manga_id).await.map_err(to_server_err)
}

#[server]
pub async fn download_chapter(chapter_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    state
        .download_chapter(chapter_id)
        .await
        .map_err(to_server_err)
}

#[server]
pub async fn cancel_download(chapter_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    let was_cancelled = state.downloader.cancel_download(chapter_id).await;

    if was_cancelled {
        sqlx::query!(
            "UPDATE chapters SET download_status = 0 WHERE id = ? AND download_status = 1",
            chapter_id
        )
        .execute(&state.db)
        .await
        .map_err(to_server_err)?;
    }

    Ok(())
}

#[server]
pub async fn download_all(manga_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = state_clone.download_all_chapters(manga_id).await {
            tracing::error!("Failed to queue all downloads for manga {}: {}", manga_id, e);
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
pub async fn get_local_manga(id: i64) -> Result<(MangaInfo, Source, bool, bool), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let manga = sqlx::query_as!(crate::models::Manga, "SELECT * FROM manga WHERE id = ?", id)
        .fetch_optional(&state.db)
        .await
        .map_err(to_server_err)?
        .ok_or_else(|| ServerFnError::new("Manga not found"))?;

    let source = sqlx::query_as!(Source, "SELECT * FROM sources WHERE id = ?", manga.source_id)
        .fetch_optional(&state.db)
        .await
        .map_err(to_server_err)?
        .ok_or_else(|| ServerFnError::new("Source not found"))?;

    let record = sqlx::query!(r#"SELECT (SELECT json_group_array(p.name)
                                    FROM manga_people mp JOIN people p ON mp.person_id = p.id
                                    WHERE mp.manga_id = m.id and role = 'author') as "authors!",
                                    (SELECT json_group_array(p.name)
                                    FROM manga_people mp JOIN people p ON mp.person_id = p.id
                                    WHERE mp.manga_id = m.id and role = 'artist') as "artists!",
                                    (SELECT json_group_array(t.name)
                                    FROM manga_tags mt JOIN tags t ON mt.tag_id = t.id
                                    WHERE mt.manga_id = m.id) as "tags!"
                                FROM manga m
                                where m.id = ?"#, id)
                    .fetch_optional(&state.db)
                    .await?
                    .ok_or_else(|| {
                        crate::error::AppError::NotFound(format!(
                            "Manga {id} not found"
                        ))
                    })?;
    
    let info = MangaInfo {
        id: manga.source_manga_id,
        title: manga.name,
        cover_url: manga.cover_url,
        description: manga.description,
        status: crate::types::MangaStatus::from(i64::from(manga.status)),
        authors: serde_json::from_str(&record.authors).unwrap_or_default(),
        artists: serde_json::from_str(&record.artists).unwrap_or_default(),
        tags: serde_json::from_str(&record.tags).unwrap_or_default(),
    };

    let auto_scan = state.settings.read().await.auto_scan;

    Ok((info, source, manga.auto_download, auto_scan))
}

#[server]
pub async fn get_local_chapter_list(manga_id: i64, page: i32, sort_order: crate::types::ChapterSortOrder) -> Result<ChapterList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let limit = 66;
    let offset = ((page - 1).max(0) * limit) as i64;

    let sql = format!(
        r#"SELECT id, source_chapter_id, name, chapter_number, language,
                volume, scanlator, uploaded_at, download_status
        FROM chapters
        WHERE manga_id = ?
        ORDER BY {}
        LIMIT ? OFFSET ?"#,
        sort_order.to_sql_order()
    );

    let mut chapters_db = sqlx::query_as::<sqlx::Sqlite, crate::models::Chapter>(&sql)
        .bind(manga_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(to_server_err)?;

    let has_next_page = chapters_db.len() == limit as usize;

    if has_next_page {
        chapters_db.pop();
    }

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
        has_next_page,
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
    let safe_manga_name_base = kani_core::utilities::sanitize_filename(&manga.name);
    let safe_manga_name = format!("{} - {}", safe_manga_name_base, id);
    let path = library_path.join(safe_manga_name);

    match tokio::fs::remove_dir_all(&path).await {
        Ok(_) => {},
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(e) => return Err(to_server_err(format!("Failed to remove directory: {}", e))),
    }

    sqlx::query!("DELETE FROM manga WHERE id = ?", id)
        .execute(&state.db)
        .await
        .map_err(to_server_err)?;

    Ok(())
}

#[server]
pub async fn get_library(
    page: i32, 
    search: Option<String>,
    status_filter: Option<i64>,
    tag_filter: Option<i64>,
    author_filter: Option<i64>,
    artist_filter: Option<i64>,
    category_filter: Option<i64>,
    sort_by: MangaSortOrder
) -> Result<LibraryPage, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    const PAGE_SIZE: i32 = 20;

    let offset = (page - 1).max(0) * PAGE_SIZE;

    let mut qb = sqlx::QueryBuilder::new(
        "SELECT m.id, m.name, m.cover_url, s.base_url
         FROM manga m
         JOIN sources s ON m.source_id = s.id
         WHERE 1=1"
    );

    if let Some(search_str) = search {
        qb.push(" AND LOWER(m.name) LIKE '%' || LOWER(");
        qb.push_bind(search_str);
        qb.push(") || '%'");
    }

    if let Some(status_id) = status_filter {
        qb.push(" AND m.status = ");
        qb.push_bind(status_id);
    }

    if let Some(tag_id) = tag_filter {
        qb.push(" AND EXISTS (SELECT 1 FROM manga_tags mt WHERE mt.manga_id = m.id AND mt.tag_id = ");
        qb.push_bind(tag_id);
        qb.push(")");
    }

    if let Some(author_id) = author_filter {
        qb.push(" AND EXISTS (SELECT 1 FROM manga_people mp WHERE mp.manga_id = m.id AND mp.role = 'author' AND mp.person_id = ");
        qb.push_bind(author_id);
        qb.push(")");
    }

    if let Some(artist_id) = artist_filter {
        qb.push(" AND EXISTS (SELECT 1 FROM manga_people mp WHERE mp.manga_id = m.id AND mp.role = 'artist' AND mp.person_id = ");
        qb.push_bind(artist_id);
        qb.push(")");
    }

    if let Some(cat_str) = category_filter {
        qb.push(" AND EXISTS (SELECT 1 FROM manga_categories mc WHERE mc.manga_id = m.id AND mc.category_id = ");
        qb.push_bind(cat_str);
        qb.push(")");
    }

    qb.push(format!(" ORDER BY {} LIMIT {} OFFSET ", sort_by.to_sql_order(), PAGE_SIZE + 1));
    qb.push_bind(offset);

    let mut records = qb
        .build_query_as::<crate::models::LibraryRow>()
        .fetch_all(&state.db)
        .await
        .map_err(to_server_err)?;

    let has_next_page = records.len() == PAGE_SIZE as usize + 1;
    records.truncate(PAGE_SIZE as usize);

    let items = records.into_iter().map(|r| {
        let item = crate::types::MangaListItem {
            id: r.id.to_string(),
            title: r.name,
            cover_url: r.cover_url,
        };
        (item, r.base_url)
    }).collect();

    Ok(LibraryPage {items, has_next_page})
}

#[cfg(feature = "ssr")]
async fn fetch_filter_options(
    db: &sqlx::SqlitePool,
    sql: &str,
) -> Result<Vec<(i64, String)>, ServerFnError> {
    use crate::models::FilterOptionResult;

    let options = sqlx::query_as::<_, FilterOptionResult>(sql)
        .fetch_all(db)
        .await
        .map_err(to_server_err)?
        .into_iter()
        .map(|r| (r.id, r.name))
        .collect();
    Ok(options)
}

#[server]
pub async fn get_all_tags() -> Result<Vec<(i64, String)>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    fetch_filter_options(&state.db, "SELECT id, name FROM tags ORDER BY name").await
}

#[server]
pub async fn get_all_authors() -> Result<Vec<(i64, String)>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    fetch_filter_options(&state.db,
        "SELECT p.id, p.name FROM people p
         JOIN manga_people mp ON mp.person_id = p.id
         WHERE mp.role = 'author'
         ORDER BY p.name"
    ).await
}

#[server]
pub async fn get_all_artists() -> Result<Vec<(i64, String)>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    fetch_filter_options(&state.db,
        "SELECT p.id, p.name FROM people p
         JOIN manga_people mp ON mp.person_id = p.id
         WHERE mp.role = 'artist'
         ORDER BY p.name"
    ).await
}

#[server]
pub async fn get_all_categories() -> Result<Vec<(i64, String)>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    fetch_filter_options(&state.db, "SELECT id, name FROM categories ORDER BY sort_order").await
}

#[server]
pub async fn refresh_manga(id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    state.refresh_manga(id).await.map_err(to_server_err)
}

#[server]
pub async fn scan_for_new_chapters(id: i64) -> Result<i64, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let new_chapters = state.scan_for_new_chapters(id).await.map_err(to_server_err)?;
    Ok(new_chapters.len() as i64)
}

#[server]
pub async fn toggle_auto_scan() -> Result<bool, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    
    let new_state = !state.settings.read().await.auto_scan;
    
    sqlx::query!(
        "UPDATE settings SET auto_scan = ? WHERE id = 'singleton'",
        new_state
    )
    .execute(&state.db)
    .await
    .map_err(to_server_err)?;

    state.settings.write().await.auto_scan = new_state;

    Ok(new_state)
}

#[server]
pub async fn toggle_auto_download(manga_db_id: i64, enabled: bool) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query!(
        "UPDATE manga SET auto_download = ? WHERE id = ?",
        enabled, manga_db_id
    )
    .execute(&state.db)
    .await
    .map_err(to_server_err)?;
    Ok(())
}

#[server]
pub async fn global_search(
    query: String,
    scope: crate::types::SearchScope,
    page: i32,
) -> Result<Vec<GlobalSearchResult>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    state
        .global_search(&query, scope, page)
        .await
        .map_err(to_server_err)
}

#[server]
pub async fn toggle_source_favourite(
    source_id: i64,
    favourited: bool,
) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query!(
        "UPDATE sources SET favourited = ? WHERE id = ?",
        favourited,
        source_id
    )
    .execute(&state.db)
    .await
    .map(|_| ())
    .map_err(to_server_err)
}

pub fn proxy_url(url: &str, referer: &str) -> String {
    format!(
        "/rest/image_proxy?url={}&referer={}",
        urlencoding::encode(url),
        referer
    )
}
