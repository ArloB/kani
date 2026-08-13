use rhai::{Dynamic, Engine};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ScriptableRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub queries: Vec<(String, String)>,
    pub body: Option<String>,
    pub endpoint_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScriptableResponse {
    pub status: i64,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ScriptableCtx {
    pub cache_backend: Arc<dyn crate::cache::CacheBackend>,
    pub cache_namespace: String,
    pub prefs: HashMap<String, String>,
    pub v8_process: Option<crate::v8_process::V8ProcessHandle>,
    pub http: Option<crate::http::SmartClient>,
    pub browser_scripts: Option<Arc<crate::scripting::BrowserScriptRegistry>>,
    pub browser_profile_key: Option<String>,
}

impl std::fmt::Debug for dyn crate::cache::CacheBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CacheBackend").finish()
    }
}

#[derive(Debug, Clone)]
pub struct HookAction {
    pub kind: HookActionKind,
}

#[derive(Debug, Clone)]
pub enum HookActionKind {
    Proceed,
    Retry,
    RetryAfter { seconds: i64 },
    RefreshAuth { endpoint_id: String },
    Fail { kind: String, reason: String },
}

fn req_get_method(req: &mut ScriptableRequest) -> String {
    req.method.clone()
}

fn req_get_url(req: &mut ScriptableRequest) -> String {
    req.url.clone()
}

fn req_set_url(req: &mut ScriptableRequest, url: String) {
    req.url = url;
}

fn req_get_endpoint_id(req: &mut ScriptableRequest) -> String {
    req.endpoint_id.clone().unwrap_or_default()
}

fn req_get_headers(req: &mut ScriptableRequest) -> rhai::Map {
    req.headers
        .iter()
        .map(|(k, v)| (k.clone().into(), Dynamic::from(v.clone())))
        .collect()
}

fn req_set_header(req: &mut ScriptableRequest, key: String, value: String) {
    match req.headers.iter_mut().find(|(k, _)| k == &key) {
        Some(entry) => entry.1 = value,
        None => req.headers.push((key, value)),
    }
}

fn req_remove_header(req: &mut ScriptableRequest, key: String) {
    req.headers.retain(|(k, _)| k != &key);
}

fn resp_get_status(resp: &mut ScriptableResponse) -> i64 {
    resp.status
}

fn resp_get_headers(resp: &mut ScriptableResponse) -> rhai::Map {
    resp.headers
        .iter()
        .map(|(k, v)| (k.clone().into(), Dynamic::from(v.clone())))
        .collect()
}

fn ctx_pref(ctx: &mut ScriptableCtx, key: String) -> Dynamic {
    ctx.prefs
        .get(&key)
        .map(|v| Dynamic::from(v.clone()))
        .unwrap_or(Dynamic::from(()))
}

fn ctx_cache_get(ctx: &mut ScriptableCtx, namespace: String, key: String) -> Dynamic {
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(ctx.cache_backend.get(&namespace, &key))
    });
    match result {
        Some(bytes) => Dynamic::from(String::from_utf8_lossy(&bytes).to_string()),
        None => Dynamic::from(()),
    }
}

fn ctx_cache_put(
    ctx: &mut ScriptableCtx,
    namespace: String,
    key: String,
    value: String,
    ttl_secs: i64,
) {
    let dur = Duration::from_secs(ttl_secs.max(0) as u64);
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(ctx.cache_backend.put(
            &namespace,
            &key,
            value.into_bytes(),
            dur,
        ))
    });
}

fn ctx_cache_delete(ctx: &mut ScriptableCtx, namespace: String, key: String) {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(ctx.cache_backend.delete(&namespace, &key))
    });
}

