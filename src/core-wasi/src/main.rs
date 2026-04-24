/// WASI binary entrypoint for lpdf.
///
/// Protocol (stdin → stdout):
/// ```json
/// { "method": "render" | "render_tree" | "render_pdf" | "render_tree_pdf", "key": "…", "input": "…xml or json…" }
/// ```
/// - `render` / `render_tree` return the RenderTree JSON string directly.
/// - `render_pdf` / `render_tree_pdf` return `{ "pdf": "<base64>" }` on success
///   or `{ "error": "…" }` on failure.
///
/// For `render_pdf` / `render_tree_pdf`, custom font bytes can be supplied:
/// ```json
/// { "fonts": { "Inter": "<base64 TTF bytes>", … } }
/// ```

// Include the pure-Rust core modules by path — no wasm-bindgen involved.
#[path = "../../core/src/tokens.rs"]
mod tokens;
#[path = "../../core/src/parse.rs"]
mod parse;
#[path = "../../core/src/data.rs"]
mod data;
#[path = "../../core/src/layout.rs"]
mod layout;
#[path = "../../core/src/render.rs"]
mod render;
#[path = "../../core/src/pdf.rs"]
mod pdf;
#[path = "../../core/src/license.rs"]
mod license;
#[path = "../../core/src/encrypt.rs"]
mod encrypt;
#[path = "../../core/src/shared.rs"]
mod shared;
#[path = "../../core/src/kit_to_xml.rs"]
mod kit_to_xml;
#[path = "../../core/src/canvas.rs"]
mod canvas;

use std::io::Read;
use base64::Engine as _;

fn main() {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf).unwrap_or(0);
    print!("{}", dispatch(&buf));
}

fn dispatch(input: &str) -> String {
    let req: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return serde_json::json!({ "error": format!("request parse error: {e}") }).to_string(),
    };

    let method   = req["method"].as_str().unwrap_or("render");
    let key      = req["key"]   .as_str().unwrap_or("");
    let now_unix = req["now"]   .as_i64().unwrap_or(0);
    let body   = match req["input"].as_str() {
        Some(s) => s,
        None    => return r#"{"error":"request missing 'input' field"}"#.to_string(),
    };

    match method {
        "render_pdf" => {
            if body.len() > 1_048_576 {
                return r#"{"error":"input exceeds 1 MB limit"}"#.to_string();
            }
            let mut doc = match parse::parse(body) {
                Ok(d)  => d,
                Err(e) => return serde_json::json!({ "error": e }).to_string(),
            };
            // Optional data binding: the request may carry a "data" JSON object.
            if let Some(data_val) = req.get("data").filter(|v| !v.is_null()) {
                let json_str = data_val.to_string();
                if let Err(e) = data::apply(&mut doc, &json_str) {
                    return serde_json::json!({ "error": e }).to_string();
                }
            }
            render_pdf_doc(doc, key, now_unix, &req)
        }
        "render_tree_pdf" => {
            if body.len() > 4_194_304 {
                return r#"{"error":"input exceeds 4 MB limit"}"#.to_string();
            }
            if canvas::is_canvas_tree(body) {
                let canvas_doc = match canvas::parse_canvas_tree(body) {
                    Ok(d)  => d,
                    Err(e) => return serde_json::json!({ "error": e }).to_string(),
                };
                return render_canvas_pdf_doc(canvas_doc, key, now_unix, &req);
            }
            let doc = match parse::parse_tree(body) {
                Ok(d)  => d,
                Err(e) => return serde_json::json!({ "error": e }).to_string(),
            };
            render_pdf_doc(doc, key, now_unix, &req)
        }
        "render_tree" => {
            if body.len() > 4_194_304 {
                return r#"{"error":"input exceeds 4 MB limit"}"#.to_string();
            }
            match parse::parse_tree(body) {
                Ok(d)  => render_doc(d, key, now_unix),
                Err(e) => serde_json::json!({ "error": e }).to_string(),
            }
        }
        "kit_to_xml" => {
            match kit_to_xml::kit_to_xml(body) {
                Ok(xml) => serde_json::json!({ "xml": xml }).to_string(),
                Err(e)  => serde_json::json!({ "error": e }).to_string(),
            }
        }
        _ => {
            if body.len() > 1_048_576 {
                return r#"{"error":"input exceeds 1 MB limit"}"#.to_string();
            }
            match parse::parse(body) {
                Ok(d)  => render_doc(d, key, now_unix),
                Err(e) => serde_json::json!({ "error": e }).to_string(),
            }
        }
    }
}

