//! The notes of footnotes in content taller than a page, which the bottom of the page cuts (`clip_overflow`): a test
//! for each section of `showcase-footnote-clip.xml`, which says which notes are on which page and where the cut fell,
//! and the cases around it that the fixture doesn't show.

use std::sync::OnceLock;

use super::*;
use crate::parse::parse;
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../test/fixtures/showcase-footnote-clip.xml");

/// The bottom of the body of an A4 page with a 40pt margin, above the fixture's footer of one line.
const ABOVE_FOOTER: f64 = 791.09;
/// The size of the number of an anchor in 11pt text, and of the words of a note.
const ANCHOR_SIZE: f64 = 6.82;
const NOTE_SIZE: f64 = 8.8;

/// The fixture, laid out once for every test.
fn tree() -> &'static Value {
    static TREE: OnceLock<Value> = OnceLock::new();
    TREE.get_or_init(|| render(FIXTURE))
}

/// A document laid out, as the tree the engine hands out.
fn render(xml: &str) -> Value {
    let mut doc = parse(xml).unwrap();
    let pages: Vec<_> = doc.section_layouts().iter().flat_map(layout_page).collect();
    serde_json::from_str(&crate::render::pages_to_json(&pages, Value::Null, Value::Null)).unwrap()
}

fn page_count() -> usize {
    tree()["pages"].as_array().map_or(0, Vec::len)
}

/// Page `page` of the fixture.
fn at(page: usize) -> &'static Value {
    &tree()["pages"][page]
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

/// The texts of a page, in the order they are drawn: content, x, y and size.
fn texts(page: &Value) -> Vec<(String, f64, f64, f64)> {
    nodes(page).into_iter()
        .filter(|v| v["type"] == "text")
        .map(|v| (
            v["content"].as_str().unwrap_or("").to_string(),
            v["x"].as_f64().unwrap_or(0.0), v["y"].as_f64().unwrap_or(0.0), v["size"].as_f64().unwrap_or(0.0),
        ))
        .collect()
}

/// The first page that shows `content`.
fn page_with(content: &str) -> usize {
    (0..page_count()).find(|&page| texts(at(page)).iter().any(|t| t.0.contains(content)))
        .unwrap_or_else(|| panic!("no page shows {content:?}"))
}

/// The first page with a note that starts with `words`.
fn page_with_note(words: &str) -> usize {
    (0..page_count()).find(|&page| notes(at(page)).iter().any(|note| note == words))
        .unwrap_or_else(|| panic!("no page has the note {words:?}"))
}

/// Where the cut of a page falls, if the page is cut.
fn cut(page: &Value) -> Option<f64> {
    nodes(page).into_iter()
        .find(|v| v["type"] == "canvas-layer")
        .and_then(|layer| layer["clip"][3].as_f64())
}

/// The numbers of the anchors of a page in reading order, with the top of each.
fn anchors(page: &Value) -> Vec<(String, f64)> {
    texts(page).into_iter().filter(|t| (t.3 - ANCHOR_SIZE).abs() < 0.01).map(|t| (t.0, t.2)).collect()
}

/// The numbers of the anchors of a page that the cut leaves in sight: all of them on a page that isn't cut.
fn shown(page: &Value) -> Vec<String> {
    let bottom = cut(page).unwrap_or(f64::MAX);
    anchors(page).into_iter().filter(|(_, y)| *y < bottom).map(|(n, _)| n).collect()
}

/// The numbers of the notes at the foot of a page, which hang at the left margin.
fn labels(page: &Value) -> Vec<String> {
    texts(page).into_iter().filter(|t| t.1 == 40.0 && t.3 < 6.0).map(|t| t.0).collect()
}

/// The words of the notes of a page, up to the colon the fixture's notes have.
fn notes(page: &Value) -> Vec<String> {
    texts(page).into_iter()
        .filter(|t| (t.3 - NOTE_SIZE).abs() < 0.01)
        .map(|t| t.0.split(':').next().unwrap_or("").to_string())
        .collect()
}

/// The top of the rule above the notes of a page, if it has one.
fn rule(page: &Value) -> Option<f64> {
    nodes(page).into_iter().find(|v| v["type"] == "line").and_then(|line| line["y1"].as_f64())
}

fn numbered(range: std::ops::RangeInclusive<u32>) -> Vec<String> {
    range.map(|n| n.to_string()).collect()
}

fn entries(prefix: &str, range: std::ops::RangeInclusive<u32>) -> Vec<String> {
    range.map(|n| format!("{prefix} {n}")).collect()
}

