//! Self-service API token management (`/rest/me/api-tokens`).

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/api-tokens", get(list_tokens).post(create_token))
        .route("/me/api-tokens/{id}", axum::routing::delete(revoke_token))
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct TokenResponse {
    id: String,
    name: String,
    scopes: String,
    created_at: i64,
    last_used_at: Option<i64>,
    expires_at: Option<i64>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub(crate) struct CreatedTokenResponse {
    id: String,
    name: String,
    scopes: String,
    created_at: i64,
    last_used_at: Option<i64>,
    expires_at: Option<i64>,
    /// The raw token. Shown exactly once, at creation.
    raw_token: String,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub(crate) struct CreateTokenBody {
    name: String,
    expires_in_days: Option<u32>,
}

fn to_response(t: kani_app::service::api_tokens::ApiToken) -> TokenResponse {
    TokenResponse {
        id: t.id,
        name: t.name,
        scopes: t.scopes,
        created_at: t.created_at,
        last_used_at: t.last_used_at,
        expires_at: t.expires_at,
    }
}

#[utoipa::path(
    get, path = "/rest/me/api-tokens",
    responses(
        (status = 200, description = "The caller's active API tokens"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "api-tokens"
)]
pub(crate) async fn list_tokens(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::Authenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let tokens = state.list_api_tokens(user.id).await?;
    let resp: Vec<TokenResponse> = tokens.into_iter().map(to_response).collect();
    Ok(Json(resp))
}

#[utoipa::path(
    post, path = "/rest/me/api-tokens",
    request_body = CreateTokenBody,
    responses(
        (status = 201, description = "Token created; raw_token returned once"),
        (status = 400, description = "Invalid token name"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "api-tokens"
)]
pub(crate) async fn create_token(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::Authenticated>,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenBody>,
) -> Result<impl IntoResponse, AppError> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::ValidationError(
            "Token name must be 1–100 characters".into(),
        ));
    }
    let created = state
        .create_api_token(user.id, name, body.expires_in_days)
        .await?;
    state
        .audit(
            Some(user.id),
            "api_token.created",
            Some(name),
            Some(json!({ "token_id": created.token.id })),
        )
        .await;
    let resp = CreatedTokenResponse {
        id: created.token.id,
        name: created.token.name,
        scopes: created.token.scopes,
        created_at: created.token.created_at,
        last_used_at: created.token.last_used_at,
        expires_at: created.token.expires_at,
        raw_token: created.raw_token,
    };
    Ok((StatusCode::CREATED, Json(resp)))
}

#[utoipa::path(
    delete, path = "/rest/me/api-tokens/{id}",
    params(("id" = String, Path, description = "API token ID")),
    responses(
        (status = 204, description = "Token revoked"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Token not found"),
    ),
    security(("session" = [])),
    tag = "api-tokens"
)]
pub(crate) async fn revoke_token(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::Authenticated>,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.revoke_api_token(user.id, &id).await?;
    state
        .audit(Some(user.id), "api_token.revoked", Some(&id), None)
        .await;
    Ok(StatusCode::NO_CONTENT)
}
