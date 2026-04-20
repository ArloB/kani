//! Plain Axum REST handlers — mounted at /rest in main.rs.

use crate::{
    auth::{AuthBackend, AuthSession, Credentials},
    error::AppError,
    models::{
        AddDownloadRuleRequest, ChangePasswordRequest, CreateCategoryRequest, CreateSource,
        FetchWasmRequest, GlobalSearchQuery, LibraryQuery, ListItemRequest, LocalChaptersQuery,
        LoginRequest, MigrateMangaRequest, PageQuery, PreviewMigrationRequest, ProxyQuery,
        RenameCategoryRequest, ReorderCategoriesRequest, SearchMangaRequest,
        SetChapterProgressRequest, SetMangaCategoriesRequest, SetMangaTrackingRequest,
        SetPreferenceRequest, SetReadStatusRequest, SetScanlatorPrefRequest,
        ContinueReadingShelfQuery, MarkUpToRequest,
        SetTrackerConfigRequest, SetTrackerMappingRequest, TrackerAuthUrlQuery,
        TrackerCallbackQuery, TrackerSearchQuery,
        ToggleAutoDownloadRequest, ToggleEnabledRequest, ToggleFavouritedRequest,
        ToggleSelectRequest, UpdateSource,
        AdminCreateUserRequest, AdminUpdateUserRequest, AdminGrantRoleRequest,
        AdminCreateRoleRequest, AdminUpdateRoleRequest,
    },
    permissions::AuthRequirement,
    state::AppState,
    types::Source,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, FromRef, Multipart, Path, Query, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{delete, get, patch, post, put},
};
use axum_login::AuthnBackend;
use axum_login::AuthzBackend;
use futures::TryStreamExt;
use kani_core::source_manager::SourceManager;
use serde::de::DeserializeOwned;
use serde_json::json;
use std::marker::PhantomData;
use std::{convert::Infallible, sync::Arc};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

fn sign_image_url(url: &str, referer: &str, state: &AppState) -> String {
    crate::proxy::make_proxy_url(url, referer, &state.proxy_secret)
}

struct ValidatedJson<T>(T);

impl<S, T> axum::extract::FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + garde::Validate<Context = ()>,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e| AppError::ValidationError(e.to_string()))?;
        value
            .validate()
            .map_err(|e| AppError::ValidationError(e.to_string()))?;
        Ok(ValidatedJson(value))
    }
}

struct ValidatedQuery<T>(T);

impl<S, T> axum::extract::FromRequestParts<S> for ValidatedQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + garde::Validate<Context = ()>,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|e| AppError::ValidationError(e.to_string()))?;
        value
            .validate()
            .map_err(|e| AppError::ValidationError(e.to_string()))?;
        Ok(ValidatedQuery(value))
    }
}

const MAX_WASM_BYTES: usize = 10 * 1024 * 1024;

pub struct AuthGuard<P: AuthRequirement>(pub crate::auth::User, pub PhantomData<P>);

impl<S, P> axum::extract::FromRequestParts<S> for AuthGuard<P>
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
    P: AuthRequirement,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let auth_session = crate::auth::AuthSession::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::InternalServerError("Session error".into()))?;

        let user = auth_session
            .user
            .ok_or(AppError::Unauthorized("User not authenticated".into()))?;

        if let Some(perm) = P::required_permission()
            && !auth_session
                .backend
                .has_perm(&user, perm)
                .await
                .unwrap_or(false)
        {
            tracing::warn!(
                user_id = user.id,
                username = %user.username,
                permission = %perm,
                "Permission denied"
            );
            let app_state = AppState::from_ref(state);
            app_state
                .audit(
                    Some(user.id),
                    "auth.permission_denied",
                    Some(&perm.to_string()),
                    Some(serde_json::json!({ "username": user.username })),
                )
                .await;
            return Err(AppError::Forbidden(format!(
                "User lacks permission: {}",
                perm
            )));
        }

        Ok(Self(user, PhantomData))
    }
}

