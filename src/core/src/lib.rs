pub mod codegen;
mod aes256;
mod canvas;
mod data;
mod encrypt;
mod kit_to_xml;
mod layout;
mod lists;
pub mod markdown;
// Public for native consumers — the CLI's `license` command reports a key's standing from the
// typed result rather than parsing the JSON one back.
pub mod license;
mod page_scope;
mod parse;
mod pdf;
mod render;
mod rewrite;
mod shared;
mod toc;
mod tokens;

use std::collections::HashMap;
use wasm_bindgen::prelude::*;

pub use encrypt::RenderEncryption;

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
    /// The text of the files that a `<markdown-styles src>` names, by `src`; populated via `load_markdown_styles`.
    markdown_styles: HashMap<String, String>,
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
            markdown_styles: HashMap::new(),
        }
    }

    /// Register raw font bytes (TTF/OTF) for a custom font name.
    /// Call this once per font before calling `render_pdf`.
    /// Glyph advance-width metrics are extracted automatically from the font
    /// bytes so the layout engine can measure text accurately — no separate
    /// `set_font_metrics` call is required.
    pub fn load_font(&mut self, name: &str, bytes: &[u8]) {
        self.fonts.register(name, bytes.to_vec());
        if let Some(widths) = shared::extract_font_widths(bytes) {
            self.font_widths.insert(name.to_string(), widths);
        }
    }

    /// Register raw image bytes (JPEG or PNG) for an image name.
    /// Call this for every image referenced by `<img name="…">` nodes.
    pub fn load_image(&mut self, name: &str, bytes: &[u8]) {
        self.images.load(name, bytes.to_vec());
    }

    /// Register the text of a file of Markdown rules, the root of which is a `<markdown-styles>`, for the
    /// `src` that names it. The engine reads no files: call this once for every `<markdown-styles src="…">`
    /// of the document before `render_pdf`, as `load_font` is called for a font.
    pub fn load_markdown_styles(&mut self, src: &str, xml: &str) {
        self.markdown_styles.insert(src.to_string(), xml.to_string());
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
                    self.font_widths.insert(name.clone(), tokens::FontWidths::new(default, ascii, HashMap::new()));
                }
            }
        }
    }

    /// Render `xml` to binary PDF bytes.
    ///
    /// Any custom fonts referenced in `<font src="…">` declarations must have
    /// their bytes registered via `load_font` before calling this method.
    ///
    /// `json_data` is an optional JSON string used to resolve `data-*`
    /// attributes in the template.  Pass `None` (or `null` / `undefined` from
    /// JavaScript) to render the template with its inline fallback content.
    ///
    /// The rest is for a document that asks for encryption in its `<output>`, which sets the cipher and
    /// the permissions: the passwords, which the template never holds, and `entropy`, at least 32 random
    /// bytes from the host's secure generator for AES-256. A document that asks for encryption needs an
    /// owner password, or `skip_encryption` to render it unencrypted, as a preview does; passwords for a
    /// document that doesn't ask for it are an error, and so are passwords with `skip_encryption`.
    pub fn render_pdf(
        &self,
        xml: &str,
        json_data: Option<String>,
        user_password: Option<String>,
        owner_password: Option<String>,
        skip_encryption: bool,
        entropy: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        let encryption = RenderEncryption { user_password, owner_password, skip_encryption, entropy: entropy.to_vec() };
        self.render_xml(xml, json_data.as_deref(), &encryption).map_err(|e| JsValue::from_str(&e))
    }

    /// Render a JSON kit-tree document to PDF bytes.
    ///
    /// This is the JSON counterpart of `render_pdf`. The Node adapter uses it
    /// when an `LpdfDocument` Kit tree is passed to `renderPdf()`, avoiding an
    /// intermediate XML serialisation step. PHP, Python, and .NET adapters also
    /// use this entry point.
    ///
    /// The passwords, `skip_encryption` and `entropy` are those of `render_pdf`, for a document whose
    /// `output` asks for encryption.
    pub fn render_tree_pdf(
        &self,
        json: &str,
        user_password: Option<String>,
        owner_password: Option<String>,
        skip_encryption: bool,
        entropy: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        if json.len() > 4_194_304 {
            return Err(JsValue::from_str("input exceeds 4 MB limit"));
        }

        // ── Kit mode ──────────────────────────────────────────────────────────
        let mut doc = parse::parse_tree_with(json, &self.resources())
            .map_err(|e| JsValue::from_str(&e))?;

        // Checked before the layout, so a missing or stray password fails without the work of a render.
        let encryption = RenderEncryption { user_password, owner_password, skip_encryption, entropy: entropy.to_vec() };
        let config = encryption.resolve(doc.output.encryption.as_ref()).map_err(|e| JsValue::from_str(&e))?;

        // Confirm every image declared in the tree has bytes in the registry.
        for (_alias, name) in &doc.images {
            if self.images.get(name).is_none() {
                return Err(JsValue::from_str(&format!(
                    "image '{name}' declared in assets but not loaded via loadImage()"
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
        let mut lp = doc.section_layouts();
        for page in &mut lp {
            layout::prefill_section_image_sizes(page, &meta);
        }

        // Merge font widths: engine-level + doc-level (doc-level takes precedence).
        let mut merged = self.font_widths.clone();
        merged.extend(std::mem::take(&mut doc.font_widths));
        layout::set_font_widths(merged);

        let pages: Vec<render::RenderPage> =
            layout::layout_document(&lp);

        let status = license::check(&self.license_key, self.now_unix);
        // An unlicensed status draws the attribution line on every page.
        let bytes = pdf::render_pdf(
            &pages,
            &doc.fonts,
            &self.fonts,
            &self.images,
            &doc.meta,
            &doc.gradients,
            self.created_on.as_deref(),
            status.is_licensed(),
        )
        .map_err(|e| JsValue::from_str(&e))?;

        let bytes = match &config {
            Some(cfg) => encrypt::encrypt_pdf(&bytes, cfg).map_err(|e| JsValue::from_str(&e))?,
            None      => bytes,
        };
        Ok(bytes)
    }

    pub fn render(&self, xml: &str) -> String {
        if xml.len() > 1_048_576 {
            return r#"{"error":"input exceeds 1 MB limit"}"#.to_string();
        }

        let doc = match parse::parse_with(xml, &self.resources()) {
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

        let doc = match parse::parse_tree_with(json, &self.resources()) {
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
    /// Render `xml` to PDF bytes with the fonts, images and styles files loaded on this engine: `render_pdf`, with an
    /// error that is not a JS value, which the command line and the tests can read. `encryption` is what the render
    /// call says about the encryption the document's `<output>` asks for.
    pub fn render_xml(&self, xml: &str, json_data: Option<&str>, encryption: &RenderEncryption) -> Result<Vec<u8>, String> {
        if xml.len() > 1_048_576 {
            return Err("input exceeds 1 MB limit".to_string());
        }

        let mut doc = parse::parse_with(xml, &self.resources())?;
        // Checked before the layout, so a missing or stray password fails without the work of a render.
        let config = encryption.resolve(doc.output.encryption.as_ref())?;

        if let Some(json) = json_data {
            data::apply(&mut doc, json)?;
        }

        // Confirm every image declared in <assets> has bytes in the registry.
        for (_alias, name) in &doc.images {
            if self.images.get(name).is_none() {
                return Err(format!("image '{name}' declared in <assets> but not loaded via loadImage()"));
            }
            if let Some(bytes) = self.images.get(name) {
                if let Some(reason) = pdf::image_format_error(bytes) {
                    return Err(format!("image '{name}': {reason}"));
                }
            }
        }

        let meta = pdf::build_image_meta(&self.images);
        let mut lp = doc.section_layouts();
        for page in &mut lp {
            layout::prefill_section_image_sizes(page, &meta);
        }

        layout::set_font_widths(self.font_widths.clone());

        let pages: Vec<render::RenderPage> =
            layout::layout_document(&lp);

        let status = license::check(&self.license_key, self.now_unix);
        // An unlicensed status draws the attribution line on every page.
        let bytes = pdf::render_pdf(
            &pages,
            &doc.fonts,
            &self.fonts,
            &self.images,
            &doc.meta,
            &doc.gradients,
            self.created_on.as_deref(),
            status.is_licensed(),
        )?;

        match &config {
            Some(cfg) => encrypt::encrypt_pdf(&bytes, cfg),
            None      => Ok(bytes),
        }
    }

    /// What the caller loaded for the `src` attributes of the document, for the parser.
    fn resources(&self) -> parse::Resources {
        parse::Resources { markdown_styles: self.markdown_styles.clone() }
    }

    /// Render XML to PDF bytes — native API used by the CLI and tests.
    ///
    /// Draws the attribution line when `license_key` is empty or invalid.
    /// No custom fonts or images are resolved; built-in fonts only.
    ///
    /// `now_unix` is the caller's clock, in seconds, used to check the key's expiry; `0` skips
    /// that check. The core compiles to wasm, which has no clock of its own, so the time can
    /// only come from the host — the same reason `LpdfEngine::set_now` exists. This used to
    /// hard-code `0`, which meant an expired key rendered without the attribution line for ever.
    ///
    /// `json_data` resolves the template's `data-*` attributes, as it does in `render_pdf`;
    /// `None` renders the template with its inline content.
    ///
    /// It takes no passwords, so a document that asks for encryption in its `<output>` renders only with
    /// `skip_encryption`, unencrypted, and is an error without it.
    pub fn render_xml_to_pdf(
        xml: &str,
        json_data: Option<&str>,
        license_key: &str,
        now_unix: i64,
        skip_encryption: bool,
    ) -> Result<Vec<u8>, String> {
        let mut doc = parse::parse(xml)?;
        RenderEncryption::skipping(skip_encryption).resolve(doc.output.encryption.as_ref())?;
        if let Some(json) = json_data {
            data::apply(&mut doc, json)?;
        }
        let lp = doc.section_layouts();
        let pages: Vec<render::RenderPage> =
            layout::layout_document(&lp);
        let status = license::check(license_key, now_unix);
        pdf::render_pdf(
            &pages,
            &doc.fonts,
            &pdf::FontRegistry::new(),
            &pdf::ImageRegistry::new(),
            &doc.meta,
            &doc.gradients,
            None,
            status.is_licensed(),
        )
    }

    /// Render XML to PDF bytes without WASM error types — used by tests.
    #[cfg(test)]
    pub(crate) fn render_xml_to_pdf_bytes(xml: &str) -> Result<Vec<u8>, String> {
        let mut doc = parse::parse(xml)?;
        let lp = doc.section_layouts();
        let pages: Vec<render::RenderPage> =
            layout::layout_document(&lp);
        // Render as unlicensed (with the attribution line) to match what the adapters
        // produce when no valid license key is supplied — keeps snapshot hashes
        // consistent between the Rust tests and the adapter test suites.
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(), &doc.meta, &doc.gradients, None, false)
    }

    /// Render XML to PDF bytes with `images` loaded, the way an SDK does: the sizes of the images are known before
    /// the layout. Used by tests.
    #[cfg(test)]
    pub(crate) fn render_xml_with_images_to_pdf_bytes(xml: &str, images: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
        let mut doc = parse::parse(xml)?;
        let mut registry = pdf::ImageRegistry::new();
        for (name, bytes) in images {
            registry.load(name, bytes.to_vec());
        }
        let meta = pdf::build_image_meta(&registry);
        let mut lp = doc.section_layouts();
        for page in &mut lp {
            layout::prefill_section_image_sizes(page, &meta);
        }
        let pages: Vec<render::RenderPage> = layout::layout_document(&lp);
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &registry, &doc.meta, &doc.gradients, None, false)
    }

    /// Render XML to PDF bytes as `render_xml` does, unlicensed and with built-in fonts only, encrypted as the
    /// document's `<output>` and `encryption` say. Used by tests.
    #[cfg(test)]
    pub(crate) fn render_xml_with_encryption(xml: &str, encryption: &RenderEncryption) -> Result<Vec<u8>, String> {
        LpdfEngine::new("").render_xml(xml, None, encryption)
    }

    fn render_doc(&self, mut doc: parse::Document) -> String {
        // Merge widths: engine-level (from set_font_metrics) + doc-level (from
        // tree JSON). Doc-level takes precedence — the adapter that built the
        // tree knows the exact bytes it loaded.
        // doc is owned so we take font_widths without cloning it.
        let mut merged = self.font_widths.clone();
        merged.extend(std::mem::take(&mut doc.font_widths));
        shared::render_doc_shared(doc, merged, &self.license_key, self.now_unix)
    }
}

// ── Standalone exports ────────────────────────────────────────────────────────

/// What this build of the engine makes of a license key, as JSON.
///
/// ```json
/// { "status": "licensed", "product": "lpdf", "tier": "professional",
///   "expires": "2027-09-19T00:00:00Z", "license": "L-7K3M9Q", "key": 3 }
/// ```
///
/// `status` is one of `licensed`, `no_key`, `expired`, `version_mismatch`, `wrong_product`,
/// `unknown_key`, `bad_signature` or `malformed`. The remaining fields appear only once the
/// signature verified — see [`license::report_json`].
///
/// A free function, not a method: asking what a key is should not require building an engine.
/// `now_unix` is the caller's clock in seconds, since wasm has none; `0` skips the expiry check.
///
/// The answer is this build's. An engine older than the key, or one built for another
/// environment, answers `unknown_key` — which is the useful part, not a caveat.
#[wasm_bindgen]
pub fn check_license(token: &str, now_unix: i64) -> String {
    license::report_json(token, now_unix)
}

/// What kind of file a `src` of a document names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// The `src` of an `<image>`, loaded under the image's `ref` or name with `load_image`.
    Image,
    /// The `src` of a `<markdown-styles>`, loaded under the `src` itself with `load_markdown_styles`.
    MarkdownStyles,
}

/// A file that a document names with a `src` and whoever renders it loads, as the engine reads no files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub kind: SourceKind,
    /// What it is loaded under: the `ref` or name of an image, the `src` of a styles file.
    pub key:  String,
    pub src:  String,
}

/// The files that the XML document `xml` names with a `src`: its images and the files of its Markdown styles.
/// A font is not among them: the command line draws in the built-in fonts.
pub fn source_files(xml: &str) -> Result<Vec<SourceFile>, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("XML parse error: {e}"))?;
    let mut files = Vec::new();
    for node in doc.descendants().filter(|n| n.is_element()) {
        let Some(src) = node.attribute("src") else { continue };
        match node.tag_name().name() {
            "image" => {
                let key = node.attribute("ref").or_else(|| node.attribute("name")).unwrap_or(src);
                files.push(SourceFile { kind: SourceKind::Image, key: key.to_string(), src: src.to_string() });
            }
            "markdown-styles" => {
                files.push(SourceFile { kind: SourceKind::MarkdownStyles, key: src.to_string(), src: src.to_string() });
            }
            _ => {}
        }
    }
    Ok(files)
}

/// Whether the XML document `xml` asks for encryption: an `<output>` of its `<document>` with `encrypt`. A document
/// that does not parse doesn't, and its render says why.
pub fn asks_for_encryption(xml: &str) -> bool {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return false };
    doc.descendants()
        .filter(|n| n.has_tag_name("document"))
        .flat_map(|document| document.children())
        .any(|n| n.has_tag_name("output") && n.attribute("encrypt").is_some())
}

