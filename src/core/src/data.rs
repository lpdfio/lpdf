use serde_json::Value;

use crate::canvas::{CanvasLayer, CanvasPrimitive};
use crate::parse::{Document, Node, NodeKind, TextRun, TOC_DATA_KEY};

// ── Path resolution ───────────────────────────────────────────────────────────

/// Walk a dot-separated path through a JSON value, returning `None` if any
/// segment is absent.
///
/// Each dot-segment may carry a trailing `[n]` bracket index, e.g.
/// `sections[0].title` is parsed as key `sections`, index `0`, key `title`.
fn resolve_dotpath<'a>(path: &str, value: &'a Value) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        // Split off any trailing bracket indices: "sections[0][1]" → key "sections", indices [0, 1]
        let mut part = segment;
        // Resolve the key portion (everything before the first '[')
        if let Some(bracket_pos) = part.find('[') {
            let key = &part[..bracket_pos];
            part = &part[bracket_pos..];
            if !key.is_empty() {
                current = current.get(key)?;
            }
        } else {
            current = current.get(part)?;
            continue;
        }
        // Resolve consecutive bracket indices
        while part.starts_with('[') {
            let close = part.find(']')?;
            let idx_str = &part[1..close];
            let idx: usize = idx_str.parse().ok()?;
            current = current.get(idx)?;
            part = &part[close + 1..];
        }
    }
    Some(current)
}

/// Resolve a data-binding path against the scope stack and JSON root.
///
/// - `/…`   — absolute: always resolves from root.
/// - `../…` — each `../` prefix ascends one scope level; clamped at root.
/// - `…`    — relative: resolves from the innermost loop scope, or root when
///            the stack is empty.
fn resolve_path<'a>(path: &str, stack: &[&'a Value], root: &'a Value) -> Option<&'a Value> {
    if let Some(rest) = path.strip_prefix('/') {
        return resolve_dotpath(rest, root);
    }
    let mut remaining = path;
    let mut depth = stack.len(); // current item lives at stack[depth-1]
    while let Some(rest) = remaining.strip_prefix("../") {
        remaining = rest;
        if depth > 0 {
            depth -= 1;
        }
    }
    let scope = if depth == 0 { root } else { stack[depth - 1] };
    resolve_dotpath(remaining, scope)
}

// ── Truthiness ───────────────────────────────────────────────────────────────

fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map_or(false, |v| v != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Render a scalar JSON value to a display string.  Objects and arrays produce
/// an empty string (use `data-source` for arrays).
fn value_to_string(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => String::new(),
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Apply data binding to every page in `doc` using `json_data` as the data
/// source.
///
/// The pass is a pure tree transformation: it expands `data-source` loops,
/// filters `data-if` / `data-if-not` conditionals, and substitutes
/// `data-value` text content.  Layout and PDF emit receive a plain `Document`
/// with no `data-*` attributes remaining, except in a table of contents
/// section: it keeps the data and is bound by `apply_toc` once `$toc` exists.
///
/// Returns an error if `json_data` is not valid JSON.
pub fn apply(doc: &mut Document, json_data: &str) -> Result<(), String> {
    let root: Value = serde_json::from_str(json_data)
        .map_err(|e| format!("data JSON parse error: {e}"))?;

    for section in &mut doc.sections {
        if let Some(toc) = &mut section.options.toc {
            toc.data = Some(root.clone());
            continue;
        }
        // A layer is drawn or not by its data-if and data-if-not, and what is in it is bound like the layout.
        section.children.retain_mut(|sc| match sc {
            crate::parse::SectionChild::Layer(layer) => bind_layer(layer, &root),
            _ => true,
        });
        for sc in &mut section.children {
            if let crate::parse::SectionChild::Layout { children: layout_children, .. } = sc {
                // Expand the flat content list (data-source loops can expand nodes).
                let content_nodes = std::mem::take(layout_children);
                let mut new_children = Vec::with_capacity(content_nodes.len());
                let mut out = Vec::new();
                for lc in content_nodes {
                    match lc {
                        crate::parse::LayoutChild::Content(node) => {
                            out.clear();
                            apply_single_node(node, &[], &root, None, &mut out);
                            new_children.extend(out.drain(..).map(crate::parse::LayoutChild::Content));
                        }
                        crate::parse::LayoutChild::Region(mut region) => {
                            let children = std::mem::take(&mut region.children);
                            region.children = apply_nodes(children, &[], &root, None);
                            new_children.push(crate::parse::LayoutChild::Region(region));
                        }
                    }
                }
                *layout_children = new_children;
            }
        }
    }

    Ok(())
}

/// Bind a layer. Returns `false` when its `data-if` or `data-if-not` say it is not drawn.
///
/// A layer reads single values: an array is read by index (`signers[0].name`), and there is no `data-source`
/// on what is in it.
fn bind_layer(layer: &mut CanvasLayer, root: &Value) -> bool {
    if let Some(path) = &layer.data_if {
        if !is_truthy(resolve_path(path, &[], root)) { return false; }
    }
    if let Some(path) = &layer.data_if_not {
        if is_truthy(resolve_path(path, &[], root)) { return false; }
    }
    let mut bound = Vec::with_capacity(layer.children.len());
    for child in std::mem::take(&mut layer.children) {
        match child {
            CanvasPrimitive::Placed(mut placed) => {
                let mut out = Vec::new();
                apply_single_node(placed.node.clone(), &[], root, None, &mut out);
                // A node filtered out by its data-if is not drawn; there is no data-source to repeat it.
                if let Some(node) = out.pop() {
                    placed.node = node;
                    bound.push(CanvasPrimitive::Placed(placed));
                }
            }
            shape => bound.push(shape),
        }
    }
    layer.children = bound;
    true
}

/// Bind the content of a table of contents section. `root` is the caller's data with `$toc` added. Each item
/// that `data-source="$toc"` expands gets `(level - 1) × indent_step` more left padding, and its `href="#"`
/// links point at its entry's bookmark.
pub(crate) fn apply_toc(nodes: Vec<Node>, root: &Value, indent_step: f32) -> Vec<Node> {
    apply_nodes(nodes, &[], root, Some(indent_step))
}

// ── Tree walking ──────────────────────────────────────────────────────────────

/// `toc_indent_step` is set while binding a table of contents section; see `apply_toc`.
fn apply_nodes(nodes: Vec<Node>, stack: &[&Value], root: &Value, toc_indent_step: Option<f32>) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        apply_single_node(node, stack, root, toc_indent_step, &mut out);
    }
    out
}

