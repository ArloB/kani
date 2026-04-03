//! Server-specific error types with Axum `IntoResponse` implementation.

use axum::{
    Json, http::StatusCode, response::{IntoResponse, Response}
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
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            // Client errors: safe to surface the message
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::Unauthorized(msg) | Self::PasswordError(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::ValidationError(msg) => (StatusCode::BAD_REQUEST, msg.clone()),

            // Internal errors: log details, return generic message
            Self::SqlxError(e) => {
                tracing::error!("Database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::CoreError(e) => {
                tracing::error!("Core error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::IoError(e) => {
                tracing::error!("IO error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::TryFromIntError(e) => {
                tracing::error!("Integer conversion error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::MigrationError(e) => {
                tracing::error!("Migration error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::RequestError(e) => {
                tracing::error!("HTTP request error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::LockPoisoned => {
                tracing::error!("Mutex lock poisoned");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::InternalServerError(msg) => {
                tracing::error!("Internal error: {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::ChannelSendError(msg) => {
                tracing::error!("Channel send error: {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::JsonError(e) => {
                tracing::error!("JSON error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::MultipartError(e) => {
                tracing::error!("Multipart error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::InvalidHeaderValue(e) => {
                tracing::error!("Invalid header value: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::HashError(e) => {
                tracing::error!("Password hash error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::Other(msg) => {
                tracing::error!("Unclassified error: {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            Self::PermissionParseError(e) => {
                tracing::error!("Permission parse error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
        };

        let body = Json(json!({
            "error": message,
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