pub fn routes(state: AppState) -> Router {
    Router::new()
        // ── Auth (public) ────────────────────────────────────────────────
        .route("/auth/login", post(auth_login))
        .route("/auth/logout", post(auth_logout))
        .route("/auth/me", get(auth_me))
        .route("/auth/current_user", get(get_current_user))
        .route("/auth/change_password", post(change_password))
        .route("/auth/logout_everywhere", post(logout_everywhere))
        .route("/auth/permissions", get(get_my_permissions))
        // ── Image proxy — mounted separately with its own rate limit ────
        // ── Server-sent events ───────────────────────────────────────────
        .route("/events", get(combined_sse))
        .route("/boot_id", get(get_boot_id))
        // ── Source admin CRUD ────────────────────────────────────────────
        .route("/sources", get(list_sources).post(add_source))
        .route("/sources/active_ids", get(get_active_source_ids))
        .route(
            "/sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/sources/{id}/metadata", get(get_metadata))
        // WASM installation
        .route("/sources/{id}/wasm", post(upload_wasm))
        .route("/sources/{id}/wasm/fetch", post(fetch_wasm))
        // Source browse/search
        .route(
            "/sources/{id}/popular/{page}/{page_size}",
            get(get_popular_manga),
        )
        .route("/sources/{id}/search/{page}/{page_size}", get(search_manga))
        .route("/sources/{id}/details/{manga_id}", get(get_manga_details))
        .route("/sources/{id}/save/{manga_id}", post(save_to_library))
        .route(
            "/sources/{id}/chapters/{manga_id}/{page}/{page_size}",
            get(get_chapter_list),
        )
        .route("/sources/{id}/chapter-sorts/{manga_id}", get(get_chapter_sort_list))
        .route(
            "/sources/{id}/pages/{manga_id}/{chapter_id}",
            get(get_pages),
        )
        .route("/sources/{id}/in_library/{manga_id}", get(check_in_library))
        .route("/sources/{id}/toggle_enabled", patch(toggle_source_enabled))
        .route(
            "/sources/{id}/toggle_favourite",
            patch(toggle_source_favourite),
        )
        // Source filters
        .route("/sources/{id}/filters", get(get_source_filters))
        // Source preferences
        .route("/sources/{id}/preference_schema", get(get_pref_schema))
        .route("/sources/{id}/preferences", get(get_source_preferences))
        .route(
            "/sources/{id}/preferences/{key}",
            put(set_source_preference),
        )
        .route(
            "/sources/{id}/preferences/{key}/append",
            post(append_pref_list_item),
        )
        .route(
            "/sources/{id}/preferences/{key}/remove_item",
            post(remove_pref_list_item),
        )
        .route(
            "/sources/{id}/preferences/{key}/toggle_select",
            post(toggle_pref_select_item),
        )
        // ── Library ──────────────────────────────────────────────────────
        .route("/library", get(get_library_filtered))
        .route("/library/continue_reading", get(get_continue_reading_shelf))
        .route("/library/{page}/{order}", get(get_library)) // legacy Leptos compat
        .route("/recent_updates", get(get_recent_updates))
        .route("/global_search", get(global_search_handler))
        // ── Manga ────────────────────────────────────────────────────────
        .route("/manga/{id}", get(get_manga).delete(delete_manga))
        .route("/manga/{id}/cover", get(serve_manga_cover))
        .route("/manga/{id}/details", get(get_local_manga_details))
        .route("/manga/{id}/chapters", get(get_local_chapters))
        .route("/manga/{id}/chapter_ids", get(get_chapter_ids))
        .route("/manga/{id}/download_all", post(download_all))
        .route("/manga/{id}/cancel_all", post(cancel_all_downloads))
        .route("/manga/{id}/refresh", post(refresh_manga))
        .route("/manga/{id}/scan", post(scan_manga))
        .route(
            "/manga/{id}/toggle_auto_download",
            post(toggle_auto_download),
        )
        .route(
            "/manga/{id}/toggle_download_all_preferred",
            post(toggle_download_all_preferred),
        )
        .route("/manga/{id}/preview_migration", post(preview_migration))
        .route("/manga/{id}/migrate", post(migrate_manga_handler))
        // Download rules
        .route(
            "/manga/{id}/download_rules",
            get(get_download_rules).post(add_download_rule),
        )
        .route("/download_rules/{id}", delete(delete_download_rule))
        // Scanlator preferences
        .route(
            "/manga/{id}/scanlator_preferences",
            get(get_scanlator_prefs).post(set_scanlator_pref),
        )
        .route("/scanlator_preferences/{id}", delete(delete_scanlator_pref))
        .route("/manga/{id}/scanlator_mode", patch(set_scanlator_mode_handler))
        .route("/manga/{id}/scanlators", get(get_chapter_scanlators))
        .route("/manga/{id}/languages", get(get_chapter_languages))
        // Categories
        .route("/categories", get(list_categories).post(create_category))
        .route("/categories/reorder", put(reorder_categories))
        .route(
            "/categories/{id}",
            patch(rename_category).delete(delete_category_handler),
        )
        .route(
            "/manga/{id}/categories",
            get(get_manga_categories).put(set_manga_categories),
        )
        // ── Chapters ─────────────────────────────────────────────────────
        .route("/downloads/history", get(get_download_history))
        .route("/chapter/{id}/download", post(start_download))
        .route("/chapter/{id}/delete", delete(delete_downloaded))
        .route("/chapter/{id}/cancel", post(cancel_download))
        .route("/chapter/{id}/pages", get(get_chapter_page_manifest))
        .route("/chapter/{id}/page/{page_num}", get(serve_chapter_page))
        // ── Progress tracking ────────────────────────────────────────
        .route("/chapter/{id}/progress", put(set_chapter_progress_handler))
        .route("/chapters/read_status", put(set_chapter_read_status_handler))
        .route("/manga/{id}/tracking", get(get_manga_tracking_handler).put(set_manga_tracking_handler))
        .route("/manga/{id}/continue_reading", get(get_continue_reading_handler))
        .route("/manga/{id}/chapters/mark_up_to", post(mark_chapters_up_to_handler))
        // ── External trackers ────────────────────────────────────────
        .route("/trackers", get(list_trackers))
        .route("/trackers/{id}/auth_url", get(get_tracker_auth_url))
        .route("/trackers/{id}/callback", get(tracker_oauth_callback))
        .route("/trackers/{id}/unlink", post(unlink_tracker))
        .route("/trackers/{id}/search", get(search_tracker_manga))
        .route(
            "/trackers/{id}/config",
            get(get_tracker_config)
                .put(set_tracker_config)
                .delete(delete_tracker_config),
        )
        .route("/manga/{id}/tracker_mappings", get(get_tracker_mappings).put(set_tracker_mapping))
        .route("/manga/{id}/tracker_mappings/{tracker_id}", delete(delete_tracker_mapping))
        .route("/trackers/sync", post(sync_all_trackers))
        .route("/manga/{id}/sync", post(sync_manga_trackers))
        // ── Filters ──────────────────────────────────────────────────────
        .route("/filters/tags", get(get_filter_tags))
        .route("/filters/authors", get(get_filter_authors))
        .route("/filters/artists", get(get_filter_artists))
        // ── Settings & scan ──────────────────────────────────────────────
        .route("/settings", get(get_settings).patch(update_settings))
        .route("/scan/toggle_auto", post(toggle_auto_scan))
        .route("/refresh/start", post(start_refresh_all_rest))
        .route("/refresh/status", get(get_refresh_status))
        // ── Server control ───────────────────────────────────────────────
        .route("/server/stop", post(server_stop))
        .route("/server/restart", post(server_restart))
        // ── Admin — user management ──────────────────────────────────────
        .route("/admin/users", get(admin_list_users).post(admin_create_user))
        .route(
            "/admin/users/{id}",
            patch(admin_update_user).delete(admin_delete_user),
        )
        .route("/admin/users/{id}/roles", post(admin_grant_role))
        .route(
            "/admin/users/{id}/roles/{role}",
            delete(admin_revoke_role),
        )
        // ── Admin — role management ──────────────────────────────────────
        .route("/admin/roles", get(admin_list_roles).post(admin_create_role))
        .route(
            "/admin/roles/{slug}",
            patch(admin_update_role).delete(admin_delete_role),
        )
        .with_state(state)
        .layer(DefaultBodyLimit::max(MAX_WASM_BYTES))
}

/// Mounts only the image proxy route. Intended to be layered with a more
/// permissive rate limiter than the main REST API, since the reader fires
/// many concurrent image requests.
pub fn image_proxy_route(state: AppState) -> Router {
    Router::new()
        .route("/image_proxy", get(image_proxy))
        .with_state(state)
}

async fn auth_login(
    mut auth: AuthSession,
    State(state): State<AppState>,
    Json(form): Json<LoginRequest>,
) -> impl IntoResponse {
    let username = form.username.clone();
    let creds = Credentials {
        username: form.username,
        password: form.password,
    };
    match auth.authenticate(creds).await {
        Ok(Some(user)) => match auth.login(&user).await {
            Ok(_) => {
                if let Err(e) = auth.session.cycle_id().await {
                    tracing::warn!("Failed to cycle session ID after login: {}", e);
                }
                tracing::info!(user_id = user.id, username = %user.username, "User logged in");
                state
                    .audit(Some(user.id), "auth.login", Some(&user.username), None)
                    .await;
                (StatusCode::OK, Json(json!({"ok": true}))).into_response()
            }
            Err(e) => {
                tracing::error!("Session login error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": "Server error"})),
                )
                    .into_response()
            }
        },
        Ok(None) => {
            tracing::warn!(attempted_username = %username, "Failed login attempt");
            state
                .audit(None, "auth.login.failed", Some(&username), None)
                .await;
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Invalid credentials"})),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Auth backend error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Server error"})),
            )
                .into_response()
        }
    }
}

async fn auth_logout(mut auth: AuthSession, State(state): State<AppState>) -> impl IntoResponse {
    let (user_id, username) = auth
        .user
        .as_ref()
        .map(|u| (Some(u.id), Some(u.username.clone())))
        .unwrap_or((None, None));

    if let Err(e) = auth.logout().await {
        tracing::error!("Logout error: {}", e);
    }
    state
        .audit(user_id, "auth.logout", username.as_deref(), None)
        .await;
    (StatusCode::OK, Json(json!({"ok": true})))
}

async fn auth_me(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(json!({
        "id": user.id,
        "username": user.username,
    })))
}

const PER_HOST_CONCURRENCY: usize = 5;

async fn host_semaphore(
    cache: &moka::future::Cache<String, Arc<tokio::sync::Semaphore>>,
    host: &str,
) -> Arc<tokio::sync::Semaphore> {
    cache
        .get_with(host.to_string(), async {
            Arc::new(tokio::sync::Semaphore::new(PER_HOST_CONCURRENCY))
        })
        .await
}

