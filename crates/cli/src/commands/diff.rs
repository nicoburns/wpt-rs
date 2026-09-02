use std::path::PathBuf;
use std::time::Instant;

use clap::{Parser, ValueEnum};
use wptreport::aggregate::{diff, SubtestDetail, SubtestDiff, TestDiff};
use wptreport::wpt_report::{SubtestStatus, TestStatus, WptReport};
use wptreport::SubtestCounts;

use crate::compression::read_maybe_compressed_file;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum DiffFormat {
    #[default]
    Text,
    Json,
}

#[derive(Clone, Debug, Default, Parser)]
#[clap(name = "diff")]
pub struct Diff {
    /// Read report file from FILE_A
    file_a: PathBuf,

    /// Read report file from FILE_B
    file_b: PathBuf,

    /// Output format
    #[clap(long, value_enum, default_value_t = DiffFormat::Text)]
    format: DiffFormat,

    /// List the individual subtests that changed under each test
    #[clap(long, short)]
    verbose: bool,
}

impl Diff {
    pub fn run(self) {
        let start = Instant::now();

        // Read files
        let report_str = read_maybe_compressed_file(&self.file_a);
        let report_a: WptReport = serde_json::from_str(&report_str).unwrap();
        let report_str = read_maybe_compressed_file(&self.file_b);
        let report_b: WptReport = serde_json::from_str(&report_str).unwrap();

        // The individual subtest diff is only computed when it is going to be
        // displayed, as it is much more expensive than comparing counts
        let detail = if self.verbose {
            SubtestDetail::Full
        } else {
            SubtestDetail::Counts
        };
        let crashes = StatusCount::new(&report_a, &report_b, TestStatus::Crash);
        let timeouts = StatusCount::new(&report_a, &report_b, TestStatus::Timeout);

        let diffs = diff(&mut [report_a, report_b], detail);

        match self.format {
            DiffFormat::Json => {
                serde_json::to_writer_pretty(std::io::stdout(), &diffs).unwrap();
                println!();
            }
            DiffFormat::Text => {
                for line in text_lines(&diffs, self.verbose) {
                    println!("{line}");
                }

                let grand_total_time = start.elapsed().as_millis();
                println!("====================");
                for line in summary_lines(&diffs, crashes, timeouts) {
                    println!("{line}");
                }
                println!("Done in {grand_total_time}ms");
            }
        }
    }
}

/// The number of tests with a given status, before and after
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusCount {
    before: usize,
    after: usize,
}

impl StatusCount {
    fn new(before: &WptReport, after: &WptReport, status: TestStatus) -> Self {
        Self {
            before: count_status(before, status),
            after: count_status(after, status),
        }
    }

    fn delta(&self) -> i64 {
        self.after as i64 - self.before as i64
    }
}

fn count_status(report: &WptReport, status: TestStatus) -> usize {
    report
        .results
        .iter()
        .filter(|result| result.status == status)
        .count()
}

/// The change in the number of passing subtests
fn delta(diff: &TestDiff) -> i64 {
    match diff {
        TestDiff::Added { counts, .. } => i64::from(counts.pass),
        TestDiff::Removed { counts, .. } => -i64::from(counts.pass),
        TestDiff::Changed {
            counts_before,
            counts_after,
            ..
        } => i64::from(counts_after.pass) - i64::from(counts_before.pass),
    }
}

/// The subtest counts to display: the new state of the test, except for
/// removed tests which only have an old state.
fn displayed_counts(diff: &TestDiff) -> SubtestCounts {
    match diff {
        TestDiff::Added { counts, .. } | TestDiff::Removed { counts, .. } => *counts,
        TestDiff::Changed { counts_after, .. } => *counts_after,
    }
}

fn status_column(diff: &TestDiff) -> String {
    match diff {
        TestDiff::Added { .. } => String::from("ADD"),
        TestDiff::Removed { .. } => String::from("REM"),
        TestDiff::Changed { before, after, .. } => {
            format!(
                "{} => {}",
                test_status_name(*before),
                test_status_name(*after)
            )
        }
    }
}

