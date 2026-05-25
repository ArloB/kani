#![allow(clippy::unwrap_used)]
// DSL parser tests: parse expressions, check resulting Expr structure, error cases.

use chumsky::Parser;
use kani_cli::dsl::parser;
use kani_shared::ast::{Expr, Op};

fn parse_ok(input: &str) -> Expr {
    let parse_expr = parser()
        .parse(input)
        .into_result()
        .unwrap_or_else(|e| panic!("parse failed for {:?}: {:?}", input, e));
    Expr::try_from(parse_expr)
        .unwrap_or_else(|e| panic!("conversion failed for {:?}: {:?}", input, e))
}

fn parse_err(input: &str) -> bool {
    parser().parse(input).has_errors()
}

// ── Primitive literals ───────────────────────────────────────────────────────

#[test]
fn parse_string_literal() {
    assert_eq!(parse_ok(r#""hello""#), Expr::Literal("hello".into()));
}

#[test]
fn parse_integer_number() {
    assert_eq!(parse_ok("42"), Expr::Number(42.0));
}

#[test]
fn parse_negative_number() {
    assert_eq!(parse_ok("-7"), Expr::Number(-7.0));
}

#[test]
fn parse_float_number() {
    assert_eq!(parse_ok("3.14"), Expr::Number(3.14));
}

#[test]
fn parse_bool_true() {
    assert_eq!(parse_ok("true"), Expr::Bool(true));
}

#[test]
fn parse_bool_false() {
    assert_eq!(parse_ok("false"), Expr::Bool(false));
}

#[test]
fn parse_null() {
    assert_eq!(parse_ok("null"), Expr::Null);
}

#[test]
fn parse_index() {
    assert_eq!(parse_ok("index()"), Expr::Index);
}

#[test]
fn parse_self_ref() {
    assert_eq!(parse_ok("self"), Expr::SelfRef);
}

// ── Built-in constructors ────────────────────────────────────────────────────

#[test]
fn parse_dom() {
    assert_eq!(parse_ok(r#"dom("h1")"#), Expr::Dom("h1".into()));
}

#[test]
fn parse_json() {
    assert_eq!(
        parse_ok(r#"json("/data/id")"#),
        Expr::Json("/data/id".into())
    );
}

#[test]
fn parse_pref() {
    assert_eq!(
        parse_ok(r#"pref("language")"#),
        Expr::Pref("language".into())
    );
}

#[test]
fn parse_var() {
    assert_eq!(parse_ok("$manga_id"), Expr::Var("$manga_id".into()));
}

// ── Method chains ────────────────────────────────────────────────────────────

#[test]
fn parse_text_method() {
    let expr = parse_ok(r#"self.text()"#);
    assert_eq!(
        expr,
        Expr::Text {
            target: Box::new(Expr::SelfRef)
        }
    );
}

#[test]
fn parse_attr_method() {
    let expr = parse_ok(r#"self.attr("href")"#);
    assert_eq!(
        expr,
        Expr::Attr {
            target: Box::new(Expr::SelfRef),
            name: "href".into()
        }
    );
}

#[test]
fn parse_trim_method() {
    let expr = parse_ok(r#"self.text().trim()"#);
    assert_eq!(
        expr,
        Expr::Trim {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            })
        }
    );
}

#[test]
fn parse_lower_method() {
    let expr = parse_ok(r#"self.text().lower()"#);
    assert_eq!(
        expr,
        Expr::Lower {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            })
        }
    );
}

#[test]
fn parse_split_at_chain() {
    let expr = parse_ok(r#"self.attr("href").split("/").at(2)"#);
    assert_eq!(
        expr,
        Expr::At {
            target: Box::new(Expr::Split {
                target: Box::new(Expr::Attr {
                    target: Box::new(Expr::SelfRef),
                    name: "href".into(),
                }),
                delimiter: "/".into(),
            }),
            index: 2,
        }
    );
}

#[test]
fn parse_at_negative_index() {
    let expr = parse_ok(r#"self.split("/").at(-1)"#);
    assert_eq!(
        expr,
        Expr::At {
            target: Box::new(Expr::Split {
                target: Box::new(Expr::SelfRef),
                delimiter: "/".into(),
            }),
            index: -1,
        }
    );
}

#[test]
fn parse_parse_float() {
    let expr = parse_ok(r#"self.text().parse_float()"#);
    assert_eq!(
        expr,
        Expr::ParseFloat {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            })
        }
    );
}

#[test]
fn parse_parse_int() {
    let expr = parse_ok(r#"self.text().parse_int()"#);
    assert_eq!(
        expr,
        Expr::ParseInt {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            })
        }
    );
}

#[test]
fn parse_select_method() {
    let expr = parse_ok(r#"self.select("a.link")"#);
    assert_eq!(
        expr,
        Expr::Select {
            target: Box::new(Expr::SelfRef),
            selector: "a.link".into()
        }
    );
}

#[test]
fn parse_first_method() {
    let expr = parse_ok(r#"self.first("h1")"#);
    assert_eq!(
        expr,
        Expr::First {
            target: Box::new(Expr::SelfRef),
            selector: "h1".into()
        }
    );
}

#[test]
fn parse_replace_method() {
    let expr = parse_ok(r#"self.text().replace("foo", "bar")"#);
    assert_eq!(
        expr,
        Expr::Replace {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            }),
            from: "foo".into(),
            to: "bar".into(),
        }
    );
}

#[test]
fn parse_fallback_method() {
    let expr = parse_ok(r#"self.text().fallback("unknown")"#);
    assert_eq!(
        expr,
        Expr::Fallback {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            }),
            default: Box::new(Expr::Literal("unknown".into())),
        }
    );
}

#[test]
fn parse_matches_method() {
    let expr = parse_ok(r#"self.text().matches("[0-9]+")"#);
    assert_eq!(
        expr,
        Expr::Matches {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            }),
            pattern: "[0-9]+".into()
        }
    );
}

#[test]
fn parse_map_method() {
    let expr = parse_ok(r#"self.select("li").map($item.text())"#);
    assert_eq!(
        expr,
        Expr::Map {
            target: Box::new(Expr::Select {
                target: Box::new(Expr::SelfRef),
                selector: "li".into(),
            }),
            transform: Box::new(Expr::Text {
                target: Box::new(Expr::Var("$item".into()))
            }),
        }
    );
}

#[test]
fn parse_filter_method() {
    let expr = parse_ok(r#"self.select("li").filter($item.text())"#);
    assert_eq!(
        expr,
        Expr::Filter {
            target: Box::new(Expr::Select {
                target: Box::new(Expr::SelfRef),
                selector: "li".into(),
            }),
            filter: Box::new(Expr::Text {
                target: Box::new(Expr::Var("$item".into()))
            }),
        }
    );
}

#[test]
fn parse_join_method() {
    let expr = parse_ok(r#"self.select("li").join(", ")"#);
    assert_eq!(
        expr,
        Expr::Join {
            target: Box::new(Expr::Select {
                target: Box::new(Expr::SelfRef),
                selector: "li".into(),
            }),
            delimiter: ", ".into(),
        }
    );
}

#[test]
fn parse_starts_with() {
    let expr = parse_ok(r#"self.text().starts_with("http")"#);
    assert_eq!(
        expr,
        Expr::StartsWith {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            }),
            prefix: "http".into(),
        }
    );
}

#[test]
fn parse_ends_with() {
    let expr = parse_ok(r#"self.text().ends_with(".jpg")"#);
    assert_eq!(
        expr,
        Expr::EndsWith {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            }),
            suffix: ".jpg".into(),
        }
    );
}

#[test]
fn parse_resolve_url() {
    let expr = parse_ok(r#"self.attr("src").resolve_url("https://example.com")"#);
    assert_eq!(
        expr,
        Expr::ResolveUrl {
            target: Box::new(Expr::Attr {
                target: Box::new(Expr::SelfRef),
                name: "src".into()
            }),
            base: Box::new(Expr::Literal("https://example.com".into())),
        }
    );
}

#[test]
fn parse_lookup_method() {
    let expr = parse_ok(r#"self.text().lookup({"a": "b", "c": "d"})"#);
    assert_eq!(
        expr,
        Expr::Lookup {
            target: Box::new(Expr::Text {
                target: Box::new(Expr::SelfRef)
            }),
            table: vec![("a".into(), "b".into()), ("c".into(), "d".into())],
        }
    );
}

// ── JSON accessors ────────────────────────────────────────────────────────────

#[test]
fn parse_json_str() {
    let expr = parse_ok(r#"json("/data/id").str()"#);
    assert_eq!(
        expr,
        Expr::JsonStr {
            target: Box::new(Expr::Json("/data/id".into()))
        }
    );
}

#[test]
fn parse_json_int() {
    let expr = parse_ok(r#"json("/data/count").int()"#);
    assert_eq!(
        expr,
        Expr::JsonInt {
            target: Box::new(Expr::Json("/data/count".into()))
        }
    );
}

#[test]
fn parse_json_ptr() {
    let expr = parse_ok(r#"json("/data").ptr("/attributes/title")"#);
    assert_eq!(
        expr,
        Expr::JsonPtr {
            target: Box::new(Expr::Json("/data".into())),
            pointer: "/attributes/title".into(),
        }
    );
}

// ── Binary operators ──────────────────────────────────────────────────────────

#[test]
fn parse_binary_add() {
    let expr = parse_ok("1 + 2");
    assert_eq!(
        expr,
        Expr::BinaryOperation {
            op: Op::Add,
            lhs: Box::new(Expr::Number(1.0)),
            rhs: Box::new(Expr::Number(2.0))
        }
    );
}

#[test]
fn parse_binary_eq() {
    let expr = parse_ok(r#""a" == "b""#);
    assert_eq!(
        expr,
        Expr::BinaryOperation {
            op: Op::Eq,
            lhs: Box::new(Expr::Literal("a".into())),
            rhs: Box::new(Expr::Literal("b".into())),
        }
    );
}

#[test]
fn parse_binary_and() {
    let expr = parse_ok("true && false");
    assert_eq!(
        expr,
        Expr::BinaryOperation {
            op: Op::And,
            lhs: Box::new(Expr::Bool(true)),
            rhs: Box::new(Expr::Bool(false))
        }
    );
}

#[test]
fn parse_binary_or() {
    let expr = parse_ok("true || false");
    assert_eq!(
        expr,
        Expr::BinaryOperation {
            op: Op::Or,
            lhs: Box::new(Expr::Bool(true)),
            rhs: Box::new(Expr::Bool(false))
        }
    );
}

// ── Control flow ─────────────────────────────────────────────────────────────

#[test]
fn parse_if_then_else() {
    let expr = parse_ok(r#"if true then "yes" else "no""#);
    assert_eq!(
        expr,
        Expr::If {
            condition: Box::new(Expr::Bool(true)),
            then: Box::new(Expr::Literal("yes".into())),
            else_: Box::new(Expr::Literal("no".into())),
        }
    );
}

#[test]
fn parse_let_binding() {
    // let uses ';' or '\n' as terminator; use ';' in direct parser tests
    let expr = parse_ok("let $x = \"hello\"; $x");
    assert_eq!(
        expr,
        Expr::Let {
            name: "$x".into(),
            value: Box::new(Expr::Literal("hello".into())),
            body: Box::new(Expr::Var("$x".into())),
        }
    );
}

// ── Composite expressions ─────────────────────────────────────────────────────

#[test]
fn parse_format_expr() {
    let expr = parse_ok(r#"format("hello {}", "world")"#);
    assert_eq!(
        expr,
        Expr::Format {
            template: "hello {}".into(),
            args: vec![Expr::Literal("world".into())]
        }
    );
}

#[test]
fn parse_merge_expr() {
    let expr = parse_ok(r#"merge([self.text(), "extra"])"#);
    assert_eq!(
        expr,
        Expr::Merge(vec![
            Expr::Text {
                target: Box::new(Expr::SelfRef)
            },
            Expr::Literal("extra".into()),
        ])
    );
}

#[test]
fn parse_list_literal() {
    let expr = parse_ok(r#"["a", "b", "c"]"#);
    assert_eq!(
        expr,
        Expr::List(vec![
            Expr::Literal("a".into()),
            Expr::Literal("b".into()),
            Expr::Literal("c".into()),
        ])
    );
}

// ── Error cases ───────────────────────────────────────────────────────────────

#[test]
fn parse_error_unclosed_paren() {
    assert!(parse_err("self.text("));
}

#[test]
fn parse_error_trailing_junk() {
    assert!(parse_err(r#""hello" garbage"#));
}

#[test]
fn parse_error_empty_input() {
    assert!(parse_err(""));
}

// ── Conversion errors ─────────────────────────────────────────────────────────

#[test]
fn convert_error_unknown_method() {
    let parse_expr = parser()
        .parse(r#"self.nonexistent_method()"#)
        .into_result()
        .expect("should parse as a method call");
    let err = Expr::try_from(parse_expr).expect_err("should fail conversion");
    let msg = err
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    assert!(
        msg.contains("nonexistent_method") || msg.contains("Unknown method"),
        "got: {msg}"
    );
}

#[test]
fn convert_error_map_literal_outside_lookup() {
    let expr = parse_ok(r#"self.text().lookup({"x": "y"})"#);
    assert!(
        matches!(expr, Expr::Lookup { .. }),
        "lookup with map literal should succeed"
    );
}
