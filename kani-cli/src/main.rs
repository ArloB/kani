mod codegen;
mod commands;
mod dsl;
mod error;
mod yaml;

use clap::Parser;
use commands::Cli;

fn main() {
    let cli = Cli::parse();
    if let Err(e) = commands::run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}