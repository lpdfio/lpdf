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

    let method = req["method"].as_str().unwrap_or("render");
    let key    = req["key"]   .as_str().unwrap_or("");
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
            render_pdf_doc(doc, key, &req)
        }
        "render_tree_pdf" => {
            if body.len() > 4_194_304 {
                return r#"{"error":"input exceeds 4 MB limit"}"#.to_string();
            }
            let doc = match parse::parse_tree(body) {
                Ok(d)  => d,
                Err(e) => return serde_json::json!({ "error": e }).to_string(),
            };
            render_pdf_doc(doc, key, &req)
        }
        "render_tree" => {
            if body.len() > 4_194_304 {
                return r#"{"error":"input exceeds 4 MB limit"}"#.to_string();
            }
            match parse::parse_tree(body) {
                Ok(d)  => render_doc(d, key),
                Err(e) => serde_json::json!({ "error": e }).to_string(),
            }
        }
        _ => {
            if body.len() > 1_048_576 {
                return r#"{"error":"input exceeds 1 MB limit"}"#.to_string();
            }
            match parse::parse(body) {
                Ok(d)  => render_doc(d, key),
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

fn render_pdf_doc(doc: parse::Document, license_key: &str, req: &serde_json::Value) -> String {
    let pages: Vec<render::RenderPage> =
        doc.pages.iter().flat_map(layout::layout_page).collect();

    let registry = build_registry(req);

    let watermark = if license_key.is_empty() {
        Some(("made with lpdf.io", Some("https://lpdf.io")))
    } else {
        None
    };
    let watermark_ref = watermark.map(|(t, u)| (t, u));

    let created_on = req["created_on"].as_str();

    match pdf::render_pdf(&pages, &doc.fonts, &registry, &doc.meta, watermark_ref, created_on) {
        Ok(bytes) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            serde_json::json!({ "pdf": b64 }).to_string()
        }
        Err(e) => serde_json::json!({ "error": e }).to_string(),
    }
}

fn render_doc(doc: parse::Document, license_key: &str) -> String {
    let pages: Vec<render::RenderPage> =
        doc.pages.iter().flat_map(layout::layout_page).collect();

    let fonts: serde_json::Map<String, serde_json::Value> = doc.fonts
        .into_iter()
        .map(|(name, def)| {
            let v = match def {
                tokens::FontDef::Builtin(b) => serde_json::json!({ "builtin": b }),
                tokens::FontDef::Src(s)     => serde_json::json!({ "src": s }),
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

    let watermark = if license_key.is_empty() {
        serde_json::json!({
            "type": "lpdf:watermark",
            "text": "made with lpdf.io",
            "url":  "https://lpdf.io"
        })
    } else {
        serde_json::Value::Null
    };

    render::pages_to_json(&pages, meta, watermark)
}

