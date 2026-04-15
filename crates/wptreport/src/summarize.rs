use std::collections::{BTreeMap, HashSet};

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
    focus_areas: Option<&[FocusArea]>,
) -> ScoreSummaryReport {
    let focus_areas = focus_areas
        .map(|areas| areas.to_vec())
        .unwrap_or_else(|| default_focus_areas(runs));

    let mapped_runs = runs
        .iter()
        .map(|run| RunSummary {
            date: run.date.clone(),
            wpt_revision: run.info.revision[0..9].to_string(),
            product_revision: run
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

pub fn default_focus_areas(runs: &[RunInfoWithScores]) -> Vec<FocusArea> {
    let mut areas: HashSet<String> = HashSet::new();

    for run in runs {
        for area in run.scores.keys() {
            areas.insert(area.clone());
        }
    }

    let mut focus_areas = Vec::with_capacity(areas.len());

    for area in areas {
        focus_areas.push(FocusArea {
            name: area.clone(),
            areas: vec![area],
        });
    }

    focus_areas.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    focus_areas
}
