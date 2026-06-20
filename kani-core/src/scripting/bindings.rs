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
        .register_fn("cache_delete", ctx_cache_delete);

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
