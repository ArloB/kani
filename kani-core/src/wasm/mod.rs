//! WASM runtime and host ABI for extensions.

pub mod abi;
mod memory;

use std::collections::HashMap;

use wasmtime::{Config, Engine, Linker, Module, Store};

use crate::error::Result;

pub struct ResponseData {
    pub body: String,
    pub status: u16,
}

use crate::http::SmartClient;

/// Host state passed to WASM guest via Store.
/// Manages handles for HTTP requests, responses, and parsed HTML documents.
pub struct HostState {
    pub http_client: SmartClient,
    pub next_request_handle: i32,
    pub next_response_handle: i32,
    pub next_doc_handle: i32,
    pub requests: HashMap<i32, rquest::RequestBuilder>,
    pub responses: HashMap<i32, ResponseData>,
    pub html_docs: HashMap<i32, String>,
    pub html_lists: HashMap<i32, Vec<String>>,
    pub last_error: Option<i32>,
}

impl HostState {
    pub fn new(solver_url: Option<String>) -> Result<Self> {
        let http_client = SmartClient::new(solver_url)?;

        Ok(Self {
            http_client,
            next_request_handle: 1,
            next_response_handle: 1,
            next_doc_handle: 1,
            requests: HashMap::new(),
            responses: HashMap::new(),
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

        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);

        abi::register_host_functions(&mut linker)?;

        Ok(Self { engine, linker })
    }

    /// Returns a reference to the engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compiles a WASM module from bytes.
    pub fn compile_module(&self, bytes: &[u8]) -> Result<Module> {
        let module = Module::new(&self.engine, bytes)?;
        Ok(module)
    }

    /// Creates a new store with fresh host state.
    pub fn create_store(&self) -> Store<HostState> {
        Store::new(&self.engine, HostState::default())
    }

    /// Instantiates a module in the given store.
    pub async fn instantiate(
        &self,
        store: &mut Store<HostState>,
        module: &Module,
    ) -> Result<wasmtime::Instance> {
        let instance = self.linker.instantiate_async(store, module).await?;
        Ok(instance)
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