/// The template that wraps a `.md` file when it is rendered with no template of its own: A4, a margin and the
/// page number. It reads the Markdown from the data `markdown`, the rest of which is the file's front matter.
#[wasm_bindgen]
pub fn markdown_template() -> String {
    markdown::default_template().to_string()
}

/// The rules a `<markdown>` with no styles uses, as the file to start a style of one's own from.
#[wasm_bindgen]
pub fn markdown_default_styles() -> String {
    markdown::default_styles().to_string()
}

/// The looks a `.md` file can start from besides the default one, as a JSON array of `name`, `label`, `description`
/// and `styles`: the file of styles, to write beside the `.md` file and edit.
#[wasm_bindgen]
pub fn markdown_style_presets() -> String {
    let presets: Vec<_> = markdown::style_presets().iter()
        .map(|p| serde_json::json!({ "name": p.name, "label": p.label, "description": p.description, "styles": p.styles }))
        .collect();
    serde_json::json!(presets).to_string()
}

/// The data a template gets for the `.md` file `markdown`, as JSON: the text as `markdown`, and the fields of its
/// front matter, each a string.
#[wasm_bindgen]
pub fn markdown_file_data(markdown: &str) -> String {
    markdown::file_data(markdown)
}

/// The relative paths of the images of the `.md` file `markdown`, as a JSON array. The caller declares them with
/// `markdown_declare_images` and loads each file, relative to the `.md`, under the name `markdown-image-N` where N
/// counts from 1 in the order of the array.
#[wasm_bindgen]
pub fn markdown_file_images(markdown: &str) -> String {
    serde_json::json!(markdown::file_images(markdown)).to_string()
}

