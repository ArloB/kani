use std::collections::HashMap;

use rhai::{AST, Dynamic, Scope};

use super::bindings::{
    HookAction, HookActionKind, ScriptableCtx, ScriptableRequest, ScriptableResponse,
    make_hook_sandbox,
};

#[derive(Default)]
pub struct HookScripts {
    pub pre_request: Option<String>,
    pub on_status: std::collections::BTreeMap<String, String>,
    pub endpoint_pre_request: std::collections::BTreeMap<String, String>,
    pub endpoint_on_status:
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

impl HookScripts {
    pub fn is_empty(&self) -> bool {
        self.pre_request.is_none()
            && self.on_status.is_empty()
            && self.endpoint_pre_request.is_empty()
            && self.endpoint_on_status.is_empty()
    }
}

#[derive(Debug)]
pub struct HookRegistry {
    engine: rhai::Engine,
    global_pre_request: Option<AST>,
    global_on_status: HashMap<String, AST>,
    endpoint_pre_request: HashMap<String, AST>,
    endpoint_on_status: HashMap<String, HashMap<String, AST>>,
}

impl HookRegistry {
    pub fn is_empty(&self) -> bool {
        self.global_pre_request.is_none()
            && self.global_on_status.is_empty()
            && self.endpoint_pre_request.is_empty()
            && self.endpoint_on_status.is_empty()
    }

    pub fn compile(scripts: &HookScripts) -> Result<Self, String> {
        let engine = make_hook_sandbox();

        let global_pre_request = scripts
            .pre_request
            .as_deref()
            .map(|src| {
                engine
                    .compile(src)
                    .map_err(|e| format!("pre_request compile error: {e}"))
            })
            .transpose()?;

        let mut global_on_status = HashMap::new();
        for (key, src) in &scripts.on_status {
            let ast = engine
                .compile(src)
                .map_err(|e| format!("on_status[{key}] compile error: {e}"))?;
            global_on_status.insert(key.clone(), ast);
        }

        let mut endpoint_pre_request = HashMap::new();
        for (endpoint_id, src) in &scripts.endpoint_pre_request {
            let ast = engine
                .compile(src)
                .map_err(|e| format!("endpoint '{endpoint_id}' pre_request compile error: {e}"))?;
            endpoint_pre_request.insert(endpoint_id.clone(), ast);
        }

        let mut endpoint_on_status: HashMap<String, HashMap<String, AST>> = HashMap::new();
        for (endpoint_id, status_map) in &scripts.endpoint_on_status {
            let mut map = HashMap::new();
            for (key, src) in status_map {
                let ast = engine.compile(src).map_err(|e| {
                    format!("endpoint '{endpoint_id}' on_status[{key}] compile error: {e}")
                })?;
                map.insert(key.clone(), ast);
            }
            endpoint_on_status.insert(endpoint_id.clone(), map);
        }

        Ok(Self {
            engine,
            global_pre_request,
            global_on_status,
            endpoint_pre_request,
            endpoint_on_status,
        })
    }

    pub fn run_pre_request(
        &self,
        req: &mut ScriptableRequest,
        ctx: ScriptableCtx,
    ) -> Result<HookAction, String> {
        let endpoint_id = req.endpoint_id.as_deref().unwrap_or("");
        let ast = self
            .endpoint_pre_request
            .get(endpoint_id)
            .or(self.global_pre_request.as_ref());

        let Some(ast) = ast else {
            return Ok(HookAction {
                kind: HookActionKind::Proceed,
            });
        };

        let mut scope = Scope::new();
        scope.push_dynamic("req", Dynamic::from(req.clone()));
        scope.push_dynamic("ctx", Dynamic::from(ctx));

        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, ast)
            .map_err(|e| format!("pre_request hook error: {e}"))?;

        if let Some(mutated) = scope.get_value::<ScriptableRequest>("req") {
            *req = mutated;
        }

