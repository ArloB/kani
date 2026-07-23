pub mod html_eval;
pub mod id_encoding;
pub mod json_eval;
pub(crate) mod shared;

/// Marker prefix on an evaluator error carrying an HTTP status to classify.
/// Re-exported so the host (YAML source) can decode it into a typed error.
pub use shared::HTTP_STATUS_ERR_PREFIX;
pub mod trace;

mod tests;
