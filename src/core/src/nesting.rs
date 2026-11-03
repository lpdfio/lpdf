//! How deep a document may nest.
//!
//! The engine builds its tree and lays it out one call deeper for each level, so a document nested deeply enough runs
//! out of stack, a crash no error can report. A document nests `MAX_DEPTH` levels at most: the elements that enclose
//! one another on one path from the root, `<lpdf>` included, so siblings don't add up. A document tree counts as its
//! XML does: its root node is the `<document>`, under the `<lpdf>` the tree has no node for, so a document has one
//! depth whichever way it comes. The depth is checked as the document is read, before anything walks it.

use serde_json::Value;

/// The deepest a document may nest, `<lpdf>` included. The deepest of the repo's documents nests 13 levels, and the
/// first depth at which a render ran out of stack was 185, nested columns in the Python SDK.
pub(crate) const MAX_DEPTH: usize = 100;

/// The depth of a tree's root node, the `<document>`, which its XML has under `<lpdf>`.
const TREE_ROOT_DEPTH: usize = 2;

/// The deepest the JSON of a document tree may nest its objects and arrays. Each level of the tree is an object and
/// the array of its `nodes`, and the attributes of the deepest node nest a few more, so a tree within `MAX_DEPTH`
/// stays well inside it. Deeper JSON is refused before it is parsed, since the parser goes one call deeper for each.
const MAX_JSON_NESTING: usize = 2 * MAX_DEPTH + 56;

/// The error of an element of `kind`, which is nested `depth` levels deep.
fn too_deep(kind: &str, depth: usize) -> String {
    format!(
        "{kind} is nested {depth} levels deep, past the limit of {MAX_DEPTH}: a document nests {MAX_DEPTH} levels at \
        most, <lpdf> included"
    )
}

/// The first element of `doc`, in document order, that is more than `limit` levels deep, the root element being level
/// 1. It is always at level `limit + 1`: an element is one level deeper than its parent.
pub(crate) fn first_past<'a, 'input>(
    doc: &'a roxmltree::Document<'input>,
    limit: usize,
) -> Option<roxmltree::Node<'a, 'input>> {
    // The elements open around the one being read, in document order: each one's parent is among them.
    let mut open: Vec<roxmltree::NodeId> = Vec::new();
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        let parent = node.parent_element().map(|parent| parent.id());
        while open.last().copied() != parent {
            open.pop();
        }
        open.push(node.id());
        if open.len() > limit {
            return Some(node);
        }
    }
    None
}

/// Refuse a parsed XML document whose elements nest deeper than `MAX_DEPTH`, naming the first element past it.
pub(crate) fn check_xml(doc: &roxmltree::Document) -> Result<(), String> {
    match first_past(doc, MAX_DEPTH) {
        Some(node) => {
            let element = format!("<{}>{}", node.tag_name().name(), crate::parse::node_loc(&node));
            Err(too_deep(&element, MAX_DEPTH + 1))
        }
        None => Ok(()),
    }
}

/// Parse the JSON of a document tree, refusing one whose nodes nest deeper than `MAX_DEPTH`, named by the type of the
/// first node past it, or whose JSON nests too deep to parse.
pub(crate) fn parse_tree_json(json: &str) -> Result<Value, String> {
    if json_nesting_passes(json, MAX_JSON_NESTING) {
        return Err(format!(
            "the document tree's JSON nests its objects and arrays more than {MAX_JSON_NESTING} levels deep: a \
            document nests {MAX_DEPTH} levels at most, <lpdf> included"
        ));
    }
    let mut deserializer = serde_json::Deserializer::from_str(json);
    // serde_json stops at 128 levels of its own, which a tree of 64 levels reaches: the check above bounds the depth
    // instead, which a tree of `MAX_DEPTH` levels stays inside.
    deserializer.disable_recursion_limit();
    let root: Value = serde::Deserialize::deserialize(&mut deserializer)
        .and_then(|root| deserializer.end().map(|()| root))
        .map_err(|e| format!("JSON parse error: {e}"))?;
    check_tree(&root)?;
    Ok(root)
}

