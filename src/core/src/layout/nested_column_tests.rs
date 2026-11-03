//! Links in columns, and columns that justify their children. A column keeps the tree it laid a link out in to
//! measure it, when the link comes out the same laid out with a height, and lays a child of a column that justifies
//! its children out once and moves its tree to its place (`same_with_a_height`), so nested links and boxes of a set
//! height around boxes of auto height are laid out in work that grows with their depth.

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

/// The layout of `body` as the engine hands it out, with every child a column could keep laid out again when
/// `lay_out_again`, as before columns kept links and moved trees.
fn tree_of(body: &str, lay_out_again: bool) -> Value {
    LAY_OUT_AGAIN.with(|again| again.set(lay_out_again));
    let pages = pages_of(body);
    LAY_OUT_AGAIN.with(|again| again.set(false));
    serde_json::from_str(&crate::render::pages_to_json(&pages, Value::Null, Value::Null)).unwrap()
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

/// `depth` levels of `a` and `b` in turn, `a` outermost, each with its closing tag.
fn in_turn(depth: usize, (a, a_end): (&str, &str), (b, b_end): (&str, &str)) -> String {
    chain(
        depth,
        |i| if i % 2 == 0 { a.into() } else { b.into() },
        |i| if i % 2 == 0 { a_end.into() } else { b_end.into() },
    )
}

/// Where `a` and `b` differ, by the path to it: a number by more than 0.001, anything else at all.
fn difference(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            ((x - y).abs() > 0.001).then(|| format!("{path}: {x} against {y}"))
        }
        (Value::Array(xs), Value::Array(ys)) if xs.len() == ys.len() => {
            xs.iter().zip(ys).enumerate().find_map(|(i, (x, y))| difference(x, y, &format!("{path}[{i}]")))
        }
        (Value::Object(xs), Value::Object(ys)) if xs.len() == ys.len() => xs.iter().find_map(|(key, x)| match ys.get(key) {
            Some(y) => difference(x, y, &format!("{path}.{key}")),
            None => Some(format!("{path}.{key}: missing")),
        }),
        _ => (a != b).then(|| format!("{path}: {a} against {b}")),
    }
}

