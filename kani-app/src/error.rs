//! Application-layer error type — no Axum dependency.

pub type Result<T, E = ServiceError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Source {0} is disabled")]
    SourceDisabled(i64),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Other error: {0}")]
    Other(String),

    #[error("Possible duplicate")]
    PossibleDuplicate(Vec<crate::service::dedup::SimilarMangaHit>),

    #[error(transparent)]
    Core(#[from] kani_core::Error),

    #[error(transparent)]
    Db(#[from] sqlx::Error),

    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    TryFromInt(#[from] std::num::TryFromIntError),

    #[error(transparent)]
    RequestError(#[from] rquest::Error),
}
