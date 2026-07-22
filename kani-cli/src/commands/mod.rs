pub mod archive;
pub mod audit_tokens;
pub mod build;
pub mod css;
pub mod dsl_cmd;
pub mod generate;
pub mod icons;
pub mod keygen;
pub mod lint;
pub mod new;
pub mod publish;
pub mod quality;
pub mod repo;
pub mod rollback;
pub mod setup;
pub mod validate;

use crate::error::CliError;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kani-cli", about = "Kani extension development tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Scaffold a new extension (YAML by default, or a Rust/WASM crate with --rust)
    New {
        /// Extension name (e.g. my-source)
        name: String,
        /// Scaffold a Rust/WASM crate instead of a declarative YAML file
        #[arg(long)]
        rust: bool,
    },
    /// Validate a YAML extension file
    Validate {
        /// Path to the YAML file
        file: String,
    },
    /// Generate Rust source from a YAML extension file
    Generate {
        /// Path to the YAML file
        file: String,
        /// Overwrite an existing generated crate
        #[arg(long)]
        force: bool,
        /// Embed blueprint DSL as precomputed postcard bytes instead of using BlueprintBuilder
        #[arg(long)]
        embedded_bytes: bool,
    },
    /// Compile extension(s) to WASM
    Build {
        /// Extension crate name (e.g. kani-weebcentral)
        #[arg(conflicts_with_all = ["all", "dev"])]
        extension: Option<String>,
        /// Build all production extensions (excludes dev/test extensions)
        #[arg(long, conflicts_with = "dev")]
        all: bool,
        /// Build dev/test extensions only (kani-example, kani-test-abi); excluded from --all
        #[arg(long)]
        dev: bool,
        /// Override the version embedded in the WASM (e.g. 1.2.3)
        #[arg(long, value_name = "SEMVER")]
        set_version: Option<String>,
        /// Directory containing extension crates (default: kani-extensions)
        #[arg(long, value_name = "PATH")]
        ext_dir: Option<String>,
        /// Output directory for compiled .wasm files (default: wasm_sources)
        #[arg(long, value_name = "PATH")]
        out_dir: Option<String>,
        /// Build with debug info (larger binary, readable WASM backtraces)
        #[arg(long)]
        debug: bool,
    },
    /// Build the frontend CSS
    Css {
        /// Rebuild automatically on file changes
        #[arg(long, conflicts_with = "prod")]
        watch: bool,
        /// Minified production build
        #[arg(long, conflicts_with = "watch")]
        prod: bool,
    },
    /// Download required build tools and JS vendor files
    Setup {
        /// Download only the JS vendor files (Preact, htm)
        #[arg(long)]
        vendors: bool,
        /// Download only the Tailwind CSS standalone CLI
        #[arg(long)]
        tailwind: bool,
        /// Download only the esbuild binary
        #[arg(long)]
        esbuild: bool,
    },
    /// Generate PWA icon PNGs from static/icons/kani-mark.svg
    Icons,
    /// Parse a DSL expression and print the resulting Expr AST
    Dsl {
        /// DSL expression string
        expression: String,
        /// Path to a YAML extension file whose `scripts.pure` block defines user functions
        #[arg(long, value_name = "FILE")]
        scripts: Option<std::path::PathBuf>,
    },
    /// Run the workspace quality checks (clippy, machete, deny, fmt) in sequence
    Lint,
    /// Generate an Ed25519 signing keypair for extension authoring
    Keygen {
        /// Directory to write the keypair files into (default: current directory)
        #[arg(long, value_name = "PATH", default_value = ".")]
        out_dir: std::path::PathBuf,
        /// Base name for the generated files (e.g. "author" → author.pub + author.key)
        #[arg(long, default_value = "author")]
        name: String,
        /// Environment variable holding the passphrase for key encryption (not yet implemented)
        #[arg(long, value_name = "ENV_VAR")]
        passphrase_env: Option<String>,
    },
    /// Sign an extension and publish it to a local repository
    Publish {
        /// Path to the extension file (.yaml or .wasm) to publish
        file: std::path::PathBuf,
        /// Path to the author Ed25519 private key file (.key)
        #[arg(long, value_name = "PATH")]
        sign_key: std::path::PathBuf,
        /// Repository root directory (default: current directory)
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo_dir: std::path::PathBuf,
        /// Path to the maintainer private key for signing index.json
        #[arg(long, value_name = "PATH")]
        repo_sign_key: Option<std::path::PathBuf>,
        /// Minimum Kani version required to install this extension
        #[arg(long, value_name = "SEMVER")]
        min_kani_version: Option<String>,
    },
    /// Manage a local extension repository
    #[command(subcommand)]
    Repo(RepoCommand),
    /// REPL: inspect, explain, test, replay, or record a YAML extension
    #[command(subcommand)]
    Repl(ReplCommand),
    /// Scan static/js for hard-coded colour literals and report violations
    AuditTokens {
        /// Directory to scan (default: static/js)
        #[arg(long, value_name = "PATH", default_value = "static/js")]
        dir: std::path::PathBuf,
        /// Exit non-zero if violations exceed the baseline (for CI use)
        #[arg(long)]
        check: bool,
        /// With --check, tolerate up to N existing violations; fail only when exceeded
        #[arg(long, default_value_t = 0)]
        max: usize,
    },
    /// Re-hash every file a Kani archive export claims, without needing Kani
    ArchiveVerify {
        /// Path to the exported `kani-archive` directory
        #[arg(value_name = "ARCHIVE_DIR")]
        path: std::path::PathBuf,
    },
    /// Print the quality score and per-page dimensions for a CBZ
    Quality {
        /// Path to a .cbz file
        #[arg(value_name = "CBZ")]
        path: std::path::PathBuf,
    },
    /// Show what a header probe learns from an image's first few kilobytes
    Probe {
        /// Path to an image file
        #[arg(value_name = "IMAGE")]
        path: std::path::PathBuf,
    },
    /// Compare two CBZs page by page with perceptual hashes
    PhashCompare {
        /// First .cbz
        #[arg(value_name = "A")]
        a: std::path::PathBuf,
        /// Second .cbz
        #[arg(value_name = "B")]
        b: std::path::PathBuf,
    },
    /// Print the manifest computed from a CBZ on disk
    Manifest {
        /// Path to a .cbz file
        #[arg(value_name = "CBZ")]
        path: std::path::PathBuf,
    },
    /// Verify a backup archive can be restored onto this build
    Rollback {
        /// Path to a backup .zip produced by Kani
        #[arg(value_name = "BACKUP_ZIP")]
        path: std::path::PathBuf,
    },
}

