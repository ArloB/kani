#![allow(clippy::unwrap_used)]
// YAML validation tests: accept valid YAML, reject invalid with expected errors.

use std::path::Path;
use kani_cli::yaml::{schema::YamlExtension, validate};

fn validate_str(yaml: &str) -> Result<kani_cli::yaml::model::ValidatedExtension, Vec<kani_cli::error::CliError>> {
    let path = Path::new("test.yaml");
    let ext: YamlExtension = serde_yaml::from_str(yaml).expect("fixture must parse as YAML");
    validate::validate(&ext, yaml, path)
}

fn assert_valid(yaml: &str) {
    assert!(validate_str(yaml).is_ok(), "expected valid, got errors");
}

fn assert_invalid_containing(yaml: &str, needle: &str) {
    match validate_str(yaml) {
        Ok(_) => panic!("expected validation error containing '{}' but validation passed", needle),
        Err(errs) => {
            let messages: Vec<String> = errs.iter().map(|e| e.to_string()).collect();
            assert!(
                messages.iter().any(|m| m.contains(needle)),
                "expected an error containing '{}', got: {:?}", needle, messages
            );
        }
    }
}

// ── Valid fixtures ───────────────────────────────────────────────────────────

#[test]
fn valid_minimal_extension() {
    assert_valid(r#"
id: minimal
name: Minimal
version: "0.1.0"
base_url: "https://example.com"
"#);
}

#[test]
fn valid_popular_endpoint() {
    assert_valid(r#"
id: pop-test
name: PopTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    route: "/manga"
    container: ".item"
    fields:
      id: 'self.attr("href")'
      title: 'self.text()'
"#);
}

#[test]
fn valid_delegated_popular() {
    assert_valid(r#"
id: delegate-test
name: DelegateTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    delegate_to: search
    empty_without_filters: true
  search:
    route: "/search"
    queries:
      q: $query$
    container: ".item"
    fields:
      id: 'self.attr("href")'
      title: 'self.text()'
"#);
}

#[test]
fn valid_search_endpoint() {
    assert_valid(r#"
id: search-test
name: SearchTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  search:
    route: "/search"
    queries:
      q: $query$
    container: ".item"
    fields:
      id: 'self.attr("href")'
      title: 'self.text()'
"#);
}

#[test]
fn valid_manga_details_all_required_fields() {
    assert_valid(r#"
id: details-test
name: DetailsTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  manga_details:
    route: "/manga/$manga_id$"
    container: ":root"
    fields:
      id: '"$manga_id$"'
      title: 'dom("h1").text()'
      status: 'dom(".status").text()'
"#);
}

#[test]
fn valid_chapter_list() {
    assert_valid(r#"
id: chapters-test
name: ChaptersTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  chapter_list:
    route: "/manga/$manga_id$/chapters"
    container: ".chapter"
    fields:
      id: 'self.attr("data-id")'
"#);
}

#[test]
fn valid_pages() {
    assert_valid(r#"
id: pages-test
name: PagesTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  pages:
    route: "/chapter/$chapter_id$"
    container: "img"
    fields:
      index: "index()"
      url: 'self.attr("src")'
"#);
}

#[test]
fn valid_full_example_fixture() {
    let path = Path::new("tests/fixtures/manga_details.yaml");
    let src = std::fs::read_to_string(path).unwrap();
    let ext: YamlExtension = serde_yaml::from_str(&src).unwrap();
    assert!(validate::validate(&ext, &src, path).is_ok());
}

// ── Invalid: missing required fields ────────────────────────────────────────

#[test]
fn invalid_popular_missing_title() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    route: "/manga"
    container: ".item"
    fields:
      id: 'self.attr("href")'
"#, "title");
}

#[test]
fn invalid_popular_missing_id() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    route: "/manga"
    container: ".item"
    fields:
      title: 'self.text()'
"#, "id");
}

#[test]
fn invalid_manga_details_missing_status() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  manga_details:
    route: "/manga/$manga_id$"
    container: ":root"
    fields:
      id: '"$manga_id$"'
      title: 'dom("h1").text()'
"#, "status");
}

#[test]
fn invalid_pages_missing_url() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  pages:
    route: "/chapter/$chapter_id$"
    container: "img"
    fields:
      index: "index()"
"#, "url");
}

#[test]
fn invalid_pages_missing_index() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  pages:
    route: "/chapter/$chapter_id$"
    container: "img"
    fields:
      url: 'self.attr("src")'
"#, "index");
}

// ── Invalid: missing route ────────────────────────────────────────────────────

#[test]
fn invalid_endpoint_missing_route() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    container: ".item"
    fields:
      id: 'self.attr("href")'
      title: 'self.text()'
"#, "route");
}

// ── Invalid: unknown route variable ──────────────────────────────────────────

#[test]
fn invalid_route_unknown_variable() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    route: "/manga/$unknown_var$"
    container: ".item"
    fields:
      id: 'self.attr("href")'
      title: 'self.text()'
"#, "unknown_var");
}

// ── Invalid: bad DSL expression ───────────────────────────────────────────────

#[test]
fn invalid_dsl_parse_error() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    route: "/manga"
    container: ".item"
    fields:
      id: '!!invalid dsl !!'
      title: 'self.text()'
"#, "DSL parse failed");
}

// ── Invalid: delegated popular with bad target ────────────────────────────────

#[test]
fn invalid_delegated_popular_bad_target() {
    assert_invalid_containing(r#"
id: err-test
name: ErrTest
version: "0.1.0"
base_url: "https://example.com"
endpoints:
  popular:
    delegate_to: popular
"#, "delegate_to");
}
