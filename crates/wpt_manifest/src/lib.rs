//! A Rust port of the `MANIFEST.json` generator in web-platform-tests' `tools/manifest`.
//!
//! [`Manifest::update`] walks a WPT checkout, classifies each file the same way as
//! `sourcefile.py`, and produces a manifest that serializes byte-for-byte like
//! `wpt manifest`. Files whose git blob hash is unchanged from a previously loaded
//! manifest are not reparsed.

mod encoding;
mod manifest;
mod markup;
mod script_meta;
mod sourcefile;
mod test262;
mod urls;

pub use manifest::{Error, Manifest, UpdateStats, CURRENT_VERSION};

use std::fmt;
use std::str::FromStr;

/// A manifest item type: a top-level key of `items` in `MANIFEST.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemType {
    Aamtest,
    ConformanceChecker,
    Crashtest,
    Manual,
    PrintReftest,
    Reftest,
    Support,
    Test262,
    Testharness,
    Visual,
    Wdspec,
}

impl ItemType {
    pub const ALL: [ItemType; 11] = [
        ItemType::Aamtest,
        ItemType::ConformanceChecker,
        ItemType::Crashtest,
        ItemType::Manual,
        ItemType::PrintReftest,
        ItemType::Reftest,
        ItemType::Support,
        ItemType::Test262,
        ItemType::Testharness,
        ItemType::Visual,
        ItemType::Wdspec,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Aamtest => "aamtest",
            ItemType::ConformanceChecker => "conformancechecker",
            ItemType::Crashtest => "crashtest",
            ItemType::Manual => "manual",
            ItemType::PrintReftest => "print-reftest",
            ItemType::Reftest => "reftest",
            ItemType::Support => "support",
            ItemType::Test262 => "test262",
            ItemType::Testharness => "testharness",
            ItemType::Visual => "visual",
            ItemType::Wdspec => "wdspec",
        }
    }
}

impl fmt::Display for ItemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ItemType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        ItemType::ALL
            .into_iter()
            .find(|t| t.as_str() == s)
            .ok_or_else(|| format!("unknown manifest item type {s:?}"))
    }
}
