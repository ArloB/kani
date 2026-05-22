use crate::utilities::parse_date_flexible;
use crate::wasm::{SafeHtml, StoredNode};
use ego_tree::NodeId;
use kani_shared::ast::{Expr, Op};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub enum Value {
    Str(String),
    Num(f64),
    Int(i64),
    Bool(bool),
    List(Vec<Value>),
    Null,
    HtmlElement {
        doc: Arc<Mutex<SafeHtml>>,
        node_id: NodeId,
    },
    Json(serde_json::Value),
}

impl Value {
    pub fn to_json(&self) -> Option<serde_json::Value> {
        match self {
            Value::Json(json) => Some(json.clone()),
            Value::Str(s) => Some(serde_json::Value::String(s.clone())),
            Value::Bool(b) => Some(serde_json::Value::Bool(*b)),
            Value::Int(i) => Some(serde_json::Value::Number((*i).into())),
            Value::Num(f) => serde_json::Number::from_f64(*f).map(serde_json::Value::Number),
            Value::List(items) => Some(serde_json::Value::Array(
                items
                    .iter()
                    .map(|v| v.to_json().unwrap_or(serde_json::Value::Null))
                    .collect(),
            )),
            Value::Null => None,
            Value::HtmlElement { .. } => None,
        }
    }

    pub fn into_str(self, op: &str) -> Result<String, String> {
        match self {
            Value::Str(s) => Ok(s),
            _ => Err(format!("{}: expected string value", op)),
        }
    }

    /// Apply `f` to the inner string, propagating `Null` as `Null`.
    /// Returns `Err` if the value is neither `Str` nor `Null`.
    pub fn map_str<F>(self, op: &str, f: F) -> Result<Value, String>
    where
        F: FnOnce(String) -> Result<Value, String>,
    {
        match self {
            Value::Null => Ok(Value::Null),
            Value::Str(s) => f(s),
            _ => Err(format!("{}: expected string or null", op)),
        }
    }

    pub fn into_list(self, op: &str) -> Result<Vec<Value>, String> {
        match self {
            Value::List(v) => Ok(v),
            Value::Json(serde_json::Value::Array(arr)) => {
                Ok(arr.into_iter().map(Value::Json).collect())
            }
            _ => Err(format!("{}: expected a List", op)),
        }
    }

    pub fn into_json(self, op: &str) -> Result<serde_json::Value, String> {
        match self {
            Value::Json(v) => Ok(v),
            Value::Null => Ok(serde_json::Value::Null),
            _ => Err(format!("{}: expected Json value", op)),
        }
    }

    /// Returns `Ok(Some(StoredNode))` for an HTML element, `Ok(None)` for Null, or an error otherwise.
    pub fn into_html_element(self, op: &str) -> Result<Option<StoredNode>, String> {
        match self {
            Value::HtmlElement { doc, node_id } => Ok(Some(StoredNode { doc, node_id })),
            Value::Null => Ok(None),
            _ => Err(format!("{}: expected HTML element", op)),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            _ => false,
        }
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Str(s) => write!(f, "Str({:?})", s),
            Value::Num(n) => write!(f, "Num({})", n),
            Value::Int(i) => write!(f, "Int({})", i),
            Value::Bool(b) => write!(f, "Bool({})", b),
            Value::List(l) => write!(f, "List({:?})", l),
            Value::Null => write!(f, "Null"),
            Value::HtmlElement { node_id, .. } => write!(f, "HtmlElement({:?})", node_id),
            Value::Json(j) => write!(f, "Json({})", j),
        }
    }
}

/// Arc-backed environment: clone is O(1); mutation (set) triggers copy-on-write.
#[derive(Clone)]
pub struct Env(Arc<HashMap<String, Value>>);

impl Env {
    pub fn new() -> Self {
        Env(Arc::new(HashMap::new()))
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }
    pub fn set(&mut self, key: &str, value: Value) {
        Arc::make_mut(&mut self.0).insert(key.to_owned(), value);
    }
}

