//! PDF/A renders of whole documents, with fonts and pictures loaded as an SDK loads them: what a PDF/A file gets,
//! what it refuses, and the samples `make check-pdfa` hands to veraPDF and qpdf. The WASI crate has its own
//! `LpdfEngine`, so these run with the core library only.

use std::path::{Path, PathBuf};

use crate::{LpdfEngine, RenderEncryption};

const CREATED_ON: &str = "2026-10-31T09:30:00Z";

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The fixture `showcase-pdfa-<level>.xml`.
fn fixture(level: &str) -> String {
    String::from_utf8(read(&test_dir().join(format!("fixtures/showcase-pdfa-{level}.xml")))).expect("UTF-8")
}

/// An unlicensed engine with the fonts and pictures the fixtures declare, loaded under their names, as an SDK
/// loads what a document's `src` attributes name.
fn engine(created_on: Option<&str>) -> LpdfEngine {
    let mut engine = LpdfEngine::new("");
    let fonts = test_dir().join("assets/fonts");
    for (name, file) in [
        ("body", "CrimsonText-Regular.ttf"), ("italic", "CrimsonText-Italic.ttf"),
        ("heading", "CrimsonText-SemiBold.ttf"), ("sans", "Montserrat-Regular.ttf"),
    ] {
        engine.load_font(name, &read(&fonts.join(file)));
    }
    let images = test_dir().join("assets/images");
    for (name, file) in [("photo", "chapter-05-caterpillar.jpg"), ("chart", "image-sample.png"), ("badge", "image-square.png")] {
        engine.load_image(name, &read(&images.join(file)));
    }
    if let Some(date) = created_on {
        engine.set_created_on(date);
    }
    engine
}

fn render(xml: &str) -> Result<Vec<u8>, String> {
    engine(Some(CREATED_ON)).render_xml(xml, None, &RenderEncryption::default())
}

