//! Grids nested in grids, and containers too small to hold containers. A grid keeps the tree it laid a cell out in
//! to find the tallest of its row when stretching the cell to the row changes nothing (`same_when_stretched`), so
//! nested grids are laid out in work that grows with their depth. A container narrower than 2.5 mm, or set lower,
//! lays out its own box, its leaf content and its empty boxes alone (`without_inner_containers`), which ends the
//! grids that do stretch at each level.

use super::*;
use crate::parse::parse;
use serde_json::Value;

/// The pages the layout makes of `body`, on A4 with a 28pt margin.
fn pages_of(body: &str) -> Vec<RenderPage> {
    let xml = format!(
        r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
    );
    let mut doc = parse(&xml).unwrap();
    doc.section_layouts().iter().flat_map(layout_page).collect()
}

/// The layout of `body` as the engine hands it out, with every cell of a grid laid out again to stretch it when
/// `stretch_every_cell`, as before cells kept their first tree.
fn tree_of(body: &str, stretch_every_cell: bool) -> String {
    STRETCH_EVERY_CELL.with(|stretch| stretch.set(stretch_every_cell));
    let pages = pages_of(body);
    STRETCH_EVERY_CELL.with(|stretch| stretch.set(false));
    crate::render::pages_to_json(&pages, Value::Null, Value::Null)
}

/// The work of laying out `body` (`LAYOUT_WORK`).
fn work_of(body: &str) -> usize {
    LAYOUT_WORK.with(|work| work.set(0));
    pages_of(body);
    LAYOUT_WORK.with(Cell::get)
}

/// `depth` levels of `open(level)` and `close(level)`, outermost first, around one text.
fn chain(depth: usize, open: impl Fn(usize) -> String, close: impl Fn(usize) -> String) -> String {
    let opening: String = (0..depth).map(&open).collect();
    let closing: String = (0..depth).rev().map(&close).collect();
    format!("{opening}<text>deep</text>{closing}")
}

/// Every node drawn on `pages`, depth first, in the order they are drawn.
fn drawn(pages: &[RenderPage]) -> Vec<&RenderNode> {
    fn walk<'a>(node: &'a RenderNode, out: &mut Vec<&'a RenderNode>) {
        out.push(node);
        if let RenderNode::Box(RenderBox { children, .. })
        | RenderNode::Link(RenderLink { children, .. })
        | RenderNode::CanvasLayer(RenderCanvasLayer { children, .. }) = node
        {
            for child in children {
                walk(child, out);
            }
        }
    }
    let mut out = Vec::new();
    for node in pages.iter().flat_map(|page| &page.nodes) {
        walk(node, &mut out);
    }
    out
}

/// The words of the texts drawn on `pages`, in the order they are drawn.
fn words_of(pages: &[RenderPage]) -> Vec<String> {
    drawn(pages)
        .into_iter()
        .filter_map(|node| match node {
            RenderNode::Text(text) => Some(text.content.trim().to_string()),
            _ => None,
        })
        .collect()
}

const LONG: &str = "lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet";

