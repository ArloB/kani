use crate::types::{
    AppSettings, Category, ChapterList, DownloadRule, DownloadRuleKind, GlobalSearchResult, LibraryPage, MangaInfo, 
    MangaList, MangaSortOrder, RecentUpdate, RecentUpdateItem, Source
};
use leptos::prelude::*;

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

#[cfg(feature = "ssr")]
async fn get_source_base_url(db: &sqlx::SqlitePool, source_id: i64) -> Result<String, ServerFnError> {
    let base_url = {
        sqlx::query_scalar!("SELECT base_url FROM sources WHERE id = ?", source_id)
            .fetch_optional(db)
            .await
            .map_err(to_server_err)?
            .unwrap_or_default()
    };
    Ok(base_url)
}

#[server]
pub async fn get_popular_manga(source_id: i64, page: i32) -> Result<MangaList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let base_url = get_source_base_url(&state.db, source_id).await?;

    let json = state
        .get_popular_manga(source_id, page)
        .await
        .map_err(to_server_err)?;
    let mut list: MangaList = serde_json::from_str(&json).map_err(to_server_err)?;

    for item in &mut list.manga {
        if let Some(ref url) = item.cover_url.clone() {
            item.cover_url = Some(sign_image_url(url, &base_url, &state));
        }
    }

    Ok(list)
}

#[server]
pub async fn search_manga(
    source_id: i64,
    query: String,
    page: i32,
) -> Result<MangaList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let base_url = get_source_base_url(&state.db, source_id).await?;

    let json = state
        .search_manga(source_id, &query, page)
        .await
        .map_err(to_server_err)?;
    let mut list: MangaList = serde_json::from_str(&json).map_err(to_server_err)?;

    for item in &mut list.manga {
        if let Some(ref url) = item.cover_url.clone() {
            item.cover_url = Some(sign_image_url(url, &base_url, &state));
        }
    }

    Ok(list)
}

#[server]
pub async fn get_manga_details(
    source_id: i64,
    manga_id: String,
) -> Result<MangaInfo, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let base_url = get_source_base_url(&state.db, source_id).await?;

    let json = state
        .get_manga_details(source_id, &manga_id)
        .await
        .map_err(to_server_err)?;
    let mut info: MangaInfo = serde_json::from_str(&json).map_err(to_server_err)?;

    info.cover_url = info.cover_url.map(|url| sign_image_url(&url, &base_url, &state));
    info.description_html = info.description.as_deref().map(crate::markdown::render_description);

    Ok(info)
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
pub async fn get_pages(
    source_id: i64,
    manga_id: String,
    chapter_id: String,
) -> Result<crate::types::ChapterContents, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let base_url = {
        sqlx::query_scalar!("SELECT base_url FROM sources WHERE id = ?", source_id)
            .fetch_optional(&state.db)
            .await
            .map_err(to_server_err)?
            .unwrap_or_default()
    };

    let json = state
        .get_pages(source_id, &manga_id, &chapter_id)
        .await
        .map_err(to_server_err)?;
    let mut chapter: crate::types::ChapterContents = serde_json::from_str(&json).map_err(to_server_err)?;

    for page in &mut chapter.pages {
        page.url = sign_image_url(&page.url, &base_url, &state);
    }
    
    Ok(chapter)
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

    let description_html = manga.description.as_ref().map(|d| crate::markdown::render_description(d));
    
    let cover_url = if manga.local_cover_path.is_some() {
        Some(format!("/rest/manga/{}/cover", id))
    } else {
        manga.cover_url.map(|url| sign_image_url(&url, &source.base_url, &state))
    };

    let info = MangaInfo {
        id: manga.source_manga_id,
        title: manga.name,
        cover_url,
        description: manga.description,
        description_html,
        status: crate::types::MangaStatus::from(i64::from(manga.status)),
        authors: serde_json::from_str(&record.authors).unwrap_or_default(),
        artists: serde_json::from_str(&record.artists).unwrap_or_default(),
        tags: serde_json::from_str(&record.tags).unwrap_or_default(),
    };

    let auto_scan = state.settings.read().await.auto_scan;

    Ok((info, source, manga.auto_download, auto_scan))
}

