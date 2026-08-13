use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use wptreport::aggregate::aggregate;
use wptreport::wpt_report::{SubtestStatus, TestResult, WptReport};
use wptreport::TestResultIter;

use crate::compression::read_maybe_compressed_file;

#[derive(Clone, Debug, Default, Parser)]
#[clap(name = "diff")]
pub struct Diff {
    /// Read report file from FILE_A
    file_a: PathBuf,

    /// Read report file from FILE_B
    file_b: PathBuf,

    /// Also print a line for each individual subtest whose status changed
    #[clap(long)]
    subtests: bool,
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
                (None, Some(test)) => {
                    let counts = test.subtest_counts();
                    println!(
                        "ADD  {:?} {}/{}  {}",
                        test.status, counts.pass, counts.total, test.test
                    );
                }
                (Some(a), Some(b)) => {
                    let counts_a = a.subtest_counts();
                    let counts_b = b.subtest_counts();

                    if a.status != b.status || counts_a != counts_b {
                        println!(
                            "CHG  {:?} => {:?}  {}/{} => {}/{}  {}",
                            a.status,
                            b.status,
                            counts_a.pass,
                            counts_a.total,
                            counts_b.pass,
                            counts_b.total,
                            a.test
                        );
                        if self.subtests {
                            print_subtest_diff(a, b);
                        }
                    }
                }
            };
        });

        let grand_total_time = start.elapsed().as_millis();
        println!("====================");
        println!("Done in {grand_total_time}ms");
    }
}

fn print_subtest_diff(a: &TestResult, b: &TestResult) {
    let mut subtests: BTreeMap<&str, [Option<SubtestStatus>; 2]> = BTreeMap::new();
    for subtest in &a.subtests {
        subtests.entry(&subtest.name).or_default()[0] = Some(subtest.status);
    }
    for subtest in &b.subtests {
        subtests.entry(&subtest.name).or_default()[1] = Some(subtest.status);
    }

    for (name, [a, b]) in subtests {
        match (a, b) {
            (None, None) => unreachable!(),
            (Some(_), None) => println!("  REM  {name}"),
            (None, Some(b)) => println!("  ADD  {b:?}  {name}"),
            (Some(a), Some(b)) => {
                if a != b {
                    let sigil = match (a == SubtestStatus::Pass, b == SubtestStatus::Pass) {
                        (false, true) => '+',
                        (true, false) => '-',
                        _ => '~',
                    };
                    println!("  {sigil} {a:?} => {b:?}  {name}");
                }
            }
        }
    }
}
