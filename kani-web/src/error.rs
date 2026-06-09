//! Server-specific error types with Axum `IntoResponse` implementation.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Internal Server Error: {0}")]
    InternalServerError(String),

    #[error("Not Found: {0}")]
    NotFound(String),

    #[error("Other error: {0}")]
    Other(String),

    #[error("Database error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Core error: {0}")]
    CoreError(#[from] kani_core::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Migration error: {0}")]
    MigrationError(#[from] sqlx::migrate::MigrateError),

    #[error("Request error: {0}")]
    RequestError(#[from] rquest::Error),

    #[error("Lock poisoned")]
    LockPoisoned,

    #[error("Channel send error: {0}")]
    ChannelSendError(String),

    #[error("TryFromIntError: {0}")]
    TryFromIntError(#[from] std::num::TryFromIntError),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("multipart error: {0}")]
    MultipartError(#[from] axum::extract::multipart::MultipartError),

    #[error("Invalid Header Value: {0}")]
    InvalidHeaderValue(#[from] rquest::header::InvalidHeaderValue),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Password hash error: {0}")]
    HashError(#[from] argon2::password_hash::Error),

    #[error("Password error: {0}")]
    PasswordError(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Permission error: {0}")]
    PermissionParseError(#[from] crate::permissions::PermissionParseError),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("FlareSolverr is not configured")]
    FlareSolverrRequired,

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Source requires authentication: {0}")]
    SourceAuthRequired(String),

    #[error("Source {0} is disabled")]
    SourceDisabled(i64),

    #[error("Possible duplicate")]
    PossibleDuplicate(Vec<kani_app::SimilarMangaHit>),

    #[error("Email error: {0}")]
    EmailError(String),
}

impl AppError {
    /// Short machine-readable code included in JSON error bodies.
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Unauthorized(_) | Self::PasswordError(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::ValidationError(_) => "validation_error",
            Self::FlareSolverrRequired => "flaresolverr_required",
            Self::RateLimitExceeded => "rate_limited",
            Self::SourceAuthRequired(_) => "source_auth_required",
            Self::SourceDisabled(_) => "source_disabled",
            Self::EmailError(_) => "email_error",
            _ => "internal_error",
        }
    }

    /// Optional actionable guidance shown to the user.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::FlareSolverrRequired => {
                Some("This source requires FlareSolverr. Configure it in Settings > Advanced.")
            }
            Self::RateLimitExceeded => Some("Too many requests. Please wait a moment."),
            Self::SourceAuthRequired(_) => {
                Some("This source requires login. Configure credentials in source settings.")
            }
            Self::Forbidden(_) => {
                Some("You don't have permission for this action. Contact an administrator.")
            }
            Self::Unauthorized(_) => Some("Please log in to continue."),
            _ => None,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            // Client errors: safe to surface the message
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::Unauthorized(msg) | Self::PasswordError(msg) => {
                (StatusCode::UNAUTHORIZED, msg.clone())
            }
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg.clone()),

            // Internal errors: log details, return generic message
            Self::SqlxError(e) => {
                tracing::error!("Database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::CoreError(e) => {
                tracing::error!("Core error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::IoError(e) => {
                tracing::error!("IO error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::TryFromIntError(e) => {
                tracing::error!("Integer conversion error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::MigrationError(e) => {
                tracing::error!("Migration error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::RequestError(e) => {
                tracing::error!("HTTP request error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::LockPoisoned => {
                tracing::error!("Mutex lock poisoned");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::InternalServerError(msg) => {
                tracing::error!("Internal error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::ChannelSendError(msg) => {
                tracing::error!("Channel send error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::JsonError(e) => {
                tracing::error!("JSON error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::MultipartError(e) => {
                tracing::error!("Multipart error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::InvalidHeaderValue(e) => {
                tracing::error!("Invalid header value: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::HashError(e) => {
                tracing::error!("Password hash error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::Other(msg) => {
                tracing::error!("Unclassified error: {msg}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::PermissionParseError(e) => {
                tracing::error!("Permission parse error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
            Self::FlareSolverrRequired => (StatusCode::BAD_GATEWAY, self.to_string()),
            Self::RateLimitExceeded => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            Self::SourceAuthRequired(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::SourceDisabled(id) => {
                tracing::debug!("Request to disabled source {id}");
                let body = Json(json!({
                    "error": format!("Source {id} is disabled"),
                    "code": "source_disabled",
                    "disabled": true,
                    "source_id": id,
                    "hint": null,
                }));
                return (StatusCode::CONFLICT, body).into_response();
            }
            Self::PossibleDuplicate(hits) => {
                let body = Json(json!({
                    "status": "possible_duplicate",
                    "suggestions": hits,
                }));
                return (StatusCode::CONFLICT, body).into_response();
            }
            Self::EmailError(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
        };

        let body = Json(json!({
            "error": message,
            "code": self.error_code(),
            "hint": self.hint(),
        }));

        (status, body).into_response()
    }
}

impl<T> From<std::sync::PoisonError<T>> for AppError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Self::LockPoisoned
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for AppError {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Self::ChannelSendError(error.to_string())
    }
}

impl From<kani_app::ServiceError> for AppError {
    fn from(e: kani_app::ServiceError) -> Self {
        match e {
            kani_app::ServiceError::NotFound(s) => Self::NotFound(s),
            kani_app::ServiceError::SourceDisabled(id) => Self::SourceDisabled(id),
            kani_app::ServiceError::Conflict(s) => Self::Conflict(s),
            kani_app::ServiceError::Forbidden(s) => Self::Forbidden(s),
            kani_app::ServiceError::Internal(s) => Self::InternalServerError(s),
            kani_app::ServiceError::Validation(s) => Self::ValidationError(s),
            kani_app::ServiceError::Core(e) => Self::CoreError(e),
            kani_app::ServiceError::Db(e) => Self::SqlxError(e),
            kani_app::ServiceError::Migration(e) => Self::MigrationError(e),
            kani_app::ServiceError::Io(e) => Self::IoError(e),
            kani_app::ServiceError::TryFromInt(e) => Self::TryFromIntError(e),
            kani_app::ServiceError::RequestError(e) => Self::RequestError(e),
            kani_app::ServiceError::PossibleDuplicate(hits) => Self::PossibleDuplicate(hits),
        }
    }
}
