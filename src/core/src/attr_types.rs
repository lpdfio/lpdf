//! The types of the attributes, from lpdf.xsd: every attribute of a document is checked against its element's
//! table in `schema_attrs.rs` before the parser reads it, on both paths.
//!
//! - **XML:** a value that doesn't parse as its type is an error, and so is an attribute the element doesn't have.
//! - **Kit tree:** an attribute takes the JSON type of its schema type, a number for a number, `true` or `false` for a
//!   boolean and a string for the rest, and any other JSON type is an error. Each typed value is then written as its
//!   text, so the parser and `kit_to_xml` read every attribute as text, as they read XML.
//!
//! Left out on both paths: the `data-*` attributes, which bind a template to its data, and namespaced ones such as
//! `xsi:noNamespaceSchemaLocation`, which point an editor at the schema. An element the schema doesn't have, or has
//! elsewhere, is left to the parser, which says what to write instead.

use serde_json::{Map, Value};

use crate::parse::node_loc;
use crate::schema_attrs::{Attributes, attributes_of};

/// One end of the range of a number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Bound {
    Open,
    Inclusive(f64),
    Exclusive(f64),
}

/// The type of an attribute: what its value has to be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Text,
    Boolean,
    Integer { min: Option<i64>, max: Option<i64> },
    Decimal { min: Bound, max: Bound },
    /// A name, or a number in the range: `line-height`, a slot of the tokens or a factor.
    NameOrDecimal { min: Bound, max: Bound },
}

impl Kind {
    /// What a value of this kind is, for an error: `a whole number from 1 to 12`.
    fn describe(self) -> String {
        match self {
            Kind::Text => "a string".to_string(),
            Kind::Boolean => "true or false".to_string(),
            Kind::Integer { min, max } => match (min, max) {
                (Some(min), Some(max)) => format!("a whole number from {min} to {max}"),
                (Some(min), None) => format!("a whole number of {min} or more"),
                (None, Some(max)) => format!("a whole number of {max} or less"),
                (None, None) => "a whole number".to_string(),
            },
            Kind::Decimal { min, max } => describe_decimal(min, max),
            Kind::NameOrDecimal { min, max } => format!("a name or {}", describe_decimal(min, max)),
        }
    }
}

fn describe_decimal(min: Bound, max: Bound) -> String {
    if let (Bound::Inclusive(min), Bound::Inclusive(max)) = (min, max) {
        return format!("a number from {min} to {max}");
    }
    let lower = match min {
        Bound::Open => None,
        Bound::Inclusive(min) => Some(format!("of {min} or more")),
        Bound::Exclusive(min) => Some(format!("above {min}")),
    };
    let upper = match max {
        Bound::Open => None,
        Bound::Inclusive(max) => Some(format!("of {max} or less")),
        Bound::Exclusive(max) => Some(format!("below {max}")),
    };
    match (lower, upper) {
        (Some(lower), Some(upper)) => format!("a number {lower} and {upper}"),
        (Some(end), None) | (None, Some(end)) => format!("a number {end}"),
        (None, None) => "a number".to_string(),
    }
}

fn within(value: f64, min: Bound, max: Bound) -> bool {
    let above = match min {
        Bound::Open => true,
        Bound::Inclusive(min) => value >= min,
        Bound::Exclusive(min) => value > min,
    };
    let below = match max {
        Bound::Open => true,
        Bound::Inclusive(max) => value <= max,
        Bound::Exclusive(max) => value < max,
    };
    above && below
}

fn integer_within(value: i64, min: Option<i64>, max: Option<i64>) -> bool {
    min.is_none_or(|min| value >= min) && max.is_none_or(|max| value <= max)
}

/// True for a decimal as XSD writes it: an optional sign, then digits with an optional point, such as `1.5`, `-2`
/// or `.5`. Rust's own parser also takes `1e3`, `inf` and `NaN`, which the schema doesn't.
fn is_decimal(text: &str) -> bool {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    !(whole.is_empty() && fraction.is_empty())
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.bytes().all(|b| b.is_ascii_digit())
}

/// True when `text` is a value of `kind` as XML writes it.
fn text_fits(kind: Kind, text: &str) -> bool {
    match kind {
        Kind::Text => true,
        Kind::Boolean => text == "true" || text == "false",
        Kind::Integer { min, max } => text.parse::<i64>().is_ok_and(|value| integer_within(value, min, max)),
        Kind::Decimal { min, max } => is_decimal(text) && text.parse::<f64>().is_ok_and(|value| within(value, min, max)),
        // A name is the parser's to read: it is a slot or an alias of the tokens, or an error there.
        Kind::NameOrDecimal { min, max } => {
            !is_decimal(text) || text.parse::<f64>().is_ok_and(|value| within(value, min, max))
        }
    }
}

