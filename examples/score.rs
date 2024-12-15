use serde_json;
use std::env;
use std::fs::File;
use std::io::BufReader;
use wptreport::WptReport;
use xz2::read::XzDecoder;

fn main() {
    let args = env::args();
    let file_path = args.skip(1).next().unwrap();
    let file = File::open(&file_path).unwrap();

    let report: WptReport = if file_path.ends_with("xz") {
        let mut decompressed = XzDecoder::new(file);
        serde_json::from_reader(&mut decompressed).unwrap()
    } else {
        let mut buffered = BufReader::new(file);
        serde_json::from_reader(&mut buffered).unwrap()
    };

    dbg!(report);
}