/// Decode the optional `fonts` field from the request envelope into a `FontRegistry`.
fn build_registry(req: &serde_json::Value) -> pdf::FontRegistry {
    let mut registry = pdf::FontRegistry::new();
    if let Some(fonts_obj) = req.get("fonts").and_then(|v| v.as_object()) {
        for (name, val) in fonts_obj {
            if let Some(b64) = val.as_str() {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    registry.register(name, bytes);
                }
            }
        }
    }
    registry
}

/// Decode the optional `images` field from the request envelope into an `ImageRegistry`.
fn build_image_registry(req: &serde_json::Value) -> pdf::ImageRegistry {
    let mut registry = pdf::ImageRegistry::new();
    if let Some(imgs_obj) = req.get("images").and_then(|v| v.as_object()) {
        for (name, val) in imgs_obj {
            if let Some(b64) = val.as_str() {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    registry.load(name, bytes);
                }
            }
        }
    }
    registry
}

/// Decode the optional `metrics` field from the request envelope into a font-widths map.
/// Format: `{ "fontName": { "default": 500, "ascii": [260, 285, ...] } }`
/// Kept for backward compatibility; auto-extraction from font bytes is preferred.
fn build_font_widths(req: &serde_json::Value) -> std::collections::HashMap<String, tokens::FontWidths> {
    let mut map = std::collections::HashMap::new();
    if let Some(obj) = req.get("metrics").and_then(|v| v.as_object()) {
        for (name, val) in obj {
            let default = val["default"].as_u64().unwrap_or(500) as u16;
            let ascii: Vec<u16> = val["ascii"]
                .as_array()
                .map(|arr| arr.iter().map(|v| v.as_u64().unwrap_or(0) as u16).collect())
                .unwrap_or_default();
            if ascii.len() == 95 {
                map.insert(name.clone(), tokens::FontWidths { default, ascii });
            }
        }
    }
    map
}

