mod layout;
mod license;
mod parse;
mod pdf;
mod render;
mod tokens;

use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct LpdfEngine {
    license_key: String,
    /// Per-engine font registry; populated via `load_font`.
    fonts:       pdf::FontRegistry,
    /// Per-engine image registry; populated via `load_image`.
    images:      pdf::ImageRegistry,
    /// Caller-supplied glyph width tables; populated via `set_font_metrics`.
    /// Used by the layout engine to measure custom-font text accurately.
    font_widths: HashMap<String, tokens::FontWidths>,
    /// Optional ISO 8601 creation timestamp for the PDF `/CreationDate` field.
    created_on:  Option<String>,
    /// Current Unix timestamp (seconds) used for license expiry checking.
    /// Set via `set_now()`.  Defaults to `0` (expiry check skipped).
    now_unix:    i64,
}

#[wasm_bindgen]
impl LpdfEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(license_key: &str) -> LpdfEngine {
        LpdfEngine {
            license_key: license_key.to_string(),
            fonts:       pdf::FontRegistry::new(),
            images:      pdf::ImageRegistry::new(),
            font_widths: HashMap::new(),
            created_on:  None,
            now_unix:    0,
        }
    }

    /// Register raw font bytes (TTF/OTF) for a custom font name.
    /// Call this once per font before calling `render_pdf`.
    /// Glyph advance-width metrics are extracted automatically from the font
    /// bytes so the layout engine can measure text accurately — no separate
    /// `set_font_metrics` call is required.
    pub fn load_font(&mut self, name: &str, bytes: &[u8]) {
        self.fonts.register(name, bytes.to_vec());
        if let Some(widths) = extract_font_widths(bytes) {
            self.font_widths.insert(name.to_string(), widths);
        }
    }

    /// Register raw image bytes (JPEG or PNG) for an image name.
    /// Call this for every image referenced by `<img name="…">` nodes.
    pub fn load_image(&mut self, name: &str, bytes: &[u8]) {
        self.images.load(name, bytes.to_vec());
    }

    /// Set an optional ISO 8601 creation timestamp (e.g. `"2024-06-01T12:00:00"`).
    /// When provided, written as `/CreationDate` in the PDF info dictionary.
    /// Omitting this keeps builds reproducible (no embedded timestamp).
    pub fn set_created_on(&mut self, iso: &str) {
        self.created_on = Some(iso.to_string());
    }

    /// Set the current Unix timestamp (seconds) for license expiry checking.
    /// Must be called before `render_pdf` when using a time-limited token.
    /// If not set (default `0`), expiry is not checked.
    pub fn set_now(&mut self, unix: i64) {
        self.now_unix = unix;
    }

    /// Inject glyph advance-width tables for custom fonts.
    ///
    /// Call this *before* `render_pdf` / `render` when the document uses custom
    /// fonts (declared via `<font src="…"`). The adapter extracts these widths
    /// from the font binary and passes them as a JSON object:
    ///
    /// ```json
    /// { "fontName": { "default": 500, "ascii": [260, 285, …] } }
    /// ```
    ///
    /// `ascii` is a 95-element array for code points 32–126. `default` is used
    /// for code points outside that range. All values are in 1/1000 em units.
    pub fn set_font_metrics(&mut self, json: &str) {
        if let Ok(map) = serde_json::from_str::<serde_json::Value>(json) {
            if let Some(obj) = map.as_object() {
                for (name, v) in obj {
                    let default = v.get("default").and_then(|d| d.as_u64()).unwrap_or(500) as u16;
                    let ascii: Vec<u16> = v.get("ascii")
                        .and_then(|a| a.as_array())
                        .map(|arr| arr.iter().map(|n| n.as_u64().unwrap_or(500) as u16).collect())
                        .unwrap_or_default();
                    self.font_widths.insert(name.clone(), tokens::FontWidths { default, ascii });
                }
            }
        }
    }

    /// Render `xml` to binary PDF bytes.
    ///
    /// Any custom fonts referenced in `<font src="…">` declarations must have
    /// their bytes registered via `load_font` before calling this method.
    pub fn render_pdf(&self, xml: &str) -> Result<Vec<u8>, JsValue> {
        if xml.len() > 1_048_576 {
            return Err(JsValue::from_str("input exceeds 1 MB limit"));
        }

        let mut doc = parse::parse(xml)
            .map_err(|e| JsValue::from_str(&e))?;

        // Confirm every image declared in <assets> has bytes in the registry.
        for name in &doc.images {
            if self.images.get(name).is_none() {
                return Err(JsValue::from_str(&format!(
                    "image '{name}' declared in <assets> but not loaded via loadImage()"
                )));
            }
            if let Some(bytes) = self.images.get(name) {
                if let Some(reason) = pdf::image_format_error(bytes) {
                    return Err(JsValue::from_str(&format!(
                        "image '{name}': {reason}"
                    )));
                }
            }
        }

        let meta = pdf::build_image_meta(&self.images);
        for page in &mut doc.pages {
            layout::prefill_image_sizes(&mut page.children, &meta);
        }

        // Install font width tables so the layout engine can measure custom
        // fonts accurately. Engine-level widths (from set_font_metrics) are used
        // for the XML path; there are no doc-level widths for XML input.
        layout::set_font_widths(self.font_widths.clone());

        let pages: Vec<render::RenderPage> =
            doc.pages.iter().flat_map(layout::layout_page).collect();

        let status = license::check(&self.license_key, self.now_unix);
        let wm: Option<(&str, Option<&str>)> = if status.is_licensed() {
            None
        } else {
            Some(("made with lpdf.io", Some("https://lpdf.io")))
        };

        pdf::render_pdf(
            &pages,
            &doc.fonts,
            &self.fonts,
            &self.images,
            &doc.meta,
            wm,
            self.created_on.as_deref(),
            status.is_licensed(),
        )
        .map_err(|e| JsValue::from_str(&e))
    }

    pub fn render(&self, xml: &str) -> String {
        if xml.len() > 1_048_576 {
            return r#"{"error":"input exceeds 1 MB limit"}"#.to_string();
        }

        let doc = match parse::parse(xml) {
            Ok(d) => d,
            Err(e) => {
                return serde_json::json!({ "error": e }).to_string();
            }
        };

        self.render_doc(doc)
    }

    pub fn render_tree(&self, json: &str) -> String {
        if json.len() > 4_194_304 {
            return r#"{"error":"input exceeds 4 MB limit"}"#.to_string();
        }

        let doc = match parse::parse_tree(json) {
            Ok(d) => d,
            Err(e) => {
                return serde_json::json!({ "error": e }).to_string();
            }
        };

        self.render_doc(doc)
    }
}

