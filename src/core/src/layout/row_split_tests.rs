//! A row cut through a line where the page ends (`split_row`), and the bottom of a page cutting what is taller than
//! the page (`clip_overflow`): a test for each case the fixtures of the row split cover.

use super::*;
use crate::parse::parse;
use serde_json::Value;

/// The room of an A4 page with a 28pt margin, and where it ends.
const ROOM: f64 = 785.89;
const BODY_BOTTOM: f64 = 813.89;
/// The height of a line of 11pt text.
const LINE: f64 = 13.2;

fn render(body: &str) -> Value {
    let xml = format!(
        r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
    );
    let mut doc = parse(&xml).unwrap();
    let pages: Vec<_> = doc.section_layouts().iter().flat_map(layout_page).collect();
    serde_json::from_str(&crate::render::pages_to_json(&pages, Value::Null, Value::Null)).unwrap()
}

/// A frame that leaves `left` of the room of the first page.
fn filler(left: f64) -> String {
    format!(r#"<frame height="{:.2}pt" />"#, ROOM - left)
}

/// `n` words, numbered after `prefix`: `a0001 a0002 …`.
fn words(prefix: &str, n: usize) -> String {
    (1..=n).map(|i| format!("{prefix}{i:04}")).collect::<Vec<_>>().join(" ")
}

/// `n` words each too wide to share a line in a row beside a marker.
fn wide_words(n: usize) -> String {
    (1..=n).map(|i| format!("L{i:02}{}", "W".repeat(36))).collect::<Vec<_>>().join(" ")
}

/// A row of a marker and a text that fills the rest of the line.
fn item(attrs: &str, text: &str) -> String {
    format!(r#"<row {attrs}><text width="20pt">1.</text><text width="fill">{text}</text></row>"#)
}

struct Line {
    content: String,
    x:       f64,
    y:       f64,
    size:    f64,
}

/// The lines of text of page `page`, in the order they are drawn.
fn lines(tree: &Value, page: usize) -> Vec<Line> {
    fn walk(v: &Value, out: &mut Vec<Line>) {
        if v["type"] == "text" {
            out.push(Line {
                content: v["content"].as_str().unwrap_or("").to_string(),
                x:       v["x"].as_f64().unwrap_or(0.0),
                y:       v["y"].as_f64().unwrap_or(0.0),
                size:    v["size"].as_f64().unwrap_or(0.0),
            });
        }
        if let Some(nodes) = v["nodes"].as_array() {
            for node in nodes { walk(node, out); }
        }
    }
    let mut out = Vec::new();
    walk(&tree["pages"][page], &mut out);
    out
}

/// The nodes of page `page` whose type is `kind`.
fn nodes_of<'a>(tree: &'a Value, page: usize, kind: &str) -> Vec<&'a Value> {
    fn walk<'a>(v: &'a Value, kind: &str, out: &mut Vec<&'a Value>) {
        if v["type"] == kind { out.push(v); }
        if let Some(nodes) = v["nodes"].as_array() {
            for node in nodes { walk(node, kind, out); }
        }
    }
    let mut out = Vec::new();
    walk(&tree["pages"][page], kind, &mut out);
    out
}

fn page_count(tree: &Value) -> usize {
    tree["pages"].as_array().map_or(0, Vec::len)
}

/// The words of page `page` that start with `prefix`.
fn words_on(tree: &Value, page: usize, prefix: &str) -> Vec<String> {
    lines(tree, page).iter()
        .flat_map(|line| line.content.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|word| word.starts_with(prefix))
        .collect()
}

/// The words that start with `prefix`, page after page.
fn all_words(tree: &Value, prefix: &str) -> Vec<String> {
    (0..page_count(tree)).flat_map(|page| words_on(tree, page, prefix)).collect()
}

/// The lines of page `page` whose words start with `prefix`.
fn lines_of(tree: &Value, page: usize, prefix: &str) -> Vec<Line> {
    lines(tree, page).into_iter().filter(|line| line.content.starts_with(prefix)).collect()
}

/// The bottom of the lowest line of text of page `page`.
fn lowest(tree: &Value, page: usize) -> f64 {
    lines(tree, page).iter().map(|line| line.y + line.size * 1.2).fold(0.0, f64::max)
}

