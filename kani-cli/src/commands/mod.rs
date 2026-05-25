pub mod build;
pub mod css;
pub mod dsl_cmd;
pub mod generate;
pub mod icons;
pub mod new;
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
    /// Scaffold a new YAML extension
    New {
        /// Extension name (e.g. my-source)
        name: String,
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
    },
}

pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::New { name } => new::run(&name),
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
        Command::Icons => icons::run(),
        Command::Dsl { expression } => dsl_cmd::run(&expression),
    }
}