/// Process one node, appending results to `out`.  May produce:
/// - 0 outputs: the node is filtered out by `data-if` / `data-if-not`
/// - 1 output:  the normal case (data resolved and children recursed)
/// - N outputs: the node has `data-source` and is expanded once per array item
fn apply_single_node(node: Node, stack: &[&Value], root: &Value, toc_indent_step: Option<f32>, out: &mut Vec<Node>) {
    let di  = node.data_attrs.as_ref().and_then(|d| d.data_if.as_deref()).map(str::to_owned);
    let din = node.data_attrs.as_ref().and_then(|d| d.data_if_not.as_deref()).map(str::to_owned);
    let ds  = node.data_attrs.as_ref().and_then(|d| d.data_source.as_deref()).map(str::to_owned);
    let dv  = node.data_attrs.as_ref().and_then(|d| d.data_value.as_deref()).map(str::to_owned);

    // 1. data-if: skip when the path evaluates to a falsy value.
    if let Some(ref path) = di {
        if !is_truthy(resolve_path(path, stack, root)) {
            return;
        }
    }

    // 2. data-if-not: skip when the path evaluates to a truthy value.
    if let Some(ref path) = din {
        if is_truthy(resolve_path(path, stack, root)) {
            return;
        }
    }

    // 3. data-source: expand into one copy per array element.
    if let Some(ref path) = ds {
        if let Some(Value::Array(items)) = resolve_path(path, stack, root) {
            let toc_entries = toc_indent_step.filter(|_| path == TOC_DATA_KEY);
            for item in items {
                let mut new_stack = stack.to_vec();
                new_stack.push(item);
                let mut template = node.clone();
                // Clear binding attrs that belong to the container level so
                // they are not re-evaluated when we recurse into the clone.
                if let Some(d) = &mut template.data_attrs {
                    d.data_source = None;
                    d.data_if     = None;
                    d.data_if_not = None;
                }
                let first = out.len();
                apply_single_node(template, &new_stack, root, toc_indent_step, out);
                if let Some(indent_step) = toc_entries {
                    for entry_node in &mut out[first..] {
                        finish_toc_entry(entry_node, item, indent_step);
                    }
                }
            }
        }
        // Non-array value or missing path → node produces no output.
        return;
    }

    // 4. data-value: substitute the text content, or the data a barcode encodes.
    let mut node = node;
    if let Some(ref path) = dv {
        let text = value_to_string(resolve_path(path, stack, root));
        if node.kind == NodeKind::Barcode {
            node.barcode_data = Some(text);
        } else {
            node.text_runs = if text.is_empty() {
                vec![]
            } else {
                vec![TextRun {
                    text,
                    leading_space: false,
                    font:      None,
                    color:     None,
                    href:      None,
                    underline: false,
                    strike:    false,
                    bold:      false,
                    footnote: None,
                }]
            };
        }
        if let Some(d) = &mut node.data_attrs {
            d.data_value = None;
        }
    }

    // 5. Recurse into children with the current scope.
    node.children = apply_nodes(std::mem::take(&mut node.children), stack, root, toc_indent_step);
    out.push(node);
}

/// Indent an item expanded from `$toc` by its entry's level, and point the item's `href="#"` links at the
/// entry's bookmark.
fn finish_toc_entry(node: &mut Node, entry: &Value, indent_step: f32) {
    let level = entry.get("level").and_then(Value::as_u64).unwrap_or(1);
    node.padding[3] += level.saturating_sub(1) as f32 * indent_step;
    if let Some(anchor) = entry.get("anchor").and_then(Value::as_str) {
        point_links_at(node, anchor);
    }
}

