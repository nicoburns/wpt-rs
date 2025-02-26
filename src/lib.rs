pub mod reports;
pub mod score;
pub mod summarize;

pub use reports::{score_summary, servo_test_scores, wpt_report};
pub use score::score_wpt_report;

// Use jemalloc as the allocator
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub trait TestResultIter {
    fn name(&self) -> &str;
    fn subtest_counts(&self) -> SubtestCounts;
}

#[derive(Debug, Copy, Clone, Default)]
pub struct AreaScores {
    pub tests: SubtestCounts,
    pub subtests: SubtestCounts,
    // See: https://github.com/web-platform-tests/results-analysis/blob/0357bcf8973a6de5f544e1f82e50e7322805e214/interop-scoring/main.js#L250
    // This value represents the sum of the interop score for each individual test. But has not been divided by the number of tests.
    pub interop_score_sum: u64,
}

impl AreaScores {
    /// The WPT score percentage using the "interop" scoring methodology
    /// The value is represented as a number between 0 and 1000
    /// See: https://github.com/web-platform-tests/results-analysis/blob/0357bcf8973a6de5f544e1f82e50e7322805e214/interop-scoring/main.js#L250
    pub fn interop_score(&self) -> u16 {
        (self.interop_score_sum as f64 / self.tests.total as f64).floor() as u16
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct SubtestCounts {
    pub pass: u32,
    pub total: u32,
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
