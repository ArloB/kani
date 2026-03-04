//! Source host management for WASM extensions.

use crate::error::{Error, Result};
use crate::wasm::HostState;
use std::time::Instant;
use wasmtime::Store;
use wasmtime::component::Linker;

macro_rules! execute_wasm {
    ($self:expr, $method:ident $(, $args:expr)*) => {{
        let bindings = $self
            .bindings
            .as_ref()
            .expect("Bindings should be initialized");
        let store = $self
            .store
            .as_mut()
            .ok_or_else(|| Error::Internal("Store not initialized".to_string()))?;

        let provider = bindings.kani_extension_manga_provider();
        let result = provider.$method(&mut *store $(, $args)*)
            .await
            .map_err(|e| Error::Internal(format!("WASM function call failed: {}", e)))?
            .map_err(Error::Extension)?;

        store.data_mut().clear_all();
        $self.last_call = Some(Instant::now());
        Ok(result)
    }};
}

/// Hosts a single WASM source extension.
pub struct SourceInstance {
    store: Option<Store<HostState>>,
    bindings: Option<crate::wasm::KaniExtension>,
    last_call: Option<Instant>,
    smart_client: crate::http::SmartClient,
    base_url: Option<String>,
}

impl SourceInstance {
    pub fn new(smart_client: crate::http::SmartClient, base_url: Option<String>) -> Self {
        Self {
            store: None,
            bindings: None,
            last_call: None,
            smart_client,
            base_url,
        }
    }

    /// Calls the `get_popular_manga` function in the WASM module.
    pub async fn get_popular_manga(
        &mut self,
        page: i32,
    ) -> Result<crate::wasm::kani::extension::types::MangaList> {
        execute_wasm!(self, call_get_popular_manga, page)
    }

    /// Calls the `search_manga` function in the WASM module.
    pub async fn search_manga(
        &mut self,
        query: &str,
        page: i32,
    ) -> Result<crate::wasm::kani::extension::types::MangaList> {
        execute_wasm!(self, call_search_manga, query, page)
    }

    /// Calls the `get_manga_details` function in the WASM module.
    pub async fn get_manga_details(
        &mut self,
        manga_id: &str,
    ) -> Result<crate::wasm::kani::extension::types::MangaInfo> {
        execute_wasm!(self, call_get_manga_details, manga_id)
    }

    /// Calls the `get_chapter_list` function in the WASM module.
    pub async fn get_chapter_list(
        &mut self,
        manga_id: &str,
        page: i32,
    ) -> Result<crate::wasm::kani::extension::types::ChapterList> {
        execute_wasm!(self, call_get_chapter_list, manga_id, page)
    }

    /// Calls the `get_pages` function in the WASM module.
    pub async fn get_pages(
        &mut self,
        manga_id: &str,
        chapter_id: &str,
    ) -> Result<crate::wasm::kani::extension::types::Chapter> {
        execute_wasm!(self, call_get_pages, manga_id, chapter_id)
    }

    /// Calls the `get_metadata` function in the WASM module.
    pub async fn get_metadata(
        &mut self,
    ) -> Result<crate::wasm::kani::extension::types::ExtensionMetadata> {
        execute_wasm!(self, call_get_metadata)
    }

    /// Returns `Some(true)` if this instance has been idle longer than `timeout`,
    /// `Some(false)` if it is still within the window, or `None` if never called.
    pub(crate) fn is_idle(&self, timeout: std::time::Duration) -> Option<bool> {
        self.last_call.map(|t| t.elapsed() > timeout)
    }

    /// Loads a source from the component.
    pub async fn load(
        &mut self,
        engine: &wasmtime::Engine,
        component: &wasmtime::component::Component,
        linker: &Linker<HostState>,
    ) -> Result<()> {
        let mut store = Store::new(
            engine,
            HostState::new(self.smart_client.clone(), self.base_url.clone())?,
        );

        let bindings = crate::wasm::KaniExtension::instantiate_async(&mut store, component, linker)
            .await
            .map_err(|e| Error::Internal(format!("Failed to instantiate: {}", e)))?;

        self.store = Some(store);
        self.bindings = Some(bindings);
        self.last_call = Some(Instant::now());

        Ok(())
    }
}