/// Where the clip that cuts page `page` ends, if one does.
fn clip(tree: &Value, page: usize) -> Option<f64> {
    nodes_of(tree, page, "canvas-layer").iter()
        .find_map(|layer| layer["clip"].as_array().map(|c| c[1].as_f64().unwrap_or(0.0) + c[3].as_f64().unwrap_or(0.0)))
}

/// `clip` is where it ends, to the hundredth of a point the tree is written in.
fn assert_clip(tree: &Value, page: usize, bottom: f64) {
    let at = clip(tree, page).unwrap_or_else(|| panic!("page {page} is not clipped"));
    assert!((at - bottom).abs() < 0.01, "page {page} is clipped at {at}, not {bottom}");
}

fn shows(tree: &Value, page: usize, content: &str) -> bool {
    lines(tree, page).iter().any(|line| line.content.contains(content))
}

/// The words of every page that start with `prefix`, page by page.
fn words_by_page(tree: &Value, prefix: &str) -> Vec<Vec<String>> {
    (0..page_count(tree)).map(|page| words_on(tree, page, prefix)).collect()
}

fn expected(prefix: &str, n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("{prefix}{i:04}")).collect()
}

/// The page holds no text below its body.
fn assert_inside(tree: &Value, page: usize) {
    let bottom = lowest(tree, page);
    assert!(bottom <= BODY_BOTTOM + 0.01, "page {page} has text down to {bottom}, below {BODY_BOTTOM}");
}

// ── Text children ─────────────────────────────────────────────────────────────

#[test]
fn a_row_of_two_texts_goes_on_to_the_next_page_where_the_page_ends() {
    let body = format!(
        r#"{}<row gap="10pt"><text width="fill">{}</text><text width="fill">{}</text></row>"#,
        filler(300.0), words("a", 300), words("b", 300),
    );
    let tree = render(&body);
    assert_eq!(all_words(&tree, "a"), expected("a", 300));
    assert_eq!(all_words(&tree, "b"), expected("b", 300));
    for prefix in ["a", "b"] {
        assert!(!words_on(&tree, 0, prefix).is_empty(), "{prefix} starts on the first page");
        assert!(!words_on(&tree, 1, prefix).is_empty(), "{prefix} goes on on the second");
    }
    assert_inside(&tree, 0);
    assert_eq!(clip(&tree, 0), None);
}

#[test]
fn texts_of_different_sizes_side_by_side_each_end_at_their_own_last_line() {
    let body = format!(
        r#"{}<row gap="10pt"><text width="fill">{}</text><text width="fill" font-size="14pt">{}</text></row>"#,
        filler(300.5), words("a", 300), words("b", 300),
    );
    let tree = render(&body);
    let bottom = |prefix: &str| lines_of(&tree, 0, prefix).iter().map(|l| l.y + l.size * 1.2).fold(0.0, f64::max);
    // Each text fills the room to within one of its own lines: neither is pushed up to the other's lines.
    assert!(bottom("a") > BODY_BOTTOM - LINE - 0.01 && bottom("a") <= BODY_BOTTOM + 0.01, "a ends at {}", bottom("a"));
    assert!(bottom("b") > BODY_BOTTOM - 16.8 - 0.01 && bottom("b") <= BODY_BOTTOM + 0.01, "b ends at {}", bottom("b"));
    assert_eq!(all_words(&tree, "b"), expected("b", 300));
}

#[test]
fn a_text_in_a_cut_row_keeps_two_lines_for_the_next_page() {
    // Room for 5 of 6 lines would leave 1 on the next page; widows keep 2 there.
    let body = format!("{}{}", filler(LINE * 5.0 + LINE / 2.0), item("", &wide_words(6)));
    let tree = render(&body);
    let per_page: Vec<usize> = (0..page_count(&tree)).map(|p| lines_of(&tree, p, "L").len()).collect();
    assert_eq!(per_page, vec![4, 2]);
}

// ── A marker and a fill text ──────────────────────────────────────────────────

#[test]
fn a_long_item_splits_mid_text_with_its_marker_on_the_first_page_only() {
    let tree = render(&format!("{}{}", filler(300.0), item("", &words("w", 400))));
    assert_eq!(page_count(&tree), 2);
    assert!(shows(&tree, 0, "1."));
    assert!(!shows(&tree, 1, "1."));
    assert_eq!(all_words(&tree, "w"), expected("w", 400));
    assert!(!words_on(&tree, 0, "w").is_empty());
    assert_inside(&tree, 0);
}

