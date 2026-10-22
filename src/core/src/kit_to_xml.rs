/// kit_to_xml — Convert a JSON kit-tree (produced by `LpdfKit` in any adapter)
/// to a valid lpdf XML string.
///
/// This is the authoritative serialiser; all four adapters delegate to this
/// implementation so the output stays in sync with the XML schema owned by
/// `parse.rs`. A schema change only needs one Rust update.
///
/// A tree writes the attributes of every element under the names the schema gives them. The `assets`
/// of the document are lists of objects that carry the attributes of the `<font>` and `<image>`
/// elements, and become the `<assets>` block before `<tokens>`, the order the schema gives them.
use serde_json::Value;

// ── XML escaping ──────────────────────────────────────────────────────────────

fn escape_attr(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('"', "&quot;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

// ── Attribute helpers ─────────────────────────────────────────────────────────

/// Serialise a JSON object's string-valued entries as XML attribute pairs,
/// skipping the keys listed in `skip`.
fn attrs_str(obj: &serde_json::Map<String, Value>, skip: &[&str]) -> String {
    let mut out = String::new();
    for (k, v) in obj {
        if skip.contains(&k.as_str()) {
            continue;
        }
        if let Some(s) = v.as_str() {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            out.push_str(&escape_attr(s));
            out.push('"');
        }
    }
    out
}

// ── Tokens block ──────────────────────────────────────────────────────────────

const TOKEN_SCALES: &[&str] = &["space", "grid", "border", "radius", "width", "height", "text-size", "line-height"];

fn render_tokens(tokens: &serde_json::Map<String, Value>, depth: usize) -> String {
    let pad   = "  ".repeat(depth);
    let inner = "  ".repeat(depth + 1);
    let mut lines: Vec<String> = vec![format!("{pad}<tokens>")];

    for &scale in TOKEN_SCALES {
        if let Some(map) = tokens.get(scale).and_then(|v| v.as_object()) {
            if !map.is_empty() {
                let mut tag = format!("{inner}<{scale}");
                for (k, v) in map {
                    if let Some(s) = v.as_str() {
                        tag.push_str(&format!(" {}=\"{}\"", k, escape_attr(s)));
                    }
                }
                tag.push_str("/>");
                lines.push(tag);
            }
        }
    }

    if let Some(aliases) = tokens.get("aliases").and_then(|v| v.as_object())
        && !aliases.is_empty()
    {
        let alias_pad = "  ".repeat(depth + 2);
        lines.push(format!("{inner}<aliases>"));
        for (name, val) in aliases {
            if let Some(v) = val.as_str() {
                lines.push(format!(
                    "{alias_pad}<alias name=\"{}\" value=\"{}\"/>",
                    escape_attr(name),
                    escape_attr(v)
                ));
            }
        }
        lines.push(format!("{inner}</aliases>"));
    }

    if let Some(colors) = tokens.get("colors").and_then(|v| v.as_object()) {
        if !colors.is_empty() {
            let color_pad = "  ".repeat(depth + 2);
            lines.push(format!("{inner}<colors>"));
            for (name, val) in colors {
                if let Some(v) = val.as_str() {
                    lines.push(format!(
                        "{color_pad}<color name=\"{}\" value=\"{}\"/>",
                        escape_attr(name),
                        escape_attr(v)
                    ));
                }
            }
            lines.push(format!("{inner}</colors>"));
        }
    }

    if let Some(gradients) = tokens.get("gradients").and_then(|v| v.as_object())
        && !gradients.is_empty()
    {
        lines.push(format!("{inner}<gradients>"));
        for (name, gradient) in gradients {
            lines.extend(render_gradient(name, gradient, depth + 2));
        }
        lines.push(format!("{inner}</gradients>"));
    }

    lines.push(format!("{pad}</tokens>"));
    lines.join("\n")
}

/// One `<gradient>` of the tokens, from its entry in the Kit tree: `{ type, from, to, angle, stops }`. A number
/// is written as it is, so the angle may be one.
fn render_gradient(name: &str, gradient: &Value, depth: usize) -> Vec<String> {
    let pad = "  ".repeat(depth);
    let Some(gradient) = gradient.as_object() else {
        return Vec::new();
    };
    let mut tag = format!("{pad}<gradient name=\"{}\"", escape_attr(name));
    for key in ["type", "from", "to", "angle"] {
        let text = match gradient.get(key) {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            _                      => None,
        };
        if let Some(text) = text {
            tag.push_str(&format!(" {key}=\"{}\"", escape_attr(&text)));
        }
    }
    let stops: Vec<&Value> = gradient.get("stops").and_then(|v| v.as_array()).into_iter().flatten().collect();
    if stops.is_empty() {
        return vec![format!("{tag}/>")];
    }
    let stop_pad = "  ".repeat(depth + 1);
    let mut lines = vec![format!("{tag}>")];
    for stop in stops {
        lines.push(format!("{stop_pad}<stop{}/>", attrs_str(stop.as_object().unwrap_or(&serde_json::Map::new()), &[])));
    }
    lines.push(format!("{pad}</gradient>"));
    lines
}

// ── Assets block ──────────────────────────────────────────────────────────────

/// The attributes of the `<font>` and `<image>` elements, in the order the schema lists them.
const FONT_ATTRS:  [&str; 4] = ["name", "core", "ref", "src"];
const IMAGE_ATTRS: [&str; 3] = ["name", "ref", "src"];

fn render_asset(tag: &str, attrs: &[&str], entry: &Value, pad: &str) -> Option<String> {
    let entry = entry.as_object()?;
    let written: String = attrs
        .iter()
        .filter_map(|key| {
            let value = entry.get(*key)?.as_str()?;
            Some(format!(" {key}=\"{}\"", escape_attr(value)))
        })
        .collect();
    Some(format!("{pad}<{tag}{written}/>"))
}

fn render_assets(assets: &serde_json::Map<String, Value>, depth: usize) -> Option<String> {
    let pad      = "  ".repeat(depth);
    let item_pad = "  ".repeat(depth + 1);
    let mut items: Vec<String> = Vec::new();

    for (kind, tag, attrs) in [("fonts", "font", &FONT_ATTRS[..]), ("images", "image", &IMAGE_ATTRS[..])] {
        let entries = assets.get(kind).and_then(|v| v.as_array()).into_iter().flatten();
        items.extend(entries.filter_map(|entry| render_asset(tag, attrs, entry, &item_pad)));
    }

    if items.is_empty() {
        return None;
    }
    Some(format!("{pad}<assets>
{}
{pad}</assets>", items.join("
")))
}

// ── Meta element ──────────────────────────────────────────────────────────────

fn render_meta(meta: &serde_json::Map<String, Value>, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    let mut tag = format!("{pad}<meta");
    for (k, v) in meta {
        if let Some(s) = v.as_str() {
            tag.push_str(&format!(" {}=\"{}\"", k, escape_attr(s)));
        }
    }
    tag.push_str("/>");
    tag
}

// ── Node rendering ────────────────────────────────────────────────────────────

/// A `span`, whose content is strings, or an `fn`, whose content is strings and spans.
fn render_inline(node: &Value) -> String {
    let empty = serde_json::Map::new();
    let tag    = node.get("type").and_then(|v| v.as_str()).unwrap_or("span");
    let attrs  = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);

    let content: String = node
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
               .map(|c| match c.as_str() {
                   Some(s) => escape_text(s),
                   None if tag == "fn" && c.get("type").and_then(|v| v.as_str()) == Some("span") => render_inline(c),
                   None => String::new(),
               })
               .collect()
        })
        .unwrap_or_default();

    if content.is_empty() {
        format!("<{tag}{}/>", attrs_s)
    } else {
        format!("<{tag}{}>{}</{tag}>", attrs_s, content)
    }
}

/// A `text`, or a layer's `text-at`, with its strings and `span` and `fn` children.
fn render_text_node(node: &Value, depth: usize) -> String {
    let tag    = node.get("type").and_then(|v| v.as_str()).unwrap_or("text");
    let pad    = "  ".repeat(depth);
    let empty  = serde_json::Map::new();
    let attrs  = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);

    let children = match node.get("nodes").and_then(|v| v.as_array()) {
        Some(c) if !c.is_empty() => c,
        _ => return format!("{pad}<{tag}{}/>", attrs_s),
    };

    let inner: String = children
        .iter()
        .map(|c| {
            if let Some(s) = c.as_str() {
                escape_text(s)
            } else if matches!(c.get("type").and_then(|v| v.as_str()), Some("span" | "fn")) {
                render_inline(c)
            } else {
                String::new()
            }
        })
        .collect();

    format!("{pad}<{tag}{}>{}</{tag}>", attrs_s, inner)
}

