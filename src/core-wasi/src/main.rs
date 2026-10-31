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
/// The bytes of the files a document carries, its `<attachments>`, come the same way, by the `ref` or, with no ref,
/// the name of each `<file>`:
/// ```json
/// { "files": { "invoice.xml": "<base64 bytes>", … } }
/// ```
///
/// A document asks for encryption in its `<output>`, which names the cipher and the permissions. The
/// passwords come with the request, never with the template, and AES-256 also needs `entropy`, at least
/// 32 random bytes from the host's secure generator, base64-encoded:
/// ```json
/// { "user_password": "", "owner_password": "…", "entropy": "<base64 random bytes>" }
/// ```
/// A render that cannot have the passwords, such as a preview, asks to skip the encryption by name with
/// `"skip_encryption": true` and gets the file unencrypted. Without it, a document that asks for
/// encryption and gets no owner password is an error, and so are passwords for a document that doesn't
/// ask for it.

// Include the pure-Rust core modules by path — no wasm-bindgen involved.
#[path = "../../core/src/tokens.rs"]
mod tokens;
#[path = "../../core/src/parse.rs"]
mod parse;
#[path = "../../core/src/data.rs"]
mod data;
#[path = "../../core/src/layout.rs"]
mod layout;
#[path = "../../core/src/lists.rs"]
mod lists;
#[path = "../../core/src/markdown.rs"]
mod markdown;
#[path = "../../core/src/render.rs"]
mod render;
#[path = "../../core/src/pdf.rs"]
mod pdf;
#[path = "../../core/src/pdfa.rs"]
mod pdfa;
#[path = "../../core/src/attachments.rs"]
mod attachments;
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
            let mut doc = match parse::parse_with(body, &build_resources(&req)) {
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
            let doc = match parse::parse_tree_with(body, &build_resources(&req)) {
                Ok(d)  => d,
                Err(e) => return serde_json::json!({ "error": e }).to_string(),
            };
            render_pdf_doc(doc, key, now_unix, &req)
        }
        "render_tree" => {
            if body.len() > 4_194_304 {
                return r#"{"error":"input exceeds 4 MB limit"}"#.to_string();
            }
            match parse::parse_tree_with(body, &build_resources(&req)) {
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
            match parse::parse_with(body, &build_resources(&req)) {
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

/// Decode the optional `markdown_styles` field of the request: the text of each file that a
/// `<markdown-styles src>` names, by `src`.
fn build_resources(req: &serde_json::Value) -> parse::Resources {
    let mut resources = parse::Resources::default();
    if let Some(files) = req.get("markdown_styles").and_then(|v| v.as_object()) {
        for (src, text) in files {
            if let Some(text) = text.as_str() {
                resources.markdown_styles.insert(src.clone(), text.to_string());
            }
        }
    }
    resources
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

/// Decode the optional `files` field of the request: the bytes of the files the document carries, by the `ref` or
/// name of each `<file>`.
fn build_file_registry(req: &serde_json::Value) -> attachments::FileRegistry {
    let mut registry = attachments::FileRegistry::new();
    if let Some(files) = req.get("files").and_then(|v| v.as_object()) {
        for (key, val) in files {
            if let Some(bytes) = val.as_str().and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64).ok()) {
                registry.load(key, bytes);
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
                map.insert(name.clone(), tokens::FontWidths::new(default, ascii, std::collections::HashMap::new()));
            }
        }
    }
    map
}

fn render_pdf_doc(mut doc: parse::Document, license_key: &str, now_unix: i64, req: &serde_json::Value) -> String {
    // Checked before the layout, so a missing or stray password fails without the work of a render.
    let encryption = match render_encryption(req).and_then(|e| e.resolve(doc.output.encryption.as_ref())) {
        Ok(config) => config,
        Err(e)     => return serde_json::json!({ "error": e }).to_string(),
    };
    let created_on = req["created_on"].as_str();
    if let Err(e) = pdfa::check_render(doc.output.pdfa, created_on) {
        return serde_json::json!({ "error": e }).to_string();
    }

    let file_registry = build_file_registry(req);
    let attached = std::mem::take(&mut doc.attachments);
    let files = match attachments::resolve(&attached, &file_registry) {
        Ok(files) => files,
        Err(e)    => return serde_json::json!({ "error": e }).to_string(),
    };

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

    // An unlicensed status draws the attribution line on every page.
    match pdf::render_pdf(&pages, &doc.fonts, &registry, &image_registry, &doc.meta, &doc.gradients, created_on, status.is_licensed(), doc.output.pdfa, &files) {
        Ok(bytes) => {
            let bytes = match &encryption {
                Some(cfg) => match encrypt::encrypt_pdf(&bytes, cfg) {
                    Ok(b)  => b,
                    Err(e) => return serde_json::json!({ "error": e }).to_string(),
                },
                None => bytes,
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
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(), &doc.meta, &doc.gradients, None, false, doc.output.pdfa, &[])
    }
}

/// Reads what the request says about encryption: the passwords, the entropy and `skip_encryption`. The
/// `encrypt` object of the engines before `<output>` is refused, so that a host that still sends it gets an
/// error and not a file it believes is encrypted.
fn render_encryption(req: &serde_json::Value) -> Result<encrypt::RenderEncryption, String> {
    if req.get("encrypt").is_some() {
        return Err("the request's encrypt object is gone: the document asks for encryption in its <output>, \
            and the passwords come as user_password and owner_password".to_string());
    }
    let entropy = match req["entropy"].as_str() {
        Some(text) => base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|e| format!("entropy is not valid base64: {e}"))?,
        None => Vec::new(),
    };
    Ok(encrypt::RenderEncryption {
        user_password:   req["user_password"].as_str().map(str::to_string),
        owner_password:  req["owner_password"].as_str().map(str::to_string),
        skip_encryption: req["skip_encryption"].as_bool().unwrap_or(false),
        entropy,
    })
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

    /// A one-page document whose `<output>` has `output`.
    fn with_output(output: &str) -> String {
        format!(r#"<lpdf version="1"><document><output {output}/><section><layout><text>Hello</text></layout></section></document></lpdf>"#)
    }

    /// What `render_pdf` answers for `xml` with the rest of the request in `fields`.
    fn render_request(xml: &str, fields: serde_json::Value) -> serde_json::Value {
        let mut req = serde_json::json!({ "method": "render_pdf", "input": xml });
        for (key, value) in fields.as_object().unwrap() {
            req[key] = value.clone();
        }
        serde_json::from_str(&dispatch(&req.to_string())).unwrap()
    }

    fn pdf_text(answer: &serde_json::Value) -> String {
        let bytes = base64::engine::general_purpose::STANDARD.decode(answer["pdf"].as_str().expect(&answer.to_string())).unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    const ENTROPY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4v";

    #[test]
    fn a_request_with_passwords_encrypts_as_the_output_asks() {
        let answer = render_request(
            &with_output(r#"encrypt="aes-256" allow="print""#),
            serde_json::json!({ "user_password": "", "owner_password": "owner", "entropy": ENTROPY }),
        );
        let text = pdf_text(&answer);
        assert!(text.contains("/V 5") && text.contains("/CFM /AESV3"), "AES-256 from <output>");

        let answer = render_request(&with_output(r#"encrypt="rc4-128""#), serde_json::json!({ "owner_password": "owner" }));
        assert!(pdf_text(&answer).contains("/V 2"), "RC4-128 from <output>, with no entropy");
    }

    #[test]
    fn skip_encryption_gives_the_file_unencrypted() {
        let answer = render_request(&with_output(r#"encrypt="aes-256""#), serde_json::json!({ "skip_encryption": true }));
        assert!(!pdf_text(&answer).contains("/Encrypt"));
    }

    #[test]
    fn the_request_errors_say_what_to_change() {
        let encrypted = with_output(r#"encrypt="aes-256""#);
        let plain = with_output("");
        for (xml, fields, expected) in [
            (&encrypted, serde_json::json!({}), "no owner password"),
            (&plain, serde_json::json!({ "owner_password": "owner" }), "does not ask for encryption"),
            (&encrypted, serde_json::json!({ "owner_password": "owner", "skip_encryption": true }), "not both"),
            (&plain, serde_json::json!({ "encrypt": { "owner_password": "owner" } }), "encrypt object is gone"),
            (&encrypted, serde_json::json!({ "owner_password": "owner", "entropy": "not base64!" }), "entropy is not valid base64"),
        ] {
            let answer = render_request(xml, fields);
            let error = answer["error"].as_str().unwrap_or_else(|| panic!("expected an error with {expected:?}"));
            assert!(error.contains(expected), "expected {expected:?}, got {error}");
        }
    }

    #[test]
    fn the_files_of_the_request_are_carried_under_the_names_of_their_file() {
        let xml = r#"<lpdf version="1"><document><attachments><file name="factur-x.xml" ref="invoice" mime="text/xml"/>
            </attachments><section><layout><text>Hello</text></layout></section></document></lpdf>"#;
        // "<invoice/>" in base64.
        let answer = render_request(xml, serde_json::json!({ "files": { "invoice": "PGludm9pY2UvPg==" } }));
        let text = pdf_text(&answer);
        assert!(text.contains("/Type /EmbeddedFile") && text.contains("/Subtype /text#2Fxml"), "{answer}");
        assert!(text.contains("/Size 10"), "the size of the file as loaded");

        let answer = render_request(xml, serde_json::json!({}));
        let error = answer["error"].as_str().expect("a file with no bytes is an error");
        assert!(error.contains("file 'factur-x.xml'") && error.contains("loadFile('invoice', bytes)"), "{error}");
    }

    #[test]
    fn the_tree_path_reads_attachments_too() {
        let tree = r#"{"version":1,"type":"document","attrs":{"attachments":[{"name":"a.txt","relationship":"supplement"}]},
            "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[{"type":"text","nodes":["x"]}]}]}]}"#;
        let req = serde_json::json!({ "method": "render_tree_pdf", "input": tree, "files": { "a.txt": "QQ==" } });
        let answer: serde_json::Value = serde_json::from_str(&dispatch(&req.to_string())).unwrap();
        assert!(pdf_text(&answer).contains("/AFRelationship /Supplement"), "{answer}");
    }

    #[test]
    fn the_created_on_of_the_request_is_written_as_a_pdf_date() {
        for (created_on, expected) in [
            ("2026-10-31T09:30:00.000Z", "/CreationDate (D:20261031093000Z)"),
            ("2026-10-31T09:30:00+01:00", "/CreationDate (D:20261031093000+01'00)"),
        ] {
            let answer = render_request(&with_output(""), serde_json::json!({ "created_on": created_on }));
            assert!(pdf_text(&answer).contains(expected), "{created_on}: {answer}");
        }
        assert!(!pdf_text(&render_request(&with_output(""), serde_json::json!({}))).contains("/CreationDate"));
    }

    #[test]
    fn a_created_on_that_is_not_a_date_is_an_error_before_the_layout_on_both_methods() {
        // The file is not in the request, so an error that names the date came before the files were looked for.
        let xml = r#"<lpdf version="1"><document><attachments><file name="a.txt" relationship="supplement"/></attachments>
            <section><layout><text>x</text></layout></section></document></lpdf>"#;
        let tree = r#"{"version":1,"type":"document","attrs":{"attachments":[{"name":"a.txt","relationship":"supplement"}]},
            "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[]}]}]}"#;
        for (method, input) in [("render_pdf", xml), ("render_tree_pdf", tree)] {
            let req = serde_json::json!({ "method": method, "input": input, "created_on": "31.10.2026" });
            let answer: serde_json::Value = serde_json::from_str(&dispatch(&req.to_string())).unwrap();
            let error = answer["error"].as_str().unwrap_or_else(|| panic!("{method}: {answer}"));
            assert!(error.contains("created_on '31.10.2026'") && error.contains("2026-10-31T09:30:00Z"), "{method}: {error}");
        }
    }

    #[test]
    fn the_tree_path_reads_output_too() {
        let tree = r#"{"version":1,"type":"document","attrs":{"output":{"encrypt":"rc4-128"}},"nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[]}]}]}"#;
        let req = serde_json::json!({ "method": "render_tree_pdf", "input": tree });
        let answer: serde_json::Value = serde_json::from_str(&dispatch(&req.to_string())).unwrap();
        assert!(answer["error"].as_str().unwrap().contains("no owner password"), "{answer}");
    }
}
