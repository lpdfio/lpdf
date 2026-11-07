//! The engine's requests, in JSON: the WASI binary (`main.rs`) reads one on stdin and writes its answer to stdout, and
//! the native library (`src/core-ffi`) takes one in `lpdf_invoke` and returns its answer. Both answer through `answer`.
//!
//! ```json
//! { "method": "render" | "render_tree" | "render_pdf" | "render_tree_pdf" | "stamp" | "check_license",
//!   "key": "…", "input": "…xml or json…", "now": 1790812800 }
//! ```
//! - `render` / `render_tree` return the RenderTree JSON string directly.
//! - `render_pdf` / `render_tree_pdf` return `{ "pdf": "<base64>" }` on success
//!   or `{ "error": "…" }` on failure. The WASI binary writes the PDF's answer to stdout as it encodes it, and the
//!   native library returns it whole (`Answer`).
//! - `stamp` draws PNG images over the pages of an existing PDF and returns `{ "pdf": "<base64>" }`, the input with
//!   an incremental update, or `{ "error": "…" }`. It takes no `input`, but `pdf`, the file as base64, `images`, the
//!   PNGs by name as base64, and `stamps`, a list of stamps, each naming its PNG in `image`:
//!   `{ "method": "stamp", "pdf": "…", "images": { "seal": "…" }, "stamps": [{ "image": "seal", "page": "last", … }] }`.
//! - `check_license` reports what `key` is — `{ "status": "licensed", … }` — and takes no
//!   `input`. `now` is the host's clock in seconds; without it expiry is not checked.
//!
//! For `render_pdf` / `render_tree_pdf`, custom font bytes can be supplied:
//! ```json
//! { "fonts": { "Inter": "<base64 TTF bytes>", … } }
//! ```
//!
//! The bytes of the files a document carries, its `<attachments>`, come the same way, by the `ref` or, with no ref,
//! the name of each `<file>`:
//! ```json
//! { "files": { "invoice.xml": "<base64 bytes>", … } }
//! ```
//!
//! And so do the ICC colour profiles of its `<assets>`, such as the press profile of a PDF/X document, by the `ref`
//! or the name of each `<profile>`:
//! ```json
//! { "profiles": { "press": "<base64 ICC bytes>", … } }
//! ```
//!
//! A document asks for encryption in its `<output>`, which names the cipher and the permissions. The
//! passwords come with the request, never with the template, and AES-256 also needs `entropy`, at least
//! 32 random bytes from the host's secure generator, base64-encoded:
//! ```json
//! { "user_password": "", "owner_password": "…", "entropy": "<base64 random bytes>" }
//! ```
//! A render that cannot have the passwords, such as a preview, asks to skip the encryption by name with
//! `"skip_encryption": true` and gets the file unencrypted. Without it, a document that asks for
//! encryption and gets no owner password is an error, and so are passwords for a document that doesn't
//! ask for it.
//!
//! A render may allocate `max_memory_mb` megabytes, 1024 when the request leaves it out, from 1 to 4096; one
//! that needs more is an error that names the cap (`memory.rs`). What the request parses into counts, its document,
//! data and files, from when it is parsed:
//! ```json
//! { "max_memory_mb": 512 }
//! ```
//!
//! The native library stops a render that runs past `timeout`, in seconds, with an error that names it and
//! `"timed_out": true`; 0 or none sets no limit (`deadline.rs`). The WASI binary keeps no deadline: its hosts stop a
//! render themselves.
//! ```json
//! { "timeout": 5 }
//! ```
//!
//! A render that panics, which is a bug, answers an error with `"crashed": true`, in the native library. The WASI
//! binary aborts on a panic, and its host reports the trap.

use std::any::Any;
use std::io::{self, Write};
use std::panic::AssertUnwindSafe;

use base64::Engine as _;

use crate::{
    attachments, data, deadline, encrypt, kit_to_xml, layout, license, memory, parse, pdf, pdfa, pdfua, pdfx, references,
    render, shared, stamp, tokens,
};

/// The JSON before the base64 of a PDF's answer.
const PDF_OPEN: &[u8] = br#"{"pdf":""#;

/// The JSON after the base64 of a PDF's answer.
const PDF_CLOSE: &[u8] = br#""}"#;

/// What a request is answered: JSON text, or the PDF of a render or a stamp, whose answer is `{"pdf":"<base64>"}`.
/// The PDF's answer is written as it is encoded (`write_to`), so the render never holds its base64 whole.
pub(crate) enum Answer {
    /// The answer's JSON: an error, a license report, a render tree, the XML of a kit.
    Json(String),
    /// The PDF of a `render_pdf`, a `render_tree_pdf` or a `stamp`.
    Pdf(Vec<u8>),
}

impl Answer {
    /// The answer `{ "error": "<message>" }`.
    fn error(message: &str) -> Answer {
        Answer::Json(serde_json::json!({ "error": message }).to_string())
    }

    /// The length of the answer's JSON, the bytes `write_to` writes: `usize::MAX` for a PDF whose base64 is longer than
    /// a `usize` counts, which no build holds.
    pub(crate) fn json_len(&self) -> usize {
        match self {
            Answer::Json(text) => text.len(),
            Answer::Pdf(pdf) => base64::encoded_len(pdf.len(), true)
                .and_then(|base64| base64.checked_add(PDF_OPEN.len() + PDF_CLOSE.len()))
                .unwrap_or(usize::MAX),
        }
    }

    /// Writes the answer's JSON to `out`. A PDF goes through the base64 encoder a block at a time, so `out` gets the
    /// bytes serde_json writes for `{ "pdf": "<base64>" }`, since base64 has no character JSON escapes, while only a
    /// block of the base64 is held.
    pub(crate) fn write_to(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            Answer::Json(text) => out.write_all(text.as_bytes()),
            Answer::Pdf(pdf) => {
                out.write_all(PDF_OPEN)?;
                {
                    let mut base64 =
                        base64::write::EncoderWriter::new(&mut *out, &base64::engine::general_purpose::STANDARD);
                    base64.write_all(pdf)?;
                    base64.finish()?;
                }
                out.write_all(PDF_CLOSE)
            }
        }
    }
}

