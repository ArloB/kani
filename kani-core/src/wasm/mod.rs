//! WASM runtime and host ABI for extensions.

pub mod abi;
pub mod cache;

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
            "kani:extension/extraction": async,
            "kani:extension/cache": async,
            "kani:extension/scripting": async,
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

impl AllowedHost {
    /// Enforces this policy for a resolved request host. Shared by the HTTP path
    /// (`HostState::check_allowed_host`) and the browser `page_url` check so both
    /// apply identical matching rules.
    pub fn allows_host(&self, host: &str) -> std::result::Result<(), String> {
        match self {
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
            AllowedHost::MetadataOnly => {
                Err("HTTP requests are not permitted on metadata-only instances.".into())
            }
        }
    }
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
    pub selector_cache: std::sync::Mutex<HashMap<String, std::sync::Arc<scraper::Selector>>>,
    pub last_error: Option<i32>,
    pub preferences: std::collections::HashMap<String, String>,
    pub call_started_at: std::time::Instant,
    pub io_count: u32,
    pub last_io_at: Option<std::time::Instant>,
    /// Cache backend shared across all calls for this source. The namespace prefix
    /// (`{extension_id}:{version}:{scope}:`) is resolved at construction time.
    pub ext_cache: std::sync::Arc<dyn crate::cache::CacheBackend>,
    pub ext_cache_namespace: String,
    /// Stable per-source key (the extension id) used to derive a dedicated
    /// Chromium `userDataDir`, so browser session state never leaks between
    /// sources. Derived from `ext_cache_namespace` at construction.
    pub browser_profile_key: String,
    /// Handle to the shared Node.js V8 subprocess. Lazy-spawned on first use.
    pub v8_process: crate::v8_process::V8ProcessHandle,
    /// Named `passPayload` init scripts, for the Rhai `capture_page_payload`
    /// hook binding. `None` when the source declares no browser scripts.
    pub browser_scripts: Option<std::sync::Arc<crate::scripting::BrowserScriptRegistry>>,
    /// Operator gate for this source's browser capability. When `false`, browser
    /// capture calls are rejected before any V8 dispatch. Defaults to `true`.
    pub browser_enabled: bool,
    /// Compiled Rhai pure-function registry for `.user.<name>()` DSL calls.
    /// `None` when the source has no `scripts.pure` block.
    pub pure_fn_registry: Option<std::sync::Arc<crate::scripting::PureFunctionRegistry>>,
    /// Compiled Rhai hook registry for `pre_request` / `on_status` hooks.
    /// `None` when the source has no hooks defined.
    pub hook_registry: Option<std::sync::Arc<crate::scripting::HookRegistry>>,
    pub max_hook_requests: u32,
    /// Per-eval resource budget (iterations, depth). Reset at the start of each blueprint eval.
    pub eval_budget: std::sync::Arc<crate::evaluator::shared::EvalBudget>,
}

impl StoredNode {
    /// Locks the document, wraps the node as an `ElementRef`, and calls `f`.
    /// Returns an error if the node is not an element or the lock is poisoned.
    pub fn with_element<T, F>(&self, f: F) -> std::result::Result<T, String>
    where
        F: FnOnce(scraper::ElementRef) -> std::result::Result<T, String>,
    {
        let guard = self.doc.lock().map_err(|_| "HTML document lock poisoned")?;
        let tree_node = guard
            .0
            .tree
            .get(self.node_id)
            .ok_or("HTML node no longer present in document tree")?;
        match scraper::ElementRef::wrap(tree_node) {
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
        let tree_node = guard
            .0
            .tree
            .get(self.node_id)
            .ok_or("HTML node no longer present in document tree")?;
        Ok(match scraper::ElementRef::wrap(tree_node) {
            Some(el) => f(el)?,
            None => None,
        })
    }
}

impl HostState {
    pub fn new(
        http_client: SmartClient,
        allowed_host: AllowedHost,
        ext_cache: std::sync::Arc<dyn crate::cache::CacheBackend>,
        ext_cache_namespace: String,
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

        let browser_profile_key = ext_cache_namespace
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string();

        Ok(Self {
            http_client,
            allowed_host,
            next_doc_handle: 1,
            html_docs: HashMap::new(),
            html_lists: HashMap::new(),
            json_docs: HashMap::new(),
            selector_cache: std::sync::Mutex::new(HashMap::new()),
            last_error: None,
            preferences: HashMap::new(),
            call_started_at: std::time::Instant::now(),
            io_count: 0,
            last_io_at: None,
            ext_cache,
            ext_cache_namespace,
            browser_profile_key,
            v8_process,
            browser_scripts: None,
            browser_enabled: true,
            pure_fn_registry: None,
            hook_registry: None,
            max_hook_requests: 3,
            eval_budget: std::sync::Arc::new(crate::evaluator::shared::EvalBudget::new()),
        })
    }

