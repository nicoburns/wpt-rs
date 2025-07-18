use std::fs::{self, read_dir};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use wptreport::merge::WptReportMerger;
use wptreport::wpt_report::WptReport;

use crate::compression::read_maybe_compressed_file;

#[derive(Clone, Debug, Default, Parser)]
#[clap(name = "merge")]
pub struct Merge {
    /// Read report files from IN
    #[arg(long)]
    r#in: PathBuf,

    /// Output merged report to OUT
    #[arg(long)]
    out: PathBuf,
}

impl Merge {
    pub fn run(self) {
        let in_path = self.r#in;
        let dir_entries = read_dir(&in_path).unwrap();
        let start = Instant::now();

        let mut file_paths: Vec<_> = dir_entries
            .flatten()
            .filter(|entry| entry.metadata().unwrap().is_file())
            .map(|entry| entry.path())
            .filter(|path| path.file_name().is_some_and(|p| p.as_bytes()[0] != b'.'))
            .collect();
        file_paths.sort();

        let mut merger = WptReportMerger::new();

        let count = file_paths.len();
        let mut i = 0;
        for path in file_paths {
            // Read file
            let read_start = Instant::now();
            let report_str = read_maybe_compressed_file(&path);
            let report: WptReport = serde_json::from_str(&report_str).unwrap();
            let read_elapsed = read_start.elapsed().as_millis();

            let merge_start = Instant::now();
            merger.add_chunk(report);
            let merge_elapsed = merge_start.elapsed().as_millis();

            let total_time = read_elapsed + merge_elapsed;
            let file_name = path.file_name().unwrap().display();
            i += 1;
            println!(
              "[{i}/{count}] Processed {file_name} in {total_time}ms (read in {read_elapsed}ms; Scored in {merge_elapsed}ms)",
          );
        }

        let write_start = Instant::now();
        let merged_report = merger.into_merged_report();
        let merged_report_str = serde_json::to_string(&merged_report).unwrap();
        fs::write(&self.out, merged_report_str).unwrap();
        let write_elapsed = write_start.elapsed().as_millis();

        println!("Generated merged report in {write_elapsed}ms");

        let grand_total_time = start.elapsed().as_millis();
        let out_file_name = self.out.display();
        println!("====================");
        println!("Wrote merged report to {out_file_name} in {grand_total_time}ms");
    }
}
