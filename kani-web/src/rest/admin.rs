//! Administration, user/role management, logs, audit & maintenance routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/server/stop", post(server_stop))
        .route("/server/restart", post(server_restart))
        .route(
            "/admin/users",
            get(admin_list_users).post(admin_create_user),
        )
        .route(
            "/admin/users/{id}",
            patch(admin_update_user).delete(admin_delete_user),
        )
        .route("/admin/users/{id}/roles", post(admin_grant_role))
        .route("/admin/users/{id}/roles/{role}", delete(admin_revoke_role))
        .route("/admin/users/{id}/activity", get(admin_user_activity))
        .route(
            "/admin/roles",
            get(admin_list_roles).post(admin_create_role),
        )
        .route(
            "/admin/roles/{slug}",
            patch(admin_update_role).delete(admin_delete_role),
        )
        .route("/admin/maintenance", post(run_maintenance))
        .route("/admin/db/stats", get(db_stats))
        .route("/admin/db/analyze", post(db_analyze))
        .route("/admin/db/vacuum", post(db_vacuum))
        .route("/admin/recurring/{kind}/run", post(trigger_recurring))
        .route("/admin/cache/clear", post(clear_cache))
        .route("/admin/scan/stop", post(stop_scan))
        .route("/admin/email/test", post(admin_send_test_email))
        .route(
            "/admin/users/{id}/password-reset",
            post(admin_trigger_password_reset_handler),
        )
        .route(
            "/admin/credentials/status",
            get(get_credential_encryption_status_handler),
        )
        .route(
            "/admin/credentials/encrypt",
            post(migrate_credentials_handler),
        )
        .route("/admin/logs", get(admin_logs))
        .route("/admin/logs/stream", get(admin_logs_stream))
        .route("/admin/logs/download", get(admin_logs_download))
        .route("/admin/logs/purge", post(admin_purge_logs))
        .route("/admin/audit-log", get(admin_audit_log))
        .route("/admin/audit-log/download", get(admin_audit_log_download))
        .route("/admin/fs/browse", get(fs_browse_handler))
        .route("/admin/fs/mkdir", post(fs_mkdir_handler))
        .route("/admin/path/estimate", post(path_migrate_estimate_handler))
        .route("/admin/path/migrate", post(path_migrate_handler))
        .route("/system/capabilities", get(system_capabilities))
        .route(
            "/admin/sources/blocked-repos",
            get(list_blocked_repos_handler).post(block_repo_handler),
        )
        .route(
            "/admin/sources/blocked-repos/{id}",
            delete(delete_blocked_repo_handler),
        )
        .route("/admin/diagnostics", get(admin_diagnostics))
        .route("/admin/support-bundle", get(admin_support_bundle))
        .route("/admin/proxy/stats", get(proxy_bandwidth_stats))
        .route("/admin/sources/circuits", get(list_source_circuits))
        .route(
            "/admin/sources/circuits/{host}/reset",
            post(reset_source_circuit),
        )
        .route(
            "/admin/backup/schedule",
            get(admin_get_backup_schedule).put(admin_put_backup_schedule),
        )
        .route("/admin/backup/run-now", post(admin_backup_run_now))
        .route("/admin/storage/stats", get(admin_storage_stats))
        .route(
            "/admin/storage/stats/history",
            get(admin_storage_stats_history),
        )
        .route(
            "/admin/library/integrity-check",
            post(admin_integrity_check),
        )
}

