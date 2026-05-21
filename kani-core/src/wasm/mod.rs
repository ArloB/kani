//! WASM runtime and host ABI for extensions.

pub mod abi;

#[cfg(test)]
mod tests;

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
pub use bindings::KaniExtensionPre;
pub use bindings::exports;
pub use bindings::kani;

pub struct ResponseData {
    pub body: String,
    pub status: u16,
}

pub struct SafeHtml(pub scraper::Html);
// SAFETY: SafeHtml is only ever accessed behind Arc<Mutex<_>> (via SendHtml),
// which serialises all access. scraper::Html is !Send due to internal use of
// ego_tree NodeId (raw pointers), but these are never shared across threads.
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

#[derive(Debug, Clone)]
pub enum AllowedHost {
    Restricted(String),
    Unrestricted,
    MetadataOnly,
}

use crate::http::SmartClient;

pub const MAX_HANDLES: usize = 10_000;

pub struct HostState {
    pub http_client: SmartClient,
    pub allowed_host: AllowedHost,
    pub next_doc_handle: i32,
    pub html_docs: HashMap<i32, StoredNode>,
    pub html_lists: HashMap<i32, Vec<StoredNode>>,
    pub json_docs: HashMap<i32, serde_json::Value>,
    pub selector_cache: std::cell::RefCell<HashMap<String, scraper::Selector>>,
    pub last_error: Option<i32>,
    pub preferences: std::collections::HashMap<String, String>,
    pub call_started_at: std::time::Instant,
    pub io_count: u32,
    pub last_io_at: Option<std::time::Instant>,
    /// Handle to the shared Node.js V8 subprocess. Lazy-spawned on first use.
    pub v8_process: crate::v8_process::V8ProcessHandle,
}

impl StoredNode {
    /// Locks the document, wraps the node as an `ElementRef`, and calls `f`.
    /// Returns an error if the node is not an element or the lock is poisoned.
    pub fn with_element<T, F>(&self, f: F) -> std::result::Result<T, String>
    where
        F: FnOnce(scraper::ElementRef) -> std::result::Result<T, String>,
    {
        let guard = self.doc.lock().map_err(|_| "HTML document lock poisoned")?;
        match scraper::ElementRef::wrap(guard.0.tree.get(self.node_id).unwrap()) {
            Some(el) => f(el),
            None => Err("node is not an element".into()),
        }
    }

    /// Like [`with_element`] but returns `Ok(None)` when the node is not an element,
    /// so callers that treat that as a non-error can propagate it cleanly.
    pub fn try_with_element<T, F>(&self, f: F) -> std::result::Result<Option<T>, String>
    where
        F: FnOnce(scraper::ElementRef) -> std::result::Result<Option<T>, String>,
    {
        let guard = self.doc.lock().map_err(|_| "HTML document lock poisoned")?;
        Ok(match scraper::ElementRef::wrap(guard.0.tree.get(self.node_id).unwrap()) {
            Some(el) => f(el)?,
            None => None,
        })
    }
}