async fn image_proxy(
    AuthGuard(..): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ValidatedQuery(query): ValidatedQuery<ProxyQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (url, referer) = crate::proxy::unseal_proxy_token(&query.token, &state.proxy_secret)
        .ok_or_else(|| AppError::Other("Invalid or expired proxy token".into()))?;

    let etag = crate::proxy::compute_etag(&url, &referer, &state.proxy_secret);

    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH)
        && if_none_match.as_bytes() == etag.as_bytes()
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("public, max-age=31536000, immutable"),
                ),
                (
                    header::ETAG,
                    header::HeaderValue::from_str(&etag)
                        .map_err(|e| AppError::InternalServerError(e.to_string()))?,
                ),
            ],
            Body::empty(),
        )
            .into_response());
    }

    let host = rquest::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| url.clone());

    let semaphore = host_semaphore(&state.proxy_semaphores, &host).await;
    let _permit = semaphore
        .acquire_owned()
        .await
        .map_err(|_| AppError::InternalServerError("Semaphore closed".into()))?;

    let mut req_headers = rquest::header::HeaderMap::new();
    req_headers.insert(
        rquest::header::REFERER,
        rquest::header::HeaderValue::from_str(&referer).map_err(AppError::InvalidHeaderValue)?,
    );

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(35),
        state.proxy_client.safe_get(&url, Some(req_headers)),
    )
    .await
    .map_err(|_| AppError::Other("Upstream image fetch timed out".into()))??;

    if !response.status().is_success() {
        tracing::warn!(
            "Upstream returned {} for proxied request",
            response.status().as_u16()
        );
        return Err(AppError::Other(format!(
            "Upstream returned {}",
            response.status().as_u16()
        )));
    }

    let content_type = response
        .headers()
        .get(rquest::header::CONTENT_TYPE)
        .cloned()
        .ok_or_else(|| {
            AppError::InternalServerError("Upstream response missing Content-Type".into())
        })?;

    let ct_str = content_type.to_str().unwrap_or("");
    if !ct_str.starts_with("image/") {
        tracing::warn!("Upstream proxy returned non-image Content-Type: {}", ct_str);
        return Err(AppError::Other(format!(
            "Expected image, upstream returned Content-Type: {}",
            ct_str
        )));
    }

    let ct_value = header::HeaderValue::from_bytes(content_type.as_bytes())
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    let etag_value = header::HeaderValue::from_str(&etag)
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;

    const MAX_IMAGE_BYTES: usize = 50 * 1024 * 1024;

    let stream = futures::stream::unfold(
        (response, 0usize, Some(_permit)),
        move |(mut resp, received, permit)| async move {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    let new_total = received + chunk.len();
                    if new_total > MAX_IMAGE_BYTES {
                        tracing::warn!(
                            "Upstream image exceeded {} byte limit, aborting stream",
                            MAX_IMAGE_BYTES
                        );
                        None
                    } else {
                        Some((Ok(chunk), (resp, new_total, permit)))
                    }
                }
                Ok(None) => None,
                Err(e) => Some((
                    Err(std::io::Error::other(e.to_string())),
                    (resp, received, permit),
                )),
            }
        },
    );

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ct_value),
            (header::ETAG, etag_value),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                header::HeaderValue::from_static("nosniff"),
            ),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

async fn list_sources(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.list_sources().await?))
}

async fn add_source(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    ValidatedJson(payload): ValidatedJson<CreateSource>,
) -> Result<impl IntoResponse, AppError> {
    let id = state.add_source(&payload.name, user.id).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn get_source(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let source = state.get_source(id).await?;
    Ok(Json(source))
}

async fn update_source(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ValidatedJson(payload): ValidatedJson<UpdateSource>,
) -> Result<impl IntoResponse, AppError> {
    if payload.name.is_none() && payload.version.is_none() {
        return Ok(Json(json!({})));
    }
    state
        .update_source(id, payload.name, payload.version)
        .await?;
    Ok(Json(json!({})))
}

async fn delete_source(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SourceDelete>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_source(id, user.id).await?;
    Ok(Json(json!({})))
}

async fn get_metadata(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = state.get_metadata(id).await?;
    Ok(result)
}

pub(crate) async fn install_source(
    state: &AppState,
    id: i64,
    current_source: &Source,
    bytes: &[u8],
) -> Result<std::path::PathBuf, AppError> {
    let bytes_owned = bytes.to_vec();
    let runtime_clone = state.wasm_runtime.clone();

    let component =
        tokio::task::spawn_blocking(move || runtime_clone.compile_component(&bytes_owned))
            .await
            .map_err(|e| {
                AppError::InternalServerError(format!("WASM compilation task panicked: {}", e))
            })??;

    let (metadata, raw_schema) = {
        let mut inst =
            kani_core::sources::SourceInstance::new(state.smart_client.clone(), None, false);
        inst.load(
            state.wasm_runtime.engine(),
            &component,
            state.wasm_runtime.linker(),
        )
        .await
        .map_err(AppError::CoreError)?;
        let meta = inst.get_metadata().await.map_err(AppError::CoreError)?;
        let schema = inst.get_preferences().await.ok();
        (meta, schema)
    };

    sqlx::query!(
        "UPDATE sources SET name = ?, version = ?, base_url = ?, unrestricted_http = ? WHERE id = ?",
        metadata.name,
        metadata.version,
        metadata.base_url,
        metadata.unrestricted_http,
        id
    )
    .execute(&state.db)
    .await?;

    let settings = state.settings.read().await;
    let storage_path = settings
        .wasm_storage_path
        .to_str()
        .ok_or_else(|| AppError::InternalServerError("Failed to convert path".to_string()))?;

    if current_source.name != metadata.name {
        tracing::info!(
            "Source name changed from {} to {}. Deleting old file.",
            current_source.name,
            metadata.name
        );
        let _ = kani_core::file_storage::delete_wasm_file(storage_path, &current_source.name).await;
    }

    let path = kani_core::file_storage::save_wasm(storage_path, &metadata.name, bytes)
        .await
        .map_err(AppError::CoreError)?;
    drop(settings);

    let source_manager = SourceManager::new(
        state.wasm_runtime.engine().clone(),
        state
            .wasm_runtime
            .instantiate_pre(&component)
            .map_err(AppError::CoreError)?,
        state.smart_client.clone(),
        Some(metadata.base_url.clone()),
        metadata.unrestricted_http,
        25,
        state.load_pref_map(id).await.unwrap_or_default(),
    );

    state
        .sources
        .write()
        .await
        .insert(id, Arc::new(source_manager));

    if let Some(schema) = raw_schema {
        state.cache.insert_preference_schema(id, schema);
    }

    state.cache.invalidate_source(id);

    tracing::info!(
        "Successfully installed source {}: {} v{}",
        id,
        metadata.name,
        metadata.version
    );

    Ok(path)
}

async fn upload_wasm(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let source = state.get_source(id).await?;

    let field = multipart
        .next_field()
        .await?
        .ok_or_else(|| AppError::InternalServerError("no file field in upload".into()))?;

    let content_length = field
        .headers()
        .get(rquest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());

    let bytes: bytes::Bytes = kani_core::http::collect_bytes_limited(
        Box::pin(field.map_err(|e| kani_core::error::Error::Other(e.to_string()))),
        content_length,
        MAX_WASM_BYTES,
    )
    .await?;

    let _ = install_source(&state, id, &source, bytes.as_ref()).await?;

    Ok(StatusCode::OK)
}

async fn fetch_wasm(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceInstall>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    ValidatedJson(payload): ValidatedJson<FetchWasmRequest>,
) -> Result<impl IntoResponse, AppError> {
    let source = state.get_source(id).await?;

    let response = state.proxy_client.safe_get(&payload.url, None).await?;

    let bytes = response.bytes_limited(MAX_WASM_BYTES).await?;

    let _ = install_source(&state, id, &source, &bytes).await?;

    Ok(StatusCode::OK)
}

async fn get_popular_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, page, page_size)): Path<(i64, i32, i32)>,
    Query(query): Query<crate::models::PopularMangaQuery>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state.get_popular_manga(id, page, page_size, query.filters).await?;
    let mut list: crate::types::MangaList = serde_json::from_str(&json_str)?;
    for item in &mut list.manga {
        if let Some(ref url) = item.cover_url.clone() {
            item.cover_url = Some(sign_image_url(url, &base_url, &state));
        }
    }
    Ok(Json(list))
}

async fn search_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, page, page_size)): Path<(i64, i32, i32)>,
    ValidatedQuery(payload): ValidatedQuery<SearchMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state
        .search_manga(id, &payload.query.unwrap_or("".to_string()), page, page_size, payload.filters)
        .await?;
    let mut list: crate::types::MangaList = serde_json::from_str(&json_str)?;
    for item in &mut list.manga {
        if let Some(ref url) = item.cover_url.clone() {
            item.cover_url = Some(sign_image_url(url, &base_url, &state));
        }
    }
    Ok(Json(list))
}

