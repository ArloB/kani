use crate::evaluator::shared::{Env, Value, eval_common_expr, eval_fetch_field, fetch_body};
use kani_shared::ast::{Blueprint, Expr, OffsetType};
use std::future::Future;
use std::pin::Pin;

pub async fn extract_json(
    state: &mut crate::wasm::HostState,
    doc_handle: Option<i32>,
    blueprint: &Blueprint,
) -> Result<serde_json::Value, String> {
    let doc = match (doc_handle, &blueprint.request) {
        (Some(h), _) => state.json_docs.get(&h).ok_or("Invalid handle")?.clone(),
        (None, Some(req)) => fetch_and_parse_json(state, req).await?,
        (None, None) => return Err("No document source".into()),
    };
    extract_json_with_doc(state, doc, blueprint).await
}

pub async fn extract_json_str(
    state: &mut crate::wasm::HostState,
    body: &str,
    blueprint: &Blueprint,
) -> Result<serde_json::Value, String> {
    let doc: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("JSON parse error: {}", e))?;
    extract_json_with_doc(state, doc, blueprint).await
}

async fn extract_json_with_doc(
    state: &mut crate::wasm::HostState,
    doc: serde_json::Value,
    blueprint: &Blueprint,
) -> Result<serde_json::Value, String> {
    let mut env = Env::new();
    for (k, v) in &state.preferences {
        env.set(&format!("$pref:{}", k), Value::Str(v.clone()));
    }
    for binding in &blueprint.bindings {
        let val = eval_json_field(state, &binding.expr, &doc, None, env.clone()).await?;
        env.set(&binding.name, val);
    }

    let container_val = if blueprint.container.is_empty() {
        &doc
    } else {
        doc.pointer(&blueprint.container)
            .ok_or_else(|| format!("Container '{}' not found in document", blueprint.container))?
    };

    let items: Vec<&serde_json::Value> = match container_val.as_array() {
        Some(arr) => arr.iter().collect(),
        None => vec![container_val],
    };

    let mut scalars = serde_json::Map::new();
    for scalar in &blueprint.scalars {
        let val = eval_json_field(state, &scalar.expr, &doc, None, env.clone()).await?;
        match val.to_json() {
            Some(v) => {
                scalars.insert(scalar.name.clone(), v.clone());
                env.set(&format!("$scalar:{}", scalar.name), Value::Json(v));
            }
            None if scalar.optional => {
                scalars.insert(scalar.name.clone(), serde_json::Value::Null);
            }
            None => return Err(format!("Required scalar '{}' produced null", scalar.name)),
        }
    }

    let mut results = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let mut row = serde_json::Map::new();
        for field in &blueprint.fields {
            let val =
                eval_json_field(state, &field.expr, &doc, Some((item, index)), env.clone()).await?;
            match val.to_json() {
                Some(v) => {
                    row.insert(field.name.clone(), v);
                }
                None if field.optional => {
                    row.insert(field.name.clone(), serde_json::Value::Null);
                }
                None => return Err(format!("Required field '{}' produced null", field.name)),
            }
        }
        results.push(serde_json::Value::Object(row));
    }

    Ok(serde_json::json!({ "rows": results, "scalars": scalars }))
}

async fn eval_json_field(
    state: &mut crate::wasm::HostState,
    expr: &Expr,
    doc: &serde_json::Value,
    current: Option<(&serde_json::Value, usize)>,
    env: Env,
) -> Result<Value, String> {
    if let Expr::Fetch {
        url_expr,
        blueprint: sub_bp,
        method,
        headers,
        kind,
    } = expr
    {
        let url_val = eval_json_expr(url_expr, doc, current, env.clone()).await?;
        let url = match url_val {
            Value::Str(s) => s,
            _ => return Err("Fetch: url_expr must evaluate to a String".into()),
        };
        let mut resolved_headers = Vec::with_capacity(headers.len());
        for (k_expr, v_expr) in headers {
            let k = eval_json_expr(k_expr, doc, current, env.clone()).await?;
            let v = eval_json_expr(v_expr, doc, current, env.clone()).await?;
            match (k, v) {
                (Value::Str(k), Value::Str(v)) => resolved_headers.push((k, v)),
                _ => return Err("Fetch: header keys and values must be strings".into()),
            }
        }
        eval_fetch_field(state, &url, method, resolved_headers, sub_bp, kind).await
    } else {
        eval_json_expr(expr, doc, current, env).await
    }
}

