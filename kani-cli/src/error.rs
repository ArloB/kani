//! CLI failure types and source-span diagnostic rendering.

#[derive(thiserror::Error, Debug)]
/// Failure returned by command orchestration and extension tooling.
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
    #[error("Conversion error: {message}")]
    DslConversion {
        message: String,
        span: std::ops::Range<usize>,
    },
    #[error("{0}")]
    Other(String),
}

impl From<kani_yaml::YamlError> for CliError {
    fn from(e: kani_yaml::YamlError) -> Self {
        match e {
            kani_yaml::YamlError::DslConversion { message, span } => {
                Self::DslConversion { message, span }
            }
            kani_yaml::YamlError::Validation(msg) => Self::Other(msg),
        }
    }
}

use ariadne::{Color, Label, Report, ReportKind, Source};

pub fn report_errors(filename: &str, source: &str, errors: Vec<chumsky::error::Rich<char>>) {
    for error in errors {
        let span = error.span();
        let range = span.start..span.end;

        Report::build(ReportKind::Error, (filename, range.clone()))
            .with_message(error.to_string())
            .with_label(
                Label::new((filename, range))
                    .with_message("Found here")
                    .with_color(Color::Red),
            )
            .finish()
            .eprint((filename, Source::from(source)))
            .expect("failed to write diagnostic to stderr");
    }
}

pub fn report_custom_error(
    filename: &str,
    source: &str,
    message: &str,
    range: std::ops::Range<usize>,
) {
    use ariadne::{Color, Label, Report, ReportKind, Source};

    Report::build(ReportKind::Error, (filename, range.clone()))
        .with_message("Logic Error")
        .with_label(
            Label::new((filename, range))
                .with_message(message)
                .with_color(Color::Yellow),
        )
        .finish()
        .eprint((filename, Source::from(source)))
        .expect("failed to write diagnostic to stderr");
}
