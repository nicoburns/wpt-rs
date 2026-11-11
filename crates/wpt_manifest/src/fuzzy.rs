//! Typed access to reftest fuzzy-matching tolerances (`<meta name=fuzzy>`), as stored in
//! the `fuzzy` extra of a manifest reftest item. See
//! <https://web-platform-tests.org/writing-tests/reftests.html#fuzzy-matching>

use serde_json::Value;

/// An inclusive range of allowed values.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FuzzyRange {
    pub min: u64,
    pub max: u64,
}

/// The allowed per-channel colour difference and number of differing pixels.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FuzzyTolerance {
    pub max_difference: FuzzyRange,
    pub total_pixels: FuzzyRange,
}

/// One `<meta name=fuzzy>` tolerance of a reftest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzySpec {
    /// Reference URL this tolerance applies to (`None` = all references)
    pub reference: Option<String>,
    pub tolerance: FuzzyTolerance,
}

impl FuzzySpec {
    /// Parses one manifest fuzzy entry,
    /// `[null | [test, reference, relation], [[min, max], [min, max]]]`.
    pub fn from_json(entry: &Value) -> Option<FuzzySpec> {
        let reference = entry
            .get(0)?
            .get(1)
            .and_then(Value::as_str)
            .map(str::to_string);
        let range = |value: &Value| {
            Some(FuzzyRange {
                min: value.get(0)?.as_u64()?,
                max: value.get(1)?.as_u64()?,
            })
        };
        let ranges = entry.get(1)?;
        Some(FuzzySpec {
            reference,
            tolerance: FuzzyTolerance {
                max_difference: range(ranges.get(0)?)?,
                total_pixels: range(ranges.get(1)?)?,
            },
        })
    }

    /// The fuzzy specs in the extras object of a manifest reftest item (the item's last
    /// element). Malformed entries are skipped.
    pub fn from_extras(extras: &Value) -> Vec<FuzzySpec> {
        extras
            .get("fuzzy")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(FuzzySpec::from_json)
            .collect()
    }
}

/// Finds the tolerance applicable to `reference`: a spec naming that reference takes
/// precedence over an unnamed one.
pub fn tolerance_for_reference<'a>(
    specs: &'a [FuzzySpec],
    reference: &str,
) -> Option<&'a FuzzyTolerance> {
    specs
        .iter()
        .find(|spec| spec.reference.as_deref() == Some(reference))
        .or_else(|| specs.iter().find(|spec| spec.reference.is_none()))
        .map(|spec| &spec.tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sourcefile::SourceFile;
    use serde_json::json;

    fn range(min: u64, max: u64) -> FuzzyRange {
        FuzzyRange { min, max }
    }

    fn tolerance(max_difference: FuzzyRange, total_pixels: FuzzyRange) -> FuzzyTolerance {
        FuzzyTolerance {
            max_difference,
            total_pixels,
        }
    }

    #[test]
    fn from_extras() {
        let extras = json!({"fuzzy": [
            [null, [[1, 2], [0, 5]]],
            [["/css/a.html", "/css/a-ref.html", "=="], [[0, 3], [10, 10]]],
            [null, "malformed"],
        ]});
        assert_eq!(
            FuzzySpec::from_extras(&extras),
            [
                FuzzySpec {
                    reference: None,
                    tolerance: tolerance(range(1, 2), range(0, 5)),
                },
                FuzzySpec {
                    reference: Some("/css/a-ref.html".to_string()),
                    tolerance: tolerance(range(0, 3), range(10, 10)),
                },
            ]
        );
        assert_eq!(FuzzySpec::from_extras(&json!({})), []);
    }

    #[test]
    fn from_sourcefile_items() {
        let source = SourceFile::new(
            "/",
            "css/a.html",
            b"<link rel=match href=a-ref.html><link rel=match href=b-ref.html>\
              <meta name=fuzzy content='0-2;0-10'>\
              <meta name=fuzzy content='b-ref.html:totalPixels=5;maxDifference=3'>",
        );
        let (_, items) = source.manifest_items().unwrap();
        let specs = FuzzySpec::from_extras(items[0].as_array().unwrap().last().unwrap());
        assert_eq!(
            tolerance_for_reference(&specs, "/css/a-ref.html"),
            Some(&tolerance(range(0, 2), range(0, 10)))
        );
        assert_eq!(
            tolerance_for_reference(&specs, "/css/b-ref.html"),
            Some(&tolerance(range(3, 3), range(5, 5)))
        );
    }

    #[test]
    fn tolerance_selection() {
        let specs = vec![
            FuzzySpec {
                reference: None,
                tolerance: tolerance(range(0, 1), range(0, 1)),
            },
            FuzzySpec {
                reference: Some("ref.html".to_string()),
                tolerance: tolerance(range(0, 2), range(0, 2)),
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