/// Evaluates expression arms that are common to both the HTML and JSON evaluators.
/// Returns `Some(result)` if the arm was handled, `None` if evaluator-specific.
///
/// `recurse` takes `(&'a Expr, Env)` so callers can move `env` on the final call instead of
/// always cloning. With the Arc-backed `Env`, all other clones are O(1) regardless.
pub async fn eval_common_expr<'a, F>(
    expr: &'a Expr,
    env: Env,
    recurse: &F,
) -> Option<Result<Value, String>>
where
    F: Fn(&'a Expr, Env) -> Pin<Box<dyn Future<Output = Result<Value, String>> + 'a>>,
{
    match expr {
        Expr::Literal(s) => Some(Ok(Value::Str(s.clone()))),
        Expr::Number(n) => Some(Ok(Value::Num(*n))),
        Expr::Null => Some(Ok(Value::Null)),
        Expr::Bool(bool) => Some(Ok(Value::Bool(*bool))),

        Expr::Var(name) => Some(
            env.get(name)
                .cloned()
                .ok_or_else(|| format!("Undefined variable '{}'", name)),
        ),

        Expr::BinaryOperation { op, lhs, rhs } => Some(
            (async {
                if matches!(op, Op::And | Op::Or) {
                    let l = match recurse(lhs, env.clone()).await? {
                        Value::Bool(b) => b,
                        _ => return Err("&&/||: left operand must be Bool".into()),
                    };
                    return match op {
                        Op::And if !l => Ok(Value::Bool(false)),
                        Op::Or if l => Ok(Value::Bool(true)),
                        _ => match recurse(rhs, env).await? {
                            Value::Bool(b) => Ok(Value::Bool(b)),
                            _ => Err("&&/||: right operand must be Bool".into()),
                        },
                    };
                }
                let l = recurse(lhs, env.clone()).await?;
                let r = recurse(rhs, env).await?;
                match (op, l, r) {
                    (
                        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Lt | Op::Gt | Op::Le | Op::Ge,
                        l,
                        r,
                    ) => numeric_op(op, l, r),
                    (Op::Eq, a, b) => Ok(Value::Bool(a == b)),
                    (Op::Ne, a, b) => Ok(Value::Bool(a != b)),
                    (op, l, r) => Err(format!("type error: {:?} {:?} {:?}", l, op, r)),
                }
            })
            .await,
        ),

        Expr::Split { target, delimiter } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("split", |s| {
                Ok(Value::List(
                    s.split(delimiter.as_str())
                        .map(|p| Value::Str(p.to_owned()))
                        .collect(),
                ))
            })
        })),

        Expr::At { target, index } => Some(recurse(target, env).await.and_then(|v| match v {
            Value::Null => Ok(Value::Null),
            v => v.into_list("at").map(|items| {
                let i = if *index < 0 {
                    items.len().checked_sub((-*index) as usize)
                } else {
                    Some(*index as usize)
                };
                match i.and_then(|i| items.into_iter().nth(i)) {
                    Some(v) => v,
                    None => Value::Null,
                }
            }),
        })),

        Expr::Replace { target, from, to } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("replace", |s| {
                Ok(Value::Str(s.replace(from.as_str(), to.as_str())))
            })
        })),

        Expr::Trim { target } => Some(
            recurse(target, env)
                .await
                .and_then(|v| v.map_str("trim", |s| Ok(Value::Str(s.trim().to_owned())))),
        ),

        Expr::Prepend { target, prefix } => Some(
            (async {
                let s = recurse(target, env.clone()).await?;
                let p = recurse(prefix, env).await?;
                match (s, p) {
                    (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
                    (s, p) => Ok(Value::Str(
                        p.into_str("prepend")? + s.into_str("prepend")?.as_str(),
                    )),
                }
            })
            .await,
        ),

        Expr::Append { target, suffix } => Some(
            (async {
                let s = recurse(target, env.clone()).await?;
                let x = recurse(suffix, env).await?;
                match (s, x) {
                    (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
                    (s, x) => Ok(Value::Str(
                        s.into_str("append")? + x.into_str("append")?.as_str(),
                    )),
                }
            })
            .await,
        ),

        Expr::Lower { target } => Some(
            recurse(target, env)
                .await
                .and_then(|v| v.map_str("lower", |s| Ok(Value::Str(s.to_lowercase())))),
        ),

        Expr::Matches { target, pattern } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("matches", |s| {
                get_or_compile_regex(pattern).map(|re| Value::Bool(re.is_match(&s)))
            })
        })),

        Expr::Capture { target, pattern } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("capture", |s| {
                get_or_compile_regex(pattern).map(|re| match re.captures(&s) {
                    None => Value::List(vec![]),
                    Some(caps) => Value::List(
                        (0..caps.len())
                            .map(|i| {
                                caps.get(i)
                                    .map_or(Value::Null, |m| Value::Str(m.as_str().to_owned()))
                            })
                            .collect(),
                    ),
                })
            })
        })),

        Expr::ParseFloat { target } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("parse_float", |s| {
                s.parse::<f64>()
                    .map(Value::Num)
                    .map_err(|e| format!("Invalid float '{}': {}", s, e))
            })
        })),

        Expr::ParseInt { target } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("parse_int", |s| {
                Ok(s.parse::<i64>().map(Value::Int).unwrap_or(Value::Null))
            })
        })),

        Expr::DateParse { target, format } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("date_parse", |date| {
                Ok(parse_date_flexible(&date, format)
                    .map(Value::Int)
                    .unwrap_or(Value::Null))
            })
        })),

        Expr::DateParseRfc3339 { target } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("date_parse_rfc3339", |date| {
                time::OffsetDateTime::parse(&date, &time::format_description::well_known::Rfc3339)
                    .map(|dt| Value::Int(dt.unix_timestamp()))
                    .map_err(|e| format!("Invalid RFC3339 date '{}': {}", date, e))
            })
        })),

        Expr::Fallback { target, default } => Some(
            (async {
                match recurse(target, env.clone()).await? {
                    Value::Null => recurse(default, env).await,
                    Value::Str(s) if s.is_empty() => recurse(default, env).await,
                    v => Ok(v),
                }
            })
            .await,
        ),

        Expr::Lookup { target, table } => Some(
            recurse(target, env)
                .await
                .and_then(|v| v.into_str("lookup"))
                .map(|s| {
                    table
                        .iter()
                        .find(|(k, _)| s == *k)
                        .map(|(_, v)| Value::Str(v.clone()))
                        .unwrap_or(Value::Null)
                }),
        ),

        Expr::StartsWith { target, prefix } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("starts_with", |s| {
                Ok(Value::Bool(s.starts_with(prefix.as_str())))
            })
        })),

        Expr::EndsWith { target, suffix } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("ends_with", |s| {
                Ok(Value::Bool(s.ends_with(suffix.as_str())))
            })
        })),

        Expr::Slice { target, start, end } => Some(recurse(target, env).await.and_then(|v| {
            v.map_str("slice", |s| {
                let len = s.chars().count() as i32;
                let resolve = |i: i32| {
                    if i < 0 {
                        (len + i).max(0) as usize
                    } else {
                        (i as usize).min(len as usize)
                    }
                };
                let s_idx = resolve(*start);
                let e_idx = end.map_or(len as usize, resolve);
                Ok(Value::Str(if s_idx >= e_idx {
                    String::new()
                } else {
                    s.chars().skip(s_idx).take(e_idx - s_idx).collect()
                }))
            })
        })),

        Expr::ResolveUrl { target, base } => Some(
            (async {
                let path = recurse(target, env.clone())
                    .await
                    .and_then(|v| v.into_str("resolve_url"))?;
                let base_url = recurse(base, env)
                    .await
                    .and_then(|v| v.into_str("resolve_url"))?;
                url::Url::parse(&base_url)
                    .and_then(|b| b.join(&path))
                    .map(|u| Value::Str(u.to_string()))
                    .map_err(|e| format!("resolve_url: {}", e))
            })
            .await,
        ),

        Expr::Concat(exprs) => Some(
            (async {
                let mut s = String::new();
                for e in exprs {
                    s.push_str(
                        &recurse(e, env.clone())
                            .await
                            .and_then(|v| v.into_str("concat"))?,
                    );
                }
                Ok(Value::Str(s))
            })
            .await,
        ),

        Expr::List(exprs) => Some(
            (async {
                let mut items = Vec::with_capacity(exprs.len());
                for e in exprs {
                    items.push(recurse(e, env.clone()).await?);
                }
                Ok(Value::List(items))
            })
            .await,
        ),

        Expr::Map { target, transform } => Some(
            (async {
                let items = recurse(target, env.clone())
                    .await
                    .and_then(|v| v.into_list("map"))?;
                let mut results = Vec::with_capacity(items.len());
                for (i, item) in items.into_iter().enumerate() {
                    let mut inner = env.clone();
                    inner.set("$item", item);
                    inner.set("$index", Value::Int(i as i64));
                    match recurse(transform, inner).await? {
                        Value::Null => {}
                        v => results.push(v),
                    }
                }
                Ok(Value::List(results))
            })
            .await,
        ),

        Expr::FlatMap { target, transform } => Some(
            (async {
                let items = recurse(target, env.clone())
                    .await
                    .and_then(|v| v.into_list("flat_map"))?;
                let mut results = Vec::new();
                for (i, item) in items.into_iter().enumerate() {
                    let mut inner = env.clone();
                    inner.set("$item", item);
                    inner.set("$index", Value::Int(i as i64));
                    match recurse(transform, inner).await? {
                        Value::List(inner) => results.extend(inner),
                        _ => return Err("flat_map: body must return a List".into()),
                    }
                }
                Ok(Value::List(results))
            })
            .await,
        ),

        Expr::Filter { target, filter } => Some(
            (async {
                let items = recurse(target, env.clone())
                    .await
                    .and_then(|v| v.into_list("filter"))?;
                let mut results = Vec::new();
                for (i, item) in items.into_iter().enumerate() {
                    let mut inner = env.clone();
                    inner.set("$item", item.clone());
                    inner.set("$index", Value::Int(i as i64));
                    match recurse(filter, inner).await? {
                        Value::Bool(true) => results.push(item),
                        Value::Bool(false) | Value::Null => {}
                        _ => return Err("filter: predicate must return Bool".into()),
                    }
                }
                Ok(Value::List(results))
            })
            .await,
        ),

        Expr::Fold {
            target,
            transform,
            base,
        } => Some(
            (async {
                let items = recurse(target, env.clone())
                    .await
                    .and_then(|v| v.into_list("fold"))?;
                let mut acc = recurse(base, env.clone()).await?;
                for (i, item) in items.into_iter().enumerate() {
                    let mut inner = env.clone();
                    inner.set("$acc", acc);
                    inner.set("$item", item);
                    inner.set("$index", Value::Int(i as i64));
                    acc = recurse(transform, inner).await?;
                }
                Ok(acc)
            })
            .await,
        ),

        Expr::Let { name, value, body } => Some(
            (async {
                let v = recurse(value, env.clone()).await?;
                let mut inner = env;
                inner.set(name, v);
                recurse(body, inner).await
            })
            .await,
        ),

        Expr::If {
            condition,
            then,
            else_,
        } => Some(
            (async {
                match recurse(condition, env.clone()).await? {
                    Value::Bool(true) => recurse(then, env).await,
                    Value::Bool(false) | Value::Null => recurse(else_, env).await,
                    _ => Err("if: condition must be Bool".into()),
                }
            })
            .await,
        ),

        Expr::ToString { target } => Some(
            (async {
                match recurse(target, env).await? {
                    Value::Str(s) => Ok(Value::Str(s)),
                    Value::Int(i) => Ok(Value::Str(i.to_string())),
                    Value::Num(f) => Ok(Value::Str(if f.fract() == 0.0 && f.abs() < 1e15 {
                        (f as i64).to_string()
                    } else {
                        f.to_string()
                    })),
                    Value::Bool(b) => Ok(Value::Str(if b { "true" } else { "false" }.to_owned())),
                    Value::Null => Ok(Value::Null),
                    other => Err(format!("to_string: cannot convert {:?}", other)),
                }
            })
            .await,
        ),

        Expr::Join { target, delimiter } => Some(
            recurse(target, env)
                .await
                .and_then(|v| v.into_list("join"))
                .and_then(|items| {
                    items
                        .into_iter()
                        .filter(|v| !matches!(v, Value::Null))
                        .map(|v| v.into_str("join"))
                        .collect::<Result<Vec<_>, _>>()
                        .map(|parts| Value::Str(parts.join(delimiter)))
                }),
        ),

        Expr::Merge(lists) => Some(
            (async {
                let mut merged = Vec::new();
                for list_expr in lists {
                    let items = recurse(list_expr, env.clone())
                        .await
                        .and_then(|v| v.into_list("merge"))?;
                    merged.extend(items);
                }
                Ok(Value::List(merged))
            })
            .await,
        ),

        Expr::Pref(key) => Some(Ok(env
            .get(&format!("$pref:{}", key))
            .cloned()
            .unwrap_or(Value::Null))),

        Expr::Format { template, args } => Some(
            (async {
                let mut resolved = Vec::with_capacity(args.len());
                for arg in args {
                    resolved.push(
                        recurse(arg, env.clone())
                            .await
                            .and_then(|v| v.into_str("format"))?,
                    );
                }
                let mut result = template.clone();
                for val in resolved {
                    if let Some(pos) = result.find("{}") {
                        result = format!("{}{}{}", &result[..pos], val, &result[pos + 2..]);
                    }
                }
                Ok(Value::Str(result))
            })
            .await,
        ),

        Expr::Not { target } => Some(recurse(target, env).await.and_then(|v| match v {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            Value::Null => Ok(Value::Bool(true)),
            other => Err(format!("not: expected Bool, got {:?}", other)),
        })),

        Expr::StringLen { target } => {
            Some(recurse(target, env).await.and_then(|v| {
                v.map_str("string_len", |s| Ok(Value::Int(s.chars().count() as i64)))
            }))
        }

        _ => None,
    }
}