/// What a kit tree writes for a value of `kind`: its text, or `None` when its JSON type or its value is wrong.
fn json_text(kind: Kind, value: &Value) -> Option<String> {
    match (kind, value) {
        (Kind::Text, Value::String(text)) => Some(text.clone()),
        (Kind::Boolean, Value::Bool(flag)) => Some(flag.to_string()),
        // JSON has one number type, and a serializer may write a whole number as 3.0: Python's json.dumps does for
        // a float. It is a whole number all the same; 3.5 is not.
        (Kind::Integer { min, max }, Value::Number(number)) => {
            let whole = number.as_i64().or_else(|| {
                number.as_f64().filter(|f| f.fract() == 0.0 && f.abs() < i64::MAX as f64).map(|f| f as i64)
            })?;
            integer_within(whole, min, max).then(|| whole.to_string())
        }
        (Kind::Decimal { min, max } | Kind::NameOrDecimal { min, max }, Value::Number(number)) => {
            let value = number.as_f64()?;
            // Rust writes a float without an exponent, as the XML path reads it: 1e-7 is 0.0000001.
            within(value, min, max).then(|| value.to_string())
        }
        // A factor written as a string is refused, as "3" is for a whole number: the tree has one form for each value.
        (Kind::NameOrDecimal { .. }, Value::String(name)) => (!is_decimal(name)).then(|| name.clone()),
        _ => None,
    }
}

/// The kind of the attribute `name` of the element `element` written in `parent`: `Kind::Text` for an attribute or an
/// element the table doesn't list.
pub(crate) fn kind_of(parent: &str, element: &str, name: &str) -> Kind {
    attributes_of(parent, element)
        .and_then(|attributes| attributes.iter().find(|(known, _)| *known == name))
        .map_or(Kind::Text, |(_, kind)| *kind)
}

/// A value of `kind` as XML writes it, as a number, or `None` for one that is text: a name, or any value of a kind
/// other than a number. `text` is a value the check has let through.
pub(crate) fn number_text(kind: Kind, text: &str) -> Option<String> {
    match kind {
        Kind::Integer { .. } => text.parse::<i64>().ok().map(|value| value.to_string()),
        Kind::Decimal { .. } | Kind::NameOrDecimal { .. } if is_decimal(text) => {
            text.parse::<f64>().ok().map(|value| value.to_string())
        }
        _ => None,
    }
}

fn names(attributes: Attributes, extra: &[&str]) -> String {
    attributes.iter().map(|(name, _)| *name).chain(extra.iter().copied()).collect::<Vec<_>>().join(", ")
}

/// The error for an attribute the element doesn't have: what to write instead, for a name the element used to
/// take, or else the names it takes.
fn unknown(element: &str, name: &str, expected: String) -> String {
    match removed(element, name) {
        Some(hint) => hint,
        None => format!("unknown attribute '{name}' — expected {expected}"),
    }
}

/// What to write instead of an attribute an element no longer has.
fn removed(element: &str, name: &str) -> Option<String> {
    const PRIMITIVES: [&str; 8] = ["rect", "circle", "ellipse", "line", "path", "text-at", "img-at", "barcode-at"];
    let hint = match (element, name) {
        ("document" | "section", "margin") => return Some(crate::parse::MARGIN_MOVED.to_string()),
        ("barcode" | "barcode-at", "size") => "removed; use 'width' or 'height', which a QR code takes either of",
        (primitive, "opacity") if PRIMITIVES.contains(&primitive) => {
            "removed; put the element in a <layer> with opacity, which fades everything in it"
        }
        ("rect" | "text-at" | "img-at", "w") => "renamed to 'width'",
        ("rect" | "img-at", "h") => "renamed to 'height'",
        ("circle", "cx" | "cy") => {
            "removed; 'x' and 'y' place the circle's box, 2r square, so write x = cx - r and y = cy - r"
        }
        ("ellipse", "cx" | "cy") => {
            "removed; 'x' and 'y' place the ellipse's box, 2rx wide and 2ry high, so write x = cx - rx and y = cy - ry"
        }
        _ => return None,
    };
    Some(format!("'{name}' {hint}"))
}

/// True for an attribute that no element's table lists: data binding, or a namespaced one.
fn left_out(name: &str) -> bool {
    name.starts_with("data-") || name.contains(':')
}

// ── XML ───────────────────────────────────────────────────────────────────────

