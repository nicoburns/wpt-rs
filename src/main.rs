use std::collections::BTreeMap;
use std::fs::{read_dir, File};
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{env, time::Instant};

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde_json;
use xz2::read::XzDecoder;
// use serde_jsonlines::{json_lines, JsonLinesReader};

use wptreport::{score_wpt_report, AreaScores, WptScores};

fn as_percent(amount: u32, out_of: u32) -> f32 {
    (amount as f32 / out_of as f32) * 100.0
}

fn main() {
    let args = env::args();
    let in_path = args.skip(1).next().unwrap();
    let in_path_buf = PathBuf::from(&in_path);

    let start = Instant::now();

    if in_path_buf.is_file() {
        let result = score_report(&in_path);
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

        let count = file_paths.len();
        let i = AtomicU64::new(0);

        file_paths.par_iter().for_each(|file_path| {
            let result = score_report(&file_path);
            let file_name = &file_path.rsplit_once('/').unwrap().1;
            let i = i.fetch_add(1, Ordering::SeqCst);
            println!(
                "[{i}/{count}] Processed {file_name} in {}ms (read in {}ms; Scored in {}ms)",
                result.total_time, result.read_time, result.score_time
            );
        });

        let grand_total_time = start.elapsed().as_secs();
        println!("====================");
        println!("Processed all files in {grand_total_time}s");
    } else {
        panic!("{in_path} is not a file or directory");
    }
}

pub struct ScoreResult {
    scores_by_area: BTreeMap<String, AreaScores>,
    read_time: u128,
    score_time: u128,
    total_time: u128,
}

pub fn score_report(file_path: &str) -> ScoreResult {
    let read_start = Instant::now();

    let file = File::open(&file_path).unwrap();

    let report: WptScores = if file_path.ends_with("xz") {
        let mut decompressed = XzDecoder::new(file);
        let mut s = String::new();
        decompressed.read_to_string(&mut s).unwrap();
        serde_json::from_str(&s).unwrap()
    } else if file_path.ends_with("zst") {
        let mut decompressed = zstd::Decoder::new(file).unwrap();
        let mut s = String::new();
        decompressed.read_to_string(&mut s).unwrap();
        serde_json::from_str(&s).unwrap()

        // Code for decoding JSON directly from the stream. This is more memory efficient but
        // significantly slower.
        //
        // serde_json::from_reader(&mut decompressed).unwrap()
    } else {
        let mut buffered = BufReader::new(file);
        serde_json::from_reader(&mut buffered).unwrap()

        // Code for reading "json lines" instead of JSON
        //
        // let lines_reader = JsonLinesReader::new(&mut buffered);
        // lines_reader
        //     .read_all()
        //     .map(|result| result.unwrap())
        //     .collect()
    };
    let read_elapsed = read_start.elapsed().as_millis();

    let score_start = Instant::now();
    let scores_by_area = score_wpt_report(report.test_scores.iter());
    let score_elapsed = score_start.elapsed().as_millis();
    let total_elapsed = read_start.elapsed().as_millis();

    ScoreResult {
        scores_by_area,
        read_time: read_elapsed,
        score_time: score_elapsed,
        total_time: total_elapsed,
    }
}
