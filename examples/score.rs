use serde_json;
// use serde_jsonlines::{json_lines, JsonLinesReader};

use std::fs::File;
use std::hint::black_box;
use std::io::BufReader;
use std::{env, time::Instant};
use wptreport::{WptReport, WptScores};
use xz2::read::XzDecoder;

fn main() {
    let args = env::args();
    let file_path = args.skip(1).next().unwrap();
    let file = File::open(&file_path).unwrap();

    let start = Instant::now();

    let report: WptReport = if file_path.ends_with("xz") {
        let mut decompressed = XzDecoder::new(file);
        serde_json::from_reader(&mut decompressed).unwrap()
        // let lines_reader = JsonLinesReader::new(BufReader::new(&mut decompressed));
        // lines_reader
        //     .read_all()
        //     .map(|result| result.unwrap())
        //     .collect()
    } else {
        let mut buffered = BufReader::new(file);
        serde_json::from_reader(&mut buffered).unwrap()
        // let lines_reader = JsonLinesReader::new(&mut buffered);
        // lines_reader
        //     .read_all()
        //     .map(|result| result.unwrap())
        //     .collect()
    };

    black_box(report);

    let elapsed = start.elapsed().as_millis();
    println!("Read in {elapsed}ms");
}