#[test]
fn the_text_of_a_long_item_is_as_wide_on_the_next_page() {
    let tree = render(&format!("{}{}", filler(300.0), item("", &words("w", 400))));
    let first = lines_of(&tree, 0, "w");
    let next = lines_of(&tree, 1, "w");
    assert!((first[0].x - 48.0).abs() < 0.01, "x {}", first[0].x);
    assert!(next.iter().all(|line| (line.x - first[0].x).abs() < 0.01), "the text starts where it did");
    // The lines are as long, so the words wrap as they would have on one page.
    let per_line = |l: &Line| l.content.split_whitespace().count();
    assert_eq!(per_line(&next[0]), per_line(&first[0]));
}

#[test]
fn an_item_that_fits_stays_whole_on_its_page() {
    let tree = render(&format!("{}{}", filler(300.0), item("", &words("w", 40))));
    assert_eq!(page_count(&tree), 1);
    assert_eq!(words_on(&tree, 0, "w"), expected("w", 40));
}

#[test]
fn an_item_whose_text_orphans_keep_back_moves_whole_with_its_marker() {
    // Room for one line: the marker would fit, but the text keeps two lines together.
    let tree = render(&format!("{}{}", filler(LINE * 1.5), item("", &wide_words(6))));
    assert!(!shows(&tree, 0, "1."), "the marker is not left alone");
    assert!(shows(&tree, 1, "1."));
    assert_eq!(lines_of(&tree, 1, "L").len(), 6);
}

// ── Widths ────────────────────────────────────────────────────────────────────

#[test]
fn a_child_with_no_width_keeps_its_width_on_the_next_page() {
    // The heading makes the column as wide as it is; what is left of the column has no heading.
    let labels: String = (1..=80).map(|i| format!("<text>k{i:02}</text>")).collect();
    let body = format!(
        r#"{}<row gap="10pt"><column><text>A fairly long heading</text>{labels}</column><text width="fill">{}</text></row>"#,
        filler(300.0), words("w", 900),
    );
    let tree = render(&body);
    let x0 = lines_of(&tree, 0, "w")[0].x;
    let x1 = lines_of(&tree, 1, "w")[0].x;
    assert!((x0 - x1).abs() < 0.01, "the text starts at {x0} and then at {x1}");
}

#[test]
fn a_line_justified_between_keeps_its_places_on_the_next_page() {
    let body = format!(
        r#"{}<row justify="between"><column width="200pt"><text>{}</text></column><column width="200pt"><text>{}</text></column></row>"#,
        filler(300.0), words("a", 300), words("b", 300),
    );
    let tree = render(&body);
    let x0 = lines_of(&tree, 0, "b")[0].x;
    let x1 = lines_of(&tree, 1, "b")[0].x;
    assert!((x0 - (28.0 + 539.28 - 200.0)).abs() < 0.01, "the second column is at the right edge: {x0}");
    assert!((x0 - x1).abs() < 0.01, "and stays there: {x1}");
}

// ── align ─────────────────────────────────────────────────────────────────────

/// The marker's y on the first page, and the top and bottom of the text beside it there.
fn marker_and_part(tree: &Value) -> (f64, f64, f64) {
    let marker = lines(tree, 0).into_iter().find(|l| l.content == "1.").expect("the marker is on the first page");
    let text = lines_of(tree, 0, "w");
    let top = text.iter().map(|l| l.y).fold(f64::MAX, f64::min);
    let bottom = text.iter().map(|l| l.y + LINE).fold(0.0, f64::max);
    (marker.y, top, bottom)
}

