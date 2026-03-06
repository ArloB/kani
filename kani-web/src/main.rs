#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use kani_web::{app::App, rest, state::AppState};
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

    let rest_router = rest::routes(state.clone());

    {
        let db = state.db.clone();
        let mut rx = state.downloader.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                match event {
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
                    kani_shared::DownloadProgressEvent::ChapterCancelled { chapter_id, .. } => {
                        let _ = sqlx::query!("UPDATE chapters SET download_status = 0 WHERE id = ?", chapter_id)
                            .execute(&db)
                            .await;
                    }
                    _ => {}
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
        .layer(CompressionLayer::new())
        .with_state(leptos_options)
        .merge(Router::new().nest("/rest", rest_router));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8242")
        .await
        .expect("Failed to bind port 8242");

    tracing::info!("Server listening on http://0.0.0.0:8242");
    axum::serve(listener, app).await.expect("Server error");
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
