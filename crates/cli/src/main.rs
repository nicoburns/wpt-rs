use clap::{Parser, Subcommand};

mod commands;
mod compression;

// Use jemalloc as the allocator
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
static VERSION: &str = env!("CARGO_PKG_VERSION");

/// Dioxus: build web, desktop, and mobile apps with a single codebase.
#[derive(Parser)]
#[clap(name = "wpt", version = VERSION)]
// #[clap(styles = CARGO_STYLING)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) action: Commands,
    // #[command(flatten)]
    // pub(crate) verbosity: Verbosity,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Calculate a score summary from a directory of reports
    #[clap(name = "calc-scores")]
    CalcScores(commands::CalcScores),
}

fn main() {
    let args = Cli::parse();
    match args.action {
        Commands::CalcScores(cmd) => cmd.run(),
    };
}
