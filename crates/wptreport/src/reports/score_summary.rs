//! A score summary file as used
use serde::{Deserialize, Serialize};

use crate::AreaScores;

#[derive(Debug, Serialize, Deserialize)]
pub struct ScoreSummaryReport {
    pub focus_areas: Vec<String>,
    pub runs: Vec<RunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusArea {
    pub name: String,
    pub areas: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunSummary {
    /// The date the run occured in YYYY-MM-DD format
    pub date: String,
    /// The version of the WPT test suite that was run
    pub wpt_revision: String,
    /// The version of the browser that was tested
    pub product_version: String,
    /// Scores are a percentage expressed a number between 0 and 1000
    pub scores: Vec<RunScores>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct RunScores {
    pub interop_score: f32,
    pub total_tests: u32,
    pub total_tests_passed: u32,
    pub total_subtests: u32,
    pub total_subtests_passed: u32,
}

impl From<AreaScores> for RunScores {
    fn from(scores: AreaScores) -> Self {
        Self {
            interop_score: scores.interop_score() as f32 / 1000.0,
            total_tests: scores.tests.total,
            total_tests_passed: scores.tests.pass,
            total_subtests: scores.subtests.total,
            total_subtests_passed: scores.subtests.pass,
        }
    }
}
