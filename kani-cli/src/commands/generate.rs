use crate::codegen;
use crate::error::CliError;
use crate::yaml::{schema::YamlExtension, validate::validate};
use std::path::{Path, PathBuf};

pub fn run(file: &str, force: bool, embedded_bytes: bool) -> Result<PathBuf, CliError> {
    let path = Path::new(file);
    let source = std::fs::read_to_string(path)?;

    let ext: YamlExtension = serde_yaml::from_str(&source)
        .map_err(|e| CliError::Other(format!("YAML parse error: {e}")))?;

    let validated = validate(&ext, &source, path).map_err(|errors| {
        for e in &errors {
            eprintln!("error: {e}");
        }
        CliError::Other(format!(
            "{} validation error(s) — generation aborted",
            errors.len()
        ))
    })?;

    let generated = codegen::generate(&validated, embedded_bytes);

    let workspace_root = path
        .parent()
        .unwrap_or(Path::new("."))
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists())
        .unwrap_or(Path::new("."));

    let out_dir = workspace_root
        .join("kani-extensions")
        .join(format!("kani-{}", generated.id));

    if out_dir.exists() && !force {
        return Err(CliError::Other(format!(
            "{} already exists — pass --force to overwrite",
            out_dir.display()
        )));
    }

    std::fs::create_dir_all(out_dir.join("src"))?;
    std::fs::write(out_dir.join("Cargo.toml"), &generated.cargo_toml)?;
    std::fs::write(out_dir.join("src").join("lib.rs"), &generated.lib_rs)?;

    if !generated.browser_scripts.is_empty() || !generated.pure_scripts.is_empty() {
        let scripts_dir = out_dir.join("src").join("scripts");
        std::fs::create_dir_all(&scripts_dir)?;
        for (name, src) in &generated.browser_scripts {
            std::fs::write(scripts_dir.join(format!("{name}.js")), src)?;
        }
        for (name, src) in &generated.pure_scripts {
            std::fs::write(scripts_dir.join(format!("{name}.rhai")), src)?;
        }
    }

    println!("Generated: {}", out_dir.display());
    Ok(out_dir)
}