    pub fn clear_all(&mut self) {
        self.html_docs.clear();
        self.html_lists.clear();
        self.json_docs.clear();
        self.next_doc_handle = 1;
    }

    pub fn charge_io(&mut self) -> std::result::Result<(), String> {
        self.io_count += 1;
        if self.io_count > 32 {
            return Err("Extension exceeded maximum HTTP request count".into());
        }
        if self.call_started_at.elapsed().as_secs() > 120 {
            return Err("Extension exceeded maximum wall time".into());
        }
        Ok(())
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
        self.allowed_host.allows_host(host)
    }

    /// Returns a reference to the JSON document for `handle`, or an error string.
    pub fn get_json(&self, handle: i32) -> std::result::Result<&serde_json::Value, String> {
        self.json_docs
            .get(&handle)
            .ok_or_else(|| "Invalid JSON handle".to_string())
    }

    /// Returns a reference to the HTML document node for `handle`, or an error string.
    pub fn get_html_doc(&self, handle: i32) -> std::result::Result<&StoredNode, String> {
        self.html_docs
            .get(&handle)
            .ok_or_else(|| "Document not found".to_string())
    }

    /// Returns a reference to the compiled selector, parsing and caching it on
    /// the first call for a given `selector` string.
    pub fn get_or_parse_selector(&mut self, selector: &str) -> Result<&scraper::Selector> {
        let cache = self
            .selector_cache
            .get_mut()
            .map_err(|_| crate::error::Error::Internal("selector cache lock poisoned".into()))?;
        if !cache.contains_key(selector) {
            let parsed = scraper::Selector::parse(selector)
                .map_err(|e| crate::error::Error::Internal(format!("Invalid selector: {:?}", e)))?;
            cache.insert(selector.to_string(), std::sync::Arc::new(parsed));
        }
        Ok(&**cache.get(selector).expect("selector was just inserted"))
    }
}

impl Default for HostState {
    fn default() -> Self {
        HostState::new(
            SmartClient::new(None).expect("failed to build default SmartClient"),
            AllowedHost::MetadataOnly,
            Arc::new(crate::cache::InMemoryCache::new()),
            String::new(),
            crate::v8_process::new_handle(),
        )
        .expect("failed to construct default HostState")
    }
}

/// WASM runtime wrapper with async support enabled.
pub struct WasmRuntime {
    engine: Engine,
    linker: Linker<HostState>,
    module_cache: Option<std::sync::Mutex<cache::WasmModuleCache>>,
}

impl WasmRuntime {
    pub fn new(max_instances: u32) -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.concurrency_support(true);
        config.epoch_interruption(true);

        let mut pool = wasmtime::PoolingAllocationConfig::default();
        pool.total_component_instances(max_instances);

        config.allocation_strategy(wasmtime::InstanceAllocationStrategy::Pooling(pool));

        Self::new_with_config(config)
    }

    pub fn new_on_demand() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.concurrency_support(true);
        config.epoch_interruption(true);

