//! A section whose page grows to fit its content (`grow`): how far it grows in height and in width, what takes the
//! final size on a grown page (the footer, the footnotes, `fill` and `full`, the flow's width, a layer's anchor, the
//! background and the ruling), and the errors of a page that would grow too large and of a section in columns.

use super::*;
use crate::parse::{parse, parse_tree};
use serde_json::Value;

/// The margin of every layout here.
const MARGIN: f64 = 10.0;
/// 80mm, 40mm and 100mm in points: a till roll's width, and two starting heights.
const MM_80: f64 = 226.77;
const MM_40: f64 = 113.39;
const MM_100: f64 = 283.46;
/// The width of an A5 page, which every document here has.
const A5_WIDTH: f64 = 419.53;

/// A document on A5 of one section with `attrs`, whose layout of a 10pt margin holds `layout`.
fn document(attrs: &str, layout: &str) -> String {
    format!(r#"<lpdf version="1"><document size="a5"><section {attrs}><layout margin="10pt">{layout}</layout></section></document></lpdf>"#)
}

/// `n` lines, each a text of its own: "Line 1", "Line 2" and so on.
fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("<text>Line {i}</text>")).collect()
}

/// The pages a document lays out, as the tree the engine hands out.
fn pages(xml: &str) -> Vec<Value> {
    let pages = layout_document(&parse(xml).unwrap().section_layouts()).unwrap();
    let tree: Value = serde_json::from_str(&crate::render::pages_to_json(&pages, Value::Null, Value::Null)).unwrap();
    tree["pages"].as_array().cloned().unwrap_or_default()
}

/// The one page a document lays out.
fn one_page(xml: &str) -> Value {
    let mut pages = pages(xml);
    assert_eq!(pages.len(), 1, "one page");
    pages.remove(0)
}

/// The error the layout of a document fails with.
fn layout_error(xml: &str) -> String {
    match layout_document(&parse(xml).unwrap().section_layouts()) {
        Ok(_) => panic!("the layout fails"),
        Err(error) => error,
    }
}

/// The width and the height of a page.
fn size(page: &Value) -> (f64, f64) {
    (page["width"].as_f64().unwrap(), page["height"].as_f64().unwrap())
}

/// Every node of a page, depth first.
fn nodes(page: &Value) -> Vec<&Value> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        out.push(v);
        if let Some(children) = v["nodes"].as_array() {
            for child in children { walk(child, out); }
        }
    }
    let mut out = Vec::new();
    walk(page, &mut out);
    out
}

/// The node of `kind` drawn in `fill` on a page.
fn filled<'a>(page: &'a Value, kind: &str, fill: &str) -> &'a Value {
    nodes(page).into_iter()
        .find(|v| v["type"] == kind && v["fill"] == fill)
        .unwrap_or_else(|| panic!("no {kind} in {fill}"))
}

/// The top and the bottom of the box drawn in `fill` on a page.
fn box_edges(page: &Value, fill: &str) -> (f64, f64) {
    let b = filled(page, "box", fill);
    let y = b["y"].as_f64().unwrap();
    (y, y + b["height"].as_f64().unwrap())
}

/// The texts of a page, by their words.
fn texts(page: &Value) -> Vec<&str> {
    nodes(page).into_iter().filter(|v| v["type"] == "text").filter_map(|v| v["content"].as_str()).collect()
}

/// The y of the text `content` on a page.
fn y_of(page: &Value, content: &str) -> f64 {
    nodes(page).into_iter()
        .find(|v| v["type"] == "text" && v["content"] == content)
        .unwrap_or_else(|| panic!("no text {content}"))["y"].as_f64().unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.02
}

/// A table of two `auto` columns of one cell each, `cols` wide, whose cells are wider together than an A5 page.
fn wide_table(cols: &str) -> String {
    format!(
        "<table cols=\"{cols}\"><tr><td><text>{FIRST_CELL}</text></td><td><text>{SECOND_CELL}</text></td></tr></table>"
    )
}

const FIRST_CELL: &str = "The first cell of the table, long enough to pass half of the page on one line";
const SECOND_CELL: &str = "The second cell, which is about as long as the first one";

// ── Height ────────────────────────────────────────────────────────────────────

