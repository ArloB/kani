// Constant boilerplate strings for generated extension source.

/// Default MangaExtension stub methods emitted when an endpoint is absent.
pub const MANGA_EXT_STUB_SORT: &str = r#"
    fn get_chapter_sort_list(&self) -> ExtensionResult<Vec<wit_types::ChapterSortOption>> {
        Ok(vec![])
    }
"#;
