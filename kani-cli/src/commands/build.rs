use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::error::CliError;

pub fn run(extension: Option<&str>, all: bool) -> Result<(), CliError> {
    if extension.is_none() && !all {
        return Err(CliError::Other(
            "specify an extension name or pass --all".into(),
        ));
    }

    let extensions_dir = Path::new("kani-extensions");
    if !extensions_dir.exists() {
        return Err(CliError::Other(
            "kani-extensions/ not found — run kani-cli from the workspace root".into(),
        ));
    }

    let available: Vec<String> = fs::read_dir(extensions_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "kani-example")
        .collect();

    let to_build: Vec<String> = if all {
        available
    } else {
        let name = extension.expect("checked above");
        if !available.contains(&name.to_owned()) {
            return Err(CliError::ExtensionNotFound(name.to_owned()));
        }
        vec![name.to_owned()]
    };

    for name in &to_build {
        build_one(name)?;
    }

    Ok(())
}

fn build_one(name: &str) -> Result<(), CliError> {
    println!("── Building {name}");

    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-unknown-unknown", "--profile", "wasm-release", "-p", name])
        .status()?;
    if !status.success() {
        return Err(CliError::BuildFailed(name.to_owned()));
    }

    let wasm_sources = Path::new("wasm_sources");
    fs::create_dir_all(wasm_sources)?;

    let src = PathBuf::from(format!(
        "target/wasm32-unknown-unknown/wasm-release/{}.wasm",
        name.replace('-', "_"),
    ));
    let dest = wasm_sources.join(format!("{name}.wasm"));
    fs::copy(&src, &dest)?;

    if is_available("wasm-opt") {
        println!("   running wasm-opt");
        let tmp = tmp_path(&dest);
        let status = Command::new("wasm-opt")
            .args(["-Oz", "--enable-bulk-memory", "-o"])
            .arg(&tmp)
            .arg(&dest)
            .status()?;
        if status.success() {
            fs::rename(&tmp, &dest)?;
        } else {
            eprintln!("   warning: wasm-opt failed, keeping original");
            if tmp.exists() {
                fs::remove_file(&tmp)?;
            }
        }
    } else {
        println!("   wasm-opt not found, skipping size optimisation");
    }

    if !is_available("wasm-tools") {
        return Err(CliError::WasmToolsMissing);
    }
    println!("   converting to WASM component");
    let tmp = tmp_path(&dest);
    let status = Command::new("wasm-tools")
        .args(["component", "new"])
        .arg(&dest)
        .arg("-o")
        .arg(&tmp)
        .status()?;
    if !status.success() {
        return Err(CliError::BuildFailed(format!(
            "{name}: wasm-tools component conversion failed"
        )));
    }
    fs::rename(&tmp, &dest)?;

    let size_kb = fs::metadata(&dest)?.len() as f64 / 1024.0;
    println!("   done — {size_kb:.2} KB\n");

    Ok(())
}

fn tmp_path(path: &Path) -> PathBuf {
    let file_name = path.file_name().unwrap().to_string_lossy();
    path.with_file_name(format!("{file_name}.tmp"))
}

fn is_available(tool: &str) -> bool {
    match Command::new(tool).arg("--version").output() {
        Ok(_) => true,
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}
