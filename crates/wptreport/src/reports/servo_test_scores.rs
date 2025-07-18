//! The cut-down version of the "wptreport" format used by Servo to store scores
//! in the internal-wpt-dashboard repository

use crate::{HasRunInfo, ScorableReport, SubtestCounts, TestResultIter};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::wpt_report::{SubtestStatus, TestStatus, WptReport, WptRunInfo};

#[derive(Debug, Serialize, Deserialize)]
pub struct WptScores {
    pub run_info: WptRunInfo,
    pub test_scores: IndexMap<String, TestScore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestScore {
    pub score: u32,
    pub subtests: IndexMap<String, SubtestScore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubtestScore {
    pub score: u32,
}

#[rustfmt::skip]
impl ScorableReport for WptScores {
    type TestResultIter<'a> = (&'a String, &'a TestScore) where Self: 'a;
    fn results(&self) -> impl Iterator<Item = Self::TestResultIter<'_>> {
        self.test_scores.iter()
    }
}

impl HasRunInfo for WptScores {
    fn run_info(&self) -> &WptRunInfo {
        &self.run_info
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
                pass: self.1.score,
            }
        } else {
            let pass = self.1.subtests.values().fold(0, |mut pass_count, subtest| {
                pass_count += subtest.score;
                pass_count
            });
            SubtestCounts { pass, total }
        }
    }

    fn subtest_exist_and_passes(&self, name: &str) -> bool {
        self.1.subtests.get(name).is_some_and(|s| s.score > 0)
    }

    fn iter_subtests_results<'a>(
        &'a self,
    ) -> impl Iterator<Item = crate::SubtestNameAndResult<'a>> {
        self.1
            .subtests
            .iter()
            .map(|(name, s)| crate::SubtestNameAndResult {
                name,
                passes: s.score > 0,
            })
    }
}

impl From<WptReport> for WptScores {
    fn from(report: WptReport) -> Self {
        WptScores {
            run_info: report.run_info,
            test_scores: report
                .results
                .into_iter()
                .map(|test| {
                    let score = TestScore {
                        score: (test.status == TestStatus::Pass) as u32,
                        subtests: test
                            .subtests
                            .into_iter()
                            .map(|subtest| {
                                let score = SubtestScore {
                                    score: (subtest.status == SubtestStatus::Pass) as u32,
                                };
                                (subtest.name, score)
                            })
                            .collect(),
                    };
                    (test.test, score)
                })
                .collect(),
        }
    }
}

impl WptScores {
    /// In order to match the serialization order of a JavaScript object, it is useful to be be able to
    /// sort keys in the order in which JS seralizes things. We also first apply an alphabetic sort so
    /// that the sort order is deterministic
    pub fn apply_javascript_key_sort(&mut self) {
        use std::cmp::Ordering;

        #[inline(always)]
        fn is_digit(c: &u8) -> bool {
            *c >= b'0' && *c <= b'9'
        }

        #[inline(always)]
        fn javascript_object_key_sort<V>(
            a_key: &String,
            _a_value: &V,
            b_key: &String,
            _b_value: &V,
        ) -> Ordering {
            let a_is_int = a_key.as_bytes().iter().all(is_digit)
                && (a_key.len() == 1 || a_key.as_bytes()[0] != b'0');
            let b_is_int = b_key.as_bytes().iter().all(is_digit)
                && (b_key.len() == 1 || b_key.as_bytes()[0] != b'0');
            match (a_is_int, b_is_int) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (true, true) => {
                    let a = a_key.parse::<u64>().unwrap();
                    let b = b_key.parse::<u64>().unwrap();
                    a.cmp(&b)
                }
                (false, false) => Ordering::Equal,
            }
        }

        #[inline(always)]
        fn alphabetical_javascript_object_key_sort<V>(
            a_key: &String,
            _a_value: &V,
            b_key: &String,
            _b_value: &V,
        ) -> Ordering {
            match javascript_object_key_sort(a_key, _a_value, b_key, _b_value) {
                Ordering::Less => Ordering::Less,
                Ordering::Greater => Ordering::Greater,
                Ordering::Equal => a_key.cmp(b_key),
            }
        }

        self.test_scores
            .sort_by(alphabetical_javascript_object_key_sort);
        for scores in self.test_scores.values_mut() {
            scores.subtests.sort_by(javascript_object_key_sort);
        }
    }
}

mod score {
    use std::collections::BTreeMap;

    use super::{SubtestCounts, TestScore, WptScores};
    use crate::{score::area_iter, AreaScores};

    impl TestScore {
        /// Scores a test against a reference test
        /// This means that we only count subtests that were run in the reference test
        pub fn score_against(&self, reference: &TestScore) -> SubtestCounts {
            if reference.subtests.is_empty() {
                SubtestCounts {
                    pass: self.score,
                    total: 1,
                }
            } else {
                SubtestCounts {
                    pass: reference
                        .subtests
                        .keys()
                        .map(|subtest_name| {
                            self.subtests
                                .get(subtest_name)
                                .map(|subtest| subtest.score)
                                .unwrap_or(0)
                        })
                        .sum::<u32>(),
                    total: reference.subtests.len() as u32,
                }
            }
        }
    }

    impl WptScores {
        /// Scores a test run against a reference test run
        /// This means that we only count tests and subtests that were run in the reference run
        pub fn score_against(&self, reference: &WptScores) -> BTreeMap<String, AreaScores> {
            // let mut scores = AreaScores::default();
            let mut results = BTreeMap::<String, AreaScores>::new();

            for (test_name, reference_test) in reference.test_scores.iter() {
                let counts = match self.test_scores.get(test_name) {
                    Some(test) => test.score_against(reference_test),
                    None => SubtestCounts {
                        pass: 0,
                        total: reference_test.subtests.len().max(1) as u32,
                    },
                };
                let passes = counts.all_passing();

                // Update the scores for each area that the test belongs to
                for area in area_iter(test_name) {
                    if results.contains_key(area) {
                        let test_scores = results.get_mut(area).unwrap();
                        test_scores.tests.pass += passes as u32;
                        test_scores.tests.total += 1;
                        test_scores.subtests.pass += counts.pass;
                        test_scores.subtests.total += counts.total;
                        test_scores.interop_score_sum += counts.passes_per_1000() as u64;
                        test_scores.pass_fraction_sum += counts.pass_fraction();
                    } else {
                        let test_scores = AreaScores {
                            tests: SubtestCounts {
                                pass: passes as u32,
                                total: 1,
                            },
                            subtests: counts,
                            // The sum of the interop scores for each individual test, but not
                            // divided by the total number of tests
                            interop_score_sum: counts.passes_per_1000() as u64,
                            // The sum of the "fraction of passing subtests" for each individual test,
                            // but not divided by the total number of tests
                            pass_fraction_sum: counts.pass_fraction(),
                        };
                        results.insert(area.to_string(), test_scores);
                    };
                }
            }

            results
        }

        pub fn score(&self) -> BTreeMap<String, AreaScores> {
            self.score_against(self)
        }
    }
}
