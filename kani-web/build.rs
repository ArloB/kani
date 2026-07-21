use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../migrations");

    let git_sha = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=GIT_SHA={git_sha}");

    let is_release = std::env::var("PROFILE").unwrap_or_default() == "release";

    copy_changelog();
    build_css(is_release);

    if is_release {
        build_js();
    }
}

fn copy_changelog() {
    println!("cargo:rerun-if-changed=../CHANGELOG.md");
    if let Err(e) = std::fs::copy("../CHANGELOG.md", "../static/changelog.md") {
        println!("cargo:warning=failed to copy CHANGELOG.md into static/: {e}");
    }
}

fn build_css(minify: bool) {
    println!("cargo:rerun-if-changed=../static/css/app.css");
    println!("cargo:rerun-if-changed=../static/js");

    let esbuild_path = if cfg!(windows) {
        "../tools/tailwindcss.exe"
    } else {
        "../tools/tailwindcss"
    };
    println!("cargo:rerun-if-changed={}", esbuild_path);

    let binary = Path::new(esbuild_path).to_path_buf();

    if !binary.exists() {
        println!(
            "cargo:warning=Tailwind CLI not found at {}; skipping CSS build. \
             Run `kani-cli setup --tailwind` to download it.",
            binary.display()
        );
        return;
    }

    let mut cmd = std::process::Command::new(&binary);
    cmd.args([
        "-i",
        "../static/css/app.css",
        "-o",
        "../static/css/main.css",
    ]);
    if minify {
        cmd.arg("--minify");
    }

    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", binary.display()));

    if !status.success() {
        panic!("tailwindcss CSS build failed (exit {})", status);
    }
}

fn build_js() {
    println!("cargo:rerun-if-changed=../static/js");

    let esbuild_path = if cfg!(windows) {
        "../tools/esbuild.exe"
    } else {
        "../tools/esbuild"
    };
    println!("cargo:rerun-if-changed={}", esbuild_path);

    let binary = Path::new(esbuild_path).to_path_buf();

    if !binary.exists() {
        panic!(
            "esbuild not found at {}. \n\
             Run `kani-cli setup --esbuild` to download it.",
            binary.display()
        );
    }

    std::fs::create_dir_all("../static/js/dist").expect("failed to create static/js/dist");

    let status = std::process::Command::new(&binary)
        .args([
            "../static/js/app.js",
            "--bundle",
            "--splitting",
            "--format=esm",
            "--minify",
            "--platform=browser",
            "--outdir=../static/js/dist",
            "--alias:preact=../static/js/vendor/preact.module.js",
            "--alias:preact/hooks=../static/js/vendor/preact-hooks.module.js",
            "--alias:htm=../static/js/vendor/htm.module.js",
            "--alias:@preact/signals-core=../static/js/vendor/signals-core.module.js",
            "--alias:@preact/signals=../static/js/vendor/signals.module.js",
            "--alias:preact/compat=../static/js/vendor/compat.module.js",
        ])
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", binary.display()));

    if !status.success() {
        panic!("esbuild JS bundle failed (exit {})", status);
    }
}
