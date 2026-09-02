//! Parsing of the `// META:` comment block at the top of JS-file tests
//! (`.any.js`, `.window.js`, `.worker.js`, etc).
//!
//! Mirrors `read_script_metadata` and the `js_meta_re` regexp in upstream
//! wpt's `tools/manifest/sourcefile.py`: only the leading run of lines
//! matching `// META: key=value` is honoured, and parsing stops at the first
//! line which doesn't match (typically the first line of code).
//!
//! See <https://web-platform-tests.org/writing-tests/testharness.html>

use std::fmt;
use std::str::FromStr;

/// A single parsed `// META:` directive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptMetadata {
    /// `// META: title=...` — the title of the test
    Title(String),
    /// `// META: script=...` — an additional script to include before the test
    /// script (path is resolved as a URL, absolute or relative to the test)
    Script(String),
    /// `// META: timeout=...` — the test's timeout classification
    Timeout(Timeout),
    /// `// META: global=...` — the global scopes (shorthands unexpanded) in
    /// which a multi-global (`.any.js`) test should run
    Global(Vec<GlobalScope>),
    /// `// META: variant=...` — a URL query and/or fragment suffix with which
    /// the test should be run (a test with multiple variants runs once per
    /// variant). Use [`is_valid_variant`] to validate the value.
    Variant(String),
    /// A directive this crate doesn't know about (e.g. the informational
    /// `spec=...`, or future extensions). Upstream ignores unknown keys.
    Unknown { key: String, value: String },
}

/// A test timeout classification
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Timeout {
    /// The default timeout (10s in upstream wpt runners)
    #[default]
    Normal,
    /// An extended timeout (60s in upstream wpt runners), from `timeout=long`
    Long,
}

/// A global scope keyword accepted by `// META: global=`, as defined by
/// `_any_variants` in upstream's `tools/manifest/sourcefile.py`. Includes the
/// shorthand keywords (`worker`, `shadowrealm`) unexpanded; use
/// [`GlobalScope::longhand`] to expand them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlobalScope {
    Window,
    WindowModule,
    DedicatedWorker,
    DedicatedWorkerModule,
    SharedWorker,
    SharedWorkerModule,
    ServiceWorker,
    ServiceWorkerModule,
    /// Shorthand for dedicatedworker, sharedworker and serviceworker
    Worker,
    WorkerModule,
    ShadowRealmInWindow,
    ShadowRealmInShadowRealm,
    ShadowRealmInDedicatedWorker,
    ShadowRealmInSharedWorker,
    ShadowRealmInServiceWorker,
    ShadowRealmInAudioWorklet,
    /// Shorthand for all of the shadowrealm-in-* scopes
    ShadowRealm,
    JsShell,
    /// A keyword this crate doesn't know about
    Unknown(String),
}

impl GlobalScope {
    /// The set of scopes a multi-global test runs in when it has no
    /// `// META: global=` directive (upstream's `get_default_any_variants`)
    pub fn default_scopes() -> Vec<GlobalScope> {
        vec![GlobalScope::Window, GlobalScope::DedicatedWorker]
    }

    /// Expands shorthand keywords (`worker`, `shadowrealm`) into the scopes
    /// they denote (upstream's `get_any_variants` longhand expansion). Other
    /// keywords expand to themselves; unknown keywords expand to nothing.
    pub fn longhand(&self) -> Vec<GlobalScope> {
        use GlobalScope::*;
        match self {
            Worker => vec![DedicatedWorker, SharedWorker, ServiceWorker],
            ShadowRealm => vec![
                ShadowRealmInWindow,
                ShadowRealmInShadowRealm,
                ShadowRealmInDedicatedWorker,
                ShadowRealmInSharedWorker,
                ShadowRealmInServiceWorker,
                ShadowRealmInAudioWorklet,
            ],
            Unknown(_) => vec![],
            other => vec![other.clone()],
        }
    }

