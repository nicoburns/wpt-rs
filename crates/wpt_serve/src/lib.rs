//! Emulation of the parts of wptserve's static file handler which change what a
//! test sees: `.headers` files and pipes (`.sub.` template substitution and
//! `?pipe=` queries), for test runners which load WPT files from disk instead
//! of running wptserve.
//!
//! See `tools/wptserve/wptserve/{handlers,pipes}.py` in the WPT repository.

use std::borrow::Cow;
use std::io::Read;
use std::path::Path;

use url::Url;

mod config;
mod headers;
mod pipes;
mod template;

pub use config::{ALT_HOST, BROWSER_HOST, ORIGIN};
pub use headers::load_headers;
pub use template::{template, Escape};

/// A file as wptserve would serve it
pub struct Response {
    /// The HTTP status code: 200 unless set by the `status` pipe
    pub status: u16,
    /// The body as sent, before undoing any `Content-Encoding` (see
    /// [`Response::content`])
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

    /// Replace any headers called `name` (case-insensitive) with one header
    pub fn set_header(&mut self, name: &str, value: String) {
        self.headers
            .retain(|(key, _)| !key.eq_ignore_ascii_case(name));
        self.headers.push((name.to_string(), value));
    }

    /// The body as a browser sees it: decompressed if the response has a
    /// `Content-Encoding: gzip` header (from the `gzip` pipe or a `.headers`
    /// file)
    pub fn content(&self) -> std::io::Result<Cow<'_, [u8]>> {
        match self.header("Content-Encoding") {
            Some(encoding) if encoding.trim().eq_ignore_ascii_case("gzip") => {
                let mut content = Vec::new();
                flate2::read::GzDecoder::new(&self.body[..]).read_to_end(&mut content)?;
                Ok(Cow::Owned(content))
            }
            _ => Ok(Cow::Borrowed(&self.body)),
        }
    }

    /// Whether the response has an XML `Content-Type`. `None` if it has no
    /// `Content-Type` header, in which case wptserve picks the content type from
    /// the file extension.
    pub fn is_xml(&self) -> Option<bool> {
        let mime = self
            .header("Content-Type")?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        Some(mime.ends_with("+xml") || mime == "application/xml" || mime == "text/xml")
    }

    /// The text of the response, decoded using the charset from its
    /// `Content-Type` header (with BOM sniffing) if it has one, else as UTF-8.
    /// Returns `None` for a response without a charset which isn't valid UTF-8,
    /// or whose `Content-Encoding` can't be undone.
    pub fn text(self) -> Option<String> {
        let content = self.content().ok()?;
        let encoding = self
            .header("Content-Type")
            .and_then(headers::content_type_charset)
            .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()));
        match encoding {
            Some(encoding) => Some(encoding.decode(&content).0.into_owned()),
            None => String::from_utf8(content.into_owned()).ok(),
        }
    }
}

/// The pipes wptserve applies to the response, like its `wrap_pipeline`:
/// `sub` for `.sub.` files (HTML-escaped for markup files), then those in the
/// `?pipe=` query.
fn pipeline(relative_path: &str, request_url: &Url) -> String {
    let mut pipeline = String::new();
    if relative_path.contains(".sub.") {
        let extension = Path::new(relative_path)
            .extension()
            .and_then(|ext| ext.to_str());
        let is_markup = extension
            .is_some_and(|ext| ["html", "htm", "xht", "xhtml", "xml", "svg"].contains(&ext));
        pipeline.push_str(if is_markup { "sub(html)" } else { "sub(none)" });
    }
    let query = request_url
        .query_pairs()
        .filter(|(key, value)| key == "pipe" && !value.is_empty())
        .last();
    if let Some((_, query)) = query {
        if !pipeline.is_empty() {
            pipeline.push('|');
        }
        pipeline.push_str(&query);
    }
    pipeline
}

/// Serve the file at `relative_path` (relative to the WPT root, no query) for a
/// request to `request_url`, applying the file's `.headers` and pipes (`.sub.`
/// substitution and the `?pipe=` query) like wptserve.
pub fn serve(wpt_dir: &Path, relative_path: &str, request_url: &Url) -> std::io::Result<Response> {
    let body = std::fs::read(wpt_dir.join(relative_path))?;
    let headers = load_headers(wpt_dir, relative_path, request_url);
    let mut response = Response {
        status: 200,
        body,
        headers,
    };
    let pipeline = pipeline(relative_path, request_url);
    pipes::apply(
        &pipeline,
        &mut response,
        wpt_dir,
        relative_path,
        request_url,
    );
    Ok(response)
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
        assert_eq!(response.is_xml(), Some(true));
        assert_eq!(response.text().as_deref(), Some("café"));

        let url = Url::parse(ORIGIN).unwrap().join("css/b.html").unwrap();
        let response = serve(dir.path(), "css/b.html", &url).unwrap();
        assert_eq!(response.is_xml(), None);
        assert_eq!(response.text(), None);

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

    #[test]
    fn pipeline_from_path_and_query() {
        let url = Url::parse(ORIGIN).unwrap();
        assert_eq!(pipeline("a/b.sub.html", &url), "sub(html)");
        assert_eq!(pipeline("a/b.sub.css", &url), "sub(none)");
        assert_eq!(pipeline("a.sub/b.html", &url), "");
        let piped = url.join("a/b.js?pipe=header(X,y)|sub").unwrap();
        assert_eq!(pipeline("a/b.js", &piped), "header(X,y)|sub");
        let piped = url.join("a/b.sub.js?pipe=a&pipe=slice(1)").unwrap();
        assert_eq!(pipeline("a/b.sub.js", &piped), "sub(none)|slice(1)");
    }

    #[test]
    fn applies_pipes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "0123456789").unwrap();
        std::fs::write(
            dir.path().join("a.txt.headers"),
            "Content-Type: text/plain\n",
        )
        .unwrap();
        let serve = |query: &str| {
            let url = Url::parse(ORIGIN).unwrap().join(query).unwrap();
            serve(dir.path(), "a.txt", &url).unwrap()
        };

        let response = serve(
            "a.txt?pipe=status(404)|header(Content-Type,text/html)|header(X,1)|header(X,2,True)",
        );
        assert_eq!(response.status, 404);
        assert_eq!(response.header("Content-Type"), Some("text/html"));
        assert_eq!(
            response
                .headers
                .iter()
                .filter(|(key, _)| key == "Content-Type")
                .count(),
            1
        );
        assert_eq!(
            response
                .headers
                .iter()
                .filter(|(key, _)| key == "X")
                .count(),
            2
        );

        let response = serve("a.txt?pipe=slice(2,-3)");
        assert_eq!(response.header("Content-Length"), Some("5"));
        assert_eq!(response.text().as_deref(), Some("23456"));

        let response = serve("a.txt?pipe=slice(null,3)|gzip|trickle(d1)");
        assert_eq!(response.header("Content-Encoding"), Some("gzip"));
        assert_ne!(response.body, b"012");
        assert_eq!(response.text().as_deref(), Some("012"));

        let response = serve("a.txt?pipe=status(x)|nope|slice(5)");
        assert_eq!(response.status, 200);
        assert_eq!(response.text().as_deref(), Some("56789"));
    }
}
