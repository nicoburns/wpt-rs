use std::collections::BTreeMap;

use crate::score_summary::{FocusArea, RunScores, RunSummary, ScoreSummaryReport};
use crate::wpt_report::WptRunInfo;
use crate::AreaScores;

pub struct RunInfoWithScores {
    pub date: String,
    pub info: WptRunInfo,
    pub scores: BTreeMap<String, AreaScores>,
}

pub fn summarize_results(
    runs: &[RunInfoWithScores],
    focus_areas: &[FocusArea],
) -> ScoreSummaryReport {
    let focus_areas = (*focus_areas).to_vec();

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
                .map(|focus_area| {
                    RunScores::from(
                        focus_area
                            .areas
                            .iter()
                            .map(|area| run.scores.get(area).cloned().unwrap_or_default())
                            .sum::<AreaScores>(),
                    )
                })
                .collect(),
        })
        .collect();

    ScoreSummaryReport {
        focus_areas: focus_areas.iter().map(|a| a.name.to_string()).collect(),
        runs: mapped_runs,
    }
}
