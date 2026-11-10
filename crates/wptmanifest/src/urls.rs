//! Ports of the parts of Python's `urllib.parse` (3.10) that affect manifest output.
//!
//! WPT computes test and reference URLs with `urljoin` on path-only URLs. Python's
//! semantics differ from the WHATWG URL standard (no percent-encoding, no
//! normalisation of absolute URLs), so we port them rather than using the `url` crate.

const USES_RELATIVE: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "imap", "wais", "file", "https", "shttp", "mms",
    "prospero", "rtsp", "rtspu", "sftp", "svn", "svn+ssh", "ws", "wss",
];
const USES_NETLOC: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "telnet", "imap", "wais", "file", "mms", "https", "shttp",
    "snews", "prospero", "rtsp", "rtspu", "rsync", "svn", "svn+ssh", "sftp", "nfs", "git",
    "git+ssh", "ws", "wss",
];
const USES_PARAMS: &[&str] = &[
    "", "ftp", "hdl", "prospero", "http", "imap", "https", "shttp", "rtsp", "rtspu", "sip", "sips",
    "mms", "sftp", "tel",
];

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

/// The result of `urlparse`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    pub params: String,
    pub query: String,
    pub fragment: String,
}

/// Port of `urllib.parse.urlsplit`, returning (scheme, netloc, path, query, fragment).
fn urlsplit(url: &str, default_scheme: &str) -> (String, String, String, String, String) {
    let url = url.trim_start_matches(|c: char| c <= ' ');
    let mut url: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let mut scheme = default_scheme.to_string();
    let mut netloc = String::new();
    let mut query = String::new();
    let mut fragment = String::new();

    if let Some(i) = url.find(':') {
        if i > 0 && url[..i].chars().all(is_scheme_char) {
            scheme = url[..i].to_ascii_lowercase();
            url = url[i + 1..].to_string();
        }
    }
    if url.starts_with("//") {
        let rest = &url[2..];
        let delim = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        netloc = rest[..delim].to_string();
        url = rest[delim..].to_string();
    }
    if let Some((u, f)) = url.split_once('#') {
        fragment = f.to_string();
        url = u.to_string();
    }
    if let Some((u, q)) = url.split_once('?') {
        query = q.to_string();
        url = u.to_string();
    }
    (scheme, netloc, url, query, fragment)
}

fn splitparams(url: &str) -> (String, String) {
    let i = if let Some(slash) = url.rfind('/') {
        match url[slash..].find(';') {
            Some(i) => slash + i,
            None => return (url.to_string(), String::new()),
        }
    } else {
        url.find(';').unwrap()
    };
    (url[..i].to_string(), url[i + 1..].to_string())
}

/// Port of `urllib.parse.urlparse`.
pub fn urlparse(url: &str, default_scheme: &str) -> ParseResult {
    let (scheme, netloc, mut path, query, fragment) = urlsplit(url, default_scheme);
    let mut params = String::new();
    if USES_PARAMS.contains(&scheme.as_str()) && path.contains(';') {
        (path, params) = splitparams(&path);
    }
    ParseResult {
        scheme,
        netloc,
        path,
        params,
        query,
        fragment,
    }
}

fn urlunparse(p: &ParseResult) -> String {
    let mut url = p.path.clone();
    if !p.params.is_empty() {
        url = format!("{url};{}", p.params);
    }
    if !p.netloc.is_empty()
        || (!p.scheme.is_empty()
            && USES_NETLOC.contains(&p.scheme.as_str())
            && !url.starts_with("//"))
    {
        if !url.is_empty() && !url.starts_with('/') {
            url = format!("/{url}");
        }
        url = format!("//{}{url}", p.netloc);
    }
    if !p.scheme.is_empty() {
        url = format!("{}:{url}", p.scheme);
    }
    if !p.query.is_empty() {
        url = format!("{url}?{}", p.query);
    }
    if !p.fragment.is_empty() {
        url = format!("{url}#{}", p.fragment);
    }
    url
}

