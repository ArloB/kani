use crate::evaluator::shared::{Env, Value, eval_common_expr, fetch_body};
use kani_shared::ast::{Blueprint, Expr};
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

    let mut env = Env::new();
    for (k, v) in &state.preferences {
        env.set(&format!("$pref:{}", k), Value::Str(v.clone()));
    }
    for binding in &blueprint.bindings {
        let val = eval_json_expr(&binding.expr, &doc, None, env.clone()).await?;
        env.set(&binding.name, val);
    }

    let container_val = if blueprint.container.is_empty() {
        &doc
    } else {
        doc.pointer(&blueprint.container)
            .ok_or_else(|| format!("Container '{}' not found in document", blueprint.container))?
    };

    // Arrays iterate element-by-element; anything else is a single-item container
    // (used for details endpoints where the document root is the container).
    let items: Vec<&serde_json::Value> = match container_val.as_array() {
        Some(arr) => arr.iter().collect(),
        None => vec![container_val],
    };

    let mut scalars = serde_json::Map::new();
    for scalar in &blueprint.scalars {
        let val = eval_json_expr(&scalar.expr, &doc, None, env.clone()).await?;
        match val.to_json() {
            Some(v) => {
                scalars.insert(scalar.name.clone(), v);
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
            let val = eval_json_expr(&field.expr, &doc, Some((item, index)), env.clone()).await?;
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
