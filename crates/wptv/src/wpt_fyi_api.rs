use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::{collections::BTreeMap, fmt::Write};
use wptreport::{TestResultIter, SubtestCounts, AreaScores, score_wpt_report};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub browser_name: String,
    pub browser_version: String,
    pub created_at: String,
    pub full_revision_hash: String,
    pub id: u64,
    pub labels: Vec<String>,
    pub os_name: String,
    pub os_version: String,
    pub raw_results_url: String,
    pub results_url: String,
    pub revision: String,
    pub time_end: String,
    pub time_start: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTestResults {
    pub test: String,
    pub legacy_status: Vec<SearchTestResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTestResult {
    pub passes: u32,
    pub total: u32,
    pub status: SmolStr,
    #[serde(rename = "newAggProcess")]
    pub new_agg_process: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub runs: Vec<RunMetadata>,
    pub results: Vec<SearchTestResults>,
}

struct SearchTestResultIter<'a> {
  results: &'a SearchTestResults,
  index: usize,
}

impl<'a> SearchTestResultIter<'a> {
  fn new(results: &'a SearchTestResults, index: usize) -> Self {
    Self { results, index }
  }
}

impl<'a> TestResultIter for SearchTestResultIter<'a> {
    fn name(&self) -> &str {
        &self.results.test
    }

    fn subtest_counts(&self) -> SubtestCounts {
        SubtestCounts {
            pass: self.results.legacy_status[self.index].passes,
            total: self.results.legacy_status[self.index].total,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SummarisedResults {
  metadata: RunMetadata,
  scores: BTreeMap<String, AreaScores>
}

pub async fn get_latest_runs() -> Result<Vec<RunMetadata>, reqwest::Error> {
    const RUNS_API_URL: &str = "https://wpt.fyi/api/runs?aligned=true";

    reqwest::Client::new()
        .get(RUNS_API_URL)
        .send()
        .await?
        .json::<Vec<RunMetadata>>()
        .await
}

pub async fn get_run_data(run_ids: &[u64]) -> Result<Vec<SummarisedResults>, reqwest::Error> {
    let mut url = String::with_capacity(200);
    url.push_str("https://wpt.fyi/api/search?run_ids=");
    for id in run_ids {
        write!(url, "{id},").unwrap();
    }
    url.pop();

    let mut results = reqwest::Client::new()
        .get(url)
        .send()
        .await?
        .json::<SearchResults>()
        .await?;

    results.results.sort_by(|a, b| a.test.cmp(&b.test));

    let summary : Vec<SummarisedResults> = results.runs.into_iter().enumerate().map(|(index, metadata)| {
      let scores = score_wpt_report(results.results.iter().map(|test| SearchTestResultIter::new(&test, index)));

      SummarisedResults {
        metadata,
        scores,
      }
    }).collect();

    Ok(summary)
}