        Ok(result.try_cast::<HookAction>().unwrap_or(HookAction {
            kind: HookActionKind::Proceed,
        }))
    }

    pub fn run_on_status(
        &self,
        req: &ScriptableRequest,
        resp: &ScriptableResponse,
        ctx: ScriptableCtx,
    ) -> Result<HookAction, String> {
        let endpoint_id = req.endpoint_id.as_deref().unwrap_or("");
        let status = resp.status as u16;

        let ast = self
            .find_on_status_ast(endpoint_id, status)
            .or_else(|| self.find_on_status_ast("", status));

        let Some(ast) = ast else {
            return Ok(HookAction {
                kind: HookActionKind::Proceed,
            });
        };

        let mut scope = Scope::new();
        scope.push_dynamic("req", Dynamic::from(req.clone()));
        scope.push_dynamic("resp", Dynamic::from(resp.clone()));
        scope.push_dynamic("ctx", Dynamic::from(ctx));

        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut scope, ast)
            .map_err(|e| format!("on_status hook error: {e}"))?;

        Ok(result.try_cast::<HookAction>().unwrap_or(HookAction {
            kind: HookActionKind::Proceed,
        }))
    }

    fn find_on_status_ast(&self, endpoint_id: &str, status: u16) -> Option<&AST> {
        let map = if endpoint_id.is_empty() {
            &self.global_on_status
        } else {
            self.endpoint_on_status.get(endpoint_id)?
        };
        let exact = status.to_string();
        let class = format!("{}xx", status / 100);
        map.get(&exact)
            .or_else(|| map.get(&class))
            .or_else(|| map.get("default"))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::sync::Arc;

    fn dummy_ctx() -> ScriptableCtx {
        ScriptableCtx {
            cache_backend: Arc::new(crate::cache::InMemoryCache::new()),
            cache_namespace: "test".to_string(),
            prefs: HashMap::new(),
        }
    }

    fn dummy_req(endpoint_id: Option<&str>) -> ScriptableRequest {
        ScriptableRequest {
            method: "GET".to_string(),
            url: "https://example.com/".to_string(),
            headers: Vec::new(),
            queries: Vec::new(),
            body: None,
            endpoint_id: endpoint_id.map(str::to_string),
        }
    }

    fn dummy_resp(status: i64) -> ScriptableResponse {
        ScriptableResponse {
            status,
            headers: Vec::new(),
        }
    }

    #[test]
    fn compile_valid_pre_request() {
        let scripts = HookScripts {
            pre_request: Some(r#"req.set_header("X-Test", "value"); proceed()"#.to_string()),
            ..Default::default()
        };
        assert!(HookRegistry::compile(&scripts).is_ok());
    }

    #[test]
    fn compile_syntax_error_rejected() {
        let scripts = HookScripts {
            pre_request: Some("req.set_header( // missing paren".to_string()),
            ..Default::default()
        };
        let err = HookRegistry::compile(&scripts).unwrap_err();
        assert!(
            err.contains("pre_request"),
            "error must mention pre_request: {err}"
        );
    }

    #[test]
    fn hook_action_constructors_in_rhai() {
        use super::super::bindings::make_hook_sandbox;
        let engine = make_hook_sandbox();
        let action: HookAction = engine.eval("proceed()").unwrap();
        assert!(matches!(action.kind, HookActionKind::Proceed));
        let action: HookAction = engine.eval("retry()").unwrap();
        assert!(matches!(action.kind, HookActionKind::Retry));
        let action: HookAction = engine.eval("retry_after(30)").unwrap();
        assert!(matches!(
            action.kind,
            HookActionKind::RetryAfter { seconds: 30 }
        ));
        let action: HookAction = engine
            .eval(r#"fail("rate_limited", "too many requests")"#)
            .unwrap();
        assert!(matches!(action.kind, HookActionKind::Fail { .. }));
    }

    #[tokio::test]
    async fn pre_request_mutates_request() {
        let scripts = HookScripts {
            pre_request: Some(r#"req.set_header("X-Signed", "yes"); proceed()"#.to_string()),
            ..Default::default()
        };
        let registry = HookRegistry::compile(&scripts).unwrap();
        let mut req = dummy_req(None);
        let (action, req) = tokio::task::spawn_blocking(move || {
            let action = registry.run_pre_request(&mut req, dummy_ctx());
            (action, req)
        })
        .await
        .unwrap();
        let action = action.unwrap();
        assert!(matches!(action.kind, HookActionKind::Proceed));
        assert!(
            req.headers
                .iter()
                .any(|(k, v)| k == "X-Signed" && v == "yes"),
            "mutation must propagate back: {:?}",
            req.headers
        );
    }

    #[test]
    fn on_status_exact_match() {
        let mut on_status = std::collections::BTreeMap::new();
        on_status.insert("401".to_string(), "retry()".to_string());
        let scripts = HookScripts {
            on_status,
            ..Default::default()
        };
        let registry = HookRegistry::compile(&scripts).unwrap();
        let req = dummy_req(None);
        let resp = dummy_resp(401);
        let action = registry.run_on_status(&req, &resp, dummy_ctx()).unwrap();
        assert!(matches!(action.kind, HookActionKind::Retry));
    }

    #[test]
    fn on_status_class_match() {
        let mut on_status = std::collections::BTreeMap::new();
        on_status.insert("5xx".to_string(), "retry_after(10)".to_string());
        let scripts = HookScripts {
            on_status,
            ..Default::default()
        };
        let registry = HookRegistry::compile(&scripts).unwrap();
        let req = dummy_req(None);
        let resp = dummy_resp(503);
        let action = registry.run_on_status(&req, &resp, dummy_ctx()).unwrap();
        assert!(matches!(
            action.kind,
            HookActionKind::RetryAfter { seconds: 10 }
        ));
    }

    #[test]
    fn on_status_default_fallback() {
        let mut on_status = std::collections::BTreeMap::new();
        on_status.insert(
            "default".to_string(),
            r#"fail("unexpected", "status")"#.to_string(),
        );
        let scripts = HookScripts {
            on_status,
            ..Default::default()
        };
        let registry = HookRegistry::compile(&scripts).unwrap();
        let req = dummy_req(None);
        let resp = dummy_resp(429);
        let action = registry.run_on_status(&req, &resp, dummy_ctx()).unwrap();
        assert!(matches!(action.kind, HookActionKind::Fail { .. }));
    }

    #[test]
    fn no_matching_hook_proceeds() {
        let registry = HookRegistry::compile(&HookScripts::default()).unwrap();
        let req = dummy_req(None);
        let resp = dummy_resp(401);
        let action = registry.run_on_status(&req, &resp, dummy_ctx()).unwrap();
        assert!(matches!(action.kind, HookActionKind::Proceed));
    }

    #[test]
    fn endpoint_specific_hook_takes_precedence() {
        let mut ep_pre = std::collections::BTreeMap::new();
        ep_pre.insert(
            "search".to_string(),
            r#"req.set_header("X-Ep", "search"); proceed()"#.to_string(),
        );
        let scripts = HookScripts {
            pre_request: Some(r#"req.set_header("X-Ep", "global"); proceed()"#.to_string()),
            endpoint_pre_request: ep_pre,
            ..Default::default()
        };
        let registry = HookRegistry::compile(&scripts).unwrap();
        let mut req = dummy_req(Some("search"));
        tokio::task::block_in_place(|| registry.run_pre_request(&mut req, dummy_ctx())).unwrap();
        let header_val = req
            .headers
            .iter()
            .find(|(k, _)| k == "X-Ep")
            .map(|(_, v)| v.as_str());
        assert_eq!(
            header_val,
            Some("search"),
            "endpoint hook must win: {:?}",
            req.headers
        );
    }
}
