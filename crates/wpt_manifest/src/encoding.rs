//! Character encoding detection for HTML, ported from html5lib's `_inputstream.py`
//! (BOM sniffing, then the `<meta>` prescan of the first 1024 bytes, then windows-1252).

use encoding_rs::{Encoding, UTF_16BE, UTF_16LE, UTF_8, WINDOWS_1252};

const NUM_BYTES_META: usize = 1024;

fn is_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | b'\x0c' | b' ' | b'\r')
}

pub fn lookup_encoding(label: &[u8]) -> Option<&'static Encoding> {
    // webencodings strips ASCII whitespace and is case-insensitive, like encoding_rs.
    let label = std::str::from_utf8(label).ok()?;
    if !label.is_ascii() {
        return None;
    }
    Encoding::for_label(label.as_bytes())
}

/// Returns the encoding and the number of BOM bytes to skip.
pub fn detect_bom(data: &[u8]) -> Option<(&'static Encoding, usize)> {
    if data.starts_with(b"\xef\xbb\xbf") {
        Some((UTF_8, 3))
    } else if data.starts_with(b"\xff\xfe\x00\x00") || data.starts_with(b"\x00\x00\xfe\xff") {
        // UTF-32 isn't supported by encoding_rs (or webencodings); html5lib's lookup fails
        // and it falls back to the meta prescan, which we mimic by returning None.
        None
    } else if data.starts_with(b"\xff\xfe") {
        Some((UTF_16LE, 2))
    } else if data.starts_with(b"\xfe\xff") {
        Some((UTF_16BE, 2))
    } else {
        None
    }
}

/// Normalise an encoding found in markup: html5lib replaces UTF-16 with UTF-8.
fn normalise(encoding: &'static Encoding) -> &'static Encoding {
    if encoding == UTF_16BE || encoding == UTF_16LE {
        UTF_8
    } else {
        encoding
    }
}

pub struct Decoded {
    pub text: String,
    pub encoding: &'static Encoding,
    /// Whether the encoding came from a BOM (html5lib's "certain" confidence).
    pub certain: bool,
}

pub fn decode_html(data: &[u8]) -> Decoded {
    if let Some((encoding, skip)) = detect_bom(data) {
        return Decoded {
            text: decode_with(&data[skip..], encoding),
            encoding,
            certain: true,
        };
    }
    let encoding = prescan(&data[..data.len().min(NUM_BYTES_META)])
        .map(normalise)
        .unwrap_or(WINDOWS_1252);
    Decoded {
        text: decode_with(data, encoding),
        encoding,
        certain: false,
    }
}

pub fn decode_with(data: &[u8], encoding: &'static Encoding) -> String {
    encoding.decode_without_bom_handling(data).0.into_owned()
}

/// Encoding change requested by a `<meta>` element during parsing, mirroring html5lib's
/// `changeEncoding`: returns the new encoding if the document must be reparsed.
pub fn change_encoding(
    current: &'static Encoding,
    label: &str,
) -> Option<Option<&'static Encoding>> {
    let new = lookup_encoding(label.as_bytes())?;
    if new == UTF_16BE || new == UTF_16LE {
        // html5lib swaps in UTF-8 here but (due to its control flow) neither reparses nor
        // marks the encoding as certain, so later declarations are still considered.
        return None;
    }
    if new == current {
        Some(None)
    } else {
        Some(Some(new))
    }
}

/// Parse the `charset=` out of a `<meta http-equiv content>` value.
pub fn content_attr_charset(data: &[u8]) -> Option<&[u8]> {
    let mut pos = data.windows(7).position(|w| w == b"charset")? + 7;
    while pos < data.len() && is_space(data[pos]) {
        pos += 1;
    }
    if pos >= data.len() || data[pos] != b'=' {
        return None;
    }
    pos += 1;
    while pos < data.len() && is_space(data[pos]) {
        pos += 1;
    }
    if pos >= data.len() {
        return None;
    }
    let c = data[pos];
    if c == b'"' || c == b'\'' {
        let start = pos + 1;
        let end = data[start..].iter().position(|b| *b == c)? + start;
        Some(&data[start..end])
    } else {
        let start = pos;
        let end = data[start..]
            .iter()
            .position(|b| is_space(*b))
            .map(|i| i + start)
            .unwrap_or(data.len());
        Some(&data[start..end])
    }
}

/// Port of html5lib's `EncodingParser`.
struct Prescan<'a> {
    data: &'a [u8],
    pos: usize,
    found: Option<&'static Encoding>,
}

