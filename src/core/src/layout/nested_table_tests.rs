//! Tables nested in the cells of tables. A table lays each cell out where it stands to find the height of its row,
//! and keeps that tree when stretching the cell to the row changes nothing but the height of its box
//! (`same_when_stretched_in_table`), so nested tables, and the Markdown quotes made of them, are laid out in work that
//! grows with their depth.

use std::path::PathBuf;

use serde_json::Value;

use super::*;
use crate::parse::parse;

/// The pages the layout makes of `section`, the inside of a `<section>`, on A4.
fn pages_of(section: &str) -> Vec<RenderPage> {
    let xml = format!(r#"<lpdf version="1"><document size="a4"><section>{section}</section></document></lpdf>"#);
    let mut doc = parse(&xml).unwrap();
    doc.section_layouts().iter().flat_map(layout_page).collect()
}

/// A `<layout>` with a 28pt margin around `body`.
fn layout(body: &str) -> String {
    format!(r#"<layout margin="28pt">{body}</layout>"#)
}

/// A `<markdown>` with a 28pt margin around `markdown`.
fn markdown(markdown: &str) -> String {
    format!(r#"<markdown margin="28pt" bookmarks="2"><markdown-content><![CDATA[{markdown}]]></markdown-content></markdown>"#)
}

/// `xml` rendered to a PDF, with every cell of a table laid out again to stretch it when `stretch_every_cell`, as
/// before cells kept their first tree.
fn pdf_of(xml: &str, stretch_every_cell: bool) -> Vec<u8> {
    STRETCH_EVERY_CELL.with(|stretch| stretch.set(stretch_every_cell));
    let pdf = crate::LpdfEngine::render_xml_to_pdf_bytes(xml);
    STRETCH_EVERY_CELL.with(|stretch| stretch.set(false));
    pdf.unwrap()
}

/// The work of laying out `section` (`LAYOUT_WORK`).
fn work_of(section: &str) -> usize {
    LAYOUT_WORK.with(|work| work.set(0));
    pages_of(section);
    LAYOUT_WORK.with(Cell::get)
}

/// `depth` levels of `open(level)` and `close(level)`, outermost first, around one text.
fn chain(depth: usize, open: impl Fn(usize) -> String, close: impl Fn(usize) -> String) -> String {
    let opening: String = (0..depth).map(&open).collect();
    let closing: String = (0..depth).rev().map(&close).collect();
    format!("{opening}<text>deep</text>{closing}")
}

/// `depth` nested tables, one row and one cell each, around one text.
fn nested_tables(depth: usize) -> String {
    layout(&chain(depth, |_| r#"<table cols="1fr"><tr><td>"#.into(), |_| "</td></tr></table>".into()))
}

/// `depth` nested Markdown quotes around one paragraph.
fn nested_quotes(depth: usize) -> String {
    markdown(&format!("{} deep", ">".repeat(depth)))
}

/// `depth` nested tables, each in a cell beside a frame taller than it, which reads nothing of its row's height.
fn nested_tables_beside_taller_cells(depth: usize) -> String {
    layout(&chain(
        depth,
        |_| r#"<table cols="1fr 4pt"><tr><td>"#.into(),
        |i| format!(r#"</td><td><frame height="{}pt"/></td></tr></table>"#, 20 + 5 * (depth - i)),
    ))
}

/// A table of 80 rows that splits across pages, each row holding nested tables, `depth` tables deep with it: the rows
/// are measured to split the table (`split_table`) as well as laid out.
fn nested_tables_in_a_split_table(depth: usize) -> String {
    let nested = chain(depth - 1, |_| r#"<table cols="1fr"><tr><td>"#.into(), |_| "</td></tr></table>".into());
    layout(&format!(r#"<table cols="1fr 1fr">{}</table>"#, format!("<tr><td><text>row</text></td><td>{nested}</td></tr>").repeat(80)))
}

/// `depth` nested columns around one text: what the nested tables are timed against.
fn nested_columns(depth: usize) -> String {
    layout(&chain(depth, |_| "<column>".into(), |_| "</column>".into()))
}

const LONG: &str = "lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet";

/// The inside of a section of each kind of table the tests lay out both ways.
fn cases() -> Vec<(&'static str, String)> {
    let tall = |n: usize| (0..n).map(|i| format!("<text>line {i}</text>")).collect::<String>();
    vec![
        ("nested tables", nested_tables(12)),
        ("nested tables beside taller cells", nested_tables_beside_taller_cells(8)),
        (
            "nested tables with padding, borders, backgrounds and stripes",
            layout(&chain(
                5,
                |_| r##"<table cols="1fr 2fr" padding="4pt" gap="3pt" background="#eeeeee" border="1pt #333333" stripe="#f4f4ff" row-border="0.5pt #999999" head-border="1pt #000000"><thead background="#ccccff"><td><text>head</text></td><td><text>head</text></td></thead><tr background="#ffeeee"><td padding="5pt" background="#ddddff" border="1pt #0000ff" radius="3pt"><text>a</text></td><td padding="2pt 6pt" background="#ddffdd">"##.into(),
                |_| r#"</td></tr><tr><td><text>b</text></td><td><text>c</text></td></tr></table>"#.into(),
            )),
        ),
        (
            "a nested cell beside a longer text",
            layout(&chain(5, |_| r##"<table cols="1fr 1fr"><tr><td background="#eeeeff">"##.into(), |_| format!("</td><td><text>{LONG}</text></td></tr></table>"))),
        ),
        (
            "cells shorter than their row with a background, a border and a shadow",
            layout(&format!(
                r##"<table cols="1fr 1fr 1fr"><tr><td background="#ffeeee" border="1pt #ff0000" radius="4pt" shadow="2pt 2pt #000000 0.3"><text>a</text></td><td padding="6pt" background="#eeffee"><text>b</text><text>c</text></td><td><text>{LONG}</text></td></tr></table>"##
            )),
        ),
        (
            "cells aligned to the middle and the bottom, shorter than their row and as tall",
            layout(&format!(
                r##"<table cols="1fr 1fr 1fr 1fr"><tr><td valign="middle" background="#eeeeee"><text>a</text></td><td valign="bottom" background="#dddddd"><text>b</text></td><td valign="middle"><text>{LONG}</text></td><td valign="bottom"><text>{LONG}</text></td></tr></table>"##
            )),
        ),
        (
            "cells holding a child of full height, of fill height and a vertical divider",
            layout(&format!(
                r##"<table cols="1fr 1fr 1fr 1fr"><tr><td><text>a</text><frame height="full" background="#cccccc"/></td><td><text>b</text><frame height="fill" background="#cccccc"/></td><td><row><text width="fill">c</text><divider direction="vertical"/></row></td><td><text>{LONG}</text></td></tr></table>"##
            )),
        ),
        (
            // Set lower than 2.5 mm, the cell leaves out the column inside it, until it is stretched to its row.
            "a cell set lower than 2.5 mm holding a column",
            layout(&format!(r##"<table cols="1fr 1fr"><tr><td height="5pt" background="#eeeeee"><column><text>a</text></column></td><td><text>{LONG}</text></td></tr></table>"##)),
        ),
        (
            "cells of a set height, of full height and of fill height",
            layout(&format!(
                r##"<table cols="1fr 1fr 1fr 1fr"><tr><td height="12pt" background="#eeeeee"><text>a</text><frame height="full" background="#cccccc"/></td><td height="full" background="#dddddd"><text>b</text></td><td height="fill"><text>c</text></td><td><text>{LONG}</text></td></tr></table>"##
            )),
        ),
        (
            "cells that span rows and columns, around nested tables",
            layout(&chain(
                4,
                |_| r##"<table cols="1fr 1fr 1fr" border="1pt #333333"><tr><td rowspan="2" background="#eeeeff">"##.into(),
                |_| format!(
                    r##"</td><td colspan="2"><text>top</text></td></tr><tr><td><text>{LONG}</text></td><td><text>b</text></td></tr><tr><td colspan="2" background="#eeffee"><text>wide</text></td><td rowspan="2"><text>{LONG}</text></td></tr><tr><td><text>c</text></td><td><text>d</text></td></tr></table>"##
                ),
            )),
        ),
        (
            "a table split across pages, with a header row and nested tables in its cells",
            layout(&format!(
                r##"<table cols="1fr 2fr" border="0.5pt #999999"><thead background="#ccccff"><td><text>n</text></td><td><text>cell</text></td></thead>{}</table>"##,
                (0..90).map(|i| format!(
                    r#"<tr><td><text>{i}</text></td><td><table cols="1fr"><tr><td><table cols="1fr 1fr"><tr><td><text>{}</text></td><td>{}</td></tr></table></td></tr></table></td></tr>"#,
                    if i % 3 == 0 { LONG } else { "one" },
                    if i % 7 == 0 { tall(4) } else { "<text>x</text>".into() },
                )).collect::<String>(),
            )),
        ),
        (
            "rows and cells in optional content groups",
            layout(&chain(
                4,
                |_| r#"<table cols="1fr 1fr"><tr ocg="print"><td ocg="screen">"#.into(),
                |_| format!("</td><td><text>{LONG}</text></td></tr></table>"),
            )),
        ),
        ("nested Markdown quotes", nested_quotes(12)),
        (
            "nested Markdown quotes of several blocks, with headings and lists, across pages",
            markdown(
                &(0..40)
                    .map(|i| format!("> > ## Part {i}\n> >\n> > {LONG}\n> >\n> > - one\n> > - two\n> >\n> > > {LONG}\n\n"))
                    .collect::<String>(),
            ),
        ),
    ]
}

#[test]
fn a_table_lays_out_each_cell_as_it_did_when_every_cell_was_stretched_again() {
    for (name, section) in cases() {
        let xml = format!(r#"<lpdf version="1"><document size="a4"><section>{section}</section></document></lpdf>"#);
        assert!(pdf_of(&xml, false) == pdf_of(&xml, true), "{name}");
    }
}

#[test]
fn a_tagged_table_lays_out_each_cell_as_it_did_when_every_cell_was_stretched_again() {
    // Every cell is tagged, and a cell that spans rows sits in a box tagged as the row it starts in.
    let body = chain(
        3,
        |_| r##"<table cols="1fr 1fr 1fr"><thead><td><text>a</text></td><td><text>b</text></td><td><text>c</text></td></thead><tr><td rowspan="2" background="#eeeeff">"##.into(),
        |_| format!(r#"</td><td colspan="2"><text>{LONG}</text></td></tr><tr><td><text>d</text></td><td valign="middle"><text>e</text></td></tr></table>"#),
    );
    let xml = format!(
        r#"<lpdf version="1"><document size="a4"><meta title="Report" lang="en"/><output pdfua="1"/><section>{}</section></document></lpdf>"#,
        layout(&body),
    );
    let laid_out = |stretch_every_cell: bool| {
        STRETCH_EVERY_CELL.with(|stretch| stretch.set(stretch_every_cell));
        // As a render lays it out, the tags numbered from 1.
        let pages = lay_out_document(&parse(&xml).unwrap().section_layouts()).pages;
        STRETCH_EVERY_CELL.with(|stretch| stretch.set(false));
        (crate::render::pages_to_json(&pages, Value::Null, Value::Null), tagged_boxes(&pages))
    };
    let (tree, tags) = laid_out(false);
    assert!(tags.iter().any(|tag| tag.contains("TableCell")), "the cells are tagged");
    assert_eq!((tree, tags), laid_out(true));
}

/// The tag of each box drawn on `pages` with one, with the box's place and size, in the order they are drawn.
fn tagged_boxes(pages: &[RenderPage]) -> Vec<String> {
    fn walk(nodes: &[RenderNode], out: &mut Vec<String>) {
        for node in nodes {
            if let RenderNode::Box(b) = node {
                if let Some(tag) = &b.tag {
                    out.push(format!("{tag:?} at {} {} {} {}", b.x, b.y, b.width, b.height));
                }
                walk(&b.children, out);
            }
        }
    }
    let mut out = Vec::new();
    for page in pages {
        walk(&page.nodes, &mut out);
    }
    out
}

#[test]
fn the_table_fixtures_lay_out_as_they_did_when_every_cell_was_stretched_again() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures");
    for name in [
        "showcase-table", "showcase-table-auto", "showcase-table-colspan", "showcase-table-keep", "showcase-table-lines",
        "showcase-table-rowspan", "showcase-markdown-tables", "showcase-markdown-split",
    ] {
        let xml = std::fs::read_to_string(fixtures.join(format!("{name}.xml"))).unwrap();
        assert!(pdf_of(&xml, false) == pdf_of(&xml, true), "{name}");
    }
}

#[test]
fn nested_tables_take_work_that_grows_with_their_depth() {
    // Laid out again at each level, 31 levels would take 2^15 times the work of 16.
    let chains: [(&str, fn(usize) -> String); 4] = [
        ("tables", nested_tables),
        ("Markdown quotes", nested_quotes),
        ("tables beside taller cells", nested_tables_beside_taller_cells),
        ("tables in the rows of a table split across pages", nested_tables_in_a_split_table),
    ];
    for (name, of_depth) in chains {
        let (shallow, deep) = (work_of(&of_depth(16)), work_of(&of_depth(31)));
        assert!((deep as f64) < 2.5 * shallow as f64, "{name}: {deep} steps at 31 levels, {shallow} at 16");
    }
}

#[test]
fn thirty_one_nested_tables_take_about_the_work_of_their_ninety_five_levels_of_columns() {
    // A table, its row and its cell are three levels, and the row is not laid out: two steps for every three columns.
    let (tables, columns) = (work_of(&nested_tables(31)), work_of(&nested_columns(95)));
    assert!(tables <= columns, "{tables} steps for the tables, {columns} for the columns");
}

#[test]
fn the_nested_tables_go_as_deep_as_their_text() {
    // Narrower than 2.5 mm, a cell would leave out the tables inside it, and the chains would be shorter than they say.
    for section in [nested_tables(31), nested_quotes(31), nested_tables_beside_taller_cells(31)] {
        let words = pages_of(&section).iter().flat_map(|page| text_of(&page.nodes)).collect::<Vec<_>>();
        assert_eq!(words, ["deep"]);
    }
}

/// The words of the texts among `nodes` and inside them, in the order they are drawn.
fn text_of(nodes: &[RenderNode]) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| match node {
            RenderNode::Text(text) => vec![text.content.trim().to_string()],
            RenderNode::Box(RenderBox { children, .. }) | RenderNode::Link(RenderLink { children, .. }) => text_of(children),
            _ => Vec::new(),
        })
        .collect()
}
