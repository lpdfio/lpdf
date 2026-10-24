/// WASI binary entrypoint for lpdf.
///
/// Protocol (stdin → stdout):
/// ```json
/// { "method": "render" | "render_tree" | "render_pdf" | "render_tree_pdf" | "check_license",
///   "key": "…", "input": "…xml or json…", "now": 1790812800 }
/// ```
/// - `render` / `render_tree` return the RenderTree JSON string directly.
/// - `render_pdf` / `render_tree_pdf` return `{ "pdf": "<base64>" }` on success
///   or `{ "error": "…" }` on failure.
/// - `check_license` reports what `key` is — `{ "status": "licensed", … }` — and takes no
///   `input`. `now` is the host's clock in seconds; without it expiry is not checked.
///
/// For `render_pdf` / `render_tree_pdf`, custom font bytes can be supplied:
/// ```json
/// { "fonts": { "Inter": "<base64 TTF bytes>", … } }
/// ```
///
/// Encryption is requested with an `encrypt` object. `algorithm` is `"aes-256"` (the
/// default) or `"rc4-128"`; AES-256 also needs `entropy`, at least 32 random bytes from
/// the host's secure generator, base64-encoded:
/// ```json
/// { "encrypt": { "user_password": "", "owner_password": "…", "permissions": { "copy": false },
///                "algorithm": "aes-256", "entropy": "<base64 random bytes>" } }
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
#[path = "../../core/src/aes256.rs"]
mod aes256;
#[path = "../../core/src/encrypt.rs"]
mod encrypt;
#[path = "../../core/src/rewrite.rs"]
mod rewrite;
#[path = "../../core/src/shared.rs"]
mod shared;
#[path = "../../core/src/kit_to_xml.rs"]
mod kit_to_xml;
#[path = "../../core/src/page_scope.rs"]
mod page_scope;
#[path = "../../core/src/canvas.rs"]
mod canvas;
#[path = "../../core/src/toc.rs"]
mod toc;

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

    // Answered before `input` is required: this method asks about the key itself, and there is
    // no document to render.
    if method == "check_license" {
        return license::report_json(key, now_unix);
    }

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
    let mut lp = doc.section_layouts();
    for page in &mut lp {
        layout::prefill_section_image_sizes(page, &meta);
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
    let pages: Vec<render::RenderPage> = layout::layout_document(&lp);

    let status = license::check(license_key, now_unix);
    let created_on = req["created_on"].as_str();

    // An unlicensed status draws the attribution line on every page.
    match pdf::render_pdf(&pages, &doc.fonts, &registry, &image_registry, &doc.meta, &doc.gradients, created_on, status.is_licensed()) {
        Ok(bytes) => {
            let bytes = if let Some(enc) = req.get("encrypt") {
                let cfg = match encrypt_config(enc) {
                    Ok(cfg) => cfg,
                    Err(e)  => return serde_json::json!({ "error": e }).to_string(),
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
        let mut doc = parse::parse(xml)?;
        let lp = doc.section_layouts();
        let pages: Vec<render::RenderPage> =
            layout::layout_document(&lp);
        // Unlicensed, as in the core stub, so both crates hash the same attributed bytes.
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(), &doc.meta, &doc.gradients, None, false)
    }
}

/// Reads the request's `encrypt` object into a config the core can use.
fn encrypt_config(enc: &serde_json::Value) -> Result<encrypt::EncryptConfig, String> {
    let algorithm = match enc["algorithm"].as_str() {
        Some(name) => encrypt::Algorithm::parse(name)?,
        None       => encrypt::Algorithm::default(),
    };
    let entropy = match enc["entropy"].as_str() {
        Some(text) => base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|e| format!("encrypt.entropy is not valid base64: {e}"))?,
        None => Vec::new(),
    };
    let cfg = encrypt::EncryptConfig {
        user_password:  enc["user_password"] .as_str().unwrap_or("").to_string(),
        owner_password: enc["owner_password"].as_str().unwrap_or("").to_string(),
        permissions:    shared::parse_permissions_json(&enc["permissions"].to_string()),
        algorithm,
        entropy,
    };
    cfg.validate()?;
    Ok(cfg)
}

fn render_doc(doc: parse::Document, license_key: &str, now_unix: i64) -> String {
    let font_widths = doc.font_widths.clone();
    shared::render_doc_shared(doc, font_widths, license_key, now_unix)
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG of 1 × 2 pixels: an image twice as high as it is wide.
    const TALL_PNG: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAACCAIAAAAW4yFwAAAADklEQVR4nGMwSJjAAMQACEgCQStAOpQAAAAASUVORK5CYII=";

    /// The image matrices (`cm` lines) in the content streams of a PDF.
    fn image_matrices(pdf: &[u8]) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = pdf;
        while let Some(start) = rest.windows(7).position(|w| w == b"stream\n") {
            rest = &rest[start + 7..];
            let end = rest.windows(10).position(|w| w == b"\nendstream").unwrap_or(rest.len());
            if let Ok(content) = miniz_oxide::inflate::decompress_to_vec_zlib(&rest[..end]) {
                let text = String::from_utf8_lossy(&content);
                found.extend(text.lines().filter(|l| l.ends_with(" cm")).map(str::to_string));
            }
            rest = &rest[end..];
        }
        found
    }

    #[test]
    fn an_img_at_in_a_layer_is_sized_by_the_proportions_of_its_image() {
        let xml = r#"<lpdf version="1"><assets><image name="logo"/></assets><document size="a4"><section>
            <layer><img-at name="logo" x="10pt" y="10pt" width="50pt"/></layer>
            <layer><img-at name="logo" anchor="top-right" width="100pt" height="100pt"/></layer>
        </section></document></lpdf>"#;
        let req = serde_json::json!({ "images": { "logo": TALL_PNG } });
        let out = render_pdf_doc(parse::parse(xml).unwrap(), "", 0, &req);
        let out: serde_json::Value = serde_json::from_str(&out).unwrap();
        let pdf = base64::engine::general_purpose::STANDARD.decode(out["pdf"].as_str().expect(&out.to_string())).unwrap();

        let matrices = image_matrices(&pdf);
        // One size given: the other follows the image, so 50pt wide is 100pt high. Both given: the image fits
        // in the square, so 100pt high is 50pt wide, and the box touches the corner of the page.
        assert!(matrices.iter().any(|m| m.starts_with("50 0 0 100 10 ")), "{matrices:?}");
        assert!(matrices.iter().any(|m| m.starts_with("50 0 0 100 545.28")), "{matrices:?}");
    }
}
