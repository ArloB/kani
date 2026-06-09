//! Chapter CBZ & e-book export routes.

use super::*;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/chapters/{id}/cbz", get(serve_chapter_cbz))
        .route("/chapters/{id}/export/epub", get(export_epub))
        .route("/chapters/{id}/export/kepub", get(export_kepub))
        .route("/chapters/{id}/export/kcc", get(export_kcc))
}

async fn serve_chapter_cbz(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let info = state.chapter_cbz_path(id).await?;
    let bytes = tokio::fs::read(&info.path)
        .await
        .map_err(|_| AppError::NotFound(format!("Chapter {id} CBZ not found")))?;
    let safe_name = info.chapter_title.replace(['/', '\\', '"'], "_");
    let disposition = format!("attachment; filename=\"{safe_name}.cbz\"");
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/x-cbz".to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
        ],
        bytes,
    ))
}

async fn export_epub(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ExportQuery>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::service::export::DeviceProfile;
    let profile = q
        .profile
        .as_deref()
        .and_then(|s| s.parse::<DeviceProfile>().ok())
        .unwrap_or(DeviceProfile::Standard);
    let (bytes, filename) = state.service.export_chapter_epub(id, profile).await?;
    let disposition = format!("attachment; filename=\"{filename}\"");
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/epub+zip".to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
        ],
        bytes,
    ))
}

async fn export_kepub(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ExportQuery>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::service::export::DeviceProfile;
    let profile = q
        .profile
        .as_deref()
        .and_then(|s| s.parse::<DeviceProfile>().ok())
        .unwrap_or(DeviceProfile::KoboLibra);
    let (bytes, filename) = state.service.export_chapter_kepub(id, profile).await?;
    let disposition = format!("attachment; filename=\"{filename}\"");
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/epub+zip".to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
        ],
        bytes,
    ))
}

async fn export_kcc(
    AuthGuard(..): AuthGuard<crate::permissions::guards::LibraryView>,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<KccExportQuery>,
) -> Result<impl IntoResponse, AppError> {
    use kani_app::service::export::{KccFormat, KccOptions};
    let format: KccFormat = q
        .format
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(KccFormat::Mobi);
    let opts = KccOptions {
        format,
        profile: q.profile.clone().unwrap_or_else(|| "KPW5".to_owned()),
        manga_mode: q.manga.unwrap_or(true),
    };
    let (bytes, filename, mime) = state.service.export_chapter_kcc(id, opts).await?;
    let disposition = format!("attachment; filename=\"{filename}\"");
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime.to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
        ],
        bytes,
    ))
}
