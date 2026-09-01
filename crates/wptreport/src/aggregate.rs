use crate::{
    wpt_report::{SubtestStatus, TestResult, TestStatus, WptReport},
    SubtestCounts, TestResultIter,
};
use serde::Serialize;
use std::collections::BTreeMap;

pub fn aggregate<T>(
    reports: &mut [WptReport],
    mut map_fn: impl FnMut(&[Option<&TestResult>]) -> T,
) -> Vec<T> {
    let report_count = reports.len();
    assert!(report_count <= 64);

    let mut test_names: BTreeMap<String, u64> = BTreeMap::new();
    for (i, report) in reports.iter_mut().enumerate() {
        report.results.sort_by(|a, b| a.test.cmp(&b.test));
        let mask = 1 << i;
        for result in &report.results {
            test_names
                .entry(result.test.clone())
                .and_modify(|bitset| *bitset |= mask)
                .or_insert(mask);
        }
    }

    let mut results = Vec::with_capacity(test_names.len());
    let mut iterators = reports
        .iter_mut()
        .map(|report| report.results.iter().peekable())
        .collect::<Vec<_>>();
    let mut current_row: Vec<Option<&TestResult>> = vec![None; report_count];

    for (_, bitvec) in test_names {
        for (i, item) in current_row.iter_mut().enumerate() {
            if (bitvec & (1 << i as u64)) != 0 {
                *item = iterators[i].next()
            } else {
                *item = None
            }
        }

        results.push(map_fn(&current_row));
    }

    results
}

/// A change to a single subtest between two reports
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SubtestDiff {
    Added {
        name: String,
        status: SubtestStatus,
    },
    Removed {
        name: String,
        status: SubtestStatus,
    },
    Changed {
        name: String,
        before: SubtestStatus,
        after: SubtestStatus,
    },
}

impl SubtestDiff {
    pub fn name(&self) -> &str {
        match self {
            Self::Added { name, .. } | Self::Removed { name, .. } | Self::Changed { name, .. } => {
                name
            }
        }
    }
}

/// A change to a single test between two reports
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TestDiff {
    Added {
        test: String,
        status: TestStatus,
        counts: SubtestCounts,
    },
    Removed {
        test: String,
        status: TestStatus,
        counts: SubtestCounts,
    },
    Changed {
        test: String,
        before: TestStatus,
        after: TestStatus,
        counts_before: SubtestCounts,
        counts_after: SubtestCounts,
        subtests: Vec<SubtestDiff>,
    },
}

impl TestDiff {
    pub fn test(&self) -> &str {
        match self {
            Self::Added { test, .. } | Self::Removed { test, .. } | Self::Changed { test, .. } => {
                test
            }
        }
    }
}

/// Compute the differences between two reports.
///
/// Only tests which have been added, removed or whose status, subtest counts
/// or subtest results have changed are included in the returned list.
pub fn diff(reports: &mut [WptReport; 2]) -> Vec<TestDiff> {
    aggregate(&mut *reports, |results| match (results[0], results[1]) {
        (None, None) => unreachable!(),
        (Some(test), None) => Some(TestDiff::Removed {
            test: test.test.clone(),
            status: test.status,
            counts: test.subtest_counts(),
        }),
        (None, Some(test)) => Some(TestDiff::Added {
            test: test.test.clone(),
            status: test.status,
            counts: test.subtest_counts(),
        }),
        (Some(a), Some(b)) => {
            let subtests = diff_subtests(a, b);
            let counts_before = a.subtest_counts();
            let counts_after = b.subtest_counts();

            if a.status == b.status && counts_before == counts_after && subtests.is_empty() {
                return None;
            }

            Some(TestDiff::Changed {
                test: a.test.clone(),
                before: a.status,
                after: b.status,
                counts_before,
                counts_after,
                subtests,
            })
        }
    })
    .into_iter()
    .flatten()
    .collect()
}

