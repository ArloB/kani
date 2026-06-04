use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../migrations");

    let is_release = std::env::var("PROFILE").unwrap_or_default() == "release";

    build_css(is_release);

    if is_release {
        build_js();
    }
}

fn build_css(minify: bool) {
    // Rerun this step whenever the CSS source files or any JS component changes
    // (Tailwind scans JS for utility class names to include in the output).
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
        // The binary is optional during `cargo build` — the Dockerfile handles
        // CSS compilation in its own stage.  Developers who want the CSS
        // rebuilt automatically should run `kani-cli setup --tailwind` first.
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
        ])
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", binary.display()));

    if !status.success() {
        panic!("esbuild JS bundle failed (exit {})", status);
    }
}
