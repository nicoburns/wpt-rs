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

/// The escaping to substitute with if the response needs the `sub` pipe:
/// `.sub.` files (HTML-escaped for markup files) and `?pipe=sub` queries.
fn sub_escape(relative_path: &str, request_url: &Url) -> Option<Escape> {
    if relative_path.contains(".sub.") {
        let file_name = relative_path.rsplit('/').next().unwrap_or(relative_path);
        let extension = file_name.rsplit_once('.').map(|(_, ext)| ext);
        let is_markup = extension
            .is_some_and(|ext| ["html", "htm", "xht", "xhtml", "xml", "svg"].contains(&ext));
        return Some(if is_markup {
            Escape::Html
        } else {
            Escape::None
        });
    }
    // Only the `sub` pipe is supported from `?pipe=`
    let pipe = request_url
        .query_pairs()
        .filter(|(key, _)| key == "pipe")
        .last()?
        .1;
    pipe.split('|').find_map(|pipe| match pipe.trim() {
        "sub" | "sub(html)" => Some(Escape::Html),
        "sub(none)" => Some(Escape::None),
        _ => None,
    })
}

/// Serve the file at `relative_path` (relative to the WPT root, no query) for a
/// request to `request_url`, applying the file's `.headers` and `.sub.`
/// substitution (or a `?pipe=sub` query) like wptserve.
pub fn serve(wpt_dir: &Path, relative_path: &str, request_url: &Url) -> std::io::Result<Response> {
    let mut body = std::fs::read(wpt_dir.join(relative_path))?;
    let headers = load_headers(wpt_dir, relative_path, request_url);
    if let Some(escape) = sub_escape(relative_path, request_url) {
        body = template(wpt_dir, relative_path, request_url, &body, escape);
    }
    Ok(Response { body, headers })
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
    fn sub_pipe_selection() {
        let url = Url::parse(ORIGIN).unwrap();
        assert_eq!(sub_escape("a/b.sub.html", &url), Some(Escape::Html));
        assert_eq!(sub_escape("a/b.sub.css", &url), Some(Escape::None));
        assert_eq!(sub_escape("a.sub/b.html", &url), None);
        let piped = url.join("a/b.js?pipe=header(X,y)|sub").unwrap();
        assert_eq!(sub_escape("a/b.js", &piped), Some(Escape::Html));
        let piped = url.join("a/b.js?pipe=trickle(d1)").unwrap();
        assert_eq!(sub_escape("a/b.js", &piped), None);
    }
}