/// The template `template` with an `<image>` declared for each path of the JSON array `srcs`, named as
/// `markdown_file_images` says.
#[wasm_bindgen]
pub fn markdown_declare_images(template: &str, srcs: &str) -> Result<String, JsValue> {
    let srcs: Vec<String> = serde_json::from_str(srcs).map_err(|e| JsValue::from_str(&format!("images: {e}")))?;
    markdown::declare_images(template, &srcs).map_err(|e| JsValue::from_str(&e))
}

/// Convert a JSON kit-tree (produced by `LpdfKit` in any adapter) to an lpdf
/// XML string.
///
/// Useful for debugging Kit-generated documents, saving them as `.xml` files,
/// or feeding them into the XML render path. The output is equivalent to
/// hand-authored XML and passes through `render_pdf` without modification.
#[wasm_bindgen]
pub fn kit_to_xml(json: &str) -> Result<String, JsValue> {
    kit_to_xml::kit_to_xml(json).map_err(|e| JsValue::from_str(&e))
}

/// Generate SDK source code from an Lpdf XML string.
///
/// `options_json` is a JSON object with:
/// - `target`: `"js"` (required)
/// - `indent`: `2` or `4` (optional, default `4`)
///
/// Returns the generated source code as a string.
#[wasm_bindgen]
pub fn codegen_wasm(xml: &str, options_json: &str) -> Result<String, JsValue> {
    #[derive(serde::Deserialize)]
    struct Opts {
        target: String,
        #[serde(default = "default_indent")]
        indent: u8,
    }
    fn default_indent() -> u8 { 4 }

    let opts: Opts = serde_json::from_str(options_json)
        .map_err(|e| JsValue::from_str(&format!("options parse error: {e}")))?;

    codegen::codegen(xml, &codegen::CodegenOptions { target: opts.target, indent: opts.indent })
        .map_err(|e| JsValue::from_str(&e))
}

