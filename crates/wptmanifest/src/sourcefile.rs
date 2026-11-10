//! Port of `tools/manifest/sourcefile.py` and the `to_json` methods of `item.py`:
//! classifies a file and computes its manifest items.

use std::cell::OnceCell;
use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::markup::{self, Root, Tag};
use crate::script_meta::read_script_metadata;
use crate::test262::{self, TestRecord};
use crate::urls::{parse_qs_values, urljoin, urlparse};
use crate::ItemType;

/// html5lib's `spaceCharacters`.
const SPACE_CHARS: &[char] = &['\t', '\n', '\x0c', ' ', '\r'];

const ROOT_DIR_NON_TEST: &[&str] = &["common"];
const DIR_NON_TEST: &[&str] = &["resources", "support", "tools"];
const DIR_PATH_NON_TEST: &[&[&str]] = &[
    &["css21", "archive"],
    &["css", "CSS2", "archive"],
    &["css", "common"],
];

const CSS_MANUAL_FLAGS: &[&str] = &[
    "animated",
    "font",
    "history",
    "interact",
    "paged",
    "speech",
    "userstyle",
];

struct AnyVariant {
    name: &'static str,
    suffix: Option<&'static str>,
    longhand: &'static [&'static str],
}

const fn v(
    name: &'static str,
    suffix: Option<&'static str>,
    longhand: &'static [&'static str],
) -> AnyVariant {
    AnyVariant {
        name,
        suffix,
        longhand,
    }
}

/// `_any_variants` in `sourcefile.py`.
const ANY_VARIANTS: &[AnyVariant] = &[
    v("window", Some(".any.html"), &[]),
    v("window-module", None, &[]),
    v("serviceworker", None, &[]),
    v("serviceworker-module", None, &[]),
    v("sharedworker", None, &[]),
    v("sharedworker-module", None, &[]),
    v("dedicatedworker", Some(".any.worker.html"), &[]),
    v(
        "dedicatedworker-module",
        Some(".any.worker-module.html"),
        &[],
    ),
    v(
        "worker",
        None,
        &["dedicatedworker", "sharedworker", "serviceworker"],
    ),
    v("worker-module", None, &[]),
    v("shadowrealm-in-window", None, &[]),
    v("shadowrealm-in-shadowrealm", None, &[]),
    v("shadowrealm-in-dedicatedworker", None, &[]),
    v("shadowrealm-in-sharedworker", None, &[]),
    v(
        "shadowrealm-in-serviceworker",
        Some(".https.any.shadowrealm-in-serviceworker.html"),
        &[],
    ),
    v(
        "shadowrealm-in-audioworklet",
        Some(".https.any.shadowrealm-in-audioworklet.html"),
        &[],
    ),
    v(
        "shadowrealm",
        None,
        &[
            "shadowrealm-in-window",
            "shadowrealm-in-shadowrealm",
            "shadowrealm-in-dedicatedworker",
            "shadowrealm-in-sharedworker",
            "shadowrealm-in-serviceworker",
            "shadowrealm-in-audioworklet",
        ],
    ),
    v("jsshell", Some(".any.js"), &[]),
];

fn any_variant(name: &str) -> Option<&'static AnyVariant> {
    ANY_VARIANTS.iter().find(|v| v.name == name)
}

/// `global_suffixes`: sorted (suffix, is_jsshell) pairs for a `global=` value.
fn global_suffixes(value: &str) -> Vec<(String, bool)> {
    let mut globals: Vec<&str> = Vec::new();
    if value.is_empty() {
        globals.extend(["window", "dedicatedworker"]);
    } else {
        for item in value.split(',') {
            if let Some(variant) = any_variant(item.trim()) {
                if variant.longhand.is_empty() {
                    globals.push(variant.name);
                } else {
                    globals.extend(variant.longhand);
                }
            }
        }
    }
    let mut rv: Vec<(String, bool)> = globals
        .into_iter()
        .map(|g| {
            let suffix = any_variant(g)
                .and_then(|v| v.suffix)
                .map(str::to_string)
                .unwrap_or_else(|| format!(".any.{g}.html"));
            (suffix, g == "jsshell")
        })
        .collect();
    rv.sort();
    rv.dedup();
    rv
}

