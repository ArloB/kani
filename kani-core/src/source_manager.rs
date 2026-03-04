use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use wasmtime::component::{Component, Linker};

use crate::error::{Error, Result};
use crate::sources::SourceInstance;
use crate::wasm::HostState;

/// Manages a pool of source instances.
pub struct SourceManager {
    engine: wasmtime::Engine,
    component: Component,
    linker: Linker<HostState>,
    pool: Arc<Mutex<Vec<SourceInstance>>>,
    semaphore: Arc<Semaphore>,
    smart_client: crate::http::SmartClient,
    base_url: Option<String>,
    min_idle: usize,
}

impl SourceManager {
    pub async fn new(
        engine: wasmtime::Engine,
        component: Component,
        linker: Linker<HostState>,
        smart_client: crate::http::SmartClient,
        base_url: Option<String>,
        pool_size: usize,
        min_idle: usize,
    ) -> Result<Self> {
        let pool: Vec<SourceInstance> = Vec::new();
        let pool = Arc::new(Mutex::new(pool));

        {
            let mut locked = pool.lock().await;
            for _ in 0..min_idle.min(pool_size) {
                let mut inst = SourceInstance::new(smart_client.clone(), base_url.clone());
                inst.load(&engine, &component, &linker).await?;
                locked.push(inst);
            }
        }

        Ok(Self {
            engine,
            component,
            linker,
            pool,
            semaphore: Arc::new(Semaphore::new(pool_size)),
            smart_client,
            base_url,
            min_idle,
        })
    }

    pub async fn lease_instance(&self) -> Result<OwnedSourceInstance> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| Error::Internal(format!("Failed to acquire semaphore: {}", e)))?;

        let instance = {
            let mut pool = self.pool.lock().await;
            pool.pop()
        };

        let instance = match instance {
            Some(inst) => inst,
            None => {
                let mut inst =
                    SourceInstance::new(self.smart_client.clone(), self.base_url.clone());
                inst.load(&self.engine, &self.component, &self.linker)
                    .await?;
                inst
            }
        };
        Ok(OwnedSourceInstance {
            instance: Some(instance),
            pool: self.pool.clone(),
            _permit: Some(permit),
        })
    }

    /// Trims idle instances from the pool, always keeping at least `min_idle`.
    pub async fn cleanup(&self, idle_timeout: Duration) {
        let mut pool = self.pool.lock().await;
        let min_idle = self.min_idle;
        let mut retained = 0usize;

        pool.retain(|inst| {
            if retained < min_idle {
                retained += 1;
                return true;
            }

            inst.is_idle(idle_timeout).map(|idle| !idle).unwrap_or(true)
        });
    }
}

/// A wrapper around `SourceInstance` that returns it to the pool when dropped.
pub struct OwnedSourceInstance {
    instance: Option<SourceInstance>,
    pool: Arc<Mutex<Vec<SourceInstance>>>,
    _permit: Option<OwnedSemaphorePermit>,
}

impl OwnedSourceInstance {
    /// Calls the `get_popular_manga` function in the WASM module.
    pub async fn get_popular_manga(
        &mut self,
        page: i32,
    ) -> Result<crate::wasm::kani::extension::types::MangaList> {
        let instance = self.instance.as_mut().expect("Instance should be present");
        instance.get_popular_manga(page).await
    }

    /// Calls the `search_manga` function in the WASM module.
    pub async fn search_manga(
        &mut self,
        query: &str,
        page: i32,
    ) -> Result<crate::wasm::kani::extension::types::MangaList> {
        let instance = self.instance.as_mut().expect("Instance should be present");
        instance.search_manga(query, page).await
    }

    /// Calls the `get_manga_details` function in the WASM module.
    pub async fn get_manga_details(
        &mut self,
        manga_id: &str,
    ) -> Result<crate::wasm::kani::extension::types::MangaInfo> {
        let instance = self.instance.as_mut().expect("Instance should be present");
        instance.get_manga_details(manga_id).await
    }

    /// Calls the `get_chapter_list` function in the WASM module.
    pub async fn get_chapter_list(
        &mut self,
        manga_id: &str,
        page: i32,
    ) -> Result<crate::wasm::kani::extension::types::ChapterList> {
        let instance = self.instance.as_mut().expect("Instance should be present");
        instance.get_chapter_list(manga_id, page).await
    }

    /// Calls the `get_pages` function in the WASM module.
    pub async fn get_pages(
        &mut self,
        manga_id: &str,
        chapter_id: &str,
    ) -> Result<crate::wasm::kani::extension::types::Chapter> {
        let instance = self.instance.as_mut().expect("Instance should be present");
        instance.get_pages(manga_id, chapter_id).await
    }

    /// Calls the `get_metadata` function in the WASM module.
    pub async fn get_metadata(
        &mut self,
    ) -> Result<crate::wasm::kani::extension::types::ExtensionMetadata> {
        let instance = self.instance.as_mut().expect("Instance should be present");
        instance.get_metadata().await
    }
}

impl Drop for OwnedSourceInstance {
    fn drop(&mut self) {
        if let Some(instance) = self.instance.take() {
            let pool = self.pool.clone();
            let permit = self._permit.take();
            tokio::spawn(async move {
                let mut pool = pool.lock().await;
                pool.push(instance);
                drop(permit);
            });
        }
    }
}