/// Generate SDK source code from one or more bare Lpdf XML elements (a fragment).
///
/// Unlike `codegen_wasm`, this accepts a snippet without the `<lpdf>` wrapper and
/// returns just the node expression(s) — no imports, no engine setup, no boilerplate.
/// Ideal for documentation code examples where the same XML snippet should be shown
/// in multiple target languages.
///
/// `options_json` accepts the same shape as `codegen_wasm`:
/// - `target`: `"js"`, `"dotnet"`, `"php"`, or `"python"` (required)
/// - `indent`: `2` or `4` (optional, default `4`)
#[wasm_bindgen]
pub fn codegen_fragment_wasm(xml: &str, options_json: &str) -> Result<String, JsValue> {
    #[derive(serde::Deserialize)]
    struct Opts {
        target: String,
        #[serde(default = "default_indent")]
        indent: u8,
    }
    fn default_indent() -> u8 { 4 }

    let opts: Opts = serde_json::from_str(options_json)
        .map_err(|e| JsValue::from_str(&format!("options parse error: {e}")))?;

    codegen::codegen_fragment(xml, &codegen::CodegenOptions { target: opts.target, indent: opts.indent })
        .map_err(|e| JsValue::from_str(&e))
}

// ── Public bench API ─────────────────────────────────────────────────────────
// No cfg gate: these live in the rlib so bench binaries can link them.

/// Opaque pre-parsed document used by staged benchmarks.
pub struct BenchDoc(parse::Document);

/// Parse XML — no layout, no PDF write. Used by `parse_xml` benchmarks.
pub fn bench_parse(xml: &str) -> Result<BenchDoc, String> {
    parse::parse(xml).map(BenchDoc)
}

/// Parse a render-tree JSON — used by `parse_json` benchmarks.
pub fn bench_parse_tree(json: &str) -> Result<BenchDoc, String> {
    parse::parse_tree(json).map(BenchDoc)
}

/// Layout + PDF write on a pre-parsed doc. Used by `layout` benchmarks.
pub fn bench_render_doc(mut doc: BenchDoc) -> Result<Vec<u8>, String> {
    let lp = doc.0.section_layouts();
    let pages: Vec<render::RenderPage> =
        layout::layout_document(&lp);
    // Unlicensed, so benchmarks include the cost of the attribution line.
    pdf::render_pdf(
        &pages, &doc.0.fonts,
        &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(),
        &doc.0.meta, &doc.0.gradients, None, false,
    )
}

/// Laid-out pages, with the document whose fonts, metadata and gradients the PDF write reads.
pub struct BenchPages {
    pages: Vec<render::RenderPage>,
    doc:   parse::Document,
}

/// Layout on a pre-parsed doc, without the PDF write. With `bench_write_pdf`, it splits what
/// `bench_render_doc` measures in two. Used by the instruction-count benchmarks of `benches/counts`.
pub fn bench_layout_doc(mut doc: BenchDoc) -> BenchPages {
    let lp = doc.0.section_layouts();
    let pages: Vec<render::RenderPage> =
        layout::layout_document(&lp);
    BenchPages { pages, doc: doc.0 }
}

