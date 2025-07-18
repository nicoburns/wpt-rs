use std::fs::{self, File};
use std::io::Read;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use xz2::read::XzDecoder;

pub fn read_maybe_compressed_file(file_path: &Path) -> String {
    let file = File::open(file_path).unwrap();
    let extension = file_path.extension().unwrap().as_bytes();

    match extension {
        b"xz" => {
            let mut decompressed = XzDecoder::new(file);
            let mut s = String::new();
            decompressed.read_to_string(&mut s).unwrap();
            s
        }
        b"zst" => {
            let mut decompressed = zstd::Decoder::new(file).unwrap();
            let mut s = String::new();
            decompressed.read_to_string(&mut s).unwrap();
            s
        }
        _ => fs::read_to_string(file_path).unwrap(),
    }
}
