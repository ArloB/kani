//! Core error types for kani-core.

use thiserror::Error;

/// Core error type for kani operations.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Path traversal blocked: {0}")]
    PathTraversal(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("WASM error: {0}")]
    Wasm(#[from] wasmtime::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid WASM file")]
    InvalidWasm,

    #[error("WASM memory access error: {0}")]
    WasmMemoryAccess(String),

    #[error("Request error: {0}")]
    Request(#[from] rquest::Error),

    #[error("Lock poisoned")]
    LockPoisoned,

    #[error("Channel send error: {0}")]
    ChannelSend(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Integer conversion error: {0}")]
    IntConversion(#[from] std::num::TryFromIntError),

    #[error("Extension error: {0}")]
    Extension(String),

    #[error("{0}")]
    Other(String),
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Error::LockPoisoned
    }
}

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(error: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::ChannelSend(error.to_string())
    }
}

/// Convenient Result type alias.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::NotFound("manga 123".to_string());
        assert_eq!(err.to_string(), "Not found: manga 123");
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }
}
