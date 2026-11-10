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
    /// Build a MANIFEST.json for the whole tree
    Build(BuildArgs),
}

#[derive(Clone, Debug, Args)]
struct BuildArgs {
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
        let ManifestCommand::Build(args) = self.action;
        let start = Instant::now();
        let path = args
            .path
            .unwrap_or_else(|| args.tests_root.join("MANIFEST.json"));
        let result = wpt_manifest::Manifest::build(&args.tests_root, args.url_base)
            .and_then(|manifest| manifest.write(&path).map(|()| manifest));
        match result {
            Ok(manifest) => println!(
                "Wrote {} ({} files) in {}ms",
                path.display(),
                manifest.files().count(),
                start.elapsed().as_millis()
            ),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }
}
