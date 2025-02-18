use std::collections::BTreeMap;

use crate::{SubtestStatus, TestResult, TestScore, TestStatus};

#[derive(Debug, Copy, Clone, Default)]
pub struct SubtestCounts {
    pub pass: u32,
    pub total: u32,
}

impl SubtestCounts {
    fn all_passing(self) -> bool {
        self.pass == self.total
    }

    fn pass_fraction(self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.pass as f64) / (self.total as f64)
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct AreaScores {
    pub tests: SubtestCounts,
    pub subtests: SubtestCounts,
}

pub trait TestResultIter {
    fn name(&self) -> &str;
    fn subtest_counts(&self) -> SubtestCounts;
}

impl TestResultIter for &TestResult {
    fn name(&self) -> &str {
        &self.test
    }

    fn subtest_counts(&self) -> SubtestCounts {
        let total = self.subtests.len() as u32;

        if total == 0 {
            SubtestCounts {
                total: 1,
                pass: (self.status == TestStatus::Pass) as u32,
            }
        } else {
            let pass = self.subtests.iter().fold(0, |mut pass_count, subtest| {
                pass_count += (subtest.status == SubtestStatus::Pass) as u32;
                pass_count
            });
            SubtestCounts { pass, total }
        }
    }
}

impl TestResultIter for (&String, &TestScore) {
    fn name(&self) -> &str {
        self.0
    }

    fn subtest_counts(&self) -> SubtestCounts {
        let total = self.1.subtests.len() as u32;
        if total == 0 {
            SubtestCounts {
                total: 1,
                pass: self.1.score as u32,
            }
        } else {
            let pass = self.1.subtests.values().fold(0, |mut pass_count, subtest| {
                pass_count += subtest.score;
                pass_count
            });
            SubtestCounts { pass, total }
        }
    }
}

pub fn score_wpt_report<Test, Report>(report: Report) -> BTreeMap<String, AreaScores>
where
    Test: TestResultIter,
    Report: Iterator<Item = Test>,
{
    let mut results = BTreeMap::<String, AreaScores>::new();

    for test in report {
        // Compute scores for the test
        let counts = test.subtest_counts();
        let passes = counts.all_passing();
        let pass_fraction = counts.pass_fraction();

        // Update the scores for each area that the test belongs to
        for area in area_iter(test.name()) {
            if results.contains_key(area) {
                let test_scores = results.get_mut(area).unwrap();
                test_scores.tests.pass += passes as u32;
                test_scores.tests.total += 1;
                test_scores.subtests.pass += counts.pass;
                test_scores.subtests.total += counts.total;
            } else {
                let test_scores = AreaScores {
                    tests: SubtestCounts {
                        pass: passes as u32,
                        total: 1,
                    },
                    subtests: counts,
                };
                results.insert(area.to_string(), test_scores);
            };
        }
    }

    results
}

fn area_iter<'a>(test_path: &'a str) -> impl Iterator<Item = &'a str> {
    let stripped_path = test_path
        .rsplit_once('/')
        .expect("Test name will contain at least one '/' character")
        .0
        .trim_matches('/');

    stripped_path
        .match_indices('/')
        .map(|(idx, _)| idx)
        .chain(std::iter::once(stripped_path.len()))
        .map(|idx| &stripped_path[0..idx])
}