fn replace_end(s: &str, old: &str, new: &str) -> String {
    debug_assert!(s.ends_with(old));
    format!("{}{new}", &s[..s.len() - old.len()])
}

fn global_variant_url(url: &str, suffix: &str) -> String {
    let mut url = url.replace(".any.", ".");
    if url.contains(".https.") && suffix.starts_with(".https.") {
        url = url.replace(".https.", ".");
    } else if url.contains(".h2.") && suffix.starts_with(".h2.") {
        url = url.replace(".h2.", ".");
    }
    replace_end(&url, ".js", suffix)
}

/// Python's `os.path.splitext` on a file name.
fn splitext(filename: &str) -> (&str, &str) {
    if let Some(dot) = filename.rfind('.') {
        if filename[..dot].bytes().any(|b| b != b'.') {
            return (&filename[..dot], &filename[dot..]);
        }
    }
    (filename, "")
}

/// `reference_file_re.search(name)` for `(^|[\-_])(not)?ref[0-9]*([\-_]|$)`.
fn is_reference_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    (0..=bytes.len()).any(|i| {
        if i > 0 && !matches!(bytes[i - 1], b'-' | b'_') {
            return false;
        }
        let mut rest = &bytes[i..];
        if let Some(r) = rest.strip_prefix(b"not") {
            if r.starts_with(b"ref") {
                rest = r;
            }
        }
        let Some(rest) = rest.strip_prefix(b"ref") else {
            return false;
        };
        let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        matches!(rest.get(digits), None | Some(b'-') | Some(b'_'))
    })
}

fn err(path: &str, msg: impl std::fmt::Display) -> String {
    format!("{path}: {msg}")
}

/// `(test url, reference url, reference type)` for metadata scoped to one reference.
type RefKey = Option<(String, String, String)>;
type Fuzzy = Vec<(RefKey, Vec<[i64; 2]>)>;

pub struct SourceFile<'a> {
    rel_path: &'a str,
    contents: &'a [u8],
    dir_parts: Vec<&'a str>,
    parts: Vec<&'a str>,
    filename: &'a str,
    name: &'a str,
    ext: &'a str,
    type_flag: Option<&'a str>,
    meta_flags: Vec<&'a str>,
    url: String,
    root: OnceCell<Option<Root>>,
    script_metadata: OnceCell<Option<Vec<(String, String)>>>,
    test262_record: OnceCell<Option<TestRecord>>,
}

impl<'a> SourceFile<'a> {
    pub fn new(url_base: &str, rel_path: &'a str, contents: &'a [u8]) -> Self {
        let (dir_path, filename) = rel_path.rsplit_once('/').unwrap_or(("", rel_path));
        let (name, ext) = splitext(filename);
        let (type_flag, meta_flags) = match name.rsplit_once('-') {
            Some((_, last)) => {
                let mut it = last.split('.');
                (it.next(), it.collect())
            }
            None => (None, name.split('.').skip(1).collect()),
        };
        SourceFile {
            rel_path,
            contents,
            dir_parts: dir_path.split('/').collect(),
            parts: rel_path.split('/').collect(),
            filename,
            name,
            ext,
            type_flag,
            meta_flags,
            url: urljoin(url_base, rel_path),
            root: OnceCell::new(),
            script_metadata: OnceCell::new(),
            test262_record: OnceCell::new(),
        }
    }

    fn in_dir(&self, name: &str) -> bool {
        self.dir_parts.contains(&name)
    }

    fn has_meta_flag(&self, flag: &str) -> bool {
        self.meta_flags.contains(&flag)
    }

    fn name_is_non_test(&self) -> bool {
        self.name.starts_with("MANIFEST")
            || self.filename == "META.yml"
            || self.filename == "WEB_FEATURES.yml"
            || self.filename.starts_with('.')
            || self.filename.ends_with(".headers")
            || self.filename.ends_with(".ini")
            || self.in_non_test_dir()
    }