/// PDF write of laid-out pages. Used by the instruction-count benchmarks of `benches/counts`.
pub fn bench_write_pdf(pages: BenchPages) -> Result<Vec<u8>, String> {
    // Unlicensed, so benchmarks include the cost of the attribution line.
    pdf::render_pdf(
        &pages.pages, &pages.doc.fonts,
        &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(),
        &pages.doc.meta, &pages.doc.gradients, None, false,
    )
}

/// Embedded fonts, loaded as `LpdfEngine::load_font` loads them: the bytes the PDF write embeds, and the advances
/// layout measures their text with.
pub struct BenchFonts {
    registry: pdf::FontRegistry,
    widths:   HashMap<String, tokens::FontWidths>,
}

/// Load each font of `fonts`, a registry key and the bytes of a TTF or OTF file. Not measured: an engine loads its
/// fonts once, before it renders.
pub fn bench_fonts(fonts: &[(&str, &[u8])]) -> BenchFonts {
    let mut loaded = BenchFonts { registry: pdf::FontRegistry::new(), widths: HashMap::new() };
    for (name, bytes) in fonts {
        loaded.registry.register(name, bytes.to_vec());
        if let Some(widths) = shared::extract_font_widths(bytes) {
            loaded.widths.insert(name.to_string(), widths);
        }
    }
    loaded
}

/// The `name` and `src` of each embedded font `xml` declares, in order. A benchmark reads the files of the `src`
/// as an SDK does and loads them with `bench_fonts`. Used by the instruction-count benchmarks of `benches/counts`.
pub fn bench_font_srcs(xml: &str) -> Result<Vec<(String, String)>, String> {
    parse::font_srcs(xml)
}

/// `bench_layout_doc` with embedded fonts: their advances are set for layout as `render_pdf` sets them, so their
/// text is measured as in the SDKs. Used by the instruction-count benchmarks of `benches/counts`.
pub fn bench_layout_doc_with_fonts(mut doc: BenchDoc, fonts: &BenchFonts) -> BenchPages {
    let lp = doc.0.section_layouts();
    layout::set_font_widths(fonts.widths.clone());
    let pages: Vec<render::RenderPage> =
        layout::layout_document(&lp);
    BenchPages { pages, doc: doc.0 }
}

/// `bench_write_pdf` with embedded fonts, which it subsets and embeds. Used by the instruction-count benchmarks of
/// `benches/counts`.
pub fn bench_write_pdf_with_fonts(pages: BenchPages, fonts: &BenchFonts) -> Result<Vec<u8>, String> {
    // Unlicensed, so benchmarks include the cost of the attribution line.
    pdf::render_pdf(
        &pages.pages, &pages.doc.fonts,
        &fonts.registry, &pdf::ImageRegistry::new(),
        &pages.doc.meta, &pages.doc.gradients, None, false,
    )
}

/// `bench_render_doc` with embedded fonts: layout and the PDF write of `bench_layout_doc_with_fonts` and
/// `bench_write_pdf_with_fonts`.
pub fn bench_render_doc_with_fonts(doc: BenchDoc, fonts: &BenchFonts) -> Result<Vec<u8>, String> {
    bench_write_pdf_with_fonts(bench_layout_doc_with_fonts(doc, fonts), fonts)
}

/// `bench_render_xml` with embedded fonts: parse, then `bench_render_doc_with_fonts`.
pub fn bench_render_xml_with_fonts(xml: &str, fonts: &BenchFonts) -> Result<Vec<u8>, String> {
    bench_render_doc_with_fonts(bench_parse(xml)?, fonts)
}

/// Layout + PDF write on a pre-parsed doc, with its images loaded and sized as `render_pdf` sizes them: the
/// images of the flow and the `img-at` of the layers are fitted to the pictures' proportions. Used by the
/// `layers` and `media` benchmarks, whose documents hold images.
pub fn bench_render_doc_with_images(mut doc: BenchDoc, images: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    let mut registry = pdf::ImageRegistry::new();
    for (name, bytes) in images {
        registry.load(name, bytes.to_vec());
    }
    let meta = pdf::build_image_meta(&registry);
    let mut lp = doc.0.section_layouts();
    for page in &mut lp {
        layout::prefill_section_image_sizes(page, &meta);
    }
    let pages: Vec<render::RenderPage> =
        layout::layout_document(&lp);
    // Unlicensed, so benchmarks include the cost of the attribution line.
    pdf::render_pdf(
        &pages, &doc.0.fonts,
        &pdf::FontRegistry::new(), &registry,
        &doc.0.meta, &doc.0.gradients, None, false,
    )
}

/// Apply data binding to a pre-parsed doc. Used by staged `data/*` benchmarks.
pub fn bench_data_apply(doc: BenchDoc, json: &str) -> Result<BenchDoc, String> {
    let mut inner = doc.0;
    data::apply(&mut inner, json)?;
    Ok(BenchDoc(inner))
}

/// Full pipeline: parse + layout + PDF write. Used by `end_to_end` benchmarks.
pub fn bench_render_xml(xml: &str) -> Result<Vec<u8>, String> {
    let mut doc = parse::parse(xml)?;
    let lp = doc.section_layouts();
    let pages: Vec<render::RenderPage> =
        layout::layout_document(&lp);
    // Unlicensed, so benchmarks include the cost of the attribution line.
    pdf::render_pdf(
        &pages, &doc.fonts,
        &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(),
        &doc.meta, &doc.gradients, None, false,
    )
}

