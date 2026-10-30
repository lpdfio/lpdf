//! The notes of footnotes in content taller than a page, which the bottom of the page cuts (`clip_overflow`): a test
//! for each section of `showcase-footnote-clip.xml`, which says which notes are on which page and where the cut fell.

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
    TREE.get_or_init(|| {
        let mut doc = parse(FIXTURE).unwrap();
        let pages: Vec<_> = doc.section_layouts().iter().flat_map(layout_page).collect();
        serde_json::from_str(&crate::render::pages_to_json(&pages, Value::Null, Value::Null)).unwrap()
    })
}

fn page_count() -> usize {
    tree()["pages"].as_array().map_or(0, Vec::len)
}

/// Every node of page `page`, depth first.
fn nodes(page: usize) -> Vec<&'static Value> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        out.push(v);
        if let Some(children) = v["nodes"].as_array() {
            for child in children { walk(child, out); }
        }
    }
    let mut out = Vec::new();
    walk(&tree()["pages"][page], &mut out);
    out
}

/// The texts of page `page`, in the order they are drawn: content, x, y and size.
fn texts(page: usize) -> Vec<(String, f64, f64, f64)> {
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
    (0..page_count()).find(|&page| texts(page).iter().any(|t| t.0.contains(content)))
        .unwrap_or_else(|| panic!("no page shows {content:?}"))
}

/// The first page with a note that starts with `words`.
fn page_with_note(words: &str) -> usize {
    (0..page_count()).find(|&page| notes(page).iter().any(|note| note == words))
        .unwrap_or_else(|| panic!("no page has the note {words:?}"))
}

/// Where the cut of page `page` falls, if the page is cut.
fn cut(page: usize) -> Option<f64> {
    nodes(page).into_iter()
        .find(|v| v["type"] == "canvas-layer")
        .and_then(|layer| layer["clip"][3].as_f64())
}

/// The numbers of the anchors of page `page` in reading order, with the top of each.
fn anchors(page: usize) -> Vec<(String, f64)> {
    texts(page).into_iter().filter(|t| (t.3 - ANCHOR_SIZE).abs() < 0.01).map(|t| (t.0, t.2)).collect()
}

/// The numbers of the anchors of page `page` that the cut leaves in sight: all of them on a page that isn't cut.
fn shown(page: usize) -> Vec<String> {
    let bottom = cut(page).unwrap_or(f64::MAX);
    anchors(page).into_iter().filter(|(_, y)| *y < bottom).map(|(n, _)| n).collect()
}

/// The numbers of the notes at the foot of page `page`, which hang at the left margin.
fn labels(page: usize) -> Vec<String> {
    texts(page).into_iter().filter(|t| t.1 == 40.0 && t.3 < 6.0).map(|t| t.0).collect()
}

/// The words of the notes of page `page`, up to the colon the fixture's notes have.
fn notes(page: usize) -> Vec<String> {
    texts(page).into_iter()
        .filter(|t| (t.3 - NOTE_SIZE).abs() < 0.01)
        .map(|t| t.0.split(':').next().unwrap_or("").to_string())
        .collect()
}

/// The top of the rule above the notes of page `page`, if it has one.
fn rule(page: usize) -> Option<f64> {
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
        assert_eq!(labels(page), shown(page), "page {}", page + 1);
        if let (Some(bottom), Some(rule)) = (cut(page), rule(page)) {
            assert!(rule > bottom, "page {}: the notes start at {rule}, above the cut at {bottom}", page + 1);
        }
    }
}

#[test]
fn a_frame_taller_than_a_page_has_the_notes_of_the_numbers_shown_numbered_from_one() {
    let page = page_with("Counter: page.");
    let all = anchors(page);
    assert_eq!(all.len(), 24, "every entry of the frame is drawn, cut or not");
    let bottom = cut(page).expect("the frame is cut");
    assert_eq!(labels(page), numbered(1..=13));
    assert_eq!(notes(page), entries("Night", 1..=13));
    assert!(all[13].1 >= bottom, "the 14th number is below the cut at {bottom}: {:?}", all[13]);
    // All 24 notes would take 309.94pt and cut the frame at 481.15. The notes of the numbers cut off take no room,
    // and the frame shows more of itself.
    assert!(bottom > ABOVE_FOOTER - 309.94 + 100.0, "the cut is at {bottom}");

    let next = page_with("After the frame, a text with a note");
    assert_eq!(next, page + 1);
    assert_eq!(anchors(next).iter().map(|a| a.0.as_str()).collect::<Vec<_>>(), ["1"]);
    assert_eq!(labels(next), ["1"]);
    assert_eq!(cut(next), None);
}

