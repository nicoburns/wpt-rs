use std::collections::BTreeMap;

use crate::score_summary::{RunScores, RunSummary, ScoreSummaryReport};
use crate::wpt_report::WptRunInfo;
use crate::AreaScores;

pub struct RunInfoWithScores {
    pub date: String,
    pub info: WptRunInfo,
    pub scores: BTreeMap<String, AreaScores>,
}

pub fn summarize_results(
    runs: &[RunInfoWithScores],
    focus_areas: &Vec<String>,
) -> ScoreSummaryReport {
    let focus_areas = (*focus_areas).clone();
    let mapped_runs = runs
        .iter()
        .map(|run| RunSummary {
            date: run.date.clone(),
            wpt_revision: run.info.revision[0..9].to_string(),
            product_version: run
                .info
                .browser_version
                .clone()
                .unwrap_or_else(|| String::from("unknown")),
            scores: focus_areas
                .iter()
                .map(|area| RunScores::from(*run.scores.get(area).unwrap()))
                .collect(),
        })
        .collect();

    ScoreSummaryReport {
        focus_areas,
        runs: mapped_runs,
    }
}
