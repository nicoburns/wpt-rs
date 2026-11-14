//! wptserve's `sub` pipe: `{{...}}` template substitution

use std::collections::HashMap;
use std::path::Path;

use log::warn;
use url::Url;

use crate::config::{ALT_HOST, BROWSER_HOST, PORTS, SUBDOMAINS};

pub(crate) mod parser;

use parser::{Index, Piece, Token};

/// How substituted values are escaped
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Escape {
    /// Python's `html.escape(value, quote=True)` (`sub` / `sub(html)`)
    Html,
    /// No escaping (`sub(none)`)
    None,
}

/// Apply wptserve's `sub` pipe to `content`, the file at `relative_path`
/// (relative to the WPT root) served for a request to `request_url`.
/// Expressions which can't be evaluated are left in place, with a warning.
pub fn template(
    wpt_dir: &Path,
    relative_path: &str,
    request_url: &Url,
    content: &[u8],
    escape: Escape,
) -> Vec<u8> {
    let mut ctx = TemplateCtx {
        wpt_dir,
        relative_path,
        request_url,
        variables: HashMap::new(),
    };
    let mut out = Vec::with_capacity(content.len());
    for piece in parser::parse(content) {
        match piece {
            Piece::Text(text) => out.extend_from_slice(text),
            Piece::Template { source, tokens } => match ctx.eval(tokens) {
                Ok(value) => match escape {
                    Escape::Html => out.extend_from_slice(escape_html(&value).as_bytes()),
                    Escape::None => out.extend_from_slice(value.as_bytes()),
                },
                Err(err) => {
                    let source = String::from_utf8_lossy(source);
                    warn!("{relative_path}: can't substitute {source}: {err}");
                    out.extend_from_slice(source.as_bytes());
                }
            },
        }
    }
    out
}

/// Python's `html.escape(value, quote=True)`
fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