#[test]
fn a_page_that_grows_in_height_holds_its_flow_as_tall_as_the_content_and_the_margin() {
    let xml = document(r#"size="80mm 40mm" grow="height""#, &format!(r##"<column background="#eeeeee">{}</column>"##, lines(100)));
    let page = one_page(&xml);
    let (width, height) = size(&page);
    let (_, bottom) = box_edges(&page, "#eeeeee");
    assert!(close(width, MM_80), "the width stays: {width}");
    assert!(height > MM_40 * 5.0, "the page grew: {height}");
    assert!(close(height, bottom + MARGIN), "the page ends a margin under the content: {height}, {bottom}");
    assert_eq!(texts(&page).len(), 100);
}

#[test]
fn a_short_flow_keeps_the_starting_size() {
    let page = one_page(&document(r#"size="80mm 100mm" grow="height""#, &lines(2)));
    assert!(close(size(&page).1, MM_100), "{:?}", size(&page));
}

#[test]
fn a_break_in_the_flow_of_a_page_that_grows_in_height_starts_no_new_page() {
    let page = one_page(&document(r#"size="80mm 40mm" grow="height""#, r#"<text>One</text><text paginate="break-before">Two</text>"#));
    assert!(y_of(&page, "Two") > y_of(&page, "One"));
}

#[test]
fn without_grow_the_same_flow_paginates() {
    assert!(pages(&document(r#"size="80mm 40mm""#, &lines(100))).len() > 5);
}

// ── Width ─────────────────────────────────────────────────────────────────────

#[test]
fn a_page_that_grows_in_width_gives_each_auto_column_its_cells_on_one_line() {
    let page = one_page(&document(r#"grow="width""#, &wide_table("auto auto")));
    let (width, height) = size(&page);
    assert!(width > A5_WIDTH, "the page grew: {width}");
    assert!(close(height, 595.28), "the height stays: {height}");
    assert_eq!(texts(&page), [FIRST_CELL, SECOND_CELL], "each cell on one line");
}

#[test]
fn a_column_of_a_fixed_width_keeps_it_and_wraps_its_cells() {
    let page = one_page(&document(r#"grow="width""#, &format!(
        "<table cols=\"auto 120pt\"><tr><td><text>{FIRST_CELL}</text></td><td><text>{SECOND_CELL}</text></td></tr></table>"
    )));
    let texts = texts(&page);
    assert_eq!(texts[0], FIRST_CELL, "the auto column on one line");
    assert!(texts.len() > 2, "the fixed column wraps: {texts:?}");
    let fixed: Vec<&Value> = nodes(&page).into_iter().filter(|v| v["type"] == "text" && v["content"] != FIRST_CELL).collect();
    assert!(fixed.iter().all(|t| t["line_width"].as_f64().unwrap() <= 120.0), "in 120pt");
}

#[test]
fn a_cell_that_spans_a_fixed_and_an_auto_column_widens_the_auto_one() {
    let spanning = "A spanning cell that is much longer than both of the columns under it, long enough to pass the page on one line";
    for cols in ["100pt auto", "auto 100pt"] {
        let page = one_page(&document(r#"grow="width""#, &format!(
            r#"<table cols="{cols}"><tr><td colspan="2"><text>{spanning}</text></td></tr><tr><td><text>a</text></td><td><text>b</text></td></tr></table>"#
        )));
        assert!(size(&page).0 > A5_WIDTH, "{cols}: the page grew: {:?}", size(&page));
        assert_eq!(texts(&page)[0], spanning, "{cols}: the spanning cell on one line");
    }
}

#[test]
fn fr_and_percent_columns_grow_the_page_as_auto_columns_do() {
    let auto = size(&one_page(&document(r#"grow="width""#, &wide_table("auto auto")))).0;
    for cols in ["1fr 1fr", "1fr 2fr", "50% 50%"] {
        let width = size(&one_page(&document(r#"grow="width""#, &wide_table(cols)))).0;
        assert!(close(width, auto), "{cols}: {width}, auto auto: {auto}");
    }
}

#[test]
fn a_fixed_column_keeps_its_width_beside_percent_columns() {
    // A % column is a share of the whole table, so 400pt beside 50% needs a table of 800pt, gaps and padding added.
    for (cols, attrs, table) in [
        ("400pt 50%", "", 800.0),
        ("400pt 25% 25%", "", 800.0),
        ("400pt 50%", r#" gap="10pt" padding="5pt""#, 820.0 + 10.0),
    ] {
        let page = one_page(&document(r#"grow="width""#, &format!(
            r#"<table cols="{cols}"{attrs}><tr><td><text>Fixed</text></td><td><text align="right">Short</text></td>{}</tr></table>"#,
            if cols.matches('%').count() == 2 { "<td><text>Third</text></td>" } else { "" },
        )));
        let width = size(&page).0;
        assert!(close(width, table + 2.0 * MARGIN), "{cols}{attrs}: {width}");
        // A text aligned right is placed by its right edge, the right of the % cell.
        let short = nodes(&page).into_iter().find(|v| v["type"] == "text" && v["content"] == "Short").unwrap();
        let right = short["x"].as_f64().unwrap();
        assert!(right <= width - MARGIN + 0.02, "{cols}{attrs}: the % cell ends inside the page: {right}, {width}");
    }
    // An fr column shares only what the fixed ones leave, and percents that take the whole table fit no width.
    for cols in ["400pt 1fr", "100pt 100%"] {
        let page = one_page(&document(r#"grow="width""#, &format!(
            r#"<table cols="{cols}"><tr><td><text>Fixed</text></td><td><text>Short</text></td></tr></table>"#
        )));
        assert!(size(&page).0 < 500.0, "{cols}: measured by its cells: {:?}", size(&page));
    }
}

#[test]
fn a_box_or_an_image_of_a_fixed_width_grows_the_page() {
    let boxed = one_page(&document(r#"grow="width""#, r##"<column width="700pt" background="#eeeeee"><text>Wide</text></column>"##));
    assert!(close(size(&boxed).0, 700.0 + 2.0 * MARGIN), "{:?}", size(&boxed));
    let image = one_page(&document(r#"grow="width""#, r#"<img name="logo" width="600pt" height="20pt"/>"#)
        .replace("<document", r#"<assets><image name="logo"/></assets><document"#));
    assert!(close(size(&image).0, 600.0 + 2.0 * MARGIN), "{:?}", size(&image));
}

#[test]
fn paragraphs_and_code_never_widen_the_page() {
    let long = "words ".repeat(200);
    let page = one_page(&document(r#"grow="width""#, &format!("<text>{long}</text><code>{}</code>", "x".repeat(300))));
    assert!(close(size(&page).0, A5_WIDTH), "{:?}", size(&page));
}

#[test]
fn the_flow_is_laid_out_at_the_final_width() {
    let page = one_page(&document(r#"grow="width""#, &format!("<divider/>{}", wide_table("auto auto"))));
    let width = size(&page).0;
    let line = nodes(&page).into_iter().find(|v| v["type"] == "line").expect("the divider");
    assert!(close(line["x1"].as_f64().unwrap(), MARGIN));
    assert!(close(line["x2"].as_f64().unwrap(), width - MARGIN), "the divider spans the grown page: {line}, {width}");
}

#[test]
fn a_page_grows_in_width_and_in_height() {
    let page = one_page(&document(r#"size="80mm 40mm" grow="width height""#, &format!("{}{}", wide_table("auto auto"), lines(40))));
    let (width, height) = size(&page);
    assert!(width > A5_WIDTH && height > MM_40 * 3.0, "{width} {height}");
    assert_eq!(texts(&page)[..2], [FIRST_CELL, SECOND_CELL]);
}

// ── On a grown page ───────────────────────────────────────────────────────────

/// The flow of `n` lines in a box, a text with a footnote, and a footer in a box.
fn noted_with_footer(n: usize) -> String {
    format!(
        r##"<column background="#eeeeee">{}<text>Noted<fn>The note.</fn></text></column><region pin="bottom"><column background="#dddddd"><text>Footer</text></column></region>"##,
        lines(n)
    )
}

#[test]
fn the_footer_and_the_footnotes_sit_at_the_foot_of_the_grown_page_right_under_the_content() {
    let page = one_page(&document(r#"size="80mm 40mm" grow="height""#, &noted_with_footer(30)));
    let height = size(&page).1;
    let (_, flow_bottom) = box_edges(&page, "#eeeeee");
    let (footer_top, footer_bottom) = box_edges(&page, "#dddddd");
    assert!(close(footer_bottom, height - MARGIN), "the footer at the foot: {footer_bottom}, {height}");
    let rule = nodes(&page).into_iter().find(|v| v["type"] == "line" && v["color"] == "#888888").expect("the notes' rule");
    // The room above the rule and half of it.
    assert!(close(rule["y1"].as_f64().unwrap(), flow_bottom + 6.25), "the notes right under the content: {rule}, {flow_bottom}");
    let note = y_of(&page, "The note.");
    assert!(note > flow_bottom && note < footer_top, "the note between the content and the footer");
}

#[test]
fn on_a_page_that_keeps_its_starting_size_the_footer_stays_at_its_foot() {
    let page = one_page(&document(r#"size="80mm 100mm" grow="height""#, &noted_with_footer(2)));
    assert!(close(size(&page).1, MM_100));
    assert!(close(box_edges(&page, "#dddddd").1, MM_100 - MARGIN));
}

#[test]
fn fill_takes_the_rest_of_a_page_that_keeps_its_size_and_its_content_on_a_grown_one() {
    let kept = one_page(&document(
        r#"size="80mm 100mm" grow="height""#,
        r##"<text>Top</text><column height="fill" background="#ff0000"><text>Fill</text></column>"##,
    ));
    assert!(close(box_edges(&kept, "#ff0000").1, MM_100 - MARGIN), "to the foot of the starting size");

    let column = |height: &str| format!(r##"<text>Top</text><column {height} background="#ff0000">{}</column>"##, lines(40));
    let grown = one_page(&document(r#"size="80mm 40mm" grow="height""#, &column(r#"height="fill""#)));
    let auto = one_page(&document(r#"size="80mm 40mm" grow="height""#, &column("")));
    assert_eq!(box_edges(&grown, "#ff0000"), box_edges(&auto, "#ff0000"), "as tall as its content");
    assert!(close(size(&grown).1, box_edges(&grown, "#ff0000").1 + MARGIN));
}

#[test]
fn full_is_the_room_of_the_starting_size() {
    let page = one_page(&document(
        r#"size="80mm 40mm" grow="height""#,
        &format!(r##"<column height="full" background="#00ff00"><text>Full</text></column>{}"##, lines(20)),
    ));
    let (top, bottom) = box_edges(&page, "#00ff00");
    assert!(close(bottom - top, MM_40 - 2.0 * MARGIN), "{top} {bottom}");
    assert!(size(&page).1 > MM_40 * 2.0);
}

#[test]
fn a_layer_places_its_anchors_against_the_grown_page_and_its_plain_places_from_the_top_left() {
    let xml = format!(
        r##"<lpdf version="1"><document size="a5"><section size="80mm 40mm" grow="width height">
            <layer>
              <rect anchor="bottom-right" x="-5pt" y="-5pt" width="20pt" height="10pt" fill="#0000ff"/>
              <rect x="5pt" y="5pt" width="20pt" height="10pt" fill="#00ff00"/>
              <circle anchor="center" r="4pt" fill="#ff00ff"/>
            </layer>
            <layout margin="10pt">{}{}</layout>
            <layer><text-at anchor="bottom-center" y="-2pt">Foot</text-at></layer>
        </section></document></lpdf>"##,
        wide_table("auto auto"),
        lines(30)
    );
    let page = one_page(&xml);
    let (width, height) = size(&page);
    let anchored = filled(&page, "canvas-rect", "#0000ff");
    assert!(close(anchored["x"].as_f64().unwrap(), width - 25.0) && close(anchored["y"].as_f64().unwrap(), height - 15.0), "{anchored}");
    let plain = filled(&page, "canvas-rect", "#00ff00");
    assert!(close(plain["x"].as_f64().unwrap(), 5.0) && close(plain["y"].as_f64().unwrap(), 5.0), "{plain}");
    let centered = filled(&page, "canvas-ellipse", "#ff00ff");
    assert!(close(centered["cx"].as_f64().unwrap(), width / 2.0) && close(centered["cy"].as_f64().unwrap(), height / 2.0));
    assert!(y_of(&page, "Foot") > height - 20.0, "the text-at at the foot of the grown page");
}

#[test]
fn the_background_and_the_ruling_cover_the_grown_page() {
    let xml = format!(
        r##"<lpdf version="1"><tokens><rulings><ruling name="notes"><lines step-y="8mm"/></ruling></rulings></tokens>
        <document size="a5"><section size="80mm 40mm" grow="height" background="#ffeeee" ruling="notes">
        <layout margin="10pt">{}</layout></section></document></lpdf>"##,
        lines(60)
    );
    let page = one_page(&xml);
    let (width, height) = size(&page);
    assert_eq!(page["background"], "#ffeeee");
    let ruling = nodes(&page).into_iter().find(|v| v["type"] == "ruling").expect("the ruling");
    let set = &ruling["sets"][0];
    assert_eq!(set["area"], serde_json::json!([0.0, 0.0, width, height]), "the lines fill the grown page");
    let across = set["across"]["count"].as_u64().unwrap() as f64;
    assert!(across > height / 22.68 - 2.0, "lines down the whole page: {across} in {height}");
}

#[test]
fn a_table_of_contents_section_grows_too() {
    let xml = r#"<lpdf version="1"><document size="a5"><section type="toc" size="80mm 40mm" grow="height"><layout margin="10pt">
        <text data-source="$toc" data-value="label">-</text></layout></section>
        <section><layout>{}</layout></section></document></lpdf>"#;
    let bookmarked: String = (1..=12).map(|n| format!(r#"<bookmark label="Entry {n}"/><text>Entry {n}</text>"#)).collect();
    let pages = pages(&xml.replace("{}", &bookmarked));
    assert_eq!(pages.len(), 2, "the contents on one page, then the section it lists");
    assert!(size(&pages[0]).1 > MM_40);
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[test]
fn a_page_that_would_grow_past_the_largest_pdf_page_is_an_error_that_names_the_section() {
    let tall = format!(
        r#"<lpdf version="1"><document size="a5"><section><layout><text>Before</text></layout></section><section size="80mm 40mm" grow="height"><layout margin="10pt">{}</layout></section></document></lpdf>"#,
        lines(1300)
    );
    assert_eq!(
        layout_error(&tall),
        "the page of section 2 would grow past 14,400 pt (200 in) in height, the largest page PDF 1.7 recommends"
    );
    let wide = document(r#"grow="width""#, r#"<column width="15000pt"><text>Wide</text></column>"#);
    assert_eq!(
        layout_error(&wide),
        "the page of section 1 would grow past 14,400 pt (200 in) in width, the largest page PDF 1.7 recommends"
    );
}

#[test]
fn grow_with_columns_or_an_inset_is_an_error_on_both_paths() {
    let says = "grow doesn't work with columns or an inset, in section 1";
    let columns = document(r#"columns="2" grow="height""#, "<text>A</text>");
    let inset = document(r#"grow="width""#, r#"<region pin="inset" top="10pt" height="20pt"><text>Inset</text></region><text>A</text>"#);
    for xml in [columns, inset] {
        let error = parse(&xml).map(|_| ()).unwrap_err();
        assert!(error.starts_with(says), "{error}");
    }
    let tree = r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{"columns":2,"grow":"height"},
        "nodes":[{"type":"layout","attrs":{},"nodes":[{"type":"text","attrs":{},"nodes":["A"]}]}]}]}"#;
    let error = parse_tree(tree).map(|_| ()).unwrap_err();
    assert!(error.starts_with(says), "{error}");
}

#[test]
fn an_unknown_grow_is_an_error() {
    let error = parse(&document(r#"grow="depth""#, "<text>A</text>")).map(|_| ()).unwrap_err();
    assert!(error.contains("invalid grow 'depth': expected width, height or width height"), "{error}");
}

#[test]
fn a_page_under_three_points_is_an_error_on_a_section_and_a_document_on_both_paths() {
    let section = document(r#"size="80mm 0""#, "<text>A</text>");
    let error = parse(&section).map(|_| ()).unwrap_err();
    assert!(error.contains("page size '80mm 0' is under 3pt high, the smallest page PDF 1.7 recommends"), "{error}");
    let page = r#"<lpdf version="1"><document size="2pt 100pt"><section><layout><text>A</text></layout></section></document></lpdf>"#;
    let error = parse(page).map(|_| ()).unwrap_err();
    assert!(error.contains("page size '2pt 100pt' is under 3pt wide"), "{error}");
    // 3pt is the smallest page there is.
    assert!(parse(&document(r#"size="3pt 3pt""#, "<text>A</text>")).is_ok());

    let tree = |document: &str, section: &str| format!(
        r#"{{"version":1,"type":"document","attrs":{{{document}}},"nodes":[{{"type":"section","attrs":{{{section}}},"nodes":[]}}]}}"#
    );
    let error = parse_tree(&tree("", r#""size":"80mm 0""#)).map(|_| ()).unwrap_err();
    assert!(error.contains("page size '80mm 0' is under 3pt high"), "{error}");
    let error = parse_tree(&tree(r#""size":"2pt 100pt""#, "")).map(|_| ()).unwrap_err();
    assert!(error.contains("page size '2pt 100pt' is under 3pt wide"), "{error}");
}