fn render_canvas_pdf_doc(canvas_doc: canvas::CanvasDocument, license_key: &str, now_unix: i64, req: &serde_json::Value) -> String {
    let registry       = build_registry(req);
    let image_registry = build_image_registry(req);

    // Confirm every canvas image has bytes in the registry.
    for (_alias, name) in &canvas_doc.images {
        if image_registry.get(name).is_none() {
            return serde_json::json!({
                "error": format!("image '{name}' declared in assets but not loaded via loadImage()")
            }).to_string();
        }
        if let Some(bytes) = image_registry.get(name) {
            if let Some(reason) = pdf::image_format_error(bytes) {
                return serde_json::json!({
                    "error": format!("image '{name}': {reason}")
                }).to_string();
            }
        }
    }

    // Auto-extract glyph advance-width metrics from the loaded font bytes.
    let adapter_widths = build_font_widths(req);
    let mut merged = canvas_doc.font_widths.clone();
    for (name, bytes) in registry.iter() {
        if let Some(widths) = shared::extract_font_widths(bytes) {
            merged.entry(name.to_string()).or_insert(widths);
        }
    }
    for (name, widths) in adapter_widths {
        merged.entry(name).or_insert(widths);
    }
    layout::set_font_widths(merged);

    let pages = canvas::layout_canvas_sections(&canvas_doc);

    let status = license::check(license_key, now_unix);
    let watermark = if status.is_licensed() {
        None
    } else {
        Some(("made with lpdf.io", Some("https://lpdf.io")))
    };
    let watermark_ref = watermark.map(|(t, u)| (t, u));

    let created_on = req["created_on"].as_str();

    match pdf::render_pdf(&pages, &canvas_doc.fonts, &registry, &image_registry, &canvas_doc.meta, watermark_ref, created_on, status.is_licensed()) {
        Ok(bytes) => {
            let bytes = if let Some(enc) = req.get("encrypt") {
                let user_pw  = enc["user_password"] .as_str().unwrap_or("");
                let owner_pw = enc["owner_password"].as_str().unwrap_or("");
                let perms_json = enc["permissions"].to_string();
                let cfg = encrypt::EncryptConfig {
                    user_password:  user_pw.to_string(),
                    owner_password: owner_pw.to_string(),
                    permissions:    shared::parse_permissions_json(&perms_json),
                };
                match encrypt::encrypt_pdf(&bytes, &cfg) {
                    Ok(b)  => b,
                    Err(e) => return serde_json::json!({ "error": e }).to_string(),
                }
            } else {
                bytes
            };
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            serde_json::json!({ "pdf": b64 }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Overlay canvas sections from a Kit document onto the already-laid-out render pages.
/// `section_page_starts[i]` is the index of the first render page for section `i`.
fn add_canvas_nodes_to_pages(
    doc:                  &parse::Document,
    pages:                &mut Vec<render::RenderPage>,
    section_page_starts:  &[usize],
) {
    for (sec_idx, section) in doc.sections.iter().enumerate() {
        let Some(&page_start) = section_page_starts.get(sec_idx) else { continue };
        if page_start >= pages.len() { continue; }
        let page_w = pages[page_start].width;
        let page_h = pages[page_start].height;

        for sc in &section.children {
            if let parse::SectionChild::Canvas(canvas) = sc {
                for layer in &canvas.layers {
                    let node = canvas_layer_to_render(layer, page_w, page_h);
                    pages[page_start].nodes.push(node);
                }
            }
        }
    }
}

/// Convert a `parse::CanvasLayer` into a `RenderNode::CanvasLayer`.
fn canvas_layer_to_render(
    layer:  &parse::CanvasLayer,
    page_w: f32,
    page_h: f32,
) -> render::RenderNode {
    let opacity  = layer.opacity.unwrap_or(1.0).clamp(0.0, 1.0);
    let children = layer.children.iter()
        .filter_map(|p| canvas_primitive_to_render(p, page_w, page_h))
        .collect();
    render::RenderNode::CanvasLayer(render::RenderCanvasLayer {
        transform: None,
        clip:      None,
        opacity,
        children,
    })
}

/// Convert a `parse::CanvasPrimitive` into the appropriate `RenderNode`.
fn canvas_primitive_to_render(
    prim:   &parse::CanvasPrimitive,
    page_w: f32,
    _page_h: f32,
) -> Option<render::RenderNode> {
    fn resolve(pos: &parse::CanvasPosition) -> (f32, f32) {
        match pos {
            parse::CanvasPosition::Absolute { x, y } => (*x, *y),
            parse::CanvasPosition::Anchored { dx, dy, .. } => (*dx, *dy),
        }
    }
    fn dash(s: &str) -> Option<Vec<f32>> {
        let v: Vec<f32> = s.split_whitespace().filter_map(|t| t.parse().ok()).collect();
        if v.is_empty() { None } else { Some(v) }
    }
    fn cap(s: &str) -> u8 { match s { "round" => 1, "square" => 2, _ => 0 } }

    Some(match prim {
        parse::CanvasPrimitive::Rect(r) => {
            let (x, y) = resolve(&r.pos);
            render::RenderNode::CanvasRect(render::RenderCanvasRect {
                x, y,
                w: r.w, h: r.h,
                fill:          r.fill.clone(),
                stroke:        r.stroke.clone(),
                stroke_width:  r.stroke_width.unwrap_or(1.0),
                stroke_dash:   r.stroke_dash.as_deref().and_then(dash),
                border_radius: r.radius.unwrap_or(0.0),
            })
        }
        parse::CanvasPrimitive::Circle(c) => {
            let (cx, cy) = resolve(&c.pos);
            render::RenderNode::CanvasEllipse(render::RenderCanvasEllipse {
                cx, cy,
                rx: c.r, ry: c.r,
                fill:         c.fill.clone(),
                stroke:       c.stroke.clone(),
                stroke_width: c.stroke_width.unwrap_or(1.0),
                stroke_dash:  c.stroke_dash.as_deref().and_then(dash),
            })
        }
        parse::CanvasPrimitive::Ellipse(e) => {
            let (cx, cy) = resolve(&e.pos);
            render::RenderNode::CanvasEllipse(render::RenderCanvasEllipse {
                cx, cy,
                rx: e.rx, ry: e.ry,
                fill:         e.fill.clone(),
                stroke:       e.stroke.clone(),
                stroke_width: e.stroke_width.unwrap_or(1.0),
                stroke_dash:  e.stroke_dash.as_deref().and_then(dash),
            })
        }
        parse::CanvasPrimitive::Line(l) => {
            render::RenderNode::CanvasLine(render::RenderCanvasLine {
                x1: l.x1, y1: l.y1, x2: l.x2, y2: l.y2,
                stroke:       l.stroke.as_deref().unwrap_or("#000000").to_string(),
                stroke_width: l.stroke_width.unwrap_or(1.0),
                stroke_dash:  l.stroke_dash.as_deref().and_then(dash),
                line_cap:     l.line_cap.as_deref().map(cap).unwrap_or(0),
                line_join:    0,
            })
        }
        parse::CanvasPrimitive::Path(p) => {
            render::RenderNode::CanvasPath(render::RenderCanvasPath {
                d:                  p.d.clone(),
                fill:               p.fill.clone(),
                stroke:             p.stroke.clone(),
                stroke_width:       p.stroke_width.unwrap_or(1.0),
                stroke_dash:        p.stroke_dash.as_deref().and_then(dash),
                fill_rule_evenodd:  p.fill_rule.as_deref() == Some("evenodd"),
                line_cap:           p.line_cap.as_deref().map(cap).unwrap_or(0),
                line_join:          0,
            })
        }
        parse::CanvasPrimitive::Text(t) => {
            let (x, y) = resolve(&t.pos);
            let runs = t.runs.iter().map(|r| render::RenderCanvasRun {
                text:  r.text.clone(),
                font:  r.font.clone(),
                size:  None,
                color: r.color.clone(),
            }).collect();
            render::RenderNode::CanvasText(render::RenderCanvasText {
                x, y,
                font:        t.font.as_deref().unwrap_or("Helvetica").to_string(),
                size:        t.font_size.unwrap_or(12.0),
                color:       t.color.as_deref().unwrap_or("#000000").to_string(),
                align:       t.align.as_deref().unwrap_or("left").to_string(),
                line_height: t.line_height.unwrap_or(1.2),
                width:       t.w.or(Some(page_w - x)),
                content:     t.content.clone(),
                runs,
            })
        }
        parse::CanvasPrimitive::Img(i) => {
            let (x, y) = resolve(&i.pos);
            render::RenderNode::CanvasImage(render::RenderCanvasImage {
                x, y, w: i.w, h: i.h,
                src: i.name.clone(),
            })
        }
    })
}

fn render_pdf_doc(mut doc: parse::Document, license_key: &str, now_unix: i64, req: &serde_json::Value) -> String {
    let registry       = build_registry(req);
    let image_registry = build_image_registry(req);

    // Confirm every image declared in <assets> has bytes in the registry.
    for (_alias, name) in &doc.images {
        if image_registry.get(name).is_none() {
            return serde_json::json!({
                "error": format!("image '{name}' declared in <assets> but not loaded via loadImage()")
            }).to_string();
        }
        if let Some(bytes) = image_registry.get(name) {
            if let Some(reason) = pdf::image_format_error(bytes) {
                return serde_json::json!({
                    "error": format!("image '{name}': {reason}")
                }).to_string();
            }
        }
    }

    let meta = pdf::build_image_meta(&image_registry);
    {
        let mut lp = doc.section_layouts();
        for page in &mut lp {
            layout::prefill_image_sizes(&mut page.children, &meta);
        }
    }

    // Auto-extract glyph advance-width metrics from the loaded font bytes.
    // This is the primary source of font metrics; adapter-supplied `metrics`
    // (kept for backward compatibility) fill in any remaining gaps.
    for (name, bytes) in registry.iter() {
        if let Some(widths) = shared::extract_font_widths(bytes) {
            doc.font_widths.entry(name.to_string()).or_insert(widths);
        }
    }
    let adapter_widths = build_font_widths(req);
    for (name, widths) in adapter_widths {
        doc.font_widths.entry(name).or_insert(widths);
    }

    layout::set_font_widths(doc.font_widths.clone());
    let lp = doc.section_layouts();
    let mut section_page_starts: Vec<usize> = Vec::new();
    let mut pages: Vec<render::RenderPage> = Vec::new();
    for page in lp.iter() {
        section_page_starts.push(pages.len());
        pages.extend(layout::layout_page(page));
    }
    add_canvas_nodes_to_pages(&doc, &mut pages, &section_page_starts);

    let status = license::check(license_key, now_unix);
    let watermark = if status.is_licensed() {
        None
    } else {
        Some(("made with lpdf.io", Some("https://lpdf.io")))
    };
    let watermark_ref = watermark.map(|(t, u)| (t, u));

    let created_on = req["created_on"].as_str();

    match pdf::render_pdf(&pages, &doc.fonts, &registry, &image_registry, &doc.meta, watermark_ref, created_on, status.is_licensed()) {
        Ok(bytes) => {
            let bytes = if let Some(enc) = req.get("encrypt") {
                let user_pw  = enc["user_password"] .as_str().unwrap_or("");
                let owner_pw = enc["owner_password"].as_str().unwrap_or("");
                let perms_json = enc["permissions"].to_string();
                let cfg = encrypt::EncryptConfig {
                    user_password:  user_pw.to_string(),
                    owner_password: owner_pw.to_string(),
                    permissions:    shared::parse_permissions_json(&perms_json),
                };
                match encrypt::encrypt_pdf(&bytes, &cfg) {
                    Ok(b)  => b,
                    Err(e) => return serde_json::json!({ "error": e }).to_string(),
                }
            } else {
                bytes
            };
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            serde_json::json!({ "pdf": b64 }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

/// Test-only stub that mirrors `LpdfEngine::render_xml_to_pdf_bytes` from the
/// core library crate so that `encrypt.rs` tests can reference `crate::LpdfEngine`
/// even when compiled inside this WASI binary crate.
#[cfg(test)]
struct LpdfEngine;

#[cfg(test)]
impl LpdfEngine {
    fn render_xml_to_pdf_bytes(xml: &str) -> Result<Vec<u8>, String> {
        let doc = parse::parse(xml)?;
        let lp = doc.section_layouts();
        let pages: Vec<render::RenderPage> =
            lp.iter().flat_map(layout::layout_page).collect();
        let wm = Some(("made with lpdf.io", Some("https://lpdf.io")));
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(), &doc.meta, wm, None, false)
    }
}

fn render_doc(doc: parse::Document, license_key: &str, now_unix: i64) -> String {
    let font_widths = doc.font_widths.clone();
    shared::render_doc_shared(doc, font_widths, license_key, now_unix)
}