fn point_links_at(node: &mut Node, anchor: &str) {
    if node.url.as_deref() == Some("#") {
        node.url = Some(anchor.to_string());
    }
    for run in &mut node.text_runs {
        if run.href.as_deref() == Some("#") {
            run.href = Some(anchor.to_string());
        }
    }
    for child in &mut node.children {
        point_links_at(child, anchor);
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{
        Align, BarcodeEcLevel, DataAttrs, DEFAULT_LINE_HEIGHT, DEFAULT_ORPHANS, DEFAULT_WIDOWS, Direction, HeightMode, Justify, Meta, NodeKind,
        Paginate, RowKeep, TextAlign,
    };
    use std::collections::HashMap;

    // ── Node / Document builders ──────────────────────────────────────────────

    fn bare_node(kind: NodeKind) -> Node {
        Node {
            kind,
            gap: 0.0,
            padding: [0.0; 4],
            background: None,
            border: None,
            radius: 0.0,
            opacity: 1.0,
            shadow: None,
            height_mode: HeightMode::Auto,
            width_constraint: None,
            width_fill: false,
            page_scope: None,
            paginate: Paginate::None,
            keep: RowKeep::None,
            debug: false,
            align: Align::Stretch,
            justify: Justify::Start,
            cols: 1,
            col_width: None,
            direction: Direction::Horizontal,
            color: None,
            thickness: 1.0,
            text_runs: vec![],
            font: "Helvetica".to_owned(),
            font_size: 11.0,
            text_color: None,
            text_align: TextAlign::Left,
            widows: DEFAULT_WIDOWS,
            orphans: DEFAULT_ORPHANS,
            line_height: DEFAULT_LINE_HEIGHT,
            max_lines: None,
            ellipsis: false,
            capture: None,
            url: None,
            image_name: None,
            img_height_constraint: None,
            img_aspect: None,
            shape: None,
            barcode_type: None,
            barcode_data: None,
            barcode_ec: BarcodeEcLevel::M,
            barcode_hrt: false,
            barcode_color: None,
            barcode_bg: None,
            table_cols: String::new(),
            stripe: None,
            col_span: 1,
            field_kind: None,
            field_name: None,
            field_value: None,
            field_label: None,
            field_options: vec![],
            field_required: false,
            field_readonly: false,
            field_checked: false,
            field_max_len: None,
            field_group: None,
            field_action_url: None,
            data_attrs: None,
            bookmarks: vec![],
            children: vec![],
        }
    }

    fn text_node(content: &str) -> Node {
        let mut n = bare_node(NodeKind::Text);
        n.text_runs = vec![TextRun {
            text: content.to_owned(),
            leading_space: false,
            font: None,
            color: None,
            href: None,
            underline: false,
            strike: false,
            bold: false,
            footnote: None,
        }];
        n
    }

    fn make_doc(children: Vec<Node>) -> Document {
        use crate::parse::{Section, SectionChild, LayoutChild, SectionOptions};
        let layout_children: Vec<LayoutChild> = children.into_iter().map(LayoutChild::Content).collect();
        Document {
            meta: Meta::default(),
            fonts: HashMap::new(),
            images: HashMap::new(),
            page_width: 595.28,
            page_height: 841.89,
            background: None,
            gradients: HashMap::new(),
            debug: false,
            footnote_counter: None,
            sections: vec![Section {
                children: vec![SectionChild::Layout { margin: [0.0; 4], children: layout_children }],
                options: SectionOptions { size: Some((595.28, 841.89)), ..SectionOptions::default() },
                bookmark: None,
            }],
            font_widths: HashMap::new(),
        }
    }

    /// Extract the flat content nodes from the first section's first layout child.
    fn layout_children(doc: &Document) -> Vec<Node> {
        use crate::parse::{SectionChild, LayoutChild};
        if let SectionChild::Layout { children: ref lc_vec, .. } = doc.sections[0].children[0] {
            lc_vec.iter().filter_map(|lc| {
                if let LayoutChild::Content(n) = lc { Some(n.clone()) } else { None }
            }).collect()
        } else {
            Vec::new()
        }
    }

    // ── Tests ──────────────────────────────────────────────────────────────────

    #[test]
    fn data_value_substitutes_scalar() {
        let mut n = text_node("fallback");
        n.data_attrs = Some(Box::new(DataAttrs { data_value: Some("name".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"name":"Acme Inc"}"#).unwrap();
        assert_eq!(layout_children(&doc)[0].text_runs[0].text, "Acme Inc");
    }

    #[test]
    fn data_value_nested_dot_path() {
        let mut n = text_node("fallback");
        n.data_attrs = Some(Box::new(DataAttrs { data_value: Some("customer.name".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"customer":{"name":"Acme Inc"}}"#).unwrap();
        assert_eq!(layout_children(&doc)[0].text_runs[0].text, "Acme Inc");
    }

    #[test]
    fn data_value_missing_path_produces_empty_runs() {
        let mut n = text_node("fallback");
        n.data_attrs = Some(Box::new(DataAttrs { data_value: Some("nonexistent".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{}"#).unwrap();
        assert!(layout_children(&doc)[0].text_runs.is_empty());
    }

    #[test]
    fn data_source_expands_array() {
        let mut child = text_node("item");
        child.data_attrs = Some(Box::new(DataAttrs { data_value: Some("label".to_owned()), ..DataAttrs::default() }));
        let mut container = bare_node(NodeKind::Column);
        container.data_attrs = Some(Box::new(DataAttrs { data_source: Some("items".to_owned()), ..DataAttrs::default() }));
        container.children = vec![child];
        let mut doc = make_doc(vec![container]);
        apply(
            &mut doc,
            r#"{"items":[{"label":"A"},{"label":"B"},{"label":"C"}]}"#,
        )
        .unwrap();
        let ch = layout_children(&doc);
        assert_eq!(ch.len(), 3);
        assert_eq!(ch[0].children[0].text_runs[0].text, "A");
        assert_eq!(ch[1].children[0].text_runs[0].text, "B");
        assert_eq!(ch[2].children[0].text_runs[0].text, "C");
    }

    #[test]
    fn data_source_empty_array_removes_node() {
        let mut container = bare_node(NodeKind::Column);
        container.data_attrs = Some(Box::new(DataAttrs { data_source: Some("items".to_owned()), ..DataAttrs::default() }));
        container.children = vec![text_node("item")];
        let mut doc = make_doc(vec![container]);
        apply(&mut doc, r#"{"items":[]}"#).unwrap();
        assert_eq!(layout_children(&doc).len(), 0);
    }

    #[test]
    fn data_if_truthy_keeps_node() {
        let mut n = text_node("premium");
        n.data_attrs = Some(Box::new(DataAttrs { data_if: Some("isPremium".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"isPremium":true}"#).unwrap();
        assert_eq!(layout_children(&doc).len(), 1);
    }

    #[test]
    fn data_if_falsy_removes_node() {
        let mut n = text_node("premium");
        n.data_attrs = Some(Box::new(DataAttrs { data_if: Some("isPremium".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"isPremium":false}"#).unwrap();
        assert_eq!(layout_children(&doc).len(), 0);
    }

    #[test]
    fn data_if_not_falsy_keeps_node() {
        let mut n = text_node("unpaid");
        n.data_attrs = Some(Box::new(DataAttrs { data_if_not: Some("isPaid".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"isPaid":false}"#).unwrap();
        assert_eq!(layout_children(&doc).len(), 1);
    }

    #[test]
    fn data_if_not_truthy_removes_node() {
        let mut n = text_node("unpaid");
        n.data_attrs = Some(Box::new(DataAttrs { data_if_not: Some("isPaid".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"isPaid":true}"#).unwrap();
        assert_eq!(layout_children(&doc).len(), 0);
    }

    #[test]
    fn data_if_inside_source_per_item() {
        let mut flag = text_node("highlighted");
        flag.data_attrs = Some(Box::new(DataAttrs { data_if: Some("isHighlighted".to_owned()), ..DataAttrs::default() }));
        let mut container = bare_node(NodeKind::Column);
        container.data_attrs = Some(Box::new(DataAttrs { data_source: Some("items".to_owned()), ..DataAttrs::default() }));
        container.children = vec![flag];
        let mut doc = make_doc(vec![container]);
        apply(
            &mut doc,
            r#"{"items":[{"isHighlighted":true},{"isHighlighted":false}]}"#,
        )
        .unwrap();
        let ch = layout_children(&doc);
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].children.len(), 1); // flag shown
        assert_eq!(ch[1].children.len(), 0); // flag hidden
    }

    #[test]
    fn data_value_inside_source() {
        let mut child = text_node("desc");
        child.data_attrs = Some(Box::new(DataAttrs { data_value: Some("description".to_owned()), ..DataAttrs::default() }));
        let mut container = bare_node(NodeKind::Column);
        container.data_attrs = Some(Box::new(DataAttrs { data_source: Some("items".to_owned()), ..DataAttrs::default() }));
        container.children = vec![child];
        let mut doc = make_doc(vec![container]);
        apply(
            &mut doc,
            r#"{"items":[{"description":"Consulting"},{"description":"Design"}]}"#,
        )
        .unwrap();
        let ch = layout_children(&doc);
        assert_eq!(ch[0].children[0].text_runs[0].text, "Consulting");
        assert_eq!(ch[1].children[0].text_runs[0].text, "Design");
    }

    #[test]
    fn parent_path_from_nested_source() {
        // outer loop: items[].description  inner loop: items[].notes (unused)
        // child references ../description to read from the outer scope
        let mut inner_child = text_node("note");
        inner_child.data_attrs = Some(Box::new(DataAttrs { data_value: Some("../description".to_owned()), ..DataAttrs::default() }));
        let mut inner_template = bare_node(NodeKind::Column);
        inner_template.children = vec![inner_child];
        let mut inner_loop = bare_node(NodeKind::Column);
        inner_loop.data_attrs = Some(Box::new(DataAttrs { data_source: Some("notes".to_owned()), ..DataAttrs::default() }));
        inner_loop.children = vec![inner_template];

        let mut outer_container = bare_node(NodeKind::Column);
        outer_container.children = vec![inner_loop];
        let mut outer_loop = bare_node(NodeKind::Column);
        outer_loop.data_attrs = Some(Box::new(DataAttrs { data_source: Some("items".to_owned()), ..DataAttrs::default() }));
        outer_loop.children = vec![outer_container];

        let mut doc = make_doc(vec![outer_loop]);
        apply(
            &mut doc,
            r#"{"items":[{"description":"Consulting","notes":[{"text":"Note A"}]}]}"#,
        )
        .unwrap();
        // outer_loop → 1 item → outer_container → inner_loop → 1 note → inner_template → inner_child
        let ch = layout_children(&doc);
        let outer_item     = &ch[0]; // outer_loop template (Column)
        let outer_cont     = &outer_item.children[0];   // outer_container
        let inner_expanded = &outer_cont.children[0];   // inner_loop template clone
        let inner_templ    = &inner_expanded.children[0]; // inner_template
        let inner_child    = &inner_templ.children[0];  // inner_child (Text)
        assert_eq!(inner_child.text_runs[0].text, "Consulting");
    }

    #[test]
    fn root_path_anchor_inside_source() {
        let mut child = text_node("company");
        child.data_attrs = Some(Box::new(DataAttrs { data_value: Some("/company".to_owned()), ..DataAttrs::default() }));
        let mut container = bare_node(NodeKind::Column);
        container.data_attrs = Some(Box::new(DataAttrs { data_source: Some("items".to_owned()), ..DataAttrs::default() }));
        container.children = vec![child];
        let mut doc = make_doc(vec![container]);
        apply(
            &mut doc,
            r#"{"company":"Acme Inc","items":[{"id":1}]}"#,
        )
        .unwrap();
        assert_eq!(
            layout_children(&doc)[0].children[0].text_runs[0].text,
            "Acme Inc"
        );
    }

    #[test]
    fn parent_path_beyond_root_is_clamped() {
        // ../../.. from root level — must clamp to root, not panic
        let mut n = text_node("fallback");
        n.data_attrs = Some(Box::new(DataAttrs { data_value: Some("../../../name".to_owned()), ..DataAttrs::default() }));
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"name":"Safe"}"#).unwrap();
        assert_eq!(layout_children(&doc)[0].text_runs[0].text, "Safe");
    }

    #[test]
    fn static_node_unchanged_with_data() {
        let n = text_node("Static text");
        let mut doc = make_doc(vec![n]);
        apply(&mut doc, r#"{"name":"Acme"}"#).unwrap();
        assert_eq!(layout_children(&doc)[0].text_runs[0].text, "Static text");
    }

    #[test]
    fn invalid_json_returns_error() {
        let n = text_node("fallback");
        let mut doc = make_doc(vec![n]);
        assert!(apply(&mut doc, "not json").is_err());
    }

    // ── Layers ────────────────────────────────────────────────────────────────

    /// The layers of the first section of `xml`, with `data` bound to them.
    fn bound_layers(layers: &str, data: &str) -> Vec<CanvasLayer> {
        let xml = format!(r#"<lpdf version="1"><document><section>{layers}</section></document></lpdf>"#);
        let mut doc = crate::parse::parse(&xml).unwrap();
        apply(&mut doc, data).unwrap();
        doc.sections.remove(0).children.into_iter().filter_map(|child| match child {
            crate::parse::SectionChild::Layer(layer) => Some(layer),
            _ => None,
        }).collect()
    }

    /// What a layer's `text-at` and `barcode-at` say: their text and their data.
    fn said(layer: &CanvasLayer) -> Vec<String> {
        layer.children.iter().filter_map(|child| match child {
            CanvasPrimitive::Placed(placed) if placed.node.kind == NodeKind::Barcode => placed.node.barcode_data.clone(),
            CanvasPrimitive::Placed(placed) => Some(placed.node.text_runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ")),
            _ => None,
        }).collect()
    }

    #[test]
    fn data_value_fills_a_text_at_and_a_barcode_at_and_reads_arrays_by_index() {
        let layers = bound_layers(
            r#"<layer>
                <text-at x="0pt" y="0pt" data-value="signers[1].name">Name</text-at>
                <text-at x="0pt" y="20pt" data-value="missing">Fallback</text-at>
                <barcode-at type="qr" data="placeholder" x="0pt" y="40pt" width="40pt" data-value="id"/>
            </layer>"#,
            r#"{"signers":[{"name":"Jane Smith"},{"name":"John Roe"}],"id":"A-77"}"#,
        );
        // A path that is missing gives an empty text, as in the layout.
        assert_eq!(said(&layers[0]), vec!["John Roe", "", "A-77"]);
    }

    #[test]
    fn data_if_and_data_if_not_keep_or_drop_a_text_at() {
        let layers = bound_layers(
            r#"<layer>
                <text-at x="0pt" y="0pt" data-if="paid">PAID</text-at>
                <text-at x="0pt" y="20pt" data-if-not="paid">UNPAID</text-at>
                <text-at x="0pt" y="40pt" data-if="notes">NOTES</text-at>
            </layer>"#,
            r#"{"paid":true,"notes":""}"#,
        );
        assert_eq!(said(&layers[0]), vec!["PAID"]);
    }

    #[test]
    fn a_layer_is_drawn_only_when_its_data_if_and_data_if_not_say_so() {
        let xml = r#"<layer data-if="draft"><text-at x="0pt" y="0pt">DRAFT</text-at></layer>
            <layer data-if-not="draft"><text-at x="0pt" y="0pt">FINAL</text-at></layer>
            <layer><text-at x="0pt" y="0pt">ALWAYS</text-at></layer>"#;
        let names = |data: &str| -> Vec<String> {
            bound_layers(xml, data).iter().flat_map(said).collect()
        };
        assert_eq!(names(r#"{"draft":true}"#), vec!["DRAFT", "ALWAYS"]);
        assert_eq!(names(r#"{"draft":false}"#), vec!["FINAL", "ALWAYS"]);
        // A missing value is falsy.
        assert_eq!(names("{}"), vec!["FINAL", "ALWAYS"]);
    }

    #[test]
    fn data_value_sets_the_data_of_a_barcode_in_the_layout() {
        let xml = r#"<lpdf version="1"><document><section><layout>
            <barcode type="qr" data="placeholder" width="40pt" data-value="id"/></layout></section></document></lpdf>"#;
        let mut doc = crate::parse::parse(xml).unwrap();
        apply(&mut doc, r#"{"id":"A-77"}"#).unwrap();
        assert_eq!(layout_children(&doc)[0].barcode_data.as_deref(), Some("A-77"));
    }
}