impl HostState {
    pub fn new(
        http_client: SmartClient,
        allowed_host: AllowedHost,
        v8_process: crate::v8_process::V8ProcessHandle,
    ) -> Result<Self> {
        let allowed_host = match allowed_host {
            AllowedHost::Restricted(raw) => {
                let host = raw
                    .parse::<rquest::Url>()
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.to_string()))
                    .ok_or_else(|| {
                        crate::error::Error::Internal(format!(
                            "Cannot derive hostname from base_url '{}' for HTTP restriction",
                            raw
                        ))
                    })?;
                AllowedHost::Restricted(host)
            }
            other => other,
        };

        Ok(Self {
            http_client,
            allowed_host,
            next_doc_handle: 1,
            html_docs: HashMap::new(),
            html_lists: HashMap::new(),
            json_docs: HashMap::new(),
            selector_cache: std::cell::RefCell::new(HashMap::new()),
            last_error: None,
            preferences: HashMap::new(),
            call_started_at: std::time::Instant::now(),
            io_count: 0,
            last_io_at: None,
            v8_process,
        })
    }

    pub fn clear_all(&mut self) {
        self.html_docs.clear();
        self.html_lists.clear();
        self.json_docs.clear();
        self.next_doc_handle = 1;
    }

    /// Returns an error string if the total live handle count is at or above
    /// [`MAX_HANDLES`].
    pub fn check_handle_capacity(&self) -> std::result::Result<(), String> {
        let total = self.html_docs.len() + self.html_lists.len() + self.json_docs.len();
        if total >= MAX_HANDLES {
            Err(format!(
                "handle limit reached ({MAX_HANDLES}): extension is leaking document handles"
            ))
        } else {
            Ok(())
        }
    }

    /// Enforces the `AllowedHost` policy for a given request host string.
    pub fn check_allowed_host(&self, host: &str) -> std::result::Result<(), String> {
        match &self.allowed_host {
            AllowedHost::Restricted(allowed) => {
                if host != allowed {
                    Err(format!(
                        "Request blocked: extension may only contact '{}', got '{}'",
                        allowed, host
                    ))
                } else {
                    Ok(())
                }
            }
            AllowedHost::Unrestricted => Ok(()),
            AllowedHost::MetadataOnly => Err("HTTP requests are not permitted on metadata-only instances.".into()),
        }
    }

    /// Returns a reference to the JSON document for `handle`, or an error string.
    pub fn get_json(&self, handle: i32) -> std::result::Result<&serde_json::Value, String> {
        self.json_docs.get(&handle).ok_or_else(|| "Invalid JSON handle".to_string())
    }

    /// Returns a reference to the HTML document node for `handle`, or an error string.
    pub fn get_html_doc(&self, handle: i32) -> std::result::Result<&StoredNode, String> {
        self.html_docs.get(&handle).ok_or_else(|| "Document not found".to_string())
    }

    /// Returns a reference to the compiled selector, parsing and caching it on
    /// the first call for a given `selector` string.
    pub fn get_or_parse_selector(&mut self, selector: &str) -> Result<&scraper::Selector> {
        let cache = self.selector_cache.get_mut();
        if !cache.contains_key(selector) {
            let parsed = scraper::Selector::parse(selector)
                .map_err(|e| crate::error::Error::Internal(format!("Invalid selector: {:?}", e)))?;
            cache.insert(selector.to_string(), parsed);
        }
        Ok(cache.get(selector).unwrap())
    }
}

impl Default for HostState {
    fn default() -> Self {
        HostState::new(
            SmartClient::new(None).unwrap(),
            AllowedHost::MetadataOnly,
            Arc::new(Mutex::new(None)),
        )
        .unwrap()
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

    /// Pre-links a component's imports so instances can be created cheaply.
    pub fn instantiate_pre(&self, component: &Component) -> Result<KaniExtensionPre<HostState>> {
        let instance_pre = self.linker.instantiate_pre(component)?;
        let pre = KaniExtensionPre::new(instance_pre)?;
        Ok(pre)
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

pub mod filter_conversions {
    use crate::wasm::kani::extension::types as wit;
    use kani_shared::types::{ActiveFilter, FilterState};

    impl From<wit::FilterState> for FilterState {
        fn from(s: wit::FilterState) -> Self {
            match s {
                wit::FilterState::Checkbox(c) => FilterState::Checkbox(c),
                wit::FilterState::TextInput(t) => FilterState::TextInput(t),
                wit::FilterState::Selection(opt) => {
                    FilterState::Selection { name: opt.name, value: opt.value }
                }
                wit::FilterState::Multiselect(values) => FilterState::Multiselect(values),
            }
        }
    }

    impl From<FilterState> for wit::FilterState {
        fn from(s: FilterState) -> Self {
            match s {
                FilterState::Checkbox(c) => Self::Checkbox(c),
                FilterState::TextInput(t) => Self::TextInput(t),
                FilterState::Selection { name, value } => {
                    Self::Selection(wit::OptionState { name, value })
                }
                FilterState::Multiselect(values) => Self::Multiselect(values),
            }
        }
    }

    pub fn to_wit_active_filters(filters: &[ActiveFilter]) -> Vec<wit::ActiveFilter> {
        filters.iter().map(|f| wit::ActiveFilter {
            filter_name: f.filter_name.clone(),
            state: f.state.clone().into(),
        }).collect()
    }
}
