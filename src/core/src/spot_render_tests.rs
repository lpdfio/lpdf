//! Spot colors: the Separation space each ink gets, tints, overprint, gradients of tints, what a page names, the errors a
//! document gets for one, PDF/A and PDF/X, and the samples `make check-spot`, `check-pdfa` and `check-pdfx` validate.
//! The WASI crate has its own `LpdfEngine`, so these run with the core library only.

use crate::{LpdfEngine, RenderEncryption};

/// Three inks: a brand color in CMYK, a varnish that overprints, and a die line with an RGB stand-in that overprints.
const SPOTS: &str = r##"<spots>
      <spot name="brand" ink="PANTONE 1665 C" value="cmyk(0, 85, 100, 0)"/>
      <spot name="varnish" ink="Varnish" value="cmyk(0, 0, 0, 10)" overprint="true"/>
      <spot name="die" ink="Dieline" value="#ff00ff" overprint="true"/>
    </spots>"##;

/// A document whose tokens hold `SPOTS` and `tokens`, and whose one layout holds `layout`.
fn document(tokens: &str, layout: &str) -> String {
    format!(r#"<lpdf version="1"><tokens>{SPOTS}{tokens}</tokens><document><section><layout>{layout}</layout></section></document></lpdf>"#)
}

fn render(xml: &str) -> Vec<u8> {
    LpdfEngine::render_xml_to_pdf_bytes(xml).unwrap_or_else(|e| panic!("renders: {e}"))
}

fn refused(xml: &str) -> String {
    LpdfEngine::render_xml_to_pdf_bytes(xml).expect_err("a render that is refused")
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// The content streams of `pdf` and its form XObjects, inflated, as text.
fn streams(pdf: &[u8]) -> Vec<String> {
    crate::ruling_render_tests::streams(pdf)
}

/// The text of the object `/Separation /<ink>` is in, from `/Separation` to its `]`, on one line.
fn separation(pdf: &str, ink: &str) -> String {
    let at = pdf.find(&format!("/Separation /{ink} ")).unwrap_or_else(|| panic!("no Separation space for {ink}: {pdf}"));
    let end = at + pdf[at..].find("endobj").expect("the object ends");
    one_line(&pdf[at..end])
}

/// `text` with each run of whitespace as one space, and none after `<<` or before `>>`.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").replace("<< ", "<<").replace(" >>", ">>")
}

/// The text of the object that holds `needle`, on one line.
fn object_with(pdf: &str, needle: &str) -> String {
    let at = pdf.find(needle).unwrap_or_else(|| panic!("no {needle}: {pdf}"));
    let start = pdf[..at].rfind(" obj\n").expect("the object starts");
    let end = at + pdf[at..].find("endobj").expect("the object ends");
    one_line(&pdf[start..end])
}

/// The number of the object whose text starts with `start`.
fn object_number(pdf: &str, start: &str) -> String {
    let at = pdf.find(&format!(" 0 obj\n{start}")).unwrap_or_else(|| panic!("no object starts with {start}: {pdf}"));
    pdf[..at].rsplit('\n').next().expect("the object's line").to_string()
}

/// The text of the graphics state dictionary that holds `/OPM`, of the one whose `op` and `OP` are as given.
fn overprint_state(pdf: &str, fill: bool, stroke: bool) -> bool {
    pdf.split("endobj").any(|object| {
        object.contains("/OPM 1") && object.contains(&format!("/op {fill}")) && object.contains(&format!("/OP {stroke}"))
    })
}

// ── The PDF ───────────────────────────────────────────────────────────────────

#[test]
fn each_ink_is_a_separation_space_with_its_stand_in_as_the_alternate() {
    let pdf = text(&render(&document(
        "",
        r#"<text color="brand">A</text><text color="die">B</text><text color="varnish">C</text>"#,
    )));
    assert_eq!(
        separation(&pdf, "PANTONE#201665#20C"),
        "/Separation /PANTONE#201665#20C /DeviceCMYK <</FunctionType 2 /Domain [0 1] /C0 [0 0 0 0] /C1 [0 0.85 1 0] /N 1>>]",
    );
    assert!(separation(&pdf, "Dieline").contains("/DeviceRGB <</FunctionType 2 /Domain [0 1] /C0 [1 1 1] /C1 [1 0 1] /N 1>>"));
    assert!(separation(&pdf, "Varnish").contains("/C1 [0 0 0 0.1]"));
    // Each is in the page's resources under the spot color's name.
    for name in ["brand", "die", "varnish"] {
        assert!(pdf.contains(&format!("/CS-{name} ")), "{name}: {pdf}");
    }
}

#[test]
fn the_inks_are_numbered_in_the_order_the_pages_first_paint_with_them() {
    let pdf = text(&render(&document("", r#"<text color="die">B</text><text color="brand">A</text>"#)));
    assert!(pdf.find("/Separation /Dieline").unwrap() < pdf.find("/Separation /PANTONE").unwrap());
    assert!(!pdf.contains("/Separation /Varnish"), "an ink no page paints with is not written");
    assert_eq!(text(&render(&document("", r#"<text color="die">B</text><text color="brand">A</text>"#))), pdf);
}

#[test]
fn a_tint_is_the_value_its_color_operator_sets_and_a_solid_spot_color_is_1() {
    let pdf = render(&document(
        "",
        r#"<column background="brand 30%" border="s brand 12.5%"><text color="brand">A</text></column>"#,
    ));
    let page = &streams(&pdf)[0];
    assert!(page.contains("/CS-brand cs\n0.3 scn\n/CS-brand CS\n0.125 SCN\n"), "{page}");
    assert!(page.contains("/CS-brand cs\n1 scn\n"), "{page}");
    // `brand 100%` is `brand`, and `brand 0%` no ink.
    let full = render(&document("", r#"<text color="brand 100%">A</text>"#));
    assert_eq!(full, render(&document("", r#"<text color="brand">A</text>"#)));
    assert!(streams(&render(&document("", r#"<text color="brand 0%">A</text>"#)))[0].contains("/CS-brand cs\n0 scn\n"));
}

#[test]
fn a_spot_color_of_a_shadow_and_of_a_canvas_shape_is_its_ink_too() {
    let pdf = render(&document(
        "",
        r#"<column shadow="2pt 2pt brand 40% 0.5" background="surface" height="20pt"/>"#,
    ));
    let pdf = text(&pdf);
    assert!(pdf.contains("/CS-brand "), "{pdf}");
    let canvas = render(&format!(
        r#"<lpdf version="1"><tokens>{SPOTS}</tokens><document><section><layout/><layer><rect x="10pt" y="10pt" width="40pt" height="40pt" fill="brand 50%" stroke="die" stroke-width="2pt"/></layer></section></document></lpdf>"#
    ));
    let page = streams(&canvas).join("\n");
    assert!(page.contains("/CS-brand cs\n0.5 scn\n/CS-die CS\n1 SCN\n"), "{page}");
    assert!(page.contains("/OP-stroke gs"), "the die line overprints its stroke: {page}");
}

#[test]
fn an_overprinting_spot_color_overprints_its_fill_or_its_stroke_and_nothing_after_it() {
    let pdf = render(&document(
        "",
        r##"<column background="varnish" border="s #000000" height="20pt"/><column background="#ffffff" border="s die" height="20pt"/>
        <column background="varnish" border="s die" height="20pt"/><column background="#00aaff" height="20pt"/>"##,
    ));
    let page = &streams(&pdf)[0];
    let boxes: Vec<&str> = page.split("\nQ\n").collect();
    assert!(boxes[0].contains("/OP-fill gs"), "{page}");
    assert!(boxes[1].contains("/OP-stroke gs"), "{page}");
    assert!(boxes[2].contains("/OP-both gs"), "{page}");
    assert!(!boxes[3].contains(" gs"), "the box after them is in the page's state, which doesn't overprint: {page}");
    let pdf = text(&pdf);
    assert!(overprint_state(&pdf, true, false) && overprint_state(&pdf, false, true) && overprint_state(&pdf, true, true), "{pdf}");
}

#[test]
fn overprinting_text_has_a_q_of_its_own_around_its_text_object() {
    let page = &streams(&render(&document("", r#"<text color="varnish">A</text><text color="brand">B</text>"#)))[0];
    assert!(page.contains("q\n/OP-fill gs\nBT\n/CS-varnish cs\n1 scn\n"), "{page}");
    let after = &page[page.find("/CS-varnish").unwrap()..];
    assert!(after.contains("ET\nQ\nBT\n/CS-brand cs"), "{page}");
}

#[test]
fn the_digits_of_a_barcode_in_an_overprinting_ink_do_not_overprint() {
    let page = &streams(&render(&document("", r#"<barcode type="code128" data="LPDF-537" hrt="true" color="varnish" width="160pt" height="50pt"/>"#)))[0];
    let bars = page.find("/OP-fill gs").expect("the bars overprint");
    let digits = page.find("BT").expect("the digits");
    assert!(bars < digits, "{page}");
    assert!(page[bars..digits].contains("\nQ\n"), "the bars' state ends before the digits: {page}");
}

#[test]
fn a_document_without_an_overprinting_spot_color_writes_no_overprint_state() {
    let pdf = text(&render(&document("", r#"<text color="brand">A</text>"#)));
    assert!(!pdf.contains("/OPM"), "{pdf}");
}

#[test]
fn a_gradient_of_tints_is_a_shading_in_the_separation_space_of_its_ink() {
    let pdf = text(&render(&document(
        r#"<gradients><gradient name="fade" type="linear" from="brand" to="brand 20%" angle="90"/></gradients>"#,
        r#"<column background="gradient:fade" height="40pt"/>"#,
    )));
    let space = object_number(&pdf, "[/Separation /PANTONE");
    let shading = object_with(&pdf, "/ShadingType 2");
    assert!(shading.contains(&format!("/ColorSpace {space} 0 R")), "{shading}");
    let function = object_with(&pdf, "/C1 [0.2]");
    assert!(function.contains("/C0 [1] /C1 [0.2]"), "{function}");
}

#[test]
fn a_gradient_of_an_overprinting_ink_overprints() {
    let pdf = render(&document(
        r#"<gradients><gradient name="sheen" type="radial" from="varnish" to="varnish 0%"/></gradients>"#,
        r#"<column background="gradient:sheen" height="40pt"/>"#,
    ));
    assert!(streams(&pdf)[0].contains("/OP-fill gs"));
}

#[test]
fn a_page_names_only_the_spot_colors_it_paints_with() {
    let xml = document("", r#"<text color="brand">A</text>"#)
        .replace("</section>", r#"</section><section><layout><text color="die">B</text></layout></section>"#);
    let pdf = text(&render(&xml));
    let pages: Vec<&str> = pdf.split("/Type /Page\n").skip(1).collect();
    let resources = |page: &str| page[..page.find(">>\n/Contents").unwrap_or(page.len())].to_string();
    let first = resources(pages[0]);
    let second = resources(pages[1]);
    assert!(first.contains("/CS-brand") && !first.contains("/CS-die"), "{first}");
    assert!(second.contains("/CS-die") && !second.contains("/CS-brand"), "{second}");
    assert!(second.contains("/OP-stroke") && !first.contains("/OP-"), "only a page with an overprinting ink names the states");
}

#[test]
fn a_spot_color_in_a_faded_box_is_named_by_its_group() {
    let pdf = render(&document("", r#"<column opacity="0.5" background="brand"><text>A</text></column>"#));
    let group = streams(&pdf).into_iter().find(|s| s.contains("/CS-brand cs")).expect("the group draws the ink");
    assert!(!group.contains("/GS50"), "the group is drawn faded, not faded inside: {group}");
    let pdf = text(&pdf);
    assert_eq!(pdf.matches("/CS-brand ").count(), 2, "the page and its group name it: {pdf}");
}

#[test]
fn the_lines_of_a_ruling_in_a_spot_color_name_its_space_in_their_drawing() {
    let xml = format!(
        r#"<lpdf version="1"><tokens>{SPOTS}<rulings><ruling name="grid"><lines step="20mm" color="die 50%"/></ruling></rulings></tokens><document><section ruling="grid"><layout/></section></document></lpdf>"#
    );
    let pdf = render(&xml);
    let drawing = streams(&pdf).into_iter().find(|s| s.contains("/CS-die CS")).expect("the ruling's drawing");
    assert!(drawing.contains("/CS-die CS\n0.5 SCN\n/OP-stroke gs"), "{drawing}");
    let pdf = text(&pdf);
    let form = &pdf[pdf.find("/Subtype /Form").unwrap()..];
    assert!(form[..form.find("endobj").unwrap()].contains("/CS-die"), "the form names the space: {form}");
}

#[test]
fn a_form_field_in_a_spot_color_names_its_space_in_its_appearance() {
    let pdf = text(&render(&document("", r#"<field type="text" name="code" width="120pt" height="20pt" background="brand 20%" border="s die"/>"#)));
    let appearance = &pdf[pdf.find("/BBox [0 0 120 20]").unwrap()..];
    let appearance = &appearance[..appearance.find("endobj").unwrap()];
    assert!(appearance.contains("/CS-brand") && appearance.contains("/CS-die") && appearance.contains("/OP-stroke"), "{appearance}");
    // A field without spot colors has an appearance without resources, as before.
    let plain = text(&render(&document("", r##"<field type="text" name="code" width="120pt" height="20pt" background="#eeeeee"/>"##)));
    let appearance = &plain[plain.find("/BBox [0 0 120 20]").unwrap()..];
    assert!(!appearance[..appearance.find("endobj").unwrap()].contains("/Resources"));
}

#[test]
fn a_linearized_file_names_the_spot_colors_its_pages_paint_with() {
    let xml = document("", r#"<text color="brand">A</text><column background="varnish" height="10pt"/>"#)
        .replace("<document>", r#"<document><output linearize="true"/>"#);
    let pdf = text(&LpdfEngine::new("").render_xml(&xml, None, &RenderEncryption::default()).expect("renders"));
    assert!(pdf.contains("/Linearized"), "{pdf}");
    assert!(pdf.contains("/CS-brand ") && pdf.contains("/CS-varnish ") && pdf.contains("/OP-fill "), "{pdf}");
    assert!(!pdf.contains("/OP-stroke ") && !pdf.contains("/CS-die "), "only what the page's stream names: {pdf}");
}

#[test]
fn a_bound_color_can_be_a_tint() {
    let xml = document("", r#"<text data-color="tone">A</text>"#);
    let pdf = LpdfEngine::new("").render_xml(&xml, Some(r#"{"tone":"brand 60%"}"#), &RenderEncryption::default()).expect("renders");
    assert!(streams(&pdf)[0].contains("/CS-brand cs\n0.6 scn\n"));
}

#[test]
fn a_document_that_paints_with_no_spot_color_writes_the_bytes_it_wrote_without_them() {
    let layout = r##"<column background="primary" padding="m"><text color="#ffffff">A</text></column>"##;
    let without = render(&format!(r#"<lpdf version="1"><document><section><layout>{layout}</layout></section></document></lpdf>"#));
    assert_eq!(render(&document("", layout)), without);
}

#[test]
fn a_spot_color_may_take_the_name_of_a_default_color() {
    let xml = r##"<lpdf version="1"><tokens><spots><spot name="primary" ink="Brand blue" value="#1763cf"/></spots></tokens>
        <document><section><layout><text color="primary">A</text></layout></section></document></lpdf>"##;
    assert!(streams(&render(xml))[0].contains("/CS-primary cs"));
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[test]
fn a_tint_of_a_color_of_the_colors_row_is_an_error_that_says_to_write_the_lighter_color() {
    let error = refused(&document(r##"<colors><color name="ink" value="#1f2a44"/></colors>"##, r#"<text color="ink 30%">A</text>"#));
    assert!(error.contains("only a spot color takes a tint, and 'ink' is a color of the colors row: write the lighter color itself"), "{error}");
    let error = refused(&document("", r#"<text color="primary 30%">A</text>"#));
    assert!(error.contains("'primary' is a color of the colors row"), "a default color too: {error}");
}

#[test]
fn a_tint_out_of_range_or_without_a_percent_sign_is_an_error() {
    for (tint, says) in [
        ("brand 130%", "the tint '130%' is not a percentage from 0% to 100%"),
        ("brand -5%", "the tint '-5%' is not a percentage from 0% to 100%"),
        ("brand 30", "the tint '30' needs a %, as in 30%"),
        ("brand 0.3", "the tint '0.3' needs a %, as in 30%"),
        ("nothing 30%", "unknown spot color 'nothing': a tint takes a spot color of the spots row"),
    ] {
        let error = refused(&document("", &format!(r#"<text color="{tint}">A</text>"#)));
        assert!(error.contains(&format!("'{tint}': {says}")), "{tint}: {error}");
    }
}

#[test]
fn each_wrong_spot_color_is_an_error_that_says_what_to_change() {
    for (spot, says) in [
        (r##"<spot name="Brand" ink="X" value="#000000"/>"##, "the name must be lowercase letters, digits and hyphens"),
        (r##"<spot name="other" ink="All" value="#000000"/>"##, "the ink can't be named 'All'"),
        (r##"<spot name="other" ink="Cyan" value="#000000"/>"##, "the ink can't be named 'Cyan'"),
        (r##"<spot name="other" ink=" " value="#000000"/>"##, "the ink has no name"),
        (r##"<spot name="other" ink="Varnish" value="#000000"/>"##, "the spot color 'varnish' has the ink 'Varnish' too"),
        (r##"<spot name="brand" ink="Other" value="#000000"/>"##, "the spots row has it twice"),
        (r##"<spot name="other" ink="Other" value="primary"/>"##, "the value 'primary' is not a color as written"),
        (r##"<spot name="other" ink="Other" value="cmyk(0, 0, 0)"/>"##, "invalid color 'cmyk(0, 0, 0)'"),
        (r##"<spot name="other" ink="Other" value="#000000" overprint="yes"/>"##, "overprint must be true or false"),
    ] {
        let xml = document("", "<text>A</text>").replace("</spots>", &format!("{spot}</spots>"));
        let error = refused(&xml);
        assert!(error.contains(says), "{spot}: {error}");
        assert!(error.contains("<spot>"), "the error names the element: {error}");
    }
}

#[test]
fn a_spot_color_with_the_name_of_a_color_the_document_writes_is_an_error() {
    let error = refused(&document(r##"<colors><color name="brand" value="#e35205"/></colors>"##, "<text>A</text>"));
    assert!(error.contains("spot color 'brand': the colors row has a color of the same name"), "{error}");
}

#[test]
fn a_gradient_that_mixes_a_spot_color_with_another_color_is_an_error_naming_it() {
    for (from, to, other) in [("brand", "#ffffff", "the color #ffffff"), ("brand", "die 50%", "the spot color 'die'"), ("cmyk(0 0 0 100)", "brand", "the color cmyk(0, 0, 0, 100)")] {
        let error = refused(&document(
            &format!(r#"<gradients><gradient name="mix" type="linear" from="{from}" to="{to}"/></gradients>"#),
            "<text>A</text>",
        ));
        assert!(error.contains(&format!("gradient 'mix': its stops mix the spot color 'brand' with {other}")), "{error}");
    }
}

#[test]
fn a_gradient_names_a_spot_color_declared_after_it() {
    let xml = r##"<lpdf version="1"><tokens><gradients><gradient name="fade" type="linear" from="brand" to="brand 0%"/></gradients>
        <spots><spot name="brand" ink="Brand" value="#e35205"/></spots></tokens>
        <document><section><layout><column background="gradient:fade" height="20pt"/></layout></section></document></lpdf>"##;
    assert!(text(&render(xml)).contains("/Separation /Brand"));
}

// ── The kit tree ──────────────────────────────────────────────────────────────

#[test]
fn spot_colors_from_a_kit_tree_render_as_the_xml_does() {
    let tree = r##"{"version":1,"type":"document","attrs":{"tokens":{"spots":{
        "brand":{"ink":"PANTONE 1665 C","value":"cmyk(0, 85, 100, 0)"},
        "die":{"ink":"Dieline","value":"#ff00ff","overprint":true}}}},
        "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","nodes":[
            {"type":"column","attrs":{"background":"brand 30%","border":"s die"},"nodes":[{"type":"text","attrs":{"color":"brand"},"nodes":["A"]}]}]}]}]}"##;
    let engine = LpdfEngine::new("");
    let from_tree = engine.render_json(tree, &RenderEncryption::default()).expect("renders");
    let xml = crate::kit_to_xml::kit_to_xml(tree).expect("XML");
    assert!(xml.contains(r##"<spot name="die" ink="Dieline" value="#ff00ff" overprint="true"/>"##), "{xml}");
    assert_eq!(from_tree, engine.render_xml(&xml, None, &RenderEncryption::default()).expect("renders"));
    assert!(text(&from_tree).contains("/Separation /Dieline"));
}

#[test]
fn a_spot_color_of_a_kit_tree_without_its_ink_is_an_error() {
    let tree = r##"{"version":1,"type":"document","attrs":{"tokens":{"spots":{"brand":{"value":"#e35205"}}}},"nodes":[]}"##;
    let error = LpdfEngine::new("").render_json(tree, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("spot color 'brand': missing ink"), "{error}");
}

// ── PDF/A and PDF/X ───────────────────────────────────────────────────────────

/// Spot colors for the samples of the standards: a brand color, and a die line that overprints, both with the stand-in
/// `stand_in` gives each, as a tint and solid, in a gradient of tints, and as a frame.
fn with_spots(xml: &str, brand: &str, die: &str) -> String {
    let gradient = r#"<gradient name="brand-fade" type="linear" from="brand" to="brand 10%" angle="90"/>"#;
    // Into the fixture's own gradients, where it has them: a second row would take their place.
    let (xml, gradients) = match xml.find("<gradients>") {
        Some(at) => (format!("{}<gradients>{gradient}{}", &xml[..at], &xml[at + "<gradients>".len()..]), String::new()),
        None => (xml.to_string(), format!("<gradients>{gradient}</gradients>")),
    };
    let tokens = format!(
        r#"<spots><spot name="brand" ink="PANTONE 1665 C" value="{brand}"/><spot name="die" ink="Dieline" value="{die}" overprint="true"/></spots>{gradients}"#
    );
    let content = r#"<row gap="m"><column width="120pt" height="40pt" background="brand"/><column width="120pt" height="40pt" background="brand 30%" border="m die"/>
        <column width="120pt" height="40pt" background="gradient:brand-fade"/></row>"#;
    let xml = match xml.find("</tokens>") {
        Some(at) => format!("{}{tokens}{}", &xml[..at], &xml[at..]),
        None => {
            let at = xml.find("<document").expect("a document");
            format!("{}<tokens>{tokens}</tokens>{}", &xml[..at], &xml[at..])
        }
    };
    let at = xml.find("</layout>").expect("a layout");
    format!("{}{content}{}", &xml[..at], &xml[at..])
}

#[test]
fn pdfa_takes_a_spot_color_with_an_rgb_stand_in_and_refuses_a_cmyk_one() {
    let fixture = crate::pdfa_render_tests::fixture("2b");
    if let Err(error) = crate::pdfa_render_tests::render(&with_spots(&fixture, "#e35205", "#ff00ff")) {
        panic!("{error}");
    }
    let error = crate::pdfa_render_tests::render(&with_spots(&fixture, "cmyk(0, 85, 100, 0)", "#ff00ff")).unwrap_err();
    assert!(
        error.contains("the spot color 'brand' stands in for its ink with the CMYK color cmyk(0, 85, 100, 0), and the sRGB output intent of a PDF/A file covers RGB colors only"),
        "{error}"
    );
}

#[test]
fn pdfx_takes_a_spot_color_and_refuses_a_cmyk_stand_in_without_a_cmyk_press_profile() {
    let xml = with_spots(&crate::pdfx_render_tests::fixture(), "cmyk(0, 85, 100, 0)", "cmyk(0, 100, 0, 0)");
    let pdf = text(&crate::pdfx_render_tests::render(&xml).expect("renders"));
    assert!(pdf.contains("/Separation /PANTONE#201665#20C /DeviceCMYK"), "{pdf}");
    // A document of RGB colors, but for the spot colors' stand-ins.
    let rgb = crate::pdfx_render_tests::document("", "", "<text>Flyer</text>");
    let error = crate::pdfx_render_tests::render_with_rgb_press(&with_spots(&rgb, "cmyk(0, 85, 100, 0)", "#ff00ff")).unwrap_err();
    assert!(error.contains("the spot color 'brand' stands in for its ink with the CMYK color cmyk(0, 85, 100, 0)") && error.contains("RGB color space"), "{error}");
    // An RGB stand-in goes to sRGB with the other RGB colors.
    if let Err(error) = crate::pdfx_render_tests::render_with_rgb_press(&with_spots(&rgb, "#e35205", "#ff00ff")) {
        panic!("{error}");
    }
}

/// The PDF/X fixture with spot colors in CMYK, for `make check-pdfx`.
pub(crate) fn pdfx_sample() -> Vec<u8> {
    crate::pdfx_render_tests::render(&with_spots(&crate::pdfx_render_tests::fixture(), "cmyk(0, 85, 100, 0)", "cmyk(0, 100, 0, 0)"))
        .expect("renders")
}

/// The PDF/A-2u fixture with spot colors in RGB, for `make check-pdfa`.
pub(crate) fn pdfa_sample() -> Vec<u8> {
    crate::pdfa_render_tests::render(&with_spots(&crate::pdfa_render_tests::fixture("2u"), "#e35205", "#ff00ff")).expect("renders")
}

// ── The plates ────────────────────────────────────────────────────────────────

/// A page of 200 by 100pt for `make check-spot`, whose plates Ghostscript renders at 72 dpi, a pixel a point. Each
/// square is 20pt at a known place, so the check reads the middle of each on each plate (`scripts/check-spot.mjs`):
///
/// - `brand` solid at (10, 10) and as a 30% tint at (40, 10), on its own plate, and on none of the process plates;
/// - cyan at (70, 10) under `varnish`, which overprints: the cyan plate still prints there, and the varnish's;
/// - cyan at (100, 10) under `knock`, which doesn't: the cyan plate is empty there, where knock prints;
/// - a gradient of `brand` from 100% at the left to 0% at the right across (10, 50) to (190, 70).
pub(crate) fn plates_sample() -> Vec<u8> {
    let xml = r#"<lpdf version="1"><tokens>
        <spots>
          <spot name="brand" ink="PANTONE 1665 C" value="cmyk(0, 85, 100, 0)"/>
          <spot name="varnish" ink="Varnish" value="cmyk(0, 0, 0, 10)" overprint="true"/>
          <spot name="knock" ink="Knockout" value="cmyk(0, 0, 100, 0)"/>
        </spots>
        <gradients><gradient name="ramp" type="linear" from="brand" to="brand 0%" angle="90"/></gradients>
      </tokens>
      <document size="200pt 100pt"><section><layout margin="0"/><layer>
        <rect x="10pt" y="10pt" width="20pt" height="20pt" fill="brand"/>
        <rect x="40pt" y="10pt" width="20pt" height="20pt" fill="brand 30%"/>
        <rect x="70pt" y="10pt" width="20pt" height="20pt" fill="cmyk(100, 0, 0, 0)"/>
        <rect x="70pt" y="10pt" width="20pt" height="20pt" fill="varnish"/>
        <rect x="100pt" y="10pt" width="20pt" height="20pt" fill="cmyk(100, 0, 0, 0)"/>
        <rect x="100pt" y="10pt" width="20pt" height="20pt" fill="knock"/>
        <rect x="10pt" y="50pt" width="180pt" height="20pt" fill="gradient:ramp"/>
      </layer></section></document></lpdf>"#;
    // Licensed, so that no attribution line prints on the plates.
    let mut doc = crate::parse::parse(xml).expect("parses");
    let layouts = doc.section_layouts();
    let pages = crate::layout::layout_document(&layouts).expect("lays out");
    let (fonts, images) = (crate::pdf::FontRegistry::new(), crate::pdf::ImageRegistry::new());
    crate::pdf::render_pdf(&pages, &doc.fonts, &fonts, &images, &doc.meta, doc.paints(), None, true, &doc.output, &[], None)
        .expect("renders")
}

/// Writes the sample of `make check-spot` into the folder `LPDF_SPOT_OUT` names. Without the variable the test does
/// nothing, so a plain `cargo test` needs no other tool.
#[test]
fn writes_the_spot_sample_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_SPOT_OUT") else { return };
    let dir = std::path::Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the sample's folder");
    std::fs::write(dir.join("plates.pdf"), plates_sample()).expect("a sample");
}