enum Value {
    Str(String),
    /// `hosts`: all domains, by host name ("" or "alt") then subdomain
    Hosts,
    /// The domains of one host, by subdomain
    Domains(&'static str),
    Ports,
    PortList(&'static [u16]),
    Location,
    Get,
    Headers,
    Function(String),
}

struct TemplateCtx<'a> {
    wpt_dir: &'a Path,
    relative_path: &'a str,
    request_url: &'a Url,
    variables: HashMap<String, String>,
}

impl TemplateCtx<'_> {
    fn eval(&mut self, tokens: Vec<Token>) -> Result<String, String> {
        let mut tokens = tokens.into_iter();
        let mut token = tokens.next().ok_or("empty expression")?;
        let variable = match token {
            Token::Var(name) => {
                token = tokens.next().ok_or("missing value")?;
                Some(name)
            }
            _ => None,
        };
        let Token::Ident(field) = token else {
            return Err(format!("expected ident, got {token:?}"));
        };
        let mut value = match field.as_str() {
            _ if self.variables.contains_key(&field) => Value::Str(self.variables[&field].clone()),
            "uuid" | "file_hash" | "fs_path" | "header_or_default" => Value::Function(field),
            "headers" => Value::Headers,
            "GET" => Value::Get,
            "hosts" => Value::Hosts,
            "domains" => Value::Domains(BROWSER_HOST),
            "host" | "browser_host" => Value::Str(BROWSER_HOST.to_string()),
            "ports" => Value::Ports,
            "location" => Value::Location,
            "url_base" => Value::Str("/".to_string()),
            _ => return Err(format!("undefined template variable {field}")),
        };
        for token in tokens {
            value = match (value, token) {
                (Value::Hosts, Token::Index(Index::Str(name))) => match name.as_str() {
                    "" => Value::Domains(BROWSER_HOST),
                    "alt" => Value::Domains(ALT_HOST),
                    _ => return Err(format!("unknown host {name:?}")),
                },
                (Value::Domains(host), Token::Index(Index::Str(subdomain))) => {
                    if subdomain.is_empty() {
                        Value::Str(host.to_string())
                    } else {
                        let encoded = SUBDOMAINS
                            .get(&subdomain)
                            .ok_or_else(|| format!("unknown subdomain {subdomain:?}"))?;
                        Value::Str(format!("{encoded}.{host}"))
                    }
                }
                (Value::Ports, Token::Index(Index::Str(protocol))) => Value::PortList(
                    PORTS
                        .iter()
                        .find(|(name, _)| *name == protocol)
                        .ok_or_else(|| format!("unknown protocol {protocol:?}"))?
                        .1,
                ),
                (Value::PortList(ports), Token::Index(Index::Int(index))) => {
                    // Python list indexing: negative indices count from the end
                    let index = if index < 0 {
                        ports.len().checked_sub(index.unsigned_abs() as usize)
                    } else {
                        Some(index as usize)
                    };
                    Value::Str(
                        index
                            .and_then(|index| ports.get(index))
                            .ok_or("port index out of range")?
                            .to_string(),
                    )
                }
                (Value::Location, Token::Index(Index::Str(key))) => {
                    Value::Str(self.location(&key)?)
                }
                (Value::Get, Token::Index(Index::Str(key))) => Value::Str(
                    self.request_url
                        .query_pairs()
                        .find(|(name, _)| *name == key)
                        .map(|(_, value)| value.into_owned())
                        .unwrap_or_default(),
                ),
                // wptserve looks up integer keys as is, which never match
                (Value::Get, Token::Index(Index::Int(_))) => Value::Str(String::new()),
                // There are no request headers
                (Value::Headers, Token::Index(key)) => {
                    return Err(format!("no request header {key:?}"));
                }
                (Value::Function(name), Token::Arguments(args)) => {
                    Value::Str(self.call(&name, &args)?)
                }
                (_, token) => return Err(format!("unexpected token {token:?}")),
            };
        }
        let Value::Str(value) = value else {
            return Err("expression doesn't evaluate to a string".to_string());
        };
        if let Some(variable) = variable {
            self.variables.insert(variable, value.clone());
        }
        Ok(value)
    }

    fn location(&self, key: &str) -> Result<String, String> {
        let url = self.request_url;
        let hostname = url.host_str().unwrap_or_default();
        let port = url.port_or_known_default().unwrap_or_default();
        Ok(match key {
            "server" => format!("{}://{hostname}:{port}", url.scheme()),
            "scheme" => url.scheme().to_string(),
            "host" => format!("{hostname}:{port}"),
            "hostname" => hostname.to_string(),
            "port" => port.to_string(),
            "path" | "pathname" => url.path().to_string(),
            "query" => format!("?{}", url.query().unwrap_or_default()),
            _ => return Err(format!("unknown location key {key:?}")),
        })
    }

    fn call(&self, name: &str, args: &[String]) -> Result<String, String> {
        match (name, args) {
            ("uuid", []) => Ok(uuid::Uuid::new_v4().to_string()),
            ("header_or_default", [_, default]) => Ok(default.clone()),
            ("fs_path", [path]) => self.fs_path(path),
            _ => Err(format!("unsupported function {name}({})", args.join(", "))),
        }
    }