    /// The keyword as it appears in a `// META: global=` value
    pub fn as_str(&self) -> &str {
        use GlobalScope::*;
        match self {
            Window => "window",
            WindowModule => "window-module",
            DedicatedWorker => "dedicatedworker",
            DedicatedWorkerModule => "dedicatedworker-module",
            SharedWorker => "sharedworker",
            SharedWorkerModule => "sharedworker-module",
            ServiceWorker => "serviceworker",
            ServiceWorkerModule => "serviceworker-module",
            Worker => "worker",
            WorkerModule => "worker-module",
            ShadowRealmInWindow => "shadowrealm-in-window",
            ShadowRealmInShadowRealm => "shadowrealm-in-shadowrealm",
            ShadowRealmInDedicatedWorker => "shadowrealm-in-dedicatedworker",
            ShadowRealmInSharedWorker => "shadowrealm-in-sharedworker",
            ShadowRealmInServiceWorker => "shadowrealm-in-serviceworker",
            ShadowRealmInAudioWorklet => "shadowrealm-in-audioworklet",
            ShadowRealm => "shadowrealm",
            JsShell => "jsshell",
            Unknown(keyword) => keyword,
        }
    }
}

impl FromStr for GlobalScope {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use GlobalScope::*;
        Ok(match s {
            "window" => Window,
            "window-module" => WindowModule,
            "dedicatedworker" => DedicatedWorker,
            "dedicatedworker-module" => DedicatedWorkerModule,
            "sharedworker" => SharedWorker,
            "sharedworker-module" => SharedWorkerModule,
            "serviceworker" => ServiceWorker,
            "serviceworker-module" => ServiceWorkerModule,
            "worker" => Worker,
            "worker-module" => WorkerModule,
            "shadowrealm-in-window" => ShadowRealmInWindow,
            "shadowrealm-in-shadowrealm" => ShadowRealmInShadowRealm,
            "shadowrealm-in-dedicatedworker" => ShadowRealmInDedicatedWorker,
            "shadowrealm-in-sharedworker" => ShadowRealmInSharedWorker,
            "shadowrealm-in-serviceworker" => ShadowRealmInServiceWorker,
            "shadowrealm-in-audioworklet" => ShadowRealmInAudioWorklet,
            "shadowrealm" => ShadowRealm,
            "jsshell" => JsShell,
            other => Unknown(other.to_string()),
        })
    }
}

impl fmt::Display for GlobalScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parses the leading `// META:` block of a JS test file into directives.
/// Matches upstream's `js_meta_re` (`//\s*META:\s*(\w*)=(.*)$`) applied
/// line-by-line until the first non-matching line.
pub fn parse_script_metadata(js_source: &str) -> Vec<ScriptMetadata> {
    js_source
        .lines()
        .map_while(parse_meta_line)
        .map(|(key, value)| match key {
            "title" => ScriptMetadata::Title(value.to_string()),
            "script" => ScriptMetadata::Script(value.to_string()),
            "timeout" => ScriptMetadata::Timeout(match value {
                "long" => Timeout::Long,
                _ => Timeout::Normal,
            }),
            "global" => ScriptMetadata::Global(
                value
                    .split(',')
                    .map(|item| item.trim().parse().unwrap())
                    .collect(),
            ),
            "variant" => ScriptMetadata::Variant(value.to_string()),
            _ => ScriptMetadata::Unknown {
                key: key.to_string(),
                value: value.to_string(),
            },
        })
        .collect()
}

/// Matches a single line against upstream's `js_meta_re`:
/// `//`, optional whitespace, `META:`, optional whitespace, a (possibly
/// empty) `\w*` key, `=`, then the rest of the line verbatim as the value.
fn parse_meta_line(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix("//")?.trim_start();
    let rest = rest.strip_prefix("META:")?.trim_start();
    let (key, value) = rest.split_once('=')?;
    key.chars()
        .all(|c| c.is_alphanumeric() || c == '_')
        .then_some((key, value))
}

/// Validates a variant value per upstream's `test_variants` checks:
/// a non-empty variant must start with `?` or `#` and must not have an
/// empty query or fragment part.
pub fn is_valid_variant(variant: &str) -> bool {
    if variant.is_empty() {
        return true;
    }
    if !variant.starts_with('?') && !variant.starts_with('#') {
        return false;
    }
    !(variant.len() == 1 || variant.starts_with("?#"))
}

/// Convenience queries over a parsed metadata block
pub trait ScriptMetadataExt {
    /// The test's title, if any
    fn title(&self) -> Option<&str>;
    /// The additional scripts to include, in order
    fn scripts(&self) -> Vec<&str>;
    /// The test's timeout classification
    fn timeout(&self) -> Timeout;
    /// The full expanded set of global scopes the test should run in:
    /// the union of the longhand expansions of every `global=` directive,
    /// or the default scopes if there are none
    fn global_scopes(&self) -> Vec<GlobalScope>;
    /// The test's variants (URL query/fragment suffixes), in order
    fn variants(&self) -> Vec<&str>;
}