async fn get_source_filters(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let filter_list = state.get_filter_list(id).await?;
    Ok(Json(filter_list))
}

async fn get_manga_details(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state.get_manga_details(id, &manga_id).await?;
    let mut info: crate::types::MangaInfo = serde_json::from_str(&json_str)?;
    info.cover_url = info
        .cover_url
        .map(|url| sign_image_url(&url, &base_url, &state));
    info.description_html = info
        .description
        .as_deref()
        .map(crate::utils::render_description);
    Ok(Json(info))
}

#[derive(serde::Deserialize, Default)]
struct ChapterListQuery {
    sort: Option<String>,
}

async fn get_chapter_list(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, manga_id, page, page_size)): Path<(i64, String, i32, i32)>,
    Query(q): Query<ChapterListQuery>,
) -> Result<impl IntoResponse, AppError> {
    let json_str = state
        .get_chapter_list_paged(id, &manga_id, page, page_size, q.sort)
        .await?;
    let list: crate::types::ChapterList = serde_json::from_str(&json_str)?;
    Ok(Json(list))
}

async fn get_chapter_sort_list(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, _manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let opts = state.get_chapter_sort_list(id).await?;
    Ok(Json(opts))
}

async fn get_pages(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path((id, manga_id, chapter_id)): Path<(i64, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let base_url = state.get_source_base_url(id).await?;
    let json_str = state.get_pages(id, &manga_id, &chapter_id).await?;
    let mut contents: crate::types::ChapterContents = serde_json::from_str(&json_str)?;
    for page in &mut contents.pages {
        page.url = sign_image_url(&page.url, &base_url, &state);
    }
    Ok(Json(contents))
}

async fn save_to_library(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryAdd>,
    State(state): State<AppState>,
    Path((id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let manga_row_id = state.save_to_library(id, &manga_id).await?;
    Ok(Json(json!({ "db_id": manga_row_id })))
}

async fn get_library(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path((page, order)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_library(page, order).await?))
}

async fn get_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_manga_by_id(id).await?))
}

async fn delete_manga(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryDelete>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_manga(id, user.id).await?;
    Ok(Json(json!({})))
}

async fn start_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.download_chapter(id).await?;
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

async fn delete_downloaded(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDelete>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_downloaded(id).await?;
    Ok((StatusCode::OK, Json(json!({}))))
}

async fn serve_manga_cover(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let full_path = state.get_manga_cover_path(id).await?;

    let metadata = tokio::fs::metadata(&full_path)
        .await
        .map_err(|_| AppError::NotFound("Cover file not found on disk".into()))?;

    let mtime = metadata
        .modified()
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        })
        .unwrap_or(0);
    let etag = format!("\"{}\"", mtime);

    if let Some(inm) = headers.get(header::IF_NONE_MATCH)
        && inm.as_bytes() == etag.as_bytes()
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("public, max-age=31536000, immutable"),
                ),
                (
                    header::ETAG,
                    header::HeaderValue::from_str(&etag)
                        .map_err(|e| AppError::InternalServerError(e.to_string()))?,
                ),
            ],
            axum::body::Body::empty(),
        )
            .into_response());
    }

    let ext = full_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpg");
    let content_type = match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    };

    let bytes = tokio::fs::read(&full_path)
        .await
        .map_err(AppError::IoError)?;

    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                header::HeaderValue::from_static(content_type),
            ),
            (
                header::ETAG,
                header::HeaderValue::from_str(&etag)
                    .map_err(|e| AppError::InternalServerError(e.to_string()))?,
            ),
            (
                header::CONTENT_LENGTH,
                header::HeaderValue::from(bytes.len()),
            ),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                header::HeaderValue::from_static("nosniff"),
            ),
        ],
        axum::body::Body::from(bytes),
    )
        .into_response())
}

// ── Library ──────────────────────────────────────────────────────────────────

async fn get_library_filtered(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<LibraryQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (records, has_next_page) = state
        .get_library_filtered(
            user.id,
            q.page,
            q.page_size,
            q.search,
            q.status_filter,
            q.tag_filter,
            q.author_filter,
            q.artist_filter,
            q.category_filter,
            q.reading_status_filter,
            q.hide_no_unread,
            q.hide_completed_status,
            q.source_id,
            q.sort_by,
        )
        .await?;

    let items = records
        .into_iter()
        .map(|r| {
            let cover_url = if r.local_cover_path.is_some() {
                Some(format!("/rest/manga/{}/cover", r.id))
            } else {
                r.cover_url
                    .map(|url| sign_image_url(&url, &r.base_url, &state))
            };
            crate::types::MangaListItem {
                id: r.id.to_string(),
                title: r.name,
                cover_url,
            }
        })
        .collect();

    Ok(Json(crate::types::LibraryPage {
        items,
        has_next_page,
    }))
}

async fn get_local_manga_details(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    use crate::types::{MangaInfo, MangaStatus};
    let d = state.get_local_manga_details(id).await?;
    let description_html = d
        .manga
        .description
        .as_ref()
        .map(|s| crate::utils::render_description(s));
    let cover_url = if d.manga.local_cover_path.is_some() {
        Some(format!("/rest/manga/{}/cover", id))
    } else {
        d.manga
            .cover_url
            .map(|url| sign_image_url(&url, &d.source.base_url, &state))
    };
    let info = MangaInfo {
        id: d.manga.source_manga_id,
        title: d.manga.name,
        cover_url,
        description: d.manga.description,
        description_html,
        status: MangaStatus::from(i64::from(d.manga.status)),
        authors: d.authors,
        artists: d.artists,
        tags: d.tags,
    };
    Ok(Json(json!({
        "info":                        info,
        "source":                      d.source,
        "auto_download":               d.manga.auto_download,
        "auto_scan":                   d.auto_scan,
        "scanlator_mode":              d.manga.scanlator_mode,
        "download_all_preferred_only": d.manga.download_all_preferred_only,
    })))
}

async fn get_local_chapters(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    ValidatedQuery(q): ValidatedQuery<LocalChaptersQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (chapters, has_next_page) = state
        .get_local_chapters(
            manga_id,
            q.page,
            q.page_size,
            q.sort_order,
            user.id,
            q.filter_downloaded,
            q.filter_unread,
            q.filter_scanlator,
        )
        .await?;
    Ok(Json(crate::types::ChapterList {
        chapters,
        has_next_page,
    }))
}

async fn get_chapter_ids(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    ValidatedQuery(q): ValidatedQuery<crate::models::ChapterIdsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let ids = state
        .get_chapter_ids(
            manga_id,
            user.id,
            q.sort_order,
            q.filter_downloaded,
            q.filter_unread,
            q.filter_scanlator,
            q.preferred_only,
        )
        .await?;
    Ok(Json(json!({ "ids": ids })))
}

async fn check_in_library(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path((source_id, manga_id)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let decoded = crate::utils::decode_manga_id(&manga_id);
    let db_id = state.check_in_library(source_id, &decoded).await?;
    Ok(Json(json!({ "db_id": db_id })))
}

async fn download_all(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = state_clone.download_all_chapters(manga_id).await {
            tracing::error!(
                "Failed to queue all downloads for manga {}: {}",
                manga_id,
                e
            );
        }
    });
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

async fn cancel_all_downloads(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.cancel_all_downloads(manga_id).await?;
    Ok(Json(json!({})))
}

async fn cancel_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ChapterDownload>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.cancel_download(chapter_id).await?;
    Ok(Json(json!({})))
}

#[derive(serde::Deserialize)]
struct DownloadHistoryQuery {
    #[serde(default = "default_history_limit")]
    limit: i64,
}
fn default_history_limit() -> i64 { 50 }

