//! PDF/X renders of whole documents, with fonts, pictures and the press profile loaded as an SDK loads them: what a
//! PDF/X file gets, what it leaves out and what it refuses, and the samples `make check-pdfx` hands to qpdf. The WASI
//! crate has its own `LpdfEngine`, so these run with the core library only.

use std::path::{Path, PathBuf};

use crate::{LpdfEngine, RenderEncryption};

const CREATED_ON: &str = "2026-10-31T09:30:00Z";

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The fixture `showcase-pdfx.xml`.
fn fixture() -> String {
    String::from_utf8(read(&test_dir().join("fixtures/showcase-pdfx.xml"))).expect("UTF-8")
}

/// The FOGRA39 printer profile of the test assets.
fn press_profile() -> Vec<u8> {
    read(&test_dir().join("assets/profiles/FOGRA39L_coated.icc"))
}

/// An unlicensed engine with the fonts, pictures and press profile the fixture declares, loaded under their names, as
/// an SDK loads what a document's `src` attributes name.
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
    engine.load_profile("press", &press_profile());
    if let Some(date) = created_on {
        engine.set_created_on(date);
    }
    engine
}

pub(crate) fn render(xml: &str) -> Result<Vec<u8>, String> {
    engine(Some(CREATED_ON)).render_xml(xml, None, &RenderEncryption::default())
}

/// The fixture `showcase-pdfx.xml` rendered as an SDK renders it, unlicensed: for the snapshot test.
pub(crate) fn render_fixture() -> Vec<u8> {
    render(&fixture()).unwrap_or_else(|e| panic!("showcase-pdfx: {e}"))
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// A titled document in the font `body` that asks for PDF/X-4 with the profile `press`, `assets` beside the font and
/// the profile, `attrs` on the document, and `layout` as its one layout.
fn document(assets: &str, attrs: &str, layout: &str) -> String {
    format!(
        r##"<lpdf version="1"><assets><font name="body" src="body.ttf"/><profile name="press" src="press.icc"/>{assets}</assets>
        <document font="body"{attrs}><meta title="Flyer"/><output pdfx="4" profile="press"/><section><layout>{layout}</layout></section></document></lpdf>"##
    )
}

fn refused(xml: &str) -> String {
    render(xml).expect_err("a PDF/X render that is refused")
}

/// The numbers of the `/Name [a b c d]` boxes of the page dictionaries of `pdf`, in the order they appear.
fn boxes(pdf: &str, name: &str) -> Vec<[f32; 4]> {
    let key = format!("/{name} [");
    pdf.match_indices(&key)
        .map(|(at, _)| {
            let rest = &pdf[at + key.len()..];
            let numbers: Vec<f32> = rest[..rest.find(']').unwrap()].split_whitespace().map(|n| n.parse().unwrap()).collect();
            numbers.try_into().unwrap()
        })
        .collect()
}

/// The decoded data of every Flate stream of `pdf`, read by the `/Length` of its dictionary.
fn flate_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = pdf[from..].windows(7).position(|w| w == b"stream\n") {
        let start = from + found + 7;
        from = start;
        if pdf[..start - 7].ends_with(b"end") {
            continue;
        }
        let dict_start = pdf[..start].windows(3).rposition(|w| w == b"obj").unwrap_or(0);
        let dict = String::from_utf8_lossy(&pdf[dict_start..start]);
        if !dict.contains("/FlateDecode") {
            continue;
        }
        let Some(length) = dict.split("/Length ").nth(1).and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        if let Ok(data) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..start + length]) {
            out.push(data);
        }
    }
    out
}

const A4: (f32, f32) = (595.28, 841.89);
const MM: f32 = 72.0 / 25.4;

fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
}

// ── What a PDF/X file gets ────────────────────────────────────────────────────

#[test]
fn every_page_has_a_trim_box_and_a_bleed_box_grown_by_its_bleed() {
    let pdf = text(&render_fixture());
    let (w, h) = A4;
    let trims = boxes(&pdf, "TrimBox");
    let bleeds = boxes(&pdf, "BleedBox");
    let media = boxes(&pdf, "MediaBox");
    assert_eq!(trims.len(), 2, "a TrimBox on each of the two pages");
    assert_eq!((bleeds.len(), media.len()), (2, 2), "a BleedBox and a MediaBox on each page");
    for trim in &trims {
        assert!(near(*trim, [0.0, 0.0, w, h]), "the trim box is the page: {trim:?}");
    }
    for (index, bleed) in [3.0 * MM, 5.0 * MM].into_iter().enumerate() {
        let sheet = [-bleed, -bleed, w + bleed, h + bleed];
        assert!(near(bleeds[index], sheet), "page {index}: {:?}", bleeds[index]);
        assert!(near(media[index], sheet), "page {index}: {:?}", media[index]);
    }
}

