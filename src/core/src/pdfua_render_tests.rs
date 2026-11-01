//! PDF/UA renders of whole documents, with fonts and pictures loaded as an SDK loads them: the structure a tagged
//! file gets, what it refuses, and the sample `make check-pdfua` hands to veraPDF and qpdf. The WASI crate has its
//! own `LpdfEngine`, so these run with the core library only.

use std::path::{Path, PathBuf};

use crate::{LpdfEngine, RenderEncryption};

const CREATED_ON: &str = "2026-10-31T09:30:00Z";

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// An unlicensed engine with the fonts and pictures the fixture declares, loaded under their names, as an SDK loads
/// what a document's `src` attributes name.
fn engine(created_on: Option<&str>) -> LpdfEngine {
    let mut engine = LpdfEngine::new("");
    let fonts = test_dir().join("assets/fonts");
    for (name, file) in [
        ("body", "CrimsonText-Regular.ttf"), ("heading", "CrimsonText-SemiBold.ttf"), ("sans", "Montserrat-Regular.ttf"),
    ] {
        engine.load_font(name, &read(&fonts.join(file)));
    }
    let images = test_dir().join("assets/images");
    for (name, file) in [("chart", "image-sample.png"), ("badge", "image-square.png")] {
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

/// The fixture `showcase-pdfua.xml` rendered as an SDK renders it, unlicensed: for the snapshot test.
pub(crate) fn render_fixture() -> Vec<u8> {
    let xml = String::from_utf8(read(&test_dir().join("fixtures/showcase-pdfua.xml"))).expect("UTF-8");
    render(&xml).unwrap_or_else(|e| panic!("showcase-pdfua: {e}"))
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// How many times `needle` is in `haystack`.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The content streams of `pdf`, inflated: where the marked content is.
fn contents(pdf: &[u8]) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(start) = find(&pdf[at..], b"stream\n").map(|i| at + i + 7) {
        let Some(end) = find(&pdf[start..], b"\nendstream").map(|i| start + i) else { break };
        if let Ok(inflated) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]) {
            out.push_str(&String::from_utf8_lossy(&inflated));
        }
        at = end;
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A PDF/UA document in the font `body`, with the title and the language it needs, and `layout` as its one layout.
fn document(layout: &str) -> String {
    format!(
        r#"<lpdf version="1"><assets><font name="body" src="body.ttf"/><image name="chart" src="chart.png"/></assets>
        <document font="body"><meta title="Report" lang="en"/><output pdfua="1"/>
        <section><layout>{layout}</layout></section></document></lpdf>"#
    )
}

// ── What a PDF/UA file gets ───────────────────────────────────────────────────

#[test]
fn the_catalog_says_the_file_is_tagged_in_a_language_and_shows_its_title() {
    let pdf = text(&render_fixture());
    for expected in [
        "/MarkInfo <<\n    /Marked true\n  >>", "/Lang (en-US)", "/StructTreeRoot", "/DisplayDocTitle true",
        "/ParentTree", "/ParentTreeNextKey", "/Tabs /S", "/StructParents", "/StructParent",
    ] {
        assert!(pdf.contains(expected), "{expected}");
    }
}

#[test]
fn the_xmp_packet_names_pdfua_part_1_and_the_dates_of_created_on() {
    let pdf = text(&render_fixture());
    assert!(pdf.contains("<pdfuaid:part>1</pdfuaid:part>"));
    assert!(pdf.contains("<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Annual Report — Accessible Copy</rdf:li></rdf:Alt></dc:title>"));
    assert!(pdf.contains("<xmp:CreateDate>2026-10-31T09:30:00Z</xmp:CreateDate>"));
    assert!(pdf.contains("/ModDate (D:20261031093000Z)"));
}

#[test]
fn each_thing_the_fixture_shows_has_its_structure_element() {
    let pdf = text(&render_fixture());
    for kind in [
        "Document", "H1", "H2", "H3", "P", "Table", "THead", "TBody", "TR", "TH", "TD", "L", "LI", "Lbl", "LBody",
        "Link", "Note", "TOC", "TOCI", "Figure",
    ] {
        assert!(pdf.contains(&format!("/S /{kind}\n")), "no {kind} element");
    }
    // The header cells head their columns, and the cells that span say so.
    assert_eq!(count(&pdf, "/Scope /Column"), 5, "the three header cells of the table and the two of the Markdown's");
    assert!(pdf.contains("/RowSpan 2") && pdf.contains("/ColSpan 2"));
    // The chart, the barcode and the map are figures with their descriptions; the badge is decoration.
    for alt in [
        "/Alt (A bar chart of the units sold in each quarter, rising from left to right)",
        "/Alt (QR code to pay this invoice)", "/Alt (A map of the two regions: North above South)",
    ] {
        assert!(pdf.contains(alt), "{alt}");
    }
    assert_eq!(count(&pdf, "/S /Figure\n"), 3);
    // A note has an id, and the ids are in the tree of ids.
    assert!(pdf.contains("/ID (note-1)") && pdf.contains("/IDTree"));
}

#[test]
fn a_link_is_described_by_its_words_and_found_from_its_element() {
    let pdf = text(&render_fixture());
    assert!(pdf.contains("/Contents (the project's site)"), "the link in a text");
    let wrapped = "/Contents (the method of the report, with the steps of each count, the returns of every region as they were \
        filed, and the checks made against the invoices of each quarter)";
    assert!(count(&pdf, wrapped) >= 2, "the link of the Markdown, which wraps: each part says all its words");
    assert!(pdf.contains("/Contents (Go to the details)"), "a link around a text");
    assert!(pdf.contains("/Contents (Lpdf)"), "the attribution's link");
    assert!(pdf.contains("/Type /OBJR"));
}

#[test]
fn the_decoration_is_marked_as_artifacts_and_the_text_with_mcids() {
    let content = contents(&render_fixture());
    assert!(count(&content, "/Artifact BMC") > 20, "the header, the footer, rules, borders, backgrounds and the band");
    assert!(content.contains("/H1 <<\n  /MCID 0\n>> BDC"), "the first heading is the page's first content");
    assert!(content.contains("/Figure <<"), "a figure is marked as a whole");
    let opened = count(&content, " BDC") + count(&content, " BMC");
    assert_eq!(opened, count(&content, "EMC"), "every marked sequence is closed");
}

#[test]
fn a_text_split_across_pages_stays_one_paragraph() {
    let long = "Words that fill the page. ".repeat(400);
    let pdf = text(&render(&document(&format!("<text>{long}</text>"))).expect("renders"));
    assert!(count(&pdf, "/Type /Page\n") >= 2, "the text runs onto a second page");
    assert_eq!(count(&pdf, "/S /P\n"), 1, "one paragraph, with marked content on each page");
}

#[test]
fn markdown_headings_carry_their_level() {
    let xml = r#"<lpdf version="1"><assets><font name="body" src="body.ttf"/></assets>
        <document font="body"><meta title="Report" lang="en"/><output pdfua="1"/>
        <section><markdown><markdown-content><![CDATA[# Top

## Under it

Words.
]]></markdown-content></markdown></section></document></lpdf>"#;
    let pdf = text(&render(xml).expect("renders"));
    assert!(pdf.contains("/S /H1\n") && pdf.contains("/S /H2\n") && pdf.contains("/S /P\n"));
}

#[test]
fn a_document_without_pdfua_is_not_tagged_whatever_its_new_attributes_say() {
    let plain = r#"<lpdf version="1"><assets><image name="chart" src="chart.png"/></assets>
        <document><meta title="Report" lang="en"/><section><layout>
        <text role="h1">Report</text><img name="chart" width="40pt" alt="A chart"/>
        </layout></section></document></lpdf>"#;
    let bare = r#"<lpdf version="1"><assets><image name="chart" src="chart.png"/></assets>
        <document><meta title="Report"/><section><layout>
        <text>Report</text><img name="chart" width="40pt"/>
        </layout></section></document></lpdf>"#;
    assert_eq!(render(plain).expect("renders"), render(bare).expect("renders"), "lang, role and alt change nothing");
    let pdf = text(&render(plain).expect("renders"));
    for absent in ["/StructTreeRoot", "/MarkInfo", "/Lang", "pdfuaid"] {
        assert!(!pdf.contains(absent), "{absent}");
    }
}

#[test]
fn pdfua_combines_with_pdfa_in_one_xmp_packet() {
    let xml = document(r#"<text role="h1">Report</text>"#).replace(r#"<output pdfua="1"/>"#, r#"<output pdfa="2u" pdfua="1"/>"#);
    let pdf = text(&render(&xml).expect("renders"));
    assert!(pdf.contains("<pdfaid:part>2</pdfaid:part>") && pdf.contains("<pdfuaid:part>1</pdfuaid:part>"));
    assert!(pdf.contains("<pdfaSchema:prefix>pdfuaid</pdfaSchema:prefix>") && pdf.contains("/StructTreeRoot"));
}

// ── What it refuses ───────────────────────────────────────────────────────────

#[test]
fn a_render_without_created_on_is_an_error_that_names_it() {
    let error = engine(None).render_xml(&document("<text>x</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.starts_with("<output pdfua=\"1\">") && error.contains("created_on"), "{error}");
}

#[test]
fn text_in_a_built_in_font_is_an_error_that_names_the_text() {
    let error = render(&document(r#"<text font="Helvetica">Plain words</text>"#)).unwrap_err();
    assert!(error.starts_with("<output pdfua=\"1\">") && error.contains("\"Plain words\"") && error.contains("PDF/UA"), "{error}");
}

#[test]
fn a_character_the_font_has_no_glyph_for_is_an_error_that_names_it() {
    let error = render(&document("<text>Price 日本</text>")).unwrap_err();
    assert!(error.starts_with("<output pdfua=\"1\">") && error.contains("'日' (U+65E5)") && error.contains("PDF/UA"), "{error}");
}

#[test]
fn a_font_whose_licence_forbids_embedding_is_an_error() {
    let mut engine = engine(Some(CREATED_ON));
    engine.load_font("body", &restricted(&read(&test_dir().join("assets/fonts/CrimsonText-Regular.ttf"))));
    let error = engine.render_xml(&document("<text>A</text>"), None, &RenderEncryption::default()).unwrap_err();
    assert!(error.starts_with("<output pdfua=\"1\">") && error.contains("'body'") && error.contains("does not allow embedding"), "{error}");
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
fn the_digits_of_a_barcode_are_an_error() {
    let barcode = |hrt: &str| document(&format!(r#"<barcode type="code128" data="ABC" height="40pt" alt="Order number"{hrt}/>"#));
    let error = render(&barcode(r#" hrt="true""#)).unwrap_err();
    assert!(error.starts_with("<output pdfua=\"1\">") && error.contains("hrt=\"false\""), "{error}");
    assert!(render(&barcode("")).is_ok(), "without its digits it renders");
}

#[test]
fn a_link_that_wraps_is_one_link_described_by_all_its_words() {
    let xml = document(r#"<column width="120pt"><text>See <span href="https://lpdf.io/docs">the guide to accessible documents on the site</span> for more.</text></column>"#);
    let pdf = text(&render(&xml).expect("renders"));
    // The render is unlicensed, so the attribution line has a link, and an element, of its own.
    let parts = count(&pdf, "/Subtype /Link") - 1;
    assert!(parts > 1, "the link wraps onto several lines, an annotation on each");
    assert_eq!(count(&pdf, "/S /Link\n"), 2, "one link element, and the attribution's");
    assert_eq!(count(&pdf, "/Contents (the guide to accessible documents on the site)"), parts);
}

#[test]
fn a_link_that_runs_onto_the_next_page_is_one_link_described_by_all_its_words() {
    let lines = "<text>A line.</text>".repeat(48);
    let words: Vec<String> = (0..150).map(|i| format!("word{i}")).collect();
    let xml = document(&format!(r#"{lines}<text>See <span href="https://lpdf.io/docs">{}</span> for more.</text>"#, words.join(" ")));
    let pdf = text(&render(&xml).expect("renders"));
    assert!(count(&pdf, "/Type /Page\n") >= 2, "the link runs onto the next page");
    let all = format!("/Contents ({})", words.join(" "));
    assert!(count(&pdf, &all) >= 2, "every part, on both pages, says all its words");
    assert_eq!(count(&pdf, "/Contents (word"), count(&pdf, &all), "no part says only its own page's words");
}

#[test]
fn a_link_around_a_text_that_runs_onto_the_next_page_is_one_link() {
    // Unlicensed, so the attribution line, with a link of its own, is drawn after page 1; and a footnote's note at the
    // foot of page 1. Neither splits the link.
    let lines = "<text>A line.</text>".repeat(48);
    let words: Vec<String> = (0..150).map(|i| format!("word{i}")).collect();
    let xml = document(&format!(
        r#"{lines}<link href="https://x.example"><text>{}<fn>A note.</fn></text></link>"#,
        words.join(" ")
    ));
    let pdf = text(&render(&xml).expect("renders"));
    let pages = count(&pdf, "/Type /Page\n");
    assert!(pages >= 2, "the link runs onto the next page");
    assert!(pdf.contains("/S /Note\n"));
    assert_eq!(count(&pdf, "/S /Link\n"), pages + 1, "one link, and the attribution's on each page");
    assert!(!pdf.contains("/Contents (https://x.example)"), "no part is described only by its URL");
    assert!(count(&pdf, &format!("/Contents ({}", words[0])) >= 2, "every part says the link's words");
}

#[test]
fn an_image_between_two_links_to_one_url_leaves_them_two_links_in_order() {
    let xml = document(
        r#"<link href="https://a.example"><img name="chart" alt="First" width="40pt"/></link>
        <img name="chart" alt="Middle" width="40pt"/>
        <link href="https://a.example"><img name="chart" alt="Last" width="40pt"/></link>"#,
    );
    let pdf = text(&render(&xml).expect("renders"));
    assert!(pdf.contains("/Contents (First)") && pdf.contains("/Contents (Last)"), "each link says its own figure");
    assert!(!pdf.contains("First Last"));
    let at = |alt: &str| pdf.find(&format!("/Alt ({alt})")).expect(alt);
    assert!(at("First") < at("Middle") && at("Middle") < at("Last"), "the figures are written in the order they are read");
}

#[test]
fn a_region_on_the_side_stays_in_the_reading_and_a_header_is_decoration() {
    let side = render(&document(r#"<region pin="left"><img name="chart" width="40pt"/></region><text>Body</text>"#)).unwrap_err();
    assert!(side.contains("the image 'chart' has no alt"), "a side region is read, so its image needs alt: {side}");
    assert!(render(&document(r#"<region pin="top"><img name="chart" width="40pt"/></region><text>Body</text>"#)).is_ok());
}

#[test]
fn an_image_without_alt_is_an_error_that_names_it() {
    let error = render(&document(r#"<img name="chart" width="40pt"/>"#)).unwrap_err();
    assert!(error.contains("the image 'chart' has no alt"), "{error}");
}

#[test]
fn an_encrypted_pdfua_file_must_allow_accessibility() {
    let xml = document("<text>x</text>").replace(r#"<output pdfua="1"/>"#, r#"<output pdfua="1" encrypt="aes-256" allow="print"/>"#);
    let error = render(&xml).unwrap_err();
    assert!(error.contains("add accessibility to allow"), "{error}");
}

/// Write the sample `make check-pdfua` validates with veraPDF and qpdf, into the folder `LPDF_PDFUA_OUT` names. It
/// does nothing without it, so `cargo test` writes no files.
#[test]
fn writes_pdfua_samples_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_PDFUA_OUT") else { return };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the samples' folder");
    std::fs::write(dir.join("pdfua-1.pdf"), render_fixture()).expect("a sample");
    let xml = String::from_utf8(read(&test_dir().join("fixtures/showcase-pdfua.xml"))).expect("UTF-8")
        .replace(r#"<output pdfua="1" />"#, r#"<output pdfa="2u" pdfua="1" />"#);
    std::fs::write(dir.join("pdfua-1-pdfa-2u.pdf"), render(&xml).expect("renders")).expect("a sample");
}
