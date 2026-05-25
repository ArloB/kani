//! Sanitization, parsing, and other shared utilities.
use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// Parse a date string using a format pattern, falling back from datetime to date-only.
/// Returns a Unix timestamp in seconds.
pub fn parse_date_flexible(date: &str, format: &str) -> std::result::Result<i64, String> {
    let fmt = time::format_description::parse(format)
        .map_err(|e| format!("Invalid format string: {}", e))?;
    if let Ok(dt) = time::PrimitiveDateTime::parse(date, &fmt) {
        return Ok(dt.assume_utc().unix_timestamp());
    }
    if let Ok(d) = time::Date::parse(date, &fmt) {
        return Ok(time::PrimitiveDateTime::new(d, time::Time::MIDNIGHT)
            .assume_utc()
            .unix_timestamp());
    }
    Err(format!(
        "Unable to parse date '{}' with format '{}'",
        date, format
    ))
}

/// Sanitizes a string to be used as a safe filename or directory name.
pub fn sanitize_filename(name: &str) -> String {
    let mut safe_name = String::with_capacity(name.len());

    for c in name.chars() {
        if c == '/'
            || c == '\\'
            || c == '<'
            || c == '>'
            || c == ':'
            || c == '"'
            || c == '|'
            || c == '?'
            || c == '*'
        {
            continue;
        }

        if c.is_control() {
            continue;
        }

        safe_name.push(c);
    }

    let safe_name = safe_name.trim().trim_matches('.').to_string();

    if safe_name.is_empty() {
        "_unnamed".to_string()
    } else {
        safe_name
    }
}

