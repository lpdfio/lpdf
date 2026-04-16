mod layout;
mod license;
mod parse;
mod pdf;
mod render;
mod tokens;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct LpdfEngine {
    license_key: String,
    /// Per-engine font registry; populated via `load_font`.
    fonts:       pdf::FontRegistry,
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
            created_on:  None,
            now_unix:    0,
        }
    }

    /// Register raw font bytes (TTF/OTF) for a custom font name.
    /// Call this once per font before calling `render_pdf`.
    pub fn load_font(&mut self, name: &str, bytes: &[u8]) {
        self.fonts.register(name, bytes.to_vec());
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

    /// Render `xml` to binary PDF bytes.
    ///
    /// Any custom fonts referenced in `<font src="…">` declarations must have
    /// their bytes registered via `load_font` before calling this method.
    pub fn render_pdf(&self, xml: &str) -> Result<Vec<u8>, JsValue> {
        if xml.len() > 1_048_576 {
            return Err(JsValue::from_str("input exceeds 1 MB limit"));
        }

        let doc = parse::parse(xml)
            .map_err(|e| JsValue::from_str(&e))?;

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
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &doc.meta, wm, None, false)
    }

    fn render_doc(&self, doc: parse::Document) -> String {
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
}