impl Prescan<'_> {
    fn cur(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn jump_to(&mut self, needle: &[u8]) -> bool {
        match self.data[self.pos.min(self.data.len())..]
            .windows(needle.len())
            .position(|w| w == needle)
        {
            Some(i) => {
                self.pos += i + needle.len() - 1;
                true
            }
            None => false,
        }
    }

    fn match_bytes(&mut self, needle: &[u8]) -> bool {
        if self.data[self.pos.min(self.data.len())..].starts_with(needle) {
            self.pos += needle.len();
            true
        } else {
            false
        }
    }

    fn skip(&mut self, f: impl Fn(u8) -> bool) -> Option<u8> {
        while let Some(c) = self.cur() {
            if !f(c) {
                return Some(c);
            }
            self.pos += 1;
        }
        None
    }

    fn next(&mut self) -> Option<u8> {
        self.pos += 1;
        self.cur()
    }

    fn get_encoding(mut self) -> Option<&'static Encoding> {
        if !self.data.windows(5).any(|w| w == b"<meta") {
            return None;
        }
        loop {
            if !self.jump_to(b"<") {
                return None;
            }
            let keep_parsing = if self.match_bytes(b"<!--") {
                self.jump_to(b"-->")
            } else if self.match_bytes(b"<meta") {
                match self.handle_meta() {
                    Ok(true) => true,
                    Ok(false) => return self.found,
                    Err(()) => false,
                }
            } else if self.match_bytes(b"</") {
                self.handle_possible_tag(true)
            } else if self.match_bytes(b"<!") || self.match_bytes(b"<?") {
                self.jump_to(b">")
            } else if self.match_bytes(b"<") {
                self.handle_possible_tag(false)
            } else {
                true
            };
            // html5lib's `for _ in self.data` advances one byte per iteration.
            self.pos += 1;
            if !keep_parsing || self.pos >= self.data.len() {
                return self.found;
            }
        }
    }

    fn handle_meta(&mut self) -> Result<bool, ()> {
        match self.cur() {
            Some(c) if is_space(c) => {}
            Some(_) => return Ok(true),
            None => return Err(()),
        }
        let mut has_pragma = false;
        let mut pending = None;
        loop {
            let Some((name, value)) = self.get_attribute()? else {
                return Ok(true);
            };
            match &name[..] {
                b"http-equiv" => {
                    has_pragma = value == b"content-type";
                    if has_pragma && pending.is_some() {
                        self.found = pending;
                        return Ok(false);
                    }
                }
                b"charset" => {
                    if let Some(enc) = lookup_encoding(&value) {
                        self.found = Some(enc);
                        return Ok(false);
                    }
                }
                b"content" => {
                    if let Some(enc) = content_attr_charset(&value).and_then(lookup_encoding) {
                        if has_pragma {
                            self.found = Some(enc);
                            return Ok(false);
                        }
                        pending = Some(enc);
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_possible_tag(&mut self, end_tag: bool) -> bool {
        if end_tag {
            // html5lib calls `next(self.data)` after matching `</`, skipping one more byte.
            self.pos += 1;
        }
        match self.cur() {
            Some(c) if c.is_ascii_alphabetic() => {}
            _ => {
                if end_tag {
                    return self.jump_to(b">");
                }
                return true;
            }
        }
        let c = self.skip(|b| !(is_space(b) || b == b'<' || b == b'>'));
        if c == Some(b'<') {
            self.pos -= 1;
        } else {
            while let Ok(Some(_)) = self.get_attribute() {}
        }
        true
    }

    /// `Err` corresponds to html5lib hitting the end of the data (StopIteration).
    #[allow(clippy::type_complexity)]
    fn get_attribute(&mut self) -> Result<Option<(Vec<u8>, Vec<u8>)>, ()> {
        let mut c = self.skip(|b| is_space(b) || b == b'/');
        if matches!(c, Some(b'>') | None) {
            return Ok(None);
        }
        let mut name = Vec::new();
        let mut value = Vec::new();
        loop {
            match c {
                Some(b'=') if !name.is_empty() => break,
                Some(b) if is_space(b) => {
                    c = self.skip(is_space);
                    break;
                }
                Some(b'/') | Some(b'>') => return Ok(Some((name, Vec::new()))),
                Some(b) => name.push(b.to_ascii_lowercase()),
                None => return Ok(None),
            }
            c = self.next();
        }
        if c != Some(b'=') {
            self.pos -= 1;
            return Ok(Some((name, Vec::new())));
        }
        self.next();
        c = self.skip(is_space);
        match c {
            Some(q @ (b'\'' | b'"')) => loop {
                match self.next() {
                    Some(b) if b == q => {
                        self.next();
                        return Ok(Some((name, value)));
                    }
                    Some(b) => value.push(b.to_ascii_lowercase()),
                    None => return Err(()),
                }
            },
            Some(b'>') => return Ok(Some((name, Vec::new()))),
            Some(b) => value.push(b.to_ascii_lowercase()),
            None => return Ok(None),
        }
        loop {
            match self.next() {
                Some(b) if is_space(b) || b == b'<' || b == b'>' => return Ok(Some((name, value))),
                Some(b) => value.push(b.to_ascii_lowercase()),
                None => return Ok(None),
            }
        }
    }
}

fn prescan(data: &[u8]) -> Option<&'static Encoding> {
    Prescan {
        data,
        pos: 0,
        found: None,
    }
    .get_encoding()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta() {
        assert_eq!(prescan(b"<!doctype html><meta charset=utf-8>"), Some(UTF_8));
        assert_eq!(
            prescan(b"<meta http-equiv=\"Content-Type\" content=\"text/html; charset=shift_jis\">"),
            Some(encoding_rs::SHIFT_JIS)
        );
        assert_eq!(prescan(b"<!-- <meta charset=utf-8> --><p>"), None);
        assert_eq!(prescan(b"<title>x</title>"), None);
        assert_eq!(decode_html(b"<p>\xe9").text, "<p>\u{e9}");
        assert_eq!(decode_html(b"\xef\xbb\xbf<p>\xc3\xa9").text, "<p>\u{e9}");
    }
}