async fn get_download_history(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Query(q): Query<DownloadHistoryQuery>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.get_download_history(q.limit).await?;
    Ok(Json(items))
}

async fn get_recent_updates(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<PageQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (mut items, has_next_page) = state.get_recent_updates(q.page).await?;
    for u in &mut items {
        u.cover_url = if u.local_cover_path.is_some() {
            Some(format!("/rest/manga/{}/cover", u.manga_id))
        } else if let Some(ref url) = u.cover_url.clone() {
            Some(sign_image_url(url, &u.base_url, &state))
        } else {
            None
        };
    }
    Ok(Json(crate::types::RecentUpdate {
        recent_updates: items,
        has_next_page,
    }))
}

// ── Filter metadata ───────────────────────────────────────────────────────────

async fn get_filter_tags(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_filter_tags().await?))
}

async fn get_filter_authors(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_filter_authors().await?))
}

async fn get_filter_artists(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_filter_artists().await?))
}

async fn global_search_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Query(q): Query<GlobalSearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let mut list = state
        .global_search(&q.query, q.scope, q.page, q.page_size)
        .await?;

    let source_ids: Vec<i64> = list
        .iter()
        .map(|i| i.source_id)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let source_ids_json = serde_json::to_string(&source_ids)?;
    let base_urls: std::collections::HashMap<i64, String> = sqlx::query!(
        "SELECT id, base_url FROM sources WHERE id IN (SELECT value FROM json_each(?))",
        source_ids_json
    )
    .fetch_all(&state.db)
    .await?
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
    Ok(Json(list))
}

// ── Settings & scan ───────────────────────────────────────────────────────────

async fn get_settings(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_settings().await))
}

async fn update_settings(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(update): Json<crate::types::SettingsUpdate>,
) -> Result<impl IntoResponse, AppError> {
    use crate::types::SettingsUpdate;

    let user = auth
        .user
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?;
    let required_perm = match &update {
        SettingsUpdate::Download(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditDownload)
        }
        SettingsUpdate::Scan(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditScan)
        }
        SettingsUpdate::Advanced(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditAdvanced)
        }
        SettingsUpdate::Tracking(_) => {
            crate::permissions::Permission::Settings(crate::permissions::Settings::EditScan)
        }
    };
    if !auth
        .backend
        .has_perm(&user, required_perm)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
    {
        return Err(AppError::Forbidden("Insufficient permissions".into()));
    }
    state.update_settings(update, user.id).await?;
    Ok(Json(json!({})))
}

async fn toggle_auto_scan(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditScan>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let new_val = state.toggle_auto_scan().await?;
    Ok(Json(json!({ "auto_scan": new_val })))
}

async fn get_refresh_status(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        json!({ "is_refreshing": state.is_refreshing().await }),
    ))
}

async fn server_stop(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(user_id = user.id, username = %user.username, "Server stop requested");
    state.audit(Some(user.id), "server.stop", None, None).await;
    state.shutdown_token.cancel();
    Ok(Json(json!({ "ok": true })))
}

async fn server_restart(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    use std::sync::atomic::Ordering;
    tracing::info!(user_id = user.id, username = %user.username, "Server restart requested");
    state.audit(Some(user.id), "server.restart", None, None).await;
    state.restart_requested.store(true, Ordering::Relaxed);
    state.shutdown_token.cancel();
    Ok(Json(json!({ "ok": true })))
}

async fn toggle_auto_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.toggle_auto_download(manga_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_download_all_preferred(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<ToggleAutoDownloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .toggle_download_all_preferred(manga_id, body.enabled)
        .await?;
    Ok(Json(json!({})))
}

async fn refresh_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.refresh_manga(id).await?;
    Ok(Json(json!({})))
}

async fn scan_manga(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let new_chapters = state.scan_for_new_chapters(id).await?.len() as i64;
    Ok(Json(json!({ "new_chapters": new_chapters })))
}

async fn get_boot_id(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    Json(json!({ "boot_id": state.boot_id }))
}

// ── Download rules ────────────────────────────────────────────────────────────

async fn get_download_rules(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_download_rules(manga_id).await?))
}

async fn add_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<AddDownloadRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = state.add_download_rule(manga_id, body.kind.clone()).await?;
    Ok((
        StatusCode::CREATED,
        Json(kani_shared::types::DownloadRule {
            id,
            manga_id,
            kind: body.kind,
        }),
    ))
}

async fn delete_download_rule(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(rule_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_download_rule(rule_id).await?;
    Ok(Json(json!({})))
}

// ── Scanlator preferences ─────────────────────────────────────────────────────

async fn get_scanlator_prefs(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_scanlator_prefs(manga_id).await?))
}

async fn set_scanlator_pref(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetScanlatorPrefRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_scanlator_pref(manga_id, &body.scanlator, body.priority, body.blocked)
        .await?;
    Ok(Json(json!({})))
}

async fn set_scanlator_mode_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<crate::models::SetScanlatorModeRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.set_scanlator_mode(manga_id, &body.mode).await?;
    Ok(Json(json!({})))
}

async fn get_chapter_scanlators(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_chapter_scanlators(manga_id).await?))
}

async fn get_chapter_languages(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_chapter_languages(manga_id).await?))
}

async fn delete_scanlator_pref(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(pref_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_scanlator_pref(pref_id).await?;
    Ok(Json(json!({})))
}

// ── Categories ────────────────────────────────────────────────────────────────

async fn list_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.list_categories().await?))
}

async fn create_category(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Json(body): Json<CreateCategoryRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = state.create_category(&body.name, body.sort_order).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

async fn rename_category(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(category_id): Path<i64>,
    Json(body): Json<RenameCategoryRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.rename_category(category_id, &body.name).await?;
    Ok(Json(json!({})))
}

async fn delete_category_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(category_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_category(category_id).await?;
    Ok(Json(json!({})))
}

async fn reorder_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Json(body): Json<ReorderCategoriesRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.reorder_categories(body.ordered_ids).await?;
    Ok(Json(json!({})))
}

async fn get_manga_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.get_manga_categories(manga_id).await?))
}

async fn set_manga_categories(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetMangaCategoriesRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_manga_categories(manga_id, body.category_ids)
        .await?;
    Ok(Json(json!({})))
}

// ── Source preferences ────────────────────────────────────────────────────────

async fn get_pref_schema(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(cached) = state.cache.get_preference_schema(source_id) {
        return Ok(Json(cached));
    }
    let mgr = { state.sources.read().await.get(&source_id).cloned() };
    let raw = if let Some(mgr) = mgr {
        let mut inst = mgr.lease_instance().await.map_err(AppError::CoreError)?;
        inst.get_preferences().await.map_err(AppError::CoreError)?
    } else {
        let name = sqlx::query_scalar!("SELECT name FROM sources WHERE id=?", source_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| AppError::NotFound("Source not found".into()))?;
        let wasm_path = state
            .settings
            .read()
            .await
            .wasm_storage_path
            .join(format!("{}.wasm", name));
        let bytes = tokio::fs::read(&wasm_path).await?;
        let component = state
            .wasm_runtime
            .compile_component(&bytes)
            .map_err(AppError::CoreError)?;
        let mut inst =
            kani_core::sources::SourceInstance::new(state.smart_client.clone(), None, false);
        inst.load(
            state.wasm_runtime.engine(),
            &component,
            state.wasm_runtime.linker(),
        )
        .await
        .map_err(AppError::CoreError)?;
        inst.get_preferences().await.map_err(AppError::CoreError)?
    };
    state.cache.insert_preference_schema(source_id, raw.clone());
    Ok(Json(raw))
}

async fn get_source_preferences(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = state
        .get_all_preferences(source_id)
        .await?
        .into_iter()
        .map(|(k, v)| json!({ "key": k, "value": v }))
        .collect::<Vec<_>>();
    Ok(Json(rows))
}

async fn set_source_preference(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<SetPreferenceRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.set_preference(source_id, &key, &body.value).await?;
    Ok(Json(json!({})))
}

