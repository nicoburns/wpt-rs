//! The cut-down version of the "wptreport" format used by Servo to store scores
//! in the internal-wpt-dashboard repository

use crate::{HasRunInfo, ScorableReport, SubtestCounts, TestResultIter};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::wpt_report::WptRunInfo;

#[derive(Debug, Serialize, Deserialize)]
pub struct WptScores {
    pub run_info: WptRunInfo,
    pub test_scores: BTreeMap<String, TestScore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestScore {
    pub score: u32,
    pub subtests: BTreeMap<String, SubtestScore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubtestScore {
    pub score: u32,
}

#[rustfmt::skip]
impl ScorableReport for WptScores {
    type TestResultIter<'a> = (&'a String, &'a TestScore) where Self: 'a;
    type TestIter<'a> = std::collections::btree_map::Iter<'a, String, TestScore> where Self: 'a;

    fn results(&self) -> Self::TestIter<'_> {
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
                name: &name,
                passes: s.score > 0,
            })
    }
}

mod score {
    use super::{SubtestCounts, TestScore, WptScores};
    use crate::AreaScores;

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
        pub fn score_against(&self, reference: &WptScores) -> AreaScores {
            let mut scores = AreaScores::default();

            for (test_name, reference_test) in reference.test_scores.iter() {
                // Update totals
                scores.tests.total += 1;
                scores.subtests.total = reference_test.subtests.len() as u32;

                // Get test
                let Some(test) = self.test_scores.get(test_name) else {
                    continue;
                };

                // Update passes
                let subtest_counts = test.score_against(reference_test);
                scores.subtests.pass += subtest_counts.pass;
                scores.interop_score_sum = subtest_counts.passes_per_1000().into();
                if subtest_counts.pass == subtest_counts.total && subtest_counts.total != 0 {
                    scores.tests.total += 1;
                }
            }

            scores
        }

        pub fn score(&self) -> AreaScores {
            self.score_against(self)
        }
    }
}