fn ctx_capture_page_payload(
    ctx: &mut ScriptableCtx,
    page_url: String,
    script_name: String,
    timeout_ms: i64,
) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
    let handle = ctx.v8_process.as_ref().ok_or_else(|| {
        Box::<rhai::EvalAltResult>::from("browser runtime unavailable in this context")
    })?;
    let init_script = ctx
        .browser_scripts
        .as_ref()
        .and_then(|reg| reg.get(&script_name))
        .ok_or_else(|| {
            Box::<rhai::EvalAltResult>::from(format!("browser script '{script_name}' not declared"))
        })?;
    let http = ctx
        .http
        .as_ref()
        .ok_or_else(|| Box::<rhai::EvalAltResult>::from("solver unavailable in this context"))?;
    let profile_key = ctx.browser_profile_key.clone();
    let timeout = timeout_ms.max(0) as u32;
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(
            crate::v8_process::capture_page_payload_resilient(
                handle,
                http,
                &page_url,
                init_script,
                timeout,
                profile_key.as_deref(),
                true,
            ),
        )
    });
    result
        .map(Dynamic::from)
        .map_err(|error| Box::<rhai::EvalAltResult>::from(error.to_string()))
}

pub fn register_hook_bindings(engine: &mut Engine) {
    engine
        .register_type_with_name::<ScriptableRequest>("Request")
        .register_get("method", req_get_method)
        .register_get("url", req_get_url)
        .register_set("url", req_set_url)
        .register_get("endpoint_id", req_get_endpoint_id)
        .register_get("headers", req_get_headers)
        .register_fn("set_header", req_set_header)
        .register_fn("remove_header", req_remove_header);

    engine
        .register_type_with_name::<ScriptableResponse>("Response")
        .register_get("status", resp_get_status)
        .register_get("headers", resp_get_headers);

    engine
        .register_type_with_name::<ScriptableCtx>("Ctx")
        .register_fn("pref", ctx_pref)
        .register_fn("cache_get", ctx_cache_get)
        .register_fn("cache_put", ctx_cache_put)
        .register_fn("cache_delete", ctx_cache_delete)
        .register_fn("capture_page_payload", ctx_capture_page_payload);

    engine.register_type_with_name::<HookAction>("HookAction");

    engine.register_fn("proceed", || HookAction {
        kind: HookActionKind::Proceed,
    });
    engine.register_fn("retry", || HookAction {
        kind: HookActionKind::Retry,
    });
    engine.register_fn("retry_after", |seconds: i64| HookAction {
        kind: HookActionKind::RetryAfter { seconds },
    });
    engine.register_fn("refresh_auth", |endpoint_id: String| HookAction {
        kind: HookActionKind::RefreshAuth { endpoint_id },
    });
    engine.register_fn("fail", |kind: String, reason: String| HookAction {
        kind: HookActionKind::Fail { kind, reason },
    });
}

pub fn make_hook_sandbox() -> Engine {
    let mut engine = crate::scripting::engine::make_pure_sandbox();
    register_hook_bindings(&mut engine);
    engine
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn ctx(
        v8_process: Option<crate::v8_process::V8ProcessHandle>,
        scripts: &[(&str, &str)],
    ) -> ScriptableCtx {
        let mut map = std::collections::BTreeMap::new();
        for (k, v) in scripts {
            map.insert((*k).to_string(), (*v).to_string());
        }
        ScriptableCtx {
            cache_backend: Arc::new(crate::cache::InMemoryCache::new()),
            cache_namespace: "test".to_string(),
            prefs: HashMap::new(),
            v8_process,
            http: None,
            browser_scripts: Some(Arc::new(crate::scripting::BrowserScriptRegistry::from_map(
                &map,
            ))),
            browser_profile_key: Some("test-source".to_string()),
        }
    }

    #[test]
    fn capture_page_payload_errors_without_handle() {
        let mut c = ctx(None, &[("fetch", "passPayload('{}')")]);
        let err =
            ctx_capture_page_payload(&mut c, "https://example.com".into(), "fetch".into(), 1000)
                .unwrap_err();
        assert!(
            err.to_string().contains("unavailable"),
            "expected browser-unavailable error, got: {err}"
        );
    }

    #[test]
    fn capture_page_payload_errors_on_undeclared_script() {
        let mut c = ctx(Some(crate::v8_process::new_handle()), &[]);
        let err =
            ctx_capture_page_payload(&mut c, "https://example.com".into(), "missing".into(), 1000)
                .unwrap_err();
        assert!(
            err.to_string().contains("missing") && err.to_string().contains("not declared"),
            "expected not-declared error naming the script, got: {err}"
        );
    }
}
