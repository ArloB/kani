use crate::error::CliError;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Violation {
    pub file: PathBuf,
    pub line: usize,
    pub literal: String,
}

struct Patterns {
    hex: Regex,
    rgb: Regex,
    tailwind: Regex,
}

impl Patterns {
    fn new() -> Self {
        let palette = [
            "gray", "zinc", "slate", "neutral", "stone", "red", "orange", "amber", "yellow",
            "lime", "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet",
            "purple", "fuchsia", "pink", "rose", "white", "black",
        ]
        .join("|");

        Self {
            hex: Regex::new(r"#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b")
                .expect("valid hex regex"),
            rgb: Regex::new(r"rgb[a]?\s*\(|hsl[a]?\s*\(").expect("valid rgb regex"),
            tailwind: Regex::new(&format!(
                r"\b(?:text|bg|border|ring|fill|stroke|from|to|via|shadow|accent)-(?:{})(?:-\d+)?",
                palette
            ))
            .expect("valid tailwind regex"),
        }
    }

    fn matches(&self, line: &str) -> Vec<String> {
        let mut found = Vec::new();
        for m in self.hex.find_iter(line) {
            found.push(m.as_str().to_owned());
        }
        for m in self.rgb.find_iter(line) {
            found.push(m.as_str().trim_end_matches(['(', ' ']).to_owned());
        }
        for m in self.tailwind.find_iter(line) {
            found.push(m.as_str().to_owned());
        }
        found
    }
}

pub fn run(dir: &Path, check: bool, max: usize) -> Result<(), CliError> {
    let violations = scan(dir)?;
    for v in &violations {
        println!("{}:{}: {}", v.file.display(), v.line, v.literal);
    }
    let count = violations.len();
    if count == 0 {
        println!("No hard-coded colour literals found in {}.", dir.display());
        return Ok(());
    }
    println!("\n{count} violation(s) found (baseline max: {max}).");
    if check && count > max {
        return Err(CliError::Other(format!(
            "{count} hard-coded colour violation(s) exceeds baseline of {max} — \
             migrate new literals to design tokens (or lower the baseline as you clean up)"
        )));
    }
    Ok(())
}

pub fn scan(dir: &Path) -> Result<Vec<Violation>, CliError> {
    let patterns = Patterns::new();
    let mut violations = Vec::new();
    collect(dir, &patterns, &mut violations)?;
    Ok(violations)
}

fn collect(dir: &Path, patterns: &Patterns, out: &mut Vec<Violation>) -> Result<(), CliError> {
    let entries = fs::read_dir(dir)
        .map_err(|e| CliError::Other(format!("cannot read {}: {e}", dir.display())))?;
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let skip = matches!(
            name.to_string_lossy().as_ref(),
            "dist" | "vendor" | "node_modules"
        );
        if skip {
            continue;
        }
        if path.is_dir() {
            collect(&path, patterns, out)?;
        } else if path.extension().is_some_and(|ext| ext == "js") {
            scan_js_file(&path, patterns, out)?;
        }
    }
    Ok(())
}

fn scan_js_file(
    path: &Path,
    patterns: &Patterns,
    out: &mut Vec<Violation>,
) -> Result<(), CliError> {
    let content = fs::read_to_string(path)
        .map_err(|e| CliError::Other(format!("cannot read {}: {e}", path.display())))?;

    // A file-level `audit-ignore-file` directive skips the whole file — for
    // modules that are definitionally colour sources (e.g. the theme palette).
    if content.contains("audit-ignore-file") {
        return Ok(());
    }

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        // A line-level `audit-ignore` directive opts that line out — for
        // intentional literals (the pure-black reader, scrims, contrast text)
        // that have no semantic-token equivalent.
        if line.contains("audit-ignore") {
            continue;
        }
        for literal in patterns.matches(line) {
            out.push(Violation {
                file: path.to_owned(),
                line: line_num,
                literal,
            });
        }
    }
    Ok(())
}
