#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use axum_login::{
        AuthManagerLayerBuilder,
        tower_sessions::{SessionManagerLayer, cookie::SameSite},
    };
    use tower_sessions_sqlx_store::SqliteStore;
    use kani_web::{app::App, rest, state::AppState, auth::{AuthBackend}};
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use tower_http::compression::CompressionLayer;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .init();

    let conf = get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options.clone();
    let routes = generate_route_list(App);

    let state = AppState::new().await.expect("Failed to initialise AppState");

    let session_store = SqliteStore::new(state.db.clone());
    session_store.migrate().await.unwrap();

    // TODO: Set `with_secure(true)` when running behind HTTPS
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_http_only(true)
        .with_same_site(SameSite::Lax);

    let auth_backend = AuthBackend::new(state.db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend.clone(), session_layer).build();

    if let Err(e) = ensure_default_user(&auth_backend).await {
        tracing::error!("Failed to ensure default user: {}", e);
    }

    {
        let cleanup_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            let idle_timeout = std::time::Duration::from_secs(300);
            loop {
                interval.tick().await;
                let sources = cleanup_state.sources.read().await;
                for manager in sources.values() {
                    manager.cleanup(idle_timeout).await;
                }
            }
        });
    }

    {
        let scan_state = state.clone();
        tokio::spawn(async move {
            loop {
                let interval_mins = scan_state.settings.read().await.scan_interval_minutes;
                tokio::time::sleep(std::time::Duration::from_secs(interval_mins as u64 * 60)).await;

                if !scan_state.settings.read().await.auto_scan {
                    continue;
                }

                let manga_to_scan: Vec<(i64, bool)> = sqlx::query_as(
                    "SELECT id, auto_download FROM manga"
                ).fetch_all(&scan_state.db).await.unwrap_or_default();
                for (manga_db_id, auto_download) in manga_to_scan {
                    match scan_state.scan_for_new_chapters(manga_db_id).await {
                        Ok(new_ids) if !new_ids.is_empty() => {
                            tracing::info!("Found {} new chapters for manga {}", new_ids.len(), manga_db_id);
                            if auto_download {
                                let filtered_ids = scan_state
                                    .filter_chapters_by_rules(manga_db_id, new_ids.clone())
                                    .await;

                                if filtered_ids.is_empty() {
                                    tracing::info!(
                                        "All new chapters for manga {} filtered out by download rules",
                                        manga_db_id
                                    );
                                } else {
                                    tracing::info!(
                                        "{} new chapters passed download rules for manga {}",
                                        filtered_ids.len(), manga_db_id
                                    );

                                    let futures = new_ids.into_iter().map(|new_id| {
                                        let state = scan_state.clone();
                                        async move {
                                            match state.enqueue_claimed_chapter(new_id).await {
                                                Ok(_) => {
                                                    tracing::info!("Chapter {} enqueued for download", new_id);
                                                }
                                                Err(e) => tracing::error!("Failed to enqueue chapter {}: {}", new_id, e),
                                            }
                                        }
                                    });

                                    futures::future::join_all(futures).await;
                                }
                            }
                        }
                        Err(e) => tracing::error!("Scan failed for manga {}: {}", manga_db_id, e),
                        _ => {}
                    }
                }
            }
        });
    }

    let rest_router = rest::routes(state.clone());

    {
        let db = state.db.clone();
        let mut rx = state.downloader.subscribe();
        let listener_state = state.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => match event {
                        kani_shared::DownloadProgressEvent::ChapterStarted { chapter_id, .. } => {
                            let _ = sqlx::query!(
                                "UPDATE chapters SET download_status = 1 WHERE id = ?",
                                chapter_id
                            )
                            .execute(&db)
                            .await;
                        }
                        kani_shared::DownloadProgressEvent::ChapterCompleted { chapter_id, .. } => {
                            let _ = sqlx::query!(
                                "UPDATE chapters SET download_status = 2 WHERE id = ?",
                                chapter_id
                            )
                            .execute(&db)
                            .await;
                        }
                        kani_shared::DownloadProgressEvent::ChapterFailed { chapter_id, error, .. } => {
                            tracing::error!("Chapter failed to download: {}", error);
                            let _ = sqlx::query!("UPDATE chapters SET download_status = 0 WHERE id = ?", chapter_id)
                                .execute(&db)
                                .await;
                        }
                        kani_shared::DownloadProgressEvent::ChapterCancelled { chapter_id, .. }
                        | kani_shared::DownloadProgressEvent::ChapterDeferred { chapter_id, .. } => {
                            let _ = sqlx::query!("UPDATE chapters SET download_status = 0 WHERE id = ?", chapter_id)
                                .execute(&db)
                                .await;
                        }
                        _ => {}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::error!("Download progress channel closed.");
                        break;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("Download progress listener lagged by {} events! Reconciling...", skipped);
                        let sweep_state = listener_state.clone();
                        tokio::spawn(async move {
                            let records = sqlx::query!("SELECT c.id, c.volume, c.chapter_number, c.name, m.id as manga_id, m.name as manga_name FROM chapters c JOIN manga m ON c.manga_id = m.id WHERE c.download_status = 1")
                                .fetch_all(&sweep_state.db)
                                .await
                                .unwrap_or_default();

                            let library_path = sweep_state.settings.read().await.library_path.clone();
                            for record in records {
                                let safe_manga_name_base = kani_core::utilities::sanitize_filename(&record.manga_name);
                                let safe_manga_name = format!("{} - {}", safe_manga_name_base, record.manga_id);
                                let manga_path = library_path.join(safe_manga_name);

                                let chapter_name = kani_web::state::chapter_name(record.volume, record.chapter_number, record.name);
                                let safe_chapter_name = kani_core::utilities::sanitize_filename(&chapter_name);
                                let file_path = manga_path.join(format!("{}.cbz", safe_chapter_name));

                                if tokio::fs::try_exists(&file_path).await.unwrap_or(false) {
                                    let _ = sqlx::query!("UPDATE chapters SET download_status = 2 WHERE id = ?", record.id).execute(&sweep_state.db).await;
                                } else {
                                    let _ = sqlx::query!("UPDATE chapters SET download_status = 0 WHERE id = ?", record.id).execute(&sweep_state.db).await;
                                }
                            }
                        });
                    }
                }
            }
        });
    }

    let s_get = state.clone();
    let s_post = state.clone();

    let app = Router::new()
        .route(
            "/api/{*fn_name}",
            axum::routing::get(move |req: axum::extract::Request| {
                leptos_axum::handle_server_fns_with_context(
                    move || provide_context(s_get.clone()),
                    req,
                )
            })
            .post(move |req: axum::extract::Request| {
                leptos_axum::handle_server_fns_with_context(
                    move || provide_context(s_post.clone()),
                    req,
                )
            }),
        )
        .leptos_routes_with_context(
            &leptos_options,
            routes,
            {
                let s = state.clone();
                move || provide_context(s.clone())
            },
            {
                let opts = leptos_options.clone();
                move || shell(opts.clone())
            },
        )
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options)
        .merge(Router::new().nest("/rest", rest_router))
        .layer(axum::middleware::from_fn(kani_web::auth::auth_guard))
        .layer(auth_layer)
        .layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8242")
        .await
        .expect("Failed to bind port 8242");

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl_c");
        let _ = shutdown_tx.send(true);
    });

    tracing::info!("Server listening on http://0.0.0.0:8242");
    axum::serve(listener, app).with_graceful_shutdown(async move {
        shutdown_rx.changed().await.ok();
        sqlx::query!("UPDATE chapters SET download_status = 0 WHERE download_status = 1")
            .execute(&state.db).await.ok();
        state.db.close().await;
    }).await.expect("Server error");
}

fn write_admin_file(user: &kani_web::types::User, password: &str) -> Result<(), kani_web::error::AppError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".kani_admin_password");

    let content = format!("Username: {}\nEmail: {}\nPassword: {}", user.username, user.email, password);

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

#[cfg(feature = "ssr")]
async fn ensure_default_user(backend: &kani_web::auth::AuthBackend) -> Result<(), kani_web::error::AppError> {
    if backend.user_count().await? == 0 {
        use argon2::password_hash::rand_core::{OsRng, RngCore};
        
        let mut bytes = [0u8; 12];
        OsRng.fill_bytes(&mut bytes);
        let password = hex::encode(bytes);

        let user = backend.create_user("admin", "admin@localhost", &password).await?;
        write_admin_file(&user, &password)?;
        backend.grant_role(user.id, "admin", None).await?;
    }

    Ok(())
}


#[cfg(feature = "ssr")]
fn shell(options: leptos::prelude::LeptosOptions) -> impl leptos::IntoView {
    use kani_web::app::App;
    use leptos::prelude::*;
    use leptos_meta::MetaTags;
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options=options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[cfg(not(feature = "ssr"))]
fn main() {}