/// Fetch a URL and return the response body as a String.
pub async fn fetch_body(
    state: &mut crate::wasm::HostState,
    req: &kani_shared::ast::RequestDef,
) -> Result<String, String> {
    let mut url = url::Url::parse(&req.url).map_err(|e| format!("Invalid URL: {}", e))?;
    if !req.queries.is_empty() {
        url.query_pairs_mut().extend_pairs(req.queries.iter());
    }

    state.check_allowed_host(url.host_str().unwrap_or(""))?;

    state.io_count += 1;
    if state.io_count > 32 {
        return Err("Extension exceeded maximum HTTP request count".into());
    }
    if state.call_started_at.elapsed().as_secs() > 120 {
        return Err("Extension exceeded maximum wall time".into());
    }

    let method = match req.method.to_uppercase().as_str() {
        "GET" => rquest::Method::GET,
        "POST" => rquest::Method::POST,
        "PUT" => rquest::Method::PUT,
        "DELETE" => rquest::Method::DELETE,
        m => return Err(format!("Unsupported HTTP method: {}", m)),
    };

    let rquest_url = url
        .to_string()
        .parse::<rquest::Url>()
        .map_err(|e| format!("Invalid URL: {}", e))?;

    let mut builder = state.http_client.inner().request(method, rquest_url);
    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }
    let request = builder.build().map_err(|e| e.to_string())?;

    let ttfb_start = std::time::Instant::now();
    let response = match tokio::time::timeout(
        std::time::Duration::from_secs(90),
        state.http_client.send_request(request),
    )
    .await
    {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(_) => return Err("HTTP request timed out after 90 seconds".into()),
    };
    let ttfb = ttfb_start.elapsed();
    state.last_io_at = Some(std::time::Instant::now());
    if crate::HTTP_LOGGING_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        tracing::info!(
            ttfb_ms = ttfb.as_millis(),
            status_code = response.status().as_u16(),
            url = url.to_string(),
            "fetch_body: connection established"
        );
    }

    const MAX_BYTES: usize = 15 * 1024 * 1024;
    let body = response
        .bytes_limited(MAX_BYTES)
        .await
        .map_err(|e| e.to_string())?
        .to_vec();
    String::from_utf8(body).map_err(|_| "Invalid UTF-8 in response body".to_string())
}