/// Full pipeline with one custom font loaded. Used by `fonts` benchmarks.
pub fn bench_render_xml_with_font(
    xml: &str,
    font_name: &str,
    font_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let mut doc = parse::parse(xml)?;
    let lp = doc.section_layouts();
    let pages: Vec<render::RenderPage> =
        layout::layout_document(&lp);
    let mut fonts = pdf::FontRegistry::new();
    fonts.register(font_name, font_bytes.to_vec());
    // Unlicensed, so benchmarks include the cost of the attribution line.
    pdf::render_pdf(
        &pages, &doc.fonts,
        &fonts, &pdf::ImageRegistry::new(),
        &doc.meta, &doc.gradients, None, false,
    )
}

/// Full pipeline with one image loaded. Used by `images` benchmarks.
pub fn bench_render_xml_with_image(
    xml: &str,
    image_name: &str,
    image_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let mut doc = parse::parse(xml)?;
    let lp = doc.section_layouts();
    let pages: Vec<render::RenderPage> =
        layout::layout_document(&lp);
    let mut images = pdf::ImageRegistry::new();
    images.load(image_name, image_bytes.to_vec());
    // Unlicensed, so benchmarks include the cost of the attribution line.
    pdf::render_pdf(
        &pages, &doc.fonts,
        &pdf::FontRegistry::new(), &images,
        &doc.meta, &doc.gradients, None, false,
    )
}

#[cfg(test)]
mod snapshot_tests;

#[cfg(test)]
mod attribute_effects;

