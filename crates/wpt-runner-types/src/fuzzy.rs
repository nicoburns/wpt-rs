//! Parsing of reftest fuzziness specifications, which specify tolerances for
//! reftest image comparison. These appear in `<meta name=fuzzy content=...>`
//! tags in tests. See
//! <https://web-platform-tests.org/writing-tests/reftests.html#fuzzy-matching>
//!
//! The `content` attribute has the form `[ <ref-name> ":" ] <fuzzy-value>`
//! where `<fuzzy-value>` is `maxDifference=<range>;totalPixels=<range>` (the
//! key names are optional, in which case the ranges are positional:
//! maxDifference first, totalPixels second) and `<range>` is either a single
//! number or `<min>-<max>`.
//!
//! Parsing mirrors `SourceFile.fuzzy` in upstream wpt's
//! `tools/manifest/sourcefile.py`, and rejects the same malformed values.

use std::fmt;
use std::str::FromStr;

/// An inclusive `min..=max` range in a fuzziness specification
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FuzzyRange {
    pub min: u64,
    pub max: u64,
}

impl FuzzyRange {
    /// Does `value` fall within this (inclusive) range?
    pub fn contains(&self, value: u64) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

impl FromStr for FuzzyRange {
    type Err = FuzzyParseError;

    /// Parses `<number>` or `<min>-<max>`
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let parse = |num: &str| {
            num.trim()
                .parse()
                .map_err(|_| FuzzyParseError::InvalidNumber(num.trim().to_string()))
        };
        let (min, max) = match s.split_once('-') {
            Some((min, max)) => (parse(min)?, parse(max)?),
            None => {
                let value = parse(s)?;
                (value, value)
            }
        };
        if min > max {
            return Err(FuzzyParseError::InvertedRange { min, max });
        }
        Ok(Self { min, max })
    }
}

/// The tolerances of a fuzziness specification: the allowed range for the
/// maximum per-channel colour difference, and the allowed range for the total
/// number of differing pixels
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FuzzyTolerance {
    pub max_difference: FuzzyRange,
    pub total_pixels: FuzzyRange,
}

/// A complete fuzziness specification: a tolerance, optionally scoped to a
/// specific reference file
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzySpec {
    /// Reference file this tolerance applies to (`None` = all references)
    pub reference: Option<String>,
    pub tolerance: FuzzyTolerance,
}

impl FromStr for FuzzySpec {
    type Err = FuzzyParseError;

    /// Parses the content of a `<meta name=fuzzy>` tag:
    /// `[ <ref-name> ":" ] <fuzzy-value>`
    fn from_str(content: &str) -> Result<Self, Self::Err> {
        let content = content.trim();
        let (reference, value) = match content.split_once(':') {
            Some((prefix, rest)) if !prefix.contains('=') => {
                (Some(prefix.trim().to_string()), rest)
            }
            _ => (None, content),
        };

        let parts: Vec<&str> = value.split(';').collect();
        if parts.len() != 2 {
            return Err(FuzzyParseError::WrongPartCount(parts.len()));
        }

        let mut max_difference = None;
        let mut total_pixels = None;
        let mut positional: Vec<FuzzyRange> = Vec::new();
        for part in parts {
            let part = part.trim();
            match part.split_once('=') {
                Some((name, range)) => {
                    let slot = match name.trim() {
                        "maxDifference" => &mut max_difference,
                        "totalPixels" => &mut total_pixels,
                        other => return Err(FuzzyParseError::InvalidProperty(other.to_string())),
                    };
                    if slot.is_some() {
                        return Err(FuzzyParseError::DuplicateProperty(name.trim().to_string()));
                    }
                    *slot = Some(range.parse()?);
                }
                None => positional.push(part.parse()?),
            }
        }

        // Positional ranges fill the unnamed slots in order:
        // maxDifference first, then totalPixels
        let mut positional = positional.into_iter();
        let max_difference = match max_difference {
            Some(range) => range,
            None => positional.next().ok_or(FuzzyParseError::MissingProperty(
                "maxDifference".to_string(),
            ))?,
        };
        let total_pixels = match total_pixels {
            Some(range) => range,
            None => positional
                .next()
                .ok_or(FuzzyParseError::MissingProperty("totalPixels".to_string()))?,
        };

        Ok(FuzzySpec {
            reference,
            tolerance: FuzzyTolerance {
                max_difference,
                total_pixels,
            },
        })
    }
}

/// An error parsing a fuzziness specification
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FuzzyParseError {
    /// The value doesn't have exactly two `;`-separated parts
    WrongPartCount(usize),
    /// A named property other than `maxDifference` / `totalPixels`
    InvalidProperty(String),
    /// The same property was specified twice
    DuplicateProperty(String),
    /// A property has no named or positional value
    MissingProperty(String),
    /// A range bound isn't a valid non-negative integer
    InvalidNumber(String),
    /// A range's minimum exceeds its maximum
    InvertedRange { min: u64, max: u64 },
}

