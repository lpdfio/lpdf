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

const TOKEN_SCALES: &[&str] = &["space", "grid", "border", "radius", "width", "text-size"];

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

    lines.push(format!("{pad}</tokens>"));
    lines.join("\n")
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

fn render_span(node: &Value) -> String {
    let empty = serde_json::Map::new();
    let attrs  = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);

    let content: String = node
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
               .filter_map(|v| v.as_str())
               .map(escape_text)
               .collect()
        })
        .unwrap_or_default();

    if content.is_empty() {
        format!("<span{}/>", attrs_s)
    } else {
        format!("<span{}>{}</span>", attrs_s, content)
    }
}

fn render_text_node(node: &Value, depth: usize) -> String {
    let pad    = "  ".repeat(depth);
    let empty  = serde_json::Map::new();
    let attrs  = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);

    let children = match node.get("nodes").and_then(|v| v.as_array()) {
        Some(c) if !c.is_empty() => c,
        _ => return format!("{pad}<text{}/>", attrs_s),
    };

    let inner: String = children
        .iter()
        .map(|c| {
            if let Some(s) = c.as_str() {
                escape_text(s)
            } else if c.get("type").and_then(|v| v.as_str()) == Some("span") {
                render_span(c)
            } else {
                String::new()
            }
        })
        .collect();

    format!("{pad}<text{}>{}</text>", attrs_s, inner)
}

fn render_node(node: &Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("stack");
    let pad       = "  ".repeat(depth);
    let empty     = serde_json::Map::new();
    let attrs     = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s   = attrs_str(attrs, &[]);

    match node_type {
        "text"    => return render_text_node(node, depth),
        "divider" | "img" | "barcode" => return format!("{pad}<{node_type}{}/>", attrs_s),
        _ => {}
    }

    // Container nodes (stack, flank, split, cluster, grid, frame, link,
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

fn render_canvas_primitive(node: &Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let pad       = "  ".repeat(depth);
    let empty     = serde_json::Map::new();
    let attrs     = node.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);

    match node_type {
        // Canvas text has the same content as layout text: strings, and span children for styled runs.
        "text" => render_text_node(node, depth),
        tag => format!("{pad}<{tag}{}/>", attrs_str(attrs, &[])),
    }
}

