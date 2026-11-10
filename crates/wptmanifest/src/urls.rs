//! URL resolution for test and reference URLs, which WPT computes on path-only URLs.
//!
//! Python's `urllib.parse.urljoin` differs from WHATWG URL parsing in edge cases (it
//! doesn't percent-encode spaces or non-ASCII, and keeps backslashes and `..` above the
//! root), but no file in WPT hits them, so the manifest output is the same.

use url::{Position, Url};

/// Placeholder origin that path-only URLs are resolved against.
const ORIGIN: &str = "http://wpt.invalid/";

fn origin() -> Url {
    Url::parse(ORIGIN).expect("valid origin")
}

fn is_placeholder_origin(url: &Url) -> bool {
    url.scheme() == "http" && url.host_str() == Some("wpt.invalid") && url.port().is_none()
}

/// Resolves `url` against the path-only `base`. URLs on the base's origin come back
/// path-only, like Python's `urljoin("/a/b.html", "c.html") == "/a/c.html"`.
pub fn urljoin(base: &str, url: &str) -> String {
    let Ok(base) = origin().join(base) else {
        return url.to_string();
    };
    match base.join(url) {
        Ok(joined) if is_placeholder_origin(&joined) => joined[Position::BeforePath..].to_string(),
        Ok(joined) => joined.to_string(),
        Err(_) => url.to_string(),
    }
}

/// The non-empty values of query parameter `name` in `url`, like
/// `parse_qs(urlparse(url).query).get(name, [])`.
pub fn query_values(url: &str, name: &str) -> Vec<String> {
    let Ok(url) = origin().join(url) else {
        return Vec::new();
    };
    url.query_pairs()
        .filter(|(n, v)| n == name && !v.is_empty())
        .map(|(_, v)| v.into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join() {
        assert_eq!(urljoin("/", "a/b.html"), "/a/b.html");
        assert_eq!(urljoin("/a/b/c.html", "d.html"), "/a/b/d.html");
        assert_eq!(urljoin("/a/b/c.html", "../d.html"), "/a/d.html");
        assert_eq!(urljoin("/a/b/c.html", "/x.html"), "/x.html");
        assert_eq!(urljoin("/a/b/c.html", "?q"), "/a/b/c.html?q");
        assert_eq!(urljoin("/a/b/c.html", "#f"), "/a/b/c.html#f");
        assert_eq!(urljoin("/a/b/c.html", "."), "/a/b/");
        assert_eq!(urljoin("/a/b/c.html", "d.html;p?q#f"), "/a/b/d.html;p?q#f");
        assert_eq!(urljoin("/a/b/c.html", "about:blank"), "about:blank");
        assert_eq!(urljoin("/a/b/c.html", "http://x/y"), "http://x/y");
    }

    #[test]
    fn query() {
        assert_eq!(
            query_values(
                "/resources/testdriver.js?feature=bidi&feature=a%20b&x=1&feature=",
                "feature"
            ),
            vec!["bidi", "a b"]
        );
        assert!(query_values("/resources/testdriver.js", "feature").is_empty());
    }
}
