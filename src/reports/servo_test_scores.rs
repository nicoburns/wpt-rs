//! The cut-down version of the "wptreport" format used by Servo to store scores
//! in the internal-wpt-dashboard repository

use crate::{SubtestCounts, TestResultIter};
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
    pub score: u64,
    pub subtests: BTreeMap<String, SubtestScore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubtestScore {
    pub score: u32,
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
