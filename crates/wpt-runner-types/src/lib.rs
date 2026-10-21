//! Types and parsers for metadata used when running Web Platform Tests:
//!
//! - [`script_metadata`]: the `// META:` comment block at the top of JS-file
//!   tests (`.any.js`, `.window.js`, `.worker.js`, etc)
//! - [`fuzzy`]: reftest fuzziness specifications (`<meta name=fuzzy>`)

pub mod fuzzy;
pub mod script_metadata;

pub use fuzzy::{tolerance_for_reference, FuzzyParseError, FuzzyRange, FuzzySpec, FuzzyTolerance};
pub use script_metadata::{
    is_valid_variant, parse_script_metadata, GlobalScope, ScriptMetadata, ScriptMetadataExt,
    Timeout,
};
