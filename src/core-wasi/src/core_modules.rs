// The core's modules, included by path with no wasm-bindgen: the WASI binary (`main.rs`) and the native library
// (`src/core-ffi`) both include this list at their root, so the two build the same engine. A module added to
// `src/core/src/lib.rs` is added here too.

#[path = "../../core/src/tokens.rs"]
mod tokens;
#[path = "../../core/src/parse.rs"]
mod parse;
#[path = "../../core/src/data.rs"]
mod data;
#[path = "../../core/src/deadline.rs"]
mod deadline;
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
#[path = "../../core/src/pdfua.rs"]
mod pdfua;
#[path = "../../core/src/pdfx.rs"]
mod pdfx;
#[path = "../../core/src/tagging.rs"]
mod tagging;
#[path = "../../core/src/attachments.rs"]
mod attachments;
#[path = "../../core/src/facturx.rs"]
mod facturx;
#[path = "../../core/src/license.rs"]
mod license;
#[path = "../../core/src/aes256.rs"]
mod aes256;
#[path = "../../core/src/encrypt.rs"]
mod encrypt;
#[path = "../../core/src/rewrite.rs"]
mod rewrite;
#[path = "../../core/src/objects.rs"]
mod objects;
#[path = "../../core/src/hints.rs"]
mod hints;
#[path = "../../core/src/linearize.rs"]
mod linearize;
#[path = "../../core/src/shared.rs"]
mod shared;
#[path = "../../core/src/kit_to_xml.rs"]
mod kit_to_xml;
#[path = "../../core/src/page_numbers.rs"]
mod page_numbers;
#[path = "../../core/src/page_scope.rs"]
mod page_scope;
#[path = "../../core/src/canvas.rs"]
mod canvas;
#[path = "../../core/src/ruling.rs"]
mod ruling;
#[path = "../../core/src/toc.rs"]
mod toc;
#[path = "../../core/src/references.rs"]
mod references;
#[path = "../../core/src/attr_types.rs"]
mod attr_types;
#[path = "../../core/src/schema_attrs.rs"]
mod schema_attrs;
#[path = "../../core/src/memory.rs"]
mod memory;
#[path = "../../core/src/nesting.rs"]
mod nesting;
#[path = "../../core/src/png.rs"]
mod png;
#[path = "../../core/src/stamp.rs"]
mod stamp;

/// Test-only stub that mirrors `LpdfEngine::render_xml_to_pdf_bytes` from the
/// core library crate so that `encrypt.rs` tests can reference `crate::LpdfEngine`
/// even when compiled inside the WASI binary or the native library.
#[cfg(test)]
struct LpdfEngine;

#[cfg(test)]
impl LpdfEngine {
    fn render_xml_to_pdf_bytes(xml: &str) -> Result<Vec<u8>, String> {
        let mut doc = parse::parse(xml)?;
        let lp = doc.section_layouts();
        let pages: Vec<render::RenderPage> =
            layout::layout_document(&lp)?;
        // Unlicensed, as in the core stub, so both crates hash the same attributed bytes.
        pdf::render_pdf(&pages, &doc.fonts, &pdf::FontRegistry::new(), &pdf::ImageRegistry::new(), &doc.meta, &doc.gradients, None, false, &doc.output, &[], None)
    }
}
