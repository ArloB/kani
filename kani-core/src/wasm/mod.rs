//! WASM runtime and host ABI for extensions.

pub mod abi;

use std::collections::HashMap;

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

pub struct SendHtml(pub scraper::Html);
unsafe impl Send for SendHtml {}

impl SendHtml {
    pub fn parse_document(html: &str) -> Self {
        Self(scraper::Html::parse_document(html))
    }

    pub fn parse_fragment(html: &str) -> Self {
        Self(scraper::Html::parse_fragment(html))
    }
}

use crate::http::SmartClient;

/// Host state passed to WASM guest via Store.
/// Manages handles for HTTP requests, responses, and parsed HTML documents.
pub struct HostState {
    pub http_client: SmartClient,
    pub next_doc_handle: i32,
    pub html_docs: HashMap<i32, SendHtml>,
    pub html_lists: HashMap<i32, Vec<SendHtml>>,
    pub last_error: Option<i32>,
}

impl HostState {
    pub fn new(solver_url: Option<String>) -> Result<Self> {
        let http_client = SmartClient::new(solver_url)?;

        Ok(Self {
            http_client,
            next_doc_handle: 1,
            html_docs: HashMap::new(),
            html_lists: HashMap::new(),
            last_error: None,
        })
    }
}

impl Default for HostState {
    fn default() -> Self {
        HostState::new(None).unwrap()
    }
}

/// WASM runtime wrapper with async support enabled.
pub struct WasmRuntime {
    engine: Engine,
    linker: Linker<HostState>,
}

impl WasmRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);

        let mut pool = wasmtime::PoolingAllocationConfig::default();
        pool.total_component_instances(100);

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