#[test]
fn with_the_section_counter_the_numbers_cut_off_are_skipped() {
    let page = page_with("Counter: section.");
    assert_eq!(labels(page), numbered(1..=13));
    assert_eq!(anchors(page).last().map(|a| a.0.as_str()), Some("24"), "the numbers cut off keep theirs");
    let next = page + 1;
    assert!(texts(next).iter().any(|t| t.0.starts_with("After the frame")));
    assert_eq!(labels(next), ["25"], "the text after the frame goes on past the numbers cut off");
    assert_eq!(notes(next), ["Numbered 25"]);
}

#[test]
fn with_the_document_counter_the_next_section_goes_on_past_the_numbers_cut_off() {
    let page = page_with("Counter: document.");
    assert_eq!(labels(page), numbered(1..=13));
    let next = page_with("The next section, a text with a note");
    assert_eq!(next, page + 1);
    assert_eq!(labels(next), ["25"]);
}

#[test]
fn a_box_that_says_paginate_no_has_the_notes_of_the_numbers_above_its_cut() {
    let page = page_with_note("Record 1");
    let bottom = cut(page).expect("the box is cut");
    assert_eq!(anchors(page).len(), 20);
    assert_eq!(labels(page), numbered(1..=12));
    assert_eq!(notes(page), entries("Record", 1..=12));
    assert!(anchors(page)[12].1 >= bottom);
    let next = page_with("After the box.");
    assert_eq!(next, page + 1);
    assert!(labels(next).is_empty() && rule(next).is_none(), "no note of the box goes on to the next page");
}

#[test]
fn a_grid_row_taller_than_a_page_has_the_notes_of_the_numbers_above_its_cut() {
    let page = page_with_note("Cell 1");
    assert!(cut(page).is_some(), "the row is cut");
    // The long cell's 12 notes come first in reading order, then the short cell's, at 13.
    let mut expected = numbered(1..=6);
    expected.push("13".to_string());
    assert_eq!(labels(page), expected);
    assert_eq!(notes(page).last().map(String::as_str), Some("The note of the short cell."));
    let next = page_with("The second row of the grid");
    assert_eq!(next, page + 1);
    assert_eq!(labels(next), ["14"], "the row after goes on, past the numbers cut off");
}

#[test]
fn in_a_cut_row_the_numbers_shown_run_on_past_the_tall_childs_numbers_cut_off() {
    let page = page_with_note("Column 1");
    let bottom = cut(page).expect("the column beside the text is cut");
    // In reading order the column's numbers come first: four above the cut, six below it, then the text's.
    let all = anchors(page);
    assert!(all[..4].iter().all(|a| a.1 < bottom) && all[4..10].iter().all(|a| a.1 >= bottom), "{all:?}");
    assert_eq!(shown(page), numbered(1..=11), "the numbers shown leave no gap for those cut off");
    let mut expected = entries("Column", 1..=4);
    expected.extend(entries("Beside", 1..=7));
    assert_eq!(notes(page), expected);
    // The text goes on on the next page, numbered from 1 again.
    let next = page + 1;
    assert_eq!(labels(next), numbered(1..=3));
    assert_eq!(notes(next), entries("Beside", 8..=10));
}

#[test]
fn a_number_the_cut_runs_through_keeps_its_note() {
    let page = page_with("The line the cut runs through");
    let bottom = cut(page).expect("the box is cut");
    let (number, top) = anchors(page).into_iter().next().expect("the anchor");
    assert!(top < bottom && top + ANCHOR_SIZE > bottom, "the number from {top} is cut through at {bottom}");
    assert_eq!(number, "1");
    assert_eq!(labels(page), ["1"]);
    assert_eq!(notes(page), ["Its number is cut through, and keeps its note."]);
}

#[test]
fn a_page_whose_numbers_are_all_cut_off_has_no_rule_and_no_notes() {
    let page = page_with("Every number of this frame is below the cut.");
    assert_eq!(anchors(page).len(), 4);
    assert!(labels(page).is_empty() && notes(page).is_empty());
    assert_eq!(rule(page), None);
    let bottom = cut(page).expect("the frame is cut");
    assert!((bottom - ABOVE_FOOTER).abs() < 0.01, "the frame reaches the bottom of the body: cut at {bottom}");
}

#[test]
fn a_page_with_a_header_is_cut_where_its_own_room_ends() {
    let first = page_with("The first page, below the header");
    let second = page_with("The second page, with the room");
    assert_eq!(second, first + 1);
    assert!(texts(first).iter().any(|t| t.0 == "A header on the first page only"));
    assert!(!texts(second).iter().any(|t| t.0 == "A header on the first page only"));
    // The header moves the first frame down, so fewer of its numbers are above the cut than on the second page.
    assert_eq!(labels(first), numbered(1..=12));
    assert_eq!(notes(first), entries("Night", 1..=12));
    assert_eq!(labels(second), numbered(25..=37));
    assert_eq!(notes(second), entries("Day", 1..=13));
    for page in [first, second] {
        let bottom = cut(page).expect("each frame is cut");
        assert!(rule(page).is_some_and(|rule| rule > bottom), "page {}", page + 1);
    }
}
