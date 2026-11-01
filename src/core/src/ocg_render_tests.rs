//! Optional content (`ocg`): the groups a document writes and where its content is marked, the links and fields in
//! print content, the groups a PDF/A, PDF/UA or PDF/X file leaves out, and the samples `make check-ocg` hands to
//! pdf.js. The WASI crate has its own `LpdfEngine`, so these run with the core library only.

use std::path::{Path, PathBuf};

use crate::parse::Ocg;
use crate::render::{RenderNode, RenderPage};
use crate::{layout, parse, LpdfEngine};

/// The colors of the fixtures: what is printed, what is shown on screen, and what is in both groups at once.
const PRINT: &str = "#c00000";
const SCREEN: &str = "#0047ab";
const NEVER: &str = "#008a00";

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn fixture(name: &str) -> String {
    let path = test_dir().join(format!("fixtures/{name}.xml"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn render(xml: &str) -> Result<Vec<u8>, String> {
    LpdfEngine::render_xml_to_pdf_bytes(xml)
}

/// The fixture `<name>.xml` rendered unlicensed, with built-in fonts: for the snapshot tests.
pub(crate) fn render_fixture(name: &str) -> Vec<u8> {
    render(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A document whose one layout holds `flow`, and whose section holds `layers` after it.
fn document(flow: &str, layers: &str) -> String {
    format!(r#"<lpdf version="1"><document><section><layout>{flow}</layout>{layers}</section></document></lpdf>"#)
}

/// The pages `xml` lays out.
fn pages(xml: &str) -> Vec<RenderPage> {
    let mut doc = parse::parse(xml).unwrap_or_else(|e| panic!("parses: {e}"));
    layout::layout_document(&doc.section_layouts())
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// The content streams of `pdf`, inflated, one after the other: where the marked content is.
fn contents(pdf: &[u8]) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(start) = find(&pdf[at..], b"stream\n").map(|i| at + i + 7) {
        let Some(end) = find(&pdf[start..], b"\nendstream").map(|i| start + i) else { break };
        if let Ok(inflated) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]) {
            out.push_str(&String::from_utf8_lossy(&inflated));
            out.push('\n');
        }
        at = end;
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Each text drawn in `pages`, with its color and the groups it is drawn in, outermost first.
fn texts_in_groups(pages: &[RenderPage]) -> Vec<(String, String, Vec<Ocg>)> {
    fn walk(nodes: &[RenderNode], groups: &mut Vec<Ocg>, out: &mut Vec<(String, String, Vec<Ocg>)>) {
        for node in nodes {
            match node {
                RenderNode::Text(t) => out.push((t.content.clone(), t.color.clone(), groups.clone())),
                RenderNode::Box(b) => {
                    groups.extend(b.ocg);
                    walk(&b.children, groups, out);
                    if b.ocg.is_some() {
                        groups.pop();
                    }
                }
                RenderNode::Link(l)        => walk(&l.children, groups, out),
                RenderNode::CanvasLayer(l) => walk(&l.children, groups, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for page in pages {
        walk(&page.nodes, &mut Vec::new(), &mut out);
    }
    out
}

/// Every text of the fixture `name` is drawn in the groups its color says: red in print content, blue in screen
/// content, green in both, and any other color in none.
fn assert_groups_follow_the_colors(name: &str) {
    let texts = texts_in_groups(&pages(&fixture(name)));
    assert!(texts.iter().any(|(_, color, _)| color == PRINT), "{name} has print content");
    for (content, color, groups) in texts {
        let mut expected = match color.as_str() {
            PRINT  => vec![Ocg::Print],
            SCREEN => vec![Ocg::Screen],
            NEVER  => vec![Ocg::Print, Ocg::Screen],
            _      => vec![],
        };
        let mut found = groups.clone();
        found.sort_by_key(|g| *g as u8);
        found.dedup();
        expected.sort_by_key(|g| *g as u8);
        assert_eq!(found, expected, "{name}: '{content}' in {color} is drawn in {groups:?}");
    }
}

// ── What a document writes ────────────────────────────────────────────────────

#[test]
fn a_document_without_ocg_writes_no_optional_content() {
    let pdf = render(&document("<text>Plain</text>", "")).unwrap();
    assert!(!text(&pdf).contains("/OCProperties"));
    assert!(!contents(&pdf).contains("/OC "));
}

#[test]
fn print_and_screen_content_is_marked_with_its_group() {
    let pdf = render(&document(r#"<text>A</text><text ocg="print">B</text><text ocg="screen">C</text>"#, "")).unwrap();
    let marked = contents(&pdf);
    let print = marked.find("/OC /OCPrint BDC").expect("print content is marked");
    let screen = marked.find("/OC /OCScreen BDC").expect("screen content is marked");
    assert!(print < screen, "in the order they are drawn");
    assert_eq!(marked.matches(" BDC").count(), marked.matches("EMC").count());
    let file = text(&pdf);
    let properties = &file[file.find("/Properties").expect("the page's resources name the groups")..];
    let properties = &properties[..properties.find(">>").unwrap()];
    assert!(properties.contains("/OCPrint ") && properties.contains("/OCScreen "), "{properties}");
}

#[test]
fn the_groups_start_on_and_switch_by_their_usage_when_viewed_and_printed() {
    let pdf = text(&render(&document(r#"<text ocg="print">B</text><text ocg="screen">C</text>"#, "")).unwrap());
    let properties = &pdf[pdf.find("/OCProperties").expect("the catalog lists the groups")..];
    assert!(properties.contains("/BaseState /ON"), "{properties}");
    assert!(properties.contains("/Event /View") && properties.contains("/Event /Print"), "{properties}");
    assert!(properties.contains("/Category [/View]") && properties.contains("/Category [/Print]"), "{properties}");
    let print = &pdf[pdf.find("(Print only)").expect("the print group")..];
    assert!(print.contains("/PrintState /ON") && print.contains("/ViewState /OFF"), "{print}");
    let screen = &pdf[pdf.find("(Screen only)").expect("the screen group")..];
    assert!(screen.contains("/PrintState /OFF") && screen.contains("/ViewState /ON"), "{screen}");
}

#[test]
fn only_the_groups_a_document_draws_in_are_written_in_the_order_it_first_draws_them() {
    let screen_only = text(&render(&document(r#"<text ocg="screen">C</text>"#, "")).unwrap());
    assert!(screen_only.contains("(Screen only)") && !screen_only.contains("(Print only)"));

    let both = text(&render(&document(r#"<text ocg="screen">C</text><text ocg="print">B</text>"#, "")).unwrap());
    assert!(both.find("/OCScreen").unwrap() < both.find("/OCPrint").unwrap(), "{both}");
}

#[test]
fn the_same_document_gives_the_same_file() {
    let xml = fixture("showcase-ocg");
    assert_eq!(render(&xml).unwrap(), render(&xml).unwrap());
}

#[test]
fn a_group_inside_a_group_is_marked_inside_it() {
    let pdf = render(&document(r#"<column ocg="print"><text ocg="screen">D</text></column>"#, "")).unwrap();
    let marked = contents(&pdf);
    let outer = marked.find("/OC /OCPrint BDC").unwrap();
    let inner = marked.find("/OC /OCScreen BDC").unwrap();
    let first_end = marked.find("EMC").unwrap();
    assert!(outer < inner && inner < first_end, "{marked}");
}

#[test]
fn hidden_content_keeps_its_room() {
    let with = pages(&document(r#"<text ocg="print">B</text><text>After</text>"#, ""));
    let without = pages(&document(r#"<text>B</text><text>After</text>"#, ""));
    let after = |pages: &[RenderPage]| {
        texts_in_groups(pages).into_iter().find(|(content, ..)| content == "After").map(|(content, ..)| content)
    };
    assert_eq!(after(&with), after(&without));
    let y = |pages: &[RenderPage]| -> f32 {
        fn find_y(nodes: &[RenderNode]) -> Option<f32> {
            nodes.iter().find_map(|node| match node {
                RenderNode::Text(t) if t.content == "After" => Some(t.y),
                RenderNode::Box(b) => find_y(&b.children),
                _ => None,
            })
        }
        find_y(&pages[0].nodes).expect("the text after")
    };
    assert_eq!(y(&with), y(&without));
}

// ── Every element, and every part of it ──────────────────────────────────────

#[test]
fn every_text_of_the_fixture_is_drawn_in_the_groups_its_color_says() {
    assert_groups_follow_the_colors("showcase-ocg");
}

#[test]
fn every_part_of_content_split_across_pages_or_repeated_on_each_keeps_its_group() {
    let pages = pages(&fixture("showcase-ocg-split"));
    assert_eq!(pages.len(), 3, "the fixture splits its print box and repeats its header row and footer");
    assert_groups_follow_the_colors("showcase-ocg-split");
    // The footer on each page, and the header row on each page the table runs onto.
    for (i, page) in pages.iter().enumerate() {
        let texts = texts_in_groups(std::slice::from_ref(page));
        assert!(texts.iter().any(|(c, _, g)| c.starts_with("Printed page") && g == &[Ocg::Print]), "page {}", i + 1);
        assert!(texts.iter().any(|(c, _, g)| c.starts_with("Page") && g == &[Ocg::Screen]), "page {}", i + 1);
    }
    let header_rows = texts_in_groups(&pages).into_iter().filter(|(c, ..)| c.starts_with("Payment")).count();
    assert_eq!(header_rows, 2, "the header row on both pages of the table");
}

#[test]
fn a_layer_and_what_it_draws_take_a_group() {
    let layers = r##"<layer ocg="print"><rect x="10pt" y="10pt" width="20pt" height="20pt" fill="#ff0000"/></layer>
        <layer><circle x="40pt" y="10pt" r="10pt" ocg="screen"/><line x1="0pt" y1="0pt" x2="9pt" y2="9pt" ocg="print"/>
        <ellipse x="70pt" y="10pt" rx="9pt" ry="5pt" ocg="print"/><path d="M 0 0 L 5 5" stroke="#000000" ocg="screen"/>
        <text-at x="10pt" y="60pt" ocg="screen">Placed</text-at></layer>"##;
    let pages = pages(&document("<text>Flow</text>", layers));
    let groups: Vec<Option<Ocg>> = pages[0].nodes.iter().filter_map(|node| match node {
        RenderNode::Box(b) if b.ocg.is_some() || matches!(b.children.first(), Some(RenderNode::CanvasLayer(_))) => Some(b.ocg),
        RenderNode::CanvasLayer(_) => Some(None),
        _ => None,
    }).collect();
    assert_eq!(groups, vec![Some(Ocg::Print), None], "the first layer is in print content, the second in none");
    let RenderNode::CanvasLayer(second) = pages[0].nodes.last().unwrap() else { panic!("the second layer") };
    let inner: Vec<Option<Ocg>> = second.children.iter().map(|node| match node {
        RenderNode::Box(b) => b.ocg,
        _ => None,
    }).collect();
    assert_eq!(inner, vec![Some(Ocg::Screen), Some(Ocg::Print), Some(Ocg::Print), Some(Ocg::Screen), Some(Ocg::Screen)]);
}

#[test]
fn a_table_row_in_a_group_has_its_background_and_cells_in_it() {
    let pages = pages(&document(
        r##"<table cols="1fr"><tr><td><text>One</text></td></tr><tr ocg="print" background="#eeeeee"><td><text>Two</text></td></tr></table>"##,
        "",
    ));
    let texts = texts_in_groups(&pages);
    let group_of = |name: &str| texts.iter().find(|(c, ..)| c == name).map(|(_, _, g)| g.clone()).unwrap();
    assert_eq!(group_of("One"), vec![]);
    assert_eq!(group_of("Two"), vec![Ocg::Print]);
    let marked = contents(&render(&document(
        r##"<table cols="1fr"><tr ocg="print" background="#eeeeee"><td><text>Two</text></td></tr></table>"##, "",
    )).unwrap());
    let print = marked.find("/OC /OCPrint BDC").unwrap();
    assert!(print < marked.find(" re").unwrap(), "the row's background is in the group: {marked}");
}

// ── Links and fields in print content ─────────────────────────────────────────

#[test]
fn a_link_in_print_content_writes_no_annotation_and_one_in_screen_content_does() {
    let pdf = text(&render(&document(
        r#"<link href="https://print.example"><text ocg="print">Printed</text></link>
           <text ocg="print">Words <span href="https://words.example">linked</span></text>
           <column ocg="print"><link href="https://deep.example"><text>Deep</text></link></column>
           <link href="https://screen.example" ocg="screen"><text>On screen</text></link>"#,
        "",
    )).unwrap());
    assert!(pdf.contains("(https://screen.example)"), "a link on screen is clickable");
    for url in ["https://words.example", "https://deep.example"] {
        assert!(!pdf.contains(&format!("({url})")), "{url} is in print content and is not a link");
    }
    // The link around print content is outside the group, and stays a link: its content is what is hidden.
    assert!(pdf.contains("(https://print.example)"));
}

#[test]
fn a_form_field_in_print_content_is_an_error_that_points_to_a_frame() {
    for flow in [
        r#"<field type="text" name="amount" ocg="print"/>"#,
        r#"<column ocg="print"><row><field type="text" name="amount"/></row></column>"#,
        r#"<column ocg="print"><column ocg="screen"><field type="text" name="amount"/></column></column>"#,
    ] {
        let err = render(&document(flow, "")).expect_err(flow);
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("'amount'"), "{err}");
        assert!(err.contains("ocg=\"print\""), "{err}");
        assert!(err.contains("<frame>"), "{err}");
    }
}

#[test]
fn a_form_field_in_screen_content_is_filled_on_screen() {
    let pdf = text(&render(&document(r#"<column ocg="screen"><field type="text" name="amount"/></column>"#, "")).unwrap());
    assert!(pdf.contains("/T (amount)"), "the field is written");
}

#[test]
fn a_tree_reads_ocg_and_refuses_a_field_in_print_content() {
    let tree = |flow: &str| format!(
        r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","nodes":[{flow}]}}]}}]}}"#
    );
    let mut doc = parse::parse_tree(&tree(r#"{"type":"text","attrs":{"ocg":"screen"},"nodes":["On screen"]}"#)).unwrap();
    assert_eq!(doc.section_layouts()[0].children[0].ocg, Some(Ocg::Screen));

    let Err(err) = parse::parse_tree(&tree(r#"{"type":"column","attrs":{"ocg":"print"},"nodes":[{"type":"field","attrs":{"type":"text","name":"amount"}}]}"#))
    else {
        panic!("a field in print content is an error");
    };
    assert!(err.contains("'amount'") && err.contains("<frame>"), "{err}");

    let Err(err) = parse::parse_tree(&tree(r#"{"type":"text","attrs":{"ocg":"paper"},"nodes":["A"]}"#)) else {
        panic!("paper is not a group");
    };
    assert!(err.contains("invalid ocg value: 'paper'"), "{err}");
}

#[test]
fn an_ocg_other_than_print_or_screen_is_an_error_that_names_it() {
    for (flow, layers) in [
        (r#"<text ocg="paper">A</text>"#, ""),
        (r#"<column ocg="Print"><text>A</text></column>"#, ""),
        ("", r#"<layer ocg="paper"><rect x="0pt" y="0pt" width="9pt" height="9pt"/></layer>"#),
        ("", r#"<layer><rect x="0pt" y="0pt" width="9pt" height="9pt" ocg="paper"/></layer>"#),
        ("", r#"<layer><text-at x="0pt" y="0pt" ocg="paper">A</text-at></layer>"#),
    ] {
        let err = render(&document(flow, layers)).expect_err(flow);
        assert!(err.contains("ocg") && (err.contains("'paper'") || err.contains("'Print'")), "{err}");
        assert!(err.contains("'print'") && err.contains("'screen'"), "the error says what is allowed: {err}");
    }
}

// ── PDF/A, PDF/UA and PDF/X show everything ──────────────────────────────────

/// What `with_groups` adds to a document's first layout: content of each group, a link in print content, and a field
/// in screen content where the standard allows one.
const GROUPED_FLOW: &str = r##"
        <text ocg="print" color="#c00000" font-size="16pt">Printed, and shown on screen too in this file</text>
        <text ocg="screen" color="#0047ab" font-size="16pt">Shown on screen, and printed too in this file</text>
        <text ocg="print" color="#c00000">Terms: <span href="https://example.com/terms" color="#c00000">example.com/terms</span></text>"##;
/// What `with_groups` adds after a document's first layout: a layer of each group.
const GROUPED_LAYERS: &str = r##"
      <layer ocg="print"><rect anchor="bottom-left" x="48pt" y="-24pt" width="24pt" height="12pt" fill="#c00000"/></layer>
      <layer><rect anchor="bottom-left" x="80pt" y="-24pt" width="24pt" height="12pt" fill="#0047ab" ocg="screen"/></layer>"##;

/// `xml`, a document of the fixtures, with content in groups at the start of its first layout and layers in groups
/// after it, in the colors `make check-ocg` looks for.
pub(crate) fn with_groups(xml: &str) -> String {
    let layout = xml.find("<layout").expect("a layout");
    let open_end = layout + xml[layout..].find('>').expect("the layout's tag") + 1;
    let close = xml.find("</layout>").expect("the layout's end") + "</layout>".len();
    format!("{}{GROUPED_FLOW}{}{GROUPED_LAYERS}{}", &xml[..open_end], &xml[open_end..close], &xml[close..])
}

/// A file that shows everything: no optional content at all, and the link in print content clickable, as in a
/// document without groups. A PDF/X file has no links at all.
fn assert_shows_everything(pdf: &[u8], links: bool) {
    let file = text(pdf);
    assert!(!file.contains("/OCProperties") && !file.contains("/OCG"), "no group is written");
    assert!(!contents(pdf).contains("/OC /"), "no content is marked");
    assert_eq!(file.contains("(https://example.com/terms)"), links, "the link in print content is a link as any other");
}

#[test]
fn a_pdfa_file_shows_everything() {
    let xml = with_groups(&fixture("showcase-pdfa-2u"));
    assert_shows_everything(&crate::pdfa_render_tests::render(&xml).expect("renders"), true);
}

#[test]
fn a_pdfua_file_shows_everything_and_tags_it_as_usual() {
    let xml = with_groups(&fixture("showcase-pdfua"));
    let pdf = crate::pdfua_render_tests::render(&xml).expect("renders");
    assert_shows_everything(&pdf, true);
    // Tagged as the same document without groups is.
    let plain = crate::pdfua_render_tests::render(&xml.replace(r#" ocg="print""#, "").replace(r#" ocg="screen""#, "")).unwrap();
    assert_eq!(pdf, plain);
}

#[test]
fn a_pdfx_file_shows_everything() {
    let xml = with_groups(&fixture("showcase-pdfx"));
    assert_shows_everything(&crate::pdfx_render_tests::render(&xml).expect("renders"), false);
}

#[test]
fn a_field_in_print_content_is_an_error_with_pdfa_too() {
    let xml = r#"<lpdf version="1"><document><output pdfa="2b"/><section><layout>
        <column ocg="print"><field type="text" name="amount"/></column></layout></section></document></lpdf>"#;
    let err = render(xml).expect_err("a field in print content");
    assert!(err.contains("<frame>"), "the template is wrong whatever the output: {err}");
}

// ── Samples for the external check ───────────────────────────────────────────

/// Write the samples `make check-ocg` renders with pdf.js, to the folder `LPDF_OCG_OUT` names: the two fixtures,
/// which switch their groups, and a PDF/A, a PDF/UA and a PDF/X file with groups, which show everything. Without
/// `LPDF_OCG_OUT` it writes nothing.
#[test]
fn writes_ocg_samples_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_OCG_OUT") else { return };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the samples' folder");
    let write = |name: &str, pdf: Vec<u8>| std::fs::write(dir.join(name), pdf).expect("a sample");
    write("ocg.pdf", render_fixture("showcase-ocg"));
    write("ocg-split.pdf", render_fixture("showcase-ocg-split"));
    write("ocg-pdfa-2u.pdf", crate::pdfa_render_tests::render(&with_groups(&fixture("showcase-pdfa-2u"))).expect("renders"));
    write("ocg-pdfua-1.pdf", crate::pdfua_render_tests::render(&with_groups(&fixture("showcase-pdfua"))).expect("renders"));
    write("ocg-pdfx.pdf", crate::pdfx_render_tests::render(&with_groups(&fixture("showcase-pdfx"))).expect("renders"));
}