impl From<Result<Vec<u8>, String>> for Answer {
    /// The answer of a render or a stamp: its PDF, or its error.
    fn from(pdf: Result<Vec<u8>, String>) -> Answer {
        match pdf {
            Ok(pdf) => Answer::Pdf(pdf),
            Err(e) => Answer::error(&e),
        }
    }
}

/// What `request` is answered: what `dispatch` answers, or the error of a render that ran past its timeout or panicked.
pub(crate) fn answer(request: String) -> Answer {
    answer_with(|| dispatch(request))
}

/// What `run` answers, or the error of a render that unwound out of it.
fn answer_with(run: impl FnOnce() -> Answer) -> Answer {
    // What a render holds is freed as it unwinds, and what it keeps for the thread is reset by its guards or when the
    // next render starts, so nothing of it is seen again.
    std::panic::catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(unwound)
}

/// The answer of a render that unwound with `payload`: past its deadline (`deadline::TimedOut`), or a panic.
#[cfg(not(target_family = "wasm"))]
fn unwound(payload: Box<dyn Any + Send>) -> Answer {
    let json = match payload.downcast::<deadline::TimedOut>() {
        Ok(timed_out) => serde_json::json!({ "error": timed_out.error(), "timed_out": true }),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("no message");
            serde_json::json!({ "error": format!("the engine panicked: {message}"), "crashed": true })
        }
    };
    Answer::Json(json.to_string())
}

/// The WebAssembly builds abort on a panic, so nothing unwinds to here.
#[cfg(target_family = "wasm")]
fn unwound(payload: Box<dyn Any + Send>) -> Answer {
    std::panic::resume_unwind(payload)
}

/// What `request` asks, answered.
fn dispatch(request: String) -> Answer {
    // The render's cap counts from here, so the data the request carries counts toward it, as it does in the JS build,
    // which parses it in the render.
    let before = memory::mark();
    let mut req: serde_json::Value = match serde_json::from_str(&request) {
        Ok(v) => v,
        Err(e) => return Answer::error(&format!("request parse error: {e}")),
    };
    // The text holds the input a second time, beside the parsed request, and is no longer needed.
    drop(request);

    let method   = req["method"].as_str().unwrap_or("render");
    let key      = req["key"]   .as_str().unwrap_or("");
    let now_unix = req["now"]   .as_i64().unwrap_or(0);

    // Answered before `input` is required: this method asks about the key itself, and there is
    // no document to render.
    if method == "check_license" {
        return Answer::Json(license::report_json(key, now_unix));
    }

    if method == "stamp" {
        let licensed = license::check(key, now_unix).is_licensed();
        return stamp_request(&mut req, licensed, before).into();
    }

    // Taken out of the request rather than copied, so the render holds one copy of each.
    let method   = method.to_string();
    let key      = key.to_string();
    let body = match req.get_mut("input").map(serde_json::Value::take) {
        Some(serde_json::Value::String(s)) => s,
        _ => return Answer::error("request missing 'input' field"),
    };
    let data = req.get_mut("data").map(serde_json::Value::take).filter(|v| !v.is_null());

    if method == "kit_to_xml" {
        return match kit_to_xml::kit_to_xml(&body) {
            Ok(xml) => Answer::Json(serde_json::json!({ "xml": xml }).to_string()),
            Err(e)  => Answer::error(&e),
        };
    }

    let cap = match render_cap(&req) {
        Ok(cap) => cap,
        Err(e)  => return Answer::error(&e),
    };
    let _cap = memory::start_from(cap, before);
    let timeout = match render_timeout(&req) {
        Ok(timeout) => timeout,
        Err(e)      => return Answer::error(&e),
    };
    let _deadline = deadline::start(timeout);
    let resources = build_resources(&req);
    let parsed = match method.as_str() {
        "render_tree_pdf" | "render_tree" => parse::parse_tree_with(&body, &resources),
        _ => parse::parse_with(&body, &resources),
    };
    // The document holds what it needs of the input.
    drop(body);
    let mut doc = match parsed {
        Ok(d)  => d,
        Err(e) => return Answer::error(&e),
    };

    match method.as_str() {
        "render_pdf" => {
            // Optional data binding: the request may carry a "data" JSON object, bound as it was parsed.
            if let Some(data) = data {
                if let Err(e) = data::apply(&mut doc, data) {
                    return Answer::error(&e);
                }
            }
            render_pdf_doc(doc, &key, now_unix, &mut req).into()
        }
        "render_tree_pdf" => render_pdf_doc(doc, &key, now_unix, &mut req).into(),
        _ => Answer::Json(render_doc(doc, &key, now_unix)),
    }
}

/// A `stamp` request: the PDF in `pdf`, as base64, and the `stamps`, each naming a PNG of the request's `images`;
/// `licensed` when the request's key is, else each stamped page gets the attribution line.
/// Returns the file with the stamps added as an incremental update, or the error.
fn stamp_request(req: &mut serde_json::Value, licensed: bool, before: i64) -> Result<Vec<u8>, String> {
    let cap = render_cap(req)?;
    let _cap = memory::start_from(cap, before);
    let pdf = match req.get_mut("pdf").map(serde_json::Value::take) {
        Some(serde_json::Value::String(b64)) => base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("the request's 'pdf' is not base64: {e}"))?,
        _ => return Err("request missing 'pdf' field".to_string()),
    };
    let stamps = stamp::parse_stamps(&req["stamps"])?;
    stamp::stamp(&pdf, &stamps, &build_image_registry(req), licensed)
}