fn get_or_compile_regex(pattern: &str) -> Result<regex::Regex, String> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, regex::Regex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().map_err(|_| "regex cache lock poisoned")?;
    if let Some(re) = guard.get(pattern) {
        return Ok(re.clone());
    }
    let re =
        regex::Regex::new(pattern).map_err(|e| format!("Invalid regex '{}': {}", pattern, e))?;
    guard.insert(pattern.to_owned(), re.clone());
    Ok(re)
}

fn to_numeric(v: Value) -> Result<(f64, bool), String> {
    match v {
        Value::Int(i) => Ok((i as f64, true)),
        Value::Num(f) => Ok((f, false)),
        _ => Err(format!("expected number, got {:?}", v)),
    }
}

fn as_numeric(l: Value, r: Value) -> Result<(f64, f64, bool), String> {
    let (a, ai) = to_numeric(l)?;
    let (b, bi) = to_numeric(r)?;
    Ok((a, b, ai && bi))
}

fn numeric_op(op: &Op, l: Value, r: Value) -> Result<Value, String> {
    let (a, b, int) = as_numeric(l, r)?;
    match op {
        Op::Add => Ok(if int {
            Value::Int(a as i64 + b as i64)
        } else {
            Value::Num(a + b)
        }),
        Op::Sub => Ok(if int {
            Value::Int(a as i64 - b as i64)
        } else {
            Value::Num(a - b)
        }),
        Op::Mul => Ok(if int {
            Value::Int(a as i64 * b as i64)
        } else {
            Value::Num(a * b)
        }),
        Op::Div => {
            if b == 0.0 {
                return Err("division by zero".into());
            }
            Ok(Value::Num(a / b))
        }
        Op::Lt => Ok(Value::Bool(a < b)),
        Op::Gt => Ok(Value::Bool(a > b)),
        Op::Le => Ok(Value::Bool(a <= b)),
        Op::Ge => Ok(Value::Bool(a >= b)),
        _ => Err(format!("{:?}: not a numeric operator", op)),
    }
}