    fn in_non_test_dir(&self) -> bool {
        if self.parts.len() == 1 {
            return true;
        }
        ROOT_DIR_NON_TEST.contains(&self.parts[0])
            || self.parts.iter().any(|p| DIR_NON_TEST.contains(p))
            || DIR_PATH_NON_TEST
                .iter()
                .any(|path| self.parts.starts_with(path))
    }

    fn in_conformance_checker_dir(&self) -> bool {
        self.parts[0] == "conformance-checkers"
    }

    fn js_flag(&self, flag: &str) -> bool {
        self.has_meta_flag(flag) && self.ext == ".js"
    }

    fn name_is_multi_global(&self) -> bool {
        self.js_flag("any")
    }
    fn name_is_worker(&self) -> bool {
        self.js_flag("worker")
    }
    fn name_is_window(&self) -> bool {
        self.js_flag("window")
    }
    fn name_is_extension(&self) -> bool {
        self.js_flag("extension")
    }

    fn is_py_test_file(&self) -> bool {
        !matches!(self.filename, "__init__.py" | "conftest.py") && self.filename.ends_with(".py")
    }

    fn name_is_webdriver(&self) -> bool {
        let p = &self.parts;
        ((p[0] == "webdriver" && p.len() > 1)
            || (p.starts_with(&["infrastructure", "webdriver"]) && p.len() > 2))
            && self.is_py_test_file()
    }

    fn name_is_aamtest(&self) -> bool {
        self.parts.contains(&"aamtests") && self.is_py_test_file()
    }

    fn name_is_reference(&self) -> bool {
        self.url.contains("/reference/") || is_reference_name(self.name)
    }

