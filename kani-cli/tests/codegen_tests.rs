#![allow(clippy::unwrap_used)]
// Codegen snapshot tests: validate each fixture, generate code, snapshot with insta.

use kani_cli::{
    codegen,
    yaml::{schema::YamlExtension, validate},
};
use std::path::Path;

fn load_and_validate(fixture: &str) -> kani_cli::yaml::model::ValidatedExtension {
    let path = Path::new("tests/fixtures").join(fixture);
    let src = std::fs::read_to_string(&path).unwrap();
    let ext: YamlExtension = serde_yaml::from_str(&src).unwrap();
    validate::validate(&ext, &src, &path).unwrap()
}

#[test]
fn codegen_popular_snapshot() {
    let validated = load_and_validate("popular.yaml");
    let generated = codegen::generate(&validated, false);
    insta::assert_snapshot!("popular_lib_rs", generated.lib_rs);
    insta::assert_snapshot!("popular_cargo_toml", generated.cargo_toml);
}

#[test]
fn codegen_search_snapshot() {
    let validated = load_and_validate("search.yaml");
    let generated = codegen::generate(&validated, false);
    insta::assert_snapshot!("search_lib_rs", generated.lib_rs);
    insta::assert_snapshot!("search_cargo_toml", generated.cargo_toml);
}

#[test]
fn codegen_manga_details_snapshot() {
    let validated = load_and_validate("manga_details.yaml");
    let generated = codegen::generate(&validated, false);
    insta::assert_snapshot!("details_lib_rs", generated.lib_rs);
}

#[test]
fn codegen_chapter_list_snapshot() {
    let validated = load_and_validate("chapter_list.yaml");
    let generated = codegen::generate(&validated, false);
    insta::assert_snapshot!("chapters_lib_rs", generated.lib_rs);
}

#[test]
fn codegen_pages_snapshot() {
    let validated = load_and_validate("pages.yaml");
    let generated = codegen::generate(&validated, false);
    insta::assert_snapshot!("pages_lib_rs", generated.lib_rs);
}

#[test]
fn codegen_get_url_emits_get_url_fn() {
    let validated = load_and_validate("get_url.yaml");
    let generated = codegen::generate(&validated, false);
    assert!(
        generated.lib_rs.contains("get_url"),
        "get_url fixture must emit a get_url fn: {}",
        generated.lib_rs
    );
    assert!(
        generated.lib_rs.contains("manga_id"),
        "get_url fn must reference manga_id: {}",
        generated.lib_rs
    );
}

#[test]
fn codegen_popular_id_in_cargo_toml() {
    let validated = load_and_validate("popular.yaml");
    let generated = codegen::generate(&validated, false);
    assert!(
        generated.cargo_toml.contains("test-popular"),
        "Cargo.toml must contain the crate id: {}",
        generated.cargo_toml
    );
}

#[test]
fn codegen_embedded_bytes_flag() {
    let validated = load_and_validate("popular.yaml");
    let plain = codegen::generate(&validated, false);
    let embedded = codegen::generate(&validated, true);
    // The two outputs differ: embedded mode pre-serialises blueprints
    assert_ne!(
        plain.lib_rs, embedded.lib_rs,
        "embedded_bytes=true should produce different lib.rs output"
    );
}
