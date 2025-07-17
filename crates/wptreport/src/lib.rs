pub mod aggregate;
pub mod merge;
pub mod reports;
pub mod score;
pub mod summarize;

use std::{iter::Sum, ops::Add};

use reports::wpt_report::WptRunInfo;
pub use reports::{score_summary, servo_test_scores, wpt_report};
pub use score::score_wpt_report;

pub trait HasRunInfo {
    fn run_info(&self) -> &WptRunInfo;
}

#[rustfmt::skip]
pub trait ScorableReport {
    type TestResultIter<'b>: TestResultIter + 'b where Self: 'b;
    type TestIter<'a>: Iterator<Item = Self::TestResultIter<'a>> where Self: 'a;

    fn results(&self) -> Self::TestIter<'_>;
}

pub trait TestResultIter {
    fn name(&self) -> &str;
    fn subtest_counts(&self) -> SubtestCounts;

    fn subtest_exist_and_passes(&self, name: &str) -> bool;
    fn iter_subtests_results(&self) -> impl Iterator<Item = SubtestNameAndResult<'_>>;
}

pub struct SubtestNameAndResult<'a> {
    pub name: &'a str,
    pub passes: bool,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct AreaScores {
    pub tests: SubtestCounts,
    pub subtests: SubtestCounts,
    // See: https://github.com/web-platform-tests/results-analysis/blob/0357bcf8973a6de5f544e1f82e50e7322805e214/interop-scoring/main.js#L250
    // This value represents the sum of the interop score for each individual test. But has not been divided by the number of tests.
    pub interop_score_sum: u64,
    // The sum of the "fraction of passing subtests" for each individual test (as a float). But not divided by the total number of tests
    pub pass_fraction_sum: f64,
}

impl Add for AreaScores {
    type Output = AreaScores;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            tests: self.tests + rhs.tests,
            subtests: self.subtests + rhs.subtests,
            interop_score_sum: self.interop_score_sum + rhs.interop_score_sum,
            pass_fraction_sum: self.pass_fraction_sum + rhs.pass_fraction_sum,
        }
    }
}

impl AreaScores {
    /// The WPT score percentage using the "interop" scoring methodology
    /// The value is represented as a number between 0 and 1000
    /// See: https://github.com/web-platform-tests/results-analysis/blob/0357bcf8973a6de5f544e1f82e50e7322805e214/interop-scoring/main.js#L250
    pub fn interop_score(&self) -> u16 {
        (self.interop_score_sum as f64 / self.tests.total as f64).floor() as u16
    }

    pub fn servo_score(&self) -> f64 {
        self.pass_fraction_sum
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct SubtestCounts {
    pub pass: u32,
    pub total: u32,
}

impl Add for SubtestCounts {
    type Output = SubtestCounts;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            pass: self.pass + rhs.pass,
            total: self.total + rhs.total,
        }
    }
}
impl Sum for AreaScores {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        let mut acc = Self::default();
        for scores in iter {
            acc = acc + scores;
        }
        acc
    }
}

impl SubtestCounts {
    pub fn all_passing(self) -> bool {
        self.pass == self.total
    }

    pub fn pass_fraction(self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.pass as f64) / (self.total as f64)
        }
    }

    pub fn passes_per_1000(self) -> u16 {
        if self.total == 0 {
            0
        } else {
            ((self.pass as f64) / (self.total as f64) * 1000.0).floor() as u16
        }
    }
}
