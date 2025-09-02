use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use wptreport::aggregate::aggregate;
use wptreport::wpt_report::WptReport;

use crate::compression::read_maybe_compressed_file;

#[derive(Clone, Debug, Default, Parser)]
#[clap(name = "diff")]
pub struct Diff {
    /// Read report file from FILE_A
    file_a: PathBuf,

    /// Read report file from FILE_B
    file_b: PathBuf,
}

impl Diff {
    pub fn run(self) {
        let start = Instant::now();

        // Read files
        let report_str = read_maybe_compressed_file(&self.file_a);
        let report_a: WptReport = serde_json::from_str(&report_str).unwrap();
        let report_str = read_maybe_compressed_file(&self.file_b);
        let report_b: WptReport = serde_json::from_str(&report_str).unwrap();

        // Diff and print results
        aggregate(&mut [report_a, report_b], |results| {
            let a = results[0];
            let b = results[1];

            match (a, b) {
                (None, None) => unreachable!(),
                (Some(test), None) => println!("REM  {}", test.test),
                (None, Some(test)) => println!("ADD  {}", test.test),
                (Some(a), Some(b)) => {
                    if a.status != b.status {
                        println!("{:?} => {:?} {}", a.status, b.status, a.test)
                    }
                }
            };
        });

        let grand_total_time = start.elapsed().as_millis();
        println!("====================");
        println!("Done in {grand_total_time}ms");
    }
}