fn render_node(node: &Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("column");
    let pad       = "  ".repeat(depth);
    let empty     = serde_json::Map::new();
    let attrs     = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s   = attrs_str(attrs, &[]);

    match node_type {
        "text"    => return render_text_node(node, depth),
        "divider" | "img" | "shape" | "barcode" => return format!("{pad}<{node_type}{}/>", attrs_s),
        _ => {}
    }

    // Container nodes (column, row, grid, frame, link,
    // table, thead, tr, td)
    let children = node.get("nodes").and_then(|v| v.as_array());
    match children {
        Some(arr) if !arr.is_empty() => {
            let children_str = arr
                .iter()
                .map(|c| render_node(c, depth + 1))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{pad}<{node_type}{attrs_s}>\n{children_str}\n{pad}</{node_type}>")
        }
        _ => format!("{pad}<{node_type}{attrs_s}/>"),
    }
}

fn render_layer_primitive(node: &Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let pad       = "  ".repeat(depth);
    let empty     = serde_json::Map::new();
    let attrs     = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);

    match node_type {
        // A `text-at` has the same content as layout text: strings, and span children for styled runs. A `text`
        // is written as it is, so that reading the XML says it is a `text-at` in a layer.
        "text" | "text-at" => render_text_node(node, depth),
        tag => format!("{pad}<{tag}{}/>", attrs_str(attrs, &[])),
    }
}

fn render_layer(layer: &Value, depth: usize) -> String {
    let pad     = "  ".repeat(depth);
    let empty   = serde_json::Map::new();
    let attrs   = layer.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);
    let nodes   = layer.get("nodes").and_then(|v| v.as_array());
    match nodes {
        Some(arr) if !arr.is_empty() => {
            let prims: String = arr.iter()
                .map(|n| render_layer_primitive(n, depth + 1))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{pad}<layer{attrs_s}>\n{prims}\n{pad}</layer>")
        }
        _ => format!("{pad}<layer{attrs_s}/>"),
    }
}

fn render_section(section: &Value, depth: usize) -> String {
    let pad     = "  ".repeat(depth);
    let inner   = "  ".repeat(depth + 1);
    let empty   = serde_json::Map::new();
    let attrs   = section.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);

    let children = section.get("nodes").and_then(|v| v.as_array());
    let Some(children) = children else {
        return format!("{pad}<section{attrs_s}/>");
    };

    let mut parts: Vec<String> = Vec::new();
    for child in children {
        let kind = child.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let nodes = child.get("nodes").and_then(|v| v.as_array());
        match kind {
            "layout" => {
                let layout_attrs = child.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
                let layout_attrs_s = attrs_str(layout_attrs, &[]);
                match nodes {
                    Some(arr) if !arr.is_empty() => {
                        let content: String = arr.iter()
                            .map(|n| render_layout_node(n, depth + 2))
                            .collect::<Vec<_>>()
                            .join("\n");
                        parts.push(format!("{inner}<layout{layout_attrs_s}>\n{content}\n{inner}</layout>"));
                    }
                    _ => parts.push(format!("{inner}<layout{layout_attrs_s}/>")),
                }
            }
            "layer" => parts.push(render_layer(child, depth + 1)),
            "bookmark" => {
                let bookmark_attrs = child.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
                parts.push(format!("{inner}<bookmark{}/>", attrs_str(bookmark_attrs, &[])));
            }
            "" => {}
            // Any other child is written as it is, so that reading the XML reports it rather than the
            // content being lost here: a `canvas` node, which the format no longer has, becomes `<canvas>`.
            _ => parts.push(render_node(child, depth + 1)),
        }
    }

    if parts.is_empty() {
        format!("{pad}<section{attrs_s}/>")
    } else {
        format!("{pad}<section{attrs_s}>\n{}\n{pad}</section>", parts.join("\n"))
    }
}