#[test]
fn a_grid_lays_out_each_cell_as_it_did_when_every_cell_was_stretched_again() {
    let link = r#"<link href="https://example.com">"#;
    let cases = [
        ("nested grids", chain(12, |_| r#"<grid cols="1">"#.into(), |_| "</grid>".into())),
        (
            "nested grids with padding, a background and a border",
            chain(
                5,
                |_| r##"<grid cols="3" padding="4pt" background="#eeeeee" border="1pt #333333"><text>a</text><column background="#ddddff">"##.into(),
                |_| "</column><text>c</text></grid>".into(),
            ),
        ),
        (
            "a nested cell beside a longer text",
            chain(6, |_| r#"<grid cols="2"><column>"#.into(), |_| format!("</column><text>{LONG}</text></grid>")),
        ),
        (
            "a nested cell taller than the text beside it",
            chain(6, |_| r#"<grid cols="2"><text>short</text><column>"#.into(), |_| "</column></grid>".into()),
        ),
        (
            "cells of full and fill height",
            r#"<grid cols="2"><column height="fill"><text>x</text></column><column height="full"><text>y</text></column></grid>"#.into(),
        ),
        (
            "a cell as tall as its row holding a child of full height",
            r##"<grid cols="1"><column><text>a</text><frame height="full" background="#cccccc"/></column></grid>"##.into(),
        ),
        (
            "a cell as tall as its row holding a child of fill height",
            r##"<grid cols="1"><column><text>a</text><frame height="fill" background="#cccccc"/></column></grid>"##.into(),
        ),
        (
            "a row as a cell, with a text filling it and a vertical divider",
            format!(r#"<grid cols="2"><row><text width="fill">a</text><divider direction="vertical"/></row><text>{LONG}</text></grid>"#),
        ),
        (
            "a link as a cell holding a child of fill height",
            format!(r##"<grid cols="1">{link}<text>a</text><frame height="fill" background="#cccccc"/></link></grid>"##),
        ),
        (
            "a link in a cell holding a child of full height",
            format!(r##"<grid cols="2"><column>{link}<text>a</text><frame height="full" background="#cccccc"/></link></column><text>{LONG}</text></grid>"##),
        ),
        (
            "cells of a set height holding a child of full height",
            r##"<grid cols="2"><column height="40pt"><text>a</text><frame height="full" background="#cccccc"/></column><text>b</text></grid>"##.into(),
        ),
        (
            "a link of a set height as a cell",
            format!(r##"<grid cols="2"><link href="https://example.com" height="40pt"><text>a</text><frame height="full" background="#cccccc"/></link><text>{LONG}</text></grid>"##),
        ),
        ("a vertical divider as a cell", format!(r#"<grid cols="2"><divider direction="vertical"/><text>{LONG}</text></grid>"#)),
        ("a frame as a cell", r#"<grid cols="2"><frame><text>x</text></frame><text>y</text></grid>"#.into()),
        (
            "columns justified to the center and the end as cells",
            format!(r#"<grid cols="3"><column justify="center"><text>a</text></column><column justify="end"><text>b</text></column><text>{LONG}</text></grid>"#),
        ),
        (
            "grids each in a link",
            chain(12, |i| if i % 2 == 0 { r#"<grid cols="1">"#.into() } else { link.into() }, |i| if i % 2 == 0 { "</grid>".into() } else { "</link>".into() }),
        ),
        ("grids of a set height", chain(8, |_| r#"<grid cols="1" height="50pt">"#.into(), |_| "</grid>".into())),
        (
            "a grid split across pages",
            format!(
                r#"<grid cols="2">{}</grid>"#,
                (0..120).map(|i| format!("<column><text>cell {i}</text><text>{}</text></column>", if i % 3 == 0 { LONG } else { "one" })).collect::<String>(),
            ),
        ),
    ];
    for (name, body) in cases {
        assert_eq!(tree_of(&body, false), tree_of(&body, true), "{name}");
    }
}

/// The four kinds of chain of nested grids, `depth` elements deep.
fn grid_chains(depth: usize) -> [(&'static str, String); 4] {
    [
        ("plain grids", chain(depth, |_| r#"<grid cols="1">"#.into(), |_| "</grid>".into())),
        ("grids of a set height", chain(depth, |_| r#"<grid cols="1" height="50pt">"#.into(), |_| "</grid>".into())),
        (
            "grids each in a link",
            chain(
                depth,
                |i| if i % 2 == 0 { r#"<grid cols="1">"#.into() } else { r#"<link href="https://example.com">"#.into() },
                |i| if i % 2 == 0 { "</grid>".into() } else { "</link>".into() },
            ),
        ),
        (
            "grids beside a taller cell",
            chain(depth, |_| r#"<grid cols="2">"#.into(), |i| format!(r#"<frame height="{}pt"/></grid>"#, 20 + 5 * (depth - 1 - i))),
        ),
    ]
}

#[test]
fn nested_grids_take_work_that_grows_with_their_depth() {
    // Laid out again at each level, 16 levels would take 16 times the work of 12, or 4 times with a link at every
    // other level.
    for ((name, shallow), (_, deep)) in grid_chains(12).into_iter().zip(grid_chains(16)) {
        let (shallow, deep) = (work_of(&shallow), work_of(&deep));
        assert!((deep as f64) < 1.5 * shallow as f64, "{name}: {deep} steps at 16 levels, {shallow} at 12");
    }
}

/// The fill and the border width of each box drawn on `pages` with a fill, in the order they are drawn.
fn fills_of(pages: &[RenderPage]) -> Vec<(&str, f32)> {
    drawn(pages)
        .into_iter()
        .filter_map(|node| match node {
            RenderNode::Box(b) => b.fill.as_deref().map(|fill| (fill, b.border_width)),
            _ => None,
        })
        .collect()
}

// The sizes under the limit are 2 mm, 5.67pt: a width and a height are written in points.

#[test]
fn a_container_narrower_than_2_5mm_draws_its_box_its_leaf_content_and_its_empty_boxes_and_no_container_inside_it() {
    let pages = pages_of(
        r##"<column width="5.67pt" background="#ff0000" border="1pt #000000"><text>a</text><frame background="#0000ff"><text>b</text></frame><frame width="4pt" height="4pt" background="#00ff00"/></column>"##,
    );
    assert_eq!(words_of(&pages), ["a"]);
    assert_eq!(fills_of(&pages), [("#ff0000", 1.0), ("#00ff00", 0.0)], "the column's box and the empty frame's");
}

#[test]
fn a_container_set_lower_than_2_5mm_draws_its_leaf_content_and_no_container_inside_it() {
    let pages = pages_of(r##"<column height="5.67pt" background="#ff0000"><text>a</text><column><text>b</text></column></column>"##);
    assert_eq!(words_of(&pages), ["a"]);
}

#[test]
fn a_row_set_lower_than_2_5mm_draws_the_empty_frames_in_it() {
    let pages = pages_of(
        r##"<row height="5.67pt" gap="4pt"><frame width="5.67pt" height="5.67pt" background="#ff0000"/><frame width="5.67pt" height="5.67pt" background="#00ff00"/></row>"##,
    );
    assert_eq!(fills_of(&pages), [("#ff0000", 0.0), ("#00ff00", 0.0)]);
}

#[test]
fn a_container_of_2_5mm_lays_out_the_containers_inside_it() {
    // 2.5 mm as the engine works it out, to the last bit.
    for size in [format!(r#"width="{MIN_NESTING_SIZE}pt""#), format!(r#"height="{MIN_NESTING_SIZE}pt""#)] {
        let pages = pages_of(&format!(r#"<column {size}><text>a</text><column><text>b</text></column></column>"#));
        assert_eq!(words_of(&pages), ["a", "b"], "{size}");
    }
}

#[test]
fn a_small_icon_in_a_2mm_frame_still_draws() {
    let pages = pages_of(r##"<frame width="5.67pt" height="5.67pt"><shape type="circle" r="2pt" fill="#000000"/></frame>"##);
    assert!(drawn(&pages).iter().any(|node| matches!(node, RenderNode::CanvasEllipse(_))), "no circle drawn");
}

#[test]
fn a_narrow_container_split_across_pages_leaves_no_room_for_the_containers_it_does_not_lay_out() {
    // A column 2mm wide runs over two pages. The column inside it isn't laid out, so it takes no room on the first
    // page: that page holds as many texts as it does without it.
    let texts = |n: usize| "<text>a</text>".repeat(n);
    let first_page = |inside: &str| {
        let pages = pages_of(&format!(r#"<column width="5.67pt">{}{inside}{}</column>"#, texts(10), texts(90)));
        assert!(pages.len() > 1, "{} pages", pages.len());
        words_of(&pages[..1]).len()
    };
    assert_eq!(first_page("<column><text>b</text><text>b</text></column>"), first_page(""));
}
