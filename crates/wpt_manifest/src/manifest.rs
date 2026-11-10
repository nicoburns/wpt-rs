//! Port of `tools/manifest/manifest.py`, `typedata.py` and `vcs.py`: the manifest data
//! model, its JSON form, and building it from the filesystem.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{Map, Value};
use sha1::{Digest, Sha1};

use crate::sourcefile::SourceFile;
use crate::ItemType;

/// The manifest format version written by this crate (`manifest.CURRENT_VERSION`).
pub const CURRENT_VERSION: u64 = 9;

#[derive(Debug)]
pub enum Error {
    Io(PathBuf, io::Error),
    Json(serde_json::Error),
    Walk(ignore::Error),
    /// The equivalent of Python's `ManifestError`: a test file has invalid metadata.
    Manifest(String),
    /// An existing manifest can't be used and must be rebuilt.
    Incompatible(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(path, e) => write!(f, "{}: {e}", path.display()),
            Error::Json(e) => write!(f, "invalid manifest JSON: {e}"),
            Error::Walk(e) => write!(f, "{e}"),
            Error::Manifest(e) => write!(f, "{e}"),
            Error::Incompatible(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone)]
struct FileEntry {
    item_type: ItemType,
    hash: String,
    items: Vec<Value>,
}

/// An in-memory `MANIFEST.json`, keyed by the `/`-separated path of each file.
#[derive(Debug, Clone)]
pub struct Manifest {
    url_base: String,
    files: BTreeMap<String, FileEntry>,
}

impl Manifest {
    pub fn new(url_base: impl Into<String>) -> Self {
        Manifest {
            url_base: url_base.into(),
            files: BTreeMap::new(),
        }
    }

    pub fn url_base(&self) -> &str {
        &self.url_base
    }

    /// Iterates over `(path, item type, hash)` for every file in the manifest.
    pub fn files(&self) -> impl Iterator<Item = (&str, ItemType, &str)> {
        self.files
            .iter()
            .map(|(path, e)| (path.as_str(), e.item_type, e.hash.as_str()))
    }

    /// Port of `Manifest.from_json`.
    pub fn from_json(json: &Value) -> Result<Self, Error> {
        let incompatible = |msg: &str| Error::Incompatible(msg.to_string());
        let obj = json
            .as_object()
            .ok_or_else(|| incompatible("manifest is not an object"))?;
        if obj.get("version").and_then(Value::as_u64) != Some(CURRENT_VERSION) {
            return Err(incompatible("manifest version is not 9"));
        }
        let url_base = obj.get("url_base").and_then(Value::as_str).unwrap_or("/");
        let mut manifest = Manifest::new(url_base);
        let items = obj
            .get("items")
            .and_then(Value::as_object)
            .ok_or_else(|| incompatible("manifest has no items"))?;
        for (ty, trie) in items {
            let item_type: ItemType = ty.parse().map_err(|e: String| Error::Incompatible(e))?;
            let mut stack = vec![(String::new(), trie)];
            while let Some((prefix, node)) = stack.pop() {
                let node = node
                    .as_object()
                    .ok_or_else(|| incompatible("malformed manifest"))?;
                for (name, value) in node {
                    let path = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{prefix}/{name}")
                    };
                    match value {
                        Value::Array(leaf) => {
                            let hash = leaf
                                .first()
                                .and_then(Value::as_str)
                                .ok_or_else(|| incompatible("malformed manifest"))?;
                            manifest.files.insert(
                                path,
                                FileEntry {
                                    item_type,
                                    hash: hash.to_string(),
                                    items: leaf[1..].to_vec(),
                                },
                            );
                        }
                        _ => stack.push((path, value)),
                    }
                }
            }
        }
        Ok(manifest)
    }

