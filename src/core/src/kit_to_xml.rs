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

    // Strip the "canvas-" prefix to get the XML tag name
    let tag = node_type.strip_prefix("canvas-").unwrap_or(node_type);

    match tag {
        "text" => {
            let attrs_s = attrs_str(attrs, &[]);
            let content = node.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(runs) = node.get("runs").and_then(|v| v.as_array()) {
                let inner_pad = "  ".repeat(depth + 1);
                let runs_str: String = runs.iter().map(|r| {
                    let text      = r.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    let run_attrs = r.get("attrs");
                    let font      = run_attrs.and_then(|a| a.get("font"))     .and_then(|v| v.as_str());
                    let font_size = run_attrs.and_then(|a| a.get("font-size")).and_then(|v| v.as_str());
                    let color     = run_attrs.and_then(|a| a.get("color"))    .and_then(|v| v.as_str());
                    let mut span_attrs = String::new();
                    if let Some(f)  = font      { span_attrs.push_str(&format!(" font=\"{}\"", escape_attr(f))); }
                    if let Some(fs) = font_size { span_attrs.push_str(&format!(" font-size=\"{}\"", escape_attr(fs))); }
                    if let Some(c)  = color     { span_attrs.push_str(&format!(" color=\"{}\"", escape_attr(c))); }
                    format!("{inner_pad}<span{span_attrs}>{}</span>", escape_text(text))
                }).collect::<Vec<_>>().join("\n");
                format!("{pad}<text{attrs_s}>\n{runs_str}\n{pad}</text>")
            } else if content.is_empty() {
                format!("{pad}<text{attrs_s}/>")
            } else {
                format!("{pad}<text{attrs_s}>{}</text>", escape_text(content))
            }
        }
        _ => {
            let attrs_s = attrs_str(attrs, &[]);
            format!("{pad}<{tag}{attrs_s}/>")
        }
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

// Helper: render a layout node that may be a layout-region or a regular node
fn render_layout_node(node: &Value, depth: usize) -> String {
    let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("stack");
    if node_type == "layout-region" {
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
                    "text":  { "body": "12pt" }
                }
            },
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
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
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
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
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
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
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
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
            "nodes": [{"type":"section","attrs":{},"nodes":[]}]
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
                    "colors": { "primary": "#1763cf" },
                    "fonts": { "heading": { "builtin": "Helvetica-Bold" } }
                },
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
