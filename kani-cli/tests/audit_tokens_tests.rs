#![allow(clippy::unwrap_used)]

use kani_cli::commands::audit_tokens;
use std::path::Path;

#[test]
fn scan_finds_hex_and_rgb_and_tailwind_violations() {
    let dir = Path::new("tests/fixtures/audit-tokens");
    let violations = audit_tokens::scan(dir).unwrap();

    let literals: Vec<&str> = violations.iter().map(|v| v.literal.as_str()).collect();
    assert!(literals.contains(&"#fff"), "should flag 3-digit hex");
    assert!(literals.contains(&"#e8545a"), "should flag 6-digit hex");
    assert!(
        literals.iter().any(|l| l.starts_with("rgb")),
        "should flag rgb() call"
    );
    assert!(
        literals.contains(&"text-white"),
        "should flag tailwind colour literal"
    );
    assert!(
        literals.contains(&"bg-gray-800"),
        "should flag tailwind palette class"
    );
    assert_eq!(violations.len(), 5, "expected exactly 5 violations");
}

#[test]
fn scan_ignores_semantic_tokens() {
    let dir = Path::new("tests/fixtures/audit-tokens");
    let violations = audit_tokens::scan(dir).unwrap();

    let literals: Vec<&str> = violations.iter().map(|v| v.literal.as_str()).collect();
    assert!(
        !literals.contains(&"text-accent"),
        "text-accent is a semantic token, not a violation"
    );
    assert!(
        !literals.iter().any(|l| l.contains("bg-surface")),
        "bg-surface is a semantic token, not a violation"
    );
}

#[test]
fn check_ratchet_fails_only_above_baseline() {
    let dir = Path::new("tests/fixtures/audit-tokens");
    assert!(
        audit_tokens::run(dir, true, 4).is_err(),
        "5 violations should exceed baseline of 4"
    );
    assert!(
        audit_tokens::run(dir, true, 5).is_ok(),
        "5 violations should not exceed baseline of 5"
    );
    assert!(
        audit_tokens::run(dir, true, 10).is_ok(),
        "5 violations are under baseline of 10"
    );
    assert!(
        audit_tokens::run(dir, false, 0).is_ok(),
        "without --check, never fails"
    );
}

#[test]
fn scan_skips_dist_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let dist = tmp.path().join("dist");
    std::fs::create_dir(&dist).unwrap();
    let bad = dist.join("bundle.js");
    std::fs::write(&bad, "const c = '#ff0000';").unwrap();

    let violations = audit_tokens::scan(tmp.path()).unwrap();
    assert!(violations.is_empty(), "should skip files inside dist/");
}
