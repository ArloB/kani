mod error;
mod handlers;
mod models;
mod state;

use axum::{
    Router,
    routing::{get, post},
};
use handlers::*;
use state::AppState;

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
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Failed to unload source: {}", e);
                    }
                }
            }
        }
    });

    let app = Router::new()
        .route("/image_proxy", get(image_proxy))
        // Source Routes
        .route("/sources", get(list_sources).post(add_source))
        .route(
            "/sources/{id}",
            get(get_source).patch(update_source).delete(delete_source),
        )
        .route("/sources/{id}/wasm", post(upload_wasm))
        .route("/sources/{id}/wasm/fetch", post(fetch_wasm))
        .route("/sources/{id}/popular/{page}", get(get_popular_manga))
        .route("/sources/{id}/search/{page}", get(search_manga))
        .route("/sources/{id}/details/{manga_id}", get(get_manga_details))
        .route(
            "/sources/{id}/chapters/{manga_id}/{page}",
            get(get_chapter_list),
        )
        .route("/sources/{id}/download/{chapter_id}", post(start_download))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8242")
        .await
        .expect("Failed to bind port 8242");
    println!("Server running on http://0.0.0.0:8242");
    axum::serve(listener, app).await?;

    Ok(())
}
