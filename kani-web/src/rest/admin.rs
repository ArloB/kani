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
}

async fn server_stop(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(user_id = user.id.0, username = %user.username, "Server stop requested");
    state.audit(Some(user.id), "server.stop", None, None).await;
    state.shutdown_token.cancel();
    Ok(Json(json!({ "ok": true })))
}

async fn server_restart(
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

async fn admin_update_user(
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

async fn admin_delete_user(
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

async fn admin_grant_role(
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

async fn admin_revoke_role(
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

async fn admin_user_activity(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::UserManage>,
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
        .audit(Some(admin.id), "admin.role.create", Some(&body.slug), None)
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
        .audit(Some(admin.id), "admin.role.update", Some(&slug), None)
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
        .audit(Some(admin.id), "admin.role.delete", Some(&slug), None)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn run_maintenance(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let (before_bytes, after_bytes) = state.run_maintenance().await?;
    Ok(Json(
        json!({ "before_bytes": before_bytes, "after_bytes": after_bytes }),
    ))
}

async fn clear_cache(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.cache.clear_all();
    Ok(Json(json!({ "ok": true })))
}

async fn stop_scan(
    AuthGuard(_, _): AuthGuard<crate::permissions::guards::ServerManage>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.abort_refresh().await;
    Ok(Json(json!({ "ok": true })))
}

async fn admin_send_test_email(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
    Json(body): Json<SendTestEmailBody>,
) -> Result<impl IntoResponse, AppError> {
    state
        .send_test_email_to(&body.to)
        .await
        .map_err(AppError::EmailError)?;
    Ok(Json(json!({ "ok": true })))
}

async fn admin_trigger_password_reset_handler(
    AuthGuard(admin, _): AuthGuard<crate::permissions::guards::UserManage>,
    State(state): State<AppState>,
    Path(user_id): Path<UserId>,
) -> Result<impl IntoResponse, AppError> {
    state
        .admin_trigger_password_reset(user_id, admin.id)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

async fn get_credential_encryption_status_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let status = state.get_encryption_status().await?;
    Ok(Json(status))
}

async fn migrate_credentials_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    state.migrate_credentials_to_encrypted().await?;
    Ok(Json(json!({ "ok": true })))
}

async fn admin_logs(
    AuthGuard(..): AuthGuard<crate::permissions::guards::AdminViewLogs>,
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

async fn admin_logs_stream(
    AuthGuard(..): AuthGuard<crate::permissions::guards::AdminViewLogs>,
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

async fn admin_logs_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::AdminViewLogs>,
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

/// POST /admin/logs/purge — clear the in-memory log ring buffer immediately.
async fn admin_purge_logs(
    AuthGuard(user, _): AuthGuard<crate::permissions::guards::AdminViewLogs>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    state.log_handle.clear();
    state
        .audit(Some(user.id), "admin.logs.purged", None, None)
        .await;
    StatusCode::NO_CONTENT
}

async fn admin_audit_log(
    AuthGuard(..): AuthGuard<crate::permissions::guards::AdminViewAudit>,
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

async fn admin_audit_log_download(
    AuthGuard(..): AuthGuard<crate::permissions::guards::AdminViewAudit>,
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

async fn fs_browse_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
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

async fn fs_mkdir_handler(
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

async fn path_migrate_estimate_handler(
    AuthGuard(..): AuthGuard<crate::permissions::guards::SettingsEditAdvanced>,
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

async fn path_migrate_handler(
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

async fn system_capabilities(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let kcc_version = kani_app::AppService::kcc_version().await;
    Json(serde_json::json!({
        "kcc": kcc_version.is_some(),
        "kcc_version": kcc_version,
    }))
}
