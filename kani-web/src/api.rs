use gloo_net::http::Request;
use kani_shared::{ChapterList, MangaInfo, MangaList};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum ApiError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Server error: {0}")]
    Server(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub base_url: String,
}

pub async fn fetch_sources() -> Result<Vec<Source>, ApiError> {
    Request::get("/sources")
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| ApiError::Parse(e.to_string()))
}

pub async fn get_popular_manga(source_id: i64, page: i32) -> Result<MangaList, ApiError> {
    Request::get(&format!("/sources/{}/popular/{}", source_id, page))
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| ApiError::Parse(e.to_string()))
}

pub async fn search_manga(source_id: i64, query: &str, page: i32) -> Result<MangaList, ApiError> {
    Request::get(&format!("/sources/{}/search/{}", source_id, page))
        .query([("query", query)])
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| ApiError::Parse(e.to_string()))
}

pub async fn get_manga_details(source_id: i64, manga_id: &str) -> Result<MangaInfo, ApiError> {
    Request::get(&format!("/sources/{}/details/{}", source_id, manga_id))
        .send()
        .await
        .map_err(|e| ApiError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| ApiError::Parse(e.to_string()))
}

pub async fn get_chapter_list(
    source_id: i64,
    manga_id: &str,
    page: i32,
) -> Result<ChapterList, ApiError> {
    Request::get(&format!(
        "/sources/{}/chapters/{}/{}",
        source_id, manga_id, page
    ))
    .send()
    .await
    .map_err(|e| ApiError::Network(e.to_string()))?
    .json()
    .await
    .map_err(|e| ApiError::Parse(e.to_string()))
}

pub fn proxy_url(url: &str, referer: &str) -> String {
    format!(
        "/api/image_proxy?url={}&referer={}",
        urlencoding::encode(url),
        referer
    )
}