#[test]
fn the_output_intent_is_the_press_profile() {
    let bytes = render_fixture();
    let pdf = text(&bytes);
    for expected in [
        "/OutputIntents [<<", "/Type /OutputIntent", "/S /GTS_PDFX", "/OutputConditionIdentifier (Custom)",
        "/OutputCondition (FOGRA39L Coated)", "/Info (FOGRA39L Coated)", "/DestOutputProfile",
        "/N 4", "/Alternate /DeviceCMYK",
    ] {
        assert!(pdf.contains(expected), "{expected}");
    }
    assert_eq!(pdf.matches("/S /GTS_PDFX").count(), 1, "one output intent");
    assert!(!pdf.contains("/GTS_PDFA1"), "no PDF/A output intent");
    // The profile's stream holds the profile itself.
    let profile = press_profile();
    let streams = flate_streams(&bytes);
    assert!(streams.iter().any(|stream| *stream == profile), "the profile's bytes are in the file");
}

#[test]
fn rgb_is_mapped_to_srgb_on_every_page_and_in_every_group() {
    let bytes = render_fixture();
    let pdf = text(&bytes);
    let default_rgb = pdf.matches("/DefaultRGB [/ICCBased").count();
    let resources = pdf.matches("/Resources <<").count();
    assert!(default_rgb >= 2 && default_rgb == resources, "every resource dictionary maps RGB: {default_rgb} of {resources}");
    assert!(pdf.contains("/CS [/ICCBased"), "a transparency group blends in sRGB");
    assert!(!pdf.contains("/CS /DeviceRGB"), "no group blends in device RGB");
    assert!(pdf.contains("/N 3"), "the sRGB profile's stream");
    assert!(bytes.windows(crate::pdfa::SRGB_PROFILE.len()).any(|w| w == crate::pdfa::SRGB_PROFILE), "the bundled sRGB profile");
}

#[test]
fn a_pdfx_file_carries_its_version_dates_id_and_trapping() {
    let pdf = text(&render_fixture());
    assert!(pdf.starts_with("%PDF-1.6"), "PDF/X-4 is a profile of PDF 1.6");
    for expected in [
        "/GTS_PDFXVersion (PDF/X-4)", "/Trapped /False", "/Title ",
        "/CreationDate (D:20261031093000Z)", "/ModDate (D:20261031093000Z)",
        "/Type /Metadata", "/Subtype /XML",
        "<pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion>", "<pdf:Trapped>False</pdf:Trapped>",
        "<xmpMM:DocumentID>uuid:", "<xmpMM:InstanceID>uuid:", "<xmpMM:VersionID>1</xmpMM:VersionID>",
        "<xmpMM:RenditionClass>default</xmpMM:RenditionClass>", "<dc:title><rdf:Alt>",
        "<xmp:CreateDate>2026-10-31T09:30:00Z</xmp:CreateDate>", "<xmp:ModifyDate>2026-10-31T09:30:00Z</xmp:ModifyDate>",
    ] {
        assert!(pdf.contains(expected), "{expected}");
    }
    assert!(!pdf.contains("GTS_PDFXConformance"), "PDF/X-4 writes the version alone");
    let trailer = &pdf[pdf.rfind("trailer").expect("a trailer")..];
    assert!(trailer.contains("/ID ["), "{trailer}");
}

#[test]
fn every_font_is_embedded() {
    let pdf = text(&render_fixture());
    assert!(pdf.contains("/FontFile2") || pdf.contains("/FontFile3"), "embedded font programs");
    assert!(!pdf.contains("/Subtype /Type1"), "no built-in font");
}

