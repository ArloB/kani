//! MangaDex extension for Kani manga downloader.
//!
//! This crate implements the MangaExtension trait for MangaDex,
//! and can be compiled to WASM for use with the Kani host.

use kani_shared::{
    Chapter, ChapterInfo, ChapterList, ExtensionError, ExtensionMetadata, ExtensionResult,
    MangaExtension, MangaInfo, MangaList, MangaStatus, Page, HttpMethod,
};

/// MangaDex source implementation.
pub struct MangaDex {
    base_url: String,
}

impl Default for MangaDex {
    fn default() -> Self {
        Self::new()
    }
}

impl MangaDex {
    pub fn new() -> Self {
        Self {
            base_url: "https://api.mangadex.org".to_string(),
        }
    }

    /// Returns metadata about this extension.
    pub fn metadata() -> ExtensionMetadata {
        ExtensionMetadata {
            id: "mangadex".to_string(),
            name: "MangaDex".to_string(),
            version: "0.1.0".to_string(),
            base_url: "https://mangadex.org".to_string(),
            language: "multi".to_string(),
            nsfw: false,
        }
    }
}

impl MangaExtension for MangaDex {
    fn name(&self) -> &str {
        "MangaDex"
    }

    fn get_popular_manga(&self, page: i32) -> ExtensionResult<MangaList> {
        // TODO: Implement actual MangaDex API call
        // This is a stub that returns empty results
        Ok(MangaList {
            manga: vec![],
            has_next_page: false,
        })
    }

    fn search_manga(&self, query: &str, page: i32) -> ExtensionResult<MangaList> {
        // TODO: Implement actual MangaDex API call
        Ok(MangaList {
            manga: vec![],
            has_next_page: false,
        })
    }

    fn get_manga_details(&self, manga_id: &str) -> ExtensionResult<MangaInfo> {
        // TODO: Implement actual MangaDex API call
        Err(ExtensionError::NotFound(format!(
            "Manga {} not found",
            manga_id
        )))
    }

    fn get_chapter_list(&self, manga_id: &str) -> ExtensionResult<ChapterList> {
        // TODO: Implement actual MangaDex API call
        Ok(ChapterList { chapters: vec![] })
    }

    fn get_pages(&self, manga_id: &str, chapter_id: &str) -> ExtensionResult<Chapter> {
        // TODO: Implement actual MangaDex API call
        Err(ExtensionError::NotFound(format!(
            "Chapter {} not found",
            chapter_id
        )))
    }
}

// ============================================================
// WASM Exports
// ============================================================
// These functions are the entry points called by the host when
// this extension is loaded as a WASM module.

use std::sync::OnceLock;

static EXTENSION: OnceLock<MangaDex> = OnceLock::new();

fn get_extension() -> &'static MangaDex {
    EXTENSION.get_or_init(MangaDex::new)
}

/// Allocates memory for the host to write data into.
#[unsafe(no_mangle)]
pub extern "C" fn allocate(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Deallocates memory previously allocated by allocate.
#[unsafe(no_mangle)]
pub extern "C" fn deallocate(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

/// Returns (ptr, len) for popular manga JSON.
#[unsafe(no_mangle)]
pub extern "C" fn get_popular_manga(page: i32) -> u64 {
    let ext = get_extension();
    let result = match ext.get_popular_manga(page) {
        Ok(list) => serde_json::to_string(&list).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

/// Returns (ptr, len) for search results JSON.
#[unsafe(no_mangle)]
pub extern "C" fn search_manga(page: i32, query_ptr: i32, query_len: i32) -> u64 {
    let query = unsafe {
        let slice = std::slice::from_raw_parts(query_ptr as *const u8, query_len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    let ext = get_extension();
    let result = match ext.search_manga(&query, page) {
        Ok(list) => serde_json::to_string(&list).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

/// Returns (ptr, len) for manga details JSON.
#[unsafe(no_mangle)]
pub extern "C" fn get_manga_details(manga_id: i32) -> u64 {
    let ext = get_extension();
    let result = match ext.get_manga_details(&manga_id.to_string()) {
        Ok(info) => serde_json::to_string(&info).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

/// Returns (ptr, len) for chapter pages JSON.
#[unsafe(no_mangle)]
pub extern "C" fn get_pages(manga_id: i32, chapter_id: i32) -> u64 {
    let ext = get_extension();
    let result = match ext.get_pages(&manga_id.to_string(), &chapter_id.to_string()) {
        Ok(chapter) => serde_json::to_string(&chapter).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

/// Converts a string to a packed (ptr, len) u64.
fn string_to_ptr_len(s: &str) -> u64 {
    let bytes = s.as_bytes().to_vec();
    let len = bytes.len() as u32;
    let ptr = bytes.as_ptr() as u32;
    std::mem::forget(bytes);
    ((ptr as u64) << 32) | (len as u64)
}
