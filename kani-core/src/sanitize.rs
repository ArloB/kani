//! Sanitization utilities for untrusted input.

/// Sanitizes a string to be used as a safe filename or directory name.
pub fn sanitize_filename(name: &str) -> String {
    let mut safe_name = String::with_capacity(name.len());

    for c in name.chars() {
        if c == '/' || c == '\\' {
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