/// Evaluates an expression in a JSON document context.
///
/// Returns a boxed future (rather than `async fn`) so recursive calls through
/// `eval_common_expr` don't produce an infinitely-sized state machine.
fn eval_json_expr<'a>(
    expression: &'a Expr,
    doc: &'a serde_json::Value,
    current: Option<(&'a serde_json::Value, usize)>,
    env: Env,
) -> Pin<Box<dyn Future<Output = Result<Value, String>> + 'a>> {
    Box::pin(async move {
        if let Some(result) = eval_common_expr(expression, env.clone(), &|e, env| {
            eval_json_expr(e, doc, current, env)
        })
        .await
        {
            return result;
        }

        match expression {
            Expr::Json(pointer) => Ok(doc
                .pointer(pointer)
                .map(|v| Value::Json(v.clone()))
                .unwrap_or(Value::Null)),

            Expr::SelfRef => current
                .map(|(n, _)| Value::Json((*n).clone()))
                .ok_or_else(|| "SelfRef used outside of a container loop".into()),

            Expr::Index => current
                .map(|(_, i)| Value::Int(i as i64))
                .ok_or_else(|| "Index used outside of a container loop".into()),

            Expr::JsonPtr { target, pointer } => eval_json_expr(target, doc, current, env)
                .await
                .and_then(|v| v.into_json("ptr"))
                .map(|v| {
                    v.pointer(pointer)
                        .map(|v| Value::Json(v.clone()))
                        .unwrap_or(Value::Null)
                }),

            Expr::JsonStr { target } => eval_json_expr(target, doc, current, env)
                .await
                .and_then(|v| v.into_json("str"))
                .map(|v| {
                    v.as_str()
                        .map(|s| Value::Str(s.to_owned()))
                        .unwrap_or(Value::Null)
                }),

            Expr::JsonInt { target } => eval_json_expr(target, doc, current, env)
                .await
                .and_then(|v| v.into_json("int"))
                .map(|v| v.as_i64().map(Value::Int).unwrap_or(Value::Null)),

            Expr::JsonFloat { target } => eval_json_expr(target, doc, current, env)
                .await
                .and_then(|v| v.into_json("float"))
                .map(|v| v.as_f64().map(Value::Num).unwrap_or(Value::Null)),

            Expr::JsonBool { target } => eval_json_expr(target, doc, current, env)
                .await
                .and_then(|v| v.into_json("bool"))
                .map(|v| v.as_bool().map(Value::Bool).unwrap_or(Value::Null)),

            Expr::ArrayLen { target } => eval_json_expr(target, doc, current, env)
                .await
                .and_then(|v| v.into_json("array_len"))
                .map(|v| Value::Int(v.as_array().map(|a| a.len() as i64).unwrap_or(0))),

            Expr::JsonKeys { target } => eval_json_expr(target, doc, current, env)
                .await
                .and_then(|v| v.into_json("keys"))
                .map(|v| {
                    Value::List(
                        v.as_object()
                            .map(|o| o.keys().map(|k| Value::Str(k.clone())).collect())
                            .unwrap_or_default(),
                    )
                }),

            Expr::JsonGet { target, key } => {
                let val = eval_json_expr(target, doc, current, env.clone())
                    .await
                    .and_then(|v| v.into_json("get"))?;
                let key_str = eval_json_expr(key, doc, current, env)
                    .await
                    .and_then(|v| v.into_str("get"))?;
                Ok(val
                    .get(&key_str)
                    .map(|v| Value::Json(v.clone()))
                    .unwrap_or(Value::Null))
            }

            Expr::JsonFind { target, key, value } => {
                let arr = eval_json_expr(target, doc, current, env.clone())
                    .await
                    .and_then(|v| v.into_json("find"))?;
                let key_str = eval_json_expr(key, doc, current, env.clone())
                    .await
                    .and_then(|v| v.into_str("find"))?;
                let val_str = eval_json_expr(value, doc, current, env)
                    .await
                    .and_then(|v| v.into_str("find"))?;
                Ok(arr
                    .as_array()
                    .and_then(|items| {
                        items.iter().find(|item| {
                            item.get(&key_str).and_then(|v| v.as_str()) == Some(val_str.as_str())
                        })
                    })
                    .map(|v| Value::Json(v.clone()))
                    .unwrap_or(Value::Null))
            }

            Expr::JsonArray(items) => {
                let mut arr = Vec::with_capacity(items.len());
                for item in items {
                    let v = eval_json_expr(item, doc, current, env.clone()).await?;
                    arr.push(v.to_json().unwrap_or(serde_json::Value::Null));
                }
                Ok(Value::Json(serde_json::Value::Array(arr)))
            }

            Expr::JsonFold { target } => {
                let items = eval_json_expr(target, doc, current, env)
                    .await
                    .and_then(|v| v.into_list("json_fold"))?;
                let mut merged: Option<serde_json::Value> = None;
                for item in items {
                    let v = item.into_json("json_fold")?;
                    if v.is_null() {
                        continue;
                    }
                    merged = Some(match merged {
                        None => v,
                        Some(acc) => json_merge_two(acc, v)?,
                    });
                }
                Ok(merged.map(Value::Json).unwrap_or(Value::Null))
            }

            _ => Err(format!(
                "Unhandled expression in JSON evaluator: {:?}",
                expression
            )),
        }
    })
}

