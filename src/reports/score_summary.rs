//! A score summary file as used
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusArea {
    pub name: String,
    pub areas: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunSummary {
    /// The date the run occured inYYYY-MM-DD format
    pub date: String,
    /// The version of the WPT test suite that was run
    pub wpt_revision: String,
    /// The version of the browser that was tested
    pub product_version: String,
    /// Scores are a percentage expressed a number between 0 and 1000
    pub scores: Vec<u16>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScoreSummaryReport {
    pub focus_areas: Vec<FocusArea>,
    pub scores: Vec<RunSummary>,
}
