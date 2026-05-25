use kani_shared::bindings::exports::kani::extension::manga_provider::Guest;
use kani_shared::{
    ExtensionResult, MangaExtension, MangaStatus, bindings, ext_version, to_shared_filters,
    types::ActiveFilter, wit_types,
};
use wit_types::{Chapter, ChapterList, ExtensionMetadata, MangaInfo, MangaList, PreferenceSpec};

#[cfg(target_family = "wasm")]
#[global_allocator]
static ALLOCATOR: talc::TalckWasm = unsafe { talc::TalckWasm::new_global() };

pub struct Example {
    _base_url: String,
}

impl Default for Example {
    fn default() -> Self {
        Self::new()
    }
}

impl Example {
    pub fn new() -> Self {
        Self {
            _base_url: "https://example.com".to_string(),
        }
    }

    pub fn metadata() -> ExtensionMetadata {
        ExtensionMetadata {
            id: "example".to_string(),
            name: "Example".to_string(),
            version: ext_version!("0.1.0"),
            base_url: "https://example.com".to_string(),
            language: "multi".to_string(),
            nsfw: false,
            unrestricted_http: false,
            mihon_source_id: None,
        }
    }
}

impl Guest for Example {
    fn get_metadata() -> Result<ExtensionMetadata, String> {
        Ok(Example::metadata())
    }

    fn get_popular_manga(
        page: i32,
        page_size: i32,
        filters: Vec<wit_types::ActiveFilter>,
    ) -> Result<MangaList, String> {
        let shared = to_shared_filters(filters);
        get_extension()
            .get_popular_manga(page, page_size, &shared)
            .map_err(|e| e.to_string())
    }

    fn search_manga(
        query: String,
        page: i32,
        page_size: i32,
        filters: Vec<wit_types::ActiveFilter>,
    ) -> Result<MangaList, String> {
        let shared = to_shared_filters(filters);
        get_extension()
            .search_manga(&query, page, page_size, &shared)
            .map_err(|e| e.to_string())
    }

    fn get_filter_list() -> Result<wit_types::FilterList, String> {
        get_extension().get_filter_list().map_err(|e| e.to_string())
    }

    fn get_manga_details(manga_id: String) -> Result<MangaInfo, String> {
        get_extension()
            .get_manga_details(&manga_id)
            .map_err(|e| e.to_string())
    }

    fn get_chapter_list(
        manga_id: String,
        page: i32,
        page_size: Option<i32>,
        sort: Option<String>,
    ) -> Result<ChapterList, String> {
        get_extension()
            .get_chapter_list(&manga_id, page, page_size, sort)
            .map_err(|e| e.to_string())
    }

    fn get_chapter_sort_list() -> Result<Vec<wit_types::ChapterSortOption>, String> {
        get_extension()
            .get_chapter_sort_list()
            .map_err(|e| e.to_string())
    }

    fn get_pages(manga_id: String, chapter_id: String) -> Result<Chapter, String> {
        get_extension()
            .get_pages(&manga_id, &chapter_id)
            .map_err(|e| e.to_string())
    }

    fn get_preferences() -> Result<Vec<PreferenceSpec>, String> {
        get_extension().get_preferences().map_err(|e| e.to_string())
    }
    fn get_url(manga_id: String) -> Result<String, String> {
        get_extension()
            .get_url(&manga_id)
            .map_err(|e| e.to_string())
    }
}

impl MangaExtension for Example {
    fn name(&self) -> &str {
        "Example"
    }

    fn get_popular_manga(
        &self,
        _page: i32,
        _page_size: i32,
        _filters: &[ActiveFilter],
    ) -> ExtensionResult<MangaList> {
        Ok(MangaList {
            manga: vec![],
            has_next_page: false,
            total_pages: None,
        })
    }

    fn search_manga(
        &self,
        _query: &str,
        _page: i32,
        _page_size: i32,
        _filters: &[ActiveFilter],
    ) -> ExtensionResult<MangaList> {
        Ok(MangaList {
            manga: vec![],
            has_next_page: false,
            total_pages: None,
        })
    }

    fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo> {
        Ok(MangaInfo {
            id: manga_id.to_string(),
            title: "Example".to_string(),
            description: Some("Example".to_string()),
            status: MangaStatus::Ongoing,
            authors: vec![],
            artists: vec![],
            tags: vec![],
            cover_url: Some("https://example.com/cover.jpg".to_string()),
        })
    }

    fn get_chapter_list(
        &self,
        _manga_id: &str,
        _page: i32,
        _page_size: Option<i32>,
        _sort: Option<String>,
    ) -> ExtensionResult<ChapterList> {
        Ok(ChapterList {
            chapters: vec![],
            has_next_page: false,
            total_pages: None,
        })
    }

    fn get_pages(&self, _manga_id: &str, _chapter_id: &str) -> ExtensionResult<Chapter> {
        Ok(Chapter { pages: vec![] })
    }

    fn get_filter_list(&self) -> ExtensionResult<wit_types::FilterList> {
        Ok(wit_types::FilterList { filters: vec![] })
    }

    fn get_preferences(&self) -> ExtensionResult<Vec<PreferenceSpec>> {
        Ok(vec![])
    }

    fn get_chapter_sort_list(&self) -> ExtensionResult<Vec<wit_types::ChapterSortOption>> {
        Ok(vec![])
    }
}

// ============================================================
// WASM Exports
// ============================================================

use std::sync::OnceLock;

static EXTENSION: OnceLock<Example> = OnceLock::new();

fn get_extension() -> &'static Example {
    EXTENSION.get_or_init(Example::new)
}

bindings::export!(Example);
