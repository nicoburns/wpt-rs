use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use wptreport::servo_test_scores::WptScores;
use wptreport::wpt_report::WptReport;

use crate::compression::read_maybe_compressed_file;

#[derive(Clone, Debug, Default, Parser)]
#[clap(name = "merge")]
pub struct Convert {
    /// Read report files from IN
    #[arg(long)]
    r#in: PathBuf,

    /// Output merged report to OUT
    #[arg(long)]
    out: PathBuf,

    /// Sort tests in JS object iteration order
    #[arg(long)]
    js_sort: bool,
}

impl Convert {
    pub fn run(self) {
        let in_path = self.r#in;
        let start = Instant::now();

        // Read file
        let read_start = Instant::now();
        let report_str = read_maybe_compressed_file(&in_path);
        let wpt_report: WptReport = serde_json::from_str(&report_str).unwrap();
        let read_elapsed = read_start.elapsed().as_millis();
        println!("Read and decompress report in {read_elapsed}ms");

        let convert_start = Instant::now();
        let mut servo_scores_report = WptScores::from(wpt_report);
        let convert_elapsed = convert_start.elapsed().as_millis();
        println!("Converted report to servo wpt scores format in {convert_elapsed}ms");

        if self.js_sort {
            servo_scores_report.apply_javascript_key_sort();
        }

        let write_start = Instant::now();
        let servo_scores_report_str = serde_json::to_string(&servo_scores_report).unwrap();
        fs::write(&self.out, servo_scores_report_str).unwrap();
        let write_elapsed = write_start.elapsed().as_millis();
        println!("Wrote report in {write_elapsed}ms");

        let grand_total_time = start.elapsed().as_millis();
        let out_file_name = self.out.display();
        println!("====================");
        println!("Wrote merged report to {out_file_name} in {grand_total_time}ms");
    }
}
