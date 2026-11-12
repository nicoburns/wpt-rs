//! Emulation of the parts of wptserve's static file handler which change what a
//! test sees: `.sub.` template substitution and `.headers` files, for test
//! runners which load WPT files from disk instead of running wptserve.
//!
//! See `tools/wptserve/wptserve/{handlers,pipes}.py` in the WPT repository.

use std::path::Path;

use url::Url;

mod config;
mod headers;
mod template;

pub use config::{ALT_HOST, BROWSER_HOST, ORIGIN};
pub use headers::load_headers;
pub use template::{template, Escape};

/// A file as wptserve would serve it
pub struct Response {
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
}

impl Response {
    /// The value of the last header called `name` (case-insensitive)
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// The text of the response, decoded using the charset from its
    /// `Content-Type` header (with BOM sniffing) if it has one, else as UTF-8.
    /// Returns `None` for a response without a charset which isn't valid UTF-8.
    pub fn text(self) -> Option<String> {
        let encoding = self
            .header("Content-Type")
            .and_then(headers::content_type_charset)
            .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()));
        match encoding {
            Some(encoding) => Some(encoding.decode(&self.body).0.into_owned()),
            None => String::from_utf8(self.body).ok(),
        }
    }
}

/// Serve the file at `relative_path` (relative to the WPT root, no query) for a
/// request to `request_url`, applying the file's `.headers` and `.sub.`
/// substitution (or a `?pipe=sub` query) like wptserve.
pub fn serve(wpt_dir: &Path, relative_path: &str, request_url: &Url) -> std::io::Result<Response> {
    let mut body = std::fs::read(wpt_dir.join(relative_path))?;
    let headers = load_headers(wpt_dir, relative_path, request_url);
    if let Some(escape) = template::sub_escape(relative_path, request_url) {
        body = template(wpt_dir, relative_path, request_url, &body, escape);
    }
    Ok(Response { body, headers })
}

/// Whether wptserve would serve `relative_path` (a WPT-root-relative path,
/// optionally with a query) with an XML content type. `None` if its `.headers`
/// don't set a `Content-Type`, in which case the file extension decides.
pub fn headers_say_xml(wpt_dir: &Path, relative_path: &str, request_url: &Url) -> Option<bool> {
    let path = relative_path
        .split(['?', '#'])
        .next()
        .unwrap_or(relative_path);
    let headers = load_headers(wpt_dir, path, request_url);
    let content_type = headers
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("Content-Type"))?;
    let mime = content_type
        .1
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    Some(mime.ends_with("+xml") || mime == "application/xml" || mime == "text/xml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_headers_files() {
        let dir = tempfile::tempdir().unwrap();
        let css = dir.path().join("css");
        std::fs::create_dir(&css).unwrap();
        std::fs::write(css.join("__dir__.headers"), "X-Dir: 1\n").unwrap();
        std::fs::write(css.join("a.xht"), b"caf\xe9").unwrap();
        std::fs::write(
            css.join("a.xht.sub.headers"),
            "Content-Type: application/xhtml+xml; charset=iso-8859-1\nX-Host: {{host}}\n",
        )
        .unwrap();
        std::fs::write(css.join("b.html"), b"caf\xe9").unwrap();
        std::fs::write(css.join("c.sub.html"), "<p title='{{GET[t]}}'>{{host}}").unwrap();

        let url = Url::parse(ORIGIN).unwrap().join("css/a.xht").unwrap();
        let response = serve(dir.path(), "css/a.xht", &url).unwrap();
        assert_eq!(response.header("x-dir"), Some("1"));
        assert_eq!(response.header("X-Host"), Some(BROWSER_HOST));
        assert_eq!(response.text().as_deref(), Some("café"));
        assert_eq!(headers_say_xml(dir.path(), "css/a.xht?x", &url), Some(true));

        let url = Url::parse(ORIGIN).unwrap().join("css/b.html").unwrap();
        assert_eq!(serve(dir.path(), "css/b.html", &url).unwrap().text(), None);
        assert_eq!(headers_say_xml(dir.path(), "css/b.html", &url), None);

        let url = Url::parse(ORIGIN)
            .unwrap()
            .join("css/c.sub.html?t=<'>")
            .unwrap();
        let response = serve(dir.path(), "css/c.sub.html", &url).unwrap();
        assert_eq!(
            response.text().as_deref(),
            Some("<p title='&lt;&#x27;&gt;'>web-platform.test")
        );
    }
}