/// Port of `urllib.parse.urljoin`.
pub fn urljoin(base: &str, url: &str) -> String {
    if base.is_empty() {
        return url.to_string();
    }
    if url.is_empty() {
        return base.to_string();
    }
    let b = urlparse(base, "");
    let mut u = urlparse(url, &b.scheme);

    if u.scheme != b.scheme || !USES_RELATIVE.contains(&u.scheme.as_str()) {
        return url.to_string();
    }
    if USES_NETLOC.contains(&u.scheme.as_str()) {
        if !u.netloc.is_empty() {
            return urlunparse(&u);
        }
        u.netloc = b.netloc.clone();
    }

    if u.path.is_empty() && u.params.is_empty() {
        u.path = b.path.clone();
        u.params = b.params.clone();
        if u.query.is_empty() {
            u.query = b.query.clone();
        }
        return urlunparse(&u);
    }

    let mut base_parts: Vec<&str> = b.path.split('/').collect();
    if base_parts.last() != Some(&"") {
        base_parts.pop();
    }

    let segments: Vec<&str> = if u.path.starts_with('/') {
        u.path.split('/').collect()
    } else {
        let mut segments = base_parts;
        segments.extend(u.path.split('/'));
        // Drop empty segments between the first and last ones.
        if segments.len() > 2 {
            let last = segments.len() - 1;
            let mut filtered = vec![segments[0]];
            filtered.extend(segments[1..last].iter().filter(|s| !s.is_empty()));
            filtered.push(segments[last]);
            segments = filtered;
        }
        segments
    };

    let mut resolved: Vec<&str> = Vec::new();
    for seg in &segments {
        match *seg {
            ".." => {
                resolved.pop();
            }
            "." => {}
            _ => resolved.push(seg),
        }
    }
    if matches!(segments.last(), Some(&".") | Some(&"..")) {
        resolved.push("");
    }
    let joined = resolved.join("/");
    u.path = if joined.is_empty() {
        "/".to_string()
    } else {
        joined
    };
    urlunparse(&u)
}

/// Port of `urllib.parse.unquote` (UTF-8, errors="replace").
pub fn unquote(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
            out.push(u8::from_str_radix(hex, 16).unwrap());
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Port of `urllib.parse.parse_qs(qs)["name"]` with default arguments.
pub fn parse_qs_values(qs: &str, name: &str) -> Vec<String> {
    let mut rv = Vec::new();
    for name_value in qs.split('&') {
        if name_value.is_empty() {
            continue;
        }
        let Some((n, v)) = name_value.split_once('=') else {
            continue;
        };
        if v.is_empty() {
            continue;
        }
        if unquote(&n.replace('+', " ")) == name {
            rv.push(unquote(&v.replace('+', " ")));
        }
    }
    rv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join() {
        assert_eq!(urljoin("/a/b/c.html", "d.html"), "/a/b/d.html");
        assert_eq!(urljoin("/a/b/c.html", "../d.html"), "/a/d.html");
        // Python's urljoin drops the leading `/` when `..` goes above the root.
        assert_eq!(urljoin("/a/b/c.html", "../../../d.html"), "d.html");
        assert_eq!(urljoin("/a/b/c.html", "/../x.html"), "x.html");
        assert_eq!(urljoin("/a/b/c.html", "./../x/./y/.."), "/a/x/");
        assert_eq!(urljoin("/a/b/c.html", "/x.html"), "/x.html");
        assert_eq!(urljoin("/a/b/c.html", "about:blank"), "about:blank");
        assert_eq!(urljoin("/a/b/c.html", "http://x/y"), "http://x/y");
        assert_eq!(urljoin("/a/b/c.html", "//x/y"), "//x/y");
        assert_eq!(urljoin("/a/b/c.html", "?q"), "/a/b/c.html?q");
        assert_eq!(urljoin("/a/b/c.html", "#f"), "/a/b/c.html#f");
        assert_eq!(urljoin("/a/b/c.html", "."), "/a/b/");
        assert_eq!(urljoin("/a/b/c.html", "x//y.html"), "/a/b/x/y.html");
        assert_eq!(urljoin("/", "a/b.html"), "/a/b.html");
        assert_eq!(urljoin("/a/b/c.html", "d.html;p?q#f"), "/a/b/d.html;p?q#f");
    }

    #[test]
    fn qs() {
        assert_eq!(
            parse_qs_values("feature=bidi&feature=a%20b&x=1&feature=", "feature"),
            vec!["bidi", "a b"]
        );
    }
}