/// The memory cap of the request's render: its `max_memory_mb`, or the default.
fn render_cap(req: &serde_json::Value) -> Result<u32, String> {
    match req.get("max_memory_mb").filter(|v| !v.is_null()) {
        None => memory::cap_mb(None),
        Some(mb) => match mb.as_f64() {
            Some(mb) => memory::cap_mb(Some(mb)),
            None => Err(memory::cap_error(&mb.to_string())),
        },
    }
}

/// The timeout of the request's render, in seconds: its `timeout`, or 0, no limit.
fn render_timeout(req: &serde_json::Value) -> Result<f64, String> {
    match req.get("timeout").filter(|v| !v.is_null()) {
        None => deadline::seconds(None),
        Some(seconds) => match seconds.as_f64() {
            Some(seconds) => deadline::seconds(Some(seconds)),
            None => Err(deadline::seconds_error(&seconds.to_string())),
        },
    }
}

/// Decodes each asset of the request's `field`, base64 by name, and hands it to `load`. The assets are taken out of the
/// request, and each one's base64 is freed once it is decoded, so the render holds an asset once, as its bytes. One
/// that is not base64 is left out.
fn take_assets(req: &mut serde_json::Value, field: &str, mut load: impl FnMut(&str, Vec<u8>)) {
    if let Some(serde_json::Value::Object(assets)) = req.get_mut(field).map(serde_json::Value::take) {
        for (name, asset) in assets {
            if let serde_json::Value::String(b64) = asset {
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                    load(&name, bytes);
                }
            }
        }
    }
}

