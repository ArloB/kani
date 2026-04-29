/// Adds `Cache-Control: public, max-age=31536000, immutable` to all `/js/*` and `/css/*`
/// responses. Only compiled into release builds — dev builds serve files uncached so that
/// changes are visible immediately without a hard refresh.
#[cfg(not(debug_assertions))]
async fn cache_control_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use rquest::header;

    let is_static =
        request.uri().path().starts_with("/js/") || request.uri().path().starts_with("/css/");
    let mut response = next.run(request).await;
    if is_static {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}

#[tokio::main]
async fn main() {
    use axum::Router;
    use axum::http::{HeaderValue, Method, header};
    use axum_login::{
        AuthManagerLayerBuilder,
        tower_sessions::{SessionManagerLayer, cookie::SameSite},
    };
    use kani_web::{auth::AuthBackend, rest, state::AppState};
    use std::sync::Arc;
    use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
    use tower_http::{
        compression::CompressionLayer,
        cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer},
        services::{ServeDir, ServeFile},
        set_header::SetResponseHeaderLayer,
        trace::TraceLayer,
    };
    use tower_sessions_sqlx_store::SqliteStore;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,axum_login=warn,tower_sessions=warn,tower_sessions_core=warn,sqlx=warn",
                )
            }),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    tracing::info!("Starting Kani Web Server");

    let state = AppState::new()
        .await
        .expect("Failed to initialise AppState");

    let session_store = SqliteStore::new(state.db.clone());
    session_store.migrate().await.unwrap();

    let secure_cookies = std::env::var("KANI_SECURE_COOKIES")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(secure_cookies)
        .with_http_only(true)
        .with_same_site(SameSite::Lax);

    let auth_backend = AuthBackend::new(state.db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend.clone(), session_layer).build();

    if let Err(e) = ensure_default_user(&auth_backend).await {
        tracing::error!("Failed to ensure default user: {}", e);
    }

    // Initialise HTTP logging toggle from persisted setting.
    kani_web::HTTP_LOGGING_ENABLED.store(
        state.get_settings().await.http_request_logging,
        std::sync::atomic::Ordering::Relaxed,
    );

    state.spawn_auto_scan();
    state.spawn_cover_retry();

    // Rate limiter settings.
    // API: enough for normal UI use while protecting against abuse.
    // Proxy: much more permissive — the reader fires many concurrent image
    // requests; the per-host semaphore in rest.rs already throttles upstream.
    const API_RATE_PER_SECOND: u64 = 5;
    const API_BURST_SIZE: u32 = 200;
    const PROXY_RATE_PER_SECOND: u64 = 30;
    const PROXY_BURST_SIZE: u32 = 600;

    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(API_RATE_PER_SECOND)
            .burst_size(API_BURST_SIZE)
            .finish()
            .unwrap(),
    );
    let proxy_governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(PROXY_RATE_PER_SECOND)
            .burst_size(PROXY_BURST_SIZE)
            .finish()
            .unwrap(),
    );

    let cors_layer = {
        let allow_origin = std::env::var("KANI_CORS_ORIGIN")
            .map(|origin| {
                origin
                    .parse::<HeaderValue>()
                    .map(AllowOrigin::exact)
                    .unwrap_or_else(|_| AllowOrigin::mirror_request())
            })
            .unwrap_or_else(|_| AllowOrigin::mirror_request());

        CorsLayer::new()
            .allow_origin(allow_origin)
            .allow_methods(AllowMethods::list([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ]))
            .allow_headers(AllowHeaders::mirror_request())
            .allow_credentials(true)
    };

    let rest_router = rest::routes(state.clone());
    let proxy_router = rest::image_proxy_route(state.clone());
    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(rest::health))
        .route("/ready", axum::routing::get(rest::ready))
        .with_state(state.clone());

    {
        let db = state.db.clone();
        let mut rx = state.downloader.subscribe();
        let listener_state = state.clone();
        let token = state.shutdown_token.clone();
        tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    _ = token.cancelled() => {
                        tracing::info!("Download listener shutting down");
                        break;
                    }
                    result = rx.recv() => result,
                };
                match event {
                    Ok(event) => match event {
                        kani_shared::DownloadProgressEvent::ChapterStarted {
                            chapter_id, ..
                        } => {
                            if let Err(e) = sqlx::query!(
                                "UPDATE chapters SET download_status = 1 WHERE id = ?",
                                chapter_id
                            )
                            .execute(&db)
                            .await
                            {
                                tracing::warn!(
                                    "Failed to update download_status=1 for chapter {}: {}",
                                    chapter_id,
                                    e
                                );
                            }
                        }
                        kani_shared::DownloadProgressEvent::ChapterCompleted {
                            chapter_id, successful_pages, ..
                        } => {
                            let pc = successful_pages as i64;
                            if let Err(e) = sqlx::query!(
                                "UPDATE chapters SET download_status = 2, page_count = ?, downloaded_at = CURRENT_TIMESTAMP WHERE id = ?",
                                pc,
                                chapter_id
                            )
                            .execute(&db)
                            .await
                            {
                                tracing::warn!(
                                    "Failed to update chapter {}: {}",
                                    chapter_id,
                                    e
                                );
                            }
                        }
                        kani_shared::DownloadProgressEvent::ChapterFailed {
                            chapter_id,
                            error,
                            ..
                        } => {
                            tracing::error!("Chapter failed to download: {}", error);
                            if let Err(e) = sqlx::query!(
                                "UPDATE chapters SET download_status = 0 WHERE id = ?",
                                chapter_id
                            )
                            .execute(&db)
                            .await
                            {
                                tracing::warn!(
                                    "Failed to reset download_status for failed chapter {}: {}",
                                    chapter_id,
                                    e
                                );
                            }
                        }
                        kani_shared::DownloadProgressEvent::ChapterCancelled {
                            chapter_id, ..
                        }
                        | kani_shared::DownloadProgressEvent::ChapterDeferred {
                            chapter_id, ..
                        } => {
                            if let Err(e) = sqlx::query!(
                                "UPDATE chapters SET download_status = 0 WHERE id = ?",
                                chapter_id
                            )
                            .execute(&db)
                            .await
                            {
                                tracing::warn!(
                                    "Failed to reset download_status for cancelled/deferred chapter {}: {}",
                                    chapter_id,
                                    e
                                );
                            }
                        }
                        _ => {}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::error!("Download progress channel closed.");
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            "Download progress listener lagged by {} events! Reconciling...",
                            skipped
                        );
                        let sweep_state = listener_state.clone();
                        tokio::spawn(async move {
                            let records = sqlx::query!("SELECT c.id, c.volume, c.chapter_number, c.name, m.id as manga_id, m.name as manga_name FROM chapters c JOIN manga m ON c.manga_id = m.id WHERE c.download_status = 1")
                                .fetch_all(&sweep_state.db)
                                .await
                                .unwrap_or_default();

                            let library_path =
                                sweep_state.settings.read().await.library_path.clone();
                            for record in records {
                                let safe_manga_name_base =
                                    kani_core::utilities::sanitize_filename(&record.manga_name);
                                let safe_manga_name =
                                    format!("{} - {}", safe_manga_name_base, record.manga_id);
                                let manga_path = library_path.join(safe_manga_name);

                                let chapter_name = kani_web::state::chapter_name(
                                    record.volume,
                                    record.chapter_number,
                                    record.name,
                                );
                                let safe_chapter_name =
                                    kani_core::utilities::sanitize_filename(&chapter_name);
                                let file_path =
                                    manga_path.join(format!("{}.cbz", safe_chapter_name));

                                if tokio::fs::try_exists(&file_path).await.unwrap_or(false) {
                                    if let Err(e) = sqlx::query!(
                                        "UPDATE chapters SET download_status = 2, downloaded_at = COALESCE(downloaded_at, CURRENT_TIMESTAMP) WHERE id = ?",
                                        record.id
                                    )
                                    .execute(&sweep_state.db)
                                    .await
                                    {
                                        tracing::warn!(
                                            "Reconcile: failed to set download_status=2 for chapter {}: {}",
                                            record.id,
                                            e
                                        );
                                    }
                                } else if let Err(e) = sqlx::query!(
                                    "UPDATE chapters SET download_status = 0 WHERE id = ?",
                                    record.id
                                )
                                .execute(&sweep_state.db)
                                .await
                                {
                                    tracing::warn!(
                                        "Reconcile: failed to reset download_status for chapter {}: {}",
                                        record.id,
                                        e
                                    );
                                }
                            }
                        });
                    }
                }
            }
        });
    }

    let static_dir = std::env::var("KANI_STATIC_DIR").unwrap_or_else(|_| "static".to_string());
    tracing::info!("Serving static files from: {static_dir}");

    // Release builds serve the production bundle (bundled/minified JS, no import map).
    // Debug builds serve raw source files so changes are picked up without rebuilding.
    let index_html = if cfg!(debug_assertions) {
        format!("{static_dir}/index.html")
    } else {
        format!("{static_dir}/index.prod.html")
    };

    let app = Router::new()
        .merge(Router::new().nest("/rest", rest_router).layer(GovernorLayer { config: governor_conf }))
        .merge(Router::new().nest("/rest", proxy_router).layer(GovernorLayer { config: proxy_governor_conf }))
        .merge(health_router)
        .nest_service("/js", ServeDir::new(format!("{static_dir}/js")))
        .nest_service("/css", ServeDir::new(format!("{static_dir}/css")))
        .fallback_service(ServeFile::new(index_html))
        .layer(axum::middleware::from_fn(kani_web::auth::auth_guard))
        .layer(auth_layer)
        .layer(CompressionLayer::new())
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'"
            ),
        ))
        .layer(cors_layer)
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                if kani_web::HTTP_LOGGING_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
                    tracing::info_span!(
                        "http",
                        method = %request.method(),
                        uri = %request.uri(),
                        status = tracing::field::Empty,
                    )
                } else {
                    tracing::Span::none()
                }
            }),
        );

    // Cache-Control headers for static assets — release only.
    #[cfg(not(debug_assertions))]
    let app = app.layer(axum::middleware::from_fn(cache_control_middleware));

    let bind_addr = std::env::var("KANI_BIND").unwrap_or_else(|_| "0.0.0.0:8242".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|_| panic!("Failed to bind {bind_addr}"));

    let shutdown_token = state.shutdown_token.clone();

    tracing::info!("Server listening on http://{}", bind_addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Ctrl-C received, shutting down...");
            }
            _ = shutdown_token.cancelled() => {
                tracing::info!("Shutdown token cancelled, shutting down...");
            }
        }
        tracing::info!("Stopping background tasks...");
        shutdown_token.cancel();
        tokio::task::yield_now().await;
        if let Err(e) =
            sqlx::query!("UPDATE chapters SET download_status = 0 WHERE download_status = 1")
                .execute(&state.db)
                .await
        {
            tracing::warn!("Failed to reset in-flight download statuses on shutdown: {e}");
        }
        state.db.close().await;
        let exit_code = if state.restart_requested.load(std::sync::atomic::Ordering::Relaxed) {
            tracing::info!("Restart requested — exiting with code 42.");
            42
        } else {
            tracing::info!("Shutdown complete, exiting.");
            0
        };
        std::process::exit(exit_code);
    })
    .await
    .unwrap_or_else(|e| panic!("Server error: {e}"));
}

