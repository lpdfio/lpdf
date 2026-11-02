//! Centered and right-aligned text with spans in it. A line drawn as one text is aligned by the writer; any other
//! line is placed from where its measured width says it starts, so its spans follow the words before them and stay
//! inside the box, with their lines, tints and link areas under their own words.

use super::*;
use crate::parse::parse;
use serde_json::Value;

/// The edges and the middle of the body of an A4 page with a 50pt margin.
const LEFT: f64 = 50.0;
const RIGHT: f64 = 595.28 - 50.0;
const MIDDLE: f64 = 595.28 / 2.0;

/// How far a place may be from where it is expected: the layout tree rounds every number to hundredths.
const NEAR: f64 = 0.05;

/// A section of an A4 page holding `content`, laid out as the tree the engine hands out.
fn section(content: &str) -> Value {
    let xml = format!(r#"<lpdf version="1"><document size="a4"><section>{content}</section></document></lpdf>"#);
    let mut doc = parse(&xml).unwrap();
    let pages: Vec<_> = doc.section_layouts().iter().flat_map(layout_page).collect();
    serde_json::from_str(&crate::render::pages_to_json(&pages, Value::Null, Value::Null)).unwrap()
}

/// `body` in the layout of an A4 page with a 50pt margin, laid out.
fn page(body: &str) -> Value {
    section(&format!(r#"<layout margin="50pt">{body}</layout>"#))
}

/// The box of the first text of the page.
fn text_box(tree: &Value) -> &Value {
    &tree["pages"][0]["nodes"][0]
}

/// `node` and every node under it, depth first, in the order they are drawn.
fn nodes(node: &Value) -> Vec<&Value> {
    fn walk<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
        out.push(v);
        for child in v["nodes"].as_array().into_iter().flatten() {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(node, &mut out);
    out
}

fn num(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or_else(|| panic!("no {key} in {v}"))
}

/// A text as the writer draws it: its words, where they start and end, and the top of its line.
#[derive(Debug)]
struct Drawn {
    content: String,
    left:    f64,
    right:   f64,
    y:       f64,
}

/// Every text under `node`, in the order they are drawn, placed as the writer places them (`RenderNode::Text` in
/// `pdf.rs`): a right-aligned text ends at its `x`, a centered one is around it, any other starts at it.
fn drawn(node: &Value) -> Vec<Drawn> {
    nodes(node).into_iter()
        .filter(|v| v["type"] == "text")
        .map(|v| {
            let content = v["content"].as_str().unwrap().to_string();
            let width = f64::from(text_width(v["font"].as_str().unwrap(), &content, num(v, "size") as f32));
            let x = num(v, "x");
            let left = match v["text_align"].as_str().unwrap() {
                "right" => x - width,
                "center" => x - width / 2.0,
                _ => x,
            };
            Drawn { content, left, right: left + width, y: num(v, "y") }
        })
        .collect()
}

/// The words of `texts`, in order.
fn words(texts: &[Drawn]) -> Vec<&str> {
    texts.iter().map(|t| t.content.as_str()).collect()
}

/// Each text of a line starts at or after the end of the one before it.
fn assert_in_order(texts: &[Drawn]) {
    for pair in texts.windows(2) {
        assert!(pair[1].left >= pair[0].right - NEAR, "{:?} is drawn over {:?}", pair[1], pair[0]);
    }
}

/// The text with the words `content`.
fn text<'a>(texts: &'a [Drawn], content: &str) -> &'a Drawn {
    texts.iter().find(|t| t.content == content).unwrap_or_else(|| panic!("no text {content:?} in {texts:?}"))
}

/// The lines of a text: its texts grouped by the top of the line they are on.
fn lines(texts: Vec<Drawn>) -> Vec<Vec<Drawn>> {
    let mut out: Vec<Vec<Drawn>> = Vec::new();
    for t in texts {
        match out.last_mut() {
            Some(line) if (line[0].y - t.y).abs() < 0.01 => line.push(t),
            _ => out.push(vec![t]),
        }
    }
    out
}

fn assert_near(actual: f64, expected: f64, what: &str) {
    assert!((actual - expected).abs() < NEAR, "{what}: {actual}, expected {expected}");
}

const NOTICE: &str = r##"Notice: <span color="#ff0000">clause 4, page 3</span>"##;

#[test]
fn a_color_span_in_a_right_aligned_text_follows_the_words_before_it_and_the_line_ends_at_the_right_edge() {
    let tree = page(&format!(r#"<text align="right">{NOTICE}</text>"#));
    let texts = drawn(text_box(&tree));
    assert_eq!(words(&texts), ["Notice:", "clause 4, page 3"]);
    assert_in_order(&texts);
    let space = f64::from(text_width("Helvetica", " ", num(&nodes(text_box(&tree))[1], "size") as f32));
    assert_near(texts[1].left - texts[0].right, space, "the gap between the words and the span is a space");
    assert_near(texts[1].right, RIGHT, "the line ends at the right edge");
}

#[test]
fn a_color_span_in_a_centered_text_follows_the_words_before_it_and_the_line_is_centered_on_the_box() {
    let tree = page(&format!(r#"<text align="center">{NOTICE}</text>"#));
    let texts = drawn(text_box(&tree));
    assert_eq!(words(&texts), ["Notice:", "clause 4, page 3"]);
    assert_in_order(&texts);
    assert_near((texts[0].left + texts[1].right) / 2.0, MIDDLE, "the middle of the line");
}

/// A span of each kind that is drawn with something more than its words, with the words after it.
const MARKED: [(&str, &str); 5] = [
    ("link", r#"See <span href="https://lpdf.io">the terms</span> now"#),
    ("underline", r#"See <span underline="true">the terms</span> now"#),
    ("strike", r#"See <span strike="true">the terms</span> now"#),
    ("tint", r#"See <code-span>the terms</code-span> now"#),
    ("footnote", r#"See the terms<fn>The note.</fn> now"#),
];

#[test]
fn a_marked_span_in_an_aligned_text_stays_inside_the_box_after_the_words_before_it() {
    for align in ["right", "center"] {
        for (kind, markup) in MARKED {
            let tree = page(&format!(r#"<text align="{align}">{markup}</text>"#));
            let texts = drawn(text_box(&tree));
            let expected: &[&str] = if kind == "footnote" { &["See the terms", "1", "now"] } else { &["See", "the terms", "now"] };
            assert_eq!(words(&texts), expected, "{align} {kind}");
            assert_in_order(&texts);
            let (start, end) = (texts[0].left, texts[texts.len() - 1].right);
            assert!(start >= LEFT - NEAR && end <= RIGHT + NEAR, "{align} {kind}: the line runs from {start} to {end}");
            match align {
                "right" => assert_near(end, RIGHT, &format!("{kind}: the end of the right-aligned line")),
                _ => assert_near((start + end) / 2.0, MIDDLE, &format!("{kind}: the middle of the centered line")),
            }
        }
    }
}

#[test]
fn the_line_tint_and_link_area_of_a_span_in_an_aligned_text_are_under_its_own_words() {
    for align in ["right", "center"] {
        for (kind, markup) in &MARKED[..4] {
            let tree = page(&format!(r#"<text align="{align}">{markup}</text>"#));
            let texts = drawn(text_box(&tree));
            let span = text(&texts, "the terms");
            let all = nodes(text_box(&tree));
            let (from, to) = match *kind {
                "link" => {
                    let link = all.iter().find(|v| v["type"] == "link").expect("a link");
                    (num(link, "x"), num(link, "x") + num(link, "width"))
                }
                "tint" => {
                    let tint = all.iter().find(|v| v["type"] == "box" && v["fill"].is_string()).expect("a tint");
                    (num(tint, "x"), num(tint, "x") + num(tint, "width"))
                }
                _ => {
                    let line = all.iter().find(|v| v["type"] == "line").expect("a line");
                    (num(line, "x1"), num(line, "x2"))
                }
            };
            assert_near(from, span.left, &format!("{align} {kind}: the start of what is drawn with the span"));
            assert_near(to, span.right, &format!("{align} {kind}: its end"));
        }
    }
}

#[test]
fn each_line_of_an_aligned_text_that_wraps_is_aligned_on_its_own_spans_included() {
    let markup = r##"The <span color="#ff0000">first clause</span> applies to every order placed after the start of the term, and <span href="https://lpdf.io">the terms</span> say what <span underline="true">a notice</span> must hold."##;
    for align in ["right", "center"] {
        let tree = page(&format!(r#"<column width="200pt"><text align="{align}">{markup}</text></column>"#));
        let column = &tree["pages"][0]["nodes"][0];
        let text_box = &column["nodes"][0];
        let (left, width) = (num(text_box, "x"), num(text_box, "width"));
        let lines = lines(drawn(text_box));
        assert!(lines.len() > 2, "{align}: the text wraps, {lines:?}");
        for line in &lines {
            assert_in_order(line);
            let (start, end) = (line[0].left, line[line.len() - 1].right);
            match align {
                "right" => assert_near(end, left + width, &format!("the end of the right-aligned line {line:?}")),
                _ => assert_near((start + end) / 2.0, left + width / 2.0, &format!("the middle of the centered line {line:?}")),
            }
        }
    }
}

#[test]
fn a_line_drawn_as_one_text_is_left_to_the_writer_to_align() {
    for (align, anchor) in [("right", RIGHT), ("center", MIDDLE)] {
        let tree = page(&format!(r#"<text align="{align}">Notice: clause 4, page 3</text>"#));
        let texts: Vec<&Value> = nodes(text_box(&tree)).into_iter().filter(|v| v["type"] == "text").collect();
        assert_eq!(texts.len(), 1, "a text with no span is one text");
        assert_eq!(texts[0]["text_align"], align);
        assert_near(num(texts[0], "x"), anchor, &format!("a {align}-aligned text is anchored at its edge"));

        let tree = page(&format!(r#"<text align="{align}"><span font="Helvetica-Bold">Notice:</span> <span font="Helvetica-Bold">clause 4</span></text>"#));
        let texts: Vec<&Value> = nodes(text_box(&tree)).into_iter().filter(|v| v["type"] == "text").collect();
        assert_eq!(texts.len(), 1, "spans in one font and color are one text");
        assert_eq!(texts[0]["text_align"], align);
    }
}

#[test]
fn a_line_of_an_aligned_text_with_spans_that_has_none_of_them_is_left_to_the_writer() {
    let markup = r##"Every order placed after the start of the term is covered by the clause, which says what a notice must hold, <span color="#ff0000">unless stated</span>"##;
    let tree = page(&format!(r#"<column width="200pt"><text align="right">{markup}</text></column>"#));
    let first = nodes(&tree["pages"][0]["nodes"][0]["nodes"][0]).into_iter().find(|v| v["type"] == "text").cloned().expect("a text");
    assert!(!first["content"].as_str().unwrap().contains("unless"), "the span is not on the first line: {first}");
    assert_eq!(first["text_align"], "right");
}

#[test]
fn a_markdown_table_keeps_bold_words_and_links_in_order_in_its_right_aligned_and_centered_columns() {
    let table = "| Left | Right | Center |\n|:--|--:|:-:|\n| x | Pay **now** or see [terms](https://lpdf.io) | Pay **now** or see [terms](https://lpdf.io) |\n";
    let tree = section(&format!(r#"<markdown margin="50pt"><markdown-content><![CDATA[{table}]]></markdown-content></markdown>"#));
    let cells: Vec<&Value> = nodes(&tree["pages"][0]).into_iter()
        .filter(|v| v["type"] == "box" && v["nodes"].as_array().is_some_and(|kids| kids.iter().any(|k| k["content"] == "Pay")))
        .collect();
    assert_eq!(cells.len(), 2, "the right-aligned and the centered cell");
    for (cell, align) in cells.into_iter().zip(["right", "center"]) {
        let (left, width) = (num(cell, "x"), num(cell, "width"));
        let texts = drawn(cell);
        assert_eq!(words(&texts), ["Pay", "now", "or see", "terms"], "{align}");
        assert_in_order(&texts);
        let (start, end) = (texts[0].left, texts[texts.len() - 1].right);
        assert!(start >= left - NEAR && end <= left + width + NEAR, "{align}: the cell runs from {left} for {width}, the line from {start} to {end}");
        let link = nodes(cell).into_iter().find(|v| v["type"] == "link").expect("a link");
        let terms = text(&texts, "terms");
        assert_near(num(link, "x"), terms.left, &format!("{align}: the start of the link area"));
        match align {
            "right" => assert_near(end, left + width, "the end of the right-aligned line"),
            _ => assert_near((start + end) / 2.0, left + width / 2.0, "the middle of the centered line"),
        }
    }
}

#[test]
fn a_span_in_a_right_aligned_text_at_follows_the_words_before_it() {
    let tree = section(&format!(
        r#"<layer><text-at x="100pt" y="40pt" width="300pt" align="right">{NOTICE}</text-at></layer><layout margin="50pt"/>"#
    ));
    let layer = tree["pages"][0]["nodes"].as_array().unwrap().iter().find(|n| n["type"] == "canvas-layer").expect("a layer");
    let texts = drawn(&layer["nodes"][0]);
    assert_eq!(words(&texts), ["Notice:", "clause 4, page 3"]);
    assert_in_order(&texts);
    assert_near(texts[1].right, 400.0, "the line ends at the right edge of the box");
}