/// Resolves `target` and asserts it remains under `root`.
/// Fails if `target` does not yet exist — use the parent dir for new files.
pub fn assert_within_root(root: &Path, target: &Path) -> Result<PathBuf> {
    let full = if target.exists() {
        dunce::canonicalize(target)?
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| Error::Internal("path has no parent".to_string()))?;

        let canonical_parent = dunce::canonicalize(parent)
            .map_err(|_| Error::Internal("parent directory does not exist".to_string()))?;

        let file_name = target
            .file_name()
            .ok_or_else(|| Error::Internal("path has no filename".to_string()))?;

        canonical_parent.join(file_name)
    };

    let canonical_root = dunce::canonicalize(root)?;

    if !full.starts_with(&canonical_root) {
        return Err(Error::Internal(format!(
            "path traversal detected: {:?} escapes root {:?}",
            full, canonical_root
        )));
    }

    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // ── sanitize_filename ────────────────────────────────────────────────────

    #[test]
    fn normal_text_is_unchanged() {
        assert_eq!(sanitize_filename("My Manga Vol 1"), "My Manga Vol 1");
    }

    #[test]
    fn forward_slash_removed() {
        assert_eq!(sanitize_filename("a/b/c"), "abc");
    }

    #[test]
    fn backslash_removed() {
        assert_eq!(sanitize_filename("a\\b"), "ab");
    }

    #[test]
    fn forbidden_chars_stripped() {
        assert_eq!(sanitize_filename("file:name"), "filename");
        assert_eq!(sanitize_filename("a<b>c"), "abc");
        assert_eq!(sanitize_filename("foo|bar"), "foobar");
        assert_eq!(sanitize_filename("test?query"), "testquery");
        assert_eq!(sanitize_filename("glob*"), "glob");
        assert_eq!(sanitize_filename(r#"say "hello""#), "say hello");
    }

    #[test]
    fn control_chars_removed() {
        assert_eq!(sanitize_filename("hello\x00world"), "helloworld");
        assert_eq!(sanitize_filename("tab\there"), "tabhere");
        assert_eq!(sanitize_filename("newline\nhere"), "newlinehere");
    }

    #[test]
    fn all_forbidden_produces_unnamed() {
        assert_eq!(sanitize_filename("/\\<>:\"|?*"), "_unnamed");
    }

    #[test]
    fn empty_input_produces_unnamed() {
        assert_eq!(sanitize_filename(""), "_unnamed");
    }

    #[test]
    fn only_dots_produces_unnamed() {
        assert_eq!(sanitize_filename("..."), "_unnamed");
    }

    #[test]
    fn leading_trailing_dots_trimmed() {
        assert_eq!(sanitize_filename(".hidden."), "hidden");
    }

    #[test]
    fn leading_trailing_whitespace_trimmed() {
        assert_eq!(sanitize_filename("  spaced  "), "spaced");
    }

    #[test]
    fn unicode_preserved() {
        assert_eq!(sanitize_filename("Berserk 全集"), "Berserk 全集");
    }

    // ── assert_within_root ───────────────────────────────────────────────────

    #[test]
    fn allows_existing_file_within_root() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file.txt");
        fs::write(&file, b"").unwrap();

        let result = assert_within_root(dir.path(), &file);
        assert!(result.is_ok());
    }

    #[test]
    fn allows_new_file_whose_parent_is_root() {
        let dir = tempdir().unwrap();
        let new_file = dir.path().join("new_file.txt");
        // File doesn't exist yet but parent does.
        let result = assert_within_root(dir.path(), &new_file);
        assert!(result.is_ok());
    }

    #[test]
    fn allows_file_in_subdirectory() {
        let dir = tempdir().unwrap();
        let subdir = dir.path().join("sub");
        fs::create_dir_all(&subdir).unwrap();
        let file = subdir.join("file.txt");
        fs::write(&file, b"").unwrap();

        let result = assert_within_root(dir.path(), &file);
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_path_that_escapes_root() {
        let root = tempdir().unwrap();
        let other = tempdir().unwrap();
        // A file that lives outside root entirely.
        let outside = other.path().join("escape.txt");
        fs::write(&outside, b"").unwrap();

        let result = assert_within_root(root.path(), &outside);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_dotdot_traversal() {
        let dir = tempdir().unwrap();
        let escape = dir.path().join("..").join("escape.txt");

        let result = assert_within_root(dir.path(), &escape);
        assert!(result.is_err());
    }

    // ── parse_date_flexible ──────────────────────────────────────────────────

    #[test]
    fn parse_date_datetime_format() {
        let ts = parse_date_flexible(
            "2024-01-15 10:30:00",
            "[year]-[month]-[day] [hour]:[minute]:[second]",
        )
        .unwrap();
        assert!(ts > 0);
    }

    #[test]
    fn parse_date_date_only_format() {
        // format is date-only; PrimitiveDateTime parse fails, Date parse succeeds
        let ts = parse_date_flexible("2024-06-01", "[year]-[month]-[day]").unwrap();
        assert!(ts > 0);
    }

    #[test]
    fn parse_date_known_epoch_value() {
        // 1970-01-01 00:00:00 UTC → Unix timestamp 0
        let ts = parse_date_flexible(
            "1970-01-01 00:00:00",
            "[year]-[month]-[day] [hour]:[minute]:[second]",
        )
        .unwrap();
        assert_eq!(ts, 0);
    }

    #[test]
    fn parse_date_date_only_known_value() {
        // 1970-01-02 (midnight UTC) → 86400 seconds
        let ts = parse_date_flexible("1970-01-02", "[year]-[month]-[day]").unwrap();
        assert_eq!(ts, 86400);
    }

    #[test]
    fn parse_date_invalid_format_string() {
        // strftime-style format — not valid time crate syntax; date won't match
        assert!(parse_date_flexible("2024-01-15", "%Y-%m-%d").is_err());
    }

    #[test]
    fn parse_date_empty_date_returns_err() {
        assert!(parse_date_flexible("", "[year]-[month]-[day]").is_err());
    }

    #[test]
    fn parse_date_garbage_returns_err() {
        assert!(parse_date_flexible("not-a-date", "[year]-[month]-[day]").is_err());
    }

    #[test]
    fn parse_date_wrong_format_for_value_returns_err() {
        // format expects time component; date-only string doesn't satisfy it
        assert!(
            parse_date_flexible(
                "2024-01-15",
                "[year]-[month]-[day] [hour]:[minute]:[second]"
            )
            .is_err()
        );
    }
}
