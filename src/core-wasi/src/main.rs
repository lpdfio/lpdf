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
#[path = "../../core/src/layout.rs"]
mod layout;
#[path = "../../core/src/render.rs"]
mod render;
#[path = "../../core/src/pdf.rs"]
mod pdf;
#[path = "../../core/src/license.rs"]
mod license;

use std::io::Read;
use base64::Engine as _;

/// Extract per-glyph advance widths for printable ASCII (code points 32–126)
/// from raw TrueType/OpenType font bytes, normalised to 1/1000 em units.
/// Returns `None` if the font cannot be parsed (WOFF/WOFF2, corrupt data, etc.).
fn extract_font_widths(bytes: &[u8]) -> Option<tokens::FontWidths> {
    let face = ttf_parser::Face::parse(bytes, 0).ok()?;
    let upm = face.units_per_em() as u32;
    if upm == 0 { return None; }

    let mut ascii: Vec<u16> = Vec::with_capacity(95);
    let mut sum: u32 = 0;
    let mut count: u32 = 0;

    for cp in 32u32..=126 {
        let ch = char::from_u32(cp).unwrap_or(' ');
        let adv = face.glyph_index(ch)
            .and_then(|gid| face.glyph_hor_advance(gid))
            .unwrap_or_else(|| face.glyph_hor_advance(ttf_parser::GlyphId(0)).unwrap_or(0));
        let w = ((adv as u32 * 1000 + upm / 2) / upm) as u16;
        ascii.push(w);
        if w > 0 { sum += w as u32; count += 1; }
    }

    let default = if count > 0 { ((sum + count / 2) / count) as u16 } else { 500 };
    Some(tokens::FontWidths { default, ascii })
}

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
            let doc = match parse::parse(body) {
                Ok(d)  => d,
                Err(e) => return serde_json::json!({ "error": e }).to_string(),
            };
            render_pdf_doc(doc, key, now_unix, &req)
        }
        "render_tree_pdf" => {
            if body.len() > 4_194_304 {
                return r#"{"error":"input exceeds 4 MB limit"}"#.to_string();
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

fn render_pdf_doc(mut doc: parse::Document, license_key: &str, now_unix: i64, req: &serde_json::Value) -> String {
    let registry       = build_registry(req);
    let image_registry = build_image_registry(req);

    // Confirm every image declared in <assets> has bytes in the registry.
    for name in &doc.images {
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
    for page in &mut doc.pages {
        layout::prefill_image_sizes(&mut page.children, &meta);
    }

    // Auto-extract glyph advance-width metrics from the loaded font bytes.
    // This is the primary source of font metrics; adapter-supplied `metrics`
    // (kept for backward compatibility) fill in any remaining gaps.
    for (name, bytes) in registry.iter() {
        if let Some(widths) = extract_font_widths(bytes) {
            doc.font_widths.entry(name.to_string()).or_insert(widths);
        }
    }
    let adapter_widths = build_font_widths(req);
    for (name, widths) in adapter_widths {
        doc.font_widths.entry(name).or_insert(widths);
    }

    layout::set_font_widths(doc.font_widths.clone());
    let pages: Vec<render::RenderPage> =
        doc.pages.iter().flat_map(layout::layout_page).collect();

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
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            serde_json::json!({ "pdf": b64 }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn render_doc(doc: parse::Document, license_key: &str, now_unix: i64) -> String {
    layout::set_font_widths(doc.font_widths.clone());
    let pages: Vec<render::RenderPage> =
        doc.pages.iter().flat_map(layout::layout_page).collect();

    let fonts: serde_json::Map<String, serde_json::Value> = doc.fonts
        .into_iter()
        .map(|(name, def)| {
            let v = match def {
                tokens::FontDef::Core(b) => serde_json::json!({ "core": b }),
                tokens::FontDef::Ref(s)  => serde_json::json!({ "ref": s }),
            };
            (name, v)
        })
        .collect();

    let keywords: Vec<&str> = doc.meta.keywords
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    let meta = serde_json::json!({
        "title":    doc.meta.title,
        "author":   doc.meta.author,
        "subject":  doc.meta.subject,
        "keywords": keywords,
        "creator":  doc.meta.creator,
        "fonts":    fonts,
    });

    let status = license::check(license_key, now_unix);
    let watermark = if status.is_licensed() {
        serde_json::Value::Null
    } else {
        serde_json::json!({
            "type": "lpdf:watermark",
            "text": "made with lpdf.io",
            "url":  "https://lpdf.io"
        })
    };

    let mut output = render::pages_to_json(&pages, meta, watermark);
    if let Some(warn) = status.warning() {
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&output) {
            val["license_warning"] = serde_json::Value::String(warn.to_string());
            output = val.to_string();
        }
    }
    output
}