pub fn text_lines(diffs: &[TestDiff], verbose: bool) -> Vec<String> {
    let statuses: Vec<String> = diffs.iter().map(status_column).collect();
    let counts: Vec<SubtestCounts> = diffs.iter().map(displayed_counts).collect();
    let deltas: Vec<i64> = diffs.iter().map(delta).collect();

    let status_width = statuses.iter().map(String::len).max().unwrap_or(0);
    let pass_width = width_of(counts.iter().map(|c| c.pass));
    let total_width = width_of(counts.iter().map(|c| c.total));
    let delta_width = deltas
        .iter()
        .map(|delta| format!("{delta:+}").len())
        .max()
        .unwrap_or(0);

    let mut lines = Vec::with_capacity(diffs.len());
    for (i, diff) in diffs.iter().enumerate() {
        let status = &statuses[i];
        let SubtestCounts { pass, total } = counts[i];
        let delta = format!("{:+}", deltas[i]);
        lines.push(format!(
            "{status:<status_width$}  [{pass:>pass_width$}/{total:>total_width$}]  {delta:>delta_width$}  {}",
            diff.test(),
        ));

        if verbose {
            if let TestDiff::Changed { subtests, .. } = diff {
                lines.extend(subtests.iter().map(subtest_line));
            }
        }
    }

    lines
}

fn subtest_line(subtest: &SubtestDiff) -> String {
    let status = match subtest {
        SubtestDiff::Added { status, .. } => {
            format!("ADD          {}", subtest_status_name(*status))
        }
        SubtestDiff::Removed { status, .. } => {
            format!("REM          {}", subtest_status_name(*status))
        }
        SubtestDiff::Changed { before, after, .. } => format!(
            "{} => {}",
            subtest_status_name(*before),
            subtest_status_name(*after)
        ),
    };
    format!("    {status}  {}", escape(subtest.name()))
}

pub fn summary_lines(
    diffs: &[TestDiff],
    crashes: StatusCount,
    timeouts: StatusCount,
) -> Vec<String> {
    let mut added = 0;
    let mut removed = 0;
    let mut subtests_gained: i64 = 0;
    let mut subtests_lost: i64 = 0;

    for diff in diffs {
        match diff {
            TestDiff::Added { .. } => added += 1,
            TestDiff::Removed { .. } => removed += 1,
            TestDiff::Changed { .. } => {}
        }

        let delta = delta(diff);
        if delta > 0 {
            subtests_gained += delta;
        } else {
            subtests_lost -= delta;
        }
    }

    let net_subtests = subtests_gained - subtests_lost;

    vec![
        format!(
            "Subtests: {subtests_gained} newly passing, {subtests_lost} newly failing \
             (net {net_subtests:+})"
        ),
        format!("Tests:    {added} added, {removed} removed"),
        format!("Crashes:  {} ({:+})", crashes.after, crashes.delta()),
        format!("Timeouts: {} ({:+})", timeouts.after, timeouts.delta()),
    ]
}

fn width_of(values: impl Iterator<Item = u32>) -> usize {
    values
        .map(|value| value.to_string().len())
        .max()
        .unwrap_or(0)
}

/// Escape characters that would break the one-line-per-change output
fn escape(name: &str) -> String {
    name.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn test_status_name(status: TestStatus) -> &'static str {
    match status {
        TestStatus::Pass => "PASS",
        TestStatus::Fail => "FAIL",
        TestStatus::Ok => "OK",
        TestStatus::Error => "ERROR",
        TestStatus::Timeout => "TIMEOUT",
        TestStatus::Crash => "CRASH",
        TestStatus::Assert => "ASSERT",
        TestStatus::PreconditionFailed => "PRECONDITION_FAILED",
        TestStatus::Skip => "SKIP",
    }
}