// Private helpers — not exported to WASM.
impl LpdfEngine {
    /// Render XML to PDF bytes without WASM error types — used by tests.
    #[cfg(test)]
    pub(crate) fn render_xml_to_pdf_bytes(xml: &str) -> Result<Vec<u8>, String> {
        let doc = parse::parse(xml)?;
        let pages: Vec<render::RenderPage> =
            doc.pages.iter().flat_map(layout::layout_page).collect();
        // Render as unlicensed (with watermark) to match what the adapters
        // produce when no valid license key is supplied — keeps snapshot hashes
        // consistent between the Rust tests and the adapter test suites.
        let wm = Some(("made with lpdf.io", Some("https://lpdf.io")));
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(), &doc.meta, wm, None, false)
    }

    fn render_doc(&self, doc: parse::Document) -> String {
        // Merge widths: engine-level (from set_font_metrics) + doc-level (from
        // tree JSON). Doc-level takes precedence — the adapter that built the
        // tree knows the exact bytes it loaded.
        let mut merged = self.font_widths.clone();
        merged.extend(doc.font_widths.clone());
        layout::set_font_widths(merged);

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

        let status = license::check(&self.license_key, self.now_unix);
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
}

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

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn render_value(xml: &str) -> serde_json::Value {
        let engine = LpdfEngine::new("");
        serde_json::from_str(&engine.render(xml))
            .expect("render() returned invalid JSON")
    }

    fn minimal(body: &str) -> String {
        format!(
            r#"<lpdf version="1"><document size="a4" margin="28pt"><pages><page>{body}</page></pages></document></lpdf>"#
        )
    }

    #[test]
    fn unlicensed_render_includes_watermark() {
        let engine = LpdfEngine::new("");
        let result: serde_json::Value =
            serde_json::from_str(&engine.render(&minimal(""))).unwrap();
        assert!(!result["watermark"].is_null());
        assert_eq!(result["watermark"]["type"], "lpdf:watermark");
    }

    #[test]
    fn malformed_token_falls_back_to_watermark_with_warning() {
        let engine = LpdfEngine::new("not-a-valid-token");
        let result: serde_json::Value =
            serde_json::from_str(&engine.render(&minimal(""))).unwrap();
        // Should still render (no hard error)
        assert!(result["error"].is_null());
        // Should have watermark (falls back to free mode)
        assert!(!result["watermark"].is_null());
        assert_eq!(result["watermark"]["type"], "lpdf:watermark");
        // Should carry a warning
        assert!(result["license_warning"].is_string());
    }

    #[test]
    fn render_tree_has_expected_shape() {
        let result = render_value(&minimal(""));
        assert_eq!(result["version"], 1);
        assert!(result["pages"].is_array());
        assert_eq!(result["pages"].as_array().unwrap().len(), 1);
        let page = &result["pages"][0];
        assert_eq!(page["width"], 595.28);
        assert_eq!(page["height"], 841.89);
    }

    #[test]
    fn input_too_large_returns_error() {
        let engine = LpdfEngine::new("key");
        let big = "x".repeat(1_048_577);
        let result: serde_json::Value =
            serde_json::from_str(&engine.render(&big)).unwrap();
        assert_eq!(result["error"], "input exceeds 1 MB limit");
    }

    #[test]
    fn invalid_xml_returns_error() {
        let engine = LpdfEngine::new("key");
        let result: serde_json::Value =
            serde_json::from_str(&engine.render("<unclosed")).unwrap();
        assert!(result["error"].is_string());
    }

    #[test]
    fn full_page_example_renders_without_error() {
        let xml = r##"<lpdf version="1">
            <document size="a4" margin="28pt">
                <pages>
                    <page background="surface">
                        <stack gap="m">
                            <frame background="primary" padding="m" radius="s">
                                <flank gap="m" align="center" end="true">
                                    <frame width="120pt" height="24pt" background="secondary" radius="xs" />
                                    <frame width="80pt" height="14pt" background="surface" radius="xs" />
                                </flank>
                            </frame>
                            <divider color="#e0e0e0" thickness="xs" />
                            <grid cols="3" gap="m">
                                <frame padding="s" border="xs #e0e0e0" radius="xs">
                                    <stack gap="s">
                                        <frame height="10pt" background="text-muted" radius="xs" />
                                    </stack>
                                </frame>
                                <frame padding="s" border="xs #e0e0e0" radius="xs" />
                                <frame padding="s" border="xs #e0e0e0" radius="xs" />
                            </grid>
                        </stack>
                    </page>
                </pages>
            </document>
        </lpdf>"##;

        let result = render_value(xml);
        assert!(!result["pages"].is_null());
        assert!(result["error"].is_null());
    }

    #[test]
    fn text_node_renders() {
        let xml = minimal(r#"<text size="m" color="text">Invoice for services rendered</text>"#);
        let result = render_value(&xml);
        let page = &result["pages"][0];
        let node = &page["nodes"][0];
        assert_eq!(node["type"], "box");
        let kids = node["children"].as_array().unwrap();
        assert!(!kids.is_empty());
        assert_eq!(kids[0]["type"], "text");
    }

    // ── render_pdf primary path ───────────────────────────────────────────────

    #[test]
    fn render_pdf_produces_pdf_header() {
        let bytes = LpdfEngine::render_xml_to_pdf_bytes(&minimal("")).unwrap();
        assert_eq!(&bytes[..5], b"%PDF-");
    }

    #[test]
    fn render_pdf_invalid_xml_returns_error() {
        let result = LpdfEngine::render_xml_to_pdf_bytes("<unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn render_pdf_size_limit() {
        // render_pdf's 1 MB guard is also enforced by render(); test via the
        // JSON path which works on non-WASM targets (JsValue panics otherwise).
        let engine = LpdfEngine::new("");
        let big = "x".repeat(1_048_577);
        let result: serde_json::Value = serde_json::from_str(&engine.render(&big)).unwrap();
        assert_eq!(result["error"], "input exceeds 1 MB limit");
    }
}