#[cfg(test)]
mod embedded_font_layout_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn render_value(xml: &str) -> serde_json::Value {
        let engine = LpdfEngine::new("");
        serde_json::from_str(&engine.render(xml))
            .expect("render() returned invalid JSON")
    }

    fn minimal(body: &str) -> String {
        if body.is_empty() {
            r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt"/></section></document></lpdf>"#.to_string()
        } else {
            format!(
                r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
            )
        }
    }

    #[test]
    fn unlicensed_render_includes_watermark() {
        let engine = LpdfEngine::new("");
        let result: serde_json::Value =
            serde_json::from_str(&engine.render(&minimal(""))).unwrap();
        assert!(!result["watermark"].is_null());
        assert_eq!(result["watermark"]["type"], "lpdf:watermark");
        assert_eq!(result["watermark"]["text"], "Lpdf", "the words of the line the PDF draws");
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

    // Page content streams are compressed, but font and annotation dictionaries are
    // written as plain text, so their entries can be found by a byte search.
    fn pdf_contains(pdf: &[u8], needle: &[u8]) -> bool {
        pdf.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn unlicensed_pdf_embeds_the_attribution_face_and_links_to_lpdf() {
        let pdf = LpdfEngine::render_xml_to_pdf(&minimal(""), None, "", 0, false).unwrap();
        assert!(pdf_contains(&pdf, pdf::ATTRIBUTION_FONT_KEY.as_bytes()));
        assert!(pdf_contains(&pdf, b"https://lpdf.io"));
    }

    #[test]
    fn renders_the_data_into_the_pdf_like_literal_text() {
        let bound = minimal(r#"<text data-value="name">Fallback</text>"#);
        let literal = minimal("<text>Ada</text>");
        let with_data = LpdfEngine::render_xml_to_pdf(&bound, Some(r#"{"name":"Ada"}"#), "", 0, false).unwrap();
        let without_data = LpdfEngine::render_xml_to_pdf(&literal, None, "", 0, false).unwrap();
        assert_eq!(with_data, without_data);
    }

    #[test]
    fn renders_the_inline_content_when_no_data_is_given() {
        let bound = minimal(r#"<text data-value="name">Fallback</text>"#);
        let literal = minimal("<text>Fallback</text>");
        let without_data = LpdfEngine::render_xml_to_pdf(&bound, None, "", 0, false).unwrap();
        let expected = LpdfEngine::render_xml_to_pdf(&literal, None, "", 0, false).unwrap();
        assert_eq!(without_data, expected);
    }

    #[test]
    fn rejects_data_that_is_not_json_and_says_where() {
        let error = LpdfEngine::render_xml_to_pdf(&minimal(""), Some("{\n  \"name\": \n}"), "", 0, false).unwrap_err();
        assert!(error.contains("line 3"), "{error}");
    }

    #[test]
    fn licensed_pdf_carries_no_attribution() {
        // A signed key cannot be minted in a unit test, so render with `licensed` set directly.
        let mut doc = parse::parse(&minimal("")).unwrap();
        let lp = doc.section_layouts();
        let pages: Vec<render::RenderPage> =
            layout::layout_document(&lp);
        let pdf = pdf::render_pdf(
            &pages, &doc.fonts, &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(),
            &doc.meta, &doc.gradients, None, true,
        ).unwrap();
        assert!(!pdf_contains(&pdf, pdf::ATTRIBUTION_FONT_KEY.as_bytes()));
        assert!(!pdf_contains(&pdf, b"https://lpdf.io"));
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
            <document size="a4">
                <section background="surface">
                    <layout margin="28pt">
                        <column gap="m">
                            <frame background="primary" padding="m" radius="s">
                                <row gap="m" align="center" justify="between">
                                    <frame width="120pt" height="24pt" background="secondary" radius="xs" />
                                    <frame width="80pt" height="14pt" background="surface" radius="xs" />
                                </row>
                            </frame>
                            <divider color="#e0e0e0" thickness="xs" />
                            <grid cols="3" gap="m">
                                <frame padding="s" border="xs #e0e0e0" radius="xs">
                                    <column gap="s">
                                        <frame height="10pt" background="text-muted" radius="xs" />
                                    </column>
                                </frame>
                                <frame padding="s" border="xs #e0e0e0" radius="xs" />
                                <frame padding="s" border="xs #e0e0e0" radius="xs" />
                            </grid>
                        </column>
                    </layout>
                </section>
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
        let kids = node["nodes"].as_array().unwrap();
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

    #[test]
    fn bench_layout_and_write_make_the_pdf_of_bench_render_doc() {
        let xml = include_str!("../../../test/fixtures/bench_xs.xml");
        let whole = bench_render_doc(bench_parse(xml).unwrap()).unwrap();
        let split = bench_write_pdf(bench_layout_doc(bench_parse(xml).unwrap())).unwrap();
        assert_eq!(whole, split);
    }

    /// The fonts a fixture of `test/fixtures` declares, each read from the file its `src` names, relative to the
    /// fixture, as an SDK reads it. A file that isn't there fails the test, as it fails a preview.
    fn fixture_fonts(xml: &str) -> Vec<(String, Vec<u8>)> {
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures");
        bench_font_srcs(xml)
            .unwrap()
            .into_iter()
            .map(|(name, src)| {
                let path = fixtures.join(&src);
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|e| panic!("the font {name}, {}, can't be read: {e}", path.display()));
                (name, bytes)
            })
            .collect()
    }

    fn font_refs(files: &[(String, Vec<u8>)]) -> Vec<(&str, &[u8])> {
        files.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())).collect()
    }

    #[test]
    fn bench_font_srcs_names_each_font_with_a_src_in_order() {
        let xml = r#"<lpdf version="1">
            <assets>
                <font name="body" src="fonts/Body.ttf" />
                <font name="mono" core="Courier" />
                <image name="logo" src="logo.png" />
                <font name="heading" src="../Heading.otf" />
            </assets>
            <document size="a4"><section><layout><text>x</text></layout></section></document>
        </lpdf>"#;
        assert_eq!(
            bench_font_srcs(xml).unwrap(),
            vec![
                ("body".to_string(), "fonts/Body.ttf".to_string()),
                ("heading".to_string(), "../Heading.otf".to_string()),
            ],
        );
    }

    #[test]
    fn bench_font_srcs_rejects_a_font_without_a_name() {
        let xml = r#"<lpdf version="1"><assets><font src="Body.ttf" /></assets></lpdf>"#;
        assert!(bench_font_srcs(xml).unwrap_err().contains("missing required attribute 'name'"));
    }

    #[test]
    fn bench_fonts_lay_out_and_embed_as_an_engine_with_the_fonts_loaded() {
        let xml = include_str!("../../../test/fixtures/bench_fonts.xml");
        let files = fixture_fonts(xml);
        let fonts = bench_fonts(&font_refs(&files));
        // The widths are a thread-local, which any earlier layout on this thread leaves set.
        layout::set_font_widths(HashMap::new());
        let in_helvetica_widths = bench_write_pdf_with_fonts(bench_layout_doc(bench_parse(xml).unwrap()), &fonts).unwrap();
        let in_own_widths = bench_write_pdf_with_fonts(bench_layout_doc_with_fonts(bench_parse(xml).unwrap(), &fonts), &fonts).unwrap();
        assert_ne!(in_own_widths, in_helvetica_widths, "the fonts' widths change the layout");

        layout::set_font_widths(HashMap::new());
        let mut engine = LpdfEngine::new("");
        for (name, bytes) in &files {
            engine.load_font(name, bytes);
        }
        assert_eq!(in_own_widths, engine.render_xml(xml, None, &RenderEncryption::default()).unwrap());
    }

    #[test]
    fn bench_layout_and_write_with_fonts_make_the_pdf_of_bench_render_doc_with_fonts() {
        let xml = include_str!("../../../test/fixtures/bench_fonts.xml");
        let files = fixture_fonts(xml);
        let fonts = bench_fonts(&font_refs(&files));
        let whole = bench_render_doc_with_fonts(bench_parse(xml).unwrap(), &fonts).unwrap();
        let split = bench_write_pdf_with_fonts(bench_layout_doc_with_fonts(bench_parse(xml).unwrap(), &fonts), &fonts).unwrap();
        assert_eq!(whole, split);
    }

    // ── <output>: encryption ──────────────────────────────────────────────────

    /// A document with `<output {output}>` after its meta, or none when `output` is `None`.
    fn with_output(output: Option<&str>) -> String {
        let element = output.map(|attributes| format!("<output {attributes}/>")).unwrap_or_default();
        format!(
            r#"<lpdf version="1"><document size="a4"><meta title="T"/>{element}<section><layout margin="28pt"><text>Hello</text></layout></section></document></lpdf>"#
        )
    }

    fn owner_password(owner: &str) -> RenderEncryption {
        RenderEncryption { owner_password: Some(owner.to_string()), entropy: (0u8..48).collect(), ..Default::default() }
    }

    fn render_with(xml: &str, encryption: &RenderEncryption) -> Result<Vec<u8>, String> {
        LpdfEngine::render_xml_with_encryption(xml, encryption)
    }

    #[test]
    fn a_document_with_encrypt_is_encrypted_with_its_cipher_and_allow() {
        let permissions = encrypt::Permissions::from_allow("print copy").unwrap();
        let p = format!("/P {}", encrypt::compute_p(&permissions));
        for (cipher, dictionary) in [("aes-256", ["/V 5", "/R 6"]), ("rc4-128", ["/V 2", "/R 3"])] {
            let xml = with_output(Some(&format!(r#"encrypt="{cipher}" allow="print copy""#)));
            let text = String::from_utf8_lossy(&render_with(&xml, &owner_password("owner")).unwrap()).into_owned();
            for expected in dictionary.iter().chain([&p.as_str()]) {
                assert!(text.contains(expected), "{cipher}: missing {expected}");
            }
        }
    }

    #[test]
    fn without_allow_everything_is_allowed() {
        let text = String::from_utf8_lossy(&render_with(&with_output(Some(r#"encrypt="rc4-128""#)), &owner_password("owner")).unwrap())
            .into_owned();
        assert!(text.contains(&format!("/P {}", encrypt::compute_p(&encrypt::Permissions::default()))));
    }

    #[test]
    fn an_output_that_does_not_encrypt_leaves_the_file_as_it_was() {
        let without = render_with(&with_output(None), &RenderEncryption::default()).unwrap();
        assert_eq!(render_with(&with_output(Some("")), &RenderEncryption::default()).unwrap(), without);
        let skipped = render_with(&with_output(Some(r#"encrypt="aes-256" allow="print""#)), &RenderEncryption::skipping(true));
        assert_eq!(skipped.unwrap(), without, "skip_encryption gives the file the document gives without <output>");
    }

    #[test]
    fn the_render_errors_say_what_to_change() {
        let encrypted = with_output(Some(r#"encrypt="aes-256""#));
        let plain = with_output(None);
        let both = RenderEncryption { skip_encryption: true, ..owner_password("owner") };
        for (xml, encryption, expected) in [
            (&encrypted, RenderEncryption::default(), "no owner password"),
            (&plain, owner_password("owner"), "does not ask for encryption"),
            (&plain, both.clone(), "not both"),
            (&encrypted, both, "not both"),
        ] {
            let error = render_with(xml, &encryption).unwrap_err();
            assert!(error.contains(expected), "expected {expected:?}, got {error}");
        }
    }

    #[test]
    fn output_errors_name_the_element_and_what_to_change() {
        for (attributes, expected) in [
            (r#"allow="print""#, "add encrypt"),
            (r#"encrypt="aes-256" allow="print printing""#, "\"printing\""),
            (r#"encrypt="aes-128""#, "expected aes-256 or rc4-128"),
        ] {
            let error = render_with(&with_output(Some(attributes)), &owner_password("owner")).unwrap_err();
            assert!(error.starts_with("<output>") && error.contains(expected), "{attributes}: {error}");
        }
    }

    #[test]
    fn the_command_line_render_skips_encryption_only_when_asked() {
        let xml = with_output(Some(r#"encrypt="aes-256""#));
        let error = LpdfEngine::render_xml_to_pdf(&xml, None, "", 0, false).unwrap_err();
        assert!(error.contains("no owner password"), "{error}");
        let skipped = LpdfEngine::render_xml_to_pdf(&xml, None, "", 0, true).unwrap();
        assert_eq!(skipped, LpdfEngine::render_xml_to_pdf(&with_output(None), None, "", 0, false).unwrap());
    }

    #[test]
    fn the_json_path_reads_output() {
        let tree = |output: &str| format!(
            r#"{{"version":1,"type":"document","attrs":{{"output":{output}}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[]}}]}}]}}"#
        );
        let doc = parse::parse_tree(&tree(r#"{"encrypt":"rc4-128","allow":"print"}"#)).unwrap();
        assert_eq!(doc.output.encryption, Some(encrypt::OutputEncryption {
            algorithm:   encrypt::Algorithm::Rc4128,
            permissions: encrypt::Permissions::from_allow("print").unwrap(),
        }));
        let error = parse::parse_tree(&tree(r#"{"allow":"print"}"#)).unwrap_err();
        assert!(error.starts_with("document output:") && error.contains("add encrypt"), "{error}");
    }

    /// Writes the files `scripts/check-encryption.mjs` opens with qpdf, when `LPDF_ENCRYPT_OUT` names a folder:
    /// `showcase-encryption` rendered with the cipher and the permissions of its `<output>` and the passwords of
    /// the render, as a host renders it. Without the variable the test does nothing, so a plain `cargo test` needs
    /// no other tool.
    #[test]
    fn writes_samples_for_the_external_check() {
        let Some(dir) = std::env::var_os("LPDF_ENCRYPT_OUT") else { return };
        let dir = std::path::Path::new(&dir);
        let xml = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/fixtures/showcase-encryption.xml")
        ).expect("fixture not found");
        let rc4 = xml.replace(r#"encrypt="aes-256""#, r#"encrypt="rc4-128""#);
        let passwords = |user: Option<&str>, owner: &str| RenderEncryption {
            user_password: user.map(str::to_string),
            ..owner_password(owner)
        };

        let samples = [
            ("plain.pdf",                &xml, RenderEncryption::skipping(true)),
            ("aes256-user.pdf",          &xml, passwords(Some("user"), "owner")),
            ("aes256-open.pdf",          &xml, passwords(None, "owner")),
            ("aes256-same-password.pdf", &xml, passwords(Some("same"), "same")),
            ("rc4-user.pdf",             &rc4, passwords(Some("user"), "owner")),
        ];
        for (name, xml, encryption) in samples {
            let bytes = render_with(xml, &encryption).expect("the sample renders");
            std::fs::write(dir.join(name), bytes).expect("could not write a sample");
        }
    }
}