#[test]
fn every_page_has_the_notes_of_the_numbers_above_its_cut_and_no_others() {
    for page in 0..page_count() {
        assert_eq!(labels(at(page)), shown(at(page)), "page {}", page + 1);
        if let (Some(bottom), Some(rule)) = (cut(at(page)), rule(at(page))) {
            assert!(rule > bottom, "page {}: the notes start at {rule}, above the cut at {bottom}", page + 1);
        }
    }
}

#[test]
fn a_frame_taller_than_a_page_has_the_notes_of_the_numbers_shown_numbered_from_one() {
    let page = page_with("Counter: page.");
    let all = anchors(at(page));
    assert_eq!(all.len(), 24, "every entry of the frame is drawn, cut or not");
    let bottom = cut(at(page)).expect("the frame is cut");
    assert_eq!(labels(at(page)), numbered(1..=13));
    assert_eq!(notes(at(page)), entries("Night", 1..=13));
    assert!(all[13].1 >= bottom, "the 14th number is below the cut at {bottom}: {:?}", all[13]);
    // All 24 notes would take 309.94pt and cut the frame at 481.15. The notes of the numbers cut off take no room,
    // and the frame shows more of itself.
    assert!(bottom > ABOVE_FOOTER - 309.94 + 100.0, "the cut is at {bottom}");

    let next = page_with("After the frame, a text with a note");
    assert_eq!(next, page + 1);
    assert_eq!(anchors(at(next)).iter().map(|a| a.0.as_str()).collect::<Vec<_>>(), ["1"]);
    assert_eq!(labels(at(next)), ["1"]);
    assert_eq!(cut(at(next)), None);
}

#[test]
fn with_the_section_counter_the_numbers_cut_off_are_skipped() {
    let page = page_with("Counter: section.");
    assert_eq!(labels(at(page)), numbered(1..=13));
    assert_eq!(anchors(at(page)).last().map(|a| a.0.as_str()), Some("24"), "the numbers cut off keep theirs");
    let next = page + 1;
    assert!(texts(at(next)).iter().any(|t| t.0.starts_with("After the frame")));
    assert_eq!(labels(at(next)), ["25"], "the text after the frame goes on past the numbers cut off");
    assert_eq!(notes(at(next)), ["Numbered 25"]);
}

#[test]
fn with_the_document_counter_the_next_section_goes_on_past_the_numbers_cut_off() {
    let page = page_with("Counter: document.");
    assert_eq!(labels(at(page)), numbered(1..=13));
    let next = page_with("The next section, a text with a note");
    assert_eq!(next, page + 1);
    assert_eq!(labels(at(next)), ["25"]);
}

#[test]
fn a_box_that_says_paginate_no_has_the_notes_of_the_numbers_above_its_cut() {
    let page = page_with_note("Record 1");
    let bottom = cut(at(page)).expect("the box is cut");
    assert_eq!(anchors(at(page)).len(), 20);
    assert_eq!(labels(at(page)), numbered(1..=12));
    assert_eq!(notes(at(page)), entries("Record", 1..=12));
    assert!(anchors(at(page))[12].1 >= bottom);
    let next = page_with("After the box.");
    assert_eq!(next, page + 1);
    assert!(labels(at(next)).is_empty() && rule(at(next)).is_none(), "no note of the box goes on to the next page");
}

#[test]
fn a_grid_row_taller_than_a_page_has_the_notes_of_the_numbers_above_its_cut() {
    let page = page_with_note("Cell 1");
    assert!(cut(at(page)).is_some(), "the row is cut");
    // The long cell's 12 notes come first in reading order, then the short cell's, at 13.
    let mut expected = numbered(1..=6);
    expected.push("13".to_string());
    assert_eq!(labels(at(page)), expected);
    assert_eq!(notes(at(page)).last().map(String::as_str), Some("The note of the short cell."));
    let next = page_with("The second row of the grid");
    assert_eq!(next, page + 1);
    assert_eq!(labels(at(next)), ["14"], "the row after goes on, past the numbers cut off");
}

#[test]
fn in_a_cut_row_the_numbers_shown_run_on_past_the_tall_childs_numbers_cut_off() {
    let page = page_with_note("Column 1");
    let bottom = cut(at(page)).expect("the column beside the text is cut");
    // In reading order the column's numbers come first: four above the cut, six below it, then the text's.
    let all = anchors(at(page));
    assert!(all[..4].iter().all(|a| a.1 < bottom) && all[4..10].iter().all(|a| a.1 >= bottom), "{all:?}");
    assert_eq!(shown(at(page)), numbered(1..=11), "the numbers shown leave no gap for those cut off");
    let mut expected = entries("Column", 1..=4);
    expected.extend(entries("Beside", 1..=7));
    assert_eq!(notes(at(page)), expected);
    // The text goes on on the next page, numbered from 1 again.
    let next = page + 1;
    assert_eq!(labels(at(next)), numbered(1..=3));
    assert_eq!(notes(at(next)), entries("Beside", 8..=10));
}

