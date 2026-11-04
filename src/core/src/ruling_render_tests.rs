//! Rulings: the layer a section that names one gets, read from XML and from a kit tree, the errors a document gets for
//! one, the drawing the PDF writes once for every page that draws it, and the ruled samples `make check-pdfa`,
//! `check-pdfua`, `check-pdfx` and `check-ocg` validate. The WASI crate has its own `LpdfEngine`, so these run with
//! the core library only.

use crate::canvas::CanvasPrimitive;
use crate::parse::{self, Section, SectionChild};
use crate::ruling::Drawing;
use crate::LpdfEngine;

const RULINGS: &str = r##"<rulings>
      <ruling name="graph" origin="margin">
        <lines step="5mm" color="#dde6f0" thickness="0.3pt"/>
        <lines step="25mm" color="#b5c4d6" thickness="0.6pt"/>
      </ruling>
      <ruling name="notes" edges="false"><lines step-y="8mm"/></ruling>
    </rulings>"##;

/// A document whose tokens hold `RULINGS` and whose `<document>` holds `sections`.
fn document(sections: &str) -> String {
    format!(r#"<lpdf version="1"><tokens>{RULINGS}</tokens><document>{sections}</document></lpdf>"#)
}

fn render(xml: &str) -> Vec<u8> {
    LpdfEngine::render_xml_to_pdf_bytes(xml).unwrap_or_else(|e| panic!("renders: {e}"))
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// Each stream of `pdf` that inflates, as text: the pages' content and the rulings' drawings.
pub(crate) fn streams(pdf: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = find(&pdf[at..], b"stream\n").map(|i| at + i + 7) {
        let Some(end) = find(&pdf[start..], b"\nendstream").map(|i| start + i) else { break };
        if let Ok(inflated) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]) {
            out.push(String::from_utf8_lossy(&inflated).into_owned());
        }
        // Past `endstream`, whose end is `stream` too.
        at = end + b"\nendstream".len();
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// How many times `needle` is in `haystack`.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// How many pages `pdf` has: each has one content stream.
fn page_count(pdf: &[u8]) -> usize {
    count(&text(pdf), "/Contents ")
}

/// The drawing of the ruling `section` draws: its first layer, which the ruling put at the bottom.
fn drawing_of(section: &Section) -> &Drawing {
    let SectionChild::Layer(layer) = &section.children[0] else { panic!("the ruling's layer comes first") };
    let CanvasPrimitive::Ruling(drawing) = &layer.children[0] else { panic!("the layer holds the ruling") };
    drawing
}

/// Enough lines of text for `pages` A4 pages in a layout of the default margin, which holds 57 and a half of them.
fn lines_for(pages: usize) -> String {
    (1..=pages * 58).map(|n| format!("<text>Line {n}</text>")).collect()
}

/// `xml`, a document of the fixtures, with `ruling` (a `<ruling>` named `name`) in its tokens and named by each of its
/// sections: the ruled samples of the standards' checks.
pub(crate) fn with_ruling(xml: &str, ruling: &str, name: &str) -> String {
    let rulings = format!("<rulings>{ruling}</rulings>");
    let xml = match xml.find("</tokens>") {
        Some(at) => format!("{}{rulings}{}", &xml[..at], &xml[at..]),
        None => {
            let at = xml.find("<document").expect("a document");
            format!("{}<tokens>{rulings}</tokens>{}", &xml[..at], &xml[at..])
        }
    };
    xml.replace("<section ", &format!(r#"<section ruling="{name}" "#)).replace("<section>", &format!(r#"<section ruling="{name}">"#))
}

/// Graph paper in two sets, inside the margin: what the samples of the standards draw.
const SAMPLE_RULING: &str = r##"<ruling name="sample" origin="margin">
    <lines step="5mm" color="#dde6f0" thickness="0.3pt"/><lines step="25mm" color="#b5c4d6" thickness="0.6pt"/></ruling>"##;

// ── Reading ───────────────────────────────────────────────────────────────────

#[test]
fn a_section_that_names_a_ruling_gets_its_lines_as_the_bottom_layer() {
    let xml = document(
        r##"<section ruling="graph"><layer><rect x="0" y="0" width="10pt" height="10pt" fill="#000"/></layer>
        <layout margin="40pt 30pt 50pt 20pt"><text>Hello</text></layout></section>"##,
    );
    let doc = parse::parse(&xml).unwrap();
    let section = &doc.sections[0];
    assert_eq!(section.children.len(), 3, "the ruling's layer, then the document's layer and its layout");
    let drawing = drawing_of(section);
    assert_eq!((drawing.width, drawing.height), (595.28, 841.89));
    assert_eq!(drawing.sets.len(), 2);
    assert_eq!(drawing.sets[0].area, [20.0, 40.0, 595.28 - 30.0, 841.89 - 50.0], "origin margin fills the layout's margin");
    let SectionChild::Layer(own) = &section.children[1] else { panic!("the document's own layer") };
    assert!(matches!(own.children[0], CanvasPrimitive::Rect(_)));
}

#[test]
fn a_section_that_names_no_ruling_has_no_lines() {
    let doc = parse::parse(&document("<section><layout><text>Hello</text></layout></section>")).unwrap();
    assert_eq!(doc.sections[0].children.len(), 1);
}

#[test]
fn a_markdown_takes_a_ruling_and_its_margin_is_the_one_origin_margin_fills() {
    let xml = document(
        r#"<section ruling="graph"><markdown margin="54pt"><markdown-content>Some text.</markdown-content></markdown></section>"#,
    );
    let doc = parse::parse(&xml).unwrap();
    assert_eq!(drawing_of(&doc.sections[0]).sets[0].area, [54.0, 54.0, 595.28 - 54.0, 841.89 - 54.0]);
}

#[test]
fn a_section_of_a_ruling_alone_is_a_page_of_paper() {
    let pdf = render(&document(r#"<section ruling="notes"/>"#));
    assert_eq!(page_count(&pdf), 1);
    assert!(streams(&pdf).iter().any(|s| s.contains("/R0 Do")), "the page draws the ruling");
}

#[test]
fn a_kit_tree_reads_rulings_as_the_xml_does() {
    let xml = r##"<lpdf version="1"><tokens><colors><color name="ink" value="#123456"/></colors><rulings>
        <ruling name="aid" origin="margin" edges="false" ocg="screen"><lines count-x="12" step-y="5mm" color="ink" thickness="xs"/></ruling>
        </rulings></tokens><document><section size="a5" ruling="aid"><layout margin="30pt"><text>Hi</text></layout></section></document></lpdf>"##;
    // The tree gives `edges` as a boolean and the count as a number, as an SDK writes them.
    let tree = r##"{"version":1,"type":"document","attrs":{"tokens":{"colors":{"ink":"#123456"},"rulings":{"aid":{
        "origin":"margin","edges":false,"ocg":"screen","lines":[{"count-x":12,"step-y":"5mm","color":"ink","thickness":"xs"}]}}}},
        "nodes":[{"type":"section","attrs":{"size":"a5","ruling":"aid"},"nodes":[{"type":"layout","attrs":{"margin":"30pt"},
        "nodes":[{"type":"text","nodes":["Hi"]}]}]}]}"##;
    let from_xml = parse::parse(xml).unwrap();
    let from_tree = parse::parse_tree(tree).unwrap();
    assert_eq!(drawing_of(&from_xml.sections[0]), drawing_of(&from_tree.sections[0]));
    let SectionChild::Layer(layer) = &from_tree.sections[0].children[0] else { panic!("a layer") };
    assert_eq!(layer.ocg, Some(parse::Ocg::Screen));
    let drawing = drawing_of(&from_tree.sections[0]);
    assert_eq!((drawing.sets[0].down.count, drawing.sets[0].color.as_str()), (11, "#123456"), "12 columns without their edges");
}

#[test]
fn the_tree_written_as_xml_keeps_its_rulings() {
    let tree = r##"{"version":1,"type":"document","attrs":{"tokens":{"rulings":{"aid":{"origin":"margin","edges":false,
        "lines":[{"count-x":12,"step-y":"5mm"},{"step":"25mm","color":"#ff0000"}]}}}},
        "nodes":[{"type":"section","attrs":{"ruling":"aid"},"nodes":[{"type":"layout","nodes":[]}]}]}"##;
    let xml = crate::kit_to_xml::kit_to_xml(tree).unwrap();
    assert!(xml.contains(r#"<ruling name="aid" origin="margin" edges="false">"#), "{xml}");
    assert!(xml.contains(r#"<lines count-x="12" step-y="5mm"/>"#), "{xml}");
    assert!(xml.contains(r##"<lines color="#ff0000" step="25mm"/>"##), "{xml}");
    assert!(xml.contains(r#"<section ruling="aid">"#), "{xml}");
    let again = parse::parse(&xml).unwrap();
    assert_eq!(drawing_of(&again.sections[0]), drawing_of(&parse::parse_tree(tree).unwrap().sections[0]));
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[test]
fn an_unknown_ruling_is_an_error_that_lists_the_declared_ones() {
    let error = parse::parse(&document(r#"<section ruling="grph"><layout/></section>"#)).unwrap_err();
    assert!(error.starts_with("<section> at line "), "{error}");
    assert!(error.ends_with(": unknown ruling 'grph': the tokens declare graph, notes"), "{error}");
    let tree = r#"{"version":1,"type":"document","nodes":[{"type":"section","attrs":{"ruling":"graph"},"nodes":[{"type":"layout"}]}]}"#;
    assert_eq!(
        parse::parse_tree(tree).unwrap_err(),
        "section: unknown ruling 'graph': the tokens declare no rulings; add it to <rulings>"
    );
}

#[test]
fn a_step_and_a_count_in_one_direction_is_an_error_where_the_lines_are_written() {
    let xml = r#"<lpdf version="1"><tokens><rulings><ruling name="bad">
        <lines step-x="5mm" count-x="12"/></ruling></rulings></tokens><document><section><layout/></section></document></lpdf>"#;
    let error = parse::parse(xml).unwrap_err();
    assert!(error.starts_with("<lines> at line 2, col 9: ruling 'bad': step-x and count-x both space"), "{error}");
    assert!(error.ends_with("so remove one of them"), "{error}");
}

#[test]
fn a_count_above_a_hundred_is_an_error_that_gives_the_range() {
    let xml = r#"<lpdf version="1"><tokens><rulings><ruling name="fine"><lines count-x="150"/></ruling></rulings></tokens>
        <document><section><layout/></section></document></lpdf>"#;
    let error = parse::parse(xml).unwrap_err();
    assert!(error.ends_with(r#": count-x must be a whole number from 1 to 100, not "150""#), "{error}");
    let tree = r#"{"version":1,"type":"document","attrs":{"tokens":{"rulings":{"fine":{"lines":[{"count-x":150}]}}}},
        "nodes":[{"type":"section","nodes":[{"type":"layout"}]}]}"#;
    assert_eq!(parse::parse_tree(tree).unwrap_err(), "lines: count-x must be a whole number from 1 to 100, not 150");
}

#[test]
fn a_ruling_without_lines_is_an_error() {
    let xml = r#"<lpdf version="1"><tokens><rulings><ruling name="empty"/></rulings></tokens>
        <document><section><layout/></section></document></lpdf>"#;
    let error = parse::parse(xml).unwrap_err();
    assert!(error.contains("ruling 'empty': it has no <lines>"), "{error}");
}

#[test]
fn a_count_that_cuts_parts_narrower_than_the_floor_is_an_error_on_the_section() {
    // Across the whole of a page 300pt wide, a hundred parts are 3pt wide, which is enough.
    let page = r#"<lpdf version="1"><tokens><rulings><ruling name="fine"><lines count-x="100"/></ruling></rulings></tokens>
        <document><section size="300pt 420pt" ruling="fine"><layout/></section></document></lpdf>"#;
    assert!(parse::parse(page).is_ok());
    let xml = r#"<lpdf version="1"><tokens><rulings><ruling name="fine" origin="margin"><lines count-x="100"/></ruling></rulings></tokens>
        <document><section size="300pt 420pt" ruling="fine"><layout margin="36pt"/></section></document></lpdf>"#;
    let error = parse::parse(xml).unwrap_err();
    assert!(error.starts_with("<section> at line 2, col "), "{error}");
    assert!(
        error.ends_with(
            ": ruling 'fine': a count of 100 cuts the area inside the margin into columns 2.28pt wide, narrower than the \
             2.5pt a ruling allows: give a count of 91 or less in x"
        ),
        "{error}"
    );
}

#[test]
fn a_cmyk_ruling_is_refused_in_a_pdfa_file_and_drawn_in_cmyk_otherwise() {
    let ruling = r#"<ruling name="print"><lines step="5mm" color="cmyk(0 0 0 30)"/></ruling>"#;
    let plain = with_ruling(r#"<lpdf version="1"><document><section><layout/></section></document></lpdf>"#, ruling, "print");
    let pdf = render(&plain);
    assert!(streams(&pdf).iter().any(|s| s.contains("0 0 0 0.3 K")), "the lines are stroked in CMYK");
    let pdfa = with_ruling(&crate::pdfa_render_tests::fixture("2b"), ruling, "print");
    let error = crate::pdfa_render_tests::render(&pdfa).unwrap_err();
    assert!(error.contains("CMYK"), "{error}");
}

// ── Writing ───────────────────────────────────────────────────────────────────

#[test]
fn the_lines_are_written_once_and_every_page_of_the_section_invokes_them() {
    let pdf = render(&document(&format!(r#"<section ruling="graph"><layout>{}</layout></section>"#, lines_for(3))));
    let pages = page_count(&pdf);
    assert!(pages >= 3, "{pages} pages");
    assert_eq!(count(&text(&pdf), "/Subtype /Form"), 1, "one drawing for every page");
    let streams = streams(&pdf);
    assert_eq!(streams.iter().map(|s| count(s, "/R0 Do")).sum::<usize>(), pages, "each page invokes it once");
    assert!(!text(&pdf).contains("/R1 "), "no second drawing");
}

#[test]
fn sections_of_one_ruling_on_pages_of_one_size_share_a_drawing_and_another_size_gets_its_own() {
    let same = render(&document(r#"<section ruling="notes"><layout/></section><section ruling="notes"><layout/></section>"#));
    assert!(text(&same).contains("/R0 ") && !text(&same).contains("/R1 "), "one drawing for both");
    let sizes = render(&document(r#"<section ruling="notes"><layout/></section><section size="a5" ruling="notes"><layout/></section>"#));
    assert!(text(&sizes).contains("/R1 "), "A5 has a drawing of its own");
    let all: String = streams(&sizes).concat();
    assert_eq!((count(&all, "/R0 Do"), count(&all, "/R1 Do")), (1, 1));
}

#[test]
fn the_drawing_is_the_trimmed_page_so_no_line_reaches_into_a_bleed() {
    let pdf = text(&render(&document(r#"<section ruling="notes" bleed="5mm"><layout/></section>"#)));
    assert!(pdf.contains("/MediaBox [-14.173228 -14.173228 609.45325 856.06323]"), "the sheet grows by the bleed");
    assert!(pdf.contains("/BBox [0 0 595.28 841.89]"), "the drawing keeps to the trimmed page: {pdf}");
}

#[test]
fn a_ruling_is_under_everything_the_section_draws_and_over_its_background() {
    let xml = document(r##"<section ruling="notes" background="#eeeeee"><layout><text>Hello</text></layout></section>"##);
    let page = streams(&render(&xml)).into_iter().find(|s| s.contains("/R0 Do")).expect("the page");
    let (background, ruling, hello) = (page.find(" re\n").unwrap(), page.find("/R0 Do").unwrap(), page.find("Tj").unwrap());
    assert!(background < ruling && ruling < hello, "{page}");
}

#[test]
fn a_dense_ruling_costs_no_more_than_a_coarse_one() {
    let section = |step: &str| {
        format!(
            r#"<lpdf version="1"><tokens><rulings><ruling name="r"><lines step="{step}"/></ruling></rulings></tokens>
            <document><section ruling="r"><layout>{}</layout></section></document></lpdf>"#,
            lines_for(100)
        )
    };
    let (dense, coarse) = (render(&section("2.5pt")), render(&section("5mm")));
    let pages = page_count(&dense);
    assert!(pages >= 100, "{pages} pages");
    let per_page = (dense.len() as f64 - coarse.len() as f64) / pages as f64;
    assert!(per_page.abs() <= 4.0, "2.5pt costs {per_page:.1} bytes a page more than 5mm: {} against {}", dense.len(), coarse.len());
}

#[test]
fn a_ruling_in_a_group_is_marked_with_it_and_a_pdfa_file_shows_it() {
    let ruling = r##"<ruling name="aid" ocg="screen"><lines count="12" color="#0047ab"/></ruling>"##;
    let plain = with_ruling(r#"<lpdf version="1"><document><section><layout/></section></document></lpdf>"#, ruling, "aid");
    let page = streams(&render(&plain)).into_iter().find(|s| s.contains("/R0 Do")).expect("the page");
    let marked = page.find("/OC /OCScreen BDC").expect("the lines are in the screen group");
    assert!(marked < page.find("/R0 Do").unwrap(), "{page}");
    let pdfa = with_ruling(&crate::pdfa_render_tests::fixture("2u"), ruling, "aid");
    let pdf = crate::pdfa_render_tests::render(&pdfa).unwrap();
    assert!(!text(&pdf).contains("/OCProperties"), "a PDF/A file shows everything");
    assert!(streams(&pdf).iter().any(|s| s.contains("/R0 Do")));
}

#[test]
fn a_tagged_pdf_marks_the_lines_as_decoration() {
    let xml = with_ruling(&crate::pdfua_render_tests::fixture(), SAMPLE_RULING, "sample");
    let pdf = crate::pdfua_render_tests::render(&xml).expect("renders");
    let page = streams(&pdf).into_iter().find(|s| s.contains("/R0 Do")).expect("a page");
    let ruling = page.find("/R0 Do").unwrap();
    let opened = page[..ruling].rfind("/Artifact").expect("an artifact around the lines");
    assert!(!page[opened..ruling].contains("EMC"), "the artifact is still open where the lines are drawn: {page}");
}

#[test]
fn a_linearized_file_names_the_drawing_on_the_pages_that_draw_it() {
    let xml = document(r#"<output linearize="true"/><section><layout><text>Plain</text></layout></section><section ruling="notes"><layout/></section>"#);
    let pdf = text(&render(&xml));
    assert_eq!(count(&pdf, "/R0 "), 1, "only the ruled page's resources name it");
}

// ── Samples for the external checks ──────────────────────────────────────────

/// The PDF/A-2u fixture with graph paper on each of its sections, for `make check-pdfa`.
pub(crate) fn pdfa_sample() -> Vec<u8> {
    crate::pdfa_render_tests::render(&with_ruling(&crate::pdfa_render_tests::fixture("2u"), SAMPLE_RULING, "sample")).expect("renders")
}

/// The PDF/UA fixture with graph paper on each of its sections, for `make check-pdfua`.
pub(crate) fn pdfua_sample() -> Vec<u8> {
    crate::pdfua_render_tests::render(&with_ruling(&crate::pdfua_render_tests::fixture(), SAMPLE_RULING, "sample")).expect("renders")
}

/// The PDF/X fixture with graph paper on each of its sections, which bleed, for `make check-pdfx`.
pub(crate) fn pdfx_sample() -> Vec<u8> {
    crate::pdfx_render_tests::render(&with_ruling(&crate::pdfx_render_tests::fixture(), SAMPLE_RULING, "sample")).expect("renders")
}

/// A page with a grid in the screen group, in the color `make check-ocg` reads as screen content, under content of
/// both groups: the grid must show on screen and not in print. The lines are thick, so that their pixels keep the
/// color whole at the scale pdf.js renders.
pub(crate) fn ocg_sample() -> Vec<u8> {
    let ruling = r##"<ruling name="aid" origin="margin" ocg="screen"><lines count="12" color="#0047ab" thickness="2pt"/></ruling>"##;
    let xml = with_ruling(r#"<lpdf version="1"><document><section><layout margin="40pt"></layout></section></document></lpdf>"#, ruling, "aid");
    render(&crate::ocg_render_tests::with_groups(&xml))
}
