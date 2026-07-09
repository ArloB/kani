#![allow(clippy::unwrap_used)]
// Both the interpreted YAML tier (`kani_yaml::build_blueprint`) and the compiled-extension
// codegen path (`kani_cli::codegen::blueprint::emit_blueprint_bytes`) now delegate their
// field/scalar/binding/pagination assembly to the single shared `kani_yaml::build_blueprint_core`.
// This test proves the two entry points still agree byte-for-byte on real fixtures — including
// the `for_each`/`then` sub-fetch chaining that used to be hand-duplicated in three places —
// so a future edit to only one of them fails loudly instead of silently drifting.

use kani_cli::{
    codegen::blueprint::emit_blueprint_bytes,
    yaml::{schema::YamlExtension, validate},
};
use kani_shared::ast::{Blueprint, RequestDef};
use std::path::Path;

fn load_and_validate(fixture: &str) -> kani_cli::yaml::model::ValidatedExtension {
    let path = Path::new("tests/fixtures").join(fixture);
    let src = std::fs::read_to_string(&path).unwrap();
    let ext: YamlExtension = serde_yaml::from_str(&src).unwrap();
    validate::validate(&ext, &src, &path).unwrap()
}

/// Parses the `const BP: &[u8] = &[0x.., ...];` codegen emits back into raw bytes.
fn decode_bytes_const(src: &str) -> Vec<u8> {
    let start = src.find("= &[").unwrap() + 4;
    let end = src.rfind(']').unwrap();
    src[start..end]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).unwrap())
        .collect()
}

fn decode_blueprint(bytes: &[u8]) -> Blueprint {
    let (_version, rest): (u32, &[u8]) = postcard::take_from_bytes(bytes).unwrap();
    postcard::from_bytes(rest).unwrap()
}

fn assert_equivalent_modulo_request(codegen_bp: &Blueprint, interpreter_bp: &Blueprint) {
    assert_eq!(codegen_bp.container, interpreter_bp.container);
    assert_eq!(codegen_bp.bindings, interpreter_bp.bindings);
    assert_eq!(codegen_bp.fields, interpreter_bp.fields);
    assert_eq!(codegen_bp.scalars, interpreter_bp.scalars);
    assert_eq!(codegen_bp.pagination, interpreter_bp.pagination);
    assert_eq!(codegen_bp.request, None);
    assert!(interpreter_bp.request.is_some());
}

#[test]
fn codegen_bytes_and_interpreter_blueprint_agree_for_each_step() {
    let ext = load_and_validate("for_each.yaml");
    let ep = ext.endpoint_by_name("search").unwrap();

    let codegen_src = emit_blueprint_bytes(ep, &ext, "search");
    let codegen_bp = decode_blueprint(&decode_bytes_const(&codegen_src));

    let req = RequestDef {
        url: "https://example.com/search".into(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        endpoint_id: None,
    };
    let interpreter_bp = kani_yaml::build_blueprint(ep, &ext, "search", req);

    assert_equivalent_modulo_request(&codegen_bp, &interpreter_bp);
    // The for_each step's sub-fetch (the previously hand-duplicated logic) must have made it
    // into both paths identically.
    assert!(
        codegen_bp
            .fields
            .iter()
            .any(|f| f.name == "details" && matches!(f.expr, kani_shared::ast::Expr::Fetch { .. }))
    );
}

#[test]
fn codegen_bytes_and_interpreter_blueprint_agree_no_sub_fetches() {
    let ext = load_and_validate("chapter_list.yaml");
    let ep = ext.endpoint_by_name("chapter_list").unwrap();

    let codegen_src = emit_blueprint_bytes(ep, &ext, "chapter_list");
    let codegen_bp = decode_blueprint(&decode_bytes_const(&codegen_src));

    let req = RequestDef {
        url: "https://example.com/manga/123/chapters".into(),
        method: "GET".into(),
        headers: vec![],
        queries: vec![],
        endpoint_id: None,
    };
    let interpreter_bp = kani_yaml::build_blueprint(ep, &ext, "chapter_list", req);

    assert_equivalent_modulo_request(&codegen_bp, &interpreter_bp);
}