/// Check the attributes of `root` and of every element in it, each against the element its name stands for where
/// it is written.
pub(crate) fn check_xml(root: roxmltree::Node) -> Result<(), String> {
    for element in root.descendants().filter(roxmltree::Node::is_element) {
        let parent = element.parent_element().map(|p| p.tag_name().name()).unwrap_or("");
        let tag = element.tag_name().name();
        let Some(attributes) = attributes_of(parent, tag) else { continue };
        for attribute in element.attributes() {
            let name = attribute.name();
            if attribute.namespace().is_some() || left_out(name) {
                continue;
            }
            let problem = match attributes.iter().find(|(known, _)| *known == name) {
                None => unknown(tag, name, names(attributes, &[])),
                Some((_, kind)) if !text_fits(*kind, attribute.value()) => {
                    format!("{name} must be {}, not \"{}\"", kind.describe(), attribute.value())
                }
                Some(_) => continue,
            };
            return Err(format!("<{tag}>{}: {problem}", node_loc(&element)));
        }
    }
    Ok(())
}

// ── Kit tree ──────────────────────────────────────────────────────────────────

/// The keys of the document's `attrs` that stand for elements rather than attributes.
const DOCUMENT_PARTS: [&str; 5] = ["assets", "tokens", "meta", "output", "attachments"];

/// Check the attributes of a kit tree and write each typed value as its text, in place.
pub(crate) fn check_tree(root: &mut Value) -> Result<(), String> {
    if let Some(attrs) = root.get_mut("attrs").and_then(Value::as_object_mut) {
        check_document(attrs)?;
    }
    for node in root.get_mut("nodes").and_then(Value::as_array_mut).into_iter().flatten() {
        check_node(node, "document")?;
    }
    Ok(())
}

/// Check the attributes in `attrs` against `attributes`, and the keys in `extra` as known names that are not
/// attributes. Each error starts with `prefix`.
fn check_attrs(prefix: &str, element: &str, attributes: Attributes, extra: &[&str], attrs: &mut Map<String, Value>) -> Result<(), String> {
    for (name, value) in attrs.iter_mut() {
        if left_out(name) || extra.contains(&name.as_str()) || value.is_null() {
            continue;
        }
        let Some((_, kind)) = attributes.iter().find(|(known, _)| known == name) else {
            return Err(format!("{prefix}: {}", unknown(element, name, names(attributes, extra))));
        };
        match json_text(*kind, value) {
            Some(text) => *value = Value::String(text),
            None => return Err(format!("{prefix}: {name} must be {}, not {value}", kind.describe())),
        }
    }
    Ok(())
}

fn check_element(prefix: &str, parent: &str, element: &str, extra: &[&str], attrs: &mut Map<String, Value>) -> Result<(), String> {
    match attributes_of(parent, element) {
        Some(attributes) => check_attrs(prefix, element, attributes, extra, attrs),
        None => Ok(()),
    }
}

fn check_document(attrs: &mut Map<String, Value>) -> Result<(), String> {
    check_element("document", "lpdf", "document", &DOCUMENT_PARTS, attrs)?;

    if let Some(assets) = attrs.get_mut("assets").and_then(Value::as_object_mut) {
        for (list, element) in [("fonts", "font"), ("images", "image")] {
            for entry in assets.get_mut(list).and_then(Value::as_array_mut).into_iter().flatten() {
                if let Some(entry) = entry.as_object_mut() {
                    check_element(&format!("document assets {list}"), "assets", element, &[], entry)?;
                }
            }
        }
    }
    if let Some(tokens) = attrs.get_mut("tokens").and_then(Value::as_object_mut) {
        check_tokens(tokens)?;
    }
    for part in ["meta", "output"] {
        if let Some(object) = attrs.get_mut(part).and_then(Value::as_object_mut) {
            check_element(&format!("document {part}"), "document", part, &[], object)?;
        }
    }
    for (index, file) in attrs.get_mut("attachments").and_then(Value::as_array_mut).into_iter().flatten().enumerate() {
        if let Some(file) = file.as_object_mut() {
            check_element(&format!("document attachments[{index}]"), "attachments", "file", &[], file)?;
        }
    }
    Ok(())
}