    /// Reads a manifest file written by this crate or by Python's `wpt manifest`.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let data = fs::read(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
        let json: Value = serde_json::from_slice(&data).map_err(Error::Json)?;
        Self::from_json(&json)
    }

    /// Port of `Manifest.to_json`.
    pub fn to_json(&self) -> Value {
        let mut items: BTreeMap<ItemType, Map<String, Value>> = BTreeMap::new();
        for (path, entry) in &self.files {
            let mut node = items.entry(entry.item_type).or_default();
            let mut parts = path.split('/').peekable();
            while let Some(part) = parts.next() {
                if parts.peek().is_none() {
                    let mut leaf = entry.items.clone();
                    leaf.sort_by(|a, b| sort_key(a).cmp(sort_key(b)));
                    leaf.insert(0, Value::String(entry.hash.clone()));
                    node.insert(part.to_string(), Value::Array(leaf));
                } else {
                    node = match node
                        .entry(part.to_string())
                        .or_insert_with(|| Value::Object(Map::new()))
                    {
                        Value::Object(map) => map,
                        _ => unreachable!("a path is both a file and a directory"),
                    };
                }
            }
        }
        let items: Map<String, Value> = items
            .into_iter()
            .map(|(ty, trie)| (ty.as_str().to_string(), Value::Object(trie)))
            .collect();
        let mut rv = Map::new();
        rv.insert("items".into(), Value::Object(items));
        rv.insert("url_base".into(), Value::String(self.url_base.clone()));
        rv.insert("version".into(), Value::from(CURRENT_VERSION));
        Value::Object(rv)
    }

    /// Serializes like `jsonlib.dump_dist`: sorted keys, one-space indent, `": "`
    /// separators, unescaped non-ASCII, and a trailing newline.
    pub fn to_writer(&self, mut writer: impl Write) -> io::Result<()> {
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(&mut writer, formatter);
        self.to_json().serialize(&mut ser)?;
        writer.write_all(b"\n")
    }

    /// Writes the manifest to `path`, via a temporary file that is renamed into place.
    pub fn write(&self, path: &Path) -> Result<(), Error> {
        let io_err = |e| Error::Io(path.to_path_buf(), e);
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(io_err)?;
        }
        let tmp = path.with_extension("json.tmp");
        let file = fs::File::create(&tmp).map_err(|e| Error::Io(tmp.clone(), e))?;
        let mut writer = io::BufWriter::new(file);
        self.to_writer(&mut writer).map_err(io_err)?;
        writer.flush().map_err(io_err)?;
        drop(writer);
        fs::rename(&tmp, path).map_err(io_err)
    }

    /// Builds the manifest for the whole tree rooted at `tests_root`, like
    /// `./wpt manifest --rebuild`.
    pub fn build(tests_root: &Path, url_base: impl Into<String>) -> Result<Self, Error> {
        let url_base = url_base.into();
        let files = list_files(tests_root)?
            .into_par_iter()
            .map(|(rel_path, is_symlink)| {
                let full = tests_root.join(&rel_path);
                let io_err = |e| Error::Io(full.clone(), e);
                let contents = fs::read(&full).map_err(io_err)?;
                let hash = if is_symlink {
                    let target = fs::read_link(&full).map_err(io_err)?;
                    git_blob_hash(target.as_os_str().as_encoded_bytes())
                } else {
                    git_blob_hash(&contents)
                };
                let source = SourceFile::new(&url_base, &rel_path, &contents);
                let (item_type, items) = source.manifest_items().map_err(Error::Manifest)?;
                Ok((
                    rel_path,
                    FileEntry {
                        item_type,
                        hash,
                        items,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        Ok(Manifest { url_base, files })
    }
}

/// Items sort by URL with `None` first, like `safe_sorter` in `typedata.py`.
fn sort_key(item: &Value) -> &str {
    item.get(0).and_then(Value::as_str).unwrap_or("")
}

/// The SHA-1 git assigns to a blob with these contents.
pub fn git_blob_hash(contents: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", contents.len()).as_bytes());
    hasher.update(contents);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The root `.gitignore` plus `.git/`, which is what `vcs.FileSystem` filters with.
fn root_gitignore(tests_root: &Path) -> Result<Gitignore, Error> {
    let mut builder = GitignoreBuilder::new(tests_root);
    let path = tests_root.join(".gitignore");
    if path.exists() {
        if let Some(e) = builder.add(&path) {
            return Err(Error::Walk(e));
        }
    }
    builder.add_line(None, ".git/").map_err(Error::Walk)?;
    builder.build().map_err(Error::Walk)
}

/// Lists every non-ignored file under `tests_root` as a sorted `/`-separated relative
/// path, along with whether it is a symlink.
fn list_files(tests_root: &Path) -> Result<Vec<(String, bool)>, Error> {
    let gitignore = root_gitignore(tests_root)?;
    let root = tests_root.to_path_buf();
    let walker = WalkBuilder::new(tests_root)
        .standard_filters(false)
        .follow_links(true)
        .filter_entry(move |entry| {
            let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
            let path = entry.path();
            path == root || !gitignore.matched(path, is_dir).is_ignore()
        })
        .build();
    let mut rv = Vec::new();
    for entry in walker {
        let entry = entry.map_err(Error::Walk)?;
        if entry.file_type().is_some_and(|t| t.is_dir()) {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(tests_root)
            .expect("walked path is under the root");
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        rv.push((rel, entry.path_is_symlink()));
    }
    rv.sort();
    Ok(rv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_hash() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(
            git_blob_hash(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn json_round_trip() {
        let json = serde_json::json!({
            "items": {"support": {"a": {"b.js": ["0123", [null, {}]]}}},
            "url_base": "/",
            "version": 9,
        });
        let manifest = Manifest::from_json(&json).unwrap();
        assert_eq!(manifest.to_json(), json);
        let mut out = Vec::new();
        manifest.to_writer(&mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "{\n \"items\": {\n  \"support\": {\n   \"a\": {\n    \"b.js\": [\n     \"0123\",\n     [\n      null,\n      {}\n     ]\n    ]\n   }\n  }\n },\n \"url_base\": \"/\",\n \"version\": 9\n}\n"
        );
    }
}
