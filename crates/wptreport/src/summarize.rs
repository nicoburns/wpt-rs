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
            wpt_revision: short_revision(&run.info).to_string(),
            product_revision: run.info.browser_version().unwrap_or("Unknown").to_string(),
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

/// The first 9 characters of the WPT revision (or the whole revision if it is shorter)
fn short_revision(info: &WptRunInfo) -> &str {
    let revision = info.revision().expect("run_info.revision is missing");
    revision.get(..9).unwrap_or(revision)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn summarize_run_info(run_info: &str) -> (String, String) {
        let run = RunInfoWithScores {
            date: String::from("2026-09-30"),
            info: serde_json::from_str(run_info).unwrap(),
            scores: BTreeMap::new(),
        };
        let summary = summarize_results(&[run], Some(&[]));
        let run = summary.runs.into_iter().next().unwrap();
        (run.wpt_revision, run.product_revision)
    }

    #[test]
    fn uses_short_revision_and_browser_version() {
        let run_info = r#"{"revision":"1cd1fadbec7e5ee8b931887b44a586fb136cfe62","browser_version":"0.6.0-5698249bd"}"#;
        assert_eq!(
            summarize_run_info(run_info),
            (String::from("1cd1fadbe"), String::from("0.6.0-5698249bd"))
        );
    }

    #[test]
    fn falls_back_for_missing_browser_version_and_short_revision() {
        for run_info in [
            r#"{"revision":"abc"}"#,
            r#"{"revision":"abc","browser_version":null}"#,
        ] {
            assert_eq!(
                summarize_run_info(run_info),
                (String::from("abc"), String::from("Unknown"))
            );
        }
    }
}