// Helper: render a layout node that may be a region or a regular node
fn render_layout_node(node: &Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("column");
    if node_type == "region" {
        let pad     = "  ".repeat(depth);
        let _inner  = "  ".repeat(depth + 1);
        let empty   = serde_json::Map::new();
        let attrs   = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
        let attrs_s = attrs_str(attrs, &[]);
        let nodes   = node.get("nodes").and_then(|v| v.as_array());
        return match nodes {
            Some(arr) if !arr.is_empty() => {
                let content: String = arr.iter()
                    .map(|n| render_node(n, depth + 1))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("{pad}<region{attrs_s}>\n{content}\n{pad}</region>")
            }
            _ => format!("{pad}<region{attrs_s}/>"),
        };
    }
    render_node(node, depth)
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Convert a JSON kit-tree (as produced by `LpdfKit` in any adapter) to an
/// lpdf XML string. The output is well-formed and passes through `render_pdf`
/// without modification.
pub fn kit_to_xml(json: &str) -> Result<String, String> {
    let root: Value = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse error: {e}"))?;

    if root.get("version").and_then(|v| v.as_u64()) != Some(1) {
        return Err("kit JSON must have version=1".into());
    }
    if root.get("type").and_then(|v| v.as_str()) != Some("document") {
        return Err("kit JSON root type must be 'document'".into());
    }

    let empty = serde_json::Map::new();
    let attrs = root.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);

    // Document-level attrs (skip assets, tokens, meta — written as elements below)
    let doc_attrs_s = attrs_str(attrs, &["assets", "tokens", "meta"]);

    let mut lines: Vec<String> = vec![
        r#"<?xml version="1.0" encoding="UTF-8"?>"#.into(),
        r#"<lpdf version="1">"#.into(),
    ];

    // <assets>, then <tokens>, as the schema orders them
    if let Some(assets_xml) = attrs
        .get("assets")
        .and_then(|v| v.as_object())
        .and_then(|assets| render_assets(assets, 1))
    {
        lines.push(assets_xml);
    }

    if let Some(tokens) = attrs.get("tokens").and_then(|v| v.as_object()) {
        let has_scales = TOKEN_SCALES.iter().any(|s| {
            tokens.get(*s).and_then(|v| v.as_object()).map_or(false, |m| !m.is_empty())
        });
        let has_colors = tokens
            .get("colors")
            .and_then(|v| v.as_object())
            .map_or(false, |m| !m.is_empty());

        let has_gradients = tokens
            .get("gradients")
            .and_then(|v| v.as_object())
            .is_some_and(|m| !m.is_empty());

        let has_aliases = tokens
            .get("aliases")
            .and_then(|v| v.as_object())
            .is_some_and(|m| !m.is_empty());

        if has_scales || has_colors || has_gradients || has_aliases {
            lines.push(render_tokens(tokens, 1));
        }
    }

    lines.push(format!("  <document{doc_attrs_s}>"));

    // <meta>
    if let Some(meta) = attrs.get("meta").and_then(|v| v.as_object()) {
        if !meta.is_empty() {
            lines.push(render_meta(meta, 2));
        }
    }

    // sections
    let nodes_arr = root.get("nodes")
        .and_then(|v| v.as_array())
        .ok_or("kit JSON must have a 'nodes' array")?;

    for section in nodes_arr {
        lines.push(render_section(section, 2));
    }

    lines.push("  </document>".into());
    lines.push("</lpdf>".into());

    Ok(lines.join("\n"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal document JSON with one empty section.
    fn minimal_doc() -> &'static str {
        r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[]}]}"#
    }

    // ── Structural output ─────────────────────────────────────────────────────

    #[test]
    fn output_starts_with_xml_declaration() {
        let xml = kit_to_xml(minimal_doc()).unwrap();
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
    }

    #[test]
    fn output_contains_lpdf_root() {
        let xml = kit_to_xml(minimal_doc()).unwrap();
        assert!(xml.contains(r#"<lpdf version="1">"#));
        assert!(xml.contains("</lpdf>"));
    }

    #[test]
    fn output_contains_document_and_section() {
        let xml = kit_to_xml(minimal_doc()).unwrap();
        assert!(xml.contains("<document>") || xml.contains("<document "));
        assert!(xml.contains("<section") || xml.contains("<section/>"));
        assert!(xml.contains("</document>"));
    }

    #[test]
    fn empty_section_rendered() {
        let xml = kit_to_xml(minimal_doc()).unwrap();
        assert!(xml.contains("<section/>") || xml.contains("<section>") || xml.contains("<section "));
    }

    // ── Document-level attrs ──────────────────────────────────────────────────

    #[test]
    fn document_attrs_forwarded() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": { "size": "a4", "orientation": "landscape" },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains(r#"size="a4""#));
        assert!(xml.contains(r#"orientation="landscape""#));
    }

    // ── Tokens ────────────────────────────────────────────────────────────────

    #[test]
    fn gradients_emitted_in_tokens_block() {
        let json = r##"{
            "version": 1,
            "type": "document",
            "attrs": {
                "tokens": {
                    "gradients": {
                        "fade": { "type": "linear", "from": "#f00", "to": "primary", "angle": 90 },
                        "sunset": { "type": "radial", "stops": [
                            { "offset": "0%", "color": "#f00" }, { "offset": "100%", "color": "#00f" }] }
                    }
                }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"##;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<tokens>") && xml.contains("<gradients>"), "{xml}");
        let fade = xml.lines().find(|line| line.contains(r#"name="fade""#)).expect("the gradient fade");
        for attribute in [r##"type="linear""##, r##"from="#f00""##, r##"to="primary""##, r##"angle="90""##] {
            assert!(fade.contains(attribute), "{attribute}: {fade}");
        }
        assert!(fade.trim_end().ends_with("/>"), "a gradient without stops is one tag: {fade}");
        assert!(xml.contains(r#"<gradient name="sunset" type="radial">"#), "{xml}");
        let stops: Vec<&str> = xml.lines().filter(|line| line.contains("<stop ")).collect();
        assert_eq!(stops.len(), 2, "{xml}");
        assert!(stops[0].contains(r##"offset="0%""##) && stops[0].contains(r##"color="#f00""##), "{}", stops[0]);
        assert!(xml.contains("</gradient>") && xml.contains("</gradients>"), "{xml}");
    }

    #[test]
    fn aliases_emitted_in_tokens_block() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": { "tokens": { "aliases": { "caption": "xs", "body": "m" } } },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<tokens>") && xml.contains("<aliases>") && xml.contains("</aliases>"), "{xml}");
        assert!(xml.contains(r#"<alias name="caption" value="xs"/>"#), "{xml}");
        assert!(xml.contains(r#"<alias name="body" value="m"/>"#), "{xml}");
    }

    #[test]
    fn no_aliases_leaves_no_tokens_block() {
        let json = r#"{"version":1,"type":"document","attrs":{"tokens":{"aliases":{}}},"nodes":[{"type":"section","attrs":{},"nodes":[]}]}"#;
        assert!(!kit_to_xml(json).unwrap().contains("<tokens>"));
    }

    #[test]
    fn scale_tokens_emitted_in_tokens_block() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {
                "tokens": {
                    "space": { "m": "8pt", "l": "16pt" },
                    "height": { "m": "30pt" },
                    "text-size":  { "body": "12pt" }
                }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<tokens>"));
        assert!(xml.contains("<space ") && xml.contains(r#"m="8pt""#));
        assert!(xml.contains("<height ") && xml.contains(r#"m="30pt""#));
        assert!(xml.contains("<text-size ") && xml.contains(r#"body="12pt""#));
    }

    #[test]
    fn colors_emitted_in_tokens_block() {
        let json = r##"{
            "version": 1,
            "type": "document",
            "attrs": {
                "tokens": {
                    "colors": { "primary": "#1763cf", "surface": "#f5f5f5" }
                }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"##;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<colors>"));
        assert!(xml.contains(r#"name="primary""#) && xml.contains(r##"value="#1763cf""##));
    }

    // ── Assets ────────────────────────────────────────────────────────────────

    #[test]
    fn fonts_and_images_are_written_under_the_schema_attribute_names() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {
                "assets": {
                    "fonts": [
                        { "name": "heading", "core": "Helvetica-Bold" },
                        { "name": "body", "ref": "body-font", "src": "/fonts/Body.ttf" }
                    ],
                    "images": [{ "name": "logo", "src": "logo.png" }]
                }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains(r#"<font name="heading" core="Helvetica-Bold"/>"#), "{xml}");
        assert!(xml.contains(r#"<font name="body" ref="body-font" src="/fonts/Body.ttf"/>"#), "{xml}");
        assert!(xml.contains(r#"<image name="logo" src="logo.png"/>"#), "{xml}");
    }

    #[test]
    fn assets_come_before_tokens_and_are_not_document_attributes() {
        let json = r##"{
            "version": 1,
            "type": "document",
            "attrs": {
                "size": "a4",
                "assets": { "images": [{ "name": "logo" }] },
                "tokens": { "colors": { "primary": "#1763cf" } }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"##;
        let xml = kit_to_xml(json).unwrap();
        let assets = xml.find("<assets>").expect("assets");
        let tokens = xml.find("<tokens>").expect("tokens");
        let document = xml.find("<document").expect("document");
        assert!(assets < tokens && tokens < document, "{xml}");
        assert!(xml.contains(r#"<document size="a4">"#), "{xml}");
    }

    // ── Meta ──────────────────────────────────────────────────────────────────

    #[test]
    fn meta_emitted_as_self_closing_element() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {
                "meta": { "title": "My Document", "author": "Alice" }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<meta "));
        assert!(xml.contains(r#"title="My Document""#));
        assert!(xml.contains(r#"author="Alice""#));
    }

    // ── Node rendering ────────────────────────────────────────────────────────

    #[test]
    fn column_with_children_rendered() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {},
            "nodes": [{
                "type": "section",
                "attrs": {},
                "nodes": [{
                    "type": "layout",
                    "nodes": [{
                        "type": "column",
                        "attrs": { "gap": "m" },
                        "nodes": [
                            { "type": "frame", "attrs": { "height": "40pt" }, "nodes": [] },
                            { "type": "frame", "attrs": { "height": "40pt" }, "nodes": [] }
                        ]
                    }]
                }]
            }]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<layout>"));
        assert!(xml.contains("</layout>"));
        assert!(xml.contains(r#"<column gap="m">"#));
        assert!(xml.contains("</column>"));
        assert!(xml.matches("<frame").count() >= 2);
    }

    #[test]
    fn text_node_with_plain_string() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {},
            "nodes": [{
                "type": "section",
                "attrs": {},
                "nodes": [{
                    "type": "layout",
                    "nodes": [{
                        "type": "text",
                        "attrs": {},
                        "nodes": ["Hello world"]
                    }]
                }]
            }]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<text>Hello world</text>") || xml.contains("<text "));
        assert!(xml.contains("Hello world"));
    }

    // The render pipeline as the engine runs it, unlicensed. Built on the modules both crates share.
    fn render_document(mut doc: crate::parse::Document) -> Vec<u8> {
        let layouts = doc.section_layouts();
        let pages: Vec<crate::render::RenderPage> = crate::layout::layout_document(&layouts);
        crate::pdf::render_pdf(
            &pages, &doc.fonts,
            &crate::pdf::FontRegistry::new(), &crate::pdf::ImageRegistry::new(),
            &doc.meta, &doc.gradients, None, false,
        )
        .unwrap()
    }

    fn render_xml(xml: &str) -> Vec<u8> {
        render_document(crate::parse::parse(xml).unwrap())
    }

    fn render_tree(json: &str) -> Vec<u8> {
        render_document(crate::parse::parse_tree(json).unwrap())
    }

    /// One layout child, as JSON, in a minimal document.
    fn doc_with_layout_child(child: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layout","nodes":[{child}]}}]}}]}}"#
        )
    }

    /// One text node, with the given attrs, in a minimal document.
    fn doc_with_text_attrs(attrs: &str) -> String {
        doc_with_layout_child(&format!(r#"{{"type":"text","attrs":{attrs},"nodes":["Hello"]}}"#))
    }

    /// One layer primitive, as JSON, in a minimal document.
    fn doc_with_layer_child(child: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layer","attrs":{{}},"nodes":[{child}]}}]}}]}}"#
        )
    }

    /// The PDF the builder's tree renders to, and the PDF its XML renders to.
    fn rendered_both_ways(kit: &str) -> (Vec<u8>, Vec<u8>) {
        let from_tree = render_tree(kit);
        let from_xml = render_xml(&kit_to_xml(kit).unwrap());
        (from_tree, from_xml)
    }

    /// A minimal document that declares `assets` and holds one text in the font `heading`.
    fn doc_with_assets_and_text(assets: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{"assets":{assets}}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layout","nodes":[{{"type":"text","attrs":{{"font":"heading"}},"nodes":["Hello"]}}]}}]}}]}}"#
        )
    }

    #[test]
    fn a_tree_declares_fonts_and_images_like_the_assets_element() {
        let json = doc_with_assets_and_text(
            r#"{"fonts":[{"name":"heading","core":"Times-Bold"},{"name":"body","src":"/fonts/Body.ttf"}],
                "images":[{"name":"logo","ref":"company-logo"}]}"#,
        );
        let doc = crate::parse::parse_tree(&json).unwrap();
        // The document's fonts are keyed by what the font resolves to, not by the name it was given.
        assert!(matches!(doc.fonts.get("Times-Bold"), Some(crate::tokens::FontDef::Core(f)) if f == "Times-Bold"));
        assert!(matches!(doc.fonts.get("body"), Some(crate::tokens::FontDef::Ref(r)) if r == "body"));
        assert_eq!(doc.images.get("logo").map(String::as_str), Some("company-logo"));
    }

    #[test]
    fn a_font_declared_in_a_tree_is_the_font_the_text_is_set_in() {
        let kit = doc_with_assets_and_text(r#"{"fonts":[{"name":"heading","core":"Times-Bold"}]}"#);
        let (from_tree, from_xml) = rendered_both_ways(&kit);
        assert_eq!(from_tree, from_xml);
        assert!(from_tree.windows(10).any(|w| w == b"Times-Bold"), "the text is not set in Times-Bold");
    }

    #[test]
    fn a_tree_asset_without_a_name_is_rejected() {
        let json = doc_with_assets_and_text(r#"{"images":[{"src":"logo.png"}]}"#);
        let err = crate::parse::parse_tree(&json).err().expect("an asset needs a name");
        assert!(err.contains("assets.images") && err.contains("name"), "{err}");
    }

    #[test]
    fn tree_assets_that_are_not_a_list_are_rejected() {
        let json = doc_with_assets_and_text(r#"{"fonts":{"heading":{"core":"Times-Bold"}}}"#);
        let err = crate::parse::parse_tree(&json).err().expect("assets are lists");
        assert!(err.contains("assets.fonts must be a list"), "{err}");
    }

    #[test]
    fn a_tree_with_a_renamed_layout_type_names_the_new_one() {
        for (old, new) in [("stack", "<column>"), ("cluster", "<row>")] {
            let json = doc_with_layout_child(&format!(r#"{{"type":"{old}","attrs":{{}},"nodes":[]}}"#));
            let err = crate::parse::parse_tree(&json).err().expect("the old name is rejected");
            assert!(err.contains(&format!("'{old}'")) && err.contains(new), "{err}");
        }
    }

    #[test]
    fn a_tree_text_with_widows_out_of_range_is_rejected() {
        let err = crate::parse::parse_tree(&doc_with_text_attrs(r#"{"widows":"11"}"#)).err().expect("widows go from 1 to 10");
        assert!(err.contains("invalid widows value: '11'"), "{err}");
    }

    #[test]
    fn a_tree_table_row_takes_keep_with_next_and_rejects_another_value() {
        let table = |keep: &str| doc_with_layout_child(&format!(
            r#"{{"type":"table","attrs":{{"cols":"1fr"}},"nodes":[{{"type":"tr","attrs":{{"keep":"{keep}"}},"nodes":[{{"type":"td","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["A"]}}]}}]}}]}}"#
        ));
        let mut doc = crate::parse::parse_tree(&table("with-next")).expect("with-next is accepted");
        let layouts = doc.section_layouts();
        assert_eq!(layouts[0].children[0].children[0].keep, crate::parse::RowKeep::WithNext);

        let err = crate::parse::parse_tree(&table("together")).err().expect("together is not a keep value");
        assert!(err.contains("invalid keep value: 'together'"), "{err}");
    }

    #[test]
    fn a_tree_font_that_is_a_url_is_rejected() {
        let json = doc_with_assets_and_text(r#"{"fonts":[{"name":"heading","ref":"https://example.com/f.ttf"}]}"#);
        let err = crate::parse::parse_tree(&json).err().expect("a URL is not a registry key");
        assert!(err.contains("registry key"), "{err}");
    }

    #[test]
    fn attribute_names_are_written_as_they_are() {
        let xml = kit_to_xml(&doc_with_text_attrs(r#"{"align":"right","bold":"true"}"#)).unwrap();
        assert!(xml.contains(r#"<text align="right" bold="true">"#), "{xml}");
    }

    #[test]
    fn text_from_the_builder_renders_the_same_from_the_tree_and_from_the_xml() {
        let kit = doc_with_text_attrs(r#"{"align":"right","bold":"true"}"#);
        let (from_tree, from_xml) = rendered_both_ways(&kit);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn a_capped_text_from_the_builder_renders_the_same_from_the_tree_and_from_the_xml() {
        let text = |attrs: &str| {
            doc_with_layout_child(&format!(
                r#"{{"type":"text","attrs":{attrs},"nodes":["A text long enough to wrap to a few lines at this width"]}}"#
            ))
        };
        let kit = text(r#"{"width":"60pt","max-lines":"2","ellipsis":"true"}"#);
        let (from_tree, from_xml) = rendered_both_ways(&kit);
        assert_eq!(from_tree, from_xml);
        assert_ne!(from_tree, render_tree(&text(r#"{"width":"60pt"}"#)), "the tree's cap is not read");
    }

    #[test]
    fn a_box_color_and_opacity_from_the_builder_render_the_same_from_the_tree_and_from_the_xml() {
        let column = |attrs: &str| {
            doc_with_layout_child(&format!(
                r##"{{"type":"column","attrs":{attrs},"nodes":[
                    {{"type":"text","attrs":{{}},"nodes":["Inherits"]}},
                    {{"type":"row","attrs":{{"opacity":"0.25"}},"nodes":[{{"type":"text","attrs":{{"color":"#00aa00"}},"nodes":["Own"]}}]}}]}}"##
            ))
        };
        let plain = render_tree(&column(r##"{"background":"#cccccc"}"##));
        for attrs in [r##"{"background":"#cccccc","color":"danger"}"##, r##"{"background":"#cccccc","opacity":"0.5"}"##] {
            let (from_tree, from_xml) = rendered_both_ways(&column(attrs));
            assert_eq!(from_tree, from_xml, "{attrs}");
            assert_ne!(from_tree, plain, "the tree's {attrs} is not read");
        }
    }

    #[test]
    fn gradients_from_the_builder_render_the_same_from_the_tree_and_from_the_xml() {
        let document = |fill: &str| {
            format!(
                r##"{{"version":1,"type":"document","attrs":{{"tokens":{{"colors":{{"brand":"#112233"}},"gradients":{{
                    "fade":{{"type":"linear","from":"brand","to":"#ffffff","angle":90}},
                    "spot":{{"type":"radial","stops":[{{"offset":"0%","color":"#ff0000"}},{{"offset":"100%","color":"brand"}}]}}}}}}}},
                    "nodes":[{{"type":"section","attrs":{{}},"nodes":[
                        {{"type":"layout","nodes":[{{"type":"column","attrs":{{"background":"{fill}"}},"nodes":[
                            {{"type":"text","attrs":{{}},"nodes":["Filled"]}}]}}]}},
                        {{"type":"layer","attrs":{{}},"nodes":[
                            {{"type":"rect","attrs":{{"x":"40pt","y":"400pt","width":"60pt","height":"40pt","fill":"{fill}"}}}}]}}]}}]}}"##
            )
        };
        let flat = render_tree(&document("#336699"));
        for fill in ["gradient:fade", "gradient:spot"] {
            let (from_tree, from_xml) = rendered_both_ways(&document(fill));
            assert_eq!(from_tree, from_xml, "{fill}");
            assert_ne!(from_tree, flat, "the tree's {fill} is not read");
        }
    }

    #[test]
    fn aliases_from_the_builder_render_the_same_from_the_tree_and_from_the_xml() {
        let document = |size: &str| {
            format!(
                r##"{{"version":1,"type":"document","attrs":{{"tokens":{{"aliases":{{"caption":"xs","heading":"xl"}}}}}},
                    "nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","nodes":[
                        {{"type":"text","attrs":{{"font-size":"{size}"}},"nodes":["Sized"]}}]}}]}}]}}"##
            )
        };
        let literal = render_tree(&document("xs"));
        for size in ["caption", "heading"] {
            let (from_tree, from_xml) = rendered_both_ways(&document(size));
            assert_eq!(from_tree, from_xml, "{size}");
        }
        let (caption, _) = rendered_both_ways(&document("caption"));
        assert_eq!(caption, literal, "the alias caption is the slot xs");
        assert_ne!(render_tree(&document("heading")), literal, "the tree's alias heading is read");
    }

    #[test]
    fn a_box_shadow_from_the_builder_renders_the_same_from_the_tree_and_from_the_xml() {
        let column = |attrs: &str| {
            doc_with_layout_child(&format!(
                r##"{{"type":"column","attrs":{attrs},"nodes":[{{"type":"text","attrs":{{}},"nodes":["Shadowed"]}}]}}"##
            ))
        };
        let plain = render_tree(&column(r##"{"background":"#cccccc"}"##));
        for attrs in [
            r##"{"background":"#cccccc","shadow":"3pt 4pt #0000ff"}"##,
            r##"{"background":"#cccccc","shadow":"3pt 4pt #0000ff 0.4"}"##,
        ] {
            let (from_tree, from_xml) = rendered_both_ways(&column(attrs));
            assert_eq!(from_tree, from_xml, "{attrs}");
            assert_ne!(from_tree, plain, "the tree's {attrs} is not read");
        }
    }

    #[test]
    fn a_layer_shape_shadow_from_the_builder_renders_the_same_from_the_tree_and_from_the_xml() {
        let layer = |shape: &str| {
            format!(
                r##"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                    {{"type":"layer","attrs":{{}},"nodes":[{shape}]}}]}}]}}"##
            )
        };
        for (kind, attrs) in [
            ("rect", r##""x":"40pt","y":"40pt","width":"60pt","height":"40pt","fill":"#00ff00""##),
            ("circle", r##""x":"40pt","y":"40pt","r":"30pt","fill":"#00ff00""##),
            ("ellipse", r##""x":"40pt","y":"40pt","rx":"40pt","ry":"20pt","fill":"#00ff00""##),
        ] {
            let shape = |shadow: &str| format!(r#"{{"type":"{kind}","attrs":{{{attrs}{shadow}}}}}"#);
            let plain = render_tree(&layer(&shape("")));
            for shadow in [r#","shadow":"3pt 4pt #0000ff""#, r#","shadow":"3pt 4pt #0000ff 0.4""#] {
                let (from_tree, from_xml) = rendered_both_ways(&layer(&shape(shadow)));
                assert_eq!(from_tree, from_xml, "{kind}{shadow}");
                assert_ne!(from_tree, plain, "the tree's {kind} shadow {shadow} is not read");
            }
        }
    }

    #[test]
    fn a_shape_of_the_layout_from_the_builder_renders_the_same_from_the_tree_and_from_the_xml() {
        let layout = |shapes: &str| {
            format!(
                r##"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                    {{"type":"layout","attrs":{{}},"nodes":[{{"type":"row","attrs":{{"align":"center","gap":"s"}},"nodes":[{shapes}]}}]}}]}}]}}"##
            )
        };
        let none = render_tree(&layout(""));
        for shape in [
            r##"{"type":"shape","attrs":{"type":"rect","width":"60pt","height":"12pt","radius":"6pt","fill":"#00ff00","stroke":"#000000","stroke-width":"2pt"}}"##,
            r##"{"type":"shape","attrs":{"type":"circle","r":"8pt","fill":"#0000ff","opacity":"0.5"}}"##,
            r##"{"type":"shape","attrs":{"type":"ellipse","width":"40pt","height":"20pt","stroke":"#ff0000","stroke-dash":"3 2"}}"##,
        ] {
            let (from_tree, from_xml) = rendered_both_ways(&layout(shape));
            assert_eq!(from_tree, from_xml, "{shape}");
            assert_ne!(from_tree, none, "the tree's {shape} is not drawn");
        }
    }

    #[test]
    fn a_layer_font_size_and_color_from_the_builder_render_the_same_from_the_tree_and_from_the_xml() {
        let layer = |attrs: &str| {
            format!(
                r##"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                    {{"type":"layer","attrs":{attrs},"nodes":[
                        {{"type":"text-at","attrs":{{"x":"20pt","y":"20pt"}},"nodes":["Inherits"]}},
                        {{"type":"text-at","attrs":{{"x":"20pt","y":"60pt","font-size":"20pt"}},"nodes":["Own size"]}}]}}]}}]}}"##
            )
        };
        let plain = render_tree(&layer("{}"));
        for attrs in [r#"{"font":"Courier"}"#, r#"{"font-size":"xl"}"#, r#"{"color":"danger"}"#] {
            let (from_tree, from_xml) = rendered_both_ways(&layer(attrs));
            assert_eq!(from_tree, from_xml, "{attrs}");
            assert_ne!(from_tree, plain, "the tree's {attrs} is not read");
        }
    }

    /// A document whose line-height row has 3 at `xxl`, holding one text that wraps at 60pt.
    fn doc_with_line_height_row_and_text(text_attrs: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{"tokens":{{"line-height":{{"xs":"1","s":"1.1","m":"1.2","l":"1.4","xl":"1.6","xxl":"3"}}}}}},
                "nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","nodes":[
                {{"type":"text","attrs":{text_attrs},"nodes":["A text long enough to wrap to a few lines at this width"]}}]}}]}}]}}"#
        )
    }

    #[test]
    fn a_line_height_from_the_builder_renders_the_same_from_the_tree_and_from_the_xml() {
        let plain = render_tree(&doc_with_line_height_row_and_text(r#"{"width":"60pt"}"#));
        for line_height in ["2", "l", "xxl"] {
            let kit = doc_with_line_height_row_and_text(&format!(r#"{{"width":"60pt","line-height":"{line_height}"}}"#));
            let (from_tree, from_xml) = rendered_both_ways(&kit);
            assert_eq!(from_tree, from_xml, "line-height {line_height}");
            assert_ne!(from_tree, plain, "the tree's line-height {line_height} is not read");
        }
        // A token is looked up in the row the tree gives, not in the default one.
        let from_row = render_tree(&doc_with_line_height_row_and_text(r#"{"width":"60pt","line-height":"xxl"}"#));
        let factor = render_tree(&doc_with_line_height_row_and_text(r#"{"width":"60pt","line-height":"3"}"#));
        assert_eq!(from_row, factor, "xxl is 3 in this row, and 2 in the default one");
    }

    #[test]
    fn a_line_height_row_with_a_value_that_is_not_a_factor_is_rejected_in_a_tree() {
        let kit = doc_with_line_height_row_and_text(r#"{}"#).replace(r#""xxl":"3""#, r#""xxl":"0""#);
        let err = crate::parse::parse_tree(&kit).err().expect("0 is not a factor above 0");
        assert!(err.contains("invalid line-height token 'xxl': '0'"), "{err}");
    }

    #[test]
    fn a_header_that_shows_a_capture_from_the_builder_renders_the_same_from_the_tree_and_from_the_xml() {
        let doc = |source_attrs: &str| {
            doc_with_layout_child(&format!(
                r#"{{"type":"region","attrs":{{"pin":"top"}},"nodes":[{{"type":"text","attrs":{{"capture-ref":"k","capture-mode":"first","capture-fallback":"none"}},"nodes":[]}}]}},
                   {{"type":"text","attrs":{source_attrs},"nodes":["Alpha"]}}"#
            ))
        };
        let kit = doc(r#"{"capture":"k"}"#);
        let (from_tree, from_xml) = rendered_both_ways(&kit);
        assert_eq!(from_tree, from_xml);
        assert_ne!(from_tree, render_tree(&doc("{}")), "the tree's capture is not read");
    }

    #[test]
    fn right_aligned_text_differs_from_left_aligned() {
        let right = render_xml(&kit_to_xml(&doc_with_text_attrs(r#"{"align":"right"}"#)).unwrap());
        let plain = render_xml(&kit_to_xml(&doc_with_text_attrs("{}")).unwrap());
        assert_ne!(right, plain);
    }

    #[test]
    fn text_align_is_not_an_alias_for_align() {
        let aliased = render_tree(&doc_with_text_attrs(r#"{"text-align":"right"}"#));
        let plain = render_tree(&doc_with_text_attrs("{}"));
        assert_eq!(aliased, plain);
    }

    #[test]
    fn bold_text_differs_from_regular_text() {
        let bold = render_xml(&kit_to_xml(&doc_with_text_attrs(r#"{"bold":"true"}"#)).unwrap());
        let plain = render_xml(&kit_to_xml(&doc_with_text_attrs("{}")).unwrap());
        assert_ne!(bold, plain);
    }

    #[test]
    fn bold_text_is_the_same_as_naming_the_bold_font() {
        let bold = render_xml(&kit_to_xml(&doc_with_text_attrs(r#"{"bold":"true"}"#)).unwrap());
        let named = render_xml(&kit_to_xml(&doc_with_text_attrs(r#"{"font":"Helvetica-Bold"}"#)).unwrap());
        assert_eq!(bold, named);
    }

    /// A document with the given attrs and text in every place a font is inherited: a layer, a header
    /// region, the layout, and a text that names a font of its own.
    fn doc_with_text_everywhere(doc_attrs: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{{doc_attrs}}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layer","attrs":{{}},"nodes":[{{"type":"text-at","attrs":{{"x":"40pt","y":"40pt"}},"nodes":["In a layer"]}}]}},
                {{"type":"layout","nodes":[
                    {{"type":"region","attrs":{{"pin":"top"}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["In a region"]}}]}},
                    {{"type":"text","attrs":{{}},"nodes":["In the layout"]}},
                    {{"type":"text","attrs":{{"font":"Helvetica-Bold"}},"nodes":["With a font of its own"]}}]}}]}}]}}"#
        )
    }

    #[test]
    fn the_documents_font_sets_its_text_the_same_from_the_tree_and_from_the_xml() {
        let plain = render_tree(&doc_with_text_everywhere(""));
        for attrs in [
            r#""font":"Times-Roman""#,
            r#""font":"heading","assets":{"fonts":[{"name":"heading","core":"Times-Bold"}]}"#,
        ] {
            let kit = doc_with_text_everywhere(attrs);
            let (from_tree, from_xml) = rendered_both_ways(&kit);
            assert_eq!(from_tree, from_xml, "{attrs}: the tree and the XML set the text in different fonts");
            assert_ne!(from_tree, plain, "{attrs}: the document's font does not reach the text");
        }
    }

    #[test]
    fn the_documents_font_is_the_font_of_a_tree_text_in_the_layout_a_region_and_a_layer() {
        let doc = crate::parse::parse_tree(&doc_with_text_everywhere(r#""font":"Times-Roman""#)).unwrap();
        let section = &doc.sections[0];
        let mut fonts = Vec::new();
        for child in &section.children {
            match child {
                crate::parse::SectionChild::Layer(layer) => for primitive in &layer.children {
                    if let crate::canvas::CanvasPrimitive::Placed(placed) = primitive {
                        fonts.push(placed.node.font.clone());
                    }
                },
                crate::parse::SectionChild::Layout { children, .. } => for child in children {
                    match child {
                        crate::parse::LayoutChild::Region(region) => fonts.extend(region.children.iter().map(|n| n.font.clone())),
                        crate::parse::LayoutChild::Content(node) => fonts.push(node.font.clone()),
                    }
                },
            }
        }
        assert_eq!(fonts, ["Times-Roman", "Times-Roman", "Times-Roman", "Helvetica-Bold"]);
    }

    #[test]
    fn bold_span_is_written_and_renders() {
        let json = doc_with_layout_child(
            r#"{"type":"text","attrs":{},"nodes":["a ",{"type":"span","attrs":{"bold":"true"},"nodes":["b"]}]}"#,
        );
        assert!(kit_to_xml(&json).unwrap().contains(r#"<span bold="true">b</span>"#));
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn footnote_is_written_with_its_words_and_spans_and_renders() {
        let json = doc_with_layout_child(
            r#"{"type":"text","attrs":{},"nodes":["a",{"type":"fn","attrs":{},"nodes":["The ",{"type":"span","attrs":{"bold":"true"},"nodes":["note"]},"."]}," b"]}"#,
        );
        assert!(kit_to_xml(&json).unwrap().contains(r#"a<fn>The <span bold="true">note</span>.</fn> b"#), "{}", kit_to_xml(&json).unwrap());
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn link_is_written_with_href() {
        let json = doc_with_layout_child(
            r#"{"type":"link","attrs":{"href":"https://lpdf.io"},"nodes":[{"type":"text","attrs":{},"nodes":["x"]}]}"#,
        );
        assert!(kit_to_xml(&json).unwrap().contains(r#"<link href="https://lpdf.io">"#));
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn link_url_is_not_an_alias_for_href() {
        let json = doc_with_layout_child(
            r#"{"type":"link","attrs":{"url":"https://lpdf.io"},"nodes":[{"type":"text","attrs":{},"nodes":["x"]}]}"#,
        );
        let err = crate::parse::parse_tree(&json).err().expect("a link with url and no href is rejected");
        assert!(err.contains("href"), "{err}");
    }

    #[test]
    fn region_is_written_as_region() {
        let json = doc_with_layout_child(
            r#"{"type":"region","attrs":{"pin":"top"},"nodes":[{"type":"text","attrs":{},"nodes":["Header"]}]}"#,
        );
        let xml = kit_to_xml(&json).unwrap();
        assert!(xml.contains(r#"<region pin="top">"#), "{xml}");
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn layer_primitives_are_written_under_their_schema_names() {
        let json = doc_with_layer_child(
            r##"{"type":"rect","attrs":{"x":"50pt","y":"50pt","width":"100pt","height":"60pt","fill":"#ff0000","radius":"8pt"}}"##,
        );
        let xml = kit_to_xml(&json).unwrap();
        assert!(xml.contains("<rect "), "{xml}");
        for attr in [r#"x="50pt""#, r#"y="50pt""#, r#"width="100pt""#, r#"height="60pt""#, r##"fill="#ff0000""##, r#"radius="8pt""#] {
            assert!(xml.contains(attr), "{attr} missing from {xml}");
        }
        assert!(!xml.contains("canvas-rect"), "{xml}");
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn layer_text_is_written_with_its_content_and_span_children() {
        let json = doc_with_layer_child(
            r##"{"type":"text-at","attrs":{"x":"50pt","y":"50pt"},"nodes":["Hello ",{"type":"span","attrs":{"color":"#ff0000"},"nodes":["world"]}]}"##,
        );
        let xml = kit_to_xml(&json).unwrap();
        assert!(xml.contains(r##"<text-at x="50pt" y="50pt">Hello <span color="#ff0000">world</span></text-at>"##), "{xml}");
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn circle_and_ellipse_are_positioned_by_their_box() {
        for (shape, size) in [("circle", r#""r":"40pt""#), ("ellipse", r#""rx":"40pt","ry":"20pt""#)] {
            for (name, other) in [("x", "y"), ("y", "x")] {
                let at = |value: &str| {
                    doc_with_layer_child(&format!(
                        r##"{{"type":"{shape}","attrs":{{"{name}":"{value}","{other}":"200pt",{size},"fill":"#ff0000"}}}}"##
                    ))
                };
                let (tree_near, xml_near) = rendered_both_ways(&at("100pt"));
                let (tree_far, xml_far) = rendered_both_ways(&at("300pt"));
                assert_eq!(tree_near, xml_near, "{shape}: the tree and the XML render differently");
                assert_eq!(tree_far, xml_far, "{shape}: the tree and the XML render differently");
                assert_ne!(tree_near, tree_far, "{shape} does not move with {name}");
            }
        }
    }

    #[test]
    fn an_anchored_circle_and_ellipse_are_placed_the_same_from_the_tree_and_from_the_xml() {
        for (shape, size) in [("circle", r#""r":"30pt""#), ("ellipse", r#""rx":"30pt","ry":"12pt""#)] {
            for anchor in ["top-right", "center", "bottom-center", "bottom-left"] {
                let kit = doc_with_layer_child(&format!(
                    r##"{{"type":"{shape}","attrs":{{"anchor":"{anchor}","x":"-18pt","y":"18pt",{size},"fill":"#ff0000"}}}}"##
                ));
                let from_tree = crate::parse::parse_tree(&kit).unwrap();
                let from_xml = crate::parse::parse(&kit_to_xml(&kit).unwrap()).unwrap();
                assert_eq!(
                    format!("{:?}", from_tree.sections),
                    format!("{:?}", from_xml.sections),
                    "{shape} {anchor}: the tree and the XML place it differently"
                );
            }
        }
    }

    #[test]
    fn the_attributes_a_layer_element_lost_fail_in_the_tree_and_in_the_xml_naming_the_new_ones() {
        for (node, attr, replacement) in [
            (r#""type":"rect","attrs":{"w":"10pt","height":"10pt"}"#, "w", "'width'"),
            (r#""type":"rect","attrs":{"width":"10pt","h":"10pt"}"#, "h", "'height'"),
            (r#""type":"circle","attrs":{"cx":"10pt","r":"5pt"}"#, "cx", "'x' and 'y'"),
            (r#""type":"circle","attrs":{"cy":"10pt","r":"5pt"}"#, "cy", "'x' and 'y'"),
            (r#""type":"ellipse","attrs":{"cx":"10pt","rx":"5pt","ry":"3pt"}"#, "cx", "'x' and 'y'"),
            (r#""type":"text-at","attrs":{"w":"100pt"},"nodes":["Hi"]"#, "w", "'width'"),
        ] {
            let json = doc_with_layer_child(&format!("{{{node}}}"));
            let from_tree = crate::parse::parse_tree(&json).err().expect("the tree is rejected");
            let from_xml = crate::parse::parse(&kit_to_xml(&json).unwrap()).err().expect("the XML is rejected");
            for err in [from_tree, from_xml] {
                assert!(err.contains(&format!("'{attr}'")) && err.contains(replacement), "{attr}: {err}");
            }
        }
    }

    #[test]
    fn an_anchored_rect_is_placed_the_same_from_the_tree_and_from_the_xml() {
        for anchor in ["top-right", "center", "bottom-center", "bottom-left"] {
            let kit = doc_with_layer_child(&format!(
                r##"{{"type":"rect","attrs":{{"anchor":"{anchor}","x":"-18pt","y":"18pt","width":"80pt","height":"24pt","fill":"#ff0000"}}}}"##
            ));
            let (from_tree, from_xml) = rendered_both_ways(&kit);
            assert_eq!(from_tree, from_xml, "{anchor}: the tree and the XML place the rect differently");
        }
    }

    #[test]
    fn an_anchored_image_is_placed_the_same_from_the_tree_and_from_the_xml() {
        for anchor in ["top-right", "center", "bottom-center", "bottom-left"] {
            let kit = format!(
                r#"{{"version":1,"type":"document","attrs":{{"assets":{{"images":[{{"name":"logo"}}]}}}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                    {{"type":"layer","attrs":{{}},"nodes":[
                        {{"type":"img-at","attrs":{{"name":"logo","anchor":"{anchor}","x":"-18pt","y":"18pt","width":"80pt","height":"24pt"}}}}]}}]}}]}}"#
            );
            let from_tree = crate::parse::parse_tree(&kit).unwrap();
            let from_xml = crate::parse::parse(&kit_to_xml(&kit).unwrap()).unwrap();
            assert_eq!(
                format!("{:?}", from_tree.sections),
                format!("{:?}", from_xml.sections),
                "{anchor}: the tree and the XML place the image differently"
            );
        }
    }

    #[test]
    fn layers_are_written_before_and_after_the_layout_in_their_order() {
        let json = r##"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[
            {"type":"layer","attrs":{"page":"first"},"nodes":[{"type":"rect","attrs":{"width":"100pt","height":"60pt","fill":"#ff0000"}}]},
            {"type":"layout","nodes":[{"type":"text","attrs":{},"nodes":["Body"]}]},
            {"type":"layer","attrs":{"page":"last"},"nodes":[{"type":"rect","attrs":{"width":"50pt","height":"30pt","fill":"#0000ff"}}]}]}]}"##;
        let xml = kit_to_xml(json).unwrap();
        let under = xml.find(r#"<layer page="first">"#).expect("the first layer is written");
        let layout = xml.find("<layout>").expect("the layout is written");
        let over = xml.find(r#"<layer page="last">"#).expect("the last layer is written");
        assert!(under < layout && layout < over, "{xml}");
        assert!(!xml.contains("<canvas"), "{xml}");
        let (from_tree, from_xml) = rendered_both_ways(json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn a_canvas_node_is_written_out_and_reading_it_says_where_its_layers_go() {
        let json = r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[
            {"type":"canvas","nodes":[{"type":"layer","attrs":{},"nodes":[]}]}]}]}"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<canvas>"), "{xml}");
        let err = crate::parse::parse(&xml).err().expect("<canvas> is rejected");
        assert!(err.contains("<layer> elements directly in the <section>"), "{err}");
    }

    #[test]
    fn text_at_and_barcode_at_render_the_same_from_the_tree_and_from_the_xml() {
        for anchor in ["top-left", "center", "bottom-right"] {
            let kit = format!(
                r##"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                    {{"type":"layer","attrs":{{}},"nodes":[
                        {{"type":"text-at","attrs":{{"anchor":"{anchor}","x":"-12pt","y":"12pt","width":"90pt","max-lines":"2","ellipsis":"true","align":"center"}},
                          "nodes":["A text that does not fit on one line ",{{"type":"span","attrs":{{"font":"Helvetica-Bold"}},"nodes":["in its box"]}}]}},
                        {{"type":"barcode-at","attrs":{{"type":"qr","data":"x","anchor":"{anchor}","x":"-12pt","y":"12pt","width":"50pt"}}}}]}}]}}]}}"##
            );
            let (from_tree, from_xml) = rendered_both_ways(&kit);
            assert_eq!(from_tree, from_xml, "{anchor}: the tree and the XML render differently");
        }
    }

    #[test]
    fn text_at_img_at_and_barcode_at_are_written_under_their_element_names() {
        let json = doc_with_layer_child(
            r##"{"type":"text-at","attrs":{"x":"1pt","y":"2pt"},"nodes":["Hi"]}"##,
        );
        let xml = kit_to_xml(&json).unwrap();
        assert!(xml.contains("<text-at x=\"1pt\" y=\"2pt\">Hi</text-at>") || xml.contains("<text-at y=\"2pt\" x=\"1pt\">Hi</text-at>"), "{xml}");
        let json = doc_with_layer_child(r#"{"type":"barcode-at","attrs":{"type":"qr","data":"x","x":"1pt","y":"2pt","width":"9pt"}}"#);
        assert!(kit_to_xml(&json).unwrap().contains("<barcode-at "));
        let json = doc_with_layer_child(r#"{"type":"img-at","attrs":{"name":"logo","x":"1pt","y":"2pt","width":"9pt"}}"#);
        assert!(kit_to_xml(&json).unwrap().contains("<img-at "));
    }

    #[test]
    fn layer_image_is_read_by_name() {
        let json = doc_with_layer_child(
            r#"{"type":"img-at","attrs":{"name":"logo","x":"10pt","y":"10pt","width":"50pt","height":"50pt"}}"#,
        );
        let err = crate::parse::parse_tree(&json).err().expect("an undeclared asset is rejected");
        assert!(err.contains("logo"), "{err}");
    }

    #[test]
    fn text_node_with_span_child() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {},
            "nodes": [{
                "type": "section",
                "attrs": {},
                "nodes": [{
                    "type": "layout",
                    "nodes": [{
                        "type": "text",
                        "attrs": {},
                        "nodes": [
                            "Total: ",
                            { "type": "span", "attrs": { "bold": "true" }, "nodes": ["$100"] }
                        ]
                    }]
                }]
            }]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("Total: "));
        assert!(xml.contains(r#"<span bold="true">$100</span>"#));
    }

    #[test]
    fn divider_is_self_closing() {
        let json = r##"{
            "version": 1,
            "type": "document",
            "attrs": {},
            "nodes": [{
                "type": "section",
                "attrs": {},
                "nodes": [{
                    "type": "layout",
                    "nodes": [{ "type": "divider", "attrs": { "color": "#ccc" }, "nodes": [] }]
                }]
            }]
        }"##;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains(r##"<divider color="#ccc"/>"##));
        assert!(!xml.contains("</divider>"));
    }

    // ── XML escaping ──────────────────────────────────────────────────────────

    #[test]
    fn attr_special_chars_escaped() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": { "meta": { "title": "A & B <test>" } },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("A &amp; B &lt;test&gt;"));
    }

    #[test]
    fn text_content_special_chars_escaped() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {},
            "nodes": [{
                "type": "section",
                "attrs": {},
                "nodes": [{
                    "type": "layout",
                    "nodes": [{ "type": "text", "attrs": {}, "nodes": ["5 < 10 & 3 > 1"] }]
                }]
            }]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("5 &lt; 10 &amp; 3 &gt; 1"));
    }

    // ── Validation errors ─────────────────────────────────────────────────────

    #[test]
    fn rejects_wrong_version() {
        let json = r#"{"version":2,"type":"document","attrs":{},"nodes":[]}"#;
        assert!(kit_to_xml(json).is_err());
    }

    #[test]
    fn rejects_wrong_type() {
        let json = r#"{"version":1,"type":"page","attrs":{},"nodes":[]}"#;
        assert!(kit_to_xml(json).is_err());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(kit_to_xml("not json at all").is_err());
    }

    #[test]
    fn rejects_missing_nodes() {
        let json = r#"{"version":1,"type":"document","attrs":{}}"#;
        assert!(kit_to_xml(json).is_err());
    }

    // ── Round-trip: XML output is parseable by parse.rs ──────────────────────
    // This ensures kit_to_xml produces XML that the engine actually accepts.

    #[test]
    fn roundtrip_through_parse() {
        let json = r##"{
            "version": 1,
            "type": "document",
            "attrs": {
                "size": "a4",
                "tokens": {
                    "space": { "xs": "2pt", "s": "4pt", "m": "8pt", "l": "16pt", "xl": "24pt", "xxl": "40pt" },
                    "colors": { "primary": "#1763cf" }
                },
                "assets": { "fonts": [{ "name": "heading", "core": "Helvetica-Bold" }] },
                "meta": { "title": "Roundtrip Test" }
            },
            "nodes": [{
                "type": "section",
                "attrs": {},
                "nodes": [{
                    "type": "layout",
                    "attrs": { "margin": "28pt" },
                    "nodes": [{
                        "type": "text",
                        "attrs": {},
                        "nodes": ["Hello roundtrip"]
                    }]
                }]
            }]
        }"##;

        let xml = kit_to_xml(json).expect("kit_to_xml should succeed");
        // parse::parse is in the parent crate — use the re-exported path
        let result = crate::parse::parse(&xml);
        assert!(result.is_ok(), "XML produced by kit_to_xml failed parse: {:?}", result.err());
        let mut doc = result.unwrap();
        let pages = doc.section_layouts();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].children.len(), 1);
        assert_eq!(pages[0].margin, [28.0; 4]);
    }

    #[test]
    fn bookmarks_and_a_toc_section_render_the_same_from_a_tree_and_from_its_xml() {
        let kit = r##"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{"type":"toc","toc-indent-step":"20pt"},"nodes":[
                {"type":"bookmark","attrs":{"label":"Contents"}},
                {"type":"layout","nodes":[
                    {"type":"link","attrs":{"href":"#methods"},"nodes":[{"type":"text","attrs":{},"nodes":["Methods"]}]}]}]},
            {"type":"section","attrs":{},"nodes":[
                {"type":"layout","nodes":[
                    {"type":"bookmark","attrs":{"label":"Methods"}},
                    {"type":"text","attrs":{},"nodes":["Methods"]},
                    {"type":"column","attrs":{},"nodes":[
                        {"type":"bookmark","attrs":{"label":"Data","level":"2","id":"data"}},
                        {"type":"text","attrs":{},"nodes":["Data"]}]}]}]}]}"##;
        let xml = kit_to_xml(kit).unwrap();
        assert!(xml.contains(r#"<bookmark label="Contents"/>"#), "{xml}");
        let (from_tree, from_xml) = rendered_both_ways(kit);
        assert!(String::from_utf8_lossy(&from_tree).contains("/Outlines"));
        assert_eq!(from_tree, from_xml);
    }

    /// A document with the same strings and spans as the text of the layout and as a layer's `text-at`.
    fn doc_with_runs(runs: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layer","attrs":{{}},"nodes":[{{"type":"text-at","attrs":{{"x":"40pt","y":"40pt"}},"nodes":{runs}}}]}},
                {{"type":"layout","nodes":[{{"type":"text","attrs":{{}},"nodes":{runs}}}]}}]}}]}}"#
        )
    }

    const BOLD_WORLD: &str = r#"{"type":"span","attrs":{"font":"Helvetica-Bold"},"nodes":["world"]}"#;

    #[test]
    fn strings_and_spans_are_read_the_same_from_the_tree_and_from_the_xml() {
        let split_span = r#"{"type":"span","attrs":{"font":"Helvetica-Bold"},"nodes":["wor","ld"]}"#;
        let price = r#"{"type":"span","attrs":{"font":"Helvetica-Bold"},"nodes":["468.88"]}"#;
        for runs in [
            format!(r#"["Hello",{BOLD_WORLD}]"#),
            format!(r#"["Hello ",{BOLD_WORLD}]"#),
            format!(r#"["Hello"," ",{BOLD_WORLD}]"#),
            format!(r#"[" Hello",{BOLD_WORLD}]"#),
            format!(r#"[{BOLD_WORLD},"again"]"#),
            format!(r#"[{BOLD_WORLD}," again"]"#),
            format!(r#"["Hello ",{BOLD_WORLD},"!"]"#),
            format!(r#"["Total: $",{price}," due"]"#),
            r#"["Hel","lo"]"#.to_string(),
            format!(r#"["Hello ",{split_span}]"#),
        ] {
            let (from_tree, from_xml) = rendered_both_ways(&doc_with_runs(&runs));
            assert_eq!(from_tree, from_xml, "{runs}: the tree and its XML space the text differently");
        }
    }

    #[test]
    fn a_span_follows_a_string_with_a_space_only_where_the_string_ends_in_one() {
        let (without_space, _) = rendered_both_ways(&doc_with_runs(&format!(r#"["Hello",{BOLD_WORLD}]"#)));
        let (with_space, _) = rendered_both_ways(&doc_with_runs(&format!(r#"["Hello ",{BOLD_WORLD}]"#)));
        assert_ne!(without_space, with_space);
    }

    #[test]
    fn a_layer_at_opacity_zero_draws_nothing_from_the_tree_and_from_the_xml() {
        let document = |layers: &str| format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{layers}]}}]}}"#
        );
        let shown = r##"{"type":"layer","attrs":{},"nodes":[{"type":"rect","attrs":{"x":"10pt","y":"10pt","width":"50pt","height":"50pt","fill":"#00ff00"}}]}"##;
        let hidden = r##"{"type":"layer","attrs":{"opacity":"0"},"nodes":[{"type":"rect","attrs":{"x":"40pt","y":"40pt","width":"50pt","height":"50pt","fill":"#ff0000"}}]}"##;
        let (from_tree, from_xml) = rendered_both_ways(&document(&format!("{shown},{hidden}")));
        let without = render_tree(&document(shown));
        assert!(from_tree == without, "the tree shows the hidden layer");
        assert!(from_xml == without, "the XML shows the hidden layer");
    }
}
