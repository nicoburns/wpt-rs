use std::collections::BTreeMap;

use crate::{AreaScores, ScorableReport, SubtestCounts, TestResultIter};

pub fn score_wpt_report<Report>(report: &Report) -> BTreeMap<String, AreaScores>
where
    Report: ScorableReport,
{
    let mut results = BTreeMap::<String, AreaScores>::new();

    for test in report.results() {
        // Compute scores for the test
        let counts = test.subtest_counts();
        let passes = counts.all_passing();
        // let pass_fraction = counts.pass_fraction();

        // Update the scores for each area that the test belongs to
        for area in area_iter(test.name()) {
            if results.contains_key(area) {
                let test_scores = results.get_mut(area).unwrap();
                test_scores.tests.pass += passes as u32;
                test_scores.tests.total += 1;
                test_scores.subtests.pass += counts.pass;
                test_scores.subtests.total += counts.total;
                test_scores.interop_score_sum += counts.passes_per_1000() as u64;
            } else {
                let test_scores = AreaScores {
                    tests: SubtestCounts {
                        pass: passes as u32,
                        total: 1,
                    },
                    subtests: counts,
                    // The sum of the interop scores for each individual test, but not
                    // divided by the total number of tests
                    interop_score_sum: counts.passes_per_1000() as u64,
                };
                results.insert(area.to_string(), test_scores);
            };
        }
    }

    results
}

fn area_iter<'a>(test_path: &'a str) -> impl Iterator<Item = &'a str> {
    let stripped_path = test_path
        .rsplit_once('/')
        .expect("Test name will contain at least one '/' character")
        .0
        .trim_matches('/');

    stripped_path
        .match_indices('/')
        .map(|(idx, _)| idx)
        .chain(std::iter::once(stripped_path.len()))
        .map(|idx| &stripped_path[0..idx])
}