#[server]
pub async fn get_local_chapter_list(
    manga_id: i64,
    page: i32,
    sort_order: crate::types::ChapterSortOrder,
) -> Result<ChapterList, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let limit = 66i64;
    let offset = ((page - 1).max(0) as i64) * limit;

    let sql = format!(
        r#"SELECT c.id, c.source_chapter_id, c.name, c.chapter_number, c.language,
                  c.volume, c.scanlator, c.uploaded_at, c.download_status
           FROM chapters c
           LEFT JOIN scanlator_preferences sp
               ON sp.manga_id = c.manga_id
               AND sp.scanlator = c.scanlator
               AND sp.manga_id = ?
           WHERE c.manga_id = ?
           ORDER BY {}, COALESCE(sp.priority, -1) DESC
           LIMIT ? OFFSET ?"#,
        sort_order.to_sql_order()
    );

    let mut chapters_db = sqlx::query_as::<sqlx::Sqlite, crate::models::Chapter>(&sql)
        .bind(manga_id)
        .bind(manga_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(to_server_err)?;

    let has_next_page = chapters_db.len() == limit as usize;
    if has_next_page { chapters_db.pop(); }

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

    Ok(ChapterList { chapters, has_next_page })
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
        "SELECT m.id, m.name, m.cover_url, m.local_cover_path, s.base_url
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
        let cover_url = if r.local_cover_path.is_some() {
            Some(format!("/rest/manga/{}/cover", r.id))
        } else {
            r.cover_url.map(|url| sign_image_url(&url, &r.base_url, &state))
        };

        crate::types::MangaListItem {
            id: r.id.to_string(),
            title: r.name,
            cover_url,
        }
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

    let mut list = state
        .global_search(&query, scope, page)
        .await
        .map_err(to_server_err)?;

    let source_ids: Vec<i64> = list
        .iter()
        .map(|item| item.source_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let source_ids_json = serde_json::to_string(&source_ids).unwrap_or_default();

    let base_urls: std::collections::HashMap<i64, String> = sqlx::query!(
        "SELECT id, base_url FROM sources WHERE id IN (SELECT value FROM json_each(?))",
        source_ids_json
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_server_err)?
    .into_iter()
    .map(|r| (r.id, r.base_url))
    .collect();

    for result in &mut list {
        let referer = base_urls
            .get(&result.source_id)
            .map(String::as_str)
            .unwrap_or("");
        
        for item in &mut result.manga {
            if let Some(ref url) = item.cover_url.clone() {
                item.cover_url = Some(sign_image_url(url, referer, &state));
            }
        }
    }

    Ok(list)
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

#[server]
pub async fn start_refresh_all() -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    state.start_refresh_all().await.map_err(to_server_err)
}

#[server]
pub async fn is_refreshing() -> Result<bool, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    Ok(state.is_refreshing().await)
}

#[server]
pub async fn get_download_rules(
    manga_db_id: i64,
) -> Result<Vec<DownloadRule>, ServerFnError> {
    use crate::models::DownloadRuleRow;

    let state = expect_context::<crate::state::AppState>();

    sqlx::query_as!(
        DownloadRuleRow,
        "SELECT id, manga_id, rule_type, value
         FROM download_rules
         WHERE manga_id = ?
         ORDER BY id ASC",
        manga_db_id
    )
        .fetch_all(&state.db)
        .await
        .map_err(to_server_err)
        .map(|rows| rows
            .into_iter()
            .filter_map(|row| DownloadRule::try_from(row).ok())
            .collect())
}

#[server]
pub async fn add_download_rule(
    manga_db_id: i64,
    kind: DownloadRuleKind,
) -> Result<i64, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    let (rule_type, value) = match &kind {
        DownloadRuleKind::ScanlatorInclude(v) => ("scanlator_include", v),
        DownloadRuleKind::ScanlatorExclude(v) => ("scanlator_exclude", v),
        DownloadRuleKind::LanguageInclude(v)  => ("language_include",  v),
        DownloadRuleKind::LanguageExclude(v)  => ("language_exclude",  v),
        DownloadRuleKind::TitleContains(v)    => ("title_contains",    v),
        DownloadRuleKind::TitleExcludes(v)    => ("title_excludes",    v),
    };

    if value.trim().is_empty() {
        return Err(ServerFnError::new("Rule value cannot be empty"));
    }

    sqlx::query_scalar!(
        "INSERT INTO download_rules (manga_id, rule_type, value)
         VALUES (?, ?, ?)
         RETURNING id",
        manga_db_id, rule_type, value
    )
    .fetch_one(&state.db)
    .await
    .map_err(to_server_err)
}

#[server]
pub async fn remove_download_rule(
    rule_id: i64,
) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    sqlx::query!("DELETE FROM download_rules WHERE id = ?", rule_id)
        .execute(&state.db)
        .await
        .map_err(to_server_err)?;

    Ok(())
}