#[utoipa::path(
    post, path = "/rest/server/stop",
    responses(
        (status = 200, description = "Server shutdown initiated"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn server_stop(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(user_id = user.id.0, username = %user.username, "Server stop requested");
    state.audit(Some(user.id), "server.stop", None, None).await;
    state.shutdown_token.cancel();
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    post, path = "/rest/server/restart",
    responses(
        (status = 200, description = "Server restart initiated"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn server_restart(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    use std::sync::atomic::Ordering;
    tracing::info!(user_id = user.id.0, username = %user.username, "Server restart requested");
    state
        .audit(Some(user.id), "server.restart", None, None)
        .await;
    state.restart_requested.store(true, Ordering::Relaxed);
    state.shutdown_token.cancel();
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    get, path = "/rest/admin/users",
    responses(
        (status = 200, description = "All user accounts"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_list_users(
    _: AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    let users = backend.list_users().await?;
    Ok(Json(users))
}

#[utoipa::path(
    post, path = "/rest/admin/users",
    request_body = AdminCreateUserRequest,
    responses(
        (status = 201, description = "User created"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_create_user(
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
    let user = backend
        .create_user(&body.username, &body.email, &body.password)
        .await?;
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

#[utoipa::path(
    patch, path = "/rest/admin/users/{id}",
    params(("id" = i64, Path, description = "User ID")),
    request_body = AdminUpdateUserRequest,
    responses(
        (status = 200, description = "User updated"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_update_user(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
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
    Ok(Json(backend.fetch_user_by_id(user_id).await?.ok_or_else(
        || AppError::NotFound("User not found".into()),
    )?))
}

#[utoipa::path(
    delete, path = "/rest/admin/users/{id}",
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 204, description = "User deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_delete_user(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
) -> Result<impl IntoResponse, AppError> {
    if user_id == admin.id {
        return Err(AppError::ValidationError(
            "Cannot delete your own account".into(),
        ));
    }
    let backend = AuthBackend::new(state.db.clone());
    // Prevent deleting the last admin user — would lock everyone out.
    let target = backend
        .fetch_user_by_id(user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;
    if target.roles.iter().any(|r| r == "admin") {
        let admin_count = backend.count_users_with_role("admin").await?;
        if admin_count <= 1 {
            return Err(AppError::ValidationError(
                "Cannot delete the only admin account".into(),
            ));
        }
    }
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

#[utoipa::path(
    post, path = "/rest/admin/users/{id}/roles",
    params(("id" = i64, Path, description = "User ID")),
    request_body = AdminGrantRoleRequest,
    responses(
        (status = 204, description = "Role granted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_grant_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
    Json(body): Json<AdminGrantRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend
        .grant_role(user_id, &body.role_slug, Some(admin.id))
        .await?;
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

#[utoipa::path(
    delete, path = "/rest/admin/users/{id}/roles/{role}",
    params(
        ("id" = i64, Path, description = "User ID"),
        ("role" = String, Path, description = "Role slug to revoke"),
    ),
    responses(
        (status = 204, description = "Role revoked"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_revoke_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path((user_id, role_slug)): Path<(UserId, String)>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    // Prevent revoking the admin role from the last admin — would lock everyone out.
    if role_slug == "admin" {
        let admin_count = backend.count_users_with_role("admin").await?;
        if admin_count <= 1 {
            return Err(AppError::ValidationError(
                "Cannot remove the admin role from the only admin account".into(),
            ));
        }
    }
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

#[utoipa::path(
    get, path = "/rest/admin/users/{id}/activity",
    params(
        ("id" = i64, Path, description = "User ID"),
        ("limit" = Option<i64>, Query, description = "Max events (default 50, max 200)"),
        ("before" = Option<String>, Query, description = "Cursor: ISO timestamp to paginate before"),
    ),
    responses(
        (status = 200, description = "Recent audit events for this user"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_user_activity(
    _: AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
    Query(q): Query<UserActivityQuery>,
) -> Result<impl IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let rows = if let Some(before) = q.before {
        sqlx::query_as!(
            ActivityEvent,
            r#"SELECT id, action, target, details, created_at
               FROM audit_log
               WHERE user_id = ? AND created_at < ?
               ORDER BY created_at DESC
               LIMIT ?"#,
            user_id,
            before,
            limit,
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as!(
            ActivityEvent,
            r#"SELECT id, action, target, details, created_at
               FROM audit_log
               WHERE user_id = ?
               ORDER BY created_at DESC
               LIMIT ?"#,
            user_id,
            limit,
        )
        .fetch_all(&state.db)
        .await?
    };
    Ok(Json(UserActivityResponse { events: rows }))
}

#[utoipa::path(
    get, path = "/rest/admin/roles",
    responses(
        (status = 200, description = "All defined roles"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_list_roles(
    _: AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    let roles = backend.list_roles().await?;
    Ok(Json(roles))
}

#[utoipa::path(
    post, path = "/rest/admin/roles",
    request_body = AdminCreateRoleRequest,
    responses(
        (status = 201, description = "Role created"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_create_role(
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
        .audit(Some(admin.id), "admin.role.create", Some(&body.slug), None)
        .await;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    patch, path = "/rest/admin/roles/{slug}",
    params(("slug" = String, Path, description = "Role slug")),
    request_body = AdminUpdateRoleRequest,
    responses(
        (status = 204, description = "Role updated"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_update_role(
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
        .audit(Some(admin.id), "admin.role.update", Some(&slug), None)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete, path = "/rest/admin/roles/{slug}",
    params(("slug" = String, Path, description = "Role slug")),
    responses(
        (status = 204, description = "Role deleted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_delete_role(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let backend = AuthBackend::new(state.db.clone());
    backend.delete_role(&slug).await?;
    state
        .audit(Some(admin.id), "admin.role.delete", Some(&slug), None)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/rest/admin/maintenance",
    responses(
        (status = 200, description = "Enqueues analyze and vacuum jobs; returns job IDs"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn run_maintenance(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::jobs::recurring::{RecurringJobKind, trigger_now};
    let analyze_id = trigger_now(&state, RecurringJobKind::DbMaintenance).await?;
    let vacuum_id = trigger_now(&state, RecurringJobKind::DbVacuum).await?;
    Ok(Json(
        json!({ "analyze_job_id": analyze_id, "vacuum_job_id": vacuum_id }),
    ))
}

/// Generic manual trigger for any recurring-job kind. Submits the kind's job
/// immediately without disturbing its schedule; returns the job id (or a
/// conflict if a singleton kind is already running, or 404 for an unknown kind).
pub(crate) async fn trigger_recurring(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
    Path(kind): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let Some(k) = kani_app::jobs::recurring::RecurringJobKind::parse(&kind) else {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown_recurring_kind" })),
        )
            .into_response());
    };
    match kani_app::jobs::recurring::trigger_now(&state, k).await? {
        Some(job_id) => Ok(Json(json!({ "job_id": job_id })).into_response()),
        None => Ok((
            StatusCode::CONFLICT,
            Json(json!({ "error": "already_running" })),
        )
            .into_response()),
    }
}

#[utoipa::path(
    get, path = "/rest/admin/db/stats",
    responses(
        (status = 200, description = "SQLite page/WAL size statistics"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn db_stats(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let (page_count,): (i64,) = sqlx::query_as("PRAGMA page_count")
        .fetch_one(&state.db)
        .await
        .map_err(kani_app::error::ServiceError::Db)?;
    let (page_size,): (i64,) = sqlx::query_as("PRAGMA page_size")
        .fetch_one(&state.db)
        .await
        .map_err(kani_app::error::ServiceError::Db)?;
    let db_size_bytes = page_count * page_size;
    let wal_size_bytes = std::fs::metadata("kani.db-wal")
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    Ok(Json(json!({
        "db_size_bytes": db_size_bytes,
        "wal_size_bytes": wal_size_bytes,
        "page_count": page_count,
        "page_size": page_size,
    })))
}

#[utoipa::path(
    post, path = "/rest/admin/db/analyze",
    responses(
        (status = 200, description = "Enqueues WAL checkpoint + ANALYZE job; returns job_id"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn db_analyze(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = kani_app::jobs::recurring::trigger_now(
        &state,
        kani_app::jobs::recurring::RecurringJobKind::DbMaintenance,
    )
    .await?;
    Ok(Json(json!({ "job_id": job_id })))
}

#[utoipa::path(
    post, path = "/rest/admin/db/vacuum",
    responses(
        (status = 200, description = "Enqueues VACUUM job; returns job_id"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn db_vacuum(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = kani_app::jobs::recurring::trigger_now(
        &state,
        kani_app::jobs::recurring::RecurringJobKind::DbVacuum,
    )
    .await?;
    Ok(Json(json!({ "job_id": job_id })))
}

#[utoipa::path(
    post, path = "/rest/admin/cache/clear",
    responses(
        (status = 200, description = "In-memory cache cleared"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn clear_cache(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.cache.clear_all();
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    post, path = "/rest/admin/scan/stop",
    responses(
        (status = 200, description = "Active background scan aborted"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn stop_scan(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.abort_refresh().await;
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    post, path = "/rest/admin/email/test",
    request_body = SendTestEmailBody,
    responses(
        (status = 200, description = "Test email sent successfully"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_send_test_email(
    _: AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Json(body): Json<SendTestEmailBody>,
) -> Result<impl IntoResponse, AppError> {
    state
        .send_test_email_to(&body.to)
        .await
        .map_err(AppError::EmailError)?;
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    post, path = "/rest/admin/users/{id}/password-reset",
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "Password reset email triggered for the user"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_trigger_password_reset_handler(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
) -> Result<impl IntoResponse, AppError> {
    state
        .admin_trigger_password_reset(user_id, admin.id)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    get, path = "/rest/admin/credentials/status",
    responses(
        (status = 200, description = "Credential encryption status"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn get_credential_encryption_status_handler(
    _: AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let status = state.get_encryption_status().await?;
    Ok(Json(status))
}

#[utoipa::path(
    post, path = "/rest/admin/credentials/encrypt",
    responses(
        (status = 200, description = "Credentials migrated to encrypted storage"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn migrate_credentials_handler(
    _: AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.migrate_credentials_to_encrypted().await?;
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    get, path = "/rest/admin/logs",
    params(
        ("level" = Option<String>, Query, description = "Comma-separated log levels (e.g. error,warn)"),
        ("source" = Option<String>, Query, description = "Comma-separated source modules"),
        ("from" = Option<String>, Query, description = "Start timestamp (ISO 8601)"),
        ("to" = Option<String>, Query, description = "End timestamp (ISO 8601)"),
        ("search" = Option<String>, Query, description = "Full-text search"),
        ("page" = Option<i32>, Query, description = "Page number"),
        ("page_size" = Option<i32>, Query, description = "Results per page"),
    ),
    responses(
        (status = 200, description = "Paginated log entries"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_logs(
    _: AuthGuard<crate::permissions::guards::AdminViewLogs>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<crate::models::LogsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let levels = parse_csv(q.level.as_deref());
    let sources = parse_csv(q.source.as_deref());
    let page = q.page.unwrap_or(1) as usize;
    let page_size = q.page_size.unwrap_or(100) as usize;

    let (entries, total) = state.log_handle.query(
        &levels,
        &sources,
        q.from.as_deref(),
        q.to.as_deref(),
        q.search.as_deref(),
        page,
        page_size,
    );

    Ok(Json(serde_json::json!({
        "entries": entries,
        "total": total,
        "page": page,
        "page_size": page_size,
    })))
}

#[utoipa::path(
    get, path = "/rest/admin/logs/stream",
    params(
        ("level" = Option<String>, Query, description = "Comma-separated log levels to filter"),
        ("source" = Option<String>, Query, description = "Comma-separated source modules to filter"),
    ),
    responses(
        (status = 200, description = "Server-sent event stream of live log entries (text/event-stream)"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_logs_stream(
    _: AuthGuard<crate::permissions::guards::AdminViewLogs>,
    State(state): State<AppState>,
    Query(q): Query<crate::models::LogsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let levels = parse_csv(q.level.as_deref());
    let sources = parse_csv(q.source.as_deref());

    let rx = state.log_handle.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(move |result| match result {
        Ok(entry) => {
            if !levels.is_empty() && !levels.iter().any(|l| l.eq_ignore_ascii_case(&entry.level)) {
                return None;
            }
            if !sources.is_empty()
                && !sources
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&entry.source))
            {
                return None;
            }
            let json = serde_json::to_string(&entry).ok()?;
            Some(Ok::<Event, Infallible>(Event::default().data(json)))
        }
        Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
            tracing::warn!("Log SSE lagged by {} events", n);
            None
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[utoipa::path(
    get, path = "/rest/admin/logs/download",
    params(
        ("format" = Option<String>, Query, description = "json or plain (default plain)"),
        ("level" = Option<String>, Query, description = "Comma-separated log levels"),
        ("source" = Option<String>, Query, description = "Comma-separated source modules"),
        ("from" = Option<String>, Query, description = "Start timestamp"),
        ("to" = Option<String>, Query, description = "End timestamp"),
        ("search" = Option<String>, Query, description = "Full-text search"),
    ),
    responses(
        (status = 200, description = "Log file download (text/plain or application/json)"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_logs_download(
    _: AuthGuard<crate::permissions::guards::AdminViewLogs>,
    State(state): State<AppState>,
    Query(q): Query<crate::models::LogsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let levels = parse_csv(q.level.as_deref());
    let sources = parse_csv(q.source.as_deref());
    let entries = state.log_handle.query_all(
        &levels,
        &sources,
        q.from.as_deref(),
        q.to.as_deref(),
        q.search.as_deref(),
    );

    let today = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".into());
    let filename = format!("kani-logs-{}.txt", &today[..10]);

    let (body, content_type) = match q.format.as_deref() {
        Some("json") => {
            let json = serde_json::to_string(&entries)
                .map_err(|e| AppError::InternalServerError(e.to_string()))?;
            (json, "application/json")
        }
        _ => {
            let text = entries
                .iter()
                .map(|e| format!("[{}] {} {}: {}", e.timestamp, e.level, e.target, e.message))
                .collect::<Vec<_>>()
                .join("\n");
            (text, "text/plain; charset=utf-8")
        }
    };

    let disposition = format!("attachment; filename=\"{filename}\"");
    axum::response::Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, &disposition)
        .body(Body::from(body))
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

#[utoipa::path(
    post, path = "/rest/admin/logs/purge",
    responses(
        (status = 204, description = "In-memory log buffer cleared"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_purge_logs(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::AdminViewLogs>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    state.log_handle.clear();
    state
        .audit(Some(user.id), "admin.logs.purged", None, None)
        .await;
    StatusCode::NO_CONTENT
}

#[utoipa::path(
    get, path = "/rest/admin/audit-log",
    params(
        ("user_id" = Option<i64>, Query, description = "Filter by user ID"),
        ("action" = Option<String>, Query, description = "Filter by action name"),
        ("from" = Option<String>, Query, description = "Start timestamp (ISO 8601)"),
        ("to" = Option<String>, Query, description = "End timestamp (ISO 8601)"),
        ("search" = Option<String>, Query, description = "Full-text search"),
        ("page" = Option<i32>, Query, description = "Page number"),
        ("page_size" = Option<i32>, Query, description = "Results per page"),
    ),
    responses(
        (status = 200, description = "Paginated audit log entries"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_audit_log(
    _: AuthGuard<crate::permissions::guards::AdminViewAudit>,
    State(state): State<AppState>,
    ValidatedQuery(q): ValidatedQuery<crate::models::AuditLogQuery>,
) -> Result<impl IntoResponse, AppError> {
    let page = q.page.unwrap_or(1);
    let page_size = q.page_size.unwrap_or(50);

    let (entries, has_next, total_pages) = state
        .get_audit_log(
            q.user_id,
            q.action.as_deref(),
            q.from.as_deref(),
            q.to.as_deref(),
            q.search.as_deref(),
            page,
            page_size,
        )
        .await?;

    Ok(Json(serde_json::json!({
        "entries": entries,
        "has_next": has_next,
        "page": page,
        "page_size": page_size,
        "total_pages": total_pages,
    })))
}

#[utoipa::path(
    get, path = "/rest/admin/audit-log/download",
    params(
        ("format" = Option<String>, Query, description = "csv or json (default csv)"),
        ("user_id" = Option<i64>, Query, description = "Filter by user ID"),
        ("action" = Option<String>, Query, description = "Filter by action name"),
    ),
    responses(
        (status = 200, description = "Audit log file download (CSV or JSON)"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_audit_log_download(
    _: AuthGuard<crate::permissions::guards::AdminViewAudit>,
    State(state): State<AppState>,
    Query(q): Query<crate::models::AuditLogQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (entries, _, _) = state
        .get_audit_log(
            q.user_id,
            q.action.as_deref(),
            q.from.as_deref(),
            q.to.as_deref(),
            q.search.as_deref(),
            1,
            10_000,
        )
        .await?;

    let today = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".into());

    let (body, content_type, filename) = match q.format.as_deref() {
        Some("json") => {
            let json = serde_json::to_string(&entries)
                .map_err(|e| AppError::InternalServerError(e.to_string()))?;
            (
                json,
                "application/json",
                format!("kani-audit-{}.json", &today[..10]),
            )
        }
        _ => {
            let mut csv = String::from("id,timestamp,user,action,target,details\n");
            for e in &entries {
                csv.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    e.id,
                    e.created_at,
                    csv_escape(e.username.as_deref().unwrap_or("")),
                    csv_escape(&e.action),
                    csv_escape(e.target.as_deref().unwrap_or("")),
                    csv_escape(e.details.as_deref().unwrap_or("")),
                ));
            }
            (
                csv,
                "text/csv; charset=utf-8",
                format!("kani-audit-{}.csv", &today[..10]),
            )
        }
    };

    let disposition = format!("attachment; filename=\"{filename}\"");
    axum::response::Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, &disposition)
        .body(Body::from(body))
        .map_err(|e| AppError::InternalServerError(e.to_string()))
}

#[utoipa::path(
    get, path = "/rest/admin/fs/browse",
    params(("path" = String, Query, description = "Directory path to browse")),
    responses(
        (status = 200, description = "Directory listing with path segments and subdirectories"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn fs_browse_handler(
    _: AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    ValidatedQuery(q): ValidatedQuery<crate::models::FsBrowseQuery>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::service::fs_browse;
    let result = tokio::task::spawn_blocking(move || fs_browse::browse_directory(&q.path))
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))??;
    Ok(Json(crate::models::FsBrowseResponse {
        path: result.canonical_path.to_string_lossy().into_owned(),
        segments: result.segments,
        dirs: result.dirs,
        drives: result.drives,
    }))
}

#[utoipa::path(
    post, path = "/rest/admin/fs/mkdir",
    request_body = crate::models::FsMkdirBody,
    responses(
        (status = 200, description = "Directory created; returns new path"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn fs_mkdir_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Json(body): Json<crate::models::FsMkdirBody>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::service::fs_browse;
    let parent = body.path.clone();
    let name = body.name.clone();
    let new_path = tokio::task::spawn_blocking(move || fs_browse::create_directory(&parent, &name))
        .await
        .map_err(|e| AppError::InternalServerError(e.to_string()))??;
    let path_str = new_path.to_string_lossy().into_owned();
    state
        .audit(Some(user.id), "fs.mkdir", Some(&path_str), None)
        .await;
    Ok(Json(crate::models::FsMkdirResponse { path: path_str }))
}

#[utoipa::path(
    post, path = "/rest/admin/path/estimate",
    request_body = crate::models::PathMigrateBody,
    responses(
        (status = 200, description = "Disk-space estimate for the path migration"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn path_migrate_estimate_handler(
    _: AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Json(body): Json<crate::models::PathMigrateBody>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::service::path_migration;
    let current = resolve_path_field(&state, &body.field).await?;
    let new = std::path::PathBuf::from(&body.new_path);
    let estimate = path_migration::estimate_path_migration(&current, &new).await?;
    Ok(Json(crate::models::PathMigrateEstimateResponse {
        current_bytes: estimate.current_bytes,
        available_bytes: estimate.available_bytes,
        can_migrate: estimate.can_migrate,
        reason: estimate.reason,
    }))
}

#[utoipa::path(
    post, path = "/rest/admin/path/migrate",
    request_body = crate::models::PathMigrateBody,
    responses(
        (status = 202, description = "Path migration started in background"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn path_migrate_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Json(body): Json<crate::models::PathMigrateBody>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::service::path_migration;

    let current = resolve_path_field(&state, &body.field).await?;
    let new = std::path::PathBuf::from(&body.new_path);

    let estimate = path_migration::estimate_path_migration(&current, &new).await?;
    if !estimate.can_migrate {
        return Err(AppError::ValidationError(
            estimate
                .reason
                .unwrap_or_else(|| "migration not possible".into()),
        ));
    }

    let service = state.service.as_ref().clone();
    path_migration::spawn_path_migration(service, body.field, current, new, user.id);

    Ok((StatusCode::ACCEPTED, Json(json!({ "started": true }))))
}

#[utoipa::path(
    get, path = "/rest/system/capabilities",
    responses(
        (status = 200, description = "Server capability flags (e.g. KCC availability)"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("session" = [])),
    tag = "system"
)]
pub(crate) async fn system_capabilities(
    _: AuthGuard<crate::permissions::guards::LibraryView>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let kcc_version = kani_app::AppService::kcc_version().await;
    Json(serde_json::json!({
        "kcc": kcc_version.is_some(),
        "kcc_version": kcc_version,
    }))
}

#[utoipa::path(
    get, path = "/rest/admin/sources/blocked-repos",
    responses(
        (status = 200, description = "All blocked repository URLs"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn list_blocked_repos_handler(
    _: AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.list_blocked_repos().await?))
}

#[utoipa::path(
    post, path = "/rest/admin/sources/blocked-repos",
    request_body = BlockRepoRequest,
    responses(
        (status = 204, description = "Repository blocked"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn block_repo_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
    ValidatedJson(payload): ValidatedJson<BlockRepoRequest>,
) -> Result<impl IntoResponse, AppError> {
    state
        .block_repo(&payload.url, &payload.reason, Some(user.id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete, path = "/rest/admin/sources/blocked-repos/{id}",
    params(("id" = i64, Path, description = "Blocked repo ID")),
    responses(
        (status = 204, description = "Blocked repo entry removed"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
        (status = 404, description = "Blocked repo not found"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn delete_blocked_repo_handler(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_blocked_repo(id, Some(user.id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn proxy_bandwidth_stats(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    use std::sync::atomic::Ordering;

    let mut entries: Vec<(String, u64)> = state
        .proxy_bandwidth
        .iter()
        .map(|e| (e.key().clone(), e.value().load(Ordering::Relaxed)))
        .collect();
    entries.sort_unstable_by_key(|e| std::cmp::Reverse(e.1));
    entries.truncate(10);

    let result: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|(host, bytes)| serde_json::json!({ "host": host, "bytes": bytes }))
        .collect();

    Ok(Json(result))
}

pub(crate) async fn list_source_circuits(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(state.smart_client.list_circuits()))
}

pub(crate) async fn reset_source_circuit(
    AuthGuard(..): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
    Path(host): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.smart_client.reset_circuit(&host);
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get, path = "/rest/admin/backup/schedule",
    responses(
        (status = 200, description = "Current backup schedule configuration (passphrase redacted)"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_get_backup_schedule(
    _: AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let mut config = state.service.get_backup_schedule().await?;
    if config.passphrase.is_some() {
        config.passphrase = Some("***".into());
    }
    Ok(Json(config))
}

#[utoipa::path(
    put, path = "/rest/admin/backup/schedule",
    request_body = inline(serde_json::Value),
    responses(
        (status = 200, description = "Backup schedule saved"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_put_backup_schedule(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
    Json(body): Json<kani_app::service::backup_scheduler::BackupScheduleConfig>,
) -> Result<impl IntoResponse, AppError> {
    if body.passphrase.as_deref() == Some("***") {
        let existing = state.service.get_backup_schedule().await?;
        let mut config = body;
        config.passphrase = existing.passphrase;
        state.service.set_backup_schedule(&config).await?;
    } else {
        state.service.set_backup_schedule(&body).await?;
    }
    state
        .audit(Some(user.id), "admin.backup.schedule.update", None, None)
        .await;
    Ok(Json(json!({ "ok": true })))
}

#[utoipa::path(
    post, path = "/rest/admin/backup/run-now",
    responses(
        (status = 200, description = "Backup job submitted; returns job_id"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Admin permission required"),
    ),
    security(("session" = [])),
    tag = "admin"
)]
pub(crate) async fn admin_backup_run_now(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let job_id = kani_app::jobs::recurring::trigger_now(
        &state,
        kani_app::jobs::recurring::RecurringJobKind::ScheduledBackup,
    )
    .await?;
    state
        .audit(Some(user.id), "admin.backup.run_now", None, None)
        .await;
    Ok(Json(json!({ "job_id": job_id })))
}

pub(crate) async fn admin_storage_stats(
    _: AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let stats = state.service.get_storage_stats().await?;
    Ok(Json(stats))
}

pub(crate) async fn admin_storage_stats_history(
    _: AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query!(
        r#"SELECT
            captured_at as "captured_at: String",
            library_used_bytes, cover_used_bytes, chapter_used_bytes,
            data_used_bytes, free_bytes, total_manga, total_chapters
           FROM storage_history
           ORDER BY captured_at DESC
           LIMIT 90"#
    )
    .fetch_all(&state.db)
    .await
    .map_err(kani_app::error::ServiceError::Db)?;

    let data: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "captured_at": r.captured_at,
                "library_used_bytes": r.library_used_bytes,
                "cover_used_bytes": r.cover_used_bytes,
                "chapter_used_bytes": r.chapter_used_bytes,
                "data_used_bytes": r.data_used_bytes,
                "free_bytes": r.free_bytes,
                "total_manga": r.total_manga,
                "total_chapters": r.total_chapters,
            })
        })
        .collect();

    Ok(Json(data))
}

#[derive(serde::Deserialize)]
pub(crate) struct IntegrityCheckQuery {
    pub fix: Option<bool>,
}

pub(crate) async fn admin_integrity_check(
    _: AuthGuard<crate::permissions::guards::AdminManage>,
    State(state): State<AppState>,
    Query(q): Query<IntegrityCheckQuery>,
) -> Result<impl IntoResponse, AppError> {
    let fix = q.fix.unwrap_or(false);
    if fix {
        let result = state.service.cleanup_orphans(false).await?;
        Ok(Json(serde_json::to_value(result).map_err(|e| {
            AppError::InternalServerError(e.to_string())
        })?))
    } else {
        let report = state.service.check_library().await?;
        Ok(Json(serde_json::to_value(report).map_err(|e| {
            AppError::InternalServerError(e.to_string())
        })?))
    }
}

pub(crate) async fn admin_diagnostics(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let mut payload = state.get_diagnostics().await?;
    payload.recent_error_count = state
        .log_handle
        .query_all(&["ERROR".to_string()], &[], None, None, None)
        .len() as u64;
    Ok(Json(payload))
}

pub(crate) async fn admin_support_bundle(
    _: AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let entries = state.log_handle.query_all(&[], &[], None, None, None);
    let mut logs_jsonl = Vec::new();
    for entry in &entries {
        if let Ok(line) = serde_json::to_vec(entry) {
            logs_jsonl.extend_from_slice(&line);
            logs_jsonl.push(b'\n');
        }
    }

    let (bytes, filename) = state.generate_support_bundle(logs_jsonl).await?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        bytes,
    ))
}