pub async fn extract_json_paginated(
    state: &mut crate::wasm::HostState,
    page: i32,
    page_size: i32,
    blueprint: &Blueprint,
) -> Result<serde_json::Value, String> {
    let pagination = blueprint
        .pagination
        .as_ref()
        .ok_or("paginated_extract_json called on blueprint without PaginationConfig")?;

    let native_size = pagination.native_page_size;
    let global_start = ((page - 1).max(0) as usize) * (page_size as usize);
    let first_chunk_offset = (global_start / native_size) * native_size;
    let offset_in_first_chunk = global_start % native_size;
    let mut remaining = page_size as usize;
    let mut current_chunk_offset = first_chunk_offset;
    let mut all_rows: Vec<serde_json::Value> = Vec::new();
    let has_next_page;

    let mut cursor: Option<String> = None;

    loop {
        let mut chunk_bp = blueprint.clone();
        if let Some(req) = &mut chunk_bp.request {
            match &pagination.offset_type {
                OffsetType::ItemOffset => {
                    let offset_value = current_chunk_offset.to_string();
                    req.queries.retain(|(k, _)| k != &pagination.offset_param);
                    req.queries
                        .push((pagination.offset_param.clone(), offset_value));
                }
                OffsetType::PageNumber { start } => {
                    let offset_value =
                        (current_chunk_offset / native_size + *start as usize).to_string();
                    req.queries.retain(|(k, _)| k != &pagination.offset_param);
                    req.queries
                        .push((pagination.offset_param.clone(), offset_value));
                }
                OffsetType::CursorToken { .. } => {
                    if let Some(ref c) = cursor {
                        req.queries.retain(|(k, _)| k != &pagination.offset_param);
                        req.queries
                            .push((pagination.offset_param.clone(), c.clone()));
                    }
                }
            }
        }

        let chunk_result = extract_json(state, None, &chunk_bp).await?;

        let empty = vec![];
        let rows = chunk_result["rows"].as_array().unwrap_or(&empty);
        let chunk_len = rows.len();

        let skip = if matches!(pagination.offset_type, OffsetType::CursorToken { .. }) {
            0
        } else if current_chunk_offset == first_chunk_offset {
            offset_in_first_chunk
        } else {
            0
        };
        let available = chunk_len.saturating_sub(skip);
        let to_take = available.min(remaining);

        all_rows.extend_from_slice(&rows[skip..skip + to_take]);
        remaining -= to_take;

        if let OffsetType::CursorToken { next_cursor_field } = &pagination.offset_type {
            let next = chunk_result["scalars"][next_cursor_field.as_str()]
                .as_str()
                .map(str::to_owned);
            let scalar_hnp = chunk_result["scalars"]["has_next_page"].as_bool();
            if remaining == 0 {
                has_next_page = scalar_hnp.unwrap_or(next.is_some());
                break;
            }
            if chunk_len == 0 || next.is_none() || scalar_hnp == Some(false) {
                has_next_page = false;
                break;
            }
            cursor = next;
            continue;
        }

        let scalar_hnp = chunk_result["scalars"]["has_next_page"].as_bool();
        let chunk_full = chunk_len >= native_size;

        if remaining == 0 {
            has_next_page = scalar_hnp.unwrap_or(chunk_full);
            break;
        }
        if chunk_len == 0 || !chunk_full || scalar_hnp == Some(false) {
            has_next_page = false;
            break;
        }

        current_chunk_offset += native_size;
    }

    Ok(serde_json::json!({
        "rows": all_rows,
        "scalars": { "has_next_page": has_next_page }
    }))
}

async fn fetch_and_parse_json(
    state: &mut crate::wasm::HostState,
    req: &kani_shared::ast::RequestDef,
) -> Result<serde_json::Value, String> {
    let body = fetch_body(state, req).await?;
    serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {}", e))
}

fn json_merge_two(a: serde_json::Value, b: serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::Value as J;
    match (a, b) {
        (J::Object(mut ma), J::Object(mb)) => {
            for (k, v) in mb {
                ma.insert(k, v);
            }
            Ok(J::Object(ma))
        }
        (J::Array(mut va), J::Array(vb)) => {
            va.extend(vb);
            Ok(J::Array(va))
        }
        (a, b) => Err(format!(
            "json_merge: cannot merge {} with {}",
            a.type_str(),
            b.type_str()
        )),
    }
}

trait JsonTypeStr {
    fn type_str(&self) -> &'static str;
}
impl JsonTypeStr for serde_json::Value {
    fn type_str(&self) -> &'static str {
        match self {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "bool",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }
}
