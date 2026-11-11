//! Extraction of the `<meta>`, `<link>` and `<script>` elements that the manifest reads.
//!
//! HTML is parsed with html5ever and XHTML/SVG with xml5ever. Both drive the same
//! [`TreeSink`], which records matching elements in the XHTML namespace instead of building
//! a DOM, mirroring the `.//{http://www.w3.org/1999/xhtml}<name>` lookups in `sourcefile.py`.

use std::borrow::Cow;
use std::cell::{Ref, RefCell};

use html5ever::interface::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::{ns, Attribute, QualName};

use crate::encoding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    Meta,
    Link,
    Script,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub tag: Tag,
    /// Attributes without a namespace, in source order (first duplicate wins).
    pub attrs: Vec<(String, String)>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// The parsed metadata of a markup file (Python's `SourceFile.root`).
#[derive(Debug, Default)]
pub struct Root {
    pub elements: Vec<Element>,
}

impl Root {
    /// Equivalent to `root.findall(".//{xhtml}<tag>[@attr='value']")`.
    pub fn find_all(&self, tag: Tag, attr: &str, value: &str) -> std::vec::IntoIter<&Element> {
        self.elements
            .iter()
            .filter(|e| e.tag == tag && e.attr(attr) == Some(value))
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn all(&self, tag: Tag) -> impl Iterator<Item = &Element> + '_ {
        self.elements.iter().filter(move |e| e.tag == tag)
    }
}

#[derive(Default)]
struct Sink {
    names: RefCell<Vec<QualName>>,
    mathml_integration_points: RefCell<Vec<bool>>,
    elements: RefCell<Vec<Element>>,
    doctype_public_id: RefCell<Option<String>>,
}

impl Sink {
    fn new_node(&self, name: QualName, integration_point: bool) -> usize {
        let mut names = self.names.borrow_mut();
        names.push(name);
        self.mathml_integration_points
            .borrow_mut()
            .push(integration_point);
        names.len() - 1
    }
}

fn placeholder_name() -> QualName {
    QualName::new(None, ns!(), html5ever::local_name!(""))
}

impl TreeSink for Sink {
    type Handle = usize;
    type Output = Self;
    type ElemName<'a> = Ref<'a, QualName>;

    fn finish(self) -> Self {
        self
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> usize {
        if self.names.borrow().is_empty() {
            self.new_node(placeholder_name(), false);
        }
        0
    }

    fn elem_name<'a>(&'a self, target: &'a usize) -> Ref<'a, QualName> {
        Ref::map(self.names.borrow(), |names| &names[*target])
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> usize {
        self.get_document();
        if name.ns == ns!(html) {
            let tag = match &*name.local {
                "meta" => Some(Tag::Meta),
                "link" => Some(Tag::Link),
                "script" => Some(Tag::Script),
                _ => None,
            };
            if let Some(tag) = tag {
                let mut element = Element {
                    tag,
                    attrs: Vec::with_capacity(attrs.len()),
                };
                for attr in attrs {
                    if attr.name.ns == ns!() && element.attr(&attr.name.local).is_none() {
                        element
                            .attrs
                            .push((attr.name.local.to_string(), attr.value.to_string()));
                    }
                }
                self.elements.borrow_mut().push(element);
            }
        }
        self.new_node(name, flags.mathml_annotation_xml_integration_point)
    }

    fn create_comment(&self, _text: StrTendril) -> usize {
        self.new_node(placeholder_name(), false)
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> usize {
        self.new_node(placeholder_name(), false)
    }

    fn append(&self, _parent: &usize, _child: NodeOrText<usize>) {}

    fn append_based_on_parent_node(
        &self,
        _element: &usize,
        _prev_element: &usize,
        _child: NodeOrText<usize>,
    ) {
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        *self.doctype_public_id.borrow_mut() = Some(public_id.to_string());
    }

    fn get_template_contents(&self, target: &usize) -> usize {
        *target
    }

    fn same_node(&self, x: &usize, y: &usize) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, _sibling: &usize, _new_node: NodeOrText<usize>) {}

    fn add_attrs_if_missing(&self, _target: &usize, _attrs: Vec<Attribute>) {}

    fn remove_from_parent(&self, _target: &usize) {}

    fn reparent_children(&self, _node: &usize, _new_parent: &usize) {}

    fn is_mathml_annotation_xml_integration_point(&self, handle: &usize) -> bool {
        self.mathml_integration_points.borrow()[*handle]
    }
}

fn parse_html_str(text: &str) -> Sink {
    html5ever::parse_document(Sink::default(), Default::default()).one(text)
}

/// Encoding declarations made by `<meta>` elements, as html5lib's tree builder sees them.
fn meta_charsets(elements: &[Element]) -> Vec<String> {
    let mut rv = Vec::new();
    for e in elements.iter().filter(|e| e.tag == Tag::Meta) {
        if let Some(charset) = e.attr("charset") {
            rv.push(charset.to_string());
        } else if let (Some(content), Some(equiv)) = (e.attr("content"), e.attr("http-equiv")) {
            if equiv.eq_ignore_ascii_case("content-type") {
                if let Some(label) = encoding::content_attr_charset(content.as_bytes()) {
                    rv.push(String::from_utf8_lossy(label).into_owned());
                }
            }
        }
    }
    rv
}

/// Port of `html5lib.parse(f, treebuilder="etree", useChardet=False)`.
pub fn parse_html(data: &[u8]) -> Root {
    let decoded = encoding::decode_html(data);
    let mut sink = parse_html_str(&decoded.text);
    if !decoded.certain {
        let charsets = meta_charsets(&sink.elements.borrow());
        for label in charsets {
            match encoding::change_encoding(decoded.encoding, &label) {
                None => continue,
                Some(None) => break,
                Some(Some(new)) => {
                    let text = encoding::decode_with(data, new);
                    sink = parse_html_str(&text);
                    break;
                }
            }
        }
    }
    Root {
        elements: sink.elements.into_inner(),
    }
}

const XHTML_PUBLIC_IDS: &[&str] = &[
    "-//W3C//DTD XHTML 1.0 Transitional//EN",
    "-//W3C//DTD XHTML 1.1//EN",
    "-//W3C//DTD XHTML 1.0 Strict//EN",
    "-//W3C//DTD XHTML 1.0 Frameset//EN",
    "-//W3C//DTD XHTML Basic 1.0//EN",
    "-//W3C//DTD XHTML 1.1 plus MathML 2.0//EN",
    "-//W3C//DTD XHTML 1.1 plus MathML 2.0 plus SVG 1.1//EN",
    "-//W3C//DTD MathML 2.0//EN",
    "-//WAPFORUM//DTD XHTML Mobile 1.0//EN",
];

fn xml_declared_encoding(data: &[u8]) -> Option<&[u8]> {
    let decl = data.strip_prefix(b"<?xml")?;
    let end = decl.windows(2).position(|w| w == b"?>")?;
    let decl = &decl[..end];
    let pos = decl.windows(8).position(|w| w == b"encoding")? + 8;
    let rest = decl[pos..]
        .trim_ascii_start()
        .strip_prefix(b"=")?
        .trim_ascii_start();
    let quote = *rest.first()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let rest = &rest[1..];
    let end = rest.iter().position(|b| *b == quote)?;
    Some(&rest[..end])
}

fn decode_xml(data: &[u8]) -> Option<String> {
    let (encoding, data) = match encoding::detect_bom(data) {
        Some((encoding, skip)) => (encoding, &data[skip..]),
        None => match xml_declared_encoding(data) {
            Some(label) => (encoding::lookup_encoding(label)?, data),
            None => (encoding_rs::UTF_8, data),
        },
    };
    encoding
        .decode_without_bom_handling_and_without_replacement(data)
        .map(Cow::into_owned)
}

/// Names of the entities declared in the doctype's internal subset.
fn internal_subset_entities(text: &str) -> Vec<&str> {
    let Some(start) = text.find("<!DOCTYPE") else {
        return Vec::new();
    };
    let doctype = &text[start..];
    let Some(open) = doctype.find('[').filter(|i| !doctype[..*i].contains('>')) else {
        return Vec::new();
    };
    let subset = &doctype[open..doctype.find("]>").unwrap_or(doctype.len())];
    subset
        .split("<!ENTITY")
        .skip(1)
        .filter_map(|decl| decl.split_whitespace().next())
        .filter(|name| *name != "%")
        .collect()
}

/// Whether the text references named entities that are neither predefined by XML nor
/// declared in the internal subset. Python's ElementTree only resolves those (from
/// `catalog/xhtml.dtd`) for XHTML doctypes.
fn uses_undeclared_entities(text: &str) -> bool {
    let declared = internal_subset_entities(text);
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        rest = &rest[i + 1..];
        let len = rest
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ':')))
            .unwrap_or(rest.len());
        if len > 0
            && rest[len..].starts_with(';')
            && !matches!(&rest[..len], "lt" | "gt" | "amp" | "quot" | "apos")
            && !declared.contains(&&rest[..len])
        {
            return true;
        }
    }
    false
}