#[test]
fn a_pdfx_file_has_no_annotations_and_its_links_print_as_text() {
    let with = render_fixture();
    let pdf = text(&with);
    for absent in ["/Annots", "/Subtype /Link", "/URI"] {
        assert!(!pdf.contains(absent), "{absent}");
    }
    // The outline is not an annotation, and stays.
    assert!(pdf.contains("/Outlines"), "the bookmarks");
    // The link's words are drawn as in a link: the same content as a document whose link is a plain column.
    let plain = fixture().replace(r#"<link href="https://lpdf.io">"#, "<column>").replace("</link>", "</column>");
    assert_eq!(content_streams(&with), content_streams(&render(&plain).expect("renders")));
}

/// The decoded content streams of the pages of `pdf`, in order.
fn content_streams(pdf: &[u8]) -> Vec<Vec<u8>> {
    flate_streams(pdf)
        .into_iter()
        .filter(|stream| stream.windows(3).any(|w| w == b" Tf"))
        .collect()
}

#[test]
fn the_same_document_gives_the_same_file() {
    assert_eq!(render_fixture(), render_fixture());
}

#[test]
fn a_document_with_neither_bleed_nor_pdfx_gets_none_of_it() {
    let xml = fixture().replace(r#"<output pdfx="4" profile="press" />"#, "").replace(r#" bleed="3mm""#, "").replace(r#" bleed="5mm""#, "");
    let pdf = text(&render(&xml).expect("renders"));
    for absent in ["/TrimBox", "/BleedBox", "/OutputIntents", "/DefaultRGB", "/GTS_PDFXVersion", "/Trapped", "/Metadata", "/ID ["] {
        assert!(!pdf.contains(absent), "{absent}");
    }
    assert!(pdf.starts_with("%PDF-1.7"));
    assert!(pdf.contains("/Subtype /Link"), "its link is clickable");
}

#[test]
fn bleed_without_pdfx_gives_the_boxes_and_the_grown_page_and_nothing_else() {
    let xml = fixture().replace(r#"<output pdfx="4" profile="press" />"#, "");
    let pdf = text(&render(&xml).expect("renders"));
    let (w, h) = A4;
    let bleed = 3.0 * MM;
    assert!(near(boxes(&pdf, "TrimBox")[0], [0.0, 0.0, w, h]));
    assert!(near(boxes(&pdf, "BleedBox")[0], [-bleed, -bleed, w + bleed, h + bleed]));
    assert!(near(boxes(&pdf, "MediaBox")[0], [-bleed, -bleed, w + bleed, h + bleed]));
    for absent in ["/OutputIntents", "/DefaultRGB", "/GTS_PDFXVersion", "/Trapped", "/Metadata"] {
        assert!(!pdf.contains(absent), "{absent}");
    }
    assert!(pdf.starts_with("%PDF-1.7"));
}

#[test]
fn the_page_background_fills_the_bleed() {
    let pdf = render(&document("", r#" bleed="10pt""#, "<text>A</text>").replace("<section>", r##"<section background="#336699">"##))
        .expect("renders");
    let page = &content_streams(&pdf)[0];
    let page = String::from_utf8_lossy(page);
    assert!(page.contains("-10 -10 615.28 861.89 re"), "{page}");
}

#[test]
fn a_section_bleed_overrides_the_document_and_zero_takes_it_away() {
    let xml = |section: &str| document("", r#" bleed="3mm""#, "<text>A</text>").replace("<section>", &format!("<section{section}>"));
    let bleed = |xml: String| boxes(&text(&render(&xml).expect("renders")), "BleedBox")[0][0];
    assert!((bleed(xml("")) + 3.0 * MM).abs() < 0.01);
    assert!((bleed(xml(r#" bleed="8pt""#)) + 8.0).abs() < 0.01);
    assert_eq!(bleed(xml(r#" bleed="0mm""#)), 0.0, "a PDF/X page with no bleed has its boxes all the same");
}

#[test]
fn pdfx_is_read_from_the_json_of_an_sdk() {
    let tree = |attrs: &str| format!(
        r#"{{"version":1,"type":"document","attrs":{attrs},"nodes":[{{"type":"section","attrs":{{"bleed":"2mm"}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[]}}]}}]}}"#
    );
    let doc = crate::parse::parse_tree(&tree(
        r#"{"bleed":"3mm","assets":{"profiles":[{"name":"press","ref":"fogra39","src":"press.icc"}]},"output":{"pdfx":"4","profile":"press"}}"#,
    ))
    .expect("parses");
    let pdfx = doc.output.pdfx.as_ref().expect("pdfx");
    assert_eq!((pdfx.version, pdfx.profile.as_str()), (crate::pdfx::PdfXVersion::X4, "press"));
    assert_eq!(doc.profiles.get("press").map(String::as_str), Some("fogra39"));
    assert!((doc.bleed - 3.0 * MM).abs() < 0.01);
    assert!((doc.sections[0].options.bleed.unwrap() - 2.0 * MM).abs() < 0.01);
    let error = crate::parse::parse_tree(&tree(r#"{"output":{"pdfx":4,"profile":"press"}}"#)).unwrap_err();
    assert!(error.contains("pdfx must be a string"), "{error}");
}

#[test]
fn a_tree_renders_as_its_xml_does() {
    let xml = document("", r#" bleed="3mm""#, "<text>Lpdf</text>");
    let tree = r#"{"version":1,"type":"document","attrs":{"font":"body","bleed":"3mm",
        "assets":{"fonts":[{"name":"body","src":"body.ttf"}],"profiles":[{"name":"press","src":"press.icc"}]},
        "meta":{"title":"Flyer"},"output":{"pdfx":"4","profile":"press"}},
        "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[{"type":"text","attrs":{},"nodes":["Lpdf"]}]}]}]}"#;
    let engine = engine(Some(CREATED_ON));
    let from_xml = engine.render_xml(&xml, None, &RenderEncryption::default()).expect("the XML renders");
    let from_tree = engine.render_json(tree, &RenderEncryption::default()).expect("the tree renders");
    assert_eq!(from_xml, from_tree);
}

// ── What it refuses ───────────────────────────────────────────────────────────

#[test]
fn pdfx_without_a_profile_is_an_error_when_the_document_is_read() {
    let xml = document("", "", "<text>A</text>").replace(r#" profile="press""#, "");
    for error in [refused(&xml), crate::parse::parse(&xml).map(|_| ()).unwrap_err()] {
        assert!(error.contains("<output>") && error.contains("needs profile") && error.contains("<profile name="), "{error}");
    }
}

#[test]
fn a_profile_without_pdfx_is_an_error() {
    let xml = document("", "", "<text>A</text>").replace(r#"pdfx="4" "#, "");
    let error = refused(&xml);
    assert!(error.contains("add pdfx=\"4\", or remove profile"), "{error}");
}

#[test]
fn a_version_the_engine_does_not_write_is_an_error() {
    let xml = document("", "", "<text>A</text>").replace(r#"pdfx="4""#, r#"pdfx="1a""#);
    let error = refused(&xml);
    assert!(error.contains("<output>") && error.contains("'1a'") && error.contains("use 4"), "{error}");
}

#[test]
fn a_profile_that_is_not_declared_or_not_loaded_is_an_error_that_names_it() {
    let undeclared = document("", "", "<text>A</text>").replace(r#"<profile name="press" src="press.icc"/>"#, "");
    let error = refused(&undeclared);
    assert!(error.contains("profile 'press' is not a <profile> of <assets>"), "{error}");
    let mut bare = LpdfEngine::new("");
    bare.load_font("body", &read(&test_dir().join("assets/fonts/CrimsonText-Regular.ttf")));
    bare.set_created_on(CREATED_ON);
    let error = bare.render_xml(&document("", "", "<text>A</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("was not loaded") && error.contains("loadProfile('press', bytes)"), "{error}");
}

#[test]
fn a_profile_that_is_not_a_printer_profile_is_an_error() {
    let mut engine = engine(Some(CREATED_ON));
    engine.load_profile("press", crate::pdfa::SRGB_PROFILE);
    let error = engine.render_xml(&document("", "", "<text>A</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.starts_with("<output pdfx=\"4\">: the profile 'press' is a display profile"), "{error}");
    engine.load_profile("press", b"not an ICC profile");
    let error = engine.render_xml(&document("", "", "<text>A</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("'press' is not an ICC profile"), "{error}");
}

#[test]
fn a_document_without_a_title_is_an_error() {
    let error = refused(&document("", "", "<text>A</text>").replace(r#"<meta title="Flyer"/>"#, ""));
    assert!(error.contains("add title to <meta>"), "{error}");
}

#[test]
fn a_render_without_created_on_is_an_error_that_names_it() {
    let error = engine(None).render_xml(&document("", "", "<text>A</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.starts_with("<output pdfx=\"4\">") && error.contains("created_on") && error.contains("PDF/X"), "{error}");
}

#[test]
fn a_built_in_font_is_an_error_that_names_the_text() {
    let xml = document("", "", "<text>A</text>").replace(r#" font="body""#, "").replace("<text>A</text>", "<text>Spring sale</text>");
    let error = refused(&xml);
    assert!(error.contains("\"Spring sale\"") && error.contains("Helvetica") && error.contains("PDF/X") && error.contains("<assets>"), "{error}");
}

#[test]
fn the_digits_of_a_barcode_are_an_error() {
    let error = refused(&document("", "", r#"<barcode type="code128" data="ABC" hrt="true"/>"#));
    assert!(error.contains("hrt=\"false\""), "{error}");
}

#[test]
fn a_form_field_is_an_error_that_names_it() {
    let error = refused(&document("", "", r#"<field type="text" name="email" width="120pt"/>"#));
    assert!(error.contains("'email'") && error.contains("PDF/X") && error.contains("Remove the field"), "{error}");
}

#[test]
fn pdfx_with_encryption_pdfa_or_attachments_is_an_error_when_the_document_is_read() {
    let base = document("", "", "<text>A</text>");
    let encrypted = base.replace(r#"pdfx="4""#, r#"pdfx="4" encrypt="aes-256""#);
    let error = crate::parse::parse(&encrypted).map(|_| ()).unwrap_err();
    assert!(error.contains("forbids encryption") && error.contains("remove pdfx"), "{error}");
    let archived = base.replace(r#"pdfx="4""#, r#"pdfx="4" pdfa="2b""#);
    let error = crate::parse::parse(&archived).map(|_| ()).unwrap_err();
    assert!(error.contains("pdfa=\"2b\"") && error.contains("Remove pdfa or pdfx"), "{error}");
    let attached = base.replace("<section>", r#"<attachments><file name="a.txt"/></attachments><section>"#);
    let error = crate::parse::parse(&attached).map(|_| ()).unwrap_err();
    assert!(error.contains("'a.txt'") && error.contains("remove pdfx"), "{error}");
}

#[test]
fn a_font_whose_licence_forbids_embedding_is_an_error() {
    let mut engine = engine(Some(CREATED_ON));
    let font = read(&test_dir().join("assets/fonts/CrimsonText-Regular.ttf"));
    engine.load_font("body", &crate::pdfa_render_tests::restricted(&font));
    let error = engine.render_xml(&document("", "", "<text>A</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.starts_with("<output pdfx=\"4\">") && error.contains("'body'") && error.contains("does not allow embedding"), "{error}");
    assert!(error.contains("a PDF/X file embeds every font"), "{error}");
}

/// An engine like `engine`, whose press profile `press` prints in RGB.
fn rgb_press_engine() -> LpdfEngine {
    let mut engine = engine(Some(CREATED_ON));
    engine.load_profile("press", &crate::pdfx::test_profile(b"prtr", b"RGB ", "Digital press"));
    engine
}

#[test]
fn a_cmyk_page_background_with_a_profile_that_is_not_cmyk_is_an_error() {
    let xml = document("", "", "<text>A</text>").replace("<section>", r#"<section background="cmyk(0, 0, 10, 0)">"#);
    let error = rgb_press_engine().render_xml(&xml, None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("cmyk(0, 0, 10, 0)") && error.contains("'press' prints in an RGB color space"), "{error}");
    assert!(render(&xml).is_ok(), "with a CMYK press profile it renders");
}

#[test]
fn a_gradient_of_cmyk_colors_with_a_profile_that_is_not_cmyk_is_an_error() {
    let tokens = r#"<tokens><gradients><gradient name="ink" type="linear" from="cmyk(0 0 0 100)" to="cmyk(0 0 0 0)"/></gradients></tokens>"#;
    let xml = document("", "", r#"<column background="gradient:ink" height="20pt"/>"#)
        .replacen("</assets>", &format!("</assets>{tokens}"), 1);
    let error = rgb_press_engine().render_xml(&xml, None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("the gradient 'ink' has CMYK colors") && error.contains("RGB color space"), "{error}");
    assert!(render(&xml).is_ok(), "with a CMYK press profile it renders");
}

#[test]
fn a_cmyk_color_with_a_profile_that_is_not_cmyk_is_an_error() {
    let mut engine = engine(Some(CREATED_ON));
    engine.load_profile("press", &crate::pdfx::test_profile(b"prtr", b"RGB ", "Digital press"));
    let error = engine
        .render_xml(&document("", "", r#"<text color="cmyk(0, 0, 0, 100)">A</text>"#), None, &RenderEncryption::default())
        .unwrap_err();
    assert!(error.contains("cmyk(") && error.contains("'press' prints in an RGB color space"), "{error}");
    // With an RGB press profile, RGB colors are what it prints.
    let xml = document("", "", "<text color=\"#336699\">A</text>");
    let pdf = text(&engine.render_xml(&xml, None, &RenderEncryption::default()).expect("renders"));
    assert!(pdf.contains("/Alternate /DeviceRGB") && pdf.contains("/Info (Digital press)"));
}

// ── PDF/X with PDF/UA ─────────────────────────────────────────────────────────

#[test]
fn pdfx_combines_with_pdfua_and_its_links_are_text() {
    let xml = document("", "", r#"<text role="h1">Lpdf</text><link href="https://lpdf.io"><text>Link</text></link>"#)
        .replace(r#"<meta title="Flyer"/>"#, r#"<meta title="Flyer" lang="en"/>"#)
        .replace(r#"pdfx="4""#, r#"pdfua="1" pdfx="4""#);
    let pdf = text(&render(&xml).expect("renders"));
    for expected in ["<pdfuaid:part>1</pdfuaid:part>", "<pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion>", "/StructTreeRoot"] {
        assert!(pdf.contains(expected), "{expected}");
    }
    assert!(!pdf.contains("/Annots") && !pdf.contains("/S /Link"), "no link annotation and no link element");
}

// ── The brochure example ──────────────────────────────────────────────────────

/// The brochure of `examples/`, rendered unlicensed with what its `src` attributes name loaded, as an SDK loads them.
fn render_brochure() -> Vec<u8> {
    let dir = test_dir().join("../examples/5-brochure");
    let xml = String::from_utf8(read(&dir.join("document.xml"))).expect("UTF-8");
    let mut engine = LpdfEngine::new("");
    for (name, src) in crate::parse::font_srcs(&xml).expect("the example parses") {
        engine.load_font(&name, &read(&dir.join(src)));
    }
    for source in crate::source_files(&xml).expect("the example parses") {
        let bytes = read(&dir.join(&source.src));
        match source.kind {
            crate::SourceKind::Image => engine.load_image(&source.key, &bytes),
            crate::SourceKind::Profile => engine.load_profile(&source.key, &bytes),
            other => panic!("the brochure names no {other:?}"),
        }
    }
    engine.set_created_on(CREATED_ON);
    engine.render_xml(&xml, None, &RenderEncryption::default()).unwrap_or_else(|e| panic!("the brochure: {e}"))
}

#[test]
fn the_brochure_example_is_a_pdfx_file() {
    let pdf = text(&render_brochure());
    for expected in ["/S /GTS_PDFX", "/Info (FOGRA39L Coated)", "/TrimBox", "/DefaultRGB", "<pdfxid:GTS_PDFXVersion>PDF/X-4"] {
        assert!(pdf.contains(expected), "{expected}");
    }
    assert!(!pdf.contains("/Annots"), "its link prints as text");
    assert_eq!(boxes(&pdf, "TrimBox").len(), 2, "two pages");
}

// ── The samples of the external check ─────────────────────────────────────────

/// Writes the files `scripts/check-pdfx.mjs` hands to qpdf, when `LPDF_PDFX_OUT` names a folder: the fixture and the
/// brochure example, rendered unlicensed, so that the attribution line is checked too. Without the variable the test
/// does nothing, so a plain `cargo test` needs no other tool.
#[test]
fn writes_the_pdfx_sample_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_PDFX_OUT") else { return };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the samples' folder");
    std::fs::write(dir.join("pdfx-4.pdf"), render_fixture()).expect("a sample");
    std::fs::write(dir.join("pdfx-4-brochure.pdf"), render_brochure()).expect("a sample");
}
