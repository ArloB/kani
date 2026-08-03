/// Sets `Cache-Control` on all `/js/*` and `/css/*` responses.
///
/// Release builds get long-lived immutable caching — filenames are content-hashed.
/// Debug builds get `no-cache` so the browser always revalidates raw source files.
/// Without an explicit header the browser applies heuristic caching and can serve a
/// stale module after an edit, which surfaces as confusing parse errors.
async fn cache_control_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use rquest::header;

    #[cfg(not(debug_assertions))]
    const CACHE_VALUE: &str = "public, max-age=31536000, immutable";
    #[cfg(debug_assertions)]
    const CACHE_VALUE: &str = "no-cache";

    let is_static =
        request.uri().path().starts_with("/js/") || request.uri().path().starts_with("/css/");
    let mut response = next.run(request).await;
    if is_static {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static(CACHE_VALUE),
        );
    }
    response
}

#[tokio::main]
async fn main() {
    use axum::Router;
    use axum::http::{HeaderValue, Method, StatusCode, header};
    use axum::response::IntoResponse;
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

    let buf_cap: usize = std::env::var("KANI_LOG_BUFFER_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);

    let (ring_layer, log_handle) = kani_web::logging::RingBufferLayer::new(buf_cap);

    const DEFAULT_FILTER: &str =
        "info,axum_login=warn,tower_sessions=warn,tower_sessions_core=warn,sqlx=warn";

    let make_filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_FILTER))
    };

    use tracing_subscriber::prelude::*;

    let json_logs = std::env::var("KANI_LOG_FORMAT").is_ok_and(|v| v.eq_ignore_ascii_case("json"));

    let fmt_layer = if json_logs {
        tracing_subscriber::fmt::layer()
            .json()
            .flatten_event(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_filter(make_filter())
            .boxed()
    } else {
        tracing_subscriber::fmt::layer()
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .with_filter(make_filter())
            .boxed()
    };

    let registry = tracing_subscriber::registry()
        .with(ring_layer.with_filter(make_filter()))
        .with(fmt_layer);

    registry.init();

    kani_app::service::diagnostics::init(env!("CARGO_PKG_VERSION"), env!("GIT_SHA"));

    tracing::info!("Starting Kani Web Server");

    // Resolve the data directory. In Docker the working directory is /data, so the
    // default (CWD) is correct without any configuration. Native installs can
    // override with KANI_DATA_DIR.
    let data_dir: std::path::PathBuf = std::env::var("KANI_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_dir().expect("Cannot determine current working directory")
        });

    let state = AppState::new(log_handle, data_dir)
        .await
        .expect("Failed to initialise AppState");

    let session_store = SqliteStore::new(state.db.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let secure_cookies = std::env::var("KANI_SECURE_COOKIES")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    // Session lifetime — defaults to 30 days of inactivity. Read from the
    // `session_timeout_secs` setting (seeded from KANI_SESSION_TIMEOUT_SECONDS on
    // first boot); changes apply on restart.
    let session_timeout_secs: i64 = state.settings.read().await.session_timeout_secs;

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(secure_cookies)
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_expiry(axum_login::tower_sessions::Expiry::OnInactivity(
            time::Duration::seconds(session_timeout_secs),
        ));

    let auth_backend = AuthBackend::new(state.db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend.clone(), session_layer).build();

    if let Err(e) = announce_first_run(&auth_backend).await {
        tracing::error!("Failed to check for a first-run instance: {}", e);
    }

    kani_web::HTTP_LOGGING_ENABLED.store(
        state.get_settings().await.http_request_logging,
        std::sync::atomic::Ordering::Relaxed,
    );

    kani_web::SOURCE_INSTALL_ALLOWED.store(
        std::env::var("KANI_SOURCE_INSTALL_ALLOWED")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true),
        std::sync::atomic::Ordering::Relaxed,
    );

    {
        let official_url = std::env::var("KANI_OFFICIAL_REPO_URL").unwrap_or_default();
        let official_key = std::env::var("KANI_OFFICIAL_REPO_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| kani_web::repo_keys::OFFICIAL_REPO_KEY.to_string());
        if !official_url.is_empty() && !official_key.is_empty() {
            let bootstrap_state = state.clone();
            tokio::spawn(async move {
                if let Err(e) = bootstrap_state
                    .bootstrap_official_repo(&official_url, &official_key)
                    .await
                {
                    tracing::warn!("Official repo bootstrap failed: {e}");
                }
            });
        }
    }

    state.spawn_cover_retry();
    state.spawn_credential_refresh();
    state.spawn_webhook_listener();
    state.spawn_login_attempt_prune();
    state.spawn_cache_prune();
    state.spawn_progress_flush();

    {
        let backfill_state = state.clone();
        tokio::spawn(async move {
            backfill_state.submit_manifest_backfill_if_needed().await;

            // After the backfill, so a startup scrub judges the paths and
            // hashes the backfill just wrote rather than the gaps it filled.
            if backfill_state.get_settings().await.scrub_on_startup {
                let job = kani_app::jobs::scrub::ScrubJob::new(
                    kani_app::service::integrity::ScrubDepth::Quick,
                    false,
                );
                if let Err(e) = backfill_state.service.job_manager.submit(job).await {
                    tracing::warn!("Failed to submit startup integrity scrub: {e}");
                }
            }
        });
    }

    if let Err(e) = kani_app::jobs::recurring::ensure_recurring_rows(&state.db).await {
        tracing::warn!("Failed to initialise recurring job rows: {e}");
    }
    kani_app::jobs::recurring::spawn_recurring_scheduler(&state);

    // Rate limiter settings.
    // API: enough for normal UI use while protecting against abuse.
    // Proxy: much more permissive — the reader fires many concurrent image
    // requests; the per-host semaphore in rest.rs already throttles upstream.
    //
    // A debug build raises both by an order of magnitude. Automated browsing —
    // a Playwright sweep, a load of the whole settings tree — drains the release
    // budget in seconds, and every subsequent call 429s, which reads as a pile
    // of application bugs rather than the limiter doing its job. The release
    // defaults are what ships; `KANI_API_RATE_PER_SECOND` and friends override
    // either build.
    let dev_build = cfg!(debug_assertions);
    let env_u64 = |name: &str, default: u64| -> u64 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(default)
    };
    let env_u32 = |name: &str, default: u32| -> u32 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(default)
    };

    let api_rate_per_second = env_u64("KANI_API_RATE_PER_SECOND", if dev_build { 50 } else { 5 });
    let api_burst_size = env_u32("KANI_API_BURST_SIZE", if dev_build { 2000 } else { 200 });
    let proxy_rate_per_second = env_u64(
        "KANI_PROXY_RATE_PER_SECOND",
        if dev_build { 300 } else { 30 },
    );
    let proxy_burst_size = env_u32("KANI_PROXY_BURST_SIZE", if dev_build { 6000 } else { 600 });

    if dev_build {
        tracing::info!(
            api_rate_per_second,
            api_burst_size,
            "Debug build: rate limits relaxed for local development"
        );
    }

    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(api_rate_per_second)
            .burst_size(api_burst_size)
            // Bucket bearer traffic per token, so a busy integration cannot
            // spend its owner's browsing budget.
            .key_extractor(kani_web::rate_limit_key::TokenOrPeerIp)
            // Emit x-ratelimit-* and retry-after: a client that cannot see its
            // budget can only retry blindly, which makes congestion worse.
            .use_headers()
            .finish()
            .expect("api rate/burst values are valid"),
    );
    let proxy_governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(proxy_rate_per_second)
            .burst_size(proxy_burst_size)
            .finish()
            .expect("proxy rate/burst values are valid"),
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
    let opds_router = kani_web::opds::routes(state.clone());
    let health_router = axum::Router::new()
        .route("/health", axum::routing::get(rest::health))
        .route("/healthz", axum::routing::get(rest::health))
        .route("/ready", axum::routing::get(rest::ready))
        .route("/readyz", axum::routing::get(rest::ready))
        .with_state(state.clone());
    let (prometheus_layer, _) = kani_web::metrics::prometheus();
    kani_web::metrics::describe();
    let metrics_router = kani_web::metrics::router(state.clone());

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
                        kani_shared::DownloadProgressEvent::ChapterCompleted {
                            chapter_id,
                            successful_pages,
                            ..
                        } => {
                            let pc = successful_pages as i64;
                            let _ = sqlx::query!(
                                "UPDATE chapters SET page_count = ? WHERE id = ?",
                                pc,
                                chapter_id
                            )
                            .execute(&db)
                            .await;
                            struct ChapterInfo {
                                manga_id: i64,
                                manga_name: String,
                                volume: Option<i64>,
                                chapter_number: f64,
                                name: Option<String>,
                            }
                            if let Ok(Some(info)) = sqlx::query_as!(
                                ChapterInfo,
                                "SELECT c.manga_id, m.name AS manga_name, \
                                 c.volume, c.chapter_number, c.name \
                                 FROM chapters c JOIN manga m ON c.manga_id = m.id \
                                 WHERE c.id = ?",
                                chapter_id
                            )
                            .fetch_optional(&db)
                            .await
                            {
                                let chapter_name = kani_app::chapter_name(
                                    info.volume,
                                    info.chapter_number,
                                    info.name,
                                );
                                listener_state
                                    .fire_webhooks(
                                        kani_app::service::webhooks::WebhookPayload::ChapterDownloaded {
                                            chapter_id: kani_app::ids::ChapterId(chapter_id),
                                            manga_id: kani_app::ids::MangaId(info.manga_id),
                                            manga_name: info.manga_name,
                                            chapter_name,
                                        },
                                    )
                                    .await;
                            }
                        }
                        kani_shared::DownloadProgressEvent::ChapterFailed { error, .. } => {
                            tracing::error!("Chapter download failed: {}", error);
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
                                        "UPDATE chapters SET download_status = ?, downloaded_at = COALESCE(downloaded_at, CURRENT_TIMESTAMP) WHERE id = ?",
                                        kani_shared::types::DownloadStatus::Complete,
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
                                    "UPDATE chapters SET download_status = ? WHERE id = ?",
                                    kani_shared::types::DownloadStatus::Pending,
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

    let manifest_path = format!("{static_dir}/manifest.webmanifest");
    let sw_path = format!("{static_dir}/sw.js");

    let app = Router::new()
        // PWA — manifest and service worker need explicit Content-Type headers
        .route(
            "/manifest.webmanifest",
            axum::routing::get(move || {
                let p = manifest_path.clone();
                async move {
                    match tokio::fs::read(&p).await {
                        Ok(b) => (
                            [(
                                header::CONTENT_TYPE,
                                "application/manifest+json; charset=utf-8",
                            )],
                            b,
                        )
                            .into_response(),
                        Err(_) => StatusCode::NOT_FOUND.into_response(),
                    }
                }
            }),
        )
        .route(
            "/sw.js",
            axum::routing::get(move || {
                let p = sw_path.clone();
                async move {
                    match tokio::fs::read(&p).await {
                        Ok(b) => {
                            let mut h = axum::http::HeaderMap::new();
                            h.insert(
                                header::CONTENT_TYPE,
                                header::HeaderValue::from_static(
                                    "application/javascript; charset=utf-8",
                                ),
                            );
                            h.insert(
                                header::HeaderName::from_static("service-worker-allowed"),
                                header::HeaderValue::from_static("/"),
                            );
                            (StatusCode::OK, h, b).into_response()
                        }
                        Err(_) => StatusCode::NOT_FOUND.into_response(),
                    }
                }
            }),
        )
        .nest_service("/icons", ServeDir::new(format!("{static_dir}/icons")))
        // OPDS catalog — auth is handled per-handler (supports Basic auth)
        .nest("/opds", opds_router)
        .merge(
            Router::new()
                .nest("/rest", rest_router)
                .layer(GovernorLayer {
                    config: governor_conf,
                }),
        )
        .merge(
            Router::new()
                .nest("/rest", proxy_router)
                .layer(GovernorLayer {
                    config: proxy_governor_conf,
                }),
        )
        .merge(health_router)
        .merge(metrics_router)
        .nest_service("/js", ServeDir::new(format!("{static_dir}/js")))
        .nest_service("/css", ServeDir::new(format!("{static_dir}/css")))
        .nest_service("/locales", ServeDir::new(format!("{static_dir}/locales")))
        .nest_service("/fonts", ServeDir::new(format!("{static_dir}/fonts")))
        .route_service(
            "/changelog.md",
            ServeFile::new(format!("{static_dir}/changelog.md")),
        )
        .fallback_service(ServeFile::new(index_html))
        .layer(axum::middleware::from_fn(kani_web::auth::auth_guard))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            kani_web::session_touch::session_touch_middleware,
        ))
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
            axum::http::HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            {
                let script_src = if cfg!(debug_assertions) {
                    // Two hashes: the importmap script, then the FOUC theme-application
                    // script in index.html. Recompute (sha256, base64) if either changes.
                    "script-src 'self' 'wasm-unsafe-eval' 'sha256-ZTuQJVyh0iAW+hoad/K7BcW9ZJN1l+yFJ/1Da8LGDJc=' 'sha256-OY6s4Q2QijBQ9AZF47LzWDgWw4/01jCMVcnpiCFAyRg='"
                } else {
                    "script-src 'self' 'wasm-unsafe-eval'"
                };
                HeaderValue::try_from(format!(
                    "default-src 'self'; \
                     img-src 'self' data: blob:; \
                     style-src 'self' 'unsafe-inline'; \
                     {script_src}; \
                     object-src 'none'; \
                     base-uri 'self'; \
                     form-action 'self'; \
                     frame-ancestors 'none'"
                ))
                .expect("CSP header value is statically valid")
            },
        ))
        // HSTS: only when KANI_SECURE_COOKIES=true, meaning TLS is terminated upstream.
        .layer(tower::util::option_layer(if secure_cookies {
            Some(SetResponseHeaderLayer::overriding(
                axum::http::HeaderName::from_static("strict-transport-security"),
                HeaderValue::from_static("max-age=31536000; includeSubDomains"),
            ))
        } else {
            None
        }))
        .layer(cors_layer)
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                if kani_web::HTTP_LOGGING_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
                    let request_id = request
                        .extensions()
                        .get::<tower_http::request_id::RequestId>()
                        .and_then(|id| id.header_value().to_str().ok())
                        .unwrap_or_default();
                    tracing::info_span!(
                        "http",
                        method = %request.method(),
                        uri = %request.uri(),
                        status = tracing::field::Empty,
                        request_id = %request_id,
                    )
                } else {
                    tracing::Span::none()
                }
            }),
        );

    // Swagger UI, debug builds only. Merged here rather than inside the chain
    // above so it sits outside the auth layers — the UI is a plain static page
    // that fetches the spec, and an auth redirect would break it. `build_app`
    // mounts its own copy for tests; this is the one the server actually serves.
    #[cfg(debug_assertions)]
    let app = {
        use utoipa::OpenApi;
        app.merge(utoipa_swagger_ui::SwaggerUi::new("/api-docs").url(
            "/api-docs/openapi.json",
            kani_web::openapi::ApiDoc::openapi(),
        ))
    };

    // Cache-Control for static assets: immutable in release, no-cache in debug.
    let app = app
        .layer(axum::middleware::from_fn(cache_control_middleware))
        .layer(prometheus_layer.clone())
        .layer(tower_http::request_id::PropagateRequestIdLayer::x_request_id())
        .layer(tower_http::request_id::SetRequestIdLayer::x_request_id(
            kani_web::middleware::trace_id::UuidRequestId,
        ));

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
        let drain_secs = std::env::var("KANI_JOB_SHUTDOWN_TIMEOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);
        state
            .job_manager
            .drain(std::time::Duration::from_secs(drain_secs))
            .await;
        state.db.close().await;
        let exit_code = if state
            .restart_requested
            .load(std::sync::atomic::Ordering::Relaxed)
        {
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

/// A fresh instance has no accounts and no generated password: the first person
/// to reach it over the local network creates the administrator through the
/// in-app setup screen (`POST /rest/auth/setup`), which closes the moment that
/// account exists.
///
/// This replaced a generated `admin` password written to the data directory. The
/// file had to be found and copy-pasted, and the wizard then made the operator
/// change it — which invalidated their session mid-setup — so the first
/// experience was two logins and a file hunt.
async fn announce_first_run(
    backend: &kani_web::auth::AuthBackend,
) -> Result<(), kani_web::error::AppError> {
    if backend.user_count().await? == 0 {
        tracing::info!(
            "No accounts yet — open Kani in a browser on this machine or your local \
             network to create the administrator. Setup closes as soon as that \
             account exists. Set KANI_ALLOW_REMOTE_SETUP=true if you must do it \
             over the internet."
        );
    }
    Ok(())
}
