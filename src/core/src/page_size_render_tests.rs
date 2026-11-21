//! The named values of `size`, on a document and on a section, rendered from the XML and from the kit tree of the same
//! document. Each name is checked against the dimensions of the paper it stands for, not against the engine's own
//! table. The WASI crate has its own `LpdfEngine`, so these run with the core library only.

use crate::orientation_render_tests::assert_page;
use crate::{LpdfEngine, RenderEncryption};

const MM: f32 = 72.0 / 25.4;
const INCH: f32 = 72.0;

/// The paper each name stands for, as its width and height in points.
const PAPER: [(&str, (f32, f32)); 12] = [
    ("a0", (841.0 * MM, 1189.0 * MM)),
    ("a1", (594.0 * MM, 841.0 * MM)),
    ("a2", (420.0 * MM, 594.0 * MM)),
    ("a3", (297.0 * MM, 420.0 * MM)),
    ("a4", (210.0 * MM, 297.0 * MM)),
    ("a5", (148.0 * MM, 210.0 * MM)),
    ("a6", (105.0 * MM, 148.0 * MM)),
    ("b4", (250.0 * MM, 353.0 * MM)),
    ("b5", (176.0 * MM, 250.0 * MM)),
    ("letter", (8.5 * INCH, 11.0 * INCH)),
    ("legal", (8.5 * INCH, 14.0 * INCH)),
    ("tabloid", (11.0 * INCH, 17.0 * INCH)),
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
    assert_page(&[("size", "tabloid"), ("orientation", "landscape")], &[], ledger);
    assert_page(&[], &[("size", "tabloid"), ("orientation", "landscape")], ledger);
}

#[test]
fn a_wrong_size_names_every_value_it_takes() {
    let engine = LpdfEngine::new("");
    let xml = r#"<lpdf version="1"><document size="a7"><section><layout><text>Page</text></layout></section></document></lpdf>"#;
    let tree = r#"{"version":1,"type":"document","attrs":{"size":"a7"},"nodes":[{"type":"section","attrs":{},"nodes":[
        {"type":"layout","attrs":{},"nodes":[{"type":"text","attrs":{},"nodes":["Page"]}]}]}]}"#;
    let from_xml = engine.render_xml(xml, None, &RenderEncryption::default()).expect_err("a7 is not a size");
    let from_tree = engine.render_json(tree, &RenderEncryption::default()).expect_err("a7 is not a size");
    let expected = "invalid page size: 'a7': expected a0, a1, a2, a3, a4, a5, a6, b4, b5, letter, legal, tabloid or a width and a height";
    assert!(from_xml.contains(expected), "the XML path says: {from_xml}");
    assert!(from_tree.contains(expected), "the kit tree path says: {from_tree}");
}
