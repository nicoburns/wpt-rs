use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;

#[derive(Clone, Debug, Parser)]
#[clap(name = "manifest")]
pub struct Manifest {
    /// Root of the web-platform-tests checkout
    #[clap(long, default_value = ".")]
    tests_root: PathBuf,

    /// Path to the manifest file [default: <TESTS_ROOT>/MANIFEST.json]
    #[clap(long, short)]
    path: Option<PathBuf>,

    /// Base URL of the tests
    #[clap(long, default_value = "/")]
    url_base: String,

    /// Ignore any existing manifest and reparse every file
    #[clap(long, short)]
    rebuild: bool,
}

impl Manifest {
    pub fn run(self) {
        let start = Instant::now();
        let path = self
            .path
            .unwrap_or_else(|| self.tests_root.join("MANIFEST.json"));
        match wptmanifest::Manifest::load_and_update(
            &self.tests_root,
            &path,
            &self.url_base,
            self.rebuild,
        ) {
            Ok((_, stats)) => println!(
                "Updated {} ({} files, {} parsed, {} removed) in {}ms",
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
