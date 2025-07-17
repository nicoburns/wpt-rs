use std::collections::BTreeMap;
use std::fs::{self, read_dir, File};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, time::Instant};

use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use serde::de::DeserializeOwned;
use wptreport::score_summary::FocusArea;
use wptreport::summarize::{summarize_results, RunInfoWithScores};
use wptreport::wpt_report::WptRunInfo;
use xz2::read::XzDecoder;
// use serde_jsonlines::{json_lines, JsonLinesReader};

use wptreport::reports::servo_test_scores::WptScores;
use wptreport::{score_wpt_report, AreaScores, HasRunInfo, ScorableReport};

// Use jemalloc as the allocator
#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn as_percent(amount: u32, out_of: u32) -> f32 {
    (amount as f32 / out_of as f32) * 100.0
}

fn get_focus_areas() -> Vec<FocusArea> {
    [
        FocusArea {
            name: String::from("All WPT tests"),
            areas: vec![String::from("")],
        },
        FocusArea::from("/content-security-policy"),
        FocusArea::from("/css"),
        FocusArea::from("/css/CSS2"),
        FocusArea::from("/css/CSS2/abspos"),
        FocusArea::from("/css/CSS2/box-display"),
        FocusArea::from("/css/CSS2/floats"),
        FocusArea::from("/css/CSS2/floats-clear"),
        FocusArea::from("/css/CSS2/linebox"),
        FocusArea::from("/css/CSS2/margin-padding-clear"),
        FocusArea::from("/css/CSS2/normal-flow"),
        FocusArea::from("/css/CSS2/positioning"),
        FocusArea {
            name: String::from("/css/CSS2/tables/ & /css/css-tables/"),
            areas: vec![
                String::from("/css/CSS2/tables"),
                String::from("/css/css-tables"),
            ],
        },
        FocusArea::from("/css/cssom"),
        FocusArea::from("/css/css-align"),
        FocusArea::from("/css/css-flexbox"),
        FocusArea::from("/css/css-grid"),
        FocusArea::from("/css/css-position"),
        FocusArea::from("/css/css-sizing"),
        FocusArea::from("/css/css-text"),
        FocusArea::from("/gamepad"),
        FocusArea::from("/shadow-dom"),
        FocusArea::from("/streams"),
        FocusArea::from("/trusted-types"),
        FocusArea::from("/WebCryptoAPI"),
        FocusArea::from("/webxr"),
    ]
    .into_iter()
    .collect()
}

fn main() {
    let mut args = env::args();
    let in_path = args.nth(1).unwrap();
    let in_path_buf = PathBuf::from(&in_path);

    let start = Instant::now();
    let focus_areas = get_focus_areas();

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
            "Processed {in_path} in {}ms (read in {}ms; Scored in {}ms)",
            result.total_time, result.read_time, result.score_time
        );
    } else if in_path_buf.is_dir() {
        let dir_entries = read_dir(&in_path_buf).unwrap();

        let mut file_paths: Vec<_> = dir_entries
            .flatten()
            .filter(|entry| entry.metadata().unwrap().is_file())
            .map(|entry| entry.path().to_string_lossy().to_string())
            .filter(|path| {
                path.rsplit_once('/')
                    .is_some_and(|(_prefix, file_name)| !file_name.starts_with('.'))
            })
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
                    score_report_against_reference::<WptScores>(file_path, &latest_report)?;
                let file_name = &file_path.rsplit_once('/').unwrap().1;
                let i = i.fetch_add(1, Ordering::SeqCst);
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
        panic!("{in_path} is not a file or directory");
    }
}

pub struct ScoreResult {
    scores_by_area: BTreeMap<String, AreaScores>,
    run_info: WptRunInfo,
    read_time: u128,
    score_time: u128,
    total_time: u128,
}

pub fn read_maybe_compressed_file(file_path: &str) -> String {
    let file = File::open(file_path).unwrap();

    if file_path.ends_with("xz") {
        let mut decompressed = XzDecoder::new(file);
        let mut s = String::new();
        decompressed.read_to_string(&mut s).unwrap();
        s
    } else if file_path.ends_with("zst") {
        let mut decompressed = zstd::Decoder::new(file).unwrap();
        let mut s = String::new();
        decompressed.read_to_string(&mut s).unwrap();
        s
    } else {
        fs::read_to_string(file_path).unwrap()
    }
}

pub fn score_report_against_reference<T>(
    file_path: &str,
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
    file_path: &str,
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