/// Decode the optional `fonts` field from the request envelope into a `FontRegistry`, taking it out of the request.
fn build_registry(req: &mut serde_json::Value) -> pdf::FontRegistry {
    let mut registry = pdf::FontRegistry::new();
    take_assets(req, "fonts", |name, bytes| registry.register(name, bytes));
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

/// Decode the optional `images` field from the request envelope into an `ImageRegistry`, taking it out of the request.
fn build_image_registry(req: &mut serde_json::Value) -> pdf::ImageRegistry {
    let mut registry = pdf::ImageRegistry::new();
    take_assets(req, "images", |name, bytes| registry.load(name, bytes));
    registry
}

/// Decode the optional `files` field of the request: the bytes of the files the document carries, by the `ref` or
/// name of each `<file>`, taken out of the request.
fn build_file_registry(req: &mut serde_json::Value) -> attachments::FileRegistry {
    let mut registry = attachments::FileRegistry::new();
    take_assets(req, "files", |key, bytes| registry.load(key, bytes));
    registry
}

/// Decode the optional `profiles` field of the request: the bytes of the ICC colour profiles of the document's
/// `<assets>`, by the `ref` or name of each `<profile>`, taken out of the request.
fn build_profile_registry(req: &mut serde_json::Value) -> pdfx::ProfileRegistry {
    let mut registry = pdfx::ProfileRegistry::new();
    take_assets(req, "profiles", |key, bytes| registry.load(key, bytes));
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

/// The PDF of `doc`, rendered as the request asks, or the error that stops it. The request's fonts, images, files and
/// profiles are taken out of it as their registries are built.
fn render_pdf_doc(
    mut doc: parse::Document, license_key: &str, now_unix: i64, req: &mut serde_json::Value,
) -> Result<Vec<u8>, String> {
    // Checked before the layout, so a missing or stray password fails without the work of a render.
    let encryption = render_encryption(req).and_then(|e| e.resolve(doc.output.encryption.as_ref()))?;
    let created_on = req["created_on"].as_str().map(str::to_owned);
    let created_on = created_on.as_deref();
    pdfa::check_render(&doc.output, created_on)?;

    let profile_registry = build_profile_registry(req);
    let press = pdfx::resolve(doc.output.pdfx.as_ref(), &doc.profiles, &profile_registry, &doc.meta)?;

    let file_registry = build_file_registry(req);
    let attached = std::mem::take(&mut doc.attachments);
    let files = attachments::resolve(&attached, &file_registry, &doc.output)?;

    let registry       = build_registry(req);
    let image_registry = build_image_registry(req);

    // Confirm every image declared in <assets> has bytes in the registry.
    for (_alias, name) in &doc.images {
        if image_registry.get(name).is_none() {
            return Err(format!("image '{name}' declared in <assets> but not loaded via loadImage()"));
        }
        if let Some(bytes) = image_registry.get(name) {
            if let Some(reason) = pdf::image_format_error(bytes) {
                return Err(format!("image '{name}': {reason}"));
            }
        }
    }

    let meta = pdf::build_image_meta(&image_registry);
    let mut lp = doc.section_layouts();
    pdfua::check(doc.output.pdfua, &doc.meta, &lp).and_then(|()| references::check(&lp))?;
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
    memory::check()?;
    let pages: Vec<render::RenderPage> = layout::layout_document(&lp)?;
    memory::check()?;

    let status = license::check(license_key, now_unix);

    // An unlicensed status draws the attribution line on every page.
    let bytes = pdf::render_pdf(&pages, &doc.fonts, &registry, &image_registry, &doc.meta, &doc.gradients, created_on, status.is_licensed(), &doc.output, &files, press.as_ref())?;
    let bytes = shared::finish_file(bytes, encryption.as_ref(), doc.output.linearize)?;
    memory::check()?;
    Ok(bytes)
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
    use crate::nesting;

    /// A PNG of 1 × 2 pixels: an image twice as high as it is wide.
    const TALL_PNG: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAACCAIAAAAW4yFwAAAADklEQVR4nGMwSJjAAMQACEgCQStAOpQAAAAASUVORK5CYII=";

    /// What the WASI binary writes to stdout for `answer`.
    fn stdout_of(answer: &Answer) -> Vec<u8> {
        let mut stdout = Vec::new();
        answer.write_to(&mut stdout).unwrap();
        stdout
    }

    /// The JSON of `answer`, as the WASI binary writes it to stdout.
    fn json(answer: Answer) -> serde_json::Value {
        serde_json::from_slice(&stdout_of(&answer)).unwrap()
    }

    /// The image matrices (`cm` lines) in the content streams of a PDF.
    fn image_matrices(pdf: &[u8]) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = pdf;
        while let Some(start) = rest.windows(7).position(|w| w == b"stream\n") {
            rest = &rest[start + 7..];
            let Some(end) = rest.windows(10).position(|w| w == b"\nendstream") else { break };
            if let Ok(content) = miniz_oxide::inflate::decompress_to_vec_zlib(&rest[..end]) {
                let text = String::from_utf8_lossy(&content);
                found.extend(text.lines().filter(|l| l.ends_with(" cm")).map(str::to_string));
            }
            // Past `endstream`, whose end is `stream` too.
            rest = &rest[end + b"\nendstream".len()..];
        }
        found
    }

    #[test]
    fn an_img_at_in_a_layer_is_sized_by_the_proportions_of_its_image() {
        let xml = r#"<lpdf version="1"><assets><image name="logo"/></assets><document size="a4"><section>
            <layer><img-at name="logo" x="10pt" y="10pt" width="50pt"/></layer>
            <layer><img-at name="logo" anchor="top-right" width="100pt" height="100pt"/></layer>
        </section></document></lpdf>"#;
        let mut req = serde_json::json!({ "images": { "logo": TALL_PNG } });
        let pdf = render_pdf_doc(parse::parse(xml).unwrap(), "", 0, &mut req).unwrap();

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
        json(dispatch(req.to_string()))
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
    fn a_request_for_a_document_that_asks_for_linearize_gets_it_after_the_encryption() {
        let plain = pdf_text(&render_request(&with_output(r#"linearize="true""#), serde_json::json!({})));
        assert!(plain.contains("<< /Linearized 1 "), "linearized from <output>");

        let answer = render_request(
            &with_output(r#"encrypt="aes-256" linearize="true""#),
            serde_json::json!({ "owner_password": "owner", "entropy": ENTROPY }),
        );
        let text = pdf_text(&answer);
        assert!(text.contains("<< /Linearized 1 ") && text.contains("/CFM /AESV3"), "encrypted, then linearized");
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
        let xml = r#"<lpdf version="1"><document><attachments><file name="invoice.xml" ref="invoice" mime="text/xml"/>
            </attachments><section><layout><text>Hello</text></layout></section></document></lpdf>"#;
        // "<invoice/>" in base64.
        let answer = render_request(xml, serde_json::json!({ "files": { "invoice": "PGludm9pY2UvPg==" } }));
        let text = pdf_text(&answer);
        assert!(text.contains("/Type /EmbeddedFile") && text.contains("/Subtype /text#2Fxml"), "{answer}");
        assert!(text.contains("/Size 10"), "the size of the file as loaded");

        let answer = render_request(xml, serde_json::json!({}));
        let error = answer["error"].as_str().expect("a file with no bytes is an error");
        assert!(error.contains("file 'invoice.xml'") && error.contains("loadFile('invoice', bytes)"), "{error}");
    }

    #[test]
    fn the_profiles_of_the_request_are_the_press_profiles_of_a_pdfx_document() {
        let xml = r#"<lpdf version="1"><assets><font name="body" src="body.ttf"/><profile name="press" ref="fogra39"/></assets>
            <document font="body"><meta title="Flyer"/><output pdfx="4" profile="press"/><section><layout><text>Lpdf</text></layout></section></document></lpdf>"#;
        let font = base64::engine::general_purpose::STANDARD.encode(pdf::ATTRIBUTION_FONT);
        let profile = base64::engine::general_purpose::STANDARD
            .encode(include_bytes!("../../../test/assets/profiles/FOGRA39L_coated.icc"));
        let fields = serde_json::json!({ "fonts": { "body": font }, "created_on": "2026-10-31T09:30:00Z" });
        let mut with_profile = fields.clone();
        with_profile["profiles"] = serde_json::json!({ "fogra39": profile });
        let text = pdf_text(&render_request(xml, with_profile));
        assert!(text.contains("/S /GTS_PDFX") && text.contains("/Info (FOGRA39L Coated)") && text.contains("/TrimBox"), "{text}");

        let answer = render_request(xml, fields);
        let error = answer["error"].as_str().expect("a profile with no bytes is an error");
        assert!(error.contains("profile 'press'") && error.contains("loadProfile('fogra39', bytes)"), "{error}");
    }

    #[test]
    fn a_pdfa_3_request_with_factur_x_xml_is_a_factur_x_invoice() {
        let xml = |level: &str| format!(
            r#"<lpdf version="1"><assets><font name="body" src="body.ttf"/></assets><document font="body"><output pdfa="{level}"/>
            <attachments><file name="factur-x.xml"/></attachments><section><layout><text>Lpdf</text></layout></section></document></lpdf>"#
        );
        let font = base64::engine::general_purpose::STANDARD.encode(pdf::ATTRIBUTION_FONT);
        let invoice = base64::engine::general_purpose::STANDARD
            .encode(include_bytes!("../../../test/assets/files/facturx/basic-wl.xml"));
        let fields = serde_json::json!({
            "fonts": { "body": font }, "files": { "factur-x.xml": invoice }, "created_on": "2026-10-31T09:30:00Z",
        });
        for (level, conformance) in [("3b", "B"), ("3u", "U")] {
            let text = pdf_text(&render_request(&xml(level), fields.clone()));
            assert!(text.contains("<pdfaid:part>3</pdfaid:part>"), "{level}: {text}");
            assert!(text.contains(&format!("<pdfaid:conformance>{conformance}</pdfaid:conformance>")), "{level}");
            assert!(text.contains("<fx:ConformanceLevel>BASIC WL</fx:ConformanceLevel>") && text.contains("/AFRelationship /Data"), "{level}");
        }
        let answer = render_request(&xml("2b"), fields);
        let error = answer["error"].as_str().expect("an invoice in PDF/A-2 is an error");
        assert!(error.contains("pdfa=\"3a\", pdfa=\"3b\" or pdfa=\"3u\""), "{error}");
    }

    #[test]
    fn a_level_a_request_is_a_tagged_pdfa_file_on_both_paths() {
        let font = base64::engine::general_purpose::STANDARD.encode(pdf::ATTRIBUTION_FONT);
        let fields = serde_json::json!({ "fonts": { "body": font }, "created_on": "2026-10-31T09:30:00Z" });
        let xml = |level: &str| format!(
            r#"<lpdf version="1"><assets><font name="body" src="body.ttf"/></assets><document font="body"><output pdfa="{level}"/>
            <section><layout><text>Lpdf</text></layout></section></document></lpdf>"#
        );
        let tree = |level: &str| format!(
            r#"{{"version":1,"type":"document","attrs":{{"font":"body","assets":{{"fonts":[{{"name":"body","src":"body.ttf"}}]}},"output":{{"pdfa":"{level}"}}}},
            "nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[{{"type":"text","nodes":["Lpdf"]}}]}}]}}]}}"#
        );
        for (level, part) in [("2a", "2"), ("3a", "3")] {
            let from_tree = {
                let mut req = fields.clone();
                req["method"] = "render_tree_pdf".into();
                req["input"] = tree(level).into();
                json(dispatch(req.to_string()))
            };
            for answer in [render_request(&xml(level), fields.clone()), from_tree] {
                let text = pdf_text(&answer);
                assert!(text.contains(&format!("<pdfaid:part>{part}</pdfaid:part>")), "{level}: {text}");
                assert!(text.contains("<pdfaid:conformance>A</pdfaid:conformance>") && text.contains("/StructTreeRoot"), "{level}");
                assert!(!text.contains("pdfuaid"), "{level}");
            }
        }
        let answer = render_request(&xml("1a"), fields);
        let error = answer["error"].as_str().expect("a level the engine doesn't write is an error");
        assert!(error.contains("use 2a, 2b, 2u, 3a, 3b or 3u"), "{error}");
    }

    #[test]
    fn the_tree_path_reads_attachments_too() {
        let tree = r#"{"version":1,"type":"document","attrs":{"attachments":[{"name":"a.txt","relationship":"supplement"}]},
            "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[{"type":"text","nodes":["x"]}]}]}]}"#;
        let req = serde_json::json!({ "method": "render_tree_pdf", "input": tree, "files": { "a.txt": "QQ==" } });
        let answer = json(dispatch(req.to_string()));
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
            let answer = json(dispatch(req.to_string()));
            let error = answer["error"].as_str().unwrap_or_else(|| panic!("{method}: {answer}"));
            assert!(error.contains("created_on '31.10.2026'") && error.contains("2026-10-31T09:30:00Z"), "{method}: {error}");
        }
    }

    /// What `method` answers for `input` with the rest of the request in `fields`.
    fn answer(method: &str, input: &str, fields: serde_json::Value) -> serde_json::Value {
        let mut req = serde_json::json!({ "method": method, "input": input });
        for (key, value) in fields.as_object().unwrap() {
            req[key] = value.clone();
        }
        json(dispatch(req.to_string()))
    }

    /// A document of `pages` pages, each a column of texts.
    fn pages_of_text(pages: usize) -> String {
        let section = format!(r#"<section><layout><column>{}</column></layout></section>"#, "<text>Line of text</text>".repeat(40));
        format!(r#"<lpdf version="1"><document>{}</document></lpdf>"#, section.repeat(pages))
    }

    #[test]
    fn the_request_sets_the_memory_cap_of_its_render() {
        let xml = pages_of_text(20);
        for method in ["render_pdf", "render"] {
            let error = answer(method, &xml, serde_json::json!({ "max_memory_mb": 1 }));
            let error = error["error"].as_str().unwrap_or_else(|| panic!("{method}: {error}"));
            assert!(error.starts_with("the render needs more memory than its cap of 1 MB"), "{method}: {error}");
        }
        assert!(answer("render_pdf", &xml, serde_json::json!({ "max_memory_mb": 64 }))["pdf"].is_string());
        assert!(answer("render_pdf", &xml, serde_json::json!({}))["pdf"].is_string(), "1024 MB when left out");
    }

    #[test]
    fn the_data_of_the_request_counts_toward_the_cap() {
        let xml = r#"<lpdf version="1"><document><section><layout><text data-value="title">x</text></layout></section></document></lpdf>"#;
        // 2 MB of text: the request's text is dropped before the cap starts, which must not hide it.
        let rows: Vec<String> = (0..10_000).map(|i| format!("{} {i}", "Some text that is bound.".repeat(8))).collect();
        let data = serde_json::json!({ "title": "Report", "rows": rows });
        let error = answer("render_pdf", xml, serde_json::json!({ "data": data, "max_memory_mb": 1 }));
        assert!(error["error"].as_str().unwrap_or_default().contains("its cap of 1 MB"), "{error}");
        assert!(answer("render_pdf", xml, serde_json::json!({ "data": data, "max_memory_mb": 64 }))["pdf"].is_string());
    }

    #[test]
    fn a_memory_cap_that_is_not_a_whole_number_of_megabytes_from_1_to_4096_is_an_error() {
        for (mb, shown) in [(serde_json::json!(0), "0"), (serde_json::json!(1.5), "1.5"), (serde_json::json!(4097), "4097"), (serde_json::json!("512"), "\"512\"")] {
            let answer = answer("render_pdf", &with_output(""), serde_json::json!({ "max_memory_mb": mb }));
            let error = answer["error"].as_str().unwrap_or_else(|| panic!("{answer}"));
            assert!(error.contains("from 1 to 4096") && error.contains(&format!("not {shown}:")), "{error}");
        }
    }

    #[test]
    fn inputs_past_the_sizes_once_refused_render() {
        let padding = "x".repeat(5 * 1_048_576);
        let xml = format!(r#"<lpdf version="1"><!-- {padding} --><document><section><layout><text>Big</text></layout></section></document></lpdf>"#);
        let tree = format!(
            r#"{{"version":1,"type":"document","attrs":{{"meta":{{"subject":"{padding}"}}}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["Big"]}}]}}]}}]}}"#
        );
        assert!(answer("render_pdf", &xml, serde_json::json!({}))["pdf"].is_string());
        assert!(answer("render_tree_pdf", &tree, serde_json::json!({}))["pdf"].is_string());
    }

    #[test]
    fn the_data_of_the_request_is_bound_as_it_was_parsed() {
        let bound = r#"<lpdf version="1"><document><section><layout><text data-value="name">x</text></layout></section></document></lpdf>"#;
        let literal = r#"<lpdf version="1"><document><section><layout><text>Ada</text></layout></section></document></lpdf>"#;
        let with_data = answer("render_pdf", bound, serde_json::json!({ "data": { "name": "Ada" } }));
        assert_eq!(with_data["pdf"], answer("render_pdf", literal, serde_json::json!({}))["pdf"]);
        let without = answer("render_pdf", bound, serde_json::json!({ "data": null }));
        assert!(pdf_text(&without).starts_with("%PDF-"), "null data renders the inline content");
    }

    #[test]
    fn a_document_nests_100_levels_at_most_on_both_methods() {
        assert!(answer("render_pdf", &nesting::nested_xml(100), serde_json::json!({}))["pdf"].is_string());
        assert!(answer("render_tree_pdf", &nesting::nested_tree(100), serde_json::json!({}))["pdf"].is_string());
        for (method, input) in [("render_pdf", nesting::nested_xml(101)), ("render_tree_pdf", nesting::nested_tree(101))] {
            let answer = answer(method, &input, serde_json::json!({}));
            let error = answer["error"].as_str().unwrap_or_else(|| panic!("{method}: {answer}"));
            assert!(error.contains("is nested 101 levels deep, past the limit of 100"), "{method}: {error}");
        }
    }

    #[test]
    fn markdown_that_nests_too_deep_is_an_ordinary_error_on_both_methods() {
        let xml = |markdown: &str| {
            format!(
                r#"<lpdf version="1"><document><section><markdown><markdown-content><![CDATA[{markdown}]]></markdown-content></markdown></section></document></lpdf>"#
            )
        };
        let tree = |markdown: &str| {
            let content = serde_json::json!({ "type": "markdown-content", "attrs": {}, "nodes": [markdown] });
            serde_json::json!({
                "version": 1, "type": "document", "attrs": {},
                "nodes": [{ "type": "section", "attrs": {}, "nodes": [{ "type": "markdown", "attrs": {}, "nodes": [content] }] }]
            })
            .to_string()
        };
        let lists: Vec<String> = (0..500).map(|i| format!("{}- item", "  ".repeat(i))).collect();
        let fits = "> ".repeat(3) + "deep";
        assert!(answer("render_pdf", &xml(&fits), serde_json::json!({}))["pdf"].is_string());
        assert!(answer("render_tree_pdf", &tree(&fits), serde_json::json!({}))["pdf"].is_string());
        for markdown in ["> ".repeat(2000) + "deep", lists.join("\n")] {
            for (method, input) in [("render_pdf", xml(&markdown)), ("render_tree_pdf", tree(&markdown))] {
                let answer = answer(method, &input, serde_json::json!({}));
                let error = answer["error"].as_str().unwrap_or_else(|| panic!("{method}: {answer}"));
                assert!(error.contains("is nested too deep: a Markdown nests 31 quotes or 47 list items"), "{method}: {error}");
            }
        }
    }

    #[test]
    fn the_tree_path_reads_output_too() {
        let tree = r#"{"version":1,"type":"document","attrs":{"output":{"encrypt":"rc4-128"}},"nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[]}]}]}"#;
        let req = serde_json::json!({ "method": "render_tree_pdf", "input": tree });
        let answer = json(dispatch(req.to_string()));
        assert!(answer["error"].as_str().unwrap().contains("no owner password"), "{answer}");
    }
    #[test]
    fn a_stamp_request_answers_the_pdf_with_an_update_after_its_bytes() {
        let plain = render_request(&with_output(""), serde_json::json!({}));
        let pdf = base64::engine::general_purpose::STANDARD.decode(plain["pdf"].as_str().expect(&plain.to_string())).unwrap();
        let stamp = |page: &str| {
            let req = serde_json::json!({
                "method": "stamp", "pdf": base64::engine::general_purpose::STANDARD.encode(&pdf), "images": { "tall": TALL_PNG },
                "stamps": [{ "image": "tall", "page": page, "x": "10pt", "y": "10pt", "width": "50pt", "height": "50pt", "max-grow": 25 }],
            });
            json(dispatch(req.to_string()))
        };
        let answer = stamp("1");
        let stamped = base64::engine::general_purpose::STANDARD.decode(answer["pdf"].as_str().expect(&answer.to_string())).unwrap();
        assert!(stamped.starts_with(&pdf) && stamped.len() > pdf.len());
        // 1 by 2 px grown 25 times, in the middle of the box: 25 by 50 pt, 22.5 pt in from its left.
        assert!(pdf_text(&answer).contains("q 25 0 0 50 22.5 "), "{answer}");
        assert_eq!(stamp("2")["error"], "stamp 1: the PDF has no page 2: it has 1 page");
        let without = serde_json::json!({ "method": "stamp", "stamps": [] });
        assert_eq!(json(dispatch(without.to_string()))["error"], "request missing 'pdf' field");
    }

    /// The SHA-256 of `bytes`, in hex, as the snapshots in `test/snapshots` store it.
    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::Digest as _;
        sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    const BENCH_XL: &str = include_str!("../../../test/fixtures/bench_xl.xml");
    const BENCH_XL_HASH: &str = include_str!("../../../test/snapshots/bench_xl.pdf.sha256");

    /// What `answer`, the way in of both builds, answers `method` for `input` with the rest of the request in `fields`.
    fn answered(method: &str, input: &str, fields: serde_json::Value) -> serde_json::Value {
        let mut req = serde_json::json!({ "method": method, "input": input });
        for (key, value) in fields.as_object().unwrap() {
            req[key] = value.clone();
        }
        json(super::answer(req.to_string()))
    }

    #[test]
    fn a_render_past_its_timeout_answers_the_timeout_error_and_the_next_render_gives_the_stored_hash() {
        let answer = answered("render_pdf", BENCH_XL, serde_json::json!({ "timeout": 0.001 }));
        let error = answer["error"].as_str().unwrap_or_else(|| panic!("{answer}"));
        assert!(error.starts_with("the render ran past its timeout of 0.001 seconds: "), "{error}");
        assert_eq!(answer["timed_out"], true);

        for fields in [serde_json::json!({}), serde_json::json!({ "timeout": 0 }), serde_json::json!({ "timeout": 600 })] {
            let pdf = answered("render_pdf", BENCH_XL, fields.clone());
            let bytes = base64::engine::general_purpose::STANDARD.decode(pdf["pdf"].as_str().expect(&pdf.to_string())).unwrap();
            assert_eq!(sha256_hex(&bytes), BENCH_XL_HASH.trim(), "{fields}");
        }
    }

    #[test]
    fn a_timeout_that_is_not_a_number_of_seconds_from_0_up_is_an_error() {
        for (timeout, shown) in [(serde_json::json!(-1), "-1"), (serde_json::json!("5"), "\"5\"")] {
            let answer = answered("render_pdf", &with_output(""), serde_json::json!({ "timeout": timeout }));
            let error = answer["error"].as_str().unwrap_or_else(|| panic!("{answer}"));
            assert_eq!(error, format!("the timeout is a number of seconds, 0 for no limit, not {shown}"));
        }
    }

    #[test]
    fn a_panic_answers_an_error_that_says_the_engine_crashed_and_the_next_render_runs() {
        let answer = json(answer_with(|| {
            let _cap = memory::start(1);
            let _deadline = deadline::start(0.001);
            panic!("a bug");
        }));
        assert_eq!(answer, serde_json::json!({ "error": "the engine panicked: a bug", "crashed": true }));

        // Neither the cap nor the deadline of the render that panicked holds for the next.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let xml = pages_of_text(20);
        assert!(answered("render_pdf", &xml, serde_json::json!({}))["pdf"].is_string());
    }

    /// The fixtures of the core's snapshot tests that render with no images and no data.
    const PLAIN_FIXTURES: &[&str] = &[
        "example1", "example2", "example3", "example4", "example5", "example6", "example7", "example8", "example9",
        "example10", "example11", "bench_xs", "bench_s", "bench_m", "bench_l", "bench_xl", "showcase-column",
        "showcase-row-fill", "showcase-frame", "showcase-code", "showcase-code-split", "showcase-markdown-styles",
        "showcase-markdown-split", "showcase-markdown-wrap", "showcase-text-wide-words", "showcase-markdown-tables",
        "showcase-markdown-breaks", "showcase-table-auto", "showcase-table-lines", "showcase-lists",
        "showcase-lists-split", "showcase-lists-in-rows", "showcase-footnote", "showcase-footnote-counter",
        "showcase-footnote-clip", "showcase-link-heights", "showcase-grid", "showcase-row", "showcase-row-between",
        "showcase-row-split", "showcase-row-split-flags", "showcase-table", "showcase-table-keep",
        "showcase-table-colspan", "showcase-table-rowspan", "showcase-text-max-lines", "showcase-text-line-height",
        "showcase-text-justify", "showcase-text-align-spans", "showcase-inherited-style", "showcase-colors",
        "showcase-shadow", "showcase-gradient", "showcase-shape", "showcase-running-header",
        "showcase-running-header-dictionary", "showcase-page-numbers", "showcase-page-numbers-batch",
        "showcase-page-letters", "showcase-references", "showcase-reference-flip", "showcase-barcode",
        "showcase-barcode-fit", "showcase-forms", "showcase-canvas-overlay", "showcase-canvas-underlay",
        "showcase-canvas-layer", "showcase-canvas-rect", "showcase-canvas-text", "showcase-canvas-opacity",
        "showcase-region-chrome", "showcase-region-page-scope", "showcase-page-scope-printed", "showcase-toc",
        "showcase-columns", "showcase-columns-flow", "showcase-columns-markdown", "showcase-ruling",
        "showcase-ruling-sections", "showcase-columns-inset-last",
    ];

    /// The fixtures of the core's snapshot tests that show the images `photo`, `avatar` and `tall`.
    const IMAGE_FIXTURES: &[&str] = &[
        "showcase-markdown", "showcase-row-split-image", "showcase-page-clip", "showcase-image-box",
        "showcase-image-fill", "showcase-columns-insets",
    ];

    /// The fixtures of the core's snapshot tests whose data is the `.json` file beside them.
    const DATA_FIXTURES: &[&str] = &["showcase-columns-data", "showcase-bulk"];

    /// The file at `path` under `test`.
    fn test_file(path: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../test/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    /// The `render_pdf` request of each snapshot fixture above, by name, with its images or data as an SDK sends them.
    fn fixture_requests() -> Vec<(&'static str, serde_json::Value)> {
        let request = |name: &str| {
            let xml = String::from_utf8(test_file(&format!("fixtures/{name}.xml"))).unwrap();
            serde_json::json!({ "method": "render_pdf", "input": xml })
        };
        let image = |file: &str| base64::engine::general_purpose::STANDARD.encode(test_file(&format!("assets/images/{file}")));
        let images = serde_json::json!({
            "photo": image("image-sample.png"), "avatar": image("image-square.png"), "tall": image("image-tall.png"),
        });
        let mut requests: Vec<_> = PLAIN_FIXTURES.iter().map(|&name| (name, request(name))).collect();
        for &name in IMAGE_FIXTURES {
            let mut req = request(name);
            req["images"] = images.clone();
            requests.push((name, req));
        }
        for &name in DATA_FIXTURES {
            let mut req = request(name);
            req["data"] = serde_json::from_slice(&test_file(&format!("fixtures/{name}.json"))).unwrap();
            requests.push((name, req));
        }
        requests
    }

    /// The PDF that `answer` carries, for the fixture `name`.
    fn pdf_of(name: &str, answer: Answer) -> Vec<u8> {
        match answer {
            Answer::Pdf(pdf) => pdf,
            Answer::Json(json) => panic!("{name}: {json}"),
        }
    }

    /// What stdout carried for `pdf` on main, which wrote the answer with serde_json whole.
    fn main_stdout(pdf: &[u8]) -> Vec<u8> {
        serde_json::json!({ "pdf": base64::engine::general_purpose::STANDARD.encode(pdf) }).to_string().into_bytes()
    }

    /// That stdout carries `pdf`'s answer as main wrote it, for the fixture `name`, and that `json_len` is its length.
    fn assert_stdout_is_mains(name: &str, pdf: Vec<u8>) {
        let expected = main_stdout(&pdf);
        let answer = Answer::Pdf(pdf);
        // Compared whole, not with `assert_eq!`, which would print megabytes of base64.
        assert!(stdout_of(&answer) == expected, "{name}: stdout differs from main's");
        assert_eq!(answer.json_len(), expected.len(), "{name}");
    }

    #[test]
    fn a_render_and_a_stamp_write_to_stdout_what_main_wrote_for_each_fixture() {
        for (name, req) in fixture_requests() {
            let pdf = pdf_of(name, dispatch(req.to_string()));
            let stored = String::from_utf8(test_file(&format!("snapshots/{name}.pdf.sha256"))).unwrap();
            assert_eq!(sha256_hex(&pdf), stored.trim(), "{name}: the PDF is the snapshot's");

            let stamp = serde_json::json!({
                "method": "stamp", "pdf": base64::engine::general_purpose::STANDARD.encode(&pdf), "images": { "tall": TALL_PNG },
                "stamps": [{ "image": "tall", "page": "last", "x": "10pt", "y": "10pt", "width": "50pt", "height": "50pt" }],
            });
            let stamped = pdf_of(name, dispatch(stamp.to_string()));
            assert!(stamped.starts_with(&pdf) && stamped.len() > pdf.len(), "{name}: the stamp is an update after the PDF");

            assert_stdout_is_mains(name, pdf);
            assert_stdout_is_mains(name, stamped);
        }
    }

    #[test]
    fn the_registries_take_their_assets_out_of_the_request_as_they_decode_them() {
        let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let profile = include_bytes!("../../../test/assets/profiles/FOGRA39L_coated.icc");
        let invoice = b"<invoice/>";
        let mut req = serde_json::json!({
            "method": "render_pdf", "fonts": { "body": encode(pdf::ATTRIBUTION_FONT) }, "images": { "logo": TALL_PNG },
            "files": { "invoice": encode(invoice) }, "profiles": { "press": encode(profile) },
        });
        let assets = req.clone();

        let fonts = build_registry(&mut req);
        let images = build_image_registry(&mut req);
        let files = build_file_registry(&mut req);
        let profiles = build_profile_registry(&mut req);

        let tall = base64::engine::general_purpose::STANDARD.decode(TALL_PNG).unwrap();
        assert_eq!(fonts.get("body"), Some(pdf::ATTRIBUTION_FONT));
        assert_eq!(images.get("logo"), Some(&tall[..]));
        assert_eq!(files.get("invoice"), Some(&invoice[..]));
        assert_eq!(profiles.get("press"), Some(&profile[..]));
        let held = req.to_string();
        for (field, name) in [("fonts", "body"), ("images", "logo"), ("files", "invoice"), ("profiles", "press")] {
            let base64 = assets[field][name].as_str().unwrap();
            assert!(!held.contains(base64), "the request still holds the base64 of {field}.{name}");
        }
        assert_eq!(req["method"], "render_pdf", "the rest of the request stays");
    }

    #[test]
    fn an_error_found_before_the_answer_starts_is_the_error_json_on_stdout() {
        let logo = r#"<lpdf version="1"><assets><image name="logo"/></assets><document><section><layout><text>Hello</text>
            </layout></section></document></lpdf>"#;
        let plain = render_request(&with_output(""), serde_json::json!({}));
        for (req, error) in [
            (serde_json::json!({ "method": "render_pdf" }), "request missing 'input' field".to_string()),
            (
                serde_json::json!({ "method": "render_pdf", "input": logo }),
                "image 'logo' declared in <assets> but not loaded via loadImage()".to_string(),
            ),
            (
                serde_json::json!({ "method": "render_pdf", "input": pages_of_text(20), "max_memory_mb": 1 }),
                "the render needs more memory than its cap of 1 MB: raise the cap in the engine's options, or render \
                    the document in smaller parts".to_string(),
            ),
            (serde_json::json!({ "method": "stamp", "stamps": [] }), "request missing 'pdf' field".to_string()),
            (
                serde_json::json!({ "method": "stamp", "pdf": plain["pdf"], "images": { "tall": TALL_PNG },
                    "stamps": [{ "image": "tall", "page": "2", "x": "0pt", "y": "0pt", "width": "9pt", "height": "9pt" }] }),
                "stamp 1: the PDF has no page 2: it has 1 page".to_string(),
            ),
        ] {
            let stdout = String::from_utf8(stdout_of(&dispatch(req.to_string()))).unwrap();
            assert_eq!(stdout, serde_json::json!({ "error": error }).to_string());
        }
    }
}