/// The rows of the tokens, each an object of slots, and the attributes of each gradient and its stops. The names of
/// aliases, colors and gradients are the document's own.
fn check_tokens(tokens: &mut Map<String, Value>) -> Result<(), String> {
    for (key, value) in tokens.iter_mut() {
        if key == "aliases" {
            // The parser reads each as a slot, and says so for any other value.
            continue;
        } else if key == "colors" {
            for (name, color) in value.as_object_mut().into_iter().flatten() {
                if !color.is_string() && !color.is_null() {
                    return Err(format!("colors: {name} must be a string, not {color}"));
                }
            }
        } else if key == "gradients" {
            for gradient in value.as_object_mut().into_iter().flat_map(|g| g.values_mut()) {
                let Some(gradient) = gradient.as_object_mut() else { continue };
                check_element("gradient", "gradients", "gradient", &["stops"], gradient)?;
                for stop in gradient.get_mut("stops").and_then(Value::as_array_mut).into_iter().flatten() {
                    if let Some(stop) = stop.as_object_mut() {
                        check_element("stop", "gradient", "stop", &[], stop)?;
                    }
                }
            }
        } else if let Some(row) = value.as_object_mut() {
            check_element(key, "tokens", key, &[], row)?;
        }
    }
    Ok(())
}