/// The fixture `showcase-pdfa-<level>.xml` rendered as an SDK renders it, unlicensed: for the snapshot tests.
pub(crate) fn render_fixture(level: &str) -> Vec<u8> {
    render(&fixture(level)).unwrap_or_else(|e| panic!("showcase-pdfa-{level}: {e}"))
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// A document in the font `body` with `pdfa="2b"`, `assets` beside the font, and `layout` as its one layout.
fn document(assets: &str, layout: &str) -> String {
    format!(
        r##"<lpdf version="1"><assets><font name="body" src="body.ttf"/>{assets}</assets>
        <document font="body"><output pdfa="2b"/><section><layout>{layout}</layout></section></document></lpdf>"##
    )
}

fn refused(xml: &str) -> String {
    render(xml).expect_err("a PDF/A render that is refused")
}

// ── What a PDF/A file gets ────────────────────────────────────────────────────

#[test]
fn a_pdfa_file_carries_its_level_profile_id_and_dates() {
    let pdf = render(&fixture("2b")).expect("renders");
    let pdf = text(&pdf);
    for expected in [
        "<pdfaid:part>2</pdfaid:part>", "<pdfaid:conformance>B</pdfaid:conformance>",
        "/Type /Metadata", "/Subtype /XML", "/OutputIntents [<<", "/S /GTS_PDFA1",
        "/OutputConditionIdentifier (sRGB IEC61966-2.1)", "/DestOutputProfile", "/N 3",
        "/CreationDate (D:20261031093000Z)", "/ModDate (D:20261031093000Z)",
        "<xmp:CreateDate>2026-10-31T09:30:00Z</xmp:CreateDate>", "/CIDToGIDMap /Identity",
    ] {
        assert!(pdf.contains(expected), "{expected}");
    }
    let trailer = &pdf[pdf.rfind("trailer").expect("a trailer")..];
    assert!(trailer.contains("/ID ["), "{trailer}");
}

#[test]
fn level_2u_is_named_in_the_xmp_packet() {
    let pdf = text(&render(&fixture("2u")).expect("renders"));
    assert!(pdf.contains("<pdfaid:conformance>U</pdfaid:conformance>"));
}

#[test]
fn every_link_of_a_pdfa_file_prints() {
    let pdf = text(&render(&fixture("2b")).expect("renders"));
    let links = pdf.matches("/Subtype /Link").count();
    // Two links of the document and the attribution line's on each of its two pages.
    assert_eq!(links, 4, "links");
    assert_eq!(pdf.matches("/F 4").count(), links, "a print flag on each link");
}

#[test]
fn the_same_document_gives_the_same_file() {
    assert_eq!(render(&fixture("2b")).expect("renders"), render(&fixture("2b")).expect("renders"));
}

#[test]
fn a_document_without_pdfa_gets_none_of_it() {
    let xml = fixture("2b").replace(r#"<output pdfa="2b" />"#, "");
    let pdf = text(&render(&xml).expect("renders"));
    for absent in ["/Metadata", "/OutputIntents", "/ModDate", "/CIDToGIDMap", "/F 4", "/ID ["] {
        assert!(!pdf.contains(absent), "{absent}");
    }
}

#[test]
fn pdfa_is_read_from_the_json_of_an_sdk() {
    let tree = |output: &str| format!(
        r#"{{"version":1,"type":"document","attrs":{{"output":{output}}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[]}}]}}]}}"#
    );
    let doc = crate::parse::parse_tree(&tree(r#"{"pdfa":"2u"}"#)).expect("parses");
    assert_eq!(doc.output.pdfa, Some(crate::pdfa::PdfA::Level2u));
    assert_eq!(crate::parse::parse_tree(&tree(r#"{"pdfa":null}"#)).expect("parses").output.pdfa, None);
    let error = crate::parse::parse_tree(&tree(r#"{"pdfa":2}"#)).unwrap_err();
    assert!(error.contains("pdfa must be a string"), "{error}");
}

// ── What it refuses ───────────────────────────────────────────────────────────

#[test]
fn a_level_the_engine_does_not_write_is_an_error() {
    let xml = document("", "<text>A</text>").replace(r#"pdfa="2b""#, r#"pdfa="2a""#);
    let error = refused(&xml);
    assert!(error.contains("<output>") && error.contains("'2a'") && error.contains("2b or 2u"), "{error}");
}

#[test]
fn pdfa_with_encryption_is_an_error_when_the_document_is_read() {
    let xml = document("", "<text>A</text>").replace(r#"pdfa="2b""#, r#"pdfa="2b" encrypt="aes-256""#);
    for error in [refused(&xml), crate::parse::parse(&xml).map(|_| ()).unwrap_err()] {
        assert!(error.contains("encrypt") && error.contains("forbids encryption") && error.contains("remove pdfa"), "{error}");
    }
}

#[test]
fn a_render_without_created_on_is_an_error_before_the_layout() {
    let xml = document("", "<text>A</text>");
    let error = engine(None).render_xml(&xml, None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("created_on") && error.contains("no clock"), "{error}");
    let error = engine(Some("31/10/2026")).render_xml(&xml, None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("'31/10/2026'"), "{error}");
}

#[test]
fn a_built_in_font_is_an_error_that_names_the_text() {
    let xml = r#"<lpdf version="1"><document><output pdfa="2b"/><section><layout><text>Quarterly figures</text></layout></section></document></lpdf>"#;
    let error = refused(xml);
    assert!(error.contains("\"Quarterly figures\"") && error.contains("Helvetica") && error.contains("<assets>"), "{error}");
}

#[test]
fn a_built_in_font_named_on_a_text_is_an_error_too() {
    let error = refused(&document("", r#"<text font="Times-Roman">Signed</text>"#));
    assert!(error.contains("\"Signed\"") && error.contains("Times-Roman"), "{error}");
}

#[test]
fn a_declared_font_whose_file_was_not_loaded_is_an_error_that_names_it() {
    let error = refused(&document(r#"<font name="brand" src="brand.ttf"/>"#, r#"<text font="brand">Logo</text>"#));
    assert!(error.contains("'brand'") && error.contains("not loaded"), "{error}");
}

#[test]
fn a_character_the_font_has_no_glyph_for_is_an_error_that_names_it() {
    let error = refused(&document("", "<text>Price 日本</text>"));
    assert!(error.contains("'日' (U+65E5)") && error.contains("'body'"), "{error}");
}

#[test]
fn a_font_whose_licence_forbids_embedding_is_an_error() {
    let mut engine = engine(Some(CREATED_ON));
    engine.load_font("body", &restricted(&read(&test_dir().join("assets/fonts/CrimsonText-Regular.ttf"))));
    let error = engine.render_xml(&document("", "<text>A</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("'body'") && error.contains("does not allow embedding"), "{error}");
}

/// `font` with the embedding permission of its OS/2 table set to restricted (`fsType` 2).
fn restricted(font: &[u8]) -> Vec<u8> {
    let mut font = font.to_vec();
    let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    let os2 = (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&record| &font[record..record + 4] == b"OS/2")
        .map(|record| u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize)
        .expect("an OS/2 table");
    font[os2 + 8..os2 + 10].copy_from_slice(&2u16.to_be_bytes());
    font
}

#[test]
fn a_form_field_is_an_error_that_names_it() {
    let error = refused(&document("", r#"<field type="text" name="email" width="120pt"/>"#));
    assert!(error.contains("'email'") && error.contains("Helvetica") && error.contains("Remove the field"), "{error}");
}

#[test]
fn the_digits_of_a_barcode_are_an_error() {
    let error = refused(&document("", r#"<barcode type="code128" data="ABC" hrt="true"/>"#));
    assert!(error.contains("hrt=\"false\""), "{error}");
    assert!(render(&document("", r#"<barcode type="code128" data="ABC"/>"#)).is_ok(), "without its digits it renders");
}

#[test]
fn a_cmyk_color_is_an_error_that_names_it() {
    for layout in [
        r#"<text color="cmyk(0, 0, 0, 100)">A</text>"#,
        r#"<column background="cmyk(0 100 100 0)"><text>A</text></column>"#,
        r##"<column background="#ffffff" shadow="2pt 2pt cmyk(0 0 0 100)"><text>A</text></column>"##,
        r#"<divider color="cmyk(100, 0, 0, 0)"/>"#,
    ] {
        let error = refused(&document("", layout));
        assert!(error.contains("cmyk(") && error.contains("sRGB output intent"), "{layout}: {error}");
    }
}

#[test]
fn a_cmyk_page_background_or_layer_shape_is_an_error() {
    let section = |inner: &str, background: &str| format!(
        r#"<lpdf version="1"><assets><font name="body" src="body.ttf"/></assets><document font="body"><output pdfa="2b"/>
        <section{background}>{inner}<layout><text>A</text></layout></section></document></lpdf>"#
    );
    let error = refused(&section("", r#" background="cmyk(0, 0, 10, 0)""#));
    assert!(error.contains("cmyk(0, 0, 10, 0)"), "{error}");
    let error = refused(&section(r#"<layer><rect x="0pt" y="0pt" width="10pt" height="10pt" fill="cmyk(0 0 0 50)"/></layer>"#, ""));
    assert!(error.contains("cmyk(0, 0, 0, 50)"), "{error}");
}

#[test]
fn a_gradient_of_cmyk_colors_is_an_error_that_names_it() {
    let tokens = r#"<tokens><gradients><gradient name="ink" type="linear" from="cmyk(0 0 0 100)" to="cmyk(0 0 0 0)"/></gradients></tokens>"#;
    let xml = document("", r#"<column background="gradient:ink" height="20pt"/>"#)
        .replacen("</assets>", &format!("</assets>{tokens}"), 1);
    let error = refused(&xml);
    assert!(error.contains("'ink'") && error.contains("CMYK"), "{error}");
}

// ── The samples of the external check ─────────────────────────────────────────

/// Writes the files `scripts/check-pdfa.mjs` hands to veraPDF and qpdf, when `LPDF_PDFA_OUT` names a folder: the two
/// fixtures, rendered unlicensed, so that the attribution line is checked too, with their fonts and pictures. Without
/// the variable the test does nothing, so a plain `cargo test` needs no other tool.
#[test]
fn writes_pdfa_samples_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_PDFA_OUT") else { return };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the samples' folder");
    for level in ["2b", "2u"] {
        std::fs::write(dir.join(format!("pdfa-{level}.pdf")), render_fixture(level)).expect("a sample");
    }
}