#[test]
fn a_number_the_cut_runs_through_keeps_its_note() {
    let page = page_with("The line the cut runs through");
    let bottom = cut(at(page)).expect("the box is cut");
    let (number, top) = anchors(at(page)).into_iter().next().expect("the anchor");
    assert!(top < bottom && top + ANCHOR_SIZE > bottom, "the number from {top} is cut through at {bottom}");
    assert_eq!(number, "1");
    assert_eq!(labels(at(page)), ["1"]);
    assert_eq!(notes(at(page)), ["Its number is cut through, and keeps its note."]);
}

#[test]
fn a_page_whose_numbers_are_all_cut_off_has_no_rule_and_no_notes() {
    let page = page_with("Every number of this frame is below the cut.");
    assert_eq!(anchors(at(page)).len(), 4);
    assert!(labels(at(page)).is_empty() && notes(at(page)).is_empty());
    assert_eq!(rule(at(page)), None);
    let bottom = cut(at(page)).expect("the frame is cut");
    assert!((bottom - ABOVE_FOOTER).abs() < 0.01, "the frame reaches the bottom of the body: cut at {bottom}");
}

#[test]
fn a_page_with_a_header_is_cut_where_its_own_room_ends() {
    let first = page_with("The first page, below the header");
    let second = page_with("The second page, with the room");
    assert_eq!(second, first + 1);
    assert!(texts(at(first)).iter().any(|t| t.0 == "A header on the first page only"));
    assert!(!texts(at(second)).iter().any(|t| t.0 == "A header on the first page only"));
    // The header moves the first frame down, so fewer of its numbers are above the cut than on the second page.
    assert_eq!(labels(at(first)), numbered(1..=12));
    assert_eq!(notes(at(first)), entries("Night", 1..=12));
    assert_eq!(labels(at(second)), numbered(25..=37));
    assert_eq!(notes(at(second)), entries("Day", 1..=13));
    for page in [first, second] {
        let bottom = cut(at(page)).expect("each frame is cut");
        assert!(rule(at(page)).is_some_and(|rule| rule > bottom), "page {}", page + 1);
    }
}

// ── Around the fixture ────────────────────────────────────────────────────────

/// A section like the fixture's, its footer then `body`.
fn section(body: &str) -> String {
    format!(
        r#"<lpdf version="1"><document size="a4"><section><layout margin="40pt"><region pin="bottom" page="each"><text font-size="s">Footer</text></region>{body}</layout></section></document></lpdf>"#
    )
}

/// A frame 1000pt high of 24 texts of two lines, each with a note.
fn tall_frame() -> String {
    let texts: String = (1..=24)
        .map(|i| format!("<text>Entry {i}. The keepers of the northern coast kept a record of every evening they lit the lamp, the hour and the weather.<fn>Note {i}.</fn></text>"))
        .collect();
    format!(r#"<frame height="1000pt"><column gap="s">{texts}</column></frame>"#)
}

#[test]
fn a_node_placed_after_the_cut_one_leaves_the_notes_cut_off_out() {
    // The column's first part, an empty column, has no height, so it goes on the page after the frame.
    let tree = render(&section(&format!("{}<column><column/><text>After.</text></column>", tall_frame())));
    let page = &tree["pages"][0];
    assert!(cut(page).is_some(), "the frame is cut");
    let shown = shown(page);
    assert!(!shown.is_empty() && shown.len() < 24, "{shown:?}");
    assert_eq!(labels(page), shown);
}

#[test]
fn a_page_may_run_past_its_room_only_when_its_first_part_holds_what_the_cut_cannot_go_through() {
    let first_page = |body: &str| {
        let mut doc = parse(&section(body)).unwrap();
        let flow: Vec<Node> = doc.section_layouts().remove(0).children.into_iter().filter(|n| n.page_scope.is_none()).collect();
        fill_page(&mut Pending::new(&flow), None, 515.28, 750.0, 750.0, 0.0).1
    };
    let text = format!("<text>{}</text>", "word ".repeat(600));
    assert!(!first_page(&format!("<column>{text}{text}</column>")), "a column of texts ends at its last line that fits");
    assert!(!first_page(&format!("<row><text width=\"fill\">{0}</text><text width=\"fill\">{0}</text></row>", "word ".repeat(1200))));
    assert!(first_page(&format!(r#"<column><frame height="1000pt"/>{text}</column>"#)), "its first child is a frame taller than the page");
    assert!(first_page(&format!(r#"<frame height="1000pt"/>{text}"#)), "a frame taller than the page");
}
