use chumsky::prelude::SimpleSpan;
use kani_shared::ast::{Expr, Op};
use crate::error::CliError;

#[derive(Debug, Clone)]
pub enum ParseExpr { // TODO: Add spans to all relevant types
    SelfRef,
    Dom(String),
    Json(String),
    Var(String),
    Literal(String),
    Number(f64),
    Bool(bool),
    Null,
    BinaryOperation { op: Op, lhs: Box<ParseExpr>, rhs: Box<ParseExpr> },
    Let { name: String, value: Box<ParseExpr>, body: Box<ParseExpr> },
    List(Vec<ParseExpr>),
    Concat(Vec<ParseExpr>),
    Merge(Vec<ParseExpr>),
    Pref(String),
    Format { template: String, args: Vec<ParseExpr> },
    JsonArray(Vec<ParseExpr>),
    If { condition: Box<ParseExpr>, then: Box<ParseExpr>, else_: Box<ParseExpr> },
    MethodCall { target: Box<ParseExpr>, name: String, args: Vec<ParseExpr>, span: SimpleSpan },
    MapLiteral(Vec<(String, String)>),
    Index
}

fn collect_results(items: Vec<ParseExpr>) -> Result<Vec<Expr>, Vec<CliError>> {
    let mut exprs = Vec::with_capacity(items.len());
    let mut errors = Vec::new();
    for item in items {
        match Expr::try_from(item) {
            Ok(e) => exprs.push(e),
            Err(mut es) => errors.append(&mut es),
        }
    }
    if errors.is_empty() { Ok(exprs) } else { Err(errors) }
}

impl TryFrom<ParseExpr> for Expr {
    type Error = Vec<CliError>;

