//! WASM runtime and host ABI for extensions.

pub mod abi;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store};

use crate::error::Result;

pub mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/kani.wit",
        world: "kani-extension",
        imports: {
            "kani:extension/http": async,
        },
        exports: {
            default: async
        },
        additional_derives: [serde::Serialize, serde::Deserialize]
    });
}

pub use bindings::KaniExtension;
pub use bindings::exports;
pub use bindings::kani;
pub struct ResponseData {
    pub body: String,
    pub status: u16,
}

pub struct SafeHtml(pub scraper::Html);
unsafe impl Send for SafeHtml {}
unsafe impl Sync for SafeHtml {}

impl SafeHtml {
    pub fn parse_document(html: &str) -> Self {
        Self(scraper::Html::parse_document(html))
    }

    pub fn parse_fragment(html: &str) -> Self {
        Self(scraper::Html::parse_fragment(html))
    }
}

pub struct SendHtml(pub Arc<Mutex<SafeHtml>>);

impl SendHtml {
    pub fn parse_document(html: &str) -> Self {
        Self(Arc::new(Mutex::new(SafeHtml::parse_document(html))))
    }

    pub fn parse_fragment(html: &str) -> Self {
        Self(Arc::new(Mutex::new(SafeHtml::parse_fragment(html))))
    }
}

#[derive(Clone)]
pub struct StoredNode {
    pub doc: Arc<Mutex<SafeHtml>>,
    pub node_id: ego_tree::NodeId,
}

use crate::http::SmartClient;

/// Host state passed to WASM guest via Store.
/// Manages handles for HTTP requests, responses, and parsed HTML documents.
pub struct HostState {
    pub http_client: SmartClient,
    pub allowed_host: Option<String>,
    pub next_doc_handle: i32,
    pub html_docs: HashMap<i32, StoredNode>,
    pub html_lists: HashMap<i32, Vec<StoredNode>>,
    pub selector_cache: HashMap<String, scraper::Selector>,
    pub last_error: Option<i32>,
}

impl HostState {
    pub fn new(http_client: SmartClient, allowed_host: Option<String>) -> Result<Self> {
        let allowed_host = allowed_host.and_then(|raw| {
            raw.parse::<rquest::Url>()
                .ok()
                .and_then(|u| u.host_str().map(|h| h.to_string()))
        });

        Ok(Self {
            http_client,
            allowed_host,
            next_doc_handle: 1,
            html_docs: HashMap::new(),
            html_lists: HashMap::new(),
            selector_cache: HashMap::new(),
            last_error: None,
        })
    }

    pub fn clear_all(&mut self) {
        self.html_docs.clear();
        self.html_lists.clear();
        self.next_doc_handle = 1;
        // selector_cache is intentionally not cleared: selectors are
        // document-agnostic and can be reused across calls.
    }

    /// Returns a reference to the compiled selector, parsing and caching it on
    /// the first call for a given `selector` string.
    pub fn get_or_parse_selector(&mut self, selector: &str) -> Result<&scraper::Selector> {
        if !self.selector_cache.contains_key(selector) {
            let parsed = scraper::Selector::parse(selector)
                .map_err(|e| crate::error::Error::Internal(format!("Invalid selector: {:?}", e)))?;
            self.selector_cache.insert(selector.to_string(), parsed);
        }
        Ok(self.selector_cache.get(selector).unwrap())
    }
}

impl Default for HostState {
    fn default() -> Self {
        HostState::new(SmartClient::new(None).unwrap(), None).unwrap()
    }
}

/// WASM runtime wrapper with async support enabled.
pub struct WasmRuntime {
    engine: Engine,
    linker: Linker<HostState>,
}

impl WasmRuntime {
    pub fn new(max_instances: u32) -> Result<Self> {
        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        config.epoch_interruption(true);

        let mut pool = wasmtime::PoolingAllocationConfig::default();
        pool.total_component_instances(max_instances);

        config.allocation_strategy(wasmtime::InstanceAllocationStrategy::Pooling(pool));

        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);

        KaniExtension::add_to_linker::<_, HasSelf<HostState>>(&mut linker, |state| state)?;

        Ok(Self { engine, linker })
    }

    /// Returns a reference to the engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compiles a WASM component from bytes.
    pub fn compile_component(&self, bytes: &[u8]) -> Result<Component> {
        let component = Component::new(&self.engine, bytes)?;
        Ok(component)
    }

    /// Creates a new store with fresh host state.
    pub fn create_store(&self) -> Store<HostState> {
        Store::new(&self.engine, HostState::default())
    }

    /// Instantiates a component in the given store.
    pub async fn instantiate(
        &self,
        store: &mut Store<HostState>,
        component: &Component,
    ) -> Result<KaniExtension> {
        let binding = KaniExtension::instantiate_async(store, component, &self.linker).await?;
        Ok(binding)
    }

    /// Returns a reference to the linker.
    pub fn linker(&self) -> &Linker<HostState> {
        &self.linker
    }
}

impl std::fmt::Debug for WasmRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmRuntime")
            .field("engine", &"<wasmtime::Engine>")
            .finish()
    }
}