impl fmt::Display for FuzzyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPartCount(count) => {
                write!(f, "expected 2 semicolon-separated parts, got {count}")
            }
            Self::InvalidProperty(name) => write!(f, "{name} is not a valid fuzzy property"),
            Self::DuplicateProperty(name) => write!(f, "got multiple values for {name}"),
            Self::MissingProperty(name) => write!(f, "no value for {name}"),
            Self::InvalidNumber(value) => write!(f, "{value:?} is not a non-negative integer"),
            Self::InvertedRange { min, max } => {
                write!(f, "range minimum {min} exceeds maximum {max}")
            }
        }
    }
}

impl std::error::Error for FuzzyParseError {}

/// Finds the tolerance applicable to `ref_file`: a spec naming that reference
/// takes precedence over an unnamed one.
pub fn tolerance_for_reference<'a>(
    specs: &'a [FuzzySpec],
    ref_file: &str,
) -> Option<&'a FuzzyTolerance> {
    specs
        .iter()
        .find(|spec| spec.reference.as_deref() == Some(ref_file))
        .or_else(|| specs.iter().find(|spec| spec.reference.is_none()))
        .map(|spec| &spec.tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(min: u64, max: u64) -> FuzzyRange {
        FuzzyRange { min, max }
    }

    #[test]
    fn parse_named_ranges() {
        let spec: FuzzySpec = "maxDifference=0-2;totalPixels=0-40".parse().unwrap();
        assert_eq!(spec.reference, None);
        assert_eq!(spec.tolerance.max_difference, range(0, 2));
        assert_eq!(spec.tolerance.total_pixels, range(0, 40));
    }

    #[test]
    fn parse_named_ranges_reversed_order() {
        let spec: FuzzySpec = "totalPixels=0-40;maxDifference=0-2".parse().unwrap();
        assert_eq!(spec.tolerance.max_difference, range(0, 2));
        assert_eq!(spec.tolerance.total_pixels, range(0, 40));
    }

    #[test]
    fn parse_positional_ranges() {
        let spec: FuzzySpec = "2;40".parse().unwrap();
        assert_eq!(spec.tolerance.max_difference, range(2, 2));
        assert_eq!(spec.tolerance.total_pixels, range(40, 40));
    }

    #[test]
    fn parse_mixed_named_and_positional() {
        let spec: FuzzySpec = "maxDifference=0-2;40".parse().unwrap();
        assert_eq!(spec.tolerance.max_difference, range(0, 2));
        assert_eq!(spec.tolerance.total_pixels, range(40, 40));
    }

    #[test]
    fn parse_per_reference() {
        let spec: FuzzySpec = "ref.html:maxDifference=0-2;totalPixels=0-40"
            .parse()
            .unwrap();
        assert_eq!(spec.reference.as_deref(), Some("ref.html"));
        assert_eq!(spec.tolerance.max_difference, range(0, 2));
    }

    #[test]
    fn parse_errors() {
        assert_eq!(
            "garbage".parse::<FuzzySpec>(),
            Err(FuzzyParseError::WrongPartCount(1))
        );
        assert_eq!(
            "maxDifference=0-2".parse::<FuzzySpec>(),
            Err(FuzzyParseError::WrongPartCount(1))
        );
        assert_eq!(
            "maxDifference=0-2;maxDifference=3".parse::<FuzzySpec>(),
            Err(FuzzyParseError::DuplicateProperty(
                "maxDifference".to_string()
            ))
        );
        assert_eq!(
            "bogus=1;2".parse::<FuzzySpec>(),
            Err(FuzzyParseError::InvalidProperty("bogus".to_string()))
        );
        assert_eq!(
            "x;2".parse::<FuzzySpec>(),
            Err(FuzzyParseError::InvalidNumber("x".to_string()))
        );
        assert_eq!(
            "5-2;2".parse::<FuzzySpec>(),
            Err(FuzzyParseError::InvertedRange { min: 5, max: 2 })
        );
    }

    #[test]
    fn range_contains() {
        assert!(range(0, 2).contains(2));
        assert!(!range(0, 2).contains(3));
    }

    #[test]
    fn tolerance_selection() {
        let specs = vec![
            FuzzySpec {
                reference: None,
                tolerance: FuzzyTolerance {
                    max_difference: range(0, 1),
                    total_pixels: range(0, 1),
                },
            },
            FuzzySpec {
                reference: Some("ref.html".to_string()),
                tolerance: FuzzyTolerance {
                    max_difference: range(0, 2),
                    total_pixels: range(0, 2),
                },
            },
        ];
        assert_eq!(
            tolerance_for_reference(&specs, "ref.html")
                .unwrap()
                .max_difference,
            range(0, 2)
        );
        assert_eq!(
            tolerance_for_reference(&specs, "other.html")
                .unwrap()
                .max_difference,
            range(0, 1)
        );
        assert_eq!(tolerance_for_reference(&[], "ref.html"), None);
    }
}
