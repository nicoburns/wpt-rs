//! Parsing of `// META: key=value` (JS) and `# META: key=value` (Python) script headers.

/// Python's `\s` for bytes patterns, excluding `\n` (lines are split on `\n`).
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\x0b' | b'\x0c')
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Equivalent to matching `<comment>\s*META:\s*(\w*)=(.*)$` against a line.
fn parse_line<'a>(line: &'a [u8], comment: &[u8]) -> Option<(&'a [u8], &'a [u8])> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let mut rest = line.strip_prefix(comment)?;
    while let [b, tail @ ..] = rest {
        if !is_space(*b) {
            break;
        }
        rest = tail;
    }
    rest = rest.strip_prefix(b"META:")?;
    while let [b, tail @ ..] = rest {
        if !is_space(*b) {
            break;
        }
        rest = tail;
    }
    let key_len = rest.iter().take_while(|b| is_word(**b)).count();
    let (key, rest) = rest.split_at(key_len);
    let value = rest.strip_prefix(b"=")?;
    Some((key, value))
}

/// Reads the leading block of META lines. Stops at the first non-matching line.
pub fn read_script_metadata(contents: &[u8], comment: &[u8]) -> Vec<(String, String)> {
    let mut rv = Vec::new();
    for line in contents.split_inclusive(|b| *b == b'\n') {
        let Some((key, value)) = parse_line(line, comment) else {
            break;
        };
        rv.push((
            String::from_utf8_lossy(key).into_owned(),
            String::from_utf8_lossy(value).into_owned(),
        ));
    }
    rv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js() {
        let src = b"// META: global=window,worker\n//META:script=/a.js\r\n// META: title=x=y\nfoo();\n// META: timeout=long\n";
        assert_eq!(
            read_script_metadata(src, b"//"),
            vec![
                ("global".into(), "window,worker".into()),
                ("script".into(), "/a.js\r".into()),
                ("title".into(), "x=y".into()),
            ]
        );
        assert!(read_script_metadata(b"// META: foo-bar=1\n", b"//").is_empty());
    }

    #[test]
    fn python() {
        assert_eq!(
            read_script_metadata(b"# META: timeout=long\nimport x\n", b"#"),
            vec![("timeout".into(), "long".into())]
        );
    }
}