fn render_canvas_layer(layer: &Value, depth: usize) -> String {
    let pad     = "  ".repeat(depth);
    let empty   = serde_json::Map::new();
    let attrs   = layer.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);
    let nodes   = layer.get("nodes").and_then(|v| v.as_array());
    match nodes {
        Some(arr) if !arr.is_empty() => {
            let prims: String = arr.iter()
                .map(|n| render_canvas_primitive(n, depth + 1))
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
                match nodes {
                    Some(arr) if !arr.is_empty() => {
                        let content: String = arr.iter()
                            .map(|n| render_layout_node(n, depth + 2))
                            .collect::<Vec<_>>()
                            .join("\n");
                        parts.push(format!("{inner}<layout>\n{content}\n{inner}</layout>"));
                    }
                    _ => parts.push(format!("{inner}<layout/>")),
                }
            }
            "canvas" => {
                match nodes {
                    Some(arr) if !arr.is_empty() => {
                        let layers_str: String = arr.iter()
                            .map(|l| render_canvas_layer(l, depth + 2))
                            .collect::<Vec<_>>()
                            .join("\n");
                        parts.push(format!("{inner}<canvas>\n{layers_str}\n{inner}</canvas>"));
                    }
                    _ => parts.push(format!("{inner}<canvas/>")),
                }
            }
            _ => {}
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
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("stack");
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

        if has_scales || has_colors {
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
            "attrs": { "size": "a4", "margin": "28pt" },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains(r#"size="a4""#));
        assert!(xml.contains(r#"margin="28pt""#));
    }

    // ── Tokens ────────────────────────────────────────────────────────────────

    #[test]
    fn scale_tokens_emitted_in_tokens_block() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {
                "tokens": {
                    "space": { "m": "8pt", "l": "16pt" },
                    "text-size":  { "body": "12pt" }
                }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<tokens>"));
        assert!(xml.contains("<space ") && xml.contains(r#"m="8pt""#));
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
    fn stack_with_children_rendered() {
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
                        "type": "stack",
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
        assert!(xml.contains(r#"<stack gap="m">"#));
        assert!(xml.contains("</stack>"));
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
        let pages: Vec<crate::render::RenderPage> = layouts.iter().flat_map(crate::layout::layout_page).collect();
        crate::pdf::render_pdf(
            &pages, &doc.fonts,
            &crate::pdf::FontRegistry::new(), &crate::pdf::ImageRegistry::new(),
            &doc.meta, None, false,
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

    /// One canvas primitive, as JSON, in a minimal document.
    fn doc_with_canvas_child(child: &str) -> String {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"canvas","nodes":[{{"type":"layer","attrs":{{}},"nodes":[{child}]}}]}}]}}]}}"#
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
    fn canvas_primitives_are_written_under_their_schema_names() {
        let json = doc_with_canvas_child(
            r##"{"type":"rect","attrs":{"x":"50pt","y":"50pt","w":"100pt","h":"60pt","fill":"#ff0000","radius":"8pt"}}"##,
        );
        let xml = kit_to_xml(&json).unwrap();
        assert!(xml.contains("<rect "), "{xml}");
        for attr in [r#"x="50pt""#, r#"y="50pt""#, r#"w="100pt""#, r#"h="60pt""#, r##"fill="#ff0000""##, r#"radius="8pt""#] {
            assert!(xml.contains(attr), "{attr} missing from {xml}");
        }
        assert!(!xml.contains("canvas-rect"), "{xml}");
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn canvas_text_is_written_with_its_content_and_span_children() {
        let json = doc_with_canvas_child(
            r##"{"type":"text","attrs":{"x":"50pt","y":"50pt"},"nodes":["Hello",{"type":"span","attrs":{"color":"#ff0000"},"nodes":["world"]}]}"##,
        );
        let xml = kit_to_xml(&json).unwrap();
        assert!(xml.contains(r##"<span color="#ff0000">world</span>"##), "{xml}");
        let (from_tree, from_xml) = rendered_both_ways(&json);
        assert_eq!(from_tree, from_xml);
    }

    #[test]
    fn circle_and_ellipse_are_positioned_by_cx_and_cy() {
        for (shape, size) in [("circle", r#""r":"40pt""#), ("ellipse", r#""rx":"40pt","ry":"20pt""#)] {
            let at = |cx: &str| {
                doc_with_canvas_child(&format!(
                    r##"{{"type":"{shape}","attrs":{{"cx":"{cx}","cy":"200pt",{size},"fill":"#ff0000"}}}}"##
                ))
            };
            let (tree_left, xml_left) = rendered_both_ways(&at("100pt"));
            let (tree_right, xml_right) = rendered_both_ways(&at("300pt"));
            assert_eq!(tree_left, xml_left, "{shape}: the tree and the XML render differently");
            assert_eq!(tree_right, xml_right, "{shape}: the tree and the XML render differently");
            assert_ne!(tree_left, tree_right, "{shape} does not move with cx");
        }
    }

    #[test]
    fn circle_x_and_y_are_not_its_position() {
        let at = |x: &str| {
            doc_with_canvas_child(&format!(
                r##"{{"type":"circle","attrs":{{"x":"{x}","y":"200pt","r":"40pt","fill":"#ff0000"}}}}"##
            ))
        };
        let (left, _) = rendered_both_ways(&at("100pt"));
        let (right, _) = rendered_both_ways(&at("300pt"));
        assert_eq!(left, right, "x moved a circle, but a circle's position is cx and cy");
    }

    #[test]
    fn an_anchored_rect_is_placed_the_same_from_the_tree_and_from_the_xml() {
        for anchor in ["top-right", "center", "bottom-center", "bottom-left"] {
            let kit = doc_with_canvas_child(&format!(
                r##"{{"type":"rect","attrs":{{"anchor":"{anchor}","x":"-18pt","y":"18pt","w":"80pt","h":"24pt","fill":"#ff0000"}}}}"##
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
                    {{"type":"canvas","nodes":[{{"type":"layer","attrs":{{}},"nodes":[
                        {{"type":"img","attrs":{{"name":"logo","anchor":"{anchor}","x":"-18pt","y":"18pt","w":"80pt","h":"24pt"}}}}]}}]}}]}}]}}"#
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
    fn canvas_image_is_read_by_name() {
        let json = doc_with_canvas_child(
            r#"{"type":"img","attrs":{"name":"logo","x":"10pt","y":"10pt","w":"50pt","h":"50pt"}}"#,
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
                "margin": "28pt",
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
    }
}
