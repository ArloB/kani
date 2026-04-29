pub mod build;
pub mod css;
pub mod dsl_cmd;
pub mod generate;
pub mod new;
pub mod setup;
pub mod validate;

use clap::{Parser, Subcommand};
use crate::error::CliError;

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
        #[arg(conflicts_with = "all")]
        extension: Option<String>,
        /// Build all extensions
        #[arg(long)]
        all: bool,
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
    /// Parse a DSL expression and print the resulting Expr AST
    Dsl {
        /// DSL expression string
        expression: String,
    },
}

pub fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::New      { name }              => new::run(&name),
        Command::Validate { file }              => validate::run(&file),
        Command::Generate { file, force, embedded_bytes } => generate::run(&file, force, embedded_bytes).map(|_| ()),
        Command::Build    { extension, all }    => build::run(extension.as_deref(), all),
        Command::Css      { watch, prod }       => css::run(watch, prod),
        Command::Setup    { vendors, tailwind, esbuild }
                                                => setup::run(vendors, tailwind, esbuild),
        Command::Dsl      { expression }        => dsl_cmd::run(&expression),
    }
}