fn subtest_status_name(status: SubtestStatus) -> &'static str {
    match status {
        SubtestStatus::Pass => "PASS",
        SubtestStatus::Fail => "FAIL",
        SubtestStatus::Error => "ERROR",
        SubtestStatus::Timeout => "TIMEOUT",
        SubtestStatus::Assert => "ASSERT",
        SubtestStatus::PreconditionFailed => "PRECONDITION_FAILED",
        SubtestStatus::Notrun => "NOTRUN",
        SubtestStatus::Skip => "SKIP",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(pass: u32, total: u32) -> SubtestCounts {
        SubtestCounts { pass, total }
    }

    fn changed(
        test: &str,
        before: TestStatus,
        after: TestStatus,
        b: (u32, u32),
        a: (u32, u32),
    ) -> TestDiff {
        TestDiff::Changed {
            test: test.to_string(),
            before,
            after,
            counts_before: counts(b.0, b.1),
            counts_after: counts(a.0, a.1),
            subtests: Vec::new(),
        }
    }

    #[test]
    fn formats_aligned_rows() {
        let diffs = vec![
            TestDiff::Added {
                test: String::from("/css/added.html"),
                status: TestStatus::Ok,
                counts: counts(4, 6),
            },
            changed(
                "/css/changed.html",
                TestStatus::Fail,
                TestStatus::Ok,
                (233, 23423),
                (477, 23423),
            ),
            TestDiff::Removed {
                test: String::from("/css/removed.html"),
                status: TestStatus::Fail,
                counts: counts(2, 3),
            },
        ];

        assert_eq!(
            text_lines(&diffs, false),
            vec![
                "ADD         [  4/    6]    +4  /css/added.html",
                "FAIL => OK  [477/23423]  +244  /css/changed.html",
                "REM         [  2/    3]    -2  /css/removed.html",
            ]
        );
    }

    #[test]
    fn lists_subtests_when_verbose() {
        let diffs = vec![TestDiff::Changed {
            test: String::from("/css/changed.html"),
            before: TestStatus::Fail,
            after: TestStatus::Fail,
            counts_before: counts(3, 10),
            counts_after: counts(4, 10),
            subtests: vec![
                SubtestDiff::Changed {
                    name: String::from("first\nsubtest"),
                    before: SubtestStatus::Fail,
                    after: SubtestStatus::Pass,
                },
                SubtestDiff::Added {
                    name: String::from("second subtest"),
                    status: SubtestStatus::Fail,
                },
                SubtestDiff::Removed {
                    name: String::from("third subtest"),
                    status: SubtestStatus::Pass,
                },
            ],
        }];

        assert_eq!(
            text_lines(&diffs, true),
            vec![
                "FAIL => FAIL  [4/10]  +1  /css/changed.html",
                "    FAIL => PASS  first\\nsubtest",
                "    ADD          FAIL  second subtest",
                "    REM          PASS  third subtest",
            ]
        );
        assert_eq!(
            text_lines(&diffs, false),
            vec!["FAIL => FAIL  [4/10]  +1  /css/changed.html"]
        );
    }

    #[test]
    fn summarises_changes() {
        let diffs = vec![
            changed(
                "/css/fixed.html",
                TestStatus::Fail,
                TestStatus::Ok,
                (1, 2),
                (2, 2),
            ),
            changed(
                "/css/regressed.html",
                TestStatus::Ok,
                TestStatus::Timeout,
                (10, 10),
                (0, 10),
            ),
            changed(
                "/css/subtests-only.html",
                TestStatus::Fail,
                TestStatus::Fail,
                (3, 10),
                (9, 10),
            ),
            TestDiff::Removed {
                test: String::from("/css/removed.html"),
                status: TestStatus::Fail,
                counts: counts(2, 3),
            },
        ];

        assert_eq!(
            summary_lines(
                &diffs,
                StatusCount {
                    before: 3,
                    after: 5
                },
                StatusCount {
                    before: 12,
                    after: 9
                },
            ),
            vec![
                "Subtests: 7 newly passing, 12 newly failing (net -5)",
                "Tests:    0 added, 1 removed",
                "Crashes:  5 (+2)",
                "Timeouts: 9 (-3)",
            ]
        );
    }
}
