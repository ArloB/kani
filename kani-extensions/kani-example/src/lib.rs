use kani_shared::bindings::exports::kani::extension::manga_provider::Guest;
use kani_shared::{
    Chapter, ChapterList, ExtensionMetadata, ExtensionResult, FilterList, MangaExtension,
    MangaInfo, MangaList, MangaStatus, PreferenceList, bindings,
};

pub struct Example {
    _base_url: String,
    _return_limit: i32,
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
            _return_limit: 65,
        }
    }

    pub fn metadata() -> ExtensionMetadata {
        ExtensionMetadata {
            id: "example".to_string(),
            name: "Example".to_string(),
            version: "0.1.0".to_string(),
            base_url: "https://example.com".to_string(),
            language: "multi".to_string(),
            nsfw: false,
        }
    }
}

impl Guest for Example {
    fn get_metadata() -> Result<ExtensionMetadata, String> {
        Ok(Example::metadata())
    }

    fn get_popular_manga(page: i32) -> Result<MangaList, String> {
        get_extension()
            .get_popular_manga(page)
            .map_err(|e| e.to_string())
    }

    fn search_manga(query: String, page: i32) -> Result<MangaList, String> {
        get_extension()
            .search_manga(&query, page)
            .map_err(|e| e.to_string())
    }

    fn get_manga_details(manga_id: String) -> Result<MangaInfo, String> {
        get_extension()
            .get_manga_details(&manga_id)
            .map_err(|e| e.to_string())
    }

    fn get_chapter_list(manga_id: String, page: i32) -> Result<ChapterList, String> {
        get_extension()
            .get_chapter_list(&manga_id, page)
            .map_err(|e| e.to_string())
    }

    fn get_pages(manga_id: String, chapter_id: String) -> Result<Chapter, String> {
        get_extension()
            .get_pages(&manga_id, &chapter_id)
            .map_err(|e| e.to_string())
    }
}

impl MangaExtension for Example {
    fn name(&self) -> &str {
        "Example"
    }

    fn get_popular_manga(&self, _page: i32) -> ExtensionResult<MangaList> {
        let manga_list = Vec::new();
        let has_next_page = false;

        Ok(MangaList {
            manga: manga_list,
            has_next_page,
        })
    }

    fn search_manga(&self, _query: &str, _page: i32) -> ExtensionResult<MangaList> {
        let manga_list = Vec::new();
        let has_next_page = false;

        Ok(MangaList {
            manga: manga_list,
            has_next_page,
        })
    }

    fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo> {
        let manga_info = MangaInfo {
            id: manga_id.to_string(),
            title: "Example".to_string(),
            description: Some("Example".to_string()),
            status: MangaStatus::Ongoing,
            authors: Vec::new(),
            artists: Vec::new(),
            tags: Vec::new(),
            cover_url: Some("https://example.com/cover.jpg".to_string()),
        };

        Ok(manga_info)
    }

    fn get_chapter_list(&self, _manga_id: &str, _page: i32) -> ExtensionResult<ChapterList> {
        let chapter_list = ChapterList {
            chapters: Vec::new(),
            has_next_page: false,
        };

        Ok(chapter_list)
    }

    fn get_pages(&self, _manga_id: &str, _chapter_id: &str) -> ExtensionResult<Chapter> {
        let chapter_info = Chapter {
            chapter_name: "Example".to_string(),
            pages: Vec::new(),
        };

        Ok(chapter_info)
    }

    fn get_filter_list(&self) -> ExtensionResult<FilterList> {
        Ok(FilterList { filters: vec![] })
    }

    fn get_preferences(&self) -> ExtensionResult<PreferenceList> {
        Ok(PreferenceList {
            preferences: vec![],
        })
    }

    fn set_preferences(&self, _json_ptr: i32) -> ExtensionResult<()> {
        Ok(())
    }
}

// ============================================================
// WASM Exports
// ============================================================
// These functions are the entry points called by the host when
// this extension is loaded as a WASM module.

use std::sync::OnceLock;

static EXTENSION: OnceLock<Example> = OnceLock::new();

fn get_extension() -> &'static Example {
    EXTENSION.get_or_init(Example::new)
}

bindings::export!(Example);