#[derive(Subcommand)]
pub enum RepoCommand {
    /// Initialise a new repository directory with an empty index.json
    Init {
        /// Path to the maintainer public key file (.pub)
        #[arg(long, value_name = "PATH")]
        maintainer_key: std::path::PathBuf,
        /// Human-readable repository name
        #[arg(long)]
        name: String,
        /// Directory to initialise as the repository root (default: current directory)
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo_dir: std::path::PathBuf,
    },
    /// Add an already-signed extension artifact to the repository index
    Add {
        /// Path to the signed extension artifact (.yaml or .wasm); its .sig must be alongside
        artifact: std::path::PathBuf,
        /// Path to the author public key file (.pub) used to verify the artifact signature
        #[arg(long, value_name = "PATH")]
        author_key: std::path::PathBuf,
        /// Repository root directory (default: current directory)
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo_dir: std::path::PathBuf,
        /// Minimum Kani version required to install this extension
        #[arg(long, value_name = "SEMVER")]
        min_kani_version: Option<String>,
        /// Path to the maintainer private key for re-signing index.json after update
        #[arg(long, value_name = "PATH")]
        repo_sign_key: Option<std::path::PathBuf>,
    },
    /// List extensions in a repository's index.json
    List {
        /// Repository root directory (default: current directory)
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo_dir: std::path::PathBuf,
    },
    /// Print the fingerprint of a public key file
    ShowFingerprint {
        /// Path to the public key file (.pub)
        #[arg(long, value_name = "PATH")]
        key: std::path::PathBuf,
    },
    /// Verify all extension signatures and SHA-256 digests in a local repository
    Verify {
        /// Repository root directory (default: current directory)
        #[arg(long, value_name = "PATH", default_value = ".")]
        repo_dir: std::path::PathBuf,
        /// Path to maintainer public key to verify index.json (defaults to key in index.json)
        #[arg(long, value_name = "PATH")]
        repo_key: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum ReplCommand {
    /// Show a structured summary of a YAML extension (endpoints, fields, filters)
    Inspect {
        /// Path to the YAML extension file
        file: String,
    },
    /// Parse a DSL expression and print its evaluation trace as an indented tree
    Explain {
        /// DSL expression to trace (e.g. 'first("a").attr("href").split("/").at(-1)')
        expression: String,
    },
    /// Run an endpoint against a HAR fixture and assert the row count
    Test {
        /// Path to the YAML extension file
        file: String,
        /// Path to the HAR fixture file
        har: String,
        /// Endpoint name: popular, search, manga_details, chapter_list, pages
        endpoint: String,
        /// Expected number of rows
        expected_count: usize,
    },
    /// Run an endpoint against a HAR fixture and diff the output against an expected JSON file
    Replay {
        /// Path to the YAML extension file
        file: String,
        /// Path to the HAR fixture file
        har: String,
        /// Endpoint name: popular, search, manga_details, chapter_list, pages
        endpoint: String,
        /// Path to the expected JSON output file
        expected: String,
    },
    /// Make a live HTTP request to an endpoint and save the response as a HAR file
    Record {
        /// Path to the YAML extension file
        file: String,
        /// Endpoint name: popular, search, manga_details, chapter_list, pages
        endpoint: String,
        /// Arguments for route placeholders and query params (e.g. manga_id=abc page=1)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
        /// Output HAR file path
        #[arg(long, short, default_value = "recorded.har")]
        output: String,
    },
}

pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::New { name, rust } => new::run(&name, rust),
        Command::Validate { file } => validate::run(&file),
        Command::Generate {
            file,
            force,
            embedded_bytes,
        } => generate::run(&file, force, embedded_bytes).map(|_| ()),
        Command::Build {
            extension,
            all,
            dev,
            set_version,
            ext_dir,
            out_dir,
            debug,
        } => build::run(
            extension.as_deref(),
            all,
            dev,
            set_version.as_deref(),
            ext_dir.as_deref(),
            out_dir.as_deref(),
            debug,
        ),
        Command::Css { watch, prod } => css::run(watch, prod),
        Command::Setup {
            vendors,
            tailwind,
            esbuild,
        } => setup::run(vendors, tailwind, esbuild),
        Command::ArchiveVerify { path } => archive::verify(&path),
        Command::Quality { path } => quality::score(&path),
        Command::Probe { path } => quality::probe(&path),
        Command::PhashCompare { a, b } => quality::phash_compare(&a, &b),
        Command::Manifest { path } => archive::manifest(&path),
        Command::Icons => icons::run(),
        Command::Dsl {
            expression,
            scripts,
        } => dsl_cmd::run(&expression, scripts.as_deref()),
        Command::Lint => lint::run(),
        Command::Keygen {
            out_dir,
            name,
            passphrase_env,
        } => keygen::run(&out_dir, &name, passphrase_env.as_deref()),
        Command::Publish {
            file,
            sign_key,
            repo_dir,
            repo_sign_key,
            min_kani_version,
        } => publish::run(
            &file,
            &sign_key,
            &repo_dir,
            repo_sign_key.as_deref(),
            min_kani_version.as_deref(),
        ),
        Command::Repo(repo_cmd) => match repo_cmd {
            RepoCommand::Init {
                maintainer_key,
                name,
                repo_dir,
            } => repo::run_init(&repo_dir, &name, &maintainer_key),
            RepoCommand::Add {
                artifact,
                author_key,
                repo_dir,
                min_kani_version,
                repo_sign_key,
            } => repo::run_add(
                &artifact,
                &author_key,
                &repo_dir,
                min_kani_version.as_deref(),
                repo_sign_key.as_deref(),
            ),
            RepoCommand::List { repo_dir } => repo::run_list(&repo_dir),
            RepoCommand::ShowFingerprint { key } => repo::run_show_fingerprint(&key),
            RepoCommand::Verify { repo_dir, repo_key } => {
                repo::run_verify(&repo_dir, repo_key.as_deref())
            }
        },
        Command::AuditTokens { dir, check, max } => audit_tokens::run(&dir, check, max),
        Command::Rollback { path } => rollback::run(&path),
        Command::Repl(repl_cmd) => match repl_cmd {
            ReplCommand::Inspect { file } => crate::repl::inspect::run(&file),
            ReplCommand::Explain { expression } => crate::repl::explain::run(&expression),
            ReplCommand::Test {
                file,
                har,
                endpoint,
                expected_count,
            } => crate::repl::test_cmd::run_test(&file, &har, &endpoint, expected_count),
            ReplCommand::Replay {
                file,
                har,
                endpoint,
                expected,
            } => crate::repl::test_cmd::run_replay(&file, &har, &endpoint, &expected),
            ReplCommand::Record {
                file,
                endpoint,
                args,
                output,
            } => crate::repl::record::run(&file, &endpoint, &args, &output),
        },
    }
}
