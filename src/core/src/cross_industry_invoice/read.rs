//! Reads the values of an invoice through its map, into the data a template binds: a value is a field, a group an
//! object, a list an array, all text. A value the invoice lacks is absent, and a list with no items is empty.

use roxmltree::Node;
use serde_json::{Map, Value};

use super::map::{Entry, Kind, ValueType};
use super::path::{Name, Operand, Path, Step, Subject};

/// The values of an invoice, and where each typed one sits in them.
#[derive(Debug)]
pub(crate) struct Extracted {
    pub data:  Value,
    pub typed: Vec<Typed>,
}

/// A typed value of the data: its place, as keys and item numbers from the root (`["lines", 0, "unit"]`), its type,
/// and the full name of its entry (`lines.unit`), which the SDK hands the `format` function of the type.
#[derive(Debug, PartialEq)]
pub(crate) struct Typed {
    pub at:   Vec<Value>,
    pub ty:   ValueType,
    pub name: String,
}

/// Reads the entries of the map from the invoice's root element.
pub(crate) fn read(root: Node, entries: &[Entry]) -> Result<Extracted, String> {
    let mut reader = Reader { typed: Vec::new(), at: Vec::new() };
    let data = reader.entries(entries, root, None)?;
    Ok(Extracted { data: Value::Object(data), typed: reader.typed })
}

struct Reader {
    typed: Vec<Typed>,
    /// The place of the entry being read.
    at:    Vec<Value>,
}

/// The item of a list being read, for errors: the list's full name and the item's number from 1.
type Item<'a> = Option<(&'a str, usize)>;

impl Reader {
    /// The object of `entries`, read from `place`, the element of the group or list item they are in.
    fn entries(&mut self, entries: &[Entry], place: Node, item: Item) -> Result<Map<String, Value>, String> {
        let mut object = Map::new();
        for entry in entries {
            // An invoice of many lines reads many entries, so the cap and the deadline are checked for each.
            crate::memory::check()?;
            crate::deadline::check();
            self.at.push(Value::String(entry.name.clone()));
            let value = self.entry(entry, place, item)?;
            self.at.pop();
            if let Some(value) = value {
                object.insert(entry.name.clone(), value);
            }
        }
        Ok(object)
    }

    fn entry(&mut self, entry: &Entry, place: Node, item: Item) -> Result<Option<Value>, String> {
        match &entry.kind {
            Kind::Value(value_type) => {
                let mut found = find_values(place, &entry.path);
                if found.len() > 1 {
                    return Err(format!(
                        "the invoice map's entry '{}' finds {} values in the invoice{}, and a value is one: narrow its \
                        path with a test in [ ], or make the entry a <list>",
                        entry.full_name,
                        found.len(),
                        in_item(item)
                    ));
                }
                let Some((element, text)) = found.pop() else {
                    return missing(entry, item).map(|()| None);
                };
                let text = match value_type {
                    Some(ValueType::Date) => normalize_date(&text, element.attribute("format"), entry, item)?,
                    _ => text,
                };
                if let Some(ty) = value_type {
                    self.typed.push(Typed { at: self.at.clone(), ty: *ty, name: entry.full_name.clone() });
                }
                Ok(Some(Value::String(text)))
            }
            Kind::Group(entries) => {
                let found = find_elements(place, &entry.path.steps, place);
                match found.as_slice() {
                    [] => missing(entry, item).map(|()| None),
                    [element] => Ok(Some(Value::Object(self.entries(entries, *element, item)?))),
                    several => Err(format!(
                        "the invoice map's group '{}' finds {} elements in the invoice{}, and a group is one: narrow its \
                        path with a test in [ ]",
                        entry.full_name,
                        several.len(),
                        in_item(item)
                    )),
                }
            }
            Kind::List(entries) => {
                let found = find_elements(place, &entry.path.steps, place);
                if found.is_empty() && entry.required {
                    return Err(format!(
                        "the invoice map's list '{}' is required, and the invoice has none{}",
                        entry.full_name,
                        in_item(item)
                    ));
                }
                let mut items = Vec::with_capacity(found.len());
                for (index, element) in found.into_iter().enumerate() {
                    self.at.push(Value::from(index));
                    let object = self.entries(entries, element, Some((&entry.full_name, index + 1)))?;
                    self.at.pop();
                    items.push(Value::Object(object));
                }
                Ok(Some(Value::Array(items)))
            }
        }
    }
}

