//! Port of `tools/manifest/test262.py`: reads the YAML frontmatter of Test262 tests.

use saphyr::{LoadableYamlNode, ScalarOwned, YamlOwned};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TestRecord {
    pub includes: Option<Vec<String>>,
    pub is_only_strict: bool,
    pub is_module: bool,
    pub is_async: bool,
}

/// Equivalent to `re.search(r"/\*---(.*)---\*/", src, re.DOTALL).group(1)` (greedy).
fn find_attrs(src: &str) -> Option<&str> {
    let start = src.find("/*---")? + 5;
    let end = src.rfind("---*/")?;
    (end >= start).then(|| &src[start..end])
}

fn as_str(node: &YamlOwned) -> Option<String> {
    match node {
        YamlOwned::Value(ScalarOwned::String(s)) => Some(s.clone()),
        YamlOwned::Representation(s, _, _) => Some(s.clone()),
        YamlOwned::Value(ScalarOwned::Integer(i)) => Some(i.to_string()),
        YamlOwned::Value(ScalarOwned::FloatingPoint(f)) => Some(f.to_string()),
        YamlOwned::Value(ScalarOwned::Boolean(b)) => {
            Some(if *b { "True" } else { "False" }.to_string())
        }
        _ => None,
    }
}

/// Python iterates whatever value `includes`/`flags` hold; a string iterates its characters.
fn as_list(node: &YamlOwned) -> Vec<String> {
    match node {
        YamlOwned::Sequence(seq) => seq.iter().filter_map(as_str).collect(),
        YamlOwned::Value(ScalarOwned::String(s)) => s.chars().map(String::from).collect(),
        _ => Vec::new(),
    }
}

fn parse_yaml_attrs(record: &mut TestRecord, attrs: &str, name: &str) {
    let attrs: String = attrs
        .chars()
        .map(|c| {
            if ('\x7f'..='\u{9f}').contains(&c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let docs = match YamlOwned::load_from_str(&attrs) {
        Ok(docs) => docs,
        Err(err) => {
            eprintln!("Failed to parse yaml in name {name}: {err}");
            return;
        }
    };
    let Some(YamlOwned::Mapping(mapping)) = docs.into_iter().next() else {
        eprintln!("Failed to parse yaml in name {name}");
        return;
    };
    for (key, value) in mapping.iter() {
        match as_str(key).as_deref() {
            Some("flags") => {
                if let YamlOwned::Sequence(_) = value {
                    for flag in as_list(value) {
                        match flag.as_str() {
                            "onlyStrict" => record.is_only_strict = true,
                            "module" => record.is_module = true,
                            "async" => record.is_async = true,
                            _ => {}
                        }
                    }
                }
            }
            Some("includes") => record.includes = Some(as_list(value)),
            _ => {}
        }
    }
}

/// Returns `None` for fixtures and files without frontmatter (which are support files).
pub fn parse(src: &str, name: &str) -> Option<TestRecord> {
    if name.ends_with("_FIXTURE.js") {
        return None;
    }
    let Some(attrs) = find_attrs(src) else {
        eprintln!("Missing frontmatter: {name}");
        return None;
    };
    let mut record = TestRecord::default();
    let attrs = attrs.trim();
    if !attrs.is_empty() {
        parse_yaml_attrs(&mut record, attrs, name);
    }
    Some(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter() {
        let src = "// Copyright\n/*---\ndescription: x\nincludes: [compareArray.js, sta.js]\nflags: [onlyStrict, async]\n---*/\nfoo();\n";
        let record = parse(src, "a.js").unwrap();
        assert_eq!(
            record.includes,
            Some(vec!["compareArray.js".to_string(), "sta.js".to_string()])
        );
        assert!(record.is_only_strict && record.is_async && !record.is_module);
        assert_eq!(parse("foo();", "a.js"), None);
        assert_eq!(parse(src, "a_FIXTURE.js"), None);
        assert_eq!(parse("/*------*/", "a.js"), Some(TestRecord::default()));
    }
}
