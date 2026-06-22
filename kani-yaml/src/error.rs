/// An error produced during YAML extension parsing or validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum YamlError {
    #[error("{message} at position {span:?}")]
    DslConversion {
        message: String,
        span: std::ops::Range<usize>,
    },
    #[error("{0}")]
    Validation(String),
}
