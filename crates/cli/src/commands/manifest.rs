use std::path::PathBuf;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};

#[derive(Clone, Debug, Parser)]
#[clap(name = "manifest")]
pub struct Manifest {
    #[command(subcommand)]
    action: ManifestCommand,
}

#[derive(Clone, Debug, Subcommand)]
enum ManifestCommand {
    /// Build a MANIFEST.json from scratch, ignoring any existing manifest
    Build(ManifestArgs),

    /// Update an existing MANIFEST.json, reparsing only changed files (builds one if missing)
    Update(ManifestArgs),
}

#[derive(Clone, Debug, Args)]
struct ManifestArgs {
    /// Root of the web-platform-tests checkout
    #[clap(long, default_value = ".")]
    tests_root: PathBuf,

    /// Path to the manifest file [default: <TESTS_ROOT>/MANIFEST.json]
    #[clap(long, short)]
    path: Option<PathBuf>,

    /// Base URL of the tests
    #[clap(long, default_value = "/")]
    url_base: String,
}

impl Manifest {
    pub fn run(self) {
        let (args, rebuild) = match self.action {
            ManifestCommand::Build(args) => (args, true),
            ManifestCommand::Update(args) => (args, false),
        };
        let start = Instant::now();
        let path = args
            .path
            .unwrap_or_else(|| args.tests_root.join("MANIFEST.json"));
        match wptmanifest::Manifest::load_and_update(
            &args.tests_root,
            &path,
            &args.url_base,
            rebuild,
        ) {
            Ok((_, stats)) => println!(
                "{}: {} files, {} parsed, {} removed in {}ms",
                path.display(),
                stats.files,
                stats.parsed,
                stats.removed,
                start.elapsed().as_millis()
            ),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }
}
