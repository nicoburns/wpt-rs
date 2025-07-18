use std::collections::BTreeMap;
use std::fs::{self, read_dir, File};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use clap::Parser;
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use serde::de::DeserializeOwned;
use wptreport::score_summary::FocusArea;
use wptreport::summarize::{summarize_results, RunInfoWithScores};
use wptreport::wpt_report::WptRunInfo;
use xz2::read::XzDecoder;
// use serde_jsonlines::{json_lines, JsonLinesReader};

use wptreport::reports::servo_test_scores::WptScores;
use wptreport::{score_wpt_report, AreaScores, HasRunInfo, ScorableReport};

#[derive(Clone, Debug, Default, Parser)]
#[clap(name = "calc-scores")]
pub struct CalcScores {
    /// Read report files from IN
    #[arg(long)]
    r#in: PathBuf,

    /// Output score summary at OUT
    #[arg(long)]
    out: PathBuf,

    /// Read focus areas from FOCUS_AREAS
    #[arg(long)]
    focus_areas: PathBuf,
}

fn as_percent(amount: u32, out_of: u32) -> f32 {
    (amount as f32 / out_of as f32) * 100.0
}

impl CalcScores {
    pub fn run(self) {
        let in_path_buf = self.r#in;
        let in_path = &in_path_buf;

        let start = Instant::now();

        let focus_areas_json = fs::read_to_string(self.focus_areas).unwrap();
        let focus_areas: Vec<FocusArea> = serde_json::from_str(&focus_areas_json).unwrap();

        if in_path_buf.is_file() {
            let result = score_report::<WptScores>(&in_path).unwrap();
            for (area, scores) in result.scores_by_area {
                let tests = scores.tests;
                let subtests = scores.subtests;
                let percentage = as_percent(scores.tests.pass, scores.tests.total);
                println!(
                    "{area}: {percentage:.2}% ({}/{} tests) ({}/{} subtests)",
                    tests.pass, tests.total, subtests.pass, subtests.total
                );
            }

            println!(
                "Processed {} in {}ms (read in {}ms; Scored in {}ms)",
                in_path.display(),
                result.total_time,
                result.read_time,
                result.score_time
            );
        } else if in_path_buf.is_dir() {
            let dir_entries = read_dir(&in_path_buf).unwrap();

            let mut file_paths: Vec<_> = dir_entries
                .flatten()
                .filter(|entry| entry.metadata().unwrap().is_file())
                .map(|entry| entry.path())
                .filter(|path| path.file_name().is_some_and(|p| p.as_bytes()[0] != b'.'))
                .collect();
            file_paths.sort();

            // Load most recent report
            let Some(latest_report_path) = file_paths.last() else {
                println!("No files found");
                return;
            };
            let latest_report_str = read_maybe_compressed_file(latest_report_path);
            let latest_report: WptScores = serde_json::from_str(&latest_report_str).ok().unwrap();

            let count = file_paths.len();
            let i = AtomicU64::new(0);
            let scores: Vec<_> = file_paths
                .par_iter()
                .filter_map(|file_path| {
                    let result =
                        score_report_against_reference::<WptScores>(file_path, &latest_report)
                            .unwrap();
                    let file_name = file_path.file_name().unwrap().to_str().unwrap();
                    let i = i.fetch_add(1, Ordering::SeqCst) + 1;
                    println!(
                    "[{i}/{count}] Processed {file_name} in {}ms (read in {}ms; Scored in {}ms)",
                    result.total_time, result.read_time, result.score_time
                );

                    let date = file_name[0..10].to_string();
                    Some(RunInfoWithScores {
                        date,
                        info: result.run_info,
                        scores: result.scores_by_area,
                    })
                })
                .collect();

            // Write scores.json file
            let score_summary = summarize_results(&scores, &focus_areas);
            let score_summary_str = serde_json::to_string(&score_summary).unwrap();
            fs::write("./scores.json", score_summary_str).unwrap();

            let grand_total_time = start.elapsed().as_secs();
            println!("====================");
            println!("Processed all files in {grand_total_time}s");
        } else {
            panic!("{} is not a file or directory", in_path.display());
        }
    }
}

pub struct ScoreResult {
    scores_by_area: BTreeMap<String, AreaScores>,
    run_info: WptRunInfo,
    read_time: u128,
    score_time: u128,
    total_time: u128,
}

pub fn read_maybe_compressed_file(file_path: &Path) -> String {
    let file = File::open(file_path).unwrap();

    let extension = file_path.extension().unwrap().as_bytes();
    if extension == b"xz" {
        let mut decompressed = XzDecoder::new(file);
        let mut s = String::new();
        decompressed.read_to_string(&mut s).unwrap();
        s
    } else if extension == b"zst" {
        let mut decompressed = zstd::Decoder::new(file).unwrap();
        let mut s = String::new();
        decompressed.read_to_string(&mut s).unwrap();
        s
    } else {
        fs::read_to_string(file_path).unwrap()
    }
}

pub fn score_report_against_reference<T>(
    file_path: &Path,
    reference: &WptScores,
) -> Option<ScoreResult>
where
    T: DeserializeOwned,
    WptScores: From<T>,
{
    let read_start = Instant::now();

    let report_str = read_maybe_compressed_file(file_path);
    let report: T = serde_json::from_str(&report_str).ok()?;
    let scores = WptScores::from(report);

    let read_elapsed = read_start.elapsed().as_millis();

    let score_start = Instant::now();
    let scores_by_area = scores.score_against(reference);
    let score_elapsed = score_start.elapsed().as_millis();
    let total_elapsed = read_start.elapsed().as_millis();

    Some(ScoreResult {
        scores_by_area,
        run_info: scores.run_info,
        read_time: read_elapsed,
        score_time: score_elapsed,
        total_time: total_elapsed,
    })
}

pub fn score_report<T: DeserializeOwned + ScorableReport + HasRunInfo>(
    file_path: &Path,
) -> Option<ScoreResult> {
    let read_start = Instant::now();

    let report_str = read_maybe_compressed_file(file_path);
    let report: T = serde_json::from_str(&report_str).ok()?;

    let read_elapsed = read_start.elapsed().as_millis();

    let score_start = Instant::now();
    let scores_by_area = score_wpt_report(&report);
    let score_elapsed = score_start.elapsed().as_millis();
    let total_elapsed = read_start.elapsed().as_millis();

    Some(ScoreResult {
        scores_by_area,
        run_info: report.run_info().clone(),
        read_time: read_elapsed,
        score_time: score_elapsed,
        total_time: total_elapsed,
    })
}
