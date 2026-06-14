use crate::{
    types::ActiveFilter,
    wit_types::{
        Chapter, ChapterList, ChapterSortOption, FilterList, MangaInfo, MangaList, PreferenceSpec,
    },
};

pub type ExtensionResult<T> = Result<T, ExtensionError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionErrorKind {
    Network,
    Parse,
    NotFound,
    RateLimited,
    Auth,
    ContentUnavailable,
    Timeout,
    InvalidInput,
    Internal,
    Unknown,
}

impl std::fmt::Display for ExtensionErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network => write!(f, "network"),
            Self::Parse => write!(f, "parse"),
            Self::NotFound => write!(f, "not-found"),
            Self::RateLimited => write!(f, "rate-limited"),
            Self::Auth => write!(f, "auth"),
            Self::ContentUnavailable => write!(f, "content-unavailable"),
            Self::Timeout => write!(f, "timeout"),
            Self::InvalidInput => write!(f, "invalid-input"),
            Self::Internal => write!(f, "internal"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionError {
    pub kind: ExtensionErrorKind,
    pub message: String,
    pub source_url: Option<String>,
    pub retry_after_secs: Option<u32>,
}

impl ExtensionError {
    pub fn network(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::Network,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn parse(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::Parse,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn not_found(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::NotFound,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn rate_limited() -> Self {
        Self {
            kind: ExtensionErrorKind::RateLimited,
            message: "Rate limited".to_string(),
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn rate_limited_with_retry(secs: u32) -> Self {
        Self {
            kind: ExtensionErrorKind::RateLimited,
            message: "Rate limited".to_string(),
            source_url: None,
            retry_after_secs: Some(secs),
        }
    }

    pub fn auth(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::Auth,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn content_unavailable(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::ContentUnavailable,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn timeout(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::Timeout,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn invalid_input(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::InvalidInput,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn internal(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::Internal,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn unknown(message: String) -> Self {
        Self {
            kind: ExtensionErrorKind::Unknown,
            message,
            source_url: None,
            retry_after_secs: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.source_url = Some(url.into());
        self
    }

    pub fn into_wit(self) -> crate::bindings::kani::extension::types::ExtensionError {
        use crate::bindings::kani::extension::types::{
            ExtensionError as WitErr, ExtensionErrorKind as WitKind,
        };
        let wit_kind = match self.kind {
            ExtensionErrorKind::Network => WitKind::Network,
            ExtensionErrorKind::Parse => WitKind::Parse,
            ExtensionErrorKind::NotFound => WitKind::NotFound,
            ExtensionErrorKind::RateLimited => WitKind::RateLimited,
            ExtensionErrorKind::Auth => WitKind::Auth,
            ExtensionErrorKind::ContentUnavailable => WitKind::ContentUnavailable,
            ExtensionErrorKind::Timeout => WitKind::Timeout,
            ExtensionErrorKind::InvalidInput => WitKind::InvalidInput,
            ExtensionErrorKind::Internal => WitKind::Internal,
            ExtensionErrorKind::Unknown => WitKind::Unknown,
        };
        WitErr {
            kind: wit_kind,
            message: self.message,
            source_url: self.source_url,
            retry_after_secs: self.retry_after_secs,
        }
    }
}

impl std::fmt::Display for ExtensionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.kind, self.message)?;
        if let Some(url) = &self.source_url {
            write!(f, " (url: {})", url)?;
        }
        Ok(())
    }
}

impl std::error::Error for ExtensionError {}

impl From<String> for ExtensionError {
    fn from(s: String) -> Self {
        Self::unknown(s)
    }
}

pub trait MangaExtension {
    fn name(&self) -> &str;

    fn get_popular_manga(
        &self,
        page: i32,
        page_size: i32,
        filters: &[ActiveFilter],
    ) -> ExtensionResult<MangaList>;

    fn search_manga(
        &self,
        query: &str,
        page: i32,
        page_size: i32,
        filters: &[ActiveFilter],
    ) -> ExtensionResult<MangaList>;

    fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo>;

    fn get_chapter_list(
        &self,
        manga_id: &str,
        page: i32,
        page_size: Option<i32>,
        sort: Option<String>,
    ) -> ExtensionResult<ChapterList>;

    fn get_pages(&self, manga_id: &str, chapter_id: &str) -> ExtensionResult<Chapter>;

    fn get_chapter_sort_list(&self) -> ExtensionResult<Vec<ChapterSortOption>>;

    fn get_filter_list(&self) -> ExtensionResult<FilterList>;

    fn get_preferences(&self) -> ExtensionResult<Vec<PreferenceSpec>>;

    fn get_url(&self, _manga_id: &str) -> ExtensionResult<String> {
        Err(ExtensionError::not_found(
            "get_url not implemented for this source".into(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "host", derive(serde::Serialize, serde::Deserialize))]
pub struct RateLimitConfig {
    pub requests_per_second: f32,
    pub burst: u32,
    pub max_concurrent: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 2.0,
            burst: 8,
            max_concurrent: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "host", derive(serde::Serialize, serde::Deserialize))]
pub struct ExtensionMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub base_url: String,
    pub language: String,
    pub nsfw: bool,
    pub unrestricted_http: bool,
    #[cfg_attr(feature = "host", serde(default))]
    pub mihon_source_id: Option<i64>,
    #[cfg_attr(feature = "host", serde(default))]
    pub rate_limit: Option<RateLimitConfig>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn kind_display() {
        assert_eq!(ExtensionErrorKind::Network.to_string(), "network");
        assert_eq!(ExtensionErrorKind::RateLimited.to_string(), "rate-limited");
    }

    #[test]
    fn constructors_set_kind() {
        assert_eq!(
            ExtensionError::network("x".into()).kind,
            ExtensionErrorKind::Network
        );
        assert_eq!(
            ExtensionError::parse("x".into()).kind,
            ExtensionErrorKind::Parse
        );
        assert_eq!(
            ExtensionError::not_found("x".into()).kind,
            ExtensionErrorKind::NotFound
        );
        assert_eq!(
            ExtensionError::rate_limited().kind,
            ExtensionErrorKind::RateLimited
        );
        assert_eq!(
            ExtensionError::auth("x".into()).kind,
            ExtensionErrorKind::Auth
        );
        assert_eq!(
            ExtensionError::timeout("x".into()).kind,
            ExtensionErrorKind::Timeout
        );
        assert_eq!(
            ExtensionError::unknown("x".into()).kind,
            ExtensionErrorKind::Unknown
        );
    }

    #[test]
    fn rate_limited_with_retry_sets_secs() {
        let e = ExtensionError::rate_limited_with_retry(30);
        assert_eq!(e.kind, ExtensionErrorKind::RateLimited);
        assert_eq!(e.retry_after_secs, Some(30));
    }

    #[test]
    fn with_url_stores_url() {
        let e = ExtensionError::network("err".into()).with_url("https://example.com/page");
        assert_eq!(e.source_url.as_deref(), Some("https://example.com/page"));
    }

    #[test]
    fn display_includes_kind_and_message() {
        let e = ExtensionError::network("connection refused".into());
        let s = e.to_string();
        assert!(s.contains("network"));
        assert!(s.contains("connection refused"));
    }

    #[test]
    #[allow(deprecated)]
    fn from_string_shim_produces_unknown() {
        let e = ExtensionError::from("oops".to_string());
        assert_eq!(e.kind, ExtensionErrorKind::Unknown);
        assert_eq!(e.message, "oops");
    }
}
