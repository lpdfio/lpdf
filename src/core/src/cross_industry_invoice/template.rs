//! Checks the paths a template binds under the prefix of an invoice map against the map, before the data is bound:
//! `invoice:sellr.name` would otherwise print the template's own text, or nothing. A path without a prefix is the
//! caller's `data`, or an item of a list, and is not checked here.

use crate::canvas::CanvasPrimitive;
use crate::parse::{Document, LayoutChild, Node, SectionChild};

use super::map::{Entry, InvoiceMap, Kind};

/// What an attribute does with what its path finds.
#[derive(Clone, Copy)]
enum Binding {
    /// `data-value` and the attributes that read a value: a value.
    Value,
    /// `data-source`: a list.
    Source,
    /// `data-if` and `data-if-not`: anything.
    Condition,
}

/// Checks every path of `doc` that has a prefix against `map`: the prefix is the map's, each name is an entry where it
/// stands, and the entry fits the attribute. Returns the first error, which names the attribute and the path.
pub(crate) fn check(doc: &Document, map: &InvoiceMap) -> Result<(), String> {
    for section in &doc.sections {
        for child in &section.children {
            match child {
                SectionChild::Layout { children, .. } => {
                    for layout_child in children {
                        match layout_child {
                            LayoutChild::Content(node) => check_node(node, map)?,
                            LayoutChild::Region(region) => {
                                for node in &region.children {
                                    check_node(node, map)?;
                                }
                            }
                        }
                    }
                }
                SectionChild::Layer(layer) => {
                    if let Some(path) = &layer.data_if {
                        check_path("data-if", path, Binding::Condition, map)?;
                    }
                    if let Some(path) = &layer.data_if_not {
                        check_path("data-if-not", path, Binding::Condition, map)?;
                    }
                    for primitive in &layer.children {
                        if let CanvasPrimitive::Placed(placed) = primitive {
                            check_node(&placed.node, map)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_node(node: &Node, map: &InvoiceMap) -> Result<(), String> {
    if let Some(data) = &node.data_attrs {
        let conditions = [("data-if", &data.data_if), ("data-if-not", &data.data_if_not)];
        for (attribute, path) in conditions {
            if let Some(path) = path {
                check_path(attribute, path, Binding::Condition, map)?;
            }
        }
        if let Some(path) = &data.data_source {
            check_path("data-source", path, Binding::Source, map)?;
        }
        if let Some(path) = &data.data_value {
            check_path("data-value", path, Binding::Value, map)?;
        }
        for (attribute, path) in &data.bound {
            check_path(attribute.data_attribute(), path, Binding::Value, map)?;
        }
    }
    if let Some(path) = crate::markdown::data_path_of(node) {
        check_path("data-value", path, Binding::Value, map)?;
    }
    node.children.iter().try_for_each(|child| check_node(child, map))
}

/// Checks one path of the attribute `attribute`, when it has a prefix.
fn check_path(attribute: &str, path: &str, binding: Binding, map: &InvoiceMap) -> Result<(), String> {
    let Some((prefix, rest)) = crate::data::split_prefix(path) else { return Ok(()) };
    let at = |problem: String| format!("{attribute}=\"{path}\": {problem}");
    if prefix != map.prefix {
        return Err(at(format!("'{prefix}:' is not the prefix of the invoice map, which is '{}:'", map.prefix)));
    }
    let mut entries = map.entries.as_slice();
    let mut found: Option<(&Entry, bool)> = None;
    for segment in rest.split('.') {
        let (name, picked) = match segment.find('[') {
            Some(open) => {
                let index = &segment[open..];
                let is_index = index.len() > 2
                    && index.starts_with('[')
                    && index.ends_with(']')
                    && index[1..index.len() - 1].bytes().all(|b| b.is_ascii_digit());
                if !is_index {
                    return Err(at(format!("'{segment}' picks an item of a list with a number in [ ], such as lines[0]")));
                }
                (&segment[..open], true)
            }
            None => (segment, false),
        };
        if let Some((outer, outer_picked)) = found {
            match &outer.kind {
                Kind::Value(_) => {
                    return Err(at(format!("'{}' is a value, which holds no '{name}'", outer.full_name)));
                }
                Kind::List(_) if !outer_picked => {
                    return Err(at(format!(
                        "'{0}' is a list: repeat an element over it with data-source=\"{prefix}:{0}\" and bind '{name}' \
                        inside it, or pick an item, such as {prefix}:{0}[0].{name}",
                        outer.full_name
                    )));
                }
                _ => {}
            }
        }
        let entry = entries.iter().find(|entry| entry.name == name).ok_or_else(|| {
            let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
            let place = found.map_or("the invoice map".to_string(), |(outer, _)| format!("'{}'", outer.full_name));
            at(format!("'{name}' is not in the map: {place} has {}", names.join(", ")))
        })?;
        if picked && !matches!(entry.kind, Kind::List(_)) {
            return Err(at(format!("'{}' is not a list, and [ ] picks an item of a list", entry.full_name)));
        }
        entries = match &entry.kind {
            Kind::Group(inner) | Kind::List(inner) => inner,
            Kind::Value(_) => &[],
        };
        found = Some((entry, picked));
    }
    let Some((entry, picked)) = found else { return Ok(()) };
    match (binding, &entry.kind, picked) {
        (Binding::Condition, ..) | (Binding::Value, Kind::Value(_), _) | (Binding::Source, Kind::List(_), false) => Ok(()),
        (Binding::Source, ..) => Err(at(format!(
            "'{}' is {}, and data-source repeats an element over a list",
            entry.full_name,
            kind_of(entry, picked)
        ))),
        (Binding::Value, ..) => Err(at(format!(
            "'{}' is {}, and {attribute} reads a value, such as {path}.{}",
            entry.full_name,
            kind_of(entry, picked),
            entries.first().map_or("…", |inner| inner.name.as_str())
        ))),
    }
}

/// What an entry is, for an error: `a group`.
fn kind_of(entry: &Entry, picked: bool) -> &'static str {
    match (&entry.kind, picked) {
        (Kind::Value(_), _) => "a value",
        (Kind::Group(_), _) => "a group",
        (Kind::List(_), true) => "an item of a list",
        (Kind::List(_), false) => "a list",
    }
}