const LINK: (&str, &str) = (r#"<link href="https://example.com">"#, "</link>");
const COLUMN: (&str, &str) = ("<column>", "</column>");
const FRAME: (&str, &str) = ("<frame>", "</frame>");
const LONG: &str = "lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet lorem ipsum dolor sit amet";

#[test]
fn a_column_keeps_a_link_as_it_was_laid_out_again() {
    let (link, end) = LINK;
    let cases = [
        ("links in links", chain(12, |_| link.into(), |_| end.into())),
        ("links and columns in turn", in_turn(12, LINK, COLUMN)),
        ("columns and links in turn", in_turn(12, COLUMN, LINK)),
        (
            "a link among other content, in a column with padding, a background and a border",
            format!(
                r##"<column padding="4pt" background="#eeeeee" border="1pt #333333" gap="3pt"><text>a</text>{link}<text>b</text><frame height="20pt" background="#ccccff"/><text>{LONG}</text>{end}<text>c</text></column>"##
            ),
        ),
        ("a link holding a child of fill height", format!(r##"<column>{link}<text>a</text><frame height="fill" background="#cccccc"/>{end}</column>"##)),
        ("a link holding a child of full height", format!(r##"<column>{link}<text>a</text><frame height="full" background="#cccccc"/>{end}</column>"##)),
        ("a link holding a vertical divider", format!(r#"<column>{link}<text>{LONG}</text><divider direction="vertical"/>{end}</column>"#)),
        (
            "a link holding a link holding a child of full height",
            format!(r##"<column>{link}<text>a</text>{link}<text>b</text><frame height="full" background="#cccccc"/>{end}{end}</column>"##),
        ),
        ("a link after a child of fill height", format!(r#"<column height="300pt"><frame height="fill"/>{link}<text>a</text>{end}</column>"#)),
        ("a link split across pages", format!("{link}{}{end}", (0..120).map(|i| format!("<text>line {i} {LONG}</text>")).collect::<String>())),
        ("links each holding a vertical divider", chain(12, |_| format!(r#"{link}<divider direction="vertical"/>"#), |_| end.into())),
        (
            "links each holding a child of full height beside a frame of a set height holding the next",
            chain(12, |_| format!(r##"{link}<frame height="full" background="#cccccc"/><frame height="20pt">"##), |_| format!("</frame>{end}")),
        ),
    ];
    for (name, body) in cases {
        assert_eq!(tree_of(&body, false), tree_of(&body, true), "{name}");
    }
}

#[test]
fn a_column_keeps_a_link_holding_a_child_of_fill_height_as_it_was_laid_out_again() {
    // The child takes what the others leave of the link's height, nothing either way, but the sums that find it
    // can come out a rounding apart.
    let (link, end) = LINK;
    let cases = [
        (
            "links each holding a child of fill height",
            chain(12, |_| format!(r##"{link}<frame height="fill" background="#cccccc"/><text>a</text>"##), |_| end.into()),
        ),
        (
            "a link holding children of fill height among others, in a column with a gap",
            format!(r##"<column gap="3pt">{link}<text>a</text><frame height="fill" background="#cccccc"/><text>{LONG}</text><frame height="fill"/>{end}<text>b</text></column>"##),
        ),
    ];
    for (name, body) in cases {
        if let Some(found) = difference(&tree_of(&body, false), &tree_of(&body, true), "") {
            panic!("{name}: {found}");
        }
    }
}

#[test]
fn a_column_moves_a_child_it_could_not_place_to_where_it_was_laid_out_again() {
    let (link, end) = LINK;
    let cases = [
        ("frames of a set height around frames", in_turn(12, (r#"<frame height="50pt">"#, "</frame>"), FRAME)),
        ("columns centered in a set height around columns", in_turn(12, (r#"<column height="50pt" justify="center">"#, "</column>"), COLUMN)),
        ("columns ended in a set height around columns", in_turn(12, (r#"<column height="50pt" justify="end">"#, "</column>"), COLUMN)),
        (
            "a frame of a set height with padding, a background and a border around a box",
            format!(
                r##"<frame height="120pt" padding="6pt" background="#eeeeee" border="1pt #333333"><column padding="4pt" background="#ddddff"><text>a</text><text>{LONG}</text></column></frame>"##
            ),
        ),
        (
            "children of a column of a set height spread between its ends",
            format!(r##"<column height="300pt" justify="between" gap="4pt"><text>a</text><row><text>b</text><divider direction="vertical"/></row><frame height="20pt" background="#cccccc"/><text>{LONG}</text></column>"##),
        ),
        (
            "a column ended in a set height holding a vertical divider and a child of fill height",
            format!(r##"<column height="300pt" justify="end"><text>a</text><divider direction="vertical"/><frame height="fill" background="#cccccc"/><text>{LONG}</text></column>"##),
        ),
        (
            "links in a column centered in a set height",
            format!(r##"<column height="300pt" justify="center"><text>a</text>{link}<text>b</text>{end}{link}<text>c</text><frame height="full" background="#cccccc"/>{end}</column>"##),
        ),
        ("a text centered in a frame of a set height", format!(r#"<frame height="200pt"><text>{LONG}</text></frame>"#)),
        (
            "boxes after a child of fill height",
            format!(r##"<column height="300pt" gap="2pt"><text>a</text><frame height="fill" background="#cccccc"/><column padding="4pt" background="#ddddff"><text>{LONG}</text></column><text>b</text></column>"##),
        ),
        (
            "boxes after a child of full height, centered",
            format!(r##"<column height="300pt" justify="center"><frame height="full" background="#cccccc"/>{link}<text>a</text>{end}<frame><text>{LONG}</text></frame></column>"##),
        ),
    ];
    for (name, body) in cases {
        if let Some(found) = difference(&tree_of(&body, false), &tree_of(&body, true), "") {
            panic!("{name}: {found}");
        }
    }
}

/// The kinds of chain a column used to lay out twice at each level, `depth` elements deep.
fn column_chains(depth: usize) -> [(&'static str, String); 9] {
    let (link, end) = LINK;
    [
        ("links in links", chain(depth, |_| link.into(), |_| end.into())),
        (
            // Beside the next link, a child of full height would be as tall as it, and the chain would double in
            // height at each level.
            "links each holding a child of full height beside a frame of a set height holding the next",
            chain(depth, |_| format!(r#"{link}<frame height="full"/><frame height="20pt">"#), |_| format!("</frame>{end}")),
        ),
        ("links each holding a child of fill height", chain(depth, |_| format!(r#"{link}<frame height="fill"/>"#), |_| end.into())),
        ("links each holding a vertical divider", chain(depth, |_| format!(r#"{link}<divider direction="vertical"/>"#), |_| end.into())),
        ("links and columns in turn", in_turn(depth, LINK, COLUMN)),
        ("columns and links in turn", in_turn(depth, COLUMN, LINK)),
        ("frames of a set height around frames", in_turn(depth, (r#"<frame height="50pt">"#, "</frame>"), FRAME)),
        ("columns centered in a set height around columns", in_turn(depth, (r#"<column height="50pt" justify="center">"#, "</column>"), COLUMN)),
        ("columns ended in a set height around columns", in_turn(depth, (r#"<column height="50pt" justify="end">"#, "</column>"), COLUMN)),
    ]
}

#[test]
fn nested_links_and_boxes_of_a_set_height_take_work_that_grows_with_their_depth() {
    // Laid out again at each level, 16 links in links would take 16 times the work of 12, and 4 times for the
    // chains that alternate.
    for ((name, shallow), (_, deep)) in column_chains(12).into_iter().zip(column_chains(16)) {
        let (shallow, deep) = (work_of(&shallow), work_of(&deep));
        assert!((deep as f64) < 1.5 * shallow as f64, "{name}: {deep} steps at 16 levels, {shallow} at 12");
    }
}