async fn append_pref_list_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ListItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .append_pref_list_item(source_id, &key, body.item)
        .await?;
    Ok(Json(json!({})))
}

async fn remove_pref_list_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ListItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .remove_pref_list_item(source_id, &key, &body.item)
        .await?;
    Ok(Json(json!({})))
}

async fn toggle_pref_select_item(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceConfigure>,
    State(state): State<AppState>,
    Path((source_id, key)): Path<(i64, String)>,
    Json(body): Json<ToggleSelectRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .toggle_pref_select_item(source_id, &key, body.item, body.selected)
        .await?;
    Ok(Json(json!({})))
}

async fn toggle_source_enabled(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceToggleEnabled>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
    Json(body): Json<ToggleEnabledRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.toggle_source_enabled(source_id, body.enabled).await?;
    Ok(Json(json!({})))
}

async fn toggle_source_favourite(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
    Path(source_id): Path<i64>,
    Json(body): Json<ToggleFavouritedRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .toggle_source_favourite(source_id, body.favourited)
        .await?;
    Ok(Json(json!({})))
}

async fn get_active_source_ids(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SourceBrowse>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let ids: Vec<i64> = state.sources.read().await.keys().copied().collect();
    Ok(Json(ids))
}

// ── Migration ─────────────────────────────────────────────────────────────────

async fn preview_migration(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<PreviewMigrationRequest>,
) -> Result<impl IntoResponse, AppError> {
    let preview = state
        .preview_migration(manga_id, body.target_source_id, body.target_source_manga_id)
        .await?;
    Ok(Json(preview))
}

async fn migrate_manga_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryManage>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<MigrateMangaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .migrate_manga(
            manga_id,
            body.target_source_id,
            body.target_source_manga_id,
            body.keep_orphaned_downloads,
        )
        .await?;
    Ok(Json(result))
}

// ── User / Auth ───────────────────────────────────────────────────────────────

async fn get_current_user(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(crate::types::AuthenticatedUser {
        id: user.id,
        username: user.username,
        email: user.email,
        roles: user.roles,
    }))
}

async fn change_password(
    auth: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user = auth
        .user
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?;
    if body.new_password.len() < 8 {
        return Err(AppError::ValidationError(
            "New password must be at least 8 characters".into(),
        ));
    }
    let backend = AuthBackend::new(state.db.clone());
    let verified = backend
        .authenticate(Credentials {
            username: user.username.clone(),
            password: body.current_password,
        })
        .await?;
    if verified.is_none() {
        return Err(AppError::PasswordError(
            "Current password is incorrect".into(),
        ));
    }
    backend.change_password(user.id, &body.new_password).await?;
    state
        .audit(
            Some(user.id),
            "auth.change_password",
            Some(&user.username),
            None,
        )
        .await;
    Ok(Json(json!({})))
}

async fn logout_everywhere(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend.cycle_change_id(user.id).await?;
    state
        .audit(
            Some(user.id),
            "auth.logout_everywhere",
            Some(&user.username),
            None,
        )
        .await;
    Ok(Json(json!({})))
}

async fn get_my_permissions(auth: AuthSession) -> Result<impl IntoResponse, AppError> {
    let user = auth
        .user
        .ok_or_else(|| AppError::Unauthorized("Not authenticated".into()))?;
    let perms = auth
        .backend
        .get_all_permissions(&user)
        .await
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(Json(perms))
}

pub async fn combined_sse(
    AuthGuard(..): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let snapshot = state.downloader.snapshot().await;
    let is_refreshing = state.is_refreshing().await;

    let snapshot_event = Ok::<Event, Infallible>(
        Event::default().data(
            serde_json::json!({
                "type": "state_snapshot",
                "chapters": snapshot,
                "is_refreshing": is_refreshing
            })
            .to_string(),
        ),
    );

    let download_rx = state.downloader.subscribe();
    let refresh_rx = state.subscribe_refresh();

    let download_stream = BroadcastStream::new(download_rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).ok()?;
            Some(Ok::<Event, Infallible>(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("Download SSE lagged by {} events", n);
            Some(Ok(Event::default().event("close").data("")))
        }
    });

    let refresh_stream = BroadcastStream::new(refresh_rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).ok()?;
            Some(Ok::<Event, Infallible>(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("Refresh SSE lagged by {} events", n);
            None
        }
    });

    let live_stream = download_stream.merge(refresh_stream);
    let stream = tokio_stream::once(snapshot_event).chain(live_stream);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn start_refresh_all_rest(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryRefresh>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.start_refresh_all().await?;
    Ok((StatusCode::ACCEPTED, Json(json!({}))))
}

// ── Chapter CBZ reader ────────────────────────────────────────────────────────

async fn get_chapter_page_manifest(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let manifest = state.get_chapter_page_manifest(id, user.id).await?;
    Ok(Json(manifest))
}

async fn serve_chapter_page(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path((id, page_num)): Path<(i64, usize)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // Resolve the path first so we can derive an ETag from the CBZ mtime.
    // This avoids reading image bytes on cache hits.
    let (cbz_path, ..) = state.chapter_cbz_path(id).await?;

    let mtime = tokio::fs::metadata(&cbz_path)
        .await
        .map(|m| {
            m.modified()
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let etag = format!("\"{mtime}-{page_num}\"");

    if let Some(inm) = headers.get(header::IF_NONE_MATCH)
        && inm.as_bytes() == etag.as_bytes()
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (
                    header::CACHE_CONTROL,
                    header::HeaderValue::from_static("public, max-age=31536000, immutable"),
                ),
                (
                    header::ETAG,
                    header::HeaderValue::from_str(&etag)
                        .map_err(|e| AppError::InternalServerError(e.to_string()))?,
                ),
            ],
            axum::body::Body::empty(),
        )
            .into_response());
    }

    let (bytes, ext) = state.read_chapter_page(id, page_num).await?;

    let content_type = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        _ => "image/jpeg",
    };

    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                header::HeaderValue::from_static(content_type),
            ),
            (
                header::ETAG,
                header::HeaderValue::from_str(&etag)
                    .map_err(|e| AppError::InternalServerError(e.to_string()))?,
            ),
            (
                header::CONTENT_LENGTH,
                header::HeaderValue::from(bytes.len()),
            ),
            (
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("public, max-age=31536000, immutable"),
            ),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                header::HeaderValue::from_static("nosniff"),
            ),
        ],
        axum::body::Body::from(bytes),
    )
        .into_response())
}

pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

pub async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ready"}))).into_response(),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "status": "not_ready",
                "reason": format!("database: {e}")
            })),
        )
            .into_response(),
    }
}

// ── External tracker handlers ────────────────────────────────────────────

const OAUTH_SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><title>Account Linked</title></head>
<body>
<script>
  if (window.opener) {
    window.opener.postMessage({ type: 'tracker_linked' }, window.location.origin);
    window.close();
  } else {
    document.body.innerText = 'Account linked successfully. You can close this window.';
  }
</script>
</body>
</html>"#;

async fn list_trackers(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.list_trackers_status(user.id).await?;
    let trackers: Vec<_> = items
        .into_iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "configured": t.configured,
                "linked": t.linked,
            })
        })
        .collect();
    Ok(Json(trackers))
}

async fn get_tracker_auth_url(
    AuthGuard(..): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Query(q): Query<TrackerAuthUrlQuery>,
) -> Result<impl IntoResponse, AppError> {
    let url = state.get_tracker_auth_url(tracker_id, &q.redirect_uri).await?;
    Ok(Json(json!({ "url": url })))
}

async fn tracker_oauth_callback(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Query(q): Query<TrackerCallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    state.complete_tracker_oauth(user.id, tracker_id, &q.code, &q.state).await?;
    Ok((
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        OAUTH_SUCCESS_HTML,
    ))
}

