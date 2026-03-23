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
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::Other(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            Self::SqlxError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::CoreError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::IoError(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("IO error: {e}")),
            Self::TryFromIntError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("TryFromIntError: {e}"),
            ),
            Self::MigrationError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Migration error: {e}"),
            ),
            Self::RequestError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::LockPoisoned => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Lock poisoned".to_string(),
            ),
            Self::InternalServerError(msg) | Self::ChannelSendError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone())
            }
            Self::JsonError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::MultipartError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::InvalidHeaderValue(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::HashError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::PasswordError(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::PermissionParseError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
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