    fn try_from(value: ParseExpr) -> Result<Self, Self::Error> {
        macro_rules! accumulate {
            ($res1:expr, $res2:expr => $map_fn:expr) => {
                match ($res1, $res2) {
                    (Ok(v1), Ok(v2)) => Ok($map_fn(Box::new(v1), Box::new(v2))),
                    (r1, r2) => {
                        let mut errs = Vec::new();
                        if let Err(mut e) = r1 { errs.append(&mut e); }
                        if let Err(mut e) = r2 { errs.append(&mut e); }
                        Err(errs)
                    }
                }
            };
            ($res1:expr, $res2:expr, $res3:expr => $map_fn:expr) => {
                match ($res1, $res2, $res3) {
                    (Ok(v1), Ok(v2), Ok(v3)) => Ok($map_fn(Box::new(v1), Box::new(v2), Box::new(v3))),
                    (r1, r2, r3) => {
                        let mut errs = Vec::new();
                        if let Err(mut e) = r1 { errs.append(&mut e); }
                        if let Err(mut e) = r2 { errs.append(&mut e); }
                        if let Err(mut e) = r3 { errs.append(&mut e); }
                        Err(errs)
                    }
                }
            };
        }

        macro_rules! wrap_target {
            ($target_res:expr, $mapper:expr) => {
                $target_res.map(|t| $mapper(Box::new(t)))
            };
        }

        match value {
            ParseExpr::SelfRef => Ok(Expr::SelfRef),
            ParseExpr::Dom(s) => Ok(Expr::Dom(s)),
            ParseExpr::Json(s) => Ok(Expr::Json(s)),
            ParseExpr::Var(s) => Ok(Expr::Var(s)),
            ParseExpr::Literal(s) => Ok(Expr::Literal(s)),
            ParseExpr::Number(n) => Ok(Expr::Number(n)),
            ParseExpr::Bool(b) => Ok(Expr::Bool(b)),
            ParseExpr::Null => Ok(Expr::Null),
            ParseExpr::Pref(s) => Ok(Expr::Pref(s)),
            ParseExpr::Index => Ok(Expr::Index),

            ParseExpr::BinaryOperation { op, lhs, rhs } =>
                accumulate!(Expr::try_from(*lhs), Expr::try_from(*rhs) => |l, r| Expr::BinaryOperation { op, lhs: l, rhs: r }),

            ParseExpr::Let { name, value, body } =>
                accumulate!(Expr::try_from(*value), Expr::try_from(*body) => |v, b| Expr::Let { name, value: v, body: b }),

            ParseExpr::If { condition, then, else_ } =>
                accumulate!(Expr::try_from(*condition), Expr::try_from(*then), Expr::try_from(*else_) => |c, t, e| Expr::If { condition: c, then: t, else_: e }),

            ParseExpr::List(l) => collect_results(l).map(Expr::List),
            ParseExpr::Concat(l) => collect_results(l).map(Expr::Concat),
            ParseExpr::Merge(l) => collect_results(l).map(Expr::Merge),
            ParseExpr::JsonArray(l) => collect_results(l).map(Expr::JsonArray),
            ParseExpr::Format { template, args } => collect_results(args).map(|a| Expr::Format { template, args: a }),

            ParseExpr::MapLiteral(_) => Err(vec![CliError::DslConversion {
                message: "Map literals '{...}' can only be used inside .lookup()".to_string(),
                span: 0..0,
            }]),

            ParseExpr::MethodCall { target, name, args, span } => {
                let target_res = Expr::try_from(*target);

                match (name.as_str(), args.as_slice()) {
                    ("attr", [ParseExpr::Literal(n)]) => wrap_target!(target_res, |t| Expr::Attr { target: t, name: n.clone() }),
                    ("text", []) => wrap_target!(target_res, |t| Expr::Text { target: t }),
                    ("inner_html", []) => wrap_target!(target_res, |t| Expr::InnerHtml { target: t }),
                    ("select", [ParseExpr::Literal(s)]) => wrap_target!(target_res, |t| Expr::Select { target: t, selector: s.clone() }),
                    ("first", [ParseExpr::Literal(s)]) => wrap_target!(target_res, |t| Expr::First { target: t, selector: s.clone() }),
                    ("split", [ParseExpr::Literal(d)]) => wrap_target!(target_res, |t| Expr::Split { target: t, delimiter: d.clone() }),
                    ("at", [ParseExpr::Number(i)]) => wrap_target!(target_res, |t| Expr::At { target: t, index: *i as i32 }),
                    ("trim", []) => wrap_target!(target_res, |t| Expr::Trim { target: t }),
                    ("lower", []) => wrap_target!(target_res, |t| Expr::Lower { target: t }),
                    ("parse_float", []) => wrap_target!(target_res, |t| Expr::ParseFloat { target: t }),
                    ("parse_int", []) => wrap_target!(target_res, |t| Expr::ParseInt { target: t }),
                    ("to_string", []) => wrap_target!(target_res, |t| Expr::ToString { target: t }),
                    ("str", []) => wrap_target!(target_res, |t| Expr::JsonStr { target: t }),
                    ("int", []) => wrap_target!(target_res, |t| Expr::JsonInt { target: t }),
                    ("float", []) => wrap_target!(target_res, |t| Expr::JsonFloat { target: t }),
                    ("bool", []) => wrap_target!(target_res, |t| Expr::JsonBool { target: t }),
                    ("array_len", []) => wrap_target!(target_res, |t| Expr::ArrayLen { target: t }),
                    ("keys", []) => wrap_target!(target_res, |t| Expr::JsonKeys { target: t }),
                    ("json_fold", []) => wrap_target!(target_res, |t| Expr::JsonFold { target: t }),
                    ("children", []) => wrap_target!(target_res, |t| Expr::Children { target: t }),
                    ("date_parse_rfc3339", []) => wrap_target!(target_res, |t| Expr::DateParseRfc3339 { target: t }),
                    ("not", []) => wrap_target!(target_res, |t| Expr::Not { target: t }),
                    ("string_len", []) => wrap_target!(target_res, |t| Expr::StringLen { target: t }),

                    ("slice", [ParseExpr::Number(s)]) => wrap_target!(target_res, |t| Expr::Slice { target: t, start: *s as i32, end: None }),
                    ("slice", [ParseExpr::Number(s), ParseExpr::Number(e)]) => wrap_target!(target_res, |t| Expr::Slice { target: t, start: *s as i32, end: Some(*e as i32) }),
                    ("replace", [ParseExpr::Literal(f), ParseExpr::Literal(to)]) => wrap_target!(target_res, |t| Expr::Replace { target: t, from: f.clone(), to: to.clone() }),
                    ("lookup", [ParseExpr::MapLiteral(table)]) => wrap_target!(target_res, |t| Expr::Lookup { target: t, table: table.clone() }),
                    ("matches", [ParseExpr::Literal(p)]) => wrap_target!(target_res, |t| Expr::Matches { target: t, pattern: p.clone() }),
                    ("capture", [ParseExpr::Literal(p)]) => wrap_target!(target_res, |t| Expr::Capture { target: t, pattern: p.clone() }),
                    ("ptr", [ParseExpr::Literal(p)]) => wrap_target!(target_res, |t| Expr::JsonPtr { target: t, pointer: p.clone() }),
                    ("has_class", [ParseExpr::Literal(c)]) => wrap_target!(target_res, |t| Expr::HasClass { target: t, class: c.clone() }),
                    ("starts_with", [ParseExpr::Literal(p)]) => wrap_target!(target_res, |t| Expr::StartsWith { target: t, prefix: p.clone() }),
                    ("ends_with", [ParseExpr::Literal(s)]) => wrap_target!(target_res, |t| Expr::EndsWith { target: t, suffix: s.clone() }),
                    ("date_parse", [ParseExpr::Literal(f)]) => wrap_target!(target_res, |t| Expr::DateParse { target: t, format: f.clone() }),
                    ("join", [ParseExpr::Literal(d)]) => wrap_target!(target_res, |t| Expr::Join { target: t, delimiter: d.clone() }),

                    // Recursive Method Calls
                    ("map", [tr]) => accumulate!(target_res, Expr::try_from(tr.clone()) => |t, tr| Expr::Map { target: t, transform: tr }),
                    ("flat_map", [tr]) => accumulate!(target_res, Expr::try_from(tr.clone()) => |t, tr| Expr::FlatMap { target: t, transform: tr }),
                    ("filter", [f]) => accumulate!(target_res, Expr::try_from(f.clone()) => |t, f| Expr::Filter { target: t, filter: f }),
                    ("prepend", [p]) => accumulate!(target_res, Expr::try_from(p.clone()) => |t, p| Expr::Prepend { target: t, prefix: p }),
                    ("append", [s]) => accumulate!(target_res, Expr::try_from(s.clone()) => |t, s| Expr::Append { target: t, suffix: s }),
                    ("fallback", [d]) => accumulate!(target_res, Expr::try_from(d.clone()) => |t, d| Expr::Fallback { target: t, default: d }),
                    ("resolve_url", [b]) => accumulate!(target_res, Expr::try_from(b.clone()) => |t, b| Expr::ResolveUrl { target: t, base: b }),
                    ("get", [k]) => accumulate!(target_res, Expr::try_from(k.clone()) => |t, k| Expr::JsonGet { target: t, key: k }),

                    ("fold", [b, tr]) => accumulate!(target_res, Expr::try_from(b.clone()), Expr::try_from(tr.clone()) => |t, b, tr| Expr::Fold { target: t, transform: tr, base: b }),
                    ("find", [k, v]) => accumulate!(target_res, Expr::try_from(k.clone()), Expr::try_from(v.clone()) => |t, k, v| Expr::JsonFind { target: t, key: k, value: v }),

                    _ => {
                        let mut errs = target_res.err().unwrap_or_default();
                        errs.push(CliError::DslConversion {
                            message: format!("Unknown method '{}' or invalid arguments", name),
                            span: span.into_range(),
                        });
                        Err(errs)
                    }
                }
            }
        }
    }
}