/// `Ok` when the entry the invoice lacks may be absent, and the error that names it when it is required.
fn missing(entry: &Entry, item: Item) -> Result<(), String> {
    if !entry.required {
        return Ok(());
    }
    Err(format!("the invoice map's entry '{}' is required, and the invoice has none{}", entry.full_name, in_item(item)))
}

/// Where in a list an error is, for its message: ` in item 2 of 'lines'`.
fn in_item(item: Item) -> String {
    item.map(|(list, number)| format!(" in item {number} of '{list}'")).unwrap_or_default()
}

/// The values `path` finds from `place`, each with the element it was read from: the text of an element, or the
/// attribute the path ends with. An empty one is left out, as the invoice lacks it.
fn find_values<'a, 'i>(place: Node<'a, 'i>, path: &Path) -> Vec<(Node<'a, 'i>, String)> {
    find_elements(place, &path.steps, place)
        .into_iter()
        .filter_map(|element| {
            let text = match &path.attribute {
                Some(attribute) => element.attribute(attribute.as_str())?.trim().to_string(),
                None => text_of(element),
            };
            (!text.is_empty()).then_some((element, text))
        })
        .collect()
}

/// The elements `steps` find from `start`, in the invoice's order, each step's test held; a value a test compares
/// with is read from `place`.
fn find_elements<'a, 'i>(start: Node<'a, 'i>, steps: &[Step], place: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    let mut found = vec![start];
    for step in steps {
        found = found
            .into_iter()
            .flat_map(|element| element.children().filter(|child| is(child, &step.name)))
            .filter(|child| step.test.as_ref().is_none_or(|test| passes(test, *child, place)))
            .collect();
    }
    found
}

/// Whether `test` holds for `element`: what it compares is there and equals the text or the value compared with.
fn passes(test: &super::path::Test, element: Node, place: Node) -> bool {
    let actual = match &test.subject {
        Subject::Attribute(attribute) => element.attribute(attribute.as_str()).map(|v| v.trim().to_string()),
        Subject::Child(name) => element.children().find(|child| is(child, name)).map(text_of),
    };
    let expected = match &test.against {
        Operand::Text(text) => Some(text.clone()),
        Operand::Value(names) => {
            let mut at = Some(place);
            for name in names {
                at = at.and_then(|element| element.children().find(|child| is(child, name)));
            }
            at.map(text_of)
        }
    };
    matches!((actual, expected), (Some(actual), Some(expected)) if actual == expected)
}

/// Whether `node` is the element `name` names, by namespace, whatever prefix the invoice gives it.
fn is(node: &Node, name: &Name) -> bool {
    node.is_element() && node.tag_name().namespace() == Some(name.namespace.uri()) && node.tag_name().name() == name.local
}

/// The text of an element, without the spaces around it.
fn text_of(element: Node) -> String {
    let text: String = element.children().filter(Node::is_text).filter_map(|n| n.text()).collect();
    text.trim().to_string()
}

/// A date in its normal form, from the digits an invoice writes in the format its `format` attribute names: `102`,
/// `20261031`, gives `2026-10-31`; `610`, a month, `202610`, gives `2026-10`; `616`, a week, `202644`, gives
/// `2026-W44`. Without a format, eight digits are read as `102`.
fn normalize_date(text: &str, format: Option<&str>, entry: &Entry, item: Item) -> Result<String, String> {
    let digits = |count: usize| text.len() == count && text.bytes().all(|b| b.is_ascii_digit());
    let number = |range: std::ops::Range<usize>| text[range].parse::<u32>().unwrap_or(0);
    let normal = match format.map(str::trim) {
        Some("102") | None if digits(8) && (1..=12).contains(&number(4..6)) && (1..=31).contains(&number(6..8)) => {
            Some(format!("{}-{}-{}", &text[..4], &text[4..6], &text[6..]))
        }
        Some("610") if digits(6) && (1..=12).contains(&number(4..6)) => Some(format!("{}-{}", &text[..4], &text[4..])),
        Some("616") if digits(6) && (1..=53).contains(&number(4..6)) => Some(format!("{}-W{}", &text[..4], &text[4..])),
        _ => None,
    };
    normal.ok_or_else(|| {
        let format = format.map_or(String::new(), |format| format!(" in format {format}"));
        format!(
            "the invoice map's entry '{}' is a date, and the invoice writes '{text}'{format}{}: a date of an invoice is \
            YYYYMMDD in format 102, YYYYMM in 610 or YYYYWW in 616",
            entry.full_name,
            in_item(item)
        )
    })
}
