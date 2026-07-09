pub mod loader;
pub mod registry;
pub mod signing;
pub mod wasm_source;
pub mod yaml_source;

pub use registry::SourceRegistry;
pub use wasm_source::WasmSource;
pub use yaml_source::YamlSource;

pub enum SourceBackend {
    Wasm(Box<WasmSource>),
    Yaml(Box<YamlSource>),
}

impl SourceBackend {
    pub fn backend_kind(&self) -> &'static str {
        match self {
            Self::Wasm(_) => "wasm",
            Self::Yaml(_) => "yaml",
        }
    }

    pub fn update_preferences(&self, prefs: std::collections::HashMap<String, String>) {
        match self {
            Self::Wasm(w) => w.update_preferences(prefs),
            Self::Yaml(y) => y.update_preferences(prefs),
        }
    }

    pub async fn get_metadata(&self) -> kani_core::error::Result<String> {
        match self {
            Self::Wasm(w) => w.lease_instance().await?.get_metadata().await,
            Self::Yaml(y) => y.get_metadata().await,
        }
    }

    pub async fn get_popular_manga(
        &self,
        page: i32,
        page_size: i32,
        filters: &[kani_shared::types::ActiveFilter],
    ) -> kani_core::error::Result<kani_core::wasm::kani::extension::types::MangaList> {
        match self {
            Self::Wasm(w) => {
                w.lease_instance()
                    .await?
                    .get_popular_manga(page, page_size, filters)
                    .await
            }
            Self::Yaml(y) => y.get_popular_manga(page, page_size, filters).await,
        }
    }

    pub async fn search_manga(
        &self,
        query: &str,
        page: i32,
        page_size: i32,
        filters: &[kani_shared::types::ActiveFilter],
    ) -> kani_core::error::Result<kani_core::wasm::kani::extension::types::MangaList> {
        match self {
            Self::Wasm(w) => {
                w.lease_instance()
                    .await?
                    .search_manga(query, page, page_size, filters)
                    .await
            }
            Self::Yaml(y) => y.search_manga(query, page, page_size, filters).await,
        }
    }

    pub async fn get_filter_list_with_options(
        &self,
    ) -> kani_core::error::Result<(kani_core::WitFilterList, String)> {
        match self {
            Self::Wasm(w) => {
                let mut instance = w.lease_instance().await?;
                let filter_list = instance.get_filter_list().await?;
                let fetched = instance
                    .get_fetched_option_sets()
                    .await
                    .unwrap_or_else(|_| "[]".to_string());
                Ok((filter_list, fetched))
            }
            Self::Yaml(y) => {
                let filter_list = y.get_filter_list().await?;
                let fetched = y
                    .get_fetched_option_sets()
                    .await
                    .unwrap_or_else(|_| "[]".to_string());
                Ok((filter_list, fetched))
            }
        }
    }

    pub async fn get_source_url(&self, manga_id: &str) -> kani_core::error::Result<String> {
        match self {
            Self::Wasm(w) => w.lease_instance().await?.get_url(manga_id).await,
            Self::Yaml(y) => y.get_source_url(manga_id).await,
        }
    }

    pub async fn get_manga_details(
        &self,
        manga_id: &str,
    ) -> kani_core::error::Result<kani_core::wasm::kani::extension::types::MangaInfo> {
        match self {
            Self::Wasm(w) => w.lease_instance().await?.get_manga_details(manga_id).await,
            Self::Yaml(y) => y.get_manga_details(manga_id).await,
        }
    }

    pub async fn get_pages(
        &self,
        manga_id: &str,
        chapter_id: &str,
    ) -> kani_core::error::Result<kani_core::wasm::kani::extension::types::Chapter> {
        match self {
            Self::Wasm(w) => {
                w.lease_instance()
                    .await?
                    .get_pages(manga_id, chapter_id)
                    .await
            }
            Self::Yaml(y) => y.get_pages(manga_id, chapter_id).await,
        }
    }

    pub async fn get_chapter_list(
        &self,
        manga_id: &str,
        page: i32,
        page_size: Option<i32>,
        sort: Option<String>,
    ) -> kani_core::error::Result<kani_core::wasm::kani::extension::types::ChapterList> {
        match self {
            Self::Wasm(w) => {
                w.lease_instance()
                    .await?
                    .get_chapter_list(manga_id, page, page_size, sort)
                    .await
            }
            Self::Yaml(y) => y.get_chapter_list(manga_id, page, page_size, sort).await,
        }
    }

    pub async fn get_chapter_sort_list(
        &self,
    ) -> kani_core::error::Result<Vec<kani_core::wasm::kani::extension::types::SortOption>> {
        match self {
            Self::Wasm(w) => w.lease_instance().await?.get_chapter_sort_list().await,
            Self::Yaml(y) => y.get_chapter_sort_list().await,
        }
    }

    pub async fn get_preferences(
        &self,
    ) -> kani_core::error::Result<Vec<kani_core::wasm::kani::extension::types::PreferenceSpec>>
    {
        match self {
            Self::Wasm(w) => w.lease_instance().await?.get_preferences().await,
            Self::Yaml(y) => y.get_preferences().await,
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
impl SourceBackend {
    pub fn is_yaml(&self) -> bool {
        matches!(self, Self::Yaml(_))
    }
}

#[async_trait::async_trait]
impl kani_core::downloader::PageListFetcher for SourceBackend {
    async fn fetch_page_list(
        &self,
        manga_id: &str,
        chapter_id: &str,
    ) -> kani_core::error::Result<(kani_core::wasm::kani::extension::types::Chapter, String)> {
        match self {
            Self::Wasm(w) => w.fetch_page_list_impl(manga_id, chapter_id).await,
            Self::Yaml(y) => y.fetch_page_list(manga_id, chapter_id).await,
        }
    }
}