#[test]
fn a_centred_row_centres_its_marker_in_the_part_on_the_first_page() {
    let tree = render(&format!("{}{}", filler(300.0), item(r#"align="center""#, &words("w", 400))));
    let (marker, top, bottom) = marker_and_part(&tree);
    assert!((marker - (top + bottom - LINE) / 2.0).abs() < 0.6, "marker at {marker}, part {top}..{bottom}");
}

#[test]
fn an_end_aligned_row_puts_its_marker_at_the_bottom_of_the_part_on_the_first_page() {
    let tree = render(&format!("{}{}", filler(300.0), item(r#"align="end""#, &words("w", 400))));
    let (marker, _, bottom) = marker_and_part(&tree);
    assert!((marker - (bottom - LINE)).abs() < 0.6, "marker at {marker}, part ends at {bottom}");
}

// ── A box that can't be cut, in the row ───────────────────────────────────────

#[test]
fn the_cut_moves_above_a_box_beside_text_and_leaves_blank_space() {
    // The frame starts under two markers and runs past the page; the text stops at the frame's top.
    let body = format!(
        r##"{}<row><column width="40pt"><text>1.</text><text>2.</text><frame height="100pt" background="#cccccc" /></column><text width="fill">{}</text></row>"##,
        filler(60.0), words("w", 400),
    );
    let tree = render(&body);
    let row_top = BODY_BOTTOM - 60.0;
    assert_eq!(lines_of(&tree, 0, "w").len(), 2, "two lines beside the markers");
    assert!(lowest(&tree, 0) <= row_top + 2.0 * LINE + 0.01);
    let frames: Vec<_> = nodes_of(&tree, 1, "box").into_iter().filter(|b| b["fill"] == "#cccccc").collect();
    assert_eq!(frames.len(), 1, "the frame is on the next page");
    assert_eq!(all_words(&tree, "w"), expected("w", 400));
}

#[test]
fn a_row_that_starts_with_a_box_taller_than_the_room_left_moves_whole() {
    let body = format!(
        r#"{}<row><frame width="100pt" height="200pt" /><text width="fill">{}</text></row>"#,
        filler(150.0), words("w", 100),
    );
    let tree = render(&body);
    assert!(words_on(&tree, 0, "w").is_empty());
    assert_eq!(words_on(&tree, 1, "w"), expected("w", 100));
}

#[test]
fn a_box_taller_than_a_page_beside_a_long_text_is_cut_and_the_text_goes_on() {
    let body = format!(r#"<row><frame width="100pt" height="1000pt" /><text width="fill">{}</text></row>"#, words("w", 800));
    let tree = render(&body);
    assert_clip(&tree, 0, BODY_BOTTOM);
    assert_inside(&tree, 0);
    assert!(!words_on(&tree, 1, "w").is_empty());
    assert_eq!(all_words(&tree, "w"), expected("w", 800));
    assert!(!shows(&tree, 1, "[lpdf]"), "no note without debug");
}

#[test]
fn a_row_with_nothing_left_after_its_cut_leaves_no_empty_part() {
    // The frame is clipped and the text beside it ends on the first page: no empty row is left for a second page.
    let tree = render(r#"<row><frame width="100pt" height="1000pt" /><text width="fill">Beside.</text></row>"#);
    assert_eq!(page_count(&tree), 1);
    let tree = render(r#"<row><frame width="100pt" height="1000pt" /><text width="fill">Beside.</text></row><text>After.</text>"#);
    let after = lines(&tree, 1).into_iter().find(|l| l.content == "After.").expect("After. starts the next page");
    assert!((after.y - 28.0).abs() < 0.01, "at its top: {}", after.y);
}

#[test]
fn a_debug_row_cut_at_the_bottom_of_the_page_says_so_on_the_next() {
    let body = format!(
        r#"<row debug="true"><frame width="100pt" height="1000pt" /><text width="fill">{}</text></row>"#,
        words("w", 800),
    );
    let tree = render(&body);
    let note = "[lpdf] cut at the bottom of the page: content taller than the page";
    assert!(!shows(&tree, 0, note));
    assert!(shows(&tree, 1, note));
}

// ── Children of unequal height ────────────────────────────────────────────────

#[test]
fn a_short_child_that_ends_above_the_cut_leaves_its_place_empty_on_the_next_page() {
    let body = format!(
        r#"{}<row gap="10pt"><column width="100pt"><text>s1</text><text>s2</text></column><text width="fill">{}</text></row>"#,
        filler(300.0), words("w", 400),
    );
    let tree = render(&body);
    assert!(shows(&tree, 0, "s1") && shows(&tree, 0, "s2"));
    assert!(!shows(&tree, 1, "s1") && !shows(&tree, 1, "s2"));
    assert!((lines_of(&tree, 1, "w")[0].x - (28.0 + 110.0)).abs() < 0.01);
}

// ── Nesting ───────────────────────────────────────────────────────────────────

#[test]
fn columns_in_a_row_both_go_on_to_the_next_page() {
    let body = format!(
        r#"{}<row gap="10pt"><column width="fill"><text>{}</text></column><column width="fill"><text>{}</text></column></row>"#,
        filler(300.0), words("a", 300), words("b", 300),
    );
    let tree = render(&body);
    assert_eq!(all_words(&tree, "a"), expected("a", 300));
    assert_eq!(all_words(&tree, "b"), expected("b", 300));
    assert!(!words_on(&tree, 0, "a").is_empty() && !words_on(&tree, 1, "b").is_empty());
    assert_inside(&tree, 0);
}

#[test]
fn a_row_in_a_row_is_cut_with_it() {
    let body = format!(r#"{}<row><row width="fill">{}</row></row>"#, filler(300.0), item("", &words("w", 400)));
    let tree = render(&body);
    assert!(shows(&tree, 0, "1.") && !shows(&tree, 1, "1."));
    assert_eq!(all_words(&tree, "w"), expected("w", 400));
    assert!(!words_on(&tree, 0, "w").is_empty());
    assert_inside(&tree, 0);
}

#[test]
fn a_row_in_a_column_in_a_row_is_cut_with_them() {
    let body = format!(
        r#"{}<row><column width="fill"><text>Heading</text>{}</column></row>"#,
        filler(300.0), item("", &words("w", 400)),
    );
    let tree = render(&body);
    assert!(shows(&tree, 0, "Heading") && shows(&tree, 0, "1."));
    assert_eq!(all_words(&tree, "w"), expected("w", 400));
    assert!(!words_on(&tree, 0, "w").is_empty());
    assert_inside(&tree, 0);
}

// ── Size at the edge ──────────────────────────────────────────────────────────

#[test]
fn a_row_that_fits_exactly_stays_on_its_page() {
    let tree = render(&format!("{}{}", filler(LINE * 5.0), item("", &wide_words(5))));
    assert_eq!(page_count(&tree), 1);
}

#[test]
fn a_row_half_a_point_over_stays_on_its_page() {
    let tree = render(&format!("{}{}", filler(LINE * 5.0 - 0.5), item("", &wide_words(5))));
    assert_eq!(page_count(&tree), 1);
}

#[test]
fn a_row_two_points_over_is_cut() {
    let tree = render(&format!("{}{}", filler(LINE * 5.0 - 2.0), item("", &wide_words(5))));
    let per_page: Vec<usize> = (0..page_count(&tree)).map(|p| lines_of(&tree, p, "L").len()).collect();
    assert_eq!(per_page, vec![3, 2]);
}

#[test]
fn a_row_at_the_top_of_a_page_and_taller_than_it_is_cut() {
    let tree = render(&item("", &words("w", 1200)));
    assert_eq!(page_count(&tree), 2);
    assert!(!words_on(&tree, 0, "w").is_empty() && !words_on(&tree, 1, "w").is_empty());
    assert_eq!(clip(&tree, 0), None);
    assert_inside(&tree, 0);
}

#[test]
fn a_row_taller_than_two_pages_runs_over_three() {
    let tree = render(&item("", &words("w", 2000)));
    assert_eq!(page_count(&tree), 3);
    assert_eq!(all_words(&tree, "w"), expected("w", 2000));
    for page in 0..3 {
        assert!(!words_on(&tree, page, "w").is_empty());
        assert_inside(&tree, page);
        assert_eq!(clip(&tree, page), None);
    }
    assert!(shows(&tree, 0, "1.") && !shows(&tree, 1, "1.") && !shows(&tree, 2, "1."));
}

// ── The row's own box ─────────────────────────────────────────────────────────

#[test]
fn a_cut_row_has_its_padding_background_and_border_on_each_page() {
    let body = format!(
        r##"{}<row padding="10pt" gap="10pt" background="#eeeeee" border="1pt #333333"><text width="20pt">1.</text><text width="fill">{}</text></row>"##,
        filler(300.0), words("w", 400),
    );
    let tree = render(&body);
    for page in 0..2 {
        let boxes: Vec<_> = nodes_of(&tree, page, "box").into_iter().filter(|b| b["fill"] == "#eeeeee").collect();
        assert_eq!(boxes.len(), 1, "page {page} has the row's box");
        let b = boxes[0];
        let (y, h) = (b["y"].as_f64().unwrap(), b["height"].as_f64().unwrap());
        assert!(y + h <= BODY_BOTTOM + 0.5, "page {page}: the box ends at {}", y + h);
        let text = lines_of(&tree, page, "w");
        assert!((text[0].y - (y + 10.0)).abs() < 0.01, "page {page}: the text starts under the padding");
        assert!((text[0].x - (28.0 + 10.0 + 20.0 + 10.0)).abs() < 0.01, "page {page}: and beside the marker's place");
    }
}

// ── Flags ─────────────────────────────────────────────────────────────────────

#[test]
fn a_row_that_says_paginate_no_moves_whole() {
    let tree = render(&format!("{}{}", filler(300.0), item(r#"paginate="no""#, &words("w", 400))));
    assert!(words_on(&tree, 0, "w").is_empty());
    assert_eq!(words_on(&tree, 1, "w"), expected("w", 400));
}

#[test]
fn a_row_with_break_before_starts_a_page_and_is_cut_there() {
    let body = format!(r#"<text>Before.</text>{}"#, item(r#"paginate="break-before""#, &words("w", 1200)));
    let tree = render(&body);
    assert!(words_on(&tree, 0, "w").is_empty());
    assert!(!words_on(&tree, 1, "w").is_empty() && !words_on(&tree, 2, "w").is_empty());
    assert_eq!(all_words(&tree, "w"), expected("w", 1200));
}

#[test]
fn a_row_with_break_after_ends_the_page_after_its_last_part() {
    let body = format!(r#"{}{}<text>After.</text>"#, filler(300.0), item(r#"paginate="break-after""#, &words("w", 400)));
    let tree = render(&body);
    assert!(!words_on(&tree, 1, "w").is_empty(), "the row ends on the second page");
    assert!(!shows(&tree, 1, "After."));
    assert!(shows(&tree, 2, "After."));
}

#[test]
fn a_row_with_keep_next_is_cut_as_one_without_it() {
    let with = render(&format!("{}{}<text>After.</text>", filler(300.0), item(r#"paginate="keep-next""#, &words("w", 400))));
    let without = render(&format!("{}{}<text>After.</text>", filler(300.0), item("", &words("w", 400))));
    assert_eq!(words_by_page(&with, "w"), words_by_page(&without, "w"));
}

#[test]
fn a_child_that_says_paginate_no_is_not_cut() {
    let body = format!(
        r#"{}<row gap="10pt"><column paginate="no" width="fill"><text>{}</text></column><text width="fill">{}</text></row>"#,
        filler(300.0), words("a", 300), words("b", 300),
    );
    let tree = render(&body);
    assert!(words_on(&tree, 0, "a").is_empty() && words_on(&tree, 0, "b").is_empty(), "the row moves whole");
    assert_eq!(words_on(&tree, 1, "a"), expected("a", 300));
}

#[test]
fn page_breaks_on_a_child_change_nothing() {
    let plain = render(&format!("{}{}", filler(300.0), item("", &words("w", 400))));
    for flag in ["break-before", "break-after", "keep-next"] {
        let body = format!(
            r#"{}<row><text width="20pt" paginate="{flag}">1.</text><text width="fill" paginate="{flag}">{}</text></row>"#,
            filler(300.0), words("w", 400),
        );
        assert_eq!(words_by_page(&render(&body), "w"), words_by_page(&plain, "w"), "{flag}");
    }
}

// ── Page budgets ──────────────────────────────────────────────────────────────

#[test]
fn a_row_is_cut_at_the_room_of_each_page() {
    let body = format!(
        r#"<region pin="top" page="first"><frame height="100pt" /></region>{}"#,
        item("", &words("w", 1200)),
    );
    let tree = render(&body);
    assert_eq!(all_words(&tree, "w"), expected("w", 1200));
    assert!(lines_of(&tree, 0, "w")[0].y >= 128.0 - 0.01, "the first page starts under the header");
    assert!((lines_of(&tree, 1, "w")[0].y - 28.0).abs() < 0.01, "the next has no header");
    assert_inside(&tree, 0);
    assert_inside(&tree, 1);
}

// ── Footnotes ─────────────────────────────────────────────────────────────────

#[test]
fn the_note_of_a_number_in_a_cut_text_is_on_the_page_of_the_number() {
    let text = format!("{} n0001<fn>The note of the item.</fn> {}", words("w", 340), words("v", 60));
    let tree = render(&format!("{}{}", filler(300.0), item("", &text)));
    assert!(words_on(&tree, 0, "n0001").is_empty(), "the number is on the second page");
    assert!(!shows(&tree, 0, "The note of the item."));
    assert!(shows(&tree, 1, "The note of the item."));
}

// ── Cut at the bottom of the page ─────────────────────────────────────────────

const FOOTER: &str = r#"<region pin="bottom" page="each"><text>FOOT</text></region>"#;

#[test]
fn a_frame_taller_than_a_page_is_cut_above_the_footer() {
    let tree = render(&format!(r##"{FOOTER}<frame height="1000pt" background="#dddddd" /><text>After.</text>"##));
    assert_clip(&tree, 0, BODY_BOTTOM - LINE);
    assert!(shows(&tree, 0, "FOOT"));
    assert!(shows(&tree, 1, "After."));
}

#[test]
fn a_box_that_says_paginate_no_and_is_taller_than_a_page_is_cut() {
    let tree = render(&format!(r#"{FOOTER}<column paginate="no"><text>{}</text></column><text>After.</text>"#, words("w", 1500)));
    assert_clip(&tree, 0, BODY_BOTTOM - LINE);
    assert!(shows(&tree, 1, "After."));
}

#[test]
fn a_grid_row_taller_than_a_page_is_cut_and_the_rows_after_it_go_on() {
    let body = format!(r#"<grid cols="2"><text>{}</text><text>short</text><text>R2</text></grid>"#, words("w", 1500));
    let tree = render(&body);
    assert_clip(&tree, 0, BODY_BOTTOM);
    assert!(!shows(&tree, 0, "R2"));
    assert!(shows(&tree, 1, "R2"));
}

#[test]
fn spanned_rows_taller_than_a_page_are_cut_and_the_rows_after_them_go_on() {
    let body = format!(
        r#"<table cols="1fr 1fr"><tr><td rowspan="2"><text>{}</text></td><td><text>x</text></td></tr><tr><td><text>y</text></td></tr><tr><td><text>AFTER</text></td><td><text>z</text></td></tr></table>"#,
        words("w", 2500),
    );
    let tree = render(&body);
    assert_clip(&tree, 0, BODY_BOTTOM);
    assert!(shows(&tree, 1, "AFTER"));
}

#[test]
fn a_column_goes_on_after_a_first_child_taller_than_a_page() {
    let tree = render(r#"<column><frame height="1000pt" /><text>Inside.</text></column>"#);
    assert_clip(&tree, 0, BODY_BOTTOM);
    assert!(!shows(&tree, 0, "Inside."));
    assert!(shows(&tree, 1, "Inside."));
}

#[test]
fn a_link_is_cut_at_the_clip_and_a_field_below_it_dropped() {
    let body = format!(
        r#"<column paginate="no"><field type="text" name="above" height="24pt" /><frame height="680pt" /><link href="https://lpdf.io"><text>{}</text></link><field type="text" name="below" height="24pt" /></column>"#,
        words("w", 400),
    );
    let tree = render(&body);
    let links = nodes_of(&tree, 0, "link");
    assert_eq!(links.len(), 1);
    let (y, h) = (links[0]["y"].as_f64().unwrap(), links[0]["height"].as_f64().unwrap());
    assert!(y < BODY_BOTTOM && (y + h - BODY_BOTTOM).abs() < 0.01, "the link ends at the clip: {}", y + h);
    let fields: Vec<_> = nodes_of(&tree, 0, "field").iter().map(|f| f["y"].as_f64().unwrap()).collect();
    assert_eq!(fields, vec![28.0], "only the field above the clip is left");

    let xml = format!(
        r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
    );
    let pdf = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).unwrap();
    let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"(above)"), "the field above the clip is in the form");
    assert!(!has(b"(below)"));
}

#[test]
fn a_bookmark_below_the_clip_marks_the_clip() {
    let mut nodes = vec![RenderNode::Box(RenderBox {
        x: 0.0, y: 700.0, width: 100.0, height: 400.0,
        fill: None, border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None, debug_self: false, tag: None,
        ocg: None, children: vec![RenderNode::Anchor(RenderAnchor { id: "deep".into(), label: "Deep".into(), level: 1, y: 950.0 })],
    })];
    fit_to_clip(&mut nodes, 800.0);
    let RenderNode::Box(b) = &nodes[0] else { panic!("a box") };
    let RenderNode::Anchor(a) = &b.children[0] else { panic!("an anchor") };
    assert_eq!(a.y, 800.0);
}

#[test]
fn a_page_whose_flow_fits_is_not_clipped() {
    let tree = render(&format!("{}<text>Last line.</text>", filler(LINE)));
    assert_eq!(page_count(&tree), 1);
    assert_eq!(clip(&tree, 0), None);
}