fn check_node(node: &mut Value, parent: &str) -> Result<(), String> {
    let Some(object) = node.as_object_mut() else { return Ok(()) };
    let Some(element) = object.get("type").and_then(Value::as_str).map(str::to_string) else { return Ok(()) };
    if let Some(attrs) = object.get_mut("attrs").and_then(Value::as_object_mut) {
        check_element(&element, parent, &element, &[], attrs)?;
    }
    for child in object.get_mut("nodes").and_then(Value::as_array_mut).into_iter().flatten() {
        check_node(child, &element)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit_to_xml::kit_to_xml;
    use crate::parse::{parse, parse_tree};

    const SCHEMA_LOCATION: &str =
        r#"xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="lpdf.xsd""#;

    /// A document of `head` (assets and tokens) and a section of `section`.
    fn xml(head: &str, section: &str) -> String {
        format!(r#"<lpdf version="1" {SCHEMA_LOCATION}>{head}<document><section>{section}</section></document></lpdf>"#)
    }

    fn layout(body: &str) -> String {
        xml("", &format!("<layout>{body}</layout>"))
    }

    /// A kit tree whose document has `attrs` and a layout of `node`.
    fn tree(attrs: &str, node: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{attrs},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layout","attrs":{{}},"nodes":[{node}]}}]}}]}}"#
        )
    }

    fn grid(cols: &str) -> String {
        tree("{}", &format!(r#"{{"type":"grid","attrs":{{"cols":{cols}}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["a"]}}]}}"#))
    }

    #[test]
    fn an_xml_value_that_is_not_of_its_type_names_the_element_the_attribute_and_the_type() {
        for (document, element, says) in [
            (layout(r#"<grid cols="x"/>"#), "<grid>", r#"cols must be a whole number from 1 to 12, not "x""#),
            (layout(r#"<text bold="yes">a</text>"#), "<text>", r#"bold must be true or false, not "yes""#),
            (layout(r#"<text bold="1">a</text>"#), "<text>", r#"bold must be true or false, not "1""#),
            (xml("", r#"<layer opacity="x"/>"#), "<layer>", r#"opacity must be a number from 0 to 1, not "x""#),
        ] {
            let error = parse(&document).unwrap_err();
            assert!(error.starts_with(&format!("{element} at line 1, col ")) && error.ends_with(&format!(": {says}")), "{error}");
        }
        assert!(parse(&layout(r#"<grid cols="3"><text bold="true">a</text></grid>"#)).is_ok());
    }

    #[test]
    fn a_tree_value_takes_the_json_type_of_its_schema_type() {
        let error = parse_tree(&grid(r#""3""#)).unwrap_err();
        assert_eq!(error, r#"grid: cols must be a whole number from 1 to 12, not "3""#);
        let error = parse_tree(&grid("3.5")).unwrap_err();
        assert_eq!(error, "grid: cols must be a whole number from 1 to 12, not 3.5");
        let error = parse_tree(&tree("{}", r#"{"type":"text","attrs":{"color":true},"nodes":["a"]}"#)).unwrap_err();
        assert_eq!(error, "text: color must be a string, not true");
        let error = parse_tree(&tree(r#"{"debug":"true"}"#, r#""a""#)).unwrap_err();
        assert_eq!(error, r#"document: debug must be true or false, not "true""#);

        // Python's json.dumps writes a float that is a whole number as 3.0.
        let whole = format!("{:?}", parse_tree(&grid("3")).unwrap().sections);
        assert_eq!(format!("{:?}", parse_tree(&grid("3.0")).unwrap().sections), whole);
        assert!(kit_to_xml(&grid("3.0")).unwrap().contains(r#"<grid cols="3">"#));
    }

    #[test]
    fn an_unknown_attribute_is_an_error_on_both_paths_that_lists_the_element_s_attributes() {
        let gradients = r##"<tokens><gradients><gradient name="fade" type="linear" from="#f00" to="#00f" agle="90"/></gradients></tokens>"##;
        let error = parse(&xml(gradients, "<layout/>")).unwrap_err();
        assert!(error.starts_with("<gradient> at line 1, col "), "{error}");
        assert!(error.ends_with(": unknown attribute 'agle' — expected name, type, from, to, angle"), "{error}");

        let tokens = r##"{"tokens":{"gradients":{"fade":{"type":"linear","from":"#f00","to":"#00f","agle":90}}}}"##;
        for error in [parse_tree(&tree(tokens, r#""a""#)).unwrap_err(), kit_to_xml(&tree(tokens, r#""a""#)).unwrap_err()] {
            assert_eq!(error, "gradient: unknown attribute 'agle' — expected name, type, from, to, angle, stops");
        }
        let error = parse_tree(&tree("{}", r#"{"type":"text","attrs":{"colr":"red"},"nodes":["a"]}"#)).unwrap_err();
        assert!(error.starts_with("text: unknown attribute 'colr' — expected font-size, font, bold, color,"), "{error}");
        let error = parse_tree(&tree(r#"{"colr":"red"}"#, r#""a""#)).unwrap_err();
        assert!(error.ends_with("footnote-counter, assets, tokens, meta, output, attachments"), "{error}");
    }

    #[test]
    fn data_attributes_and_the_schema_location_are_left_out() {
        assert!(parse(&layout(r#"<text data-value="name" data-if="shown">a</text>"#)).is_ok());
        assert!(parse_tree(&tree("{}", r#"{"type":"text","attrs":{"data-value":"name"},"nodes":["a"]}"#)).is_ok());
    }

    #[test]
    fn kit_to_xml_writes_a_typed_value_as_its_text() {
        let xml = kit_to_xml(&tree("{}", r#"{"type":"grid","attrs":{"cols":3},"nodes":[{"type":"text","attrs":{"bold":true},"nodes":["a"]}]}"#))
            .unwrap();
        assert!(xml.contains(r#"<grid cols="3">"#) && xml.contains(r#"<text bold="true">"#), "{xml}");
    }

    #[test]
    fn kit_to_xml_refuses_an_output_value_of_the_wrong_json_type_as_the_direct_render_does() {
        let encrypted = tree(r#"{"output":{"encrypt":"aes-256","allow":["print"]}}"#, r#""a""#);
        for error in [kit_to_xml(&encrypted).unwrap_err(), parse_tree(&encrypted).unwrap_err()] {
            assert_eq!(error, r#"document output: allow must be a string, not ["print"]"#);
        }
    }

    #[test]
    fn a_decimal_is_written_as_xsd_writes_it() {
        for text in ["1", "1.5", "-2", "+0.25", ".5", "5."] {
            assert!(is_decimal(text), "{text}");
        }
        for text in ["", ".", "-", "1e3", "inf", "NaN", "1.2.3", " 1", "x"] {
            assert!(!is_decimal(text), "{text}");
        }
    }

    #[test]
    fn a_json_number_is_written_without_an_exponent() {
        let kind = Kind::Decimal { min: Bound::Open, max: Bound::Open };
        assert_eq!(json_text(kind, &serde_json::json!(1e-7)).as_deref(), Some("0.0000001"));
        assert_eq!(json_text(kind, &serde_json::json!(90)).as_deref(), Some("90"));
    }

    #[test]
    fn a_whole_number_written_with_a_point_is_a_whole_number() {
        let kind = Kind::Integer { min: Some(1), max: Some(12) };
        assert_eq!(json_text(kind, &serde_json::json!(3.0)).as_deref(), Some("3"));
        assert_eq!(json_text(kind, &serde_json::json!(3.5)), None);
        assert_eq!(json_text(kind, &serde_json::json!(13)), None);
        assert_eq!(json_text(kind, &serde_json::json!("3")), None);
    }

    #[test]
    fn every_kind_says_what_it_takes() {
        assert_eq!(Kind::Integer { min: Some(1), max: Some(12) }.describe(), "a whole number from 1 to 12");
        assert_eq!(Kind::Integer { min: Some(0), max: None }.describe(), "a whole number of 0 or more");
        assert_eq!(Kind::Decimal { min: Bound::Inclusive(0.0), max: Bound::Inclusive(1.0) }.describe(), "a number from 0 to 1");
        assert_eq!(
            Kind::NameOrDecimal { min: Bound::Exclusive(0.0), max: Bound::Open }.describe(),
            "a name or a number above 0",
        );
    }
}
