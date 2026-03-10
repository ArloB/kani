//! Sanitization utilities for untrusted input.
use std::path::{Path, PathBuf};
use crate::error::{Error, Result};

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
    let (canonical_base, suffix) = if target.exists() {
        (target.canonicalize()?, PathBuf::new())
    } else {
        let parent = target
            .parent()
            .ok_or_else(|| Error::Internal("path has no parent".to_string()))?;
        let canonical_parent = parent.canonicalize()
            .map_err(|_| Error::Internal("parent directory does not exist".to_string()))?;
        let file_name = target
            .file_name()
            .ok_or_else(|| Error::Internal("path has no filename".to_string()))?;
        (canonical_parent, PathBuf::from(file_name))
    };

    let canonical_root = root.canonicalize()?;
    let full = canonical_base.join(&suffix);

    if !full.starts_with(&canonical_root) {
        return Err(Error::Internal(format!(
            "path traversal detected: {:?} escapes root {:?}",
            full, canonical_root
        )));
    }

    Ok(full)
}