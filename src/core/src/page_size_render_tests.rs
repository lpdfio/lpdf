//! The named values of `size`, on a document and on a section, rendered from the XML and from the kit tree of the same
//! document. Each name is checked against the dimensions of the paper it stands for, not against the engine's own
//! table. The WASI crate has its own `LpdfEngine`, so these run with the core library only.

use crate::orientation_render_tests::assert_page;
use crate::{LpdfEngine, RenderEncryption};

const MM: f32 = 72.0 / 25.4;
const INCH: f32 = 72.0;

/// The paper each name stands for, as its width and height in points.
const PAPER: [(&str, (f32, f32)); 12] = [
    ("A0", (841.0 * MM, 1189.0 * MM)),
    ("A1", (594.0 * MM, 841.0 * MM)),
    ("A2", (420.0 * MM, 594.0 * MM)),
    ("A3", (297.0 * MM, 420.0 * MM)),
    ("A4", (210.0 * MM, 297.0 * MM)),
    ("A5", (148.0 * MM, 210.0 * MM)),
    ("A6", (105.0 * MM, 148.0 * MM)),
    ("B4", (250.0 * MM, 353.0 * MM)),
    ("B5", (176.0 * MM, 250.0 * MM)),
    ("Letter", (8.5 * INCH, 11.0 * INCH)),
    ("Legal", (8.5 * INCH, 14.0 * INCH)),
    ("Tabloid", (11.0 * INCH, 17.0 * INCH)),
];

#[test]
fn each_named_size_gives_its_paper_on_a_document() {
    for (name, paper) in PAPER {
        assert_page(&[("size", name)], &[], paper);
    }
}

#[test]
fn each_named_size_gives_its_paper_on_a_section() {
    for (name, paper) in PAPER {
        assert_page(&[], &[("size", name)], paper);
    }
}

#[test]
fn tabloid_on_its_side_is_ledger() {
    let ledger = (17.0 * INCH, 11.0 * INCH);
    assert_page(&[("size", "Tabloid"), ("orientation", "landscape")], &[], ledger);
    assert_page(&[], &[("size", "Tabloid"), ("orientation", "landscape")], ledger);
}

/// Renders a document whose `size` is `size`, from the XML and from the kit tree, and checks that both refuse it with
/// the error that names every value `size` takes.
fn assert_refused_naming_every_size(size: &str) {
    let engine = LpdfEngine::new("");
    let xml = format!(r#"<lpdf version="1"><document size="{size}"><section><layout><text>Page</text></layout></section></document></lpdf>"#);
    let tree = format!(r#"{{"version":1,"type":"document","attrs":{{"size":"{size}"}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
        {{"type":"layout","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["Page"]}}]}}]}}]}}"#);
    let from_xml = engine.render_xml(&xml, None, &RenderEncryption::default()).expect_err("not a size");
    let from_tree = engine.render_json(&tree, &RenderEncryption::default()).expect_err("not a size");
    let expected = format!(
        "invalid page size: '{size}': expected A0, A1, A2, A3, A4, A5, A6, B4, B5, Letter, Legal, Tabloid or a width and a height"
    );
    assert!(from_xml.contains(&expected), "the XML path says: {from_xml}");
    assert!(from_tree.contains(&expected), "the kit tree path says: {from_tree}");
}

#[test]
fn a_wrong_size_names_every_value_it_takes() {
    assert_refused_naming_every_size("A7");
}

#[test]
fn a_name_in_lowercase_is_a_wrong_size() {
    for size in ["a4", "letter", "b5"] {
        assert_refused_naming_every_size(size);
    }
}