fn diff_subtests(a: &TestResult, b: &TestResult) -> Vec<SubtestDiff> {
    let mut statuses: BTreeMap<&str, (Option<SubtestStatus>, Option<SubtestStatus>)> =
        BTreeMap::new();
    for subtest in &a.subtests {
        statuses.entry(&subtest.name).or_default().0 = Some(subtest.status);
    }
    for subtest in &b.subtests {
        statuses.entry(&subtest.name).or_default().1 = Some(subtest.status);
    }

    statuses
        .into_iter()
        .filter_map(|(name, statuses)| match statuses {
            (None, None) => None,
            (Some(status), None) => Some(SubtestDiff::Removed {
                name: name.to_string(),
                status,
            }),
            (None, Some(status)) => Some(SubtestDiff::Added {
                name: name.to_string(),
                status,
            }),
            (Some(before), Some(after)) => (before != after).then(|| SubtestDiff::Changed {
                name: name.to_string(),
                before,
                after,
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wpt_report::SubtestResult;

    fn report(results: Vec<TestResult>) -> WptReport {
        let run_info = serde_json::from_str(
            r#"{
                "product": "test",
                "revision": "0000000000000000000000000000000000000000",
                "automation": true,
                "debug": false,
                "has_sandbox": false,
                "headless": true,
                "verify": false,
                "wasm": false,
                "os": "linux",
                "os_version": "24.04",
                "version": "24.04",
                "processor": "x86_64",
                "bits": 64,
                "python_version": 3
            }"#,
        )
        .unwrap();

        WptReport {
            time_start: 0,
            time_end: 1,
            run_info,
            results,
        }
    }

    fn test(name: &str, status: TestStatus, subtests: &[(&str, SubtestStatus)]) -> TestResult {
        TestResult {
            test: name.to_string(),
            status,
            duration: 0,
            message: None,
            known_intermittent: Vec::new(),
            subsuite: String::new(),
            subtests: subtests
                .iter()
                .map(|(name, status)| SubtestResult {
                    name: name.to_string(),
                    status: *status,
                    message: None,
                    known_intermittent: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn ignores_unchanged_tests() {
        let a = report(vec![test(
            "/css/a.html",
            TestStatus::Ok,
            &[("one", SubtestStatus::Pass)],
        )]);
        let b = report(vec![test(
            "/css/a.html",
            TestStatus::Ok,
            &[("one", SubtestStatus::Pass)],
        )]);

        assert_eq!(diff(&mut [a, b]), Vec::new());
    }

    #[test]
    fn reports_added_and_removed_tests() {
        let a = report(vec![test("/css/removed.html", TestStatus::Fail, &[])]);
        let b = report(vec![test(
            "/css/added.html",
            TestStatus::Ok,
            &[("one", SubtestStatus::Pass), ("two", SubtestStatus::Fail)],
        )]);

        assert_eq!(
            diff(&mut [a, b]),
            vec![
                TestDiff::Added {
                    test: String::from("/css/added.html"),
                    status: TestStatus::Ok,
                    counts: SubtestCounts { pass: 1, total: 2 },
                },
                TestDiff::Removed {
                    test: String::from("/css/removed.html"),
                    status: TestStatus::Fail,
                    counts: SubtestCounts { pass: 0, total: 1 },
                },
            ]
        );
    }

    #[test]
    fn reports_status_changes() {
        let a = report(vec![test("/css/a.html", TestStatus::Fail, &[])]);
        let b = report(vec![test("/css/a.html", TestStatus::Pass, &[])]);

        assert_eq!(
            diff(&mut [a, b]),
            vec![TestDiff::Changed {
                test: String::from("/css/a.html"),
                before: TestStatus::Fail,
                after: TestStatus::Pass,
                counts_before: SubtestCounts { pass: 0, total: 1 },
                counts_after: SubtestCounts { pass: 1, total: 1 },
                subtests: Vec::new(),
            }]
        );
    }

    #[test]
    fn reports_subtest_changes_when_status_is_unchanged() {
        let a = report(vec![test(
            "/css/a.html",
            TestStatus::Ok,
            &[
                ("one", SubtestStatus::Fail),
                ("two", SubtestStatus::Pass),
                ("three", SubtestStatus::Pass),
            ],
        )]);
        let b = report(vec![test(
            "/css/a.html",
            TestStatus::Ok,
            &[
                ("one", SubtestStatus::Pass),
                ("two", SubtestStatus::Pass),
                ("four", SubtestStatus::Fail),
            ],
        )]);

        assert_eq!(
            diff(&mut [a, b]),
            vec![TestDiff::Changed {
                test: String::from("/css/a.html"),
                before: TestStatus::Ok,
                after: TestStatus::Ok,
                counts_before: SubtestCounts { pass: 2, total: 3 },
                counts_after: SubtestCounts { pass: 2, total: 3 },
                subtests: vec![
                    SubtestDiff::Added {
                        name: String::from("four"),
                        status: SubtestStatus::Fail,
                    },
                    SubtestDiff::Changed {
                        name: String::from("one"),
                        before: SubtestStatus::Fail,
                        after: SubtestStatus::Pass,
                    },
                    SubtestDiff::Removed {
                        name: String::from("three"),
                        status: SubtestStatus::Pass,
                    },
                ],
            }]
        );
    }
}
