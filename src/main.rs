//! Point d'entrée du G27 Mode Switcher.

mod cli;

use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_logging(cli.verbose);
    cli.run()
}

/// Initialise le logging structuré.
///
/// La variable d'environnement `RUST_LOG` reste prioritaire ; sinon le niveau
/// par défaut dépend du nombre d'occurrences de `--verbose` (`info`, puis
/// `debug`, puis `trace`).
fn init_logging(verbose: u8) {
    let default_level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
