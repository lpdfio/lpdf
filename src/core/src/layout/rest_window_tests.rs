//! What is left of a long container that pages cut (`rests`): each page splits a window of it, a copy of the children
//! the page can reach, and the pages come out as they did when each page split all of what was left, in work that
//! grows with the children rather than with their square.

use super::*;
use crate::parse::parse;
use serde_json::Value;

/// A document of one A4 section with `attrs`, its layout holding `body`.
fn document(attrs: &str, body: &str) -> String {
    format!(r#"<lpdf version="1"><document size="a4"><section{attrs}><layout margin="28pt">{body}</layout></section></document></lpdf>"#)
}

fn pages_of(xml: &str) -> Vec<RenderPage> {
    let mut doc = parse(xml).unwrap();
    doc.section_layouts().iter().flat_map(layout_page).collect()
}

/// The layout of `xml` as the engine hands it out, with each rest split whole when `whole`, as pages split all of
/// what was left of a container before windows.
fn tree_of(xml: &str, whole: bool) -> String {
    rests::WHOLE_RESTS.with(|rests| rests.set(whole));
    let pages = pages_of(xml);
    rests::WHOLE_RESTS.with(|rests| rests.set(false));
    crate::render::pages_to_json(&pages, Value::Null, Value::Null)
}

/// The work of laying out `xml` (`LAYOUT_WORK`).
fn work_of(xml: &str) -> usize {
    LAYOUT_WORK.with(|work| work.set(0));
    pages_of(xml);
    LAYOUT_WORK.with(Cell::get)
}

const LONG: &str = "lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet";

/// `n` one-line texts.
fn lines(n: usize) -> String {
    (0..n).map(|i| format!("<text>Line {i}</text>")).collect()
}

/// `n` texts of a few lines each, which pages cut through.
fn paragraphs(n: usize) -> String {
    (0..n).map(|i| format!("<text>Paragraph {i}. {LONG} {}</text>", if i % 3 == 0 { LONG } else { "" })).collect()
}

/// `n` rows of a table of two columns, every fifth with a cell of a few lines.
fn rows(n: usize) -> String {
    (0..n)
        .map(|i| format!("<tr><td><text>Item {i}</text></td><td><text>{}</text></td></tr>", if i % 5 == 0 { LONG } else { "one" }))
        .collect()
}

/// `n` tags 80pt wide, every fourth of a few lines, which a row wraps.
fn tags(n: usize) -> String {
    (0..n).map(|i| format!(r#"<text width="80pt">Tag {i}{}</text>"#, if i % 4 == 0 { " with a few more words" } else { "" })).collect()
}

/// Two long columns side by side, `n` lines in each, of the widths a row gives them.
fn side_by_side(attrs: &str, n: usize) -> String {
    format!(r#"<row{attrs}><column width="fill">{}</column><column width="fill">{}</column></row>"#, lines(n), paragraphs(n / 3))
}

const HEADER: &str = r##"<thead background="#eeeeee"><td><text>Name</text></td><td><text>Value</text></td></thead>"##;

/// The long containers, each split across many pages, that a window must split as the whole rest is split.
fn cases() -> Vec<(&'static str, String)> {
    let page = |body: String| document("", &body);
    let mut cases = vec![
        ("a column of lines", page(format!("<column>{}</column>", lines(300)))),
        ("a column of paragraphs, cut through them", page(format!(r#"<column gap="6pt">{}</column>"#, paragraphs(120)))),
        (
            "a column with padding, a background and a border",
            page(format!(r##"<column padding="12pt" gap="4pt" background="#eeeeee" border="1pt #333333">{}{}</column>"##, paragraphs(60), lines(120))),
        ),
        (
            "a long column first in a column",
            page(format!(r#"<column padding="4pt"><column padding="6pt">{}</column>{}</column>"#, paragraphs(90), lines(60))),
        ),
        (
            "columns nested three deep after a title",
            page(format!(r#"<column><text>Title</text><column padding="6pt"><column gap="2pt">{}</column><text>after</text></column>{}</column>"#, paragraphs(80), lines(50))),
        ),
        ("a link holding lines", page(format!(r#"<link href="https://example.com">{}</link>"#, lines(200)))),
        (
            "a list of items, some long, some holding a list",
            page(format!(
                r#"<list type="number">{}</list>"#,
                (0..150)
                    .map(|i| match i % 7 {
                        0 => format!("<li><text>Item {i}. {LONG}</text></li>"),
                        3 => format!(r#"<li><text>Item {i}</text><list>{}</list></li>"#, "<li><text>inner</text></li>".repeat(4)),
                        _ => format!("<li><text>Item {i}</text></li>"),
                    })
                    .collect::<String>(),
            )),
        ),
        ("a table with a header", page(format!(r#"<table cols="1fr 2fr" border="1pt #999999">{HEADER}{}</table>"#, rows(300)))),
        ("a table sized by its cells", page(format!(r#"<table cols="auto auto">{HEADER}{}</table>"#, rows(250)))),
        (
            "a table with stripes and rules",
            page(format!(r##"<table cols="1fr 1fr" stripe="#f4f4f4" head-border="1pt #333333" row-border="0.5pt #cccccc">{HEADER}{}</table>"##, rows(250))),
        ),
        (
            "a table with runs of rows that keep together and cells that span rows",
            page(format!(
                r#"<table cols="1fr 1fr 1fr">{}</table>"#,
                (0..240)
                    .map(|i| match i % 9 {
                        0 => format!(r#"<tr><td rowspan="3"><text>Span {i}</text></td><td><text>a</text></td><td><text>b</text></td></tr>"#),
                        1 | 2 => format!("<tr><td><text>Under {i}</text></td><td><text>{LONG}</text></td></tr>"),
                        4 | 5 => format!(r#"<tr keep="with-next"><td><text>Kept {i}</text></td><td><text>{LONG}</text></td><td><text>c</text></td></tr>"#),
                        _ => format!("<tr><td><text>Row {i}</text></td><td><text>a</text></td><td><text>b</text></td></tr>"),
                    })
                    .collect::<String>(),
            )),
        ),
        ("a table in a column", page(format!(r#"<column padding="8pt"><text>Table</text><table cols="1fr 1fr">{HEADER}{}</table><text>after</text></column>"#, rows(260)))),
        (
            "a grid of cells of many heights",
            page(format!(
                r#"<grid cols="3" gap="4pt">{}</grid>"#,
                (0..300).map(|i| if i % 4 == 0 { format!("<text>Cell {i}. {LONG}</text>") } else { format!("<text>Cell {i}</text>") }).collect::<String>(),
            )),
        ),
        ("a grid that fits as many columns as it can", page(format!(r#"<grid col-width="120pt" gap="6pt">{}</grid>"#, lines(900)))),
        (
            "a long column that breaks before and after",
            page(format!(r#"<text>before</text><column paginate="break-before">{}</column><column paginate="break-after">{}</column><text>after</text>"#, lines(120), lines(90))),
        ),
        (
            "a column whose child is taller than a page",
            page(format!(r##"<column debug="true">{}<frame height="1200pt" background="#ff0000"/>{}</column>"##, lines(30), lines(150))),
        ),
        (
            "a column with notes",
            page(format!("<column>{}</column>", (0..140).map(|i| format!("<text>Line {i}<fn>Note {i}.</fn></text>")).collect::<String>())),
        ),
        ("a row of texts in each item of a column", page(format!("<column>{}</column>", (0..200).map(|i| format!(r#"<row><text width="80pt">Key {i}</text><text width="fill">{}</text></row>"#, if i % 6 == 0 { LONG } else { "value" })).collect::<String>()))),
        (
            "a long column under a header and over a footer",
            page(format!(
                r#"<region pin="top" page="each"><text>Header</text></region><region pin="bottom" page="each"><text>Footer</text></region><column>{}</column>"#,
                paragraphs(100),
            )),
        ),
        ("a long column in a section in columns", document(r#" columns="2""#, &format!("<column>{}</column>", paragraphs(160)))),
        ("a long table in a section in balanced columns", document(r#" columns="3" column-fill="balance""#, &format!(r#"<table cols="1fr 1fr">{}</table>"#, rows(500)))),
        (
            "two long columns side by side, as wide as their texts",
            page(format!(r#"<row gap="12pt"><column>{}</column><column>{}</column></row>"#, lines(300), lines(260))),
        ),
        ("two long columns side by side that fill the row", page(side_by_side(r#" gap="8pt""#, 240))),
        (
            "long columns side by side with padding, a background and a border",
            page(format!(
                r##"<row padding="10pt" gap="6pt" background="#eeeeee" border="1pt #333333"><column width="180pt" padding="4pt" gap="3pt" border="0.5pt #999999">{}</column><column width="fill" gap="4pt">{}{}</column></row>"##,
                paragraphs(70), lines(90), paragraphs(40),
            )),
        ),
        (
            "a sidebar that ends beside a long column",
            page(format!(r#"<row gap="10pt"><column width="140pt">{}</column><column width="fill">{}</column></row>"#, lines(20), paragraphs(120))),
        ),
        (
            "three columns side by side that end on different pages",
            page(format!(
                r#"<row gap="6pt"><column width="160pt">{}</column><column width="160pt">{}</column><column width="fill">{}</column></row>"#,
                lines(100), lines(260), paragraphs(70),
            )),
        ),
        ("long columns side by side after a title in a column", page(format!(r#"<column padding="6pt"><text>Title</text>{}<text>after</text></column>"#, side_by_side("", 200)))),
        ("long columns side by side first in a column", page(format!(r#"<column gap="4pt">{}{}</column>"#, side_by_side("", 180), lines(60)))),
        (
            "long columns side by side in a row beside a short one",
            page(format!(
                r#"<row gap="6pt"><column width="60pt"><text>Side</text></column><row width="fill" gap="4pt"><column width="fill">{}</column><column width="fill">{}</column></row></row>"#,
                lines(200), paragraphs(60),
            )),
        ),
        (
            "long columns side by side after a line of tags",
            page(format!(r#"<row gap="4pt">{}<column width="220pt">{}</column><column width="220pt">{}</column>{}</row>"#, tags(9), lines(240), lines(200), tags(12))),
        ),
        (
            "long columns side by side, one with a child taller than a page",
            page(format!(r##"<row debug="true"><column width="fill">{}<frame height="1200pt" background="#ff0000"/>{}</column><column width="fill">{}</column></row>"##, lines(30), lines(120), lines(220))),
        ),
        (
            "long columns side by side with notes",
            page(format!(
                r#"<row gap="8pt"><column width="fill">{}</column><column width="fill">{}</column></row>"#,
                (0..140).map(|i| format!("<text>Line {i}<fn>Note {i}.</fn></text>")).collect::<String>(),
                lines(200),
            )),
        ),
        (
            "long columns side by side under a header and over a footer",
            page(format!(
                r#"<region pin="top" page="each"><text>Header</text></region><region pin="bottom" page="each"><text>Footer</text></region>{}"#,
                side_by_side(r#" gap="6pt""#, 210),
            )),
        ),
        (
            "long columns side by side that break before and after",
            page(format!(
                "<text>before</text>{}{}<text>after</text>",
                side_by_side(r#" paginate="break-before""#, 150),
                side_by_side(r#" paginate="break-after""#, 120),
            )),
        ),
        ("long columns side by side in a section in columns", document(r#" columns="2""#, &side_by_side(r#" gap="4pt""#, 260))),
        ("a row of many tags", page(format!(r#"<row gap="4pt">{}</row>"#, tags(900)))),
        (
            "a row of tags with padding, a background and a border",
            page(format!(r##"<row padding="8pt" gap="6pt" background="#f4f4f4" border="1pt #333333">{}</row>"##, tags(700))),
        ),
        (
            "a row of tags with a fill child on some lines",
            page(format!(
                r#"<row gap="4pt">{}</row>"#,
                (0..800).map(|i| if i % 13 == 0 { format!(r#"<text width="fill">Fill {i}</text>"#) } else { format!(r#"<text width="70pt">Tag {i}</text>"#) }).collect::<String>(),
            )),
        ),
        ("a row of tags in a column", page(format!(r#"<column padding="6pt"><text>Tags</text><row gap="4pt">{}</row><text>after</text></column>"#, tags(800)))),
        ("a row of tags in a section in balanced columns", document(r#" columns="2" column-fill="balance""#, &format!(r#"<row gap="4pt">{}</row>"#, tags(900)))),
    ];
    // A heading that keeps with the table after it lands on each line near the foot of a page.
    for before in 44..50 {
        cases.push((
            "a heading that keeps with a long table",
            page(format!(r#"{}<text paginate="keep-next">Heading</text><table cols="1fr 1fr">{HEADER}{}</table>"#, lines(before), rows(200))),
        ));
    }
    // And with long columns side by side, and with a row of tags.
    for before in 44..50 {
        cases.push(("a heading that keeps with long columns side by side", page(format!(r#"{}<text paginate="keep-next">Heading</text>{}"#, lines(before), side_by_side("", 160)))));
        cases.push(("a heading that keeps with a row of tags", page(format!(r#"{}<text paginate="keep-next">Heading</text><row gap="4pt">{}</row>"#, lines(before), tags(700)))));
    }
    cases
}

#[test]
fn a_long_container_is_split_in_windows_as_it_was_split_whole() {
    for (name, xml) in cases() {
        rests::WINDOWS.with(|windows| windows.set(0));
        let pages = pages_of(&xml);
        assert!(pages.len() > 3, "{name}: {} pages", pages.len());
        assert!(rests::WINDOWS.with(Cell::get) > 0, "{name}: split in no window");
        assert_eq!(tree_of(&xml, false), tree_of(&xml, true), "{name}");
    }
}

#[test]
fn a_long_container_takes_work_that_grows_with_its_children() {
    // Each page laid out all of what was left: four times the children took about sixteen times the work.
    let kinds: [(&str, fn(usize) -> String); 6] = [
        ("a column", |n| format!("<column>{}</column>", lines(n))),
        ("a list", |n| format!("<list>{}</list>", "<li><text>item</text></li>".repeat(n))),
        ("a table", |n| format!(r#"<table cols="1fr 1fr">{}</table>"#, rows(n))),
        ("a grid", |n| format!(r#"<grid cols="2">{}</grid>"#, lines(n))),
        ("columns side by side", |n| format!("<row><column>{}</column><column>{}</column></row>", lines(n), lines(n))),
        ("a row of tags", |n| format!(r#"<row gap="4pt">{}</row>"#, tags(4 * n))),
    ];
    for (name, body) in kinds {
        let (short, long) = (work_of(&document("", &body(500))), work_of(&document("", &body(2000))));
        assert!((long as f64) < 5.0 * short as f64, "{name}: {long} steps for 2000 children, {short} for 500");
    }
}