    fn markup_type(&self) -> Option<&'static str> {
        match self.ext.strip_prefix('.').unwrap_or(self.ext) {
            "html" | "htm" => Some("html"),
            "xhtml" | "xht" | "xml" => Some("xhtml"),
            "svg" => Some("svg"),
            _ => None,
        }
    }

    fn name_is_crashtest(&self) -> bool {
        self.markup_type().is_some()
            && (self.type_flag == Some("crash") || self.in_dir("crashtests"))
    }

    fn name_is_print_reftest(&self) -> bool {
        self.markup_type().is_some() && (self.type_flag == Some("print") || self.in_dir("print"))
    }

    fn name_is_test262(&self) -> bool {
        self.in_dir("test262") && self.ext == ".js"
    }

    fn root(&self) -> Option<&Root> {
        self.root
            .get_or_init(|| match self.markup_type()? {
                "html" => Some(markup::parse_html(self.contents)),
                _ => markup::parse_xml(self.contents),
            })
            .as_ref()
    }

    fn test262_record(&self) -> Option<&TestRecord> {
        self.test262_record
            .get_or_init(|| {
                if !self.name_is_test262() {
                    return None;
                }
                test262::parse(&String::from_utf8_lossy(self.contents), self.rel_path)
            })
            .as_ref()
    }

    fn script_metadata(&self) -> Option<&[(String, String)]> {
        self.script_metadata
            .get_or_init(|| {
                if self.name_is_worker()
                    || self.name_is_multi_global()
                    || self.name_is_window()
                    || self.name_is_extension()
                {
                    Some(read_script_metadata(self.contents, b"//"))
                } else if self.name_is_webdriver() || self.name_is_aamtest() {
                    Some(read_script_metadata(self.contents, b"#"))
                } else if self.name_is_test262() {
                    let record = self.test262_record()?;
                    Some(
                        record
                            .includes
                            .iter()
                            .flatten()
                            .map(|f| {
                                let path = if f == "assert.js" || f == "sta.js" {
                                    format!("/third_party/test262/harness/{f}")
                                } else {
                                    format!("/resources/test262/{f}")
                                };
                                ("script".to_string(), path)
                            })
                            .collect(),
                    )
                } else {
                    None
                }
            })
            .as_deref()
    }

    fn first_meta_content(&self, name: &str) -> Option<&str> {
        self.root()?
            .find_all(Tag::Meta, "name", name)
            .next()?
            .attr("content")
    }

    fn timeout(&self) -> Option<&'static str> {
        if let Some(meta) = self.script_metadata() {
            if meta.iter().any(|(k, v)| k == "timeout" && v == "long") {
                return Some("long");
            }
        }
        let root = self.root()?;
        let timeout = root
            .find_all(Tag::Meta, "name", "timeout")
            .next()?
            .attr("content")?;
        (timeout.to_lowercase() == "long").then_some("long")
    }

    fn pac(&self) -> Option<String> {
        if let Some(meta) = self.script_metadata() {
            if let Some((_, v)) = meta.iter().find(|(k, _)| k == "pac") {
                return Some(v.clone());
            }
        }
        self.first_meta_content("pac").map(str::to_string)
    }

    fn references(&self) -> Vec<(String, &'static str)> {
        let Some(root) = self.root() else {
            return Vec::new();
        };
        let matches = root
            .all(Tag::Link)
            .filter(|e| e.attr("rel") == Some("match"));
        let mismatches = root
            .all(Tag::Link)
            .filter(|e| e.attr("rel") == Some("mismatch"));
        matches
            .map(|e| (e, "=="))
            .chain(mismatches.map(|e| (e, "!=")))
            .filter_map(|(e, ty)| {
                Some((
                    urljoin(&self.url, e.attr("href")?.trim_matches(SPACE_CHARS)),
                    ty,
                ))
            })
            .collect()
    }

    fn parse_ref_keyed_meta<'c>(
        &self,
        content: &'c str,
        references: &[(String, &'static str)],
    ) -> Result<(RefKey, &'c str), String> {
        match content.rsplit_once(':') {
            None => Ok((None, content)),
            Some((key, value)) => {
                let key_part = urljoin(&self.url, key);
                let reftype = references.iter().find(|r| r.0 == key_part).map(|r| r.1);
                let Some(reftype) = reftype else {
                    return Err(err(
                        self.rel_path,
                        format!("Key {key_part} doesn't correspond to a reference"),
                    ));
                };
                Ok((
                    Some((self.url.clone(), key_part, reftype.to_string())),
                    value,
                ))
            }
        }
    }

    fn fuzzy(&self, references: &[(String, &'static str)]) -> Result<Fuzzy, String> {
        let mut rv: Fuzzy = Vec::new();
        let Some(root) = self.root() else {
            return Ok(rv);
        };
        const ARGS: [&str; 2] = ["maxDifference", "totalPixels"];
        for node in root.find_all(Tag::Meta, "name", "fuzzy") {
            let content = node.attr("content").unwrap_or("");
            let (key, value) = self.parse_ref_keyed_meta(content, references)?;
            let ranges: Vec<&str> = value.split(';').collect();
            if ranges.len() != 2 {
                return Err(err(self.rel_path, format!("Malformed fuzzy value {value}")));
            }
            let mut named: BTreeMap<&str, [i64; 2]> = BTreeMap::new();
            let mut positional = std::collections::VecDeque::new();
            for range in ranges {
                let mut range = range;
                let mut name = None;
                if let Some((n, r)) = range.split_once('=') {
                    let n = n.trim();
                    if !ARGS.contains(&n) {
                        return Err(err(
                            self.rel_path,
                            format!("{n} is not a valid fuzzy property"),
                        ));
                    }
                    if named.contains_key(n) {
                        return Err(err(
                            self.rel_path,
                            format!("Got multiple values for argument {n}"),
                        ));
                    }
                    name = Some(n);
                    range = r.trim();
                }
                let (min, max) = match range.split_once('-') {
                    Some((min, max)) if !max.contains('-') => (min, max),
                    Some(_) => {
                        return Err(err(self.rel_path, format!("Malformed fuzzy value {value}")))
                    }
                    None => (range, range),
                };
                let parse = |s: &str| {
                    s.trim().parse::<i64>().map_err(|_| {
                        err(
                            self.rel_path,
                            format!("Fuzzy value {range} must be a range of integers"),
                        )
                    })
                };
                let value = [parse(min)?, parse(max)?];
                match name {
                    Some(n) => {
                        named.insert(n, value);
                    }
                    None => positional.push_back(value),
                }
            }
            if rv.iter().any(|(k, _)| *k == key) {
                return Err(err(self.rel_path, "Got multiple fuzzy values for key"));
            }
            let mut values = Vec::new();
            for arg in ARGS {
                let value = match named.remove(arg) {
                    Some(v) => v,
                    None => positional.pop_front().ok_or_else(|| {
                        err(self.rel_path, format!("Malformed fuzzy value {value}"))
                    })?,
                };
                values.push(value);
            }
            if !positional.is_empty() {
                return Err(err(self.rel_path, format!("Malformed fuzzy value {value}")));
            }
            rv.push((key, values));
        }
        Ok(rv)
    }

    fn page_ranges(
        &self,
        references: &[(String, &'static str)],
    ) -> Result<BTreeMap<String, Value>, String> {
        let mut rv = BTreeMap::new();
        let Some(root) = self.root() else {
            return Ok(rv);
        };
        for node in root.find_all(Tag::Meta, "name", "reftest-pages") {
            let content = node.attr("content").unwrap_or("");
            let (key_data, value) = self.parse_ref_keyed_meta(content, references)?;
            let key = match key_data {
                None => self.url.clone(),
                Some(k) => k.1,
            };
            if rv.contains_key(&key) {
                return Err(err(self.rel_path, "Duplicate page-ranges value"));
            }
            let malformed = |r: &str| err(self.rel_path, format!("Malformed page-range value {r}"));
            let mut ranges = Vec::new();
            for range in value.split(',') {
                let range = range.trim();
                if range.contains('-') {
                    let mut parts = Vec::new();
                    for item in range.split('-').map(str::trim) {
                        if item.is_empty() {
                            parts.push(Value::Null);
                        } else {
                            let n: i64 = item.parse().map_err(|_| malformed(range))?;
                            if n == 0 {
                                return Err(malformed(range));
                            }
                            parts.push(json!(n));
                        }
                    }
                    ranges.push(Value::Array(parts));
                } else {
                    let n: i64 = range.parse().map_err(|_| malformed(range))?;
                    ranges.push(json!([n]));
                }
            }
            rv.insert(key, Value::Array(ranges));
        }
        Ok(rv)
    }

    fn has_testdriver(&self) -> Option<bool> {
        Some(self.testdriver_nodes(self.root()?).next().is_some())
    }

    fn testdriver_nodes<'r>(
        &self,
        root: &'r Root,
    ) -> impl Iterator<Item = &'r markup::Element> + 'r {
        root.all(Tag::Script).filter(|e| {
            e.attr("src").is_some_and(|s| {
                s == "/resources/testdriver.js" || s.starts_with("/resources/testdriver.js?")
            })
        })
    }

    fn testdriver_features(&self) -> Option<Vec<String>> {
        let mut include = None;
        if let Some(meta) = self.script_metadata() {
            include = meta
                .iter()
                .find(|(k, v)| {
                    k.trim() == "script"
                        && (v == "/resources/testdriver.js"
                            || v.starts_with("/resources/testdriver.js?"))
                })
                .map(|(_, v)| v.trim().to_string());
        }
        if include.is_none() {
            let root = self.root()?;
            include = self
                .testdriver_nodes(root)
                .find_map(|e| e.attr("src"))
                .map(str::to_string);
        }
        let query = urlparse(&include?, "").query;
        let features = parse_qs_values(&query, "feature");
        (!features.is_empty()).then_some(features)
    }

    fn test_variants(&self) -> Result<Vec<String>, String> {
        let mut rv: Vec<String> = if self.ext == ".js" {
            self.script_metadata()
                .unwrap_or_default()
                .iter()
                .filter(|(k, _)| k == "variant")
                .map(|(_, v)| v.clone())
                .collect()
        } else {
            self.root()
                .map(|r| {
                    r.find_all(Tag::Meta, "name", "variant")
                        .filter_map(|e| e.attr("content"))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        for variant in &rv {
            if variant.is_empty() {
                continue;
            }
            if !variant.starts_with(['#', '?']) {
                return Err(err(
                    self.rel_path,
                    "Non-empty variant must start with either a ? or a #",
                ));
            }
            if variant.len() == 1 || variant.starts_with("?#") {
                return Err(err(
                    self.rel_path,
                    "Variants must not have empty fragment or query (omit the empty part instead)",
                ));
            }
        }
        if rv.is_empty() {
            rv.push(String::new());
        }
        Ok(rv)
    }

    fn css_flags(&self) -> Vec<&str> {
        let Some(root) = self.root() else {
            return Vec::new();
        };
        root.find_all(Tag::Meta, "name", "flags")
            .filter_map(|e| e.attr("content"))
            .flat_map(str::split_whitespace)
            .collect()
    }

    fn content_is_css_manual(&self) -> bool {
        self.root().is_some()
            && self
                .css_flags()
                .iter()
                .any(|f| CSS_MANUAL_FLAGS.contains(f))
    }

    fn content_is_css_visual(&self) -> bool {
        let Some(root) = self.root() else {
            return false;
        };
        matches!(
            self.ext,
            ".xht" | ".html" | ".xhtml" | ".htm" | ".xml" | ".svg"
        ) && root
            .find_all(Tag::Link, "rel", "help")
            .any(|e| e.attr("href").is_some())
    }

    fn content_is_testharness(&self) -> bool {
        self.root().is_some_and(|r| {
            r.find_all(Tag::Script, "src", "/resources/testharness.js")
                .next()
                .is_some()
        })
    }

    /// The `[url, {extras}]` JSON of a `URLManifestItem`; `url` is null when it equals the path.
    fn url_item(&self, url: &str, extras: Map<String, Value>) -> Value {
        json!([self.rel_url_json(url), Value::Object(extras)])
    }

    fn rel_url_json(&self, url: &str) -> Value {
        if url == self.rel_path {
            Value::Null
        } else {
            Value::String(url.to_string())
        }
    }

    fn testharness_extras(
        &self,
        include_features: bool,
        testdriver: Option<bool>,
        jsshell: bool,
    ) -> Map<String, Value> {
        let mut extras = BTreeMap::new();
        if let Some(t) = self.timeout() {
            extras.insert("timeout", json!(t));
        }
        if let Some(pac) = self.pac() {
            extras.insert("pac", json!(pac));
        }
        if include_features {
            if let Some(f) = self.testdriver_features() {
                extras.insert("testdriver_features", json!(f));
            }
        }
        if testdriver == Some(true) {
            extras.insert("testdriver", json!(true));
        }
        if jsshell {
            extras.insert("jsshell", json!(true));
        }
        if let Some(meta) = self.script_metadata() {
            if !meta.is_empty() {
                extras.insert(
                    "script_metadata",
                    json!(meta.iter().map(|(k, v)| [k, v]).collect::<Vec<_>>()),
                );
            }
        }
        to_map(extras)
    }

    fn timeout_extras(&self) -> Map<String, Value> {
        let mut extras = Map::new();
        if let Some(t) = self.timeout() {
            extras.insert("timeout".into(), json!(t));
        }
        extras
    }

    fn reftest_item(
        &self,
        url: &str,
        references: &[(String, &'static str)],
        print: Option<&BTreeMap<String, Value>>,
        fuzzy: &Fuzzy,
    ) -> Value {
        let mut extras = BTreeMap::new();
        if let Some(t) = self.timeout() {
            extras.insert("timeout", json!(t));
        }
        if let Some(v) = self.first_meta_content("viewport-size") {
            extras.insert("viewport_size", json!(v));
        }
        if print.is_none() {
            if let Some(v) = self.first_meta_content("device-pixel-ratio") {
                extras.insert("dpi", json!(v));
            }
        }
        if !fuzzy.is_empty() {
            let fuzzy: Vec<Value> = fuzzy
                .iter()
                .map(|(k, v)| json!([k.as_ref().map(|(a, b, c)| [a, b, c]), v]))
                .collect();
            extras.insert("fuzzy", Value::Array(fuzzy));
        }
        if self.has_testdriver() == Some(true) {
            extras.insert("testdriver", json!(true));
        }
        if let Some(pr) = print {
            if !pr.is_empty() {
                extras.insert(
                    "page_ranges",
                    Value::Object(pr.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
                );
            }
        }
        let refs: Vec<Value> = references.iter().map(|(u, t)| json!([u, t])).collect();
        json!([self.rel_url_json(url), refs, Value::Object(to_map(extras))])
    }

    /// Port of `SourceFile.manifest_items`.
    pub fn manifest_items(&self) -> Result<(ItemType, Vec<Value>), String> {
        let rel_url = self.rel_path;
        let single = |ty: ItemType, extras: Map<String, Value>| {
            Ok((ty, vec![self.url_item(rel_url, extras)]))
        };

        if self.name_is_non_test() {
            return Ok((ItemType::Support, vec![json!([])]));
        }
        if self.type_flag == Some("manual") {
            return single(ItemType::Manual, Map::new());
        }
        if self.in_conformance_checker_dir()
            && matches!(self.type_flag, Some("is-valid") | Some("no-valid"))
        {
            return single(ItemType::ConformanceChecker, Map::new());
        }
        if self.in_conformance_checker_dir() {
            return Ok((ItemType::Support, vec![json!([])]));
        }
        if self.name_is_webdriver() {
            return single(ItemType::Wdspec, self.timeout_extras());
        }
        if self.name_is_aamtest() {
            return single(ItemType::Aamtest, self.timeout_extras());
        }
        if self.type_flag == Some("visual") {
            return single(ItemType::Visual, Map::new());
        }
        if self.name_is_crashtest() {
            let mut extras = Map::new();
            if self.has_testdriver() == Some(true) {
                extras.insert("testdriver".into(), json!(true));
            }
            return single(ItemType::Crashtest, extras);
        }
        if self.name_is_print_reftest() {
            let references = self.references();
            if references.is_empty() {
                return Err(err(
                    self.rel_path,
                    "detected as print reftest but doesn't have any refs",
                ));
            }
            let fuzzy = self.fuzzy(&references)?;
            let page_ranges = self.page_ranges(&references)?;
            let item = self.reftest_item(rel_url, &references, Some(&page_ranges), &fuzzy);
            return Ok((ItemType::PrintReftest, vec![item]));
        }
        if self.name_is_multi_global() {
            let meta = self.script_metadata().unwrap_or_default();
            let globals = meta
                .iter()
                .find(|(k, _)| k == "global")
                .map(|(_, v)| v.as_str())
                .unwrap_or("");
            let variants = self.test_variants()?;
            let mut tests = Vec::new();
            for (suffix, jsshell) in global_suffixes(globals) {
                for variant in &variants {
                    let url = global_variant_url(rel_url, &suffix) + variant;
                    tests.push(self.url_item(&url, self.testharness_extras(true, None, jsshell)));
                }
            }
            return Ok((ItemType::Testharness, tests));
        }
        for (is, js_suffix, html_suffix, features) in [
            (self.name_is_worker(), ".worker.js", ".worker.html", true),
            (self.name_is_window(), ".window.js", ".window.html", true),
        ] {
            if is {
                let test_url = replace_end(rel_url, js_suffix, html_suffix);
                let tests = self
                    .test_variants()?
                    .iter()
                    .map(|v| {
                        self.url_item(
                            &(test_url.clone() + v),
                            self.testharness_extras(features, None, false),
                        )
                    })
                    .collect();
                return Ok((ItemType::Testharness, tests));
            }
        }
        if self.name_is_test262() {
            let Some(record) = self.test262_record() else {
                return Ok((ItemType::Support, vec![json!([])]));
            };
            let mut suffix = ".test262".to_string();
            if record.is_module {
                suffix += "-module";
            } else if record.is_only_strict {
                suffix += ".strict";
            }
            suffix += ".html";
            let test_url = replace_end(rel_url, ".js", &suffix);
            let tests = self
                .test_variants()?
                .iter()
                .map(|v| {
                    self.url_item(
                        &(test_url.clone() + v),
                        self.testharness_extras(true, None, false),
                    )
                })
                .collect();
            return Ok((ItemType::Test262, tests));
        }
        if self.name_is_extension() {
            let test_url = replace_end(rel_url, ".extension.js", ".extension.html");
            let tests = self
                .test_variants()?
                .iter()
                .map(|v| {
                    self.url_item(
                        &(test_url.clone() + v),
                        self.testharness_extras(false, None, false),
                    )
                })
                .collect();
            return Ok((ItemType::Testharness, tests));
        }
        if self.markup_type().is_none() {
            return Ok((ItemType::Support, vec![json!([])]));
        }
        if self.content_is_css_manual() && !self.name_is_reference() {
            return single(ItemType::Manual, Map::new());
        }
        if self.content_is_testharness() {
            let testdriver = self.has_testdriver();
            let tests = self
                .test_variants()?
                .iter()
                .map(|v| {
                    self.url_item(
                        &format!("{rel_url}{v}"),
                        self.testharness_extras(true, testdriver, false),
                    )
                })
                .collect();
            return Ok((ItemType::Testharness, tests));
        }
        let references = self.references();
        if !references.is_empty() {
            let fuzzy = self.fuzzy(&references)?;
            let tests = self
                .test_variants()?
                .iter()
                .map(|v| {
                    let refs: Vec<_> = references
                        .iter()
                        .map(|(u, t)| (format!("{u}{v}"), *t))
                        .collect();
                    self.reftest_item(&format!("{rel_url}{v}"), &refs, None, &fuzzy)
                })
                .collect();
            return Ok((ItemType::Reftest, tests));
        }
        if self.content_is_css_visual() && !self.name_is_reference() {
            return single(ItemType::Visual, Map::new());
        }
        Ok((ItemType::Support, vec![json!([])]))
    }
}

fn to_map(extras: BTreeMap<&str, Value>) -> Map<String, Value> {
    extras
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(path: &str, src: &str) -> (ItemType, Value) {
        let (ty, items) = SourceFile::new("/", path, src.as_bytes())
            .manifest_items()
            .unwrap();
        (ty, Value::Array(items))
    }

    #[test]
    fn names() {
        assert!(is_reference_name("foo-ref"));
        assert!(is_reference_name("ref"));
        assert!(is_reference_name("foo_notref2-x"));
        assert!(!is_reference_name("prefer"));
        assert!(!is_reference_name("foo-reference"));
        assert_eq!(splitext(".gitignore"), (".gitignore", ""));
        assert_eq!(splitext("a.b.js"), ("a.b", ".js"));
    }

    #[test]
    fn multi_global() {
        let (ty, json) = items(
            "a/b.any.js",
            "// META: global=window,worker\n// META: timeout=long\n",
        );
        assert_eq!(ty, ItemType::Testharness);
        let urls: Vec<_> = json
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i[0].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            urls,
            [
                "a/b.any.html",
                "a/b.any.serviceworker.html",
                "a/b.any.sharedworker.html",
                "a/b.any.worker.html"
            ]
        );
        assert_eq!(json[0][1]["timeout"], "long");
    }

    #[test]
    fn reftest() {
        let (ty, json) = items(
            "css/a.html",
            "<link rel=match href=a-ref.html><meta name=fuzzy content='maxDifference=1-2;0-5'><meta name=variant content='?x'>",
        );
        assert_eq!(ty, ItemType::Reftest);
        assert_eq!(
            json,
            json!([["css/a.html?x", [["/css/a-ref.html?x", "=="]], {"fuzzy": [[null, [[1, 2], [0, 5]]]]}]])
        );
        assert_eq!(items("css/a-ref.html", "<p>").0, ItemType::Support);
        assert_eq!(items("a/support/a.html", "<p>").0, ItemType::Support);
        assert_eq!(
            items("css/a.html", "<link rel=help href=x>").0,
            ItemType::Visual
        );
    }
}