#[server]
pub async fn get_scanlator_preferences(
    manga_db_id: i64,
) -> Result<Vec<crate::types::ScanlatorPreference>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query_as!(
        crate::types::ScanlatorPreference,
        "SELECT id, manga_id, scanlator, priority
         FROM scanlator_preferences
         WHERE manga_id = ?
         ORDER BY priority DESC",
        manga_db_id
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_server_err)
}

#[server]
pub async fn set_scanlator_preference(
    manga_db_id: i64,
    scanlator: String,
    priority: i64,
) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query!(
        "INSERT INTO scanlator_preferences (manga_id, scanlator, priority)
         VALUES (?, ?, ?)
         ON CONFLICT (manga_id, scanlator) DO UPDATE SET priority = excluded.priority",
        manga_db_id,
        scanlator,
        priority
    )
    .execute(&state.db)
    .await
    .map_err(to_server_err)?;
    Ok(())
}

#[server]
pub async fn remove_scanlator_preference(pref_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query!("DELETE FROM scanlator_preferences WHERE id = ?", pref_id)
        .execute(&state.db)
        .await
        .map_err(to_server_err)?;
    Ok(())
}

#[server]
pub async fn get_categories() -> Result<Vec<Category>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query_as!(
        Category,
        "SELECT id, name, sort_order FROM categories ORDER BY sort_order ASC, name ASC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_server_err)
}

#[server]
pub async fn create_category(name: String, sort_order: i64) -> Result<i64, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("Category name cannot be empty"));
    }
    sqlx::query_scalar!(
        "INSERT INTO categories (name, sort_order) VALUES (?, ?) RETURNING id",
        name, sort_order
    )
    .fetch_one(&state.db)
    .await
    .map_err(to_server_err)
}

#[server]
pub async fn rename_category(category_id: i64, name: String) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ServerFnError::new("Category name cannot be empty"));
    }
    sqlx::query!(
        "UPDATE categories SET name = ? WHERE id = ?",
        name, category_id
    )
    .execute(&state.db)
    .await
    .map_err(to_server_err)
    .map(|_| ())
}

#[server]
pub async fn delete_category(category_id: i64) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query!("DELETE FROM categories WHERE id = ?", category_id)
        .execute(&state.db)
        .await
        .map_err(to_server_err)
        .map(|_| ())
}

#[server]
pub async fn reorder_categories(ordered_ids: Vec<i64>) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let mut tx = state.db.begin().await.map_err(to_server_err)?;
    for (idx, id) in ordered_ids.into_iter().enumerate() {
        let order = idx as i64;
        sqlx::query!("UPDATE categories SET sort_order = ? WHERE id = ?", order, id)
            .execute(&mut *tx)
            .await
            .map_err(to_server_err)?;
    }
    tx.commit().await.map_err(to_server_err)
}

#[server]
pub async fn get_manga_categories(manga_db_id: i64) -> Result<Vec<Category>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query_as!(
        Category,
        "SELECT c.id, c.name, c.sort_order
         FROM categories c
         JOIN manga_categories mc ON mc.category_id = c.id
         WHERE mc.manga_id = ?
         ORDER BY c.sort_order ASC, c.name ASC",
        manga_db_id
    )
    .fetch_all(&state.db)
    .await
    .map_err(to_server_err)
}

#[server]
pub async fn set_manga_categories(
    manga_db_id: i64,
    category_ids: Vec<i64>,
) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let mut tx = state.db.begin().await.map_err(to_server_err)?;

    sqlx::query!("DELETE FROM manga_categories WHERE manga_id = ?", manga_db_id)
        .execute(&mut *tx)
        .await
        .map_err(to_server_err)?;

    for cat_id in category_ids {
        sqlx::query!(
            "INSERT OR IGNORE INTO manga_categories (manga_id, category_id) VALUES (?, ?)",
            manga_db_id, cat_id
        )
        .execute(&mut *tx)
        .await
        .map_err(to_server_err)?;
    }

    tx.commit().await.map_err(to_server_err)
}

#[server]
pub async fn get_settings() -> Result<AppSettings, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    let s = state.settings.read().await;
    Ok(AppSettings {
        flaresolverr_url:           s.flaresolverr_url.clone(),
        library_path:               s.library_path.to_string_lossy().into_owned(),
        concurrent_page_downloads:  s.concurrent_page_downloads,
        concurrent_manga_downloads: s.concurrent_manga_downloads,
        chapter_queue_size:         s.chapter_queue_size,
        max_retries:                s.max_retries,
        initial_retry_delay_ms:     s.initial_retry_delay_ms,
        max_wasm_instances:         s.max_wasm_instances,
        auto_scan:                  s.auto_scan,
        scan_interval_minutes:      s.scan_interval_minutes,
    })
}

