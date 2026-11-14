//! `.headers` files

use std::path::{Path, PathBuf};

use url::Url;

use crate::template::{template, Escape};

/// Load the headers from the neighbouring `__dir__{.sub}.headers` and
/// `<file>{.sub}.headers` files, in that order.
pub fn load_headers(
    wpt_dir: &Path,
    relative_path: &str,
    request_url: &Url,
) -> Vec<(String, String)> {
    let dir = Path::new(relative_path).parent().unwrap_or(Path::new(""));
    let mut headers = load_headers_file(wpt_dir, &dir.join("__dir__"), request_url);
    headers.extend(load_headers_file(
        wpt_dir,
        Path::new(relative_path),
        request_url,
    ));
    headers
}

fn load_headers_file(wpt_dir: &Path, path: &Path, request_url: &Url) -> Vec<(String, String)> {
    let with_suffix = |suffix: &str| {
        let mut path = path.as_os_str().to_owned();
        path.push(suffix);
        PathBuf::from(path)
    };
    let sub_path = with_suffix(".sub.headers");
    let (headers_path, use_sub) = if wpt_dir.join(&sub_path).exists() {
        (sub_path, true)
    } else {
        (with_suffix(".headers"), false)
    };
    let Ok(mut data) = std::fs::read(wpt_dir.join(&headers_path)) else {
        return Vec::new();
    };
    if use_sub {
        let relative_path = headers_path.to_string_lossy().replace('\\', "/");
        data = template(wpt_dir, &relative_path, request_url, &data, Escape::None);
    }
    String::from_utf8_lossy(&data)
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// The `charset` parameter of a `Content-Type` header value
pub(crate) fn content_type_charset(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|param| {
        let (key, value) = param.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches('"'))
    })
}
