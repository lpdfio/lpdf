/// kit_to_xml — Convert a JSON kit-tree (produced by `LpdfKit` in any adapter)
/// to a valid lpdf XML string.
///
/// This is the authoritative serialiser; all four adapters delegate to this
/// implementation so the output stays in sync with the XML schema owned by
/// `parse.rs`. A schema change only needs one Rust update.
///
/// Key differences vs the legacy TypeScript `kitToXml`:
/// - `tokens.fonts` with `builtin` → `<assets><font … core="…"/>` (flat, valid XML)
/// - `tokens.fonts` with `src`     → `<assets><font … ref="<alias>" src="…"/>` (flat, valid XML)
/// - The `<tokens>` block never contains a `<fonts>` child (which `parse.rs`
///   would reject as an unknown element).
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

const TOKEN_SCALES: &[&str] = &["space", "grid", "border", "radius", "width", "text"];

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

    // NOTE: tokens.fonts are intentionally NOT emitted inside <tokens> because
    // parse.rs rejects <fonts> as an unknown element there. They are emitted
    // as flat <font> children of <assets> instead (see render_assets below).

    lines.push(format!("{pad}</tokens>"));
    lines.join("\n")
}

// ── Assets block (fonts and images derived from tokens) ──────────────────────

fn render_assets(tokens: &serde_json::Map<String, Value>, depth: usize) -> Option<String> {
    let fonts  = tokens.get("fonts") .and_then(|v| v.as_object());
    let images = tokens.get("images").and_then(|v| v.as_object());
    if fonts.map_or(true, |f| f.is_empty()) && images.map_or(true, |i| i.is_empty()) {
        return None;
    }

    let pad      = "  ".repeat(depth);
    let item_pad = "  ".repeat(depth + 1);
    let mut lines: Vec<String> = vec![format!("{pad}<assets>")];

    if let Some(fonts) = fonts {
        for (name, def) in fonts {
            if let Some(obj) = def.as_object() {
                if let Some(builtin) = obj.get("builtin").and_then(|v| v.as_str()) {
                    // builtin PDF core font → core= attribute
                    lines.push(format!(
                        "{item_pad}<font name=\"{}\" core=\"{}\"/>",
                        escape_attr(name),
                        escape_attr(builtin)
                    ));
                } else if let Some(src) = obj.get("src").and_then(|v| v.as_str()) {
                    // custom font with src — preserve src= and emit ref= (alias == name)
                    let ref_key = obj.get("ref").and_then(|v| v.as_str()).unwrap_or(name);
                    lines.push(format!(
                        "{item_pad}<font name=\"{}\" ref=\"{}\" src=\"{}\"/>",
                        escape_attr(name),
                        escape_attr(ref_key),
                        escape_attr(src)
                    ));
                } else if let Some(ref_key) = obj.get("ref").and_then(|v| v.as_str()) {
                    lines.push(format!(
                        "{item_pad}<font name=\"{}\" ref=\"{}\"/>",
                        escape_attr(name),
                        escape_attr(ref_key)
                    ));
                }
            }
        }
    }

    if let Some(images) = images {
        for (name, def) in images {
            let ref_key = def.get("ref").and_then(|v| v.as_str()).unwrap_or(name);
            if let Some(src) = def.get("src").and_then(|v| v.as_str()) {
                if ref_key == name {
                    lines.push(format!(
                        "{item_pad}<image name=\"{}\" src=\"{}\"/>",
                        escape_attr(name),
                        escape_attr(src)
                    ));
                } else {
                    lines.push(format!(
                        "{item_pad}<image name=\"{}\" ref=\"{}\" src=\"{}\"/>",
                        escape_attr(name),
                        escape_attr(ref_key),
                        escape_attr(src)
                    ));
                }
            } else {
                lines.push(format!(
                    "{item_pad}<image name=\"{}\" ref=\"{}\"/>",
                    escape_attr(name),
                    escape_attr(ref_key)
                ));
            }
        }
    }

    lines.push(format!("{pad}</assets>"));
    Some(lines.join("\n"))
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
        .get("children")
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

    let children = match node.get("children").and_then(|v| v.as_array()) {
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
    let children = node.get("children").and_then(|v| v.as_array());
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

fn render_page(page: &Value, depth: usize) -> String {
    let pad     = "  ".repeat(depth);
    let empty   = serde_json::Map::new();
    let attrs   = page.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);
    let attrs_s = attrs_str(attrs, &[]);

    let children = page.get("children").and_then(|v| v.as_array());
    match children {
        Some(arr) if !arr.is_empty() => {
            let children_str = arr
                .iter()
                .map(|c| render_node(c, depth + 1))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{pad}<page{attrs_s}>\n{children_str}\n{pad}</page>")
        }
        _ => format!("{pad}<page{attrs_s}/>"),
    }
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

    // Document-level attrs (skip tokens, meta — handled separately below)
    let doc_attrs_s = attrs_str(attrs, &["tokens", "meta"]);

    let mut lines: Vec<String> = vec![
        r#"<?xml version="1.0" encoding="UTF-8"?>"#.into(),
        r#"<lpdf version="1">"#.into(),
    ];

    // <tokens> (scales + colors only; fonts move to <assets>)
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

        // <assets> (fonts from tokens.fonts)
        if let Some(assets_xml) = render_assets(tokens, 1) {
            lines.push(assets_xml);
        }
    }

    lines.push(format!("  <document{doc_attrs_s}>"));

    // <meta>
    if let Some(meta) = attrs.get("meta").and_then(|v| v.as_object()) {
        if !meta.is_empty() {
            lines.push(render_meta(meta, 2));
        }
    }

    // <pages>
    let page_arr = root
        .get("children")
        .and_then(|v| v.as_array())
        .ok_or("kit JSON must have a 'children' array")?;

    lines.push("    <pages>".into());
    for page in page_arr {
        lines.push(render_page(page, 3));
    }
    lines.push("    </pages>".into());

    lines.push("  </document>".into());
    lines.push("</lpdf>".into());

    Ok(lines.join("\n"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal document JSON with one empty page.
    fn minimal_doc() -> &'static str {
        r#"{"version":1,"type":"document","attrs":{},"children":[{"type":"page","attrs":{},"children":[]}]}"#
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
    fn output_contains_document_and_pages() {
        let xml = kit_to_xml(minimal_doc()).unwrap();
        assert!(xml.contains("<document>") || xml.contains("<document "));
        assert!(xml.contains("<pages>"));
        assert!(xml.contains("</pages>"));
        assert!(xml.contains("</document>"));
    }

    #[test]
    fn empty_page_rendered() {
        let xml = kit_to_xml(minimal_doc()).unwrap();
        // Empty page — either self-closing or open+close
        assert!(xml.contains("<page/>") || xml.contains("<page>") || xml.contains("<page "));
    }

    // ── Document-level attrs ──────────────────────────────────────────────────

    #[test]
    fn document_attrs_forwarded() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": { "size": "a4", "margin": "28pt" },
            "children": [{"type":"page","attrs":{},"children":[]}]
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
                    "text":  { "body": "12pt" }
                }
            },
            "children": [{"type":"page","attrs":{},"children":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<tokens>"));
        assert!(xml.contains("<space ") && xml.contains(r#"m="8pt""#));
        assert!(xml.contains("<text ") && xml.contains(r#"body="12pt""#));
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
            "children": [{"type":"page","attrs":{},"children":[]}]
        }"##;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<colors>"));
        assert!(xml.contains(r#"name="primary""#) && xml.contains(r##"value="#1763cf""##));
    }

    // ── Fonts in <assets>, NOT in <tokens> ───────────────────────────────────

    #[test]
    fn builtin_font_placed_in_assets_not_tokens() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {
                "tokens": {
                    "fonts": { "heading": { "builtin": "Helvetica-Bold" } }
                }
            },
            "children": [{"type":"page","attrs":{},"children":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();

        // Must appear in <assets><fonts>
        assert!(xml.contains("<assets>"), "missing <assets>");
        assert!(xml.contains(r#"core="Helvetica-Bold""#), "missing core= attribute");
        assert!(xml.contains(r#"name="heading""#), "missing name= attribute");

        // Must NOT appear inside <tokens>
        if let (Some(tok_start), Some(tok_end)) = (xml.find("<tokens>"), xml.find("</tokens>")) {
            let fonts_in_tokens = xml[tok_start..tok_end].contains("<fonts>");
            assert!(!fonts_in_tokens, "<fonts> must not appear inside <tokens>");
        }
    }

    #[test]
    fn custom_font_src_uses_ref_alias_not_filepath() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {
                "tokens": {
                    "fonts": { "body": { "src": "/fonts/MyFont.ttf" } }
                }
            },
            "children": [{"type":"page","attrs":{},"children":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();

        // ref= must be the alias name ("body"), not the file path
        assert!(xml.contains(r#"ref="body""#), "expected ref=\"body\"");
        // ref= must NOT be the file path
        assert!(!xml.contains(r#"ref="/fonts/MyFont.ttf""#), "ref must not be the file path");
        // src= should be preserved so adapters can auto-load the font bytes
        assert!(xml.contains("src="), "expected src= to be preserved");
    }

    #[test]
    fn both_builtin_and_src_fonts_in_assets() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {
                "tokens": {
                    "fonts": {
                        "heading": { "builtin": "Helvetica-Bold" },
                        "body":    { "src": "/fonts/Body.ttf" }
                    }
                }
            },
            "children": [{"type":"page","attrs":{},"children":[]}]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains(r#"core="Helvetica-Bold""#));
        assert!(xml.contains(r#"ref="body""#));
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
            "children": [{"type":"page","attrs":{},"children":[]}]
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
            "children": [{
                "type": "page",
                "attrs": {},
                "children": [{
                    "type": "stack",
                    "attrs": { "gap": "m" },
                    "children": [
                        { "type": "frame", "attrs": { "height": "40pt" }, "children": [] },
                        { "type": "frame", "attrs": { "height": "40pt" }, "children": [] }
                    ]
                }]
            }]
        }"#;
        let xml = kit_to_xml(json).unwrap();
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
            "children": [{
                "type": "page",
                "attrs": {},
                "children": [{
                    "type": "text",
                    "attrs": {},
                    "children": ["Hello world"]
                }]
            }]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("<text>Hello world</text>") || xml.contains("<text "));
        assert!(xml.contains("Hello world"));
    }

    #[test]
    fn text_node_with_span_child() {
        let json = r#"{
            "version": 1,
            "type": "document",
            "attrs": {},
            "children": [{
                "type": "page",
                "attrs": {},
                "children": [{
                    "type": "text",
                    "attrs": {},
                    "children": [
                        "Total: ",
                        { "type": "span", "attrs": { "bold": "true" }, "children": ["$100"] }
                    ]
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
            "children": [{
                "type": "page",
                "attrs": {},
                "children": [{ "type": "divider", "attrs": { "color": "#ccc" }, "children": [] }]
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
            "children": [{"type":"page","attrs":{},"children":[]}]
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
            "children": [{
                "type": "page",
                "attrs": {},
                "children": [{ "type": "text", "attrs": {}, "children": ["5 < 10 & 3 > 1"] }]
            }]
        }"#;
        let xml = kit_to_xml(json).unwrap();
        assert!(xml.contains("5 &lt; 10 &amp; 3 &gt; 1"));
    }

    // ── Validation errors ─────────────────────────────────────────────────────

    #[test]
    fn rejects_wrong_version() {
        let json = r#"{"version":2,"type":"document","attrs":{},"children":[]}"#;
        assert!(kit_to_xml(json).is_err());
    }

    #[test]
    fn rejects_wrong_type() {
        let json = r#"{"version":1,"type":"page","attrs":{},"children":[]}"#;
        assert!(kit_to_xml(json).is_err());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(kit_to_xml("not json at all").is_err());
    }

    #[test]
    fn rejects_missing_children() {
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
                    "colors": { "primary": "#1763cf" },
                    "fonts": { "heading": { "builtin": "Helvetica-Bold" } }
                },
                "meta": { "title": "Roundtrip Test" }
            },
            "children": [{
                "type": "page",
                "attrs": {},
                "children": [{
                    "type": "text",
                    "attrs": {},
                    "children": ["Hello roundtrip"]
                }]
            }]
        }"##;

        let xml = kit_to_xml(json).expect("kit_to_xml should succeed");
        // parse::parse is in the parent crate — use the re-exported path
        let result = crate::parse::parse(&xml);
        assert!(result.is_ok(), "XML produced by kit_to_xml failed parse: {:?}", result.err());
        let doc = result.unwrap();
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.pages[0].children.len(), 1);
    }
}