        Self::new_with_config(config)
    }

    fn new_with_config(config: Config) -> Result<Self> {
        let engine = Engine::new(&config)?;
        let mut linker = Linker::new(&engine);

        KaniExtension::add_to_linker::<_, HasSelf<HostState>>(&mut linker, |state| state)?;

        let module_cache = cache::WasmModuleCache::from_env().map(std::sync::Mutex::new);

        Ok(Self {
            engine,
            linker,
            module_cache,
        })
    }

    /// Returns a reference to the engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compiles a WASM component from bytes, consulting the on-disk module cache first.
    pub fn compile_component(&self, bytes: &[u8]) -> Result<Component> {
        let sha256 = cache::WasmModuleCache::sha256_hex(bytes);

        if let Some(mc) = &self.module_cache {
            let cached = mc
                .lock()
                .map_err(|_| {
                    crate::error::Error::Internal("wasm module cache lock poisoned".into())
                })?
                .try_get(&self.engine, &sha256)?;
            if let Some(c) = cached {
                return Ok(c);
            }
        }

        let component = Component::new(&self.engine, bytes)?;

        if let Some(mc) = &self.module_cache
            && let Ok(mut guard) = mc.lock()
        {
            guard.insert(&sha256, component.clone());
        }

        Ok(component)
    }

    pub fn prune_module_cache(&self, live_hashes: &std::collections::HashSet<String>) {
        if let Some(mc) = &self.module_cache
            && let Ok(guard) = mc.lock()
        {
            guard.prune(live_hashes);
        }
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
                wit::FilterState::Selection(opt) => FilterState::Selection {
                    name: opt.name,
                    value: opt.value,
                },
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
        filters
            .iter()
            .map(|f| wit::ActiveFilter {
                filter_name: f.filter_name.clone(),
                state: f.state.clone().into(),
            })
            .collect()
    }
}

pub fn ext_error_from_wit(
    e: kani::extension::types::ExtensionError,
) -> kani_shared::extension::ExtensionError {
    use kani::extension::types::ExtensionErrorKind as WitKind;
    use kani_shared::extension::ExtensionErrorKind;
    let kind = match e.kind {
        WitKind::Network => ExtensionErrorKind::Network,
        WitKind::Parse => ExtensionErrorKind::Parse,
        WitKind::NotFound => ExtensionErrorKind::NotFound,
        WitKind::RateLimited => ExtensionErrorKind::RateLimited,
        WitKind::Auth => ExtensionErrorKind::Auth,
        WitKind::ContentUnavailable => ExtensionErrorKind::ContentUnavailable,
        WitKind::Timeout => ExtensionErrorKind::Timeout,
        WitKind::InvalidInput => ExtensionErrorKind::InvalidInput,
        WitKind::Internal => ExtensionErrorKind::Internal,
        WitKind::Unknown => ExtensionErrorKind::Unknown,
        WitKind::SourceUpdating => ExtensionErrorKind::Updating,
    };
    kani_shared::extension::ExtensionError {
        kind,
        message: e.message,
        source_url: e.source_url.map(|u| redact_url(&u)),
        retry_after_secs: e.retry_after_secs,
    }
}

fn redact_url(url: &str) -> String {
    let url = strip_userinfo(url);
    strip_sensitive_params(&url)
}

fn strip_userinfo(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let after_scheme = &url[scheme_end + 3..];
        let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        let authority = &after_scheme[..host_end];
        if let Some(at_pos) = authority.rfind('@') {
            let host = &authority[at_pos + 1..];
            let rest = &after_scheme[host_end..];
            return format!("{}{}{}", &url[..scheme_end + 3], host, rest);
        }
    }
    url.to_string()
}

fn strip_sensitive_params(url: &str) -> String {
    const SENSITIVE: &[&str] = &["token", "api_key", "session", "password", "signature"];
    let (base, fragment) = match url.find('#') {
        Some(p) => (&url[..p], Some(&url[p..])),
        None => (url, None),
    };
    let Some(q_pos) = base.find('?') else {
        return url.to_string();
    };
    let path = &base[..q_pos];
    let query = &base[q_pos + 1..];
    let filtered: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("");
            !SENSITIVE.iter().any(|s| key.eq_ignore_ascii_case(s))
        })
        .collect();
    let result = if filtered.is_empty() {
        path.to_string()
    } else {
        format!("{}?{}", path, filtered.join("&"))
    };
    match fragment {
        Some(f) => format!("{}{}", result, f),
        None => result,
    }
}