async fn get_tracker_config(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let config = state.get_tracker_config(tracker_id).await?;
    match config {
        Some((client_id, secret_configured)) => Ok(Json(json!({
            "client_id": client_id,
            "secret_configured": secret_configured,
        }))),
        None => Ok(Json(json!({
            "client_id": null,
            "secret_configured": false,
        }))),
    }
}

async fn set_tracker_config(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Json(body): Json<SetTrackerConfigRequest>,
) -> Result<impl IntoResponse, AppError> {
    let secret = body.client_secret.as_deref().filter(|s| !s.is_empty());
    state.set_tracker_config(tracker_id, &body.client_id, secret).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tracker_config(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_tracker_config(tracker_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unlink_tracker(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.unlink_tracker(user.id, tracker_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn search_tracker_manga(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(tracker_id): Path<i64>,
    Query(q): Query<TrackerSearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let results = state.search_tracker_manga(user.id, tracker_id, &q.query).await?;
    Ok(Json(results))
}

async fn get_tracker_mappings(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let mappings = state.get_tracker_mappings(user.id, manga_id).await?;
    let response: Vec<_> = mappings
        .into_iter()
        .map(|m| {
            json!({
                "tracker_id": m.tracker_id,
                "tracker_name": m.tracker_name,
                "tracker_manga_id": m.tracker_manga_id,
            })
        })
        .collect();
    Ok(Json(response))
}

async fn set_tracker_mapping(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetTrackerMappingRequest>,
) -> Result<impl IntoResponse, AppError> {
    kani_app::service::trackers::set_mapping(
        &state.db,
        user.id,
        body.tracker_id,
        manga_id,
        &body.tracker_manga_id,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tracker_mapping(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path((manga_id, tracker_id)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, AppError> {
    kani_app::service::trackers::delete_mapping(&state.db, user.id, tracker_id, manga_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_all_trackers(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.sync_all_trackers(user.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_manga_trackers(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.sync_manga_trackers(user.id, manga_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Progress tracking handlers ───────────────────────────────────────────

async fn set_chapter_progress_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(chapter_id): Path<i64>,
    Json(body): Json<SetChapterProgressRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_chapter_progress(user.id, chapter_id, body.page)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_chapter_read_status_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Json(body): Json<SetReadStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .set_chapter_read_status(user.id, body.chapter_ids, body.is_read)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_manga_tracking_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let tracking = state.get_manga_tracking(user.id, manga_id).await?;
    Ok(Json(tracking))
}

async fn set_manga_tracking_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<SetMangaTrackingRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(status) = body.status {
        state.set_manga_status(user.id, manga_id, status).await?;
    }
    if let Some(score) = body.score {
        state.set_manga_score(user.id, manga_id, score).await?;
    }
    if let Some(enabled) = body.tracking_enabled {
        state
            .set_manga_tracking_enabled(user.id, manga_id, enabled)
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn get_continue_reading_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let info = state.get_continue_reading_chapter(user.id, manga_id).await?;
    Ok(Json(info))
}

async fn get_continue_reading_shelf(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Query(q): Query<ContinueReadingShelfQuery>,
) -> Result<impl IntoResponse, AppError> {
    let items = state.get_continue_reading_shelf(user.id, q.limit).await?;
    let response: Vec<_> = items
        .into_iter()
        .map(|item| {
            let cover_url = if item.local_cover_path.is_some() {
                Some(format!("/rest/manga/{}/cover", item.manga_id))
            } else {
                item.cover_url
                    .map(|url| sign_image_url(&url, &item.base_url, &state))
            };
            json!({
                "manga_id": item.manga_id,
                "manga_name": item.manga_name,
                "cover_url": cover_url,
                "chapter_id": item.chapter_id,
                "chapter_number": item.chapter_number,
                "last_page": item.last_page,
            })
        })
        .collect();
    Ok(Json(response))
}

async fn mark_chapters_up_to_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::IsAuthenticated>,
    State(state): State<AppState>,
    Path(manga_id): Path<i64>,
    Json(body): Json<MarkUpToRequest>,
) -> Result<impl IntoResponse, AppError> {
    let ids = state.get_chapters_up_to(manga_id, body.chapter_number).await?;
    state.set_chapter_read_status(user.id, ids, body.is_read).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Admin — user management ───────────────────────────────────────────────────

async fn admin_list_users(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    let users = backend.list_users().await?;
    Ok(Json(users))
}

async fn admin_create_user(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Json(body): Json<AdminCreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.password.len() < 8 {
        return Err(AppError::ValidationError(
            "Password must be at least 8 characters".into(),
        ));
    }
    let backend = AuthBackend::new(state.db.clone());
    let user = backend.create_user(&body.username, &body.email, &body.password).await?;
    for role in &body.roles {
        backend.grant_role(user.id, role, Some(admin.id)).await?;
    }
    state
        .audit(
            Some(admin.id),
            "admin.user.create",
            Some(&user.username),
            Some(json!({ "user_id": user.id })),
        )
        .await;
    let created = backend
        .fetch_user_by_id(user.id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn admin_update_user(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Json(body): Json<AdminUpdateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    if body.username.is_some() || body.email.is_some() {
        backend
            .update_user(user_id, body.username.as_deref(), body.email.as_deref())
            .await?;
    }
    if let Some(active) = body.is_active {
        backend.set_active(user_id, active).await?;
    }
    if let Some(ref pw) = body.password {
        if pw.len() < 8 {
            return Err(AppError::ValidationError(
                "Password must be at least 8 characters".into(),
            ));
        }
        backend.admin_reset_password(user_id, pw).await?;
    }
    state
        .audit(
            Some(admin.id),
            "admin.user.update",
            None,
            Some(json!({ "user_id": user_id })),
        )
        .await;
    Ok(Json(
        backend
            .fetch_user_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?,
    ))
}

async fn admin_delete_user(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if user_id == admin.id {
        return Err(AppError::ValidationError("Cannot delete your own account".into()));
    }
    let backend = AuthBackend::new(state.db.clone());
    backend.delete_user(user_id).await?;
    state
        .audit(
            Some(admin.id),
            "admin.user.delete",
            None,
            Some(json!({ "user_id": user_id })),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_grant_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Json(body): Json<AdminGrantRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend.grant_role(user_id, &body.role_slug, Some(admin.id)).await?;
    state
        .audit(
            Some(admin.id),
            "admin.user.grant_role",
            Some(&body.role_slug),
            Some(json!({ "user_id": user_id })),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_revoke_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path((user_id, role_slug)): Path<(i64, String)>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend.revoke_role(user_id, &role_slug).await?;
    state
        .audit(
            Some(admin.id),
            "admin.user.revoke_role",
            Some(&role_slug),
            Some(json!({ "user_id": user_id })),
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

// ── Admin — role management ───────────────────────────────────────────────────

async fn admin_list_roles(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    let roles = backend.list_roles().await?;
    Ok(Json(roles))
}

async fn admin_create_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Json(body): Json<AdminCreateRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend
        .create_role(
            &body.slug,
            body.parent.as_deref(),
            body.description.as_deref(),
            &body.permissions,
        )
        .await?;
    state
        .audit(
            Some(admin.id),
            "admin.role.create",
            Some(&body.slug),
            None,
        )
        .await;
    Ok(StatusCode::CREATED)
}

async fn admin_update_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<AdminUpdateRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend
        .update_role(
            &slug,
            body.description.as_deref(),
            body.permissions.as_deref().unwrap_or(&[]),
        )
        .await?;
    state
        .audit(
            Some(admin.id),
            "admin.role.update",
            Some(&slug),
            None,
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_delete_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend.delete_role(&slug).await?;
    state
        .audit(
            Some(admin.id),
            "admin.role.delete",
            Some(&slug),
            None,
        )
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_login::{
        AuthManagerLayerBuilder,
        tower_sessions::{SessionManagerLayer, cookie::SameSite},
    };
    use http_body_util::BodyExt;
    use std::{collections::HashSet, sync::Arc};
    use tower::ServiceExt;
    use tower_sessions_sqlx_store::SqliteStore;

    // ── test helpers ──────────────────────────────────────────────────────────

    async fn test_app_state(pool: sqlx::SqlitePool) -> AppState {
        use kani_app::{AppService, cache::RequestCache, models::Settings};
        use kani_core::{downloader::DownloaderManager, http::SmartClient, wasm::WasmRuntime};
        let wasm_runtime = Arc::new(WasmRuntime::new(1).expect("WasmRuntime"));
        let smart_client = SmartClient::new(None).expect("SmartClient");
        let downloader = DownloaderManager::new(smart_client.clone(), 2, 1, 3, 100, 8)
            .await
            .expect("DownloaderManager");
        let (refresh_tx, _) = tokio::sync::broadcast::channel(16);
        let cover_retry_queue = Arc::new(tokio::sync::Mutex::new(HashSet::new()));

        let service = Arc::new(AppService {
            db: pool.clone(),
            wasm_runtime,
            sources: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            settings: Arc::new(tokio::sync::RwLock::new(Settings {
                flaresolverr_url: String::new(),
                library_path: std::env::temp_dir(),
                wasm_storage_path: std::env::temp_dir(),
                concurrent_page_downloads: 4,
                concurrent_manga_downloads: 2,
                chapter_queue_size: 8,
                max_retries: 3,
                initial_retry_delay_ms: 100,
                max_wasm_instances: 10,
                auto_scan: false,
                scan_interval_minutes: 60,
                default_tracking_enabled: true,
            })),
            downloader,
            smart_client: SmartClient::new(None).expect("SmartClient"),
            proxy_client: SmartClient::new(None).expect("SmartClient proxy"),
            refresh_tx,
            refresh_task: Arc::new(tokio::sync::Mutex::new(None)),
            tracker_registry: Arc::new(tokio::sync::RwLock::new(
                kani_app::service::trackers::TrackerRegistry::new(&pool)
                    .await
                    .expect("TrackerRegistry"),
            )),
            cache: RequestCache::new(),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
            cover_retry_queue,
        });

        let restart_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));

        AppState {
            service,
            proxy_secret: Arc::new([0u8; 32]),
            proxy_semaphores: moka::future::Cache::builder().max_capacity(100).build(),
            boot_id: "test-boot-id".to_string(),
            restart_requested,
        }
    }

    /// Builds a full test router with an in-memory DB, a default test user, and
    /// the auth + session layers wired up — matching the production setup in main.rs
    /// but without rate-limiting or CORS.
    ///
    /// Returns `(router, username, password)`.
    async fn test_router() -> (axum::Router, String, String) {
        let pool = crate::auth::test_db().await;
        let backend = crate::auth::AuthBackend::new(pool.clone());
        backend
            .create_user("alice", "alice@test.com", "hunter2")
            .await
            .expect("create test user");

        let session_store = SqliteStore::new(pool.clone());
        session_store.migrate().await.expect("session migrate");

        let session_layer = SessionManagerLayer::new(session_store)
            .with_secure(false)
            .with_http_only(true)
            .with_same_site(SameSite::Lax);

        let auth_layer = AuthManagerLayerBuilder::new(
            crate::auth::AuthBackend::new(pool.clone()),
            session_layer,
        )
        .build();

        let state = test_app_state(pool).await;
        let router = routes(state).layer(auth_layer);
        (router, "alice".to_string(), "hunter2".to_string())
    }

    fn login_request(username: &str, password: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"username": username, "password": password}).to_string(),
            ))
            .unwrap()
    }

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Log in and return the raw `id=value` portion of the Set-Cookie header.
    async fn login_and_get_cookie(app: axum::Router, username: &str, password: &str) -> String {
        let resp = app
            .oneshot(login_request(username, password))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "expected login to succeed");
        resp.headers()
            .get("set-cookie")
            .expect("Set-Cookie header")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    // ── login ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn login_valid_credentials_returns_200() {
        let (app, user, pass) = test_router().await;
        let resp = app.oneshot(login_request(&user, &pass)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_sets_session_cookie() {
        let (app, user, pass) = test_router().await;
        let resp = app.oneshot(login_request(&user, &pass)).await.unwrap();
        assert!(resp.headers().contains_key("set-cookie"));
    }

    #[tokio::test]
    async fn login_response_body_has_ok_true() {
        let (app, user, pass) = test_router().await;
        let resp = app.oneshot(login_request(&user, &pass)).await.unwrap();
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["ok"], true);
    }

    #[tokio::test]
    async fn login_wrong_password_returns_401() {
        let (app, user, _) = test_router().await;
        let resp = app
            .oneshot(login_request(&user, "wrongpass"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_unknown_user_returns_401() {
        let (app, _, _) = test_router().await;
        let resp = app
            .oneshot(login_request("nobody", "whatever"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_failure_body_has_error_field() {
        let (app, user, _) = test_router().await;
        let resp = app.oneshot(login_request(&user, "bad")).await.unwrap();
        let json = body_json(resp.into_body()).await;
        assert!(json["error"].is_string());
    }

    // ── auth/me ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn auth_me_without_session_returns_401() {
        let (app, _, _) = test_router().await;
        let req = axum::http::Request::builder()
            .uri("/auth/me")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_me_with_session_returns_200() {
        let (app, user, pass) = test_router().await;
        let cookie = login_and_get_cookie(app.clone(), &user, &pass).await;

        let req = axum::http::Request::builder()
            .uri("/auth/me")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn auth_me_response_contains_username() {
        let (app, user, pass) = test_router().await;
        let cookie = login_and_get_cookie(app.clone(), &user, &pass).await;

        let req = axum::http::Request::builder()
            .uri("/auth/me")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["username"], user.as_str());
    }

    // ── logout ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn logout_returns_200() {
        let (app, user, pass) = test_router().await;
        let cookie = login_and_get_cookie(app.clone(), &user, &pass).await;

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/auth/logout")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn after_logout_old_session_is_rejected() {
        let (app, user, pass) = test_router().await;
        let cookie = login_and_get_cookie(app.clone(), &user, &pass).await;

        // Logout.
        let logout = axum::http::Request::builder()
            .method("POST")
            .uri("/auth/logout")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        app.clone().oneshot(logout).await.unwrap();

        // Old cookie should no longer grant access.
        let req = axum::http::Request::builder()
            .uri("/auth/me")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── boot_id ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn boot_id_with_session_returns_expected_value() {
        let (app, user, pass) = test_router().await;
        let cookie = login_and_get_cookie(app.clone(), &user, &pass).await;

        let req = axum::http::Request::builder()
            .uri("/boot_id")
            .header("cookie", &cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["boot_id"], "test-boot-id");
    }

    // ── smoke: unauthenticated protected routes return 401 ────────────────────

    async fn assert_401_without_session(uri: &'static str) {
        let (app, _, _) = test_router().await;
        let req = axum::http::Request::builder()
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "expected 401 for unauthenticated GET {uri}"
        );
    }

    #[tokio::test]
    async fn sources_unauthenticated_returns_401() {
        assert_401_without_session("/sources").await;
    }

    #[tokio::test]
    async fn library_unauthenticated_returns_401() {
        assert_401_without_session("/library").await;
    }

    #[tokio::test]
    async fn settings_unauthenticated_returns_401() {
        assert_401_without_session("/settings").await;
    }

    #[tokio::test]
    async fn categories_unauthenticated_returns_401() {
        assert_401_without_session("/categories").await;
    }

    #[tokio::test]
    async fn recent_updates_unauthenticated_returns_401() {
        assert_401_without_session("/recent_updates").await;
    }

    #[tokio::test]
    async fn global_search_unauthenticated_returns_401() {
        assert_401_without_session("/global_search").await;
    }

    #[tokio::test]
    async fn boot_id_unauthenticated_returns_401() {
        assert_401_without_session("/boot_id").await;
    }
}