    /// The absolute, normalised path of `path` (relative to the requested
    /// file's directory, or the WPT root if it starts with `/`)
    fn fs_path(&self, path: &str) -> Result<String, String> {
        let root_relative = match path.strip_prefix('/') {
            Some(path) => path.to_string(),
            None => match self.relative_path.rsplit_once('/') {
                Some((dir, _)) => format!("{dir}/{path}"),
                None => path.to_string(),
            },
        };
        let mut parts = Vec::new();
        for part in root_relative.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop().ok_or("path outside wpt root")?;
                }
                part => parts.push(part),
            }
        }
        // wptserve rejects any path whose root-relative form contains ".."
        if parts.iter().any(|part| part.contains("..")) {
            return Err("path outside wpt root".to_string());
        }
        let wpt_dir = std::path::absolute(self.wpt_dir).map_err(|err| err.to_string())?;
        Ok(wpt_dir
            .join(parts.join(std::path::MAIN_SEPARATOR_STR))
            .to_string_lossy()
            .into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(content: &str, request_url: &str, escape: Escape) -> String {
        let url = Url::parse(request_url).unwrap();
        let out = template(
            Path::new("/wpt"),
            "css/sub/test.sub.html",
            &url,
            content.as_bytes(),
            escape,
        );
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn substitutes_server_config() {
        let url = "http://web-platform.test:8000/css/sub/test.sub.html?allow=a%20b";
        assert_eq!(
            sub(
                "http://{{host}}:{{ports[http][0]}} {{ports[https][1]}} {{ports[https][-1]}} {{hosts[alt][]}} {{hosts[][www]}} {{domains[www2]}} {{hosts[alt][www.www1]}}",
                url,
                Escape::Html
            ),
            "http://web-platform.test:8000 8444 8444 not-web-platform.test www.web-platform.test www2.web-platform.test www.www1.not-web-platform.test"
        );
        assert_eq!(
            sub("{{domains[élève]}}", url, Escape::None),
            "xn--lve-6lad.web-platform.test"
        );
        assert_eq!(
            sub(
                "{{location[server]}}|{{location[host]}}|{{location[path]}}|{{location[query]}}",
                url,
                Escape::None
            ),
            "http://web-platform.test:8000|web-platform.test:8000|/css/sub/test.sub.html|?allow=a%20b"
        );
        assert_eq!(
            sub(
                "{{GET[allow]}}|{{GET[missing]}}|{{GET[0]}}",
                url,
                Escape::Html
            ),
            "a b||"
        );
        assert_eq!(
            sub("{{header_or_default(X-Test, absent)}}", url, Escape::Html),
            "absent"
        );
    }

    #[test]
    fn fs_path_is_normalised() {
        let url = "http://web-platform.test:8000/css/sub/test.sub.html";
        let wpt_dir = std::path::absolute("/wpt").unwrap();
        let expected = |parts: &[&str]| {
            parts
                .iter()
                .fold(wpt_dir.clone(), |path, part| path.join(part))
                .to_string_lossy()
                .into_owned()
        };
        assert_eq!(
            sub("{{fs_path(a.css)}}", url, Escape::None),
            expected(&["css", "sub", "a.css"])
        );
        assert_eq!(
            sub("{{fs_path(../a.css)}}", url, Escape::None),
            expected(&["css", "a.css"])
        );
        assert_eq!(
            sub("{{fs_path(/x/./a.css)}}", url, Escape::None),
            expected(&["x", "a.css"])
        );
        assert_eq!(
            sub("{{fs_path(../../../a.css)}}", url, Escape::None),
            "{{fs_path(../../../a.css)}}"
        );
    }

    #[test]
    fn variables_escaping_and_errors() {
        let url = "http://web-platform.test:8000/css/test.sub.html";
        let out = sub("{{$id:uuid()}} {{$id}}", url, Escape::Html);
        let (a, b) = out.split_once(' ').unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert_eq!(
            sub(
                "{{GET[q]}}",
                "http://web-platform.test:8000/a?q=%3Cb%3E%22",
                Escape::Html
            ),
            "&lt;b&gt;&quot;"
        );
        assert_eq!(
            sub(
                "{{GET[q]}}",
                "http://web-platform.test:8000/a?q=%3Cb%3E",
                Escape::None
            ),
            "<b>"
        );
        assert_eq!(
            sub(
                "{{nope}} {{ports[ftp][0]}} {{ports[http][x]}} {{}} {{uuid}}",
                url,
                Escape::Html
            ),
            "{{nope}} {{ports[ftp][0]}} {{ports[http][x]}} {{}} {{uuid}}"
        );
    }
}
