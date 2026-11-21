//! The page an `orientation` gives, on a document and on a section, rendered from the XML and from the kit tree of the
//! same document: it names the shape of the page, `portrait` taller than wide and `landscape` wider than tall,
//! whatever size the page starts from. The WASI crate has its own `LpdfEngine`, so these run with the core library
//! only.

use crate::{LpdfEngine, RenderEncryption};

const A4: (f32, f32) = (595.28, 841.89);
const A3: (f32, f32) = (841.89, 1190.55);
const MM: f32 = 72.0 / 25.4;

/// The width and height of each page of `pdf`, from the `/MediaBox` of its page dictionaries.
fn page_sizes(pdf: &[u8]) -> Vec<(f32, f32)> {
    let text = String::from_utf8_lossy(pdf);
    text.match_indices("/MediaBox [")
        .map(|(at, key)| {
            let rest = &text[at + key.len()..];
            let numbers: Vec<f32> = rest[..rest.find(']').expect("a closed box")]
                .split_whitespace()
                .map(|number| number.parse().expect("a number"))
                .collect();
            (numbers[2] - numbers[0], numbers[3] - numbers[1])
        })
        .collect()
}

fn xml_attributes(attributes: &[(&str, &str)]) -> String {
    attributes.iter().map(|(name, value)| format!(r#" {name}="{value}""#)).collect()
}

fn tree_attributes(attributes: &[(&str, &str)]) -> String {
    let pairs: Vec<String> = attributes.iter().map(|(name, value)| format!(r#""{name}":"{value}""#)).collect();
    format!("{{{}}}", pairs.join(","))
}

/// The page of a document with one section, with the attributes given to the document and to the section, rendered
/// from its XML and from its kit tree. Each is named, for the failure message.
fn rendered(document: &[(&str, &str)], section: &[(&str, &str)]) -> [(&'static str, Vec<(f32, f32)>); 2] {
    let xml = format!(
        r#"<lpdf version="1"><document{}><section{}><layout><text>Page</text></layout></section></document></lpdf>"#,
        xml_attributes(document),
        xml_attributes(section),
    );
    let tree = format!(
        r#"{{"version":1,"type":"document","attrs":{},"nodes":[{{"type":"section","attrs":{},"nodes":[
            {{"type":"layout","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["Page"]}}]}}]}}]}}"#,
        tree_attributes(document),
        tree_attributes(section),
    );
    let engine = LpdfEngine::new("");
    let from_xml = engine.render_xml(&xml, None, &RenderEncryption::default()).expect("the XML renders");
    let from_tree = engine.render_json(&tree, &RenderEncryption::default()).expect("the tree renders");
    [("the XML", page_sizes(&from_xml)), ("the kit tree", page_sizes(&from_tree))]
}

/// The one page of the document is `expected`, on both paths.
fn assert_page(document: &[(&str, &str)], section: &[(&str, &str)], expected: (f32, f32)) {
    for (path, pages) in rendered(document, section) {
        assert_eq!(pages.len(), 1, "{path}: one page for document {document:?} and section {section:?}");
        let (width, height) = pages[0];
        assert!(
            (width - expected.0).abs() < 0.01 && (height - expected.1).abs() < 0.01,
            "{path}: document {document:?} and section {section:?} give {width} × {height}pt, not {} × {}pt",
            expected.0,
            expected.1,
        );
    }
}

fn landscape(size: (f32, f32)) -> (f32, f32) {
    (size.1, size.0)
}

#[test]
fn a_portrait_section_of_a_landscape_document_has_portrait_pages() {
    assert_page(&[("orientation", "landscape")], &[("orientation", "portrait")], A4);
    assert_page(&[("size", "a3"), ("orientation", "landscape")], &[("orientation", "portrait")], A3);
}

#[test]
fn a_landscape_section_of_a_portrait_document_has_landscape_pages() {
    assert_page(&[], &[("orientation", "landscape")], landscape(A4));
    assert_page(&[("size", "a3"), ("orientation", "portrait")], &[("orientation", "landscape")], landscape(A3));
}

#[test]
fn a_section_without_an_orientation_has_the_pages_of_its_document() {
    assert_page(&[], &[], A4);
    assert_page(&[("orientation", "landscape")], &[], landscape(A4));
    assert_page(&[("size", "297mm 210mm")], &[], (297.0 * MM, 210.0 * MM));
}

#[test]
fn a_section_that_names_the_orientation_its_document_has_keeps_the_pages() {
    assert_page(&[("orientation", "landscape")], &[("orientation", "landscape")], landscape(A4));
    assert_page(&[("orientation", "portrait")], &[("orientation", "portrait")], A4);
}

#[test]
fn portrait_stands_a_landscape_shaped_size_upright_on_a_document_and_on_a_section() {
    let upright = (210.0 * MM, 297.0 * MM);
    assert_page(&[("size", "297mm 210mm"), ("orientation", "portrait")], &[], upright);
    assert_page(&[], &[("size", "297mm 210mm"), ("orientation", "portrait")], upright);
    assert_page(&[("size", "297mm 210mm")], &[("orientation", "portrait")], upright);
}

#[test]
fn landscape_leaves_a_landscape_shaped_size_as_it_is_on_a_document_and_on_a_section() {
    let on_its_side = (297.0 * MM, 210.0 * MM);
    assert_page(&[("size", "297mm 210mm"), ("orientation", "landscape")], &[], on_its_side);
    assert_page(&[], &[("size", "297mm 210mm"), ("orientation", "landscape")], on_its_side);
    assert_page(&[("size", "297mm 210mm")], &[("orientation", "landscape")], on_its_side);
}

#[test]
fn landscape_lays_a_portrait_shaped_size_on_its_side_and_portrait_leaves_it() {
    assert_page(&[("size", "210mm 297mm"), ("orientation", "landscape")], &[], (297.0 * MM, 210.0 * MM));
    assert_page(&[("size", "210mm 297mm"), ("orientation", "portrait")], &[], (210.0 * MM, 297.0 * MM));
}

#[test]
fn a_section_with_its_own_size_and_no_orientation_takes_the_size_as_written() {
    assert_page(&[("orientation", "landscape")], &[("size", "a3")], A3);
    assert_page(&[("orientation", "landscape")], &[("size", "297mm 210mm")], (297.0 * MM, 210.0 * MM));
}

#[test]
fn a_section_with_its_own_size_has_the_shape_its_orientation_names() {
    assert_page(&[], &[("size", "a3"), ("orientation", "landscape")], landscape(A3));
    assert_page(&[("orientation", "landscape")], &[("size", "a3"), ("orientation", "portrait")], A3);
}

#[test]
fn a_square_page_is_the_same_with_either_orientation() {
    let square = (200.0 * MM, 200.0 * MM);
    assert_page(&[("size", "200mm 200mm"), ("orientation", "portrait")], &[], square);
    assert_page(&[("size", "200mm 200mm"), ("orientation", "landscape")], &[], square);
    assert_page(&[("size", "200mm 200mm")], &[("orientation", "portrait")], square);
    assert_page(&[("size", "200mm 200mm")], &[("orientation", "landscape")], square);
}
