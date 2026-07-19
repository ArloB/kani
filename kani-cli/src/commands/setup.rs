use crate::error::CliError;
use base64::Engine;
use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub fn run(vendors: bool, tailwind: bool, esbuild: bool) -> Result<(), CliError> {
    let run_all = !vendors && !tailwind && !esbuild;
    let client = Client::new();

    if run_all || vendors {
        fetch_vendors(&client)?;
        fetch_fonts(&client)?;
    }
    if run_all || tailwind {
        fetch_tailwind(&client)?;
    }
    if run_all || esbuild {
        fetch_esbuild(&client)?;
    }
    if run_all {
        super::icons::run()?;
        setup_git_hooks()?;
    }

    Ok(())
}

fn setup_git_hooks() -> Result<(), CliError> {
    let status = std::process::Command::new("git")
        .args(["config", "core.hooksPath", ".githooks"])
        .status();

    match status {
        Ok(s) if s.success() => println!("Git hooks path configured (.githooks)"),
        Ok(_) => eprintln!("warning: git config failed — not inside a git repository?"),
        Err(_) => eprintln!("warning: git not found, skipping hooks configuration"),
    }

    let hooks_dir = Path::new(".githooks");
    if !hooks_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(hooks_dir)? {
        let path = entry?.path();
        if path.is_file() {
            make_executable(&path)?;
        }
    }

    Ok(())
}

fn fetch_vendors(client: &Client) -> Result<(), CliError> {
    let vendor_dir = Path::new("static/js/vendor");
    fs::create_dir_all(vendor_dir)?;

    let files = [
        (
            "https://unpkg.com/preact@10.26.4/dist/preact.module.js",
            "preact.module.js",
        ),
        (
            "https://unpkg.com/preact@10.26.4/hooks/dist/hooks.module.js",
            "preact-hooks.module.js",
        ),
        (
            "https://unpkg.com/htm@3.1.1/dist/htm.module.js",
            "htm.module.js",
        ),
        (
            "https://unpkg.com/@preact/signals-core@1.8.0/dist/signals-core.module.js",
            "signals-core.module.js",
        ),
        (
            "https://unpkg.com/@preact/signals@2.0.1/dist/signals.module.js",
            "signals.module.js",
        ),
        (
            "https://unpkg.com/preact@10.26.4/compat/dist/compat.module.js",
            "compat.module.js",
        ),
        (
            "https://unpkg.com/preact@10.26.4/debug/dist/debug.module.js",
            "debug.module.js",
        ),
        (
            "https://unpkg.com/preact@10.26.4/devtools/dist/devtools.module.js",
            "devtools.module.js",
        ),
        (
            "https://cdn.jsdelivr.net/npm/chart.js@4.4.9/dist/chart.umd.min.js",
            "chart.umd.min.js",
        ),
    ];

    let mut hashes = serde_json::Map::new();

    for (url, filename) in &files {
        println!("Downloading {filename}...");
        let bytes = client.get(*url).send()?.bytes()?;
        fs::write(vendor_dir.join(filename), &bytes)?;

        // Compute SHA-256 for subresource integrity tracking.
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();
        let b64 = base64::engine::general_purpose::STANDARD.encode(digest);
        hashes.insert(
            filename.to_string(),
            serde_json::Value::String(format!("sha256-{b64}")),
        );
    }

    // Write sri.json so CI and build.rs can verify integrity.
    let sri_path = vendor_dir.join("sri.json");
    let sri_content = serde_json::to_string_pretty(&hashes)
        .map_err(|e| CliError::Other(format!("sri.json serialise: {e}")))?;
    fs::write(&sri_path, sri_content)?;
    println!("SRI hashes written to {}", sri_path.display());

    println!("Vendor files saved to {}", vendor_dir.display());
    Ok(())
}

