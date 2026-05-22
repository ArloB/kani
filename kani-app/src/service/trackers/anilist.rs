use super::{ExternalTracker, TokenResponse, TrackerMangaResult, TrackerMangaStatus};
use crate::error::{Result, ServiceError};
use kani_shared::types::MangaTrackingStatus;
use serde::Deserialize;

const AUTH_URL: &str = "https://anilist.co/api/v2/oauth/authorize";
const TOKEN_URL: &str = "https://anilist.co/api/v2/oauth/token";
const GRAPHQL_URL: &str = "https://graphql.anilist.co";

pub struct AnilistTracker {
    client_id: String,
    client_secret: String,
    http: rquest::Client,
}

impl AnilistTracker {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            http: rquest::Client::new(),
        }
    }

    fn map_status_to_anilist(status: MangaTrackingStatus) -> &'static str {
        match status {
            MangaTrackingStatus::Reading => "CURRENT",
            MangaTrackingStatus::OnHold => "PAUSED",
            MangaTrackingStatus::Dropped => "DROPPED",
            MangaTrackingStatus::PlanToRead => "PLANNING",
            MangaTrackingStatus::Completed => "COMPLETED",
            MangaTrackingStatus::Rereading => "REPEATING",
        }
    }

    fn map_status_from_anilist(status: &str) -> Option<MangaTrackingStatus> {
        match status {
            "CURRENT" => Some(MangaTrackingStatus::Reading),
            "PAUSED" => Some(MangaTrackingStatus::OnHold),
            "DROPPED" => Some(MangaTrackingStatus::Dropped),
            "PLANNING" => Some(MangaTrackingStatus::PlanToRead),
            "COMPLETED" => Some(MangaTrackingStatus::Completed),
            "REPEATING" => Some(MangaTrackingStatus::Rereading),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct AnilistTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct GraphqlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphqlError>>,
}

#[derive(Deserialize)]
struct GraphqlError {
    message: String,
}

#[derive(Deserialize)]
struct SearchData {
    #[serde(rename = "Page")]
    page: SearchPage,
}

#[derive(Deserialize)]
struct SearchPage {
    media: Vec<SearchMedia>,
}

#[derive(Deserialize)]
struct SearchMedia {
    id: i64,
    title: MediaTitle,
    #[serde(rename = "coverImage")]
    cover_image: Option<CoverImage>,
}

#[derive(Deserialize)]
struct MediaTitle {
    #[serde(rename = "userPreferred")]
    user_preferred: Option<String>,
    romaji: Option<String>,
}

#[derive(Deserialize)]
struct CoverImage {
    large: Option<String>,
}

#[derive(Deserialize)]
struct MediaListData {
    #[serde(rename = "MediaList")]
    media_list: Option<MediaListEntry>,
}

#[derive(Deserialize)]
struct MediaListEntry {
    status: Option<String>,
    score: Option<f64>,
    progress: Option<i64>,
}

#[async_trait::async_trait]
impl ExternalTracker for AnilistTracker {
    fn name(&self) -> &'static str {
        "AniList"
    }

    fn auth_url(&self, redirect_uri: &str, state: &str, _code_challenge: Option<&str>) -> String {
        format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&state={}",
            AUTH_URL,
            urlencoding::encode(&self.client_id),
            urlencoding::encode(redirect_uri),
            urlencoding::encode(state),
        )
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        _code_verifier: Option<&str>,
    ) -> Result<TokenResponse> {
        let resp: AnilistTokenResponse = self
            .http
            .post(TOKEN_URL)
            .json(&serde_json::json!({
                "grant_type": "authorization_code",
                "client_id": self.client_id,
                "client_secret": self.client_secret,
                "redirect_uri": redirect_uri,
                "code": code,
            }))
            .send()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList token exchange failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList token parse failed: {e}")))?;

        let expires_at = resp
            .expires_in
            .map(|s| time::OffsetDateTime::now_utc() + time::Duration::seconds(s));

        Ok(TokenResponse {
            access_token: resp.access_token,
            refresh_token: resp.refresh_token,
            expires_at,
        })
    }

    async fn refresh_token(&self, _refresh_token: &str) -> Result<TokenResponse> {
        // AniList tokens are long-lived (1 year) and don't support refresh.
        // If the token is truly expired, the user must re-authenticate.
        Err(ServiceError::Internal(
            "AniList does not support token refresh — please re-link your account".into(),
        ))
    }

    async fn search_manga(
        &self,
        access_token: &str,
        query: &str,
    ) -> Result<Vec<TrackerMangaResult>> {
        let graphql = r#"
            query ($search: String) {
                Page(perPage: 10) {
                    media(search: $search, type: MANGA) {
                        id
                        title { userPreferred romaji }
                        coverImage { large }
                    }
                }
            }
        "#;

        let resp: GraphqlResponse<SearchData> = self
            .http
            .post(GRAPHQL_URL)
            .bearer_auth(access_token)
            .json(&serde_json::json!({
                "query": graphql,
                "variables": { "search": query }
            }))
            .send()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList search failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList search parse failed: {e}")))?;

        if let Some(errors) = resp.errors
            && let Some(first) = errors.first()
        {
            return Err(ServiceError::Internal(format!(
                "AniList API error: {}",
                first.message
            )));
        }

        let results = resp
            .data
            .map(|d| {
                d.page
                    .media
                    .into_iter()
                    .map(|m| TrackerMangaResult {
                        tracker_manga_id: m.id.to_string(),
                        title: m
                            .title
                            .user_preferred
                            .or(m.title.romaji)
                            .unwrap_or_default(),
                        cover_url: m.cover_image.and_then(|c| c.large),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(results)
    }

    async fn update_status(
        &self,
        access_token: &str,
        tracker_manga_id: &str,
        status: MangaTrackingStatus,
        score: Option<f64>,
        chapters_read: i64,
    ) -> Result<()> {
        let media_id: i64 = tracker_manga_id
            .parse()
            .map_err(|_| ServiceError::Internal("Invalid AniList media ID".into()))?;

        let graphql = r#"
            mutation ($mediaId: Int, $status: MediaListStatus, $score: Float, $progress: Int) {
                SaveMediaListEntry(mediaId: $mediaId, status: $status, score: $score, progress: $progress) {
                    id
                }
            }
        "#;

        let mut variables = serde_json::json!({
            "mediaId": media_id,
            "status": Self::map_status_to_anilist(status),
            "progress": chapters_read,
        });
        if let Some(s) = score {
            variables["score"] = serde_json::json!(s);
        }

        let resp: GraphqlResponse<serde_json::Value> = self
            .http
            .post(GRAPHQL_URL)
            .bearer_auth(access_token)
            .json(&serde_json::json!({
                "query": graphql,
                "variables": variables,
            }))
            .send()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList update failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList update parse failed: {e}")))?;

        if let Some(errors) = resp.errors
            && let Some(first) = errors.first()
        {
            return Err(ServiceError::Internal(format!(
                "AniList API error: {}",
                first.message
            )));
        }

        Ok(())
    }

    async fn get_status(
        &self,
        access_token: &str,
        tracker_manga_id: &str,
    ) -> Result<TrackerMangaStatus> {
        let media_id: i64 = tracker_manga_id
            .parse()
            .map_err(|_| ServiceError::Internal("Invalid AniList media ID".into()))?;

        let graphql = r#"
            query ($mediaId: Int) {
                MediaList(mediaId: $mediaId, type: MANGA) {
                    status
                    score(format: POINT_10_DECIMAL)
                    progress
                }
            }
        "#;

        let resp: GraphqlResponse<MediaListData> = self
            .http
            .post(GRAPHQL_URL)
            .bearer_auth(access_token)
            .json(&serde_json::json!({
                "query": graphql,
                "variables": { "mediaId": media_id }
            }))
            .send()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList get_status failed: {e}")))?
            .json()
            .await
            .map_err(|e| ServiceError::Internal(format!("AniList get_status parse failed: {e}")))?;

        match resp.data.and_then(|d| d.media_list) {
            Some(entry) => Ok(TrackerMangaStatus {
                status: entry
                    .status
                    .as_deref()
                    .and_then(Self::map_status_from_anilist),
                score: entry.score,
                chapters_read: entry.progress.unwrap_or(0),
            }),
            None => Ok(TrackerMangaStatus {
                status: None,
                score: None,
                chapters_read: 0,
            }),
        }
    }
}
