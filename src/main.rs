mod error;
mod etag;
mod handlers;
mod models;
mod state;

use state::AppState;

use tower_http::services::{ServeDir, ServeFile};

use axum::http::{HeaderValue, header};
use tower_http::set_header::SetResponseHeaderLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let state = AppState::new()
        .await
        .expect("Failed to initialize AppState");

    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let mut sources = cleanup_state.sources.lock().await;
            for source in sources.values_mut() {
                match source.maybe_unload() {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::error!("Failed to unload source with error {e}");
                    }
                }
            }
        }
    });

    let frontend_dist = "kani-web/dist";
    let serve_dir = if std::path::Path::new(frontend_dist).exists() {
        ServeDir::new(frontend_dist).fallback(ServeFile::new("kani-web/dist/index.html"))
    } else {
        return Err(format!("Frontend distribution not found at {frontend_dist}").into());
    };

    let app = axum::Router::new()
        .nest(
            "/api",
            handlers::routes().with_state(state.clone()).layer(
                SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("no-cache"),
                ),
            ),
        )
        .fallback_service(serve_dir);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8242")
        .await
        .expect("Failed to bind port 8242");

    println!("Server running on http://0.0.0.0:8242");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
