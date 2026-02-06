//! Server-specific error types with Axum IntoResponse implementation.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Internal Server Error: {0}")]
    InternalServerError(String),

    #[error("Not Found: {0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Core error: {0}")]
    CoreError(#[from] kani_core::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid WASM file")]
    InvalidWasmFile,

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
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::SqlxError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::CoreError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::IoError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("IO error: {}", e),
            ),
            AppError::TryFromIntError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("TryFromIntError: {}", e),
            ),
            AppError::InvalidWasmFile => (StatusCode::BAD_REQUEST, "Invalid WASM file".to_string()),
            AppError::MigrationError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Migration error: {}", e),
            ),
            AppError::RequestError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::LockPoisoned => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Lock poisoned".to_string(),
            ),
            AppError::ChannelSendError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::JsonError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

        let body = Json(json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

impl<T> From<std::sync::PoisonError<T>> for AppError {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        AppError::LockPoisoned
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for AppError {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        AppError::ChannelSendError(error.to_string())
    }
}
