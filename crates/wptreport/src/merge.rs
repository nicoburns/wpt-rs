use std::collections::BTreeMap;

use crate::wpt_report::{TestResult, WptReport, WptRunInfo};

/// Allows multiple chunks of a WPT report format to be merged into a single WPT report
/// The input and output formats are the same, but output contains the test results from all chunks
pub struct WptReportMerger {
    run_info: Option<WptRunInfo>,
    time_start: u64,
    time_end: u64,
    scores: BTreeMap<String, TestResult>,
}

impl Default for WptReportMerger {
    fn default() -> Self {
        Self::new()
    }
}

impl WptReportMerger {
    pub fn new() -> Self {
        Self {
            run_info: None,
            time_start: u64::MAX,
            time_end: 0,
            scores: BTreeMap::new(),
        }
    }

    pub fn add_chunk(&mut self, chunk: WptReport) {
        // Check that run info matches
        match &mut self.run_info {
            // If this is the first chunk then just store the run info
            None => {
                self.run_info = Some(chunk.run_info);
            }
            // Else check that it matches
            Some(run_info) => {
                if *run_info != chunk.run_info {
                    panic!("run_info doesn't match")
                }
            }
        }

        self.time_start = self.time_start.min(chunk.time_start);
        self.time_end = self.time_end.max(chunk.time_end);

        for result in chunk.results.into_iter() {
            self.scores.insert(result.test.clone(), result);
        }
    }

    pub fn into_merged_report(self) -> WptReport {
        WptReport {
            run_info: self.run_info.unwrap(),
            time_start: self.time_start,
            time_end: self.time_end,
            results: self.scores.into_values().collect(),
        }
    }
}