fn write_admin_file(
    user: &kani_web::types::User,
    password: &str,
) -> Result<(), kani_web::error::AppError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".kani_admin_password");

    let content = format!(
        "Username: {}\nEmail: {}\nPassword: {}",
        user.username, user.email, password
    );

    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);

    options.open(&path)
        .and_then(|mut f| f.write_all(content.as_bytes()))
        .inspect(|_| tracing::info!("No users found - admin password written to: {}\n\nPlease change this password immediately after logging in!\n\n", path.display()))
        .map_err(|e| {
            tracing::error!("Failed to write admin password to {:?}: {}", path, e);
            kani_web::error::AppError::InternalServerError(e.to_string())
        })
}

async fn ensure_default_user(
    backend: &kani_web::auth::AuthBackend,
) -> Result<(), kani_web::error::AppError> {
    if backend.user_count().await? == 0 {
        use argon2::password_hash::rand_core::{OsRng, RngCore};

        let mut bytes = [0u8; 12];
        OsRng.fill_bytes(&mut bytes);
        let password = hex::encode(bytes);

        let user = backend
            .create_user("admin", "admin@localhost", &password)
            .await?;
        write_admin_file(&user, &password)?;
        backend.grant_role(user.id, "admin", None).await?;
    }

    Ok(())
}
