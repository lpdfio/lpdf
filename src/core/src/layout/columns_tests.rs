//! A section's flow in columns, and the insets placed in them (`columns`): where the columns are, what ends one,
//! how the last page fills, the rule between them, where an inset goes and keeps the flow out, and the errors a
//! document gets for an inset that can't be placed. The fixtures `showcase-columns*.xml` show the same on a page.

use super::*;
use crate::parse::{parse, parse_tree};
use serde_json::Value;

/// The left of the flow, and its width, on an A4 page with a 40pt margin.
const LEFT: f64 = 40.0;
const WIDTH: f64 = 515.28;

/// A document of one section with `attrs` whose layout holds `layout`, on A4 with a 40pt margin.
fn section(attrs: &str, layout: &str) -> String {
    format!(r#"<lpdf version="1"><document size="a4"><section {attrs}><layout margin="40pt">{layout}</layout></section></document></lpdf>"#)
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

/// The texts of a page: content, x and y.
fn texts(page: &Value) -> Vec<(String, f64, f64)> {
    nodes(page).into_iter()
        .filter(|v| v["type"] == "text")
        .map(|v| (v["content"].as_str().unwrap_or("").to_string(), v["x"].as_f64().unwrap_or(0.0), v["y"].as_f64().unwrap_or(0.0)))
        .collect()
}

/// The x of the text `content` on `page`.
fn x_of(page: &Value, content: &str) -> f64 {
    texts(page).into_iter().find(|t| t.0 == content).unwrap_or_else(|| panic!("no text {content}")).1
}

/// The vertical lines of a page in `color`: x, top and bottom.
fn rules(page: &Value, color: &str) -> Vec<(f64, f64, f64)> {
    nodes(page).into_iter()
        .filter(|v| v["type"] == "line" && v["color"] == color && v["x1"] == v["x2"])
        .map(|v| (v["x1"].as_f64().unwrap_or(0.0), v["y1"].as_f64().unwrap_or(0.0), v["y2"].as_f64().unwrap_or(0.0)))
        .collect()
}

/// The numbers of the lines "Line n" on a page whose x is `x`.
fn lines_at(page: &Value, x: f64) -> Vec<usize> {
    texts(page).into_iter()
        .filter(|t| same(t.1, x))
        .filter_map(|t| t.0.strip_prefix("Line ").and_then(|n| n.parse().ok()))
        .collect()
}

/// Whether two places on a page are the same, as the tree rounds them to two decimals.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

fn pdf(xml: &str) -> Vec<u8> {
    crate::LpdfEngine::render_xml_to_pdf_bytes(xml).unwrap()
}

#[test]
fn a_flow_in_columns_fills_each_column_in_order_then_the_next_page() {
    let pages = pages(&section(r#"columns="2" column-gap="20pt""#, &lines(150)));
    let w = (WIDTH - 20.0) / 2.0;
    assert_eq!(pages.len(), 2, "150 lines take two pages in two columns");
    let first = lines_at(&pages[0], LEFT);
    let second = lines_at(&pages[0], LEFT + w + 20.0);
    assert!(!first.is_empty() && !second.is_empty(), "both columns of the first page hold lines");
    assert_eq!(first.last().map(|n| n + 1), second.first().copied(), "the second column goes on from the first");
    assert_eq!(second.last().map(|n| n + 1), lines_at(&pages[1], LEFT).first().copied(), "the next page goes on from the second column");
}

#[test]
fn every_column_is_as_wide_as_the_flow_leaves_after_the_gaps() {
    let page = &pages(&section(r#"columns="3" column-gap="20pt""#, &lines(200)))[0];
    let w = (WIDTH - 40.0) / 3.0;
    let line = lines_at(page, LEFT + 2.0 * (w + 20.0));
    assert!(!line.is_empty(), "the third column starts two columns and two gaps from the left");
}

#[test]
fn a_section_of_one_column_lays_out_as_one_without_columns() {
    let plain = pdf(&section("", &lines(80)));
    assert_eq!(pdf(&section(r#"columns="1""#, &lines(80))), plain);
    assert_eq!(pdf(&section(r#"column-gap="xl" column-rule="1pt #000000" column-fill="balance""#, &lines(80))), plain);
}

#[test]
fn the_column_gap_defaults_to_the_l_space_token() {
    let page = &pages(&section(r#"columns="2""#, &lines(120)))[0];
    let w = (WIDTH - 14.0) / 2.0;
    assert!(!lines_at(page, LEFT + w + 14.0).is_empty(), "the second column starts 14pt after the first");
}

#[test]
fn break_before_starts_the_next_column() {
    let layout = r#"<text>One</text><text paginate="break-before">Two</text><text paginate="break-before">Three</text>"#;
    let page = &pages(&section(r#"columns="3" column-gap="20pt""#, layout))[0];
    let w = (WIDTH - 40.0) / 3.0;
    assert!(same(x_of(page, "One"), LEFT));
    assert!(same(x_of(page, "Two"), LEFT + w + 20.0));
    assert!(same(x_of(page, "Three"), LEFT + 2.0 * (w + 20.0)));
}

#[test]
fn balance_levels_the_columns_of_the_last_page() {
    let pages = pages(&section(r#"columns="3" column-gap="20pt" column-fill="balance""#, &lines(40)));
    let w = (WIDTH - 40.0) / 3.0;
    let counts: Vec<usize> = (0..3).map(|i| lines_at(&pages[0], LEFT + i as f64 * (w + 20.0)).len()).collect();
    assert_eq!(counts.iter().sum::<usize>(), 40);
    // The least height that holds 40 lines in three columns is 14 lines: the first two hold 14 and the third the rest.
    assert_eq!(counts, [14, 14, 12]);
}

#[test]
fn without_balance_the_last_page_fills_in_order() {
    let pages = pages(&section(r#"columns="3" column-gap="20pt""#, &lines(40)));
    let w = (WIDTH - 40.0) / 3.0;
    assert_eq!(lines_at(&pages[0], LEFT).len(), 40, "all in the first column");
    assert!(lines_at(&pages[0], LEFT + w + 20.0).is_empty());
}

#[test]
fn the_rule_is_drawn_only_between_columns_that_hold_something() {
    let attrs = r##"columns="3" column-gap="20pt" column-rule="1pt #ff0000""##;
    let full = &pages(&section(attrs, &lines(200)))[0];
    assert_eq!(rules(full, "#ff0000").len(), 2, "two gaps between three full columns");
    let w = (WIDTH - 40.0) / 3.0;
    assert!(same(rules(full, "#ff0000")[0].0, LEFT + w + 10.0), "in the middle of the first gap");
    // Content for a column and a half: one rule.
    let short = &pages(&section(attrs, &lines(90)))[0];
    assert_eq!(rules(short, "#ff0000").len(), 1);
    // One column of content: none.
    let one = &pages(&section(attrs, &lines(10)))[0];
    assert!(rules(one, "#ff0000").is_empty());
}

#[test]
fn the_rule_runs_from_the_top_of_the_columns_to_the_foot_of_the_tallest() {
    let attrs = r##"columns="2" column-gap="20pt" column-rule="1pt #ff0000""##;
    let page = &pages(&section(attrs, &lines(90)))[0];
    let rule = rules(page, "#ff0000")[0];
    let lowest = texts(page).iter().map(|t| t.2).fold(0.0_f64, f64::max);
    assert_eq!(rule.1, 40.0, "from the top of the flow");
    assert!(rule.2 > lowest && rule.2 < lowest + 20.0, "to the foot of the first column, the taller: {rule:?}, last line at {lowest}");
}

#[test]
fn an_inset_keeps_the_flow_of_the_columns_it_covers_out_of_its_rectangle() {
    let layout = format!(r#"<region pin="inset" col="2" top="100pt" height="200pt"><text>Inset</text></region>{}"#, lines(200));
    let page = &pages(&section(r#"columns="3" column-gap="20pt""#, &layout))[0];
    let w = (WIDTH - 40.0) / 3.0;
    let x = LEFT + w + 20.0;
    assert!(same(x_of(page, "Inset"), x), "the inset is laid out in its column");
    let (top, bottom) = (40.0 + 100.0, 40.0 + 300.0);
    let inside: Vec<_> = texts(page).into_iter().filter(|t| same(t.1, x) && t.0 != "Inset" && t.2 > top - 10.0 && t.2 < bottom).collect();
    assert!(inside.is_empty(), "no line of the flow in the inset: {inside:?}");
    assert!(texts(page).iter().any(|t| same(t.1, x) && t.2 >= bottom), "the column carries on below the inset");
    // The columns it doesn't cover run straight down.
    assert!(texts(page).iter().any(|t| same(t.1, LEFT) && t.2 > top && t.2 < bottom));
}

#[test]
fn an_inset_works_in_a_section_of_one_column() {
    let layout = format!(r#"<region pin="inset" top="100pt" height="200pt"><text>Inset</text></region>{}"#, lines(60));
    let page = &pages(&section("", &layout))[0];
    let inside = texts(page).into_iter().filter(|t| t.0 != "Inset" && t.2 > 130.0 && t.2 < 340.0).count();
    assert_eq!(inside, 0, "the flow goes around the inset");
}

#[test]
fn an_inset_s_content_taller_than_it_is_clipped_at_its_foot() {
    let layout = format!(r#"<region pin="inset" top="0pt" height="30pt">{}</region><text>After</text>"#, lines(10));
    let page = &pages(&section(r#"columns="2""#, &layout))[0];
    let clip = nodes(page).into_iter().find_map(|v| (v["type"] == "canvas-layer").then(|| v["clip"].clone()));
    assert_eq!(clip, Some(serde_json::json!([40.0, 40.0, 250.64, 30.0])));
}

#[test]
fn an_inset_on_odd_pages_follows_the_number_printed_on_the_page() {
    let inset = format!(r#"<region pin="inset" top="0pt" height="100pt" page="odd"><text>Inset</text></region>{}"#, lines(150));
    let xml = format!(
        r#"<lpdf version="1"><document size="a4"><section><layout><text>Before</text></layout></section><section columns="2"><layout margin="40pt">{inset}</layout></section></document></lpdf>"#
    );
    let pages = pages(&xml);
    let shows = |page: &Value| texts(page).iter().any(|t| t.0 == "Inset");
    assert!(!shows(&pages[1]), "the section's first page is page 2");
    assert!(shows(&pages[2]), "its second page is page 3");
}

#[test]
fn an_inset_on_last_goes_on_the_last_page_of_the_flow_filled_without_it() {
    let inset = r#"<region pin="inset" top="600pt" height="100pt" page="last"><text>Inset</text></region>"#;
    let pages = pages(&section(r#"columns="2""#, &format!("{inset}{}", lines(150))));
    assert_eq!(pages.len(), 2);
    assert!(texts(&pages[1]).iter().any(|t| t.0 == "Inset"));
    assert!(!texts(&pages[0]).iter().any(|t| t.0 == "Inset"));
}

#[test]
fn an_inset_on_last_that_pushes_text_on_stays_one_page_before_the_end() {
    // 110 lines fill the two columns of a page without the inset, and not with it.
    let inset = r#"<region pin="inset" col="1" colspan="2" top="500pt" height="261pt" page="last"><text>Inset</text></region>"#;
    let pages = pages(&section(r#"columns="2""#, &format!("{inset}{}", lines(110))));
    assert_eq!(pages.len(), 2, "the inset pushes lines on to a new page");
    assert!(texts(&pages[0]).iter().any(|t| t.0 == "Inset"), "the inset stays on the page it was placed on");
    assert!(!texts(&pages[1]).iter().any(|t| t.0 == "Inset"));
}

#[test]
fn an_inset_that_reaches_below_the_room_the_footer_leaves_is_an_error() {
    let layout = format!(
        r#"<region pin="bottom"><text>Footer</text></region><region pin="inset" top="700pt" height="100pt"><text>Inset</text></region>{}"#,
        lines(10)
    );
    let xml = section(r#"columns="2""#, &layout);
    let Err(err) = layout_document(&parse(&xml).unwrap().section_layouts()) else { panic!("laid out") };
    assert!(err.contains("<region> at line 1"), "{err}");
    assert!(err.contains("reaches 800pt below the top of the columns") && err.contains("page 1"), "{err}");
    let Err(render) = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml) else { panic!("rendered") };
    assert_eq!(render, err, "a render returns the layout's error");
}

#[test]
fn an_inset_that_fits_the_room_of_its_pages_is_no_error() {
    let layout = format!(r#"<region pin="inset" top="661.89pt" height="100pt"><text>Inset</text></region>{}"#, lines(10));
    assert!(layout_document(&parse(&section(r#"columns="2""#, &layout)).unwrap().section_layouts()).is_ok());
}

#[test]
fn a_note_does_not_rise_above_the_foot_of_an_inset_and_its_line_goes_on() {
    let note = "<text>Anchor<fn>A note of a few words.</fn></text>";
    let layout = format!(
        r#"<region pin="inset" colspan="2" top="700pt" height="60pt" page="1"><text>Inset</text></region>{}{note}{}"#,
        lines(40), lines(10)
    );
    let pages = pages(&section(r#"columns="2""#, &layout));
    let anchor_page = pages.iter().position(|p| texts(p).iter().any(|t| t.0.starts_with("Anchor"))).unwrap();
    assert_eq!(anchor_page, 1, "the line with the anchor goes on to the next page, with its note");
    assert!(texts(&pages[1]).iter().any(|t| t.0.contains("A note of a few words")));
}

#[test]
fn a_col_and_colspan_past_the_section_s_columns_are_an_error() {
    let xml = section(r#"columns="3""#, r#"<region pin="inset" col="2" colspan="3" top="0pt" height="10pt"><text>I</text></region>"#);
    let err = parse(&xml).unwrap_err();
    assert!(err.contains("col 2 and colspan 3 reach column 4, past the section's 3 columns"), "{err}");
    let one = section("", r#"<region pin="inset" col="2" top="0pt" height="10pt"><text>I</text></region>"#);
    assert!(parse(&one).unwrap_err().contains("past the section's 1 column;"));
}

#[test]
fn an_inset_needs_a_top_and_a_height() {
    let err = parse(&section(r#"columns="2""#, r#"<region pin="inset" top="0pt"><text>I</text></region>"#)).unwrap_err();
    assert!(err.contains("an inset needs a top and a height") && err.contains("height is missing"), "{err}");
    let err = parse(&section(r#"columns="2""#, r#"<region pin="inset" height="10pt"><text>I</text></region>"#)).unwrap_err();
    assert!(err.contains("top is missing"), "{err}");
}

#[test]
fn a_region_pinned_to_an_edge_takes_no_place_of_an_inset() {
    for attr in [r#"col="1""#, r#"colspan="1""#, r#"top="0pt""#, r#"height="10pt""#] {
        let err = parse(&section("", &format!(r#"<region pin="top" {attr}><text>H</text></region>"#))).unwrap_err();
        assert!(err.contains("a region pinned to an edge takes no"), "{attr}: {err}");
    }
}

#[test]
fn columns_take_a_whole_number_from_1_to_12_and_column_fill_auto_or_balance() {
    for value in ["0", "13", "two"] {
        let err = parse(&section(&format!(r#"columns="{value}""#), "<text>A</text>")).unwrap_err();
        assert!(err.contains("columns") && err.contains("1 to 12"), "{err}");
    }
    let err = parse(&section(r#"columns="2" column-fill="even""#, "<text>A</text>")).unwrap_err();
    assert!(err.contains("invalid column-fill 'even'"), "{err}");
}

#[test]
fn a_kit_tree_in_columns_with_an_inset_lays_out_as_its_xml() {
    let tree = r#"{"version":1,"type":"document","attrs":{"size":"a4"},"nodes":[
        {"type":"section","attrs":{"columns":3,"column-gap":"20pt","column-rule":"1pt #ff0000","column-fill":"balance"},"nodes":[
            {"type":"layout","attrs":{"margin":"40pt"},"nodes":[
                {"type":"region","attrs":{"pin":"inset","col":2,"colspan":2,"top":"50pt","height":"100pt"},"nodes":[{"type":"text","attrs":{},"nodes":["Inset"]}]},
                {"type":"text","attrs":{},"nodes":["One"]},
                {"type":"text","attrs":{"paginate":"break-before"},"nodes":["Two"]}
            ]}
        ]}
    ]}"#;
    let xml = section(
        r##"columns="3" column-gap="20pt" column-rule="1pt #ff0000" column-fill="balance""##,
        r#"<region pin="inset" col="2" colspan="2" top="50pt" height="100pt"><text>Inset</text></region><text>One</text><text paginate="break-before">Two</text>"#,
    );
    let from_tree = layout_document(&parse_tree(tree).unwrap().section_layouts()).unwrap();
    let tree_json = crate::render::pages_to_json(&from_tree, Value::Null, Value::Null);
    let xml_json = crate::render::pages_to_json(&layout_document(&parse(&xml).unwrap().section_layouts()).unwrap(), Value::Null, Value::Null);
    assert_eq!(tree_json, xml_json);
}

#[test]
fn the_content_of_an_inset_in_a_tagged_section_is_tagged() {
    let mut sections = parse(&section(r#"columns="2""#, r#"<region pin="inset" top="0pt" height="50pt"><text>Inset</text></region><text>Flow</text>"#))
        .unwrap()
        .section_layouts();
    sections[0].tagged = true;
    let pages = layout_document(&sections).unwrap();
    fn tagged_text(nodes: &[RenderNode], content: &str) -> bool {
        nodes.iter().any(|node| match node {
            RenderNode::Box(b) if b.tag.as_ref().is_some_and(|t| t.id > 0) => {
                b.children.iter().any(|c| matches!(c, RenderNode::Text(t) if t.content == content)) || tagged_text(&b.children, content)
            }
            RenderNode::Box(b)         => tagged_text(&b.children, content),
            RenderNode::CanvasLayer(l) => tagged_text(&l.children, content),
            _ => false,
        })
    }
    assert!(tagged_text(&pages[0].nodes, "Inset"), "the inset's text is in a tagged paragraph");
    assert!(tagged_text(&pages[0].nodes, "Flow"));
}

#[test]
fn columns_under_a_header_that_shows_the_page_number_are_laid_out_below_it() {
    // A header with {page} can be another height on some page, so the columns wait for it to be laid out.
    let layout = format!(r#"<region pin="top"><text>Page {{page}}</text></region>{}"#, lines(150));
    let pages = pages(&section(r#"columns="2" column-gap="20pt""#, &layout));
    let w = (WIDTH - 20.0) / 2.0;
    let header = texts(&pages[0]).into_iter().find(|t| t.0 == "Page 1").unwrap();
    let first = texts(&pages[0]).into_iter().find(|t| t.0 == "Line 1").unwrap();
    assert!(first.2 > header.2, "the first line is below the header");
    assert!(!lines_at(&pages[0], LEFT + w + 20.0).is_empty(), "the second column holds lines");
    assert_eq!(texts(&pages[1]).first().map(|t| t.0.as_str()), Some("Page 2"));
}
