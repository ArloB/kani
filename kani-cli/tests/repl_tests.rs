#![allow(clippy::unwrap_used)]

use kani_cli::repl::{explain, test_cmd};
use std::path::Path;

fn fixture(name: &str) -> String {
    Path::new("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn inspect_repl_test_yaml() {
    kani_cli::repl::inspect::run(&fixture("repl-test.yaml")).unwrap();
}

#[test]
fn explain_trim_chain_has_steps() {
    let trace = explain::explain(r#"self.first("a").text().trim()"#).unwrap();
    assert!(
        trace.len() >= 3,
        "expected ≥3 trace steps, got {}",
        trace.len()
    );
    assert_eq!(
        trace.steps[0].expr_kind, "Trim",
        "outermost step should be Trim"
    );
    let last = trace.steps.last().unwrap();
    assert_eq!(last.expr_kind, "Self", "innermost step should be Self");
}

#[test]
fn explain_longer_chain() {
    let trace = explain::explain(r#"self.first("a").attr("href").split("/").at(-1)"#).unwrap();
    assert!(
        trace.len() >= 3,
        "expected ≥3 trace steps, got {}",
        trace.len()
    );
    assert_eq!(trace.steps[0].expr_kind, "At");
    assert_eq!(trace.steps.last().unwrap().expr_kind, "Self");
}

#[test]
fn explain_display_formatting() {
    let trace = explain::explain(r#"self.text().trim()"#).unwrap();
    let rendered = trace.to_string();
    assert!(rendered.contains("[Trim]"), "expected [Trim] in output");
    assert!(rendered.contains("[Text]"), "expected [Text] in output");
    assert!(rendered.contains("[Self]"), "expected [Self] in output");
    assert!(
        rendered.contains("  [Text]"),
        "Text should be indented under Trim"
    );
}

#[test]
fn test_command_counts_rows() {
    test_cmd::run_test(
        &fixture("repl-test.yaml"),
        &fixture("repl-test.har"),
        "popular",
        3,
    )
    .unwrap();
}

#[test]
fn test_command_wrong_count_fails() {
    let result = test_cmd::run_test(
        &fixture("repl-test.yaml"),
        &fixture("repl-test.har"),
        "popular",
        99,
    );
    assert!(result.is_err(), "should fail with wrong expected count");
}

#[test]
fn replay_matches_expected() {
    test_cmd::run_replay(
        &fixture("repl-test.yaml"),
        &fixture("repl-test.har"),
        "popular",
        &fixture("expected-popular.json"),
    )
    .unwrap();
}
