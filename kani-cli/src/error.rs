#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error("build failed for `{0}`")]
    BuildFailed(String),
    #[error("`wasm-tools` not found — install it to produce WASM components")]
    WasmToolsMissing,
    #[error("extension not found: `{0}`")]
    ExtensionNotFound(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}