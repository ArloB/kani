use kani_shared::{
    Chapter, ChapterList, ExtensionMetadata, ExtensionResult, FilterList, MangaExtension,
    MangaInfo, MangaList, MangaStatus, PreferenceList,
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
            _return_limit: 20,
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

    fn get_pages(&self, _chapter_id: &str) -> ExtensionResult<Chapter> {
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

#[unsafe(no_mangle)]
pub extern "C" fn allocate(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[unsafe(no_mangle)]
/// # Safety
///
/// This function is called by the host to deallocate memory that was allocated by the extension.
pub unsafe extern "C" fn deallocate(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn get_popular_manga(page: i32) -> u64 {
    let ext = get_extension();
    let result = match ext.get_popular_manga(page) {
        Ok(list) => serde_json::to_string(&list).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn search_manga(query_ptr: i32, query_len: i32, page: i32) -> u64 {
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

#[unsafe(no_mangle)]
pub extern "C" fn get_manga_details(ptr: i32, len: i32) -> u64 {
    let manga_id = unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    let ext = get_extension();
    let result = match ext.get_manga_details(&manga_id) {
        Ok(info) => serde_json::to_string(&info).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn get_chapter_list(ptr: i32, len: i32, page: i32) -> u64 {
    let manga_id = unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    let ext = get_extension();
    let result = match ext.get_chapter_list(&manga_id, page) {
        Ok(list) => serde_json::to_string(&list).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn get_pages(ptr: i32, len: i32) -> u64 {
    let chapter_id = unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).to_string()
    };

    let ext = get_extension();

    let result = match ext.get_pages(&chapter_id) {
        Ok(chapter) => serde_json::to_string(&chapter).unwrap_or_default(),
        Err(e) => format!("{{\"error\":\"{}\"}}", e),
    };
    string_to_ptr_len(&result)
}

#[unsafe(no_mangle)]
pub extern "C" fn get_metadata() -> u64 {
    let metadata = Example::metadata();
    let result = serde_json::to_string(&metadata).unwrap_or_default();
    string_to_ptr_len(&result)
}

fn string_to_ptr_len(s: &str) -> u64 {
    let bytes = s.as_bytes().to_vec();
    let len = bytes.len() as u32;
    let ptr = bytes.as_ptr() as u32;
    std::mem::forget(bytes);
    ((ptr as u64) << 32) | (len as u64)
}