/// Whether the objects and arrays of `json` nest deeper than `limit`. Brackets in strings don't count; JSON that isn't
/// valid is left for the parser to report.
fn json_nesting_passes(json: &str, limit: usize) -> bool {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in json.as_bytes() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > limit {
                    return true;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// Refuse a document tree whose nodes nest deeper than `MAX_DEPTH`, counted as its XML is.
fn check_tree(root: &Value) -> Result<(), String> {
    let mut pending = vec![(root, TREE_ROOT_DEPTH)];
    while let Some((node, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            let kind = node.get("type").and_then(Value::as_str).unwrap_or("node");
            return Err(too_deep(&format!("the node '{kind}'"), depth));
        }
        let children = node.get("nodes").and_then(Value::as_array).into_iter().flatten();
        pending.extend(children.filter(|child| child.is_object()).map(|child| (child, depth + 1)));
    }
    Ok(())
}

/// A document whose `<column>`s nest so that the deepest element is `depth` levels deep, `<lpdf>` included.
#[cfg(test)]
pub(crate) fn nested_xml(depth: usize) -> String {
    // <lpdf>, <document>, <section>, <layout> and the <text> inside the deepest column.
    let columns = depth - 5;
    format!(
        r#"<lpdf version="1"><document><section><layout>{}<text>deep</text>{}</layout></section></document></lpdf>"#,
        "<column>".repeat(columns),
        "</column>".repeat(columns),
    )
}

/// The tree of `nested_xml(depth)`.
#[cfg(test)]
pub(crate) fn nested_tree(depth: usize) -> String {
    let columns = depth - 5;
    format!(
        r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[{}{{"type":"text","attrs":{{}},"nodes":["deep"]}}{}]}}]}}]}}"#,
        r#"{"type":"column","attrs":{},"nodes":["#.repeat(columns),
        "]}".repeat(columns),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_nested_as_deep_as_the_limit_is_read() {
        let xml = nested_xml(MAX_DEPTH);
        check_xml(&roxmltree::Document::parse(&xml).unwrap()).unwrap();
        parse_tree_json(&nested_tree(MAX_DEPTH)).unwrap();
    }

    #[test]
    fn an_element_past_the_limit_is_named_with_its_line() {
        let xml = nested_xml(MAX_DEPTH + 1);
        let error = check_xml(&roxmltree::Document::parse(&xml).unwrap()).unwrap_err();
        assert!(error.starts_with("<text> at line 1, col "), "{error}");
        assert!(error.contains(" is nested 101 levels deep, past the limit of 100"), "{error}");
    }

    #[test]
    fn a_node_past_the_limit_is_named_by_its_type() {
        let error = parse_tree_json(&nested_tree(MAX_DEPTH + 1)).unwrap_err();
        assert!(error.starts_with("the node 'text' is nested 101 levels deep, past the limit of 100"), "{error}");
    }

    #[test]
    fn siblings_do_not_add_up() {
        let wide = format!(
            r#"<lpdf version="1"><document><section><layout>{}</layout></section></document></lpdf>"#,
            "<column><text>x</text></column>".repeat(500),
        );
        check_xml(&roxmltree::Document::parse(&wide).unwrap()).unwrap();
    }

    #[test]
    fn json_too_deep_to_parse_is_refused_before_it_is_parsed() {
        let json = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        let error = parse_tree_json(&json).unwrap_err();
        assert!(error.contains("more than 256 levels deep"), "{error}");
    }

    #[test]
    fn brackets_in_strings_do_not_count() {
        let text = format!(r#"\"{}"#, "[{".repeat(1000));
        let tree = format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["{text}"]}}]}}]}}]}}"#
        );
        parse_tree_json(&tree).unwrap();
    }

    #[test]
    fn json_that_is_not_valid_is_reported_by_the_parser() {
        let error = parse_tree_json("{\"version\": 1,").unwrap_err();
        assert!(error.starts_with("JSON parse error: "), "{error}");
        let error = parse_tree_json("{} trailing").unwrap_err();
        assert!(error.starts_with("JSON parse error: trailing characters"), "{error}");
    }
}
