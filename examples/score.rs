use serde_json;
// use serde_jsonlines::{json_lines, JsonLinesReader};

use std::fs::{read_dir, File};
use std::hint::black_box;
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::{env, time::Instant};
use wptreport::{score_wpt_report, WptScores};
use xz2::read::XzDecoder;

fn as_percent(amount: u32, out_of: u32) -> f32 {
    (amount as f32 / out_of as f32) * 100.0
}

fn main() {
    let args = env::args();
    let in_path = args.skip(1).next().unwrap();
    let in_path_buf = PathBuf::from(&in_path);

    let start = Instant::now();

    if in_path_buf.is_file() {
        score_report(&in_path);
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

        for file_path in file_paths {
            score_report(&file_path);
        }
    } else {
        panic!("{in_path} is not a file or directory");
    }

    let grand_total_time = start.elapsed().as_secs();
    println!("====================");
    println!("Processed all files in {grand_total_time}");
}

fn score_report(file_path: &str) {
    println!("Processing {file_path}");
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

    black_box(&scores_by_area);
    // for (area, scores) in scores_by_area {
    //     let tests = scores.tests;
    //     let subtests = scores.subtests;
    //     let percentage = as_percent(scores.tests.pass, scores.tests.total);
    //     println!(
    //         "{area}: {percentage:.2}% ({}/{} tests) ({}/{} subtests)",
    //         tests.pass, tests.total, subtests.pass, subtests.total
    //     );
    // }

    println!("Done in {total_elapsed}ms (read in {read_elapsed}ms; Scored in {score_elapsed}ms).");
}