/// Port of `ElementTree.parse` with the XHTML-entity fallback in `sourcefile._parse_xml`.
///
/// Returns `None` where Python's parser raises for undecodable documents or undeclared
/// entities. xml5ever's other parse errors are recoverable and are ignored: it reports
/// false positives on well-formed WPT files (internal DTD subsets, `<!-->` comments,
/// numeric references to C1 controls), and no file in WPT is otherwise malformed.
pub fn parse_xml(data: &[u8]) -> Option<Root> {
    let text = decode_xml(data)?;
    let sink =
        xml5ever::driver::parse_document(Sink::default(), Default::default()).one(text.as_str());
    if uses_undeclared_entities(&text) {
        let public_id = sink.doctype_public_id.borrow();
        if !public_id
            .as_deref()
            .is_some_and(|id| XHTML_PUBLIC_IDS.contains(&id))
        {
            return None;
        }
    }
    Some(Root {
        elements: sink.elements.into_inner(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html() {
        let root = parse_html(
            b"<!doctype html><meta name=timeout content=long><script src=/resources/testharness.js></script>\
              <svg><script src=/resources/testdriver.js></script></svg><link rel=match href=a.html>",
        );
        assert_eq!(root.find_all(Tag::Meta, "name", "timeout").count(), 1);
        assert_eq!(root.all(Tag::Script).count(), 1);
        assert_eq!(
            root.all(Tag::Link).next().unwrap().attr("href"),
            Some("a.html")
        );
    }

    #[test]
    fn xml() {
        let root = parse_xml(
            br#"<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><link rel="match" href="a.xht"/></head></html>"#,
        )
        .unwrap();
        assert_eq!(root.find_all(Tag::Link, "rel", "match").count(), 1);
        let svg = parse_xml(
            br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:h="http://www.w3.org/1999/xhtml"><h:link rel="match" href="a.svg"/><script href="x"/></svg>"#,
        )
        .unwrap();
        assert_eq!(svg.elements.len(), 1);
        assert!(
            parse_xml(b"<html xmlns='http://www.w3.org/1999/xhtml'><p>&nbsp;</p></html>").is_none()
        );
        assert!(parse_xml(
            b"<!DOCTYPE svg [<!ENTITY tree 'x'>]><svg xmlns='http://www.w3.org/2000/svg'>&tree;</svg>"
        )
        .is_some());
    }
}
