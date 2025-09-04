use std::{
    fs::{self, File},
    io,
    path::PathBuf,
};

use anyhow::Context;
use clap::Parser;
use serde::Deserialize;
use url::Url;
use wptreport::{
    score_summary::FocusArea,
    score_wpt_report,
    servo_test_scores::WptScores,
    summarize::{summarize_results, RunInfoWithScores},
    wpt_report::WptReport,
};

static ALL_BROWSERS: &str = concat!(
    "https://wpt.fyi/api/runs?label=experimental&label=master",
    "&product=chrome",
    "&product=safari",
    "&product=firefox",
    "&product=servo",
    "&product=ladybird",
);

#[derive(Clone, Debug, Default, Parser)]
#[clap(name = "other-browsers")]
pub struct OtherBrowsers {
    /// Output directory
    #[arg(long)]
    out: PathBuf,

    /// Read focus areas from FOCUS_AREAS
    #[arg(long)]
    focus_areas: PathBuf,
}

#[derive(Debug, Deserialize)]
struct LatestResults {
    browser_name: String,
    created_at: String,
    raw_results_url: Url,
}

impl OtherBrowsers {
    pub fn run(self) {
        // Check basic things
        if !self.focus_areas.exists() {
            println!("Focus area file does not exist");
            return;
        }

        if !self.out.is_dir() {
            println!("Out directory needs to be a directory");
            return;
        }

        let all_browsers: Vec<LatestResults> = reqwest::blocking::get(ALL_BROWSERS)
            .expect("Could not get list of all browsers")
            .error_for_status()
            .expect("Wrong status code")
            .json()
            .expect("Could not convert json");

        println!(
            "Found the following browsers {:?}",
            all_browsers
                .iter()
                .map(|lr| lr.browser_name.clone())
                .collect::<Vec<_>>()
        );

        for run in all_browsers {
            match self
                .per_browser(&run)
                .with_context(|| format!("Error for {}", run.browser_name))
            {
                Ok(_) => (),
                Err(e) => println!("\nError in getting {} (Error: {:?})\n", run.browser_name, e),
            }
        }
    }

    fn per_browser(&self, run: &LatestResults) -> anyhow::Result<()> {
        println!("Working on converting {}", run.browser_name);
        let mut in_file =
            tempfile::NamedTempFile::new().context("Could not create temp file to download to")?;
        println!("Downloading report");
        let mut resp = reqwest::blocking::get(run.raw_results_url.clone())
            .context("Could not download file")?;
        io::copy(&mut resp, &mut in_file).context("Failed to download report")?;

        // converting the file
        println!("Converting file");
        let report_str =
            fs::read_to_string(in_file.path()).context("Could not read report string")?;
        let wpt_report: WptReport =
            serde_json::from_str(&report_str).context("Could not parse report json")?;
        let run_info = wpt_report.run_info.clone();
        let servo_scores_report = WptScores::from(wpt_report);

        println!("Converting scores");
        // Calculating scores according to focus
        let focus_areas_json =
            fs::read_to_string(self.focus_areas.clone()).context("Could not read focus area")?;
        let focus_areas: Vec<FocusArea> =
            serde_json::from_str(&focus_areas_json).context("Could not parse focus area json")?;
        let scored_wpt_report = score_wpt_report(&servo_scores_report);

        let scored_summary_result = RunInfoWithScores {
            date: run.created_at.clone(),
            info: run_info,
            scores: scored_wpt_report,
        };

        let score_summary = summarize_results(&vec![scored_summary_result], &focus_areas);

        let path = self.out.join(format!("{}_score.json", run.browser_name));
        println!("Writing to {path:?}");
        let f = File::create(path).context("Could not create score file")?;
        serde_json::to_writer_pretty(f, &score_summary).map_err(|e| e.into())
    }
}