impl ScriptMetadataExt for [ScriptMetadata] {
    fn title(&self) -> Option<&str> {
        self.iter().find_map(|meta| match meta {
            ScriptMetadata::Title(title) => Some(title.as_str()),
            _ => None,
        })
    }

    fn scripts(&self) -> Vec<&str> {
        self.iter()
            .filter_map(|meta| match meta {
                ScriptMetadata::Script(script) => Some(script.as_str()),
                _ => None,
            })
            .collect()
    }

    fn timeout(&self) -> Timeout {
        self.iter()
            .find_map(|meta| match meta {
                ScriptMetadata::Timeout(timeout) => Some(*timeout),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn global_scopes(&self) -> Vec<GlobalScope> {
        let mut scopes: Vec<GlobalScope> = Vec::new();
        let mut any = false;
        for meta in self {
            if let ScriptMetadata::Global(keywords) = meta {
                any = true;
                for keyword in keywords {
                    for scope in keyword.longhand() {
                        if !scopes.contains(&scope) {
                            scopes.push(scope);
                        }
                    }
                }
            }
        }
        if any {
            scopes
        } else {
            GlobalScope::default_scopes()
        }
    }

    fn variants(&self) -> Vec<&str> {
        self.iter()
            .filter_map(|meta| match meta {
                ScriptMetadata::Variant(variant) => Some(variant.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_leading_block_only() {
        let js = "// META: title=A title\n\
                  //META: script=/common/utils.js\n\
                  //  META:  timeout=long\n\
                  'use strict';\n\
                  // META: script=ignored.js\n";
        let metas = parse_script_metadata(js);
        assert_eq!(
            metas,
            vec![
                ScriptMetadata::Title("A title".to_string()),
                ScriptMetadata::Script("/common/utils.js".to_string()),
                ScriptMetadata::Timeout(Timeout::Long),
            ]
        );
        assert_eq!(metas.title(), Some("A title"));
        assert_eq!(metas.scripts(), vec!["/common/utils.js"]);
        assert_eq!(metas.timeout(), Timeout::Long);
    }

    #[test]
    fn stops_at_non_meta_comment() {
        let js = "// A regular comment\n// META: title=ignored\n";
        assert_eq!(parse_script_metadata(js), vec![]);
    }

    #[test]
    fn unknown_keys_preserved() {
        let js = "// META: spec=https://example.com/spec\n";
        assert_eq!(
            parse_script_metadata(js),
            vec![ScriptMetadata::Unknown {
                key: "spec".to_string(),
                value: "https://example.com/spec".to_string(),
            }]
        );
    }

    #[test]
    fn global_scopes_default() {
        let metas = parse_script_metadata("test(() => {});");
        assert_eq!(
            metas.global_scopes(),
            vec![GlobalScope::Window, GlobalScope::DedicatedWorker]
        );
    }

    #[test]
    fn global_scopes_expand_shorthand() {
        let metas = parse_script_metadata("// META: global=window,worker\n");
        assert_eq!(
            metas.global_scopes(),
            vec![
                GlobalScope::Window,
                GlobalScope::DedicatedWorker,
                GlobalScope::SharedWorker,
                GlobalScope::ServiceWorker,
            ]
        );
    }

    #[test]
    fn worker_only_global() {
        let metas = parse_script_metadata("// META: global=worker\n");
        assert!(!metas.global_scopes().contains(&GlobalScope::Window));
    }

    #[test]
    fn variants() {
        let js = "// META: variant=?default\n// META: variant=?wss\n";
        let metas = parse_script_metadata(js);
        assert_eq!(metas.variants(), vec!["?default", "?wss"]);
    }

    #[test]
    fn value_is_rest_of_line_verbatim() {
        let js = "// META: title=a = b, c = d \n";
        assert_eq!(
            parse_script_metadata(js),
            vec![ScriptMetadata::Title("a = b, c = d ".to_string())]
        );
    }

    #[test]
    fn invalid_key_stops_parsing() {
        let js = "// META: not a key=value\n// META: title=ignored\n";
        assert_eq!(parse_script_metadata(js), vec![]);
    }
}
