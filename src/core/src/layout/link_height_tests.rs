//! The height a link gives its children: none at `auto` height, its own at a set height, and the one it is given at
//! `full` or `fill` height, as any box gives them (`inner_height`). A child of `full` height inside a link of `auto`
//! height is as tall as the link's other content, so it stays inside the link wherever the link is.

use super::*;
use crate::parse::parse;

/// The pages the layout makes of `body`, on A4 with a 28pt margin.
fn pages_of(body: &str) -> Vec<RenderPage> {
    let xml = format!(
        r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
    );
    let mut doc = parse(&xml).unwrap();
    doc.section_layouts().iter().flat_map(layout_page).collect()
}

/// Every node drawn on `pages`, depth first.
fn drawn(pages: &[RenderPage]) -> Vec<&RenderNode> {
    fn walk<'a>(node: &'a RenderNode, out: &mut Vec<&'a RenderNode>) {
        out.push(node);
        match node {
            RenderNode::Box(b) => b.children.iter().for_each(|child| walk(child, out)),
            RenderNode::Link(l) => l.children.iter().for_each(|child| walk(child, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    pages.iter().flat_map(|page| &page.nodes).for_each(|node| walk(node, &mut out));
    out
}

/// The y and height of the one box filled with `fill`.
fn filled(pages: &[RenderPage], fill: &str) -> (f32, f32) {
    let boxes: Vec<(f32, f32)> = drawn(pages)
        .into_iter()
        .filter_map(|node| match node {
            RenderNode::Box(b) if b.fill.as_deref() == Some(fill) => Some((b.y, b.height)),
            _ => None,
        })
        .collect();
    assert_eq!(boxes.len(), 1, "one box filled with {fill}");
    boxes[0]
}

/// The y and height of the first link.
fn link(pages: &[RenderPage]) -> (f32, f32) {
    drawn(pages)
        .into_iter()
        .find_map(|node| match node {
            RenderNode::Link(l) => Some((l.y, l.height)),
            _ => None,
        })
        .expect("a link")
}

/// The y of the text `words`.
fn text_y(pages: &[RenderPage], words: &str) -> f32 {
    drawn(pages)
        .into_iter()
        .find_map(|node| match node {
            RenderNode::Text(t) if t.content.trim() == words => Some(t.y),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a text {words:?}"))
}

fn assert_near(found: f32, expected: f32, what: &str) {
    assert!((found - expected).abs() < 0.001, "{what}: {found}, expected {expected}");
}

/// The height of the text `a` beside the frames: the box a lone text is laid out in.
fn line_h() -> f32 {
    drawn(&pages_of("<text>a</text>"))
        .into_iter()
        .find_map(|node| match node {
            RenderNode::Box(b) => Some(b.height),
            _ => None,
        })
        .expect("the box of the text")
}

const LINK: &str = r#"<link href="https://example.com">"#;
const FULL_FRAME: &str = r##"<frame height="full" background="#cccccc"/>"##;
const LONG: &str = "lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet \
    lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet";

#[test]
fn a_full_child_of_a_link_in_a_column_is_as_tall_as_the_links_other_content() {
    let pages = pages_of(&format!("<column>{LINK}<text>a</text>{FULL_FRAME}</link><text>after</text></column>"));
    let h = line_h();
    let (frame_y, frame_h) = filled(&pages, "#cccccc");
    let (link_y, link_h) = link(&pages);
    assert_near(frame_h, h, "the frame");
    assert_near(link_h, 2.0 * h, "the link");
    assert_near(frame_y + frame_h, link_y + link_h, "the frame's foot, at the link's");
    assert_near(text_y(&pages, "after"), link_y + link_h, "the text after the link");
}

#[test]
fn a_full_child_of_a_link_at_the_top_of_a_page_keeps_it_to_one_page() {
    let pages = pages_of(&format!("{LINK}<text>a</text>{FULL_FRAME}</link><text>after</text>"));
    let h = line_h();
    assert_eq!(pages.len(), 1);
    assert_near(filled(&pages, "#cccccc").1, h, "the frame");
    let (link_y, link_h) = link(&pages);
    assert_near(link_h, 2.0 * h, "the link");
    assert_near(text_y(&pages, "after"), link_y + link_h, "the text after the link");
}

#[test]
fn a_full_child_of_a_link_in_a_row_of_a_set_height_stays_inside_the_row() {
    let pages = pages_of(&format!(
        r##"<row height="40pt" background="#eeeeee">{LINK}<text>a</text>{FULL_FRAME}</link><text width="fill">b</text></row><text>after</text>"##
    ));
    let h = line_h();
    let (row_y, row_h) = filled(&pages, "#eeeeee");
    let (frame_y, frame_h) = filled(&pages, "#cccccc");
    assert_near(frame_h, h, "the frame");
    assert_near(link(&pages).1, 2.0 * h, "the link");
    assert!(frame_y + frame_h <= row_y + row_h, "the frame ends at {}, inside the row ending at {}", frame_y + frame_h, row_y + row_h);
    assert_near(text_y(&pages, "after"), row_y + row_h, "the text after the row");
}

#[test]
fn a_full_child_of_a_link_of_a_set_height_takes_the_links_height_in_a_grid() {
    let pages = pages_of(&format!(r#"<grid cols="2"><link href="https://example.com" height="40pt"><text>a</text>{FULL_FRAME}</link><text>{LONG}</text></grid>"#));
    let (link_y, link_h) = link(&pages);
    let (frame_y, frame_h) = filled(&pages, "#cccccc");
    assert_near(link_h, 40.0, "the link");
    assert_near(frame_h, 40.0, "the frame, as in a column of 40pt");
    assert_near(frame_y, link_y + line_h(), "the frame, below the text");
    let column = pages_of(&format!(r#"<grid cols="2"><column height="40pt"><text>a</text>{FULL_FRAME}</column><text>{LONG}</text></grid>"#));
    assert_eq!(filled(&column, "#cccccc"), (frame_y, frame_h), "the frame, where a column of 40pt puts it");
}

#[test]
fn a_link_of_full_height_gives_its_children_the_height_it_is_given() {
    let pages = pages_of(&format!(
        r##"<column height="200pt"><link href="https://example.com" height="full"><text>a</text><frame height="fill" background="#cccccc"/></link></column>"##
    ));
    assert_near(link(&pages).1, 200.0, "the link");
    assert_near(filled(&pages, "#cccccc").1, 200.0 - line_h(), "the frame, what the text leaves");
}

#[test]
fn a_link_of_fill_height_gives_its_children_the_height_it_takes() {
    let pages = pages_of(&format!(
        r#"<column height="200pt"><text>x</text><link href="https://example.com" height="fill"><text>a</text>{FULL_FRAME}</link></column>"#
    ));
    let link_h = link(&pages).1;
    assert_near(link_h, 200.0 - line_h(), "the link, what the text leaves");
    assert_near(filled(&pages, "#cccccc").1, link_h, "the frame, all of the link");
}

#[test]
fn a_link_cell_a_grid_stretches_gives_its_children_the_rows_height() {
    let pages = pages_of(&format!(
        r##"<grid cols="2" background="#eeeeee">{LINK}<text>a</text><frame height="fill" background="#cccccc"/></link><text>{LONG}</text></grid>"##
    ));
    let (_, row_h) = filled(&pages, "#eeeeee");
    assert!(row_h > 3.0 * line_h(), "the long text makes the row taller than the link's text");
    assert_near(link(&pages).1, row_h, "the link, stretched to the row");
    assert_near(filled(&pages, "#cccccc").1, row_h - line_h(), "the frame, what the text leaves of the row");
}
