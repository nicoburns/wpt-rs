//! wptserve's pipes, which a `?pipe=` query (or a `.sub.` file name) applies to
//! a response. `trickle` only changes when the bytes arrive, so it is ignored.

use std::io::Write;
use std::ops::Range;
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;
use log::warn;
use url::Url;

use crate::template::parser::python_int;
use crate::template::{template, Escape};
use crate::Response;

/// Apply the pipes in `pipeline` (e.g. `status(404)|header(X-Test,1)`) to
/// `response`, in order. A pipe wptserve would reject is skipped with a warning.
pub(crate) fn apply(
    pipeline: &str,
    response: &mut Response,
    wpt_dir: &Path,
    relative_path: &str,
    request_url: &Url,
) {
    for (name, args) in parse(pipeline) {
        let result = match name.as_str() {
            "status" => status(response, &args),
            "header" => header(response, &args),
            "slice" => slice(response, &args),
            "sub" => sub(response, &args, wpt_dir, relative_path, request_url),
            "gzip" => gzip(response, &args),
            "trickle" => arg_count(&args, 1, 1),
            _ => Err("unknown pipe".to_string()),
        };
        if let Err(err) = result {
            warn!(
                "Ignoring pipe {name}({}) for {relative_path}: {err}",
                args.join(",")
            );
        }
    }
}

/// Split a pipeline into pipe names and their arguments, like wptserve's
/// `PipeTokenizer`
pub(crate) fn parse(pipeline: &str) -> Vec<(String, Vec<String>)> {
    let mut pipes = Vec::new();
    let mut chars = pipeline.chars();
    loop {
        let mut name = String::new();
        loop {
            match chars.next() {
                None => {
                    if !name.is_empty() {
                        pipes.push((name, Vec::new()));
                    }
                    return pipes;
                }
                Some('(') => break,
                Some('|') if !name.is_empty() => {
                    pipes.push((std::mem::take(&mut name), Vec::new()))
                }
                Some('|') => {}
                Some(c) => name.push(c),
            }
        }
        let mut args = Vec::new();
        let mut arg = String::new();
        let closed = loop {
            match chars.next() {
                None => break false,
                Some('\\') => match chars.next() {
                    Some('n') => arg.push('\n'),
                    Some('r') => arg.push('\r'),
                    Some('t') => arg.push('\t'),
                    Some(c) => arg.push(c),
                    None => break false,
                },
                Some(',') => args.push(std::mem::take(&mut arg)),
                Some(')') => break true,
                Some(c) => arg.push(c),
            }
        };
        args.push(arg);
        pipes.push((name, args));
        if !closed {
            return pipes;
        }
    }
}

fn arg_count(args: &[String], min: usize, max: usize) -> Result<(), String> {
    if (min..=max).contains(&args.len()) {
        Ok(())
    } else {
        Err(format!(
            "expected between {min} and {max} args, got {}",
            args.len()
        ))
    }
}

fn int(arg: &str) -> Result<i64, String> {
    python_int(arg).ok_or_else(|| format!("{arg:?} is not an integer"))
}

fn nullable_int(arg: &str) -> Result<Option<i64>, String> {
    if arg.eq_ignore_ascii_case("null") {
        Ok(None)
    } else {
        int(arg).map(Some)
    }
}

fn boolean(arg: &str) -> Result<bool, String> {
    match arg.to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(format!("{arg:?} is not a boolean")),
    }
}

fn status(response: &mut Response, args: &[String]) -> Result<(), String> {
    arg_count(args, 1, 1)?;
    let code = int(&args[0])?;
    response.status = u16::try_from(code).map_err(|_| format!("bad status {code}"))?;
    Ok(())
}

fn header(response: &mut Response, args: &[String]) -> Result<(), String> {
    arg_count(args, 2, 3)?;
    let append = args.get(2).map(|arg| boolean(arg)).transpose()?;
    if append == Some(true) {
        response.headers.push((args[0].clone(), args[1].clone()));
    } else {
        response.set_header(&args[0], args[1].clone());
    }
    Ok(())
}

fn slice(response: &mut Response, args: &[String]) -> Result<(), String> {
    arg_count(args, 1, 2)?;
    let start = nullable_int(&args[0])?;
    let end = args
        .get(1)
        .map(|arg| nullable_int(arg))
        .transpose()?
        .flatten();
    let range = python_slice(response.body.len(), start, end);
    response.body = response.body[range].to_vec();
    response.set_header("Content-Length", response.body.len().to_string());
    Ok(())
}

/// The range Python's `[start:end]` selects from a sequence of length `len`
fn python_slice(len: usize, start: Option<i64>, end: Option<i64>) -> Range<usize> {
    let len = len as i64;
    let clamp = |index: i64| {
        if index < 0 {
            (index + len).max(0)
        } else {
            index.min(len)
        }
    };
    let start = start.map_or(0, clamp);
    let end = end.map_or(len, clamp).max(start);
    start as usize..end as usize
}

fn sub(
    response: &mut Response,
    args: &[String],
    wpt_dir: &Path,
    relative_path: &str,
    request_url: &Url,
) -> Result<(), String> {
    arg_count(args, 0, 1)?;
    let escape = match args.first().map(String::as_str) {
        None | Some("html") => Escape::Html,
        Some("none") => Escape::None,
        Some(other) => return Err(format!("unknown escape type {other:?}")),
    };
    response.body = template(wpt_dir, relative_path, request_url, &response.body, escape);
    Ok(())
}

fn gzip(response: &mut Response, args: &[String]) -> Result<(), String> {
    arg_count(args, 0, 0)?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&response.body)
        .map_err(|err| err.to_string())?;
    response.body = encoder.finish().map_err(|err| err.to_string())?;
    response.set_header("Content-Encoding", "gzip".to_string());
    response.set_header("Content-Length", response.body.len().to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipe(name: &str, args: &[&str]) -> (String, Vec<String>) {
        (
            name.to_string(),
            args.iter().map(|arg| arg.to_string()).collect(),
        )
    }

    #[test]
    fn parses_like_wptserve() {
        assert_eq!(
            parse("status(302)|header(Location,/a)|sub"),
            [
                pipe("status", &["302"]),
                pipe("header", &["Location", "/a"]),
                pipe("sub", &[])
            ]
        );
        assert_eq!(
            parse(r"header(Expires,Thu\, 14 Aug)|gzip"),
            [
                pipe("header", &["Expires", "Thu, 14 Aug"]),
                pipe("gzip", &[])
            ]
        );
        assert_eq!(
            parse(r"header(User-Agent,a (b\))"),
            [pipe("header", &["User-Agent", "a (b)"])]
        );
        assert_eq!(parse("sub(html)|"), [pipe("sub", &["html"])]);
        assert_eq!(
            parse("header(Content-Type,)"),
            [pipe("header", &["Content-Type", ""])]
        );
        assert_eq!(parse(""), []);
    }

    #[test]
    fn slices_like_python() {
        assert_eq!(python_slice(10, Some(1), Some(5)), 1..5);
        assert_eq!(python_slice(10, None, Some(-2)), 0..8);
        assert_eq!(python_slice(10, Some(-3), None), 7..10);
        assert_eq!(python_slice(10, Some(20), Some(30)), 10..10);
        assert_eq!(python_slice(10, Some(6), Some(2)), 6..6);
    }
}