#[server]
pub async fn update_settings(settings: AppSettings) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    if settings.concurrent_page_downloads < 1 || settings.concurrent_page_downloads > 32 {
        return Err(ServerFnError::new("concurrent_page_downloads must be 1–32"));
    }
    if settings.concurrent_manga_downloads < 1 || settings.concurrent_manga_downloads > 16 {
        return Err(ServerFnError::new("concurrent_manga_downloads must be 1–16"));
    }
    if settings.scan_interval_minutes < 5 {
        return Err(ServerFnError::new("scan_interval_minutes must be at least 5"));
    }

    sqlx::query!(
        "UPDATE settings SET
            flaresolverr_url           = ?,
            library_path               = ?,
            concurrent_page_downloads  = ?,
            concurrent_manga_downloads = ?,
            chapter_queue_size         = ?,
            max_retries                = ?,
            initial_retry_delay_ms     = ?,
            max_wasm_instances         = ?,
            auto_scan                  = ?,
            scan_interval_minutes      = ?
         WHERE id = 'singleton'",
        settings.flaresolverr_url,
        settings.library_path,
        settings.concurrent_page_downloads,
        settings.concurrent_manga_downloads,
        settings.chapter_queue_size,
        settings.max_retries,
        settings.initial_retry_delay_ms,
        settings.max_wasm_instances,
        settings.auto_scan,
        settings.scan_interval_minutes,
    )
    .execute(&state.db)
    .await
    .map_err(to_server_err)?;

    {
        let mut s = state.settings.write().await;
        s.flaresolverr_url          = settings.flaresolverr_url;
        s.library_path              = settings.library_path.into();
        s.concurrent_page_downloads = settings.concurrent_page_downloads;
        s.concurrent_manga_downloads = settings.concurrent_manga_downloads;
        s.chapter_queue_size        = settings.chapter_queue_size;
        s.max_retries               = settings.max_retries;
        s.initial_retry_delay_ms    = settings.initial_retry_delay_ms;
        s.max_wasm_instances        = settings.max_wasm_instances;
        s.auto_scan                 = settings.auto_scan;
        s.scan_interval_minutes     = settings.scan_interval_minutes;
    }

    Ok(())
}

#[server]
pub async fn get_recent_updates(page: i32) -> Result<RecentUpdate, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();

    let offset = (page - 1) * 50;

    let raw_updates = sqlx::query_as!(RecentUpdateItem, 
        "SELECT m.id as manga_id, m.name as manga_name, m.cover_url, m.local_cover_path, s.base_url,
            c.id as chapter_id, c.chapter_number, c.name as chapter_name, c.discovered_at
        FROM chapters c
        JOIN manga m ON c.manga_id = m.id
        JOIN sources s ON m.source_id = s.id
        WHERE c.discovered_at IS NOT NULL
        ORDER BY c.discovered_at DESC
        LIMIT 51 OFFSET ?", 
    offset)
        .fetch_all(&state.db)
        .await
        .map_err(to_server_err)?;

    let mut recent_updates: Vec<RecentUpdateItem> = raw_updates
    .into_iter()
    .map(|mut u| {
        u.cover_url = if u.local_cover_path.is_some() {
            Some(format!("/rest/manga/{}/cover", u.manga_id))
        } else if let Some(ref url) = u.cover_url.clone() {
            Some(sign_image_url(url, &u.base_url, &state))
        } else {
            None
        };
        u
    })
    .collect();

    let has_next_page = recent_updates.len() > 50;
    if has_next_page { recent_updates.truncate(50); }
    Ok(RecentUpdate { recent_updates, has_next_page })
}

#[server]
pub async fn toggle_source_enabled(
    source_id: i64,
    enabled: bool,
) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    sqlx::query!(
        "UPDATE sources SET enabled = ? WHERE id = ?",
        enabled, source_id
    )
    .execute(&state.db)
    .await
    .map(|_| ())
    .map_err(to_server_err)
}

#[server]
pub async fn get_source(id: i64) -> Result<Source, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    state.get_source(id).await.map_err(to_server_err)
}

#[cfg(feature = "ssr")]
fn sign_image_url(url: &str, referer: &str, state: &crate::state::AppState) -> String {
    crate::proxy::make_proxy_url(url, referer, &state.proxy_secret)
}
