//! Lengths in pt, mm and in: every attribute that takes a length reads all three, from XML and from a kit tree. An
//! inch is a whole 72 points, so a document written in inches renders byte for byte as the same document in points.
//! The WASI crate has its own `LpdfEngine`, so these run with the core library only.

use crate::{LpdfEngine, RenderEncryption};

/// A document whose first section holds `body`, in a document of `document_attrs`.
fn document(document_attrs: &str, body: &str) -> String {
    format!(r#"<lpdf version="1"><document {document_attrs}><section>{body}</section></document></lpdf>"#)
}

fn render_xml(xml: &str) -> Result<Vec<u8>, String> {
    LpdfEngine::render_xml_to_pdf_bytes(xml)
}

fn render_kit(json: &str) -> Result<Vec<u8>, String> {
    LpdfEngine::new("").render_json(json, &RenderEncryption::default())
}

/// The documents that use `length` in one place each, as `(what, xml)`.
fn places(length: &str) -> Vec<(&'static str, String)> {
    let flow = |body: String| document("", &format!(r#"<layout>{body}</layout>"#));
    let cell = |attrs: &str| format!(r##"<column {attrs} background="#dddddd"><text>x</text></column>"##);
    let table = |cols: String| {
        flow(format!(r#"<table cols="{cols}"><tr><td><text>a</text></td><td><text>b</text></td></tr></table>"#))
    };
    let row = |name: &str| format!(r#"<tokens><{name} xs="{length}" s="{length}" m="{length}" l="{length}" xl="{length}" xxl="{length}"/></tokens>"#);
    let tokens = |name: &str, user: &str| {
        format!(r#"<lpdf version="1">{}<document><section><layout>{user}</layout></section></document></lpdf>"#, row(name))
    };
    vec![
        ("column padding", flow(cell(&format!(r#"padding="{length}""#)))),
        ("column padding of two", flow(cell(&format!(r#"padding="{length} 2pt""#)))),
        ("column padding of three", flow(cell(&format!(r#"padding="{length} 2pt {length}""#)))),
        ("column padding of four", flow(cell(&format!(r#"padding="{length} 2pt {length} 3pt""#)))),
        ("column gap", flow(format!(r#"<column gap="{length}"><text>x</text><text>y</text></column>"#))),
        ("column width", flow(cell(&format!(r#"width="{length}""#)))),
        ("column height", flow(cell(&format!(r#"height="{length}""#)))),
        ("column radius", flow(cell(&format!(r#"radius="{length}""#)))),
        ("column border", flow(cell(&format!(r##"border="{length} #cccccc""##)))),
        ("row gap", flow(format!(r#"<row gap="{length}"><text>x</text><text>y</text></row>"#))),
        ("text font-size", flow(format!(r#"<text font-size="{length}">x</text>"#))),
        ("divider thickness", flow(format!(r#"<divider thickness="{length}"/>"#))),
        ("grid col-width", flow(format!(r#"<grid cols="2" col-width="{length}"><text>a</text><text>b</text></grid>"#))),
        ("layout margin", document("", &format!(r#"<layout margin="{length}"><text>x</text></layout>"#))),
        ("layout margin of two", document("", &format!(r#"<layout margin="{length} 20pt"><text>x</text></layout>"#))),
        ("document size", document(&format!(r#"size="{length} {length}""#), r#"<layout margin="0"><text>x</text></layout>"#)),
        ("table cols", table(format!("{length} 1fr"))),
        ("table cols with a percentage", table(format!("{length} 25%"))),
        ("text-at width", document("", &format!(r#"<layer><text-at x="10pt" y="10pt" width="{length}">some words</text-at></layer>"#))),
        ("space row", tokens("space", r#"<column gap="m"><text>x</text><text>y</text></column>"#)),
        ("border row", tokens("border", r##"<column border="m #cccccc"><text>x</text></column>"##)),
        ("radius row", tokens("radius", r##"<column radius="m" background="#dddddd"><text>x</text></column>"##)),
        ("width row", tokens("width", r##"<column width="m" background="#dddddd"><text>x</text></column>"##)),
        ("height row", tokens("height", r##"<column height="m" background="#dddddd"><text>x</text></column>"##)),
        ("grid row", tokens("grid", r#"<grid cols="2" col-width="m"><text>a</text><text>b</text></grid>"#)),
        ("text-size row", tokens("text-size", r#"<text font-size="m">x</text>"#)),
        ("markdown margin and gap", document("", &format!(
            r#"<markdown margin="{length}" gap="{length}"><markdown-content><![CDATA[one

two]]></markdown-content></markdown>"#))),
    ]
}

/// The document of the place called `what`, with `length` in it.
fn place(what: &str, length: &str) -> String {
    places(length).into_iter().find(|(name, _)| *name == what).unwrap_or_else(|| panic!("no place called {what}")).1
}

#[test]
fn a_document_in_inches_renders_as_the_same_one_in_points() {
    for ((what, inches), (_, points)) in places("1in").into_iter().zip(places("72pt")) {
        let in_inches = render_xml(&inches).unwrap_or_else(|e| panic!("{what} in inches: {e}"));
        let in_points = render_xml(&points).unwrap_or_else(|e| panic!("{what} in points: {e}"));
        assert!(in_inches == in_points, "{what}: 1in renders as 72pt");
    }
}

#[test]
fn every_place_that_takes_a_length_reads_it_in_millimetres() {
    for (what, xml) in places("10mm") {
        render_xml(&xml).unwrap_or_else(|e| panic!("{what} in millimetres: {e}"));
    }
}

#[test]
fn a_length_in_millimetres_comes_out_as_the_same_length_in_points() {
    let in_mm = render_xml(&place("column width", "25.4mm")).unwrap();
    let in_pt = render_xml(&place("column width", "72pt")).unwrap();
    let in_other = render_xml(&place("column width", "50pt")).unwrap();
    assert!(in_other != in_pt, "the width is drawn, so a different width is a different file");
    // 25.4mm is 72 points up to the rounding of an f32, which the file writes to fewer places than it keeps.
    assert!(in_mm == in_pt, "25.4mm is 72pt");
}

#[test]
fn a_length_in_another_unit_is_refused_wherever_it_is_written() {
    for (what, xml) in places("2cm") {
        let err = render_xml(&xml).expect_err(what);
        assert!(err.contains("2cm"), "{what}: {err}");
    }
}

#[test]
fn a_refused_length_names_the_units_that_would_do() {
    for xml in [
        document("", r#"<layout><column padding="2cm"><text>x</text></column></layout>"#),
        document("", r#"<layout><text font-size="2cm">x</text></layout>"#),
        document(r#"size="21cm 29cm""#, "<layout><text>x</text></layout>"),
        document("", r#"<layer><text-at x="0pt" y="0pt" width="2cm">x</text-at></layer>"#),
    ] {
        let err = render_xml(&xml).unwrap_err();
        assert!(err.contains("pt, mm or in"), "{err}");
    }
}

#[test]
fn a_length_in_another_unit_names_the_units_wherever_a_layer_a_shape_a_bleed_or_a_shadow_reads_it() {
    for (what, xml) in [
        ("a layer's rect", document("", r#"<layer><rect x="0pt" y="0pt" width="2cm" height="10pt"/></layer>"#)),
        ("a shape's radius", document("", r#"<layout><shape type="circle" r="2cm"/></layout>"#)),
        ("a bleed", document(r#"bleed="1cm""#, "<layout><text>x</text></layout>")),
        ("a shadow", document("", r##"<layout><column shadow="2cm 2cm #000000" background="#dddddd"><text>x</text></column></layout>"##)),
    ] {
        let err = render_xml(&xml).expect_err(what);
        assert!(err.contains("pt, mm or in"), "{what}: {err}");
    }
}

/// A document whose layer draws the image `logo` with `attrs`.
fn img_at(attrs: &str) -> String {
    format!(
        r#"<lpdf version="1"><assets><image name="logo"/></assets><document><section>
        <layer><img-at name="logo" x="0pt" y="0pt" {attrs}/></layer></section></document></lpdf>"#
    )
}

#[test]
fn the_width_and_height_of_an_img_at_read_lengths_in_each_unit() {
    for length in ["72pt", "1in", "25.4mm"] {
        crate::parse::parse(&img_at(&format!(r#"width="{length}" height="{length}""#)))
            .unwrap_or_else(|e| panic!("an img-at of {length}: {e}"));
    }
    for attrs in [r#"width="2cm""#, r#"height="2cm""#] {
        let err = crate::parse::parse(&img_at(attrs)).err().unwrap_or_else(|| panic!("{attrs} was accepted"));
        assert!(err.contains("2cm") && err.contains("pt, mm or in"), "{attrs}: {err}");
    }
}

fn table(cols: &str) -> String {
    document("", &format!(r#"<layout><table cols="{cols}"><tr><td><text>a</text></td><td><text>b</text></td></tr></table></layout>"#))
}

#[test]
fn a_column_width_that_is_none_of_the_units_is_an_error_not_a_dropped_column() {
    for cols in ["40cm 1fr", "40px 1fr", "1fr wide", "1fr 40"] {
        let rendered = render_xml(&table(cols));
        if cols == "1fr 40" {
            assert!(rendered.is_ok(), "a bare number is a share: {cols}");
            continue;
        }
        let err = rendered.unwrap_err();
        assert!(err.contains(&format!("cols=\"{cols}\"")) && err.contains("not a column width"), "{err}");
        assert!(err.contains("auto") && err.contains("1fr") && err.contains("pt, mm or in") && err.contains('%'), "{err}");
    }
    let err = render_xml(&table("40cm 1fr")).unwrap_err();
    assert!(err.contains("'40cm'"), "the entry that is wrong is named: {err}");
}

#[test]
fn a_column_width_of_auto_beside_a_length_in_mm_still_shares_the_rest() {
    render_xml(&table("auto 40mm")).unwrap();
    let err = render_xml(&table("auto 1fr")).unwrap_err();
    assert!(err.contains("auto and fr columns"), "{err}");
}

#[test]
fn a_bare_number_is_still_no_length_for_a_padding_a_width_and_a_page_size() {
    for (what, xml) in [
        ("padding", document("", r#"<layout><column padding="10"><text>x</text></column></layout>"#)),
        ("width", document("", r#"<layout><column width="10"><text>x</text></column></layout>"#)),
        ("page size", document(r#"size="595 842""#, "<layout><text>x</text></layout>")),
    ] {
        assert!(render_xml(&xml).is_err(), "{what}: a bare 10 has no unit");
    }
    render_xml(&document("", r#"<layout><column padding="0" gap="0"><text>x</text></column></layout>"#)).unwrap();
    render_xml(&document("", r#"<layout><text font-size="12">x</text></layout>"#)).expect("a text size keeps its plain number");
}

/// A kit tree that uses `length` in a layout, a column, a text, a table, a text-at and the space row of the tokens.
fn kit(length: &str) -> String {
    format!(
        r##"{{"version":1,"type":"document","attrs":{{"tokens":{{"space":{{"xs":"{length}","s":"{length}","m":"{length}","l":"{length}","xl":"{length}","xxl":"{length}"}}}}}},
        "nodes":[{{"type":"section","attrs":{{}},"nodes":[
            {{"type":"layout","attrs":{{"margin":"{length} 20pt"}},"nodes":[
                {{"type":"column","attrs":{{"padding":"{length}","gap":"{length}","width":"{length}","radius":"{length}","background":"#dddddd"}},"nodes":[
                    {{"type":"text","attrs":{{"font-size":"{length}"}},"nodes":["x"]}},
                    {{"type":"text","attrs":{{"font-size":"m"}},"nodes":["y"]}}]}},
                {{"type":"table","attrs":{{"cols":"{length} 1fr"}},"nodes":[
                    {{"type":"tr","attrs":{{}},"nodes":[
                        {{"type":"td","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["a"]}}]}},
                        {{"type":"td","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["b"]}}]}}]}}]}}]}},
            {{"type":"layer","attrs":{{}},"nodes":[
                {{"type":"text-at","attrs":{{"x":"10pt","y":"10pt","width":"{length}"}},"nodes":["some words"]}}]}}]}}]}}"##
    )
}

/// A kit tree whose page is `length` wide and high.
fn kit_page(length: &str) -> String {
    format!(
        r#"{{"version":1,"type":"document","attrs":{{"size":"{length} {length}"}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
            {{"type":"layout","attrs":{{"margin":"0"}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["x"]}}]}}]}}]}}"#
    )
}

#[test]
fn a_kit_tree_reads_lengths_in_inches_as_points_and_in_millimetres() {
    let in_inches = render_kit(&kit("1in")).unwrap_or_else(|e| panic!("in inches: {e}"));
    let in_points = render_kit(&kit("72pt")).unwrap_or_else(|e| panic!("in points: {e}"));
    assert!(in_inches == in_points, "1in renders as 72pt");
    render_kit(&kit("10mm")).unwrap_or_else(|e| panic!("in millimetres: {e}"));
    assert!(render_kit(&kit_page("1in")).unwrap() == render_kit(&kit_page("72pt")).unwrap(), "a page of 1in is one of 72pt");
    render_kit(&kit_page("210mm")).unwrap();
}

#[test]
fn a_kit_tree_refuses_a_length_in_another_unit_and_a_column_width_that_is_none() {
    let err = render_kit(&kit("2cm")).unwrap_err();
    assert!(err.contains("2cm"), "{err}");
    let err = render_kit(&kit_page("21cm")).unwrap_err();
    assert!(err.contains("'21cm'") && err.contains("pt, mm or in"), "{err}");
    let tree = |attrs: &str| {
        format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[
            {{"type":"table","attrs":{attrs},"nodes":[{{"type":"tr","attrs":{{}},"nodes":[{{"type":"td","attrs":{{}},"nodes":[{{"type":"text","attrs":{{}},"nodes":["a"]}}]}}]}}]}}]}}]}}]}}"#
        )
    };
    let err = render_kit(&tree(r#"{"cols":"40cm 1fr"}"#)).unwrap_err();
    assert!(err.contains("'40cm'") && err.contains("not a column width"), "{err}");
    render_kit(&tree(r#"{"cols":"40mm"}"#)).unwrap();
}