/// Self-hosts the DM Sans / DM Mono webfonts used by `static/css/app.css`.
///
/// Google's CSS2 endpoint serves different `@font-face` rules (and file
/// formats) depending on the requesting User-Agent; requesting it with a
/// modern-Chrome UA gets woff2 rules. Fetches that stylesheet, downloads
/// every referenced font file into `static/fonts/`, rewrites the `url(...)`
/// references to local relative paths, and writes the result as
/// `static/fonts/fonts.css` — `app.css` imports that file locally instead of
/// hitting fonts.googleapis.com at request time.
fn fetch_fonts(client: &Client) -> Result<(), CliError> {
    let fonts_dir = Path::new("static/fonts");
    fs::create_dir_all(fonts_dir)?;

    let css_url = "https://fonts.googleapis.com/css2?family=DM+Mono:ital,wght@0,300;0,400;0,500;1,300;1,400;1,500&family=DM+Sans:ital,opsz,wght@0,9..40,300;0,9..40,400;0,9..40,500;0,9..40,600;1,9..40,300;1,9..40,400&family=Zen+Kaku+Gothic+New:wght@700;900&display=swap";

    println!("Fetching font stylesheet...");
    let css_text = client
        .get(css_url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        )
        .send()?
        .text()?;

    let url_re = regex::Regex::new(r"url\((https://fonts\.gstatic\.com/[^)]+)\)")
        .map_err(|e| CliError::Other(format!("font URL regex: {e}")))?;

    let mut localized_css = css_text.clone();
    for cap in url_re.captures_iter(&css_text) {
        let remote_url = &cap[1];
        let filename = remote_url
            .rsplit('/')
            .next()
            .ok_or_else(|| CliError::Other(format!("unexpected font URL: {remote_url}")))?;

        println!("Downloading font {filename}...");
        let bytes = client.get(remote_url).send()?.bytes()?;
        fs::write(fonts_dir.join(filename), &bytes)?;

        localized_css = localized_css.replace(remote_url, &format!("./{filename}"));
    }

    let out_path = fonts_dir.join("fonts.css");
    fs::write(&out_path, localized_css)?;
    println!("Font files + stylesheet saved to {}", fonts_dir.display());
    Ok(())
}

fn fetch_tailwind(client: &Client) -> Result<(), CliError> {
    let platform = tailwind_platform()?;
    let url = format!(
        "https://github.com/tailwindlabs/tailwindcss/releases/latest/download/tailwindcss-{platform}"
    );
    let out = if cfg!(windows) {
        "tools/tailwindcss.exe"
    } else {
        "tools/tailwindcss"
    };

    fs::create_dir_all("tools")?;
    println!("Downloading Tailwind CSS CLI ({platform})...");

    let bytes = client.get(&url).send()?.bytes()?;
    fs::write(out, &bytes)?;
    make_executable(Path::new(out))?;

    println!("Saved to {out}");
    Ok(())
}

fn fetch_esbuild(client: &Client) -> Result<(), CliError> {
    let (npm_pkg, bin_path) = esbuild_platform()?;
    let out = if cfg!(windows) {
        "tools/esbuild.exe"
    } else {
        "tools/esbuild"
    };

    let registry_url = format!("https://registry.npmjs.org/@esbuild/{npm_pkg}/latest");
    let json: serde_json::Value = client.get(&registry_url).send()?.json()?;

    let tarball_url = json["dist"]["tarball"]
        .as_str()
        .ok_or_else(|| CliError::Other("npm registry response missing dist.tarball".into()))?
        .to_owned();

    fs::create_dir_all("tools")?;
    println!("Downloading esbuild ({npm_pkg})...");

    let bytes = client.get(&tarball_url).send()?.bytes()?;

    let cursor = std::io::Cursor::new(&bytes);
    let gz = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);

    let mut found = false;
    for entry in archive
        .entries()
        .map_err(|e| CliError::Other(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| CliError::Other(e.to_string()))?;
        let path = entry.path().map_err(|e| CliError::Other(e.to_string()))?;
        if path.to_string_lossy() == bin_path {
            entry
                .unpack(out)
                .map_err(|e| CliError::Other(e.to_string()))?;
            found = true;
            break;
        }
    }

    if !found {
        return Err(CliError::Other(format!(
            "esbuild binary not found at `{bin_path}` inside tarball"
        )));
    }

    make_executable(Path::new(out))?;
    println!("Saved to {out}");
    Ok(())
}

fn tailwind_platform() -> Result<&'static str, CliError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("linux-x64"),
        ("linux", "aarch64") => Ok("linux-arm64"),
        ("macos", "x86_64") => Ok("macos-x64"),
        ("macos", "aarch64") => Ok("macos-arm64"),
        ("windows", "x86_64") => Ok("windows-x64.exe"),
        (os, arch) => Err(CliError::Other(format!(
            "unsupported platform: {os}/{arch}"
        ))),
    }
}

fn esbuild_platform() -> Result<(&'static str, &'static str), CliError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok(("linux-x64", "package/bin/esbuild")),
        ("linux", "aarch64") => Ok(("linux-arm64", "package/bin/esbuild")),
        ("macos", "x86_64") => Ok(("darwin-x64", "package/bin/esbuild")),
        ("macos", "aarch64") => Ok(("darwin-arm64", "package/bin/esbuild")),
        ("windows", "x86_64") => Ok(("win32-x64", "package/esbuild.exe")),
        (os, arch) => Err(CliError::Other(format!(
            "unsupported platform: {os}/{arch}"
        ))),
    }
}

#[allow(unused_variables)]
fn make_executable(path: &Path) -> Result<(), CliError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
