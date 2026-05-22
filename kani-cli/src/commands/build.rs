use crate::error::CliError;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DEV_EXTENSIONS: &[&str] = &["kani-example", "kani-test-abi"];

pub fn run(
    extension: Option<&str>,
    all: bool,
    dev: bool,
    set_version: Option<&str>,
    ext_dir: Option<&str>,
    out_dir: Option<&str>,
    debug: bool,
) -> Result<(), CliError> {
    if extension.is_none() && !all && !dev {
        return Err(CliError::Other(
            "specify an extension name, --all, or --dev".into(),
        ));
    }

    let extensions_dir = PathBuf::from(ext_dir.unwrap_or("kani-extensions"));
    if !extensions_dir.exists() {
        return Err(CliError::Other(format!(
            "{} not found — run kani-cli from the workspace root",
            extensions_dir.display()
        )));
    }

    let out_dir = PathBuf::from(out_dir.unwrap_or("wasm_sources"));

    let dirs: Vec<String> = fs::read_dir(&extensions_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    let to_build: Vec<String> = if dev {
        dirs.into_iter()
            .filter(|name| DEV_EXTENSIONS.contains(&name.as_str()))
            .collect()
    } else {
        let available: Vec<String> = dirs
            .into_iter()
            .filter(|name| !DEV_EXTENSIONS.contains(&name.as_str()))
            .collect();

        if all {
            available
        } else {
            let name = extension.expect("checked above");
            if !available.contains(&name.to_owned()) {
                return Err(CliError::ExtensionNotFound(name.to_owned()));
            }
            vec![name.to_owned()]
        }
    };

    for name in &to_build {
        build_one(name, set_version, &out_dir, debug)?;
    }

    Ok(())
}

fn build_one(
    name: &str,
    set_version: Option<&str>,
    out_dir: &Path,
    debug: bool,
) -> Result<(), CliError> {
    let profile = if debug { "wasm-debug" } else { "wasm-release" };
    println!("── Building {name} [{profile}]");

    let mut cargo_args = vec![
        "build",
        "--target",
        "wasm32-unknown-unknown",
        "--profile",
        profile,
        "-p",
        name,
    ];
    // Temporary storage so the string outlives the vec push
    let version_config;
    if let Some(ver) = set_version {
        version_config = format!("package.version=\"{ver}\"");
        cargo_args.extend(["--config", &version_config]);
    }

    let status = Command::new("cargo").args(&cargo_args).status()?;
    if !status.success() {
        return Err(CliError::BuildFailed(name.to_owned()));
    }

    let metadata_output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()?;

    let mut ext_id = name.to_string();

    if metadata_output.status.success()
        && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&metadata_output.stdout)
        && let Some(pkg) = json["packages"]
            .as_array()
            .and_then(|pkgs| pkgs.iter().find(|p| p["name"] == name))
        && let Some(id) = pkg["metadata"]["id"]
            .as_str()
            .or_else(|| pkg["metadata"]["kani"]["id"].as_str())
    {
        ext_id = id.to_string();
    }

    fs::create_dir_all(out_dir)?;

    let src = PathBuf::from(format!(
        "target/wasm32-unknown-unknown/{}/{}.wasm",
        profile,
        name.replace('-', "_"),
    ));

    let dest = out_dir.join(format!("{ext_id}.wasm"));
    fs::copy(&src, &dest)?;

    if is_available("wasm-opt") {
        println!("   running wasm-opt");
        let tmp = tmp_path(&dest);
        let mut wasm_opt_args = vec![
            "-Oz",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--enable-sign-ext",
        ];
        if debug {
            wasm_opt_args.push("--debuginfo");
        }
        let status = Command::new("wasm-opt")
            .args(&wasm_opt_args)
            .args(["-o"])
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
