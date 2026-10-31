//! Lists. A `<list>` and its `<li>` items stay in the tree until the data is in, since `data-source` on an item
//! decides how many there are and so how they are numbered. `lower` then turns each list into what the layout
//! knows: a column of items, each item a row of its marker and a column that holds its content. A row splits
//! across pages through the line the page ends in, so a long item breaks mid-paragraph with its marker on the
//! first part and the lines after it under the words, with nothing in the layout that knows of lists.

use crate::parse::{
    Align, Check, Detail, ListBullet, ListFormat, ListSpec, Node, NodeKind, Shape, ShapeKind, TextAlign, TextRun,
    DEFAULT_LINE_HEIGHT, LIST_ITEM_GAP_UNSET,
};

/// The space between a marker and the words, as a factor of the font size. The marker column ends this far before
/// the words start, so the words begin exactly `indent` from the left edge of the list.
const MARKER_GAP: f32 = 0.4;

/// How far above the baseline of the first line of an item the middle of its bullet sits, as a factor of the font
/// size: where the bullet of a font stands, and a little lower for a dash, which stands where an en dash does. A
/// line's baseline is a font size below its top.
const BULLET_RAISE: f32 = 0.34;
const DASH_RAISE: f32 = 0.29;

/// Shapes of a bullet, as factors of the font size: the radius of a disc, the stroke of a circle, the side of a
/// square, and the length and thickness of a dash.
const DISC_RADIUS: f32 = 0.17;
const CIRCLE_STROKE: f32 = 0.07;
const SQUARE_SIDE: f32 = 0.3;
const DASH_LENGTH: f32 = 0.5;
const DASH_THICKNESS: f32 = 0.08;

/// Shapes of a checkbox, as factors of the font size: its side, the stroke of its outline, the radius of its corners,
/// and the space it leaves before the words, beyond what a bullet does, so that the box stands a little left of where
/// a bullet would.
const CHECKBOX_SIDE: f32 = 0.95;
const CHECKBOX_STROKE: f32 = 0.09;
const CHECKBOX_RADIUS: f32 = 0.2;
const CHECKBOX_GAP: f32 = 0.3;

/// Turn every list in `nodes` into columns and rows, wherever it stands in the tree.
pub(crate) fn lower_all(nodes: &mut [Node]) {
    for node in nodes {
        lower(node, 0);
    }
}

/// Turn the lists in `node` and under it into columns and rows. `bullet_depth` is how many bullet lists `node` is
/// inside, which chooses the shape of a bullet list that names none.
fn lower(node: &mut Node, bullet_depth: usize) {
    if node.kind == NodeKind::List {
        lower_list(node, bullet_depth);
        return;
    }
    for child in &mut node.children {
        lower(child, bullet_depth);
    }
}

/// Make `list` the column of its items. A list that has no `ListSpec` is a bullet list with the default indent.
fn lower_list(list: &mut Node, bullet_depth: usize) {
    let spec = match list.detail.take().map(|detail| *detail) {
        Some(Detail::List(spec)) => spec,
        _                        => default_spec(),
    };
    let marks = Marks {
        font:       list.font.clone(),
        font_size:  list.font_size,
        color:      list.text_color.take().unwrap_or_else(|| "#1a1a1a".to_string()),
        line_height: DEFAULT_LINE_HEIGHT,
    };
    // A list inside an item sits in its content, so a bullet list one level down is one level deeper.
    let inner_depth = bullet_depth + usize::from(!spec.numbered);

    let mut number = spec.start;
    for item in &mut list.children {
        lower_item(item, &spec, &marks, list.gap, number, bullet_depth, inner_depth);
        number = number.saturating_add(1);
    }
    list.kind = NodeKind::Column;
}

fn default_spec() -> ListSpec {
    ListSpec {
        numbered: false,
        format:   ListFormat::Decimal,
        start:    1,
        prefix:   String::new(),
        suffix:   ".".to_string(),
        bullet:   None,
        indent:   crate::parse::DEFAULT_LIST_INDENT,
    }
}

/// The font, size and color a marker takes when an item's first child is not a text.
struct Marks {
    font:        String,
    font_size:   f32,
    color:       String,
    line_height: f32,
}

/// Make `item` a row of its marker and a column that holds what it held. The item's own box, its padding,
/// background and border, goes around both. Its `gap` goes between the children of the column, and an item that
/// sets none spaces them as the list spaces its items: `list_gap`.
fn lower_item(
    item:         &mut Node,
    spec:         &ListSpec,
    marks:        &Marks,
    list_gap:     f32,
    number:       i64,
    bullet_depth: usize,
    inner_depth:  usize,
) {
    // The marker takes the look of the first line of the item: the text it starts with, or the list's own.
    let look = match item.children.first() {
        Some(first) if first.kind == NodeKind::Text => Marks {
            font:        first.font.clone(),
            font_size:   first.font_size,
            color:       first.text_color.clone().unwrap_or_else(|| marks.color.clone()),
            line_height: first.line_height,
        },
        _ => Marks { font: marks.font.clone(), font_size: marks.font_size, color: marks.color.clone(), line_height: marks.line_height },
    };

    let gap = MARKER_GAP * look.font_size;
    let check = match item.detail.take().map(|detail| *detail) {
        Some(Detail::Check(check)) => Some(check),
        _                          => None,
    };
    let marker_width = (spec.indent - gap).max(0.0);
    let marker = if let Some(check) = check {
        checkbox_marker(&check, marker_width, &look)
    } else if spec.numbered {
        number_marker(&label(spec, number), marker_width, &look)
    } else {
        bullet_marker(spec.bullet.unwrap_or_else(|| bullet_by_depth(bullet_depth)), marker_width, &look)
    };

    let mut content = Node::layout_default();
    content.kind       = NodeKind::Column;
    content.gap        = if item.gap == LIST_ITEM_GAP_UNSET { list_gap } else { item.gap };
    content.width_fill = true;
    content.children   = std::mem::take(&mut item.children);
    for child in &mut content.children {
        lower(child, inner_depth);
    }

    item.kind     = NodeKind::Row;
    item.gap      = gap;
    item.align    = Align::Start;
    item.children = vec![marker, content];
}

/// The shape of a bullet list that names none: a disc, a circle inside it, a square inside that, then the three
/// again.
fn bullet_by_depth(depth: usize) -> ListBullet {
    match depth % 3 {
        0 => ListBullet::Disc,
        1 => ListBullet::Circle,
        _ => ListBullet::Square,
    }
}

/// The marker of a number list: the prefix, the number and the suffix.
fn label(spec: &ListSpec, number: i64) -> String {
    format!("{}{}{}", spec.prefix, format_number(spec.format, number), spec.suffix)
}

/// `number` written in `format`. A letter or a numeral can't write a number below 1, and a numeral not one above
/// 3999, so those are written in decimal.
fn format_number(format: ListFormat, number: i64) -> String {
    match format {
        ListFormat::LowerAlpha if number >= 1 => alpha(number),
        ListFormat::UpperAlpha if number >= 1 => alpha(number).to_uppercase(),
        ListFormat::LowerRoman if (1..=3999).contains(&number) => roman(number),
        ListFormat::UpperRoman if (1..=3999).contains(&number) => roman(number).to_uppercase(),
        _ => number.to_string(),
    }
}

/// 1 is a, 26 is z, 27 is aa, 28 is ab: the letters count the way a spreadsheet's columns do.
fn alpha(number: i64) -> String {
    let mut letters = Vec::new();
    let mut rest = number;
    while rest > 0 {
        rest -= 1;
        letters.push(b'a' + (rest % 26) as u8);
        rest /= 26;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

/// A number from 1 to 3999 in lower-case roman numerals.
fn roman(number: i64) -> String {
    const NUMERALS: [(i64, &str); 13] = [
        (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"),
        (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i"),
    ];
    let mut rest = number;
    let mut out = String::new();
    for (value, numeral) in NUMERALS {
        while rest >= value {
            out.push_str(numeral);
            rest -= value;
        }
    }
    out
}

/// A text of one line, `width` wide with its words at the right, so that `9.` and `10.` end together.
fn number_marker(words: &str, width: f32, look: &Marks) -> Node {
    let mut text = Node::layout_default();
    text.kind            = NodeKind::Text;
    text.font            = look.font.clone();
    text.font_size       = look.font_size;
    text.text_color      = Some(look.color.clone());
    text.line_height     = look.line_height;
    text.text_align      = TextAlign::Right;
    text.width_constraint = Some(width);
    text.text_runs       = vec![TextRun {
        text: words.to_string(),
        leading_space: false,
        font: None, color: None, href: None,
        underline: false, strike: false, bold: false, background: None, footnote: None,
    }];
    text
}

/// A column `width` wide that holds a shape at its right, as far down as the middle of the item's first line.
fn bullet_marker(bullet: ListBullet, width: f32, look: &Marks) -> Node {
    let size = look.font_size;
    let (kind, box_w, box_h, fill, stroke, stroke_width) = match bullet {
        ListBullet::Disc => {
            let side = 2.0 * DISC_RADIUS * size;
            (ShapeKind::Circle, side, side, Some(look.color.clone()), None, None)
        }
        ListBullet::Circle => {
            let stroke = CIRCLE_STROKE * size;
            // The stroke is centered on the edge, so the circle is drawn smaller by half of it to end where a disc does.
            let side = 2.0 * DISC_RADIUS * size - stroke;
            (ShapeKind::Circle, side, side, None, Some(look.color.clone()), Some(stroke))
        }
        ListBullet::Square => {
            let side = SQUARE_SIDE * size;
            (ShapeKind::Rect, side, side, Some(look.color.clone()), None, None)
        }
        ListBullet::Dash => (ShapeKind::Rect, DASH_LENGTH * size, DASH_THICKNESS * size, Some(look.color.clone()), None, None),
    };

    let mut shape = Node::layout_default();
    shape.kind                  = NodeKind::Shape;
    shape.width_constraint      = Some(box_w);
    shape.img_height_constraint = Some(box_h);
    shape.detail = Some(Box::new(Detail::Shape(Shape { kind, fill, stroke, stroke_width, stroke_dash: None, tick: None })));

    let raise = if bullet == ListBullet::Dash { DASH_RAISE } else { BULLET_RAISE };
    let mut marker = Node::layout_default();
    marker.kind             = NodeKind::Column;
    marker.width_constraint = Some(width);
    marker.align            = Align::End;
    marker.padding          = [(size * (1.0 - raise) - box_h / 2.0).max(0.0), 0.0, 0.0, 0.0];
    marker.children         = vec![shape];
    marker
}

/// A column `width` wide that holds a checkbox at its right, as far down as the middle of the item's first line: an
/// outline in the `line` color, and a tick in the color of the item's words when it is checked.
fn checkbox_marker(check: &Check, width: f32, look: &Marks) -> Node {
    let size = look.font_size;
    let side = CHECKBOX_SIDE * size;

    let mut shape = Node::layout_default();
    shape.kind                  = NodeKind::Shape;
    shape.width_constraint      = Some(side);
    shape.img_height_constraint = Some(side);
    shape.radius                = CHECKBOX_RADIUS * side;
    shape.detail = Some(Box::new(Detail::Shape(Shape {
        kind:         ShapeKind::Checkbox,
        fill:         None,
        stroke:       Some(check.line.clone()),
        stroke_width: Some(CHECKBOX_STROKE * size),
        stroke_dash:  None,
        tick:         check.checked.then(|| look.color.clone()),
    })));

    let mut marker = Node::layout_default();
    marker.kind             = NodeKind::Column;
    marker.width_constraint = Some(width);
    marker.align            = Align::End;
    marker.padding          = [(size * (1.0 - BULLET_RAISE) - side / 2.0).max(0.0), CHECKBOX_GAP * size, 0.0, 0.0];
    marker.children         = vec![shape];
    marker
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    /// The first node of the layout of a document that holds `body`, once the data is in and the lists are lowered.
    fn first_node(body: &str, data: Option<&str>) -> Node {
        let xml = format!(
            r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
        );
        let mut doc = parse(&xml).unwrap();
        if let Some(data) = data {
            crate::data::apply(&mut doc, data).unwrap();
        }
        doc.section_layouts().remove(0).children.remove(0)
    }

    /// The words of a marker: a number list's text, and `bullet` for a shape.
    fn marker_words(item: &Node) -> String {
        let marker = &item.children[0];
        match marker.kind {
            NodeKind::Text => marker.text_runs.iter().map(|run| run.text.as_str()).collect(),
            _              => "bullet".to_string(),
        }
    }

    #[test]
    fn a_list_is_a_column_of_rows_that_hold_a_marker_and_a_column_of_content() {
        let list = first_node(r#"<list type="number"><li><text>A</text></li><li><text>B</text></li></list>"#, None);
        assert_eq!(list.kind, NodeKind::Column);
        assert_eq!(list.children.len(), 2);
        for item in &list.children {
            assert_eq!(item.kind, NodeKind::Row);
            assert_eq!(item.children.len(), 2);
            assert_eq!(item.children[0].kind, NodeKind::Text);
            let content = &item.children[1];
            assert_eq!((content.kind.clone(), content.width_fill), (NodeKind::Column, true));
            assert_eq!(content.children[0].kind, NodeKind::Text);
        }
    }

    #[test]
    fn what_is_in_an_item_is_spaced_like_the_items_unless_the_item_says_otherwise() {
        let list = first_node(
            r#"<list gap="10pt">
                <li><text>A</text><text>B</text></li>
                <li gap="0pt"><text>C</text><text>D</text></li>
                <li gap="4pt"><text>E</text><text>F</text></li>
            </list>"#,
            None,
        );
        assert_eq!(list.gap, 10.0);
        let gaps: Vec<f32> = list.children.iter().map(|item| item.children[1].gap).collect();
        assert_eq!(gaps, [10.0, 0.0, 4.0]);
    }

    #[test]
    fn the_words_of_an_item_start_one_indent_from_the_left_edge() {
        let list = first_node(r#"<list type="number" indent="30pt"><li><text>A</text></li></list>"#, None);
        let item = &list.children[0];
        let marker = &item.children[0];
        // 0.4 of the 11pt font size stands between the marker and the words.
        assert_eq!(marker.width_constraint.map(|w| w + item.gap), Some(30.0));
    }

    #[test]
    fn an_item_that_data_source_repeats_is_numbered_once_the_data_is_in() {
        let list = first_node(
            r#"<list type="number"><li data-source="clauses"><text data-value="text"/></li></list>"#,
            Some(r#"{"clauses":[{"text":"a"},{"text":"b"},{"text":"c"}]}"#),
        );
        let words: Vec<String> = list.children.iter().map(marker_words).collect();
        assert_eq!(words, ["1.", "2.", "3."]);
    }

    #[test]
    fn an_item_that_data_if_leaves_out_takes_no_number() {
        let list = first_node(
            r#"<list type="number"><li><text>A</text></li><li data-if="later"><text>B</text></li><li><text>C</text></li></list>"#,
            Some(r#"{"later":false}"#),
        );
        let words: Vec<String> = list.children.iter().map(marker_words).collect();
        assert_eq!(words, ["1.", "2."]);
    }

    #[test]
    fn numbers_start_at_start_and_a_nested_list_starts_over() {
        let list = first_node(
            r#"<list type="number" start="3" format="lower-alpha" prefix="(" suffix=")">
                <li><text>A</text><list type="number"><li><text>B</text></li></list></li>
                <li><text>C</text></li>
            </list>"#,
            None,
        );
        let words: Vec<String> = list.children.iter().map(marker_words).collect();
        assert_eq!(words, ["(c)", "(d)"]);
        let nested = &list.children[0].children[1].children[1];
        assert_eq!(nested.kind, NodeKind::Column);
        assert_eq!(marker_words(&nested.children[0]), "1.");
    }

    #[test]
    fn a_bullet_list_is_marked_with_a_disc_a_circle_and_a_square_by_how_deep_it_is() {
        let list = first_node(
            r#"<list><li><text>A</text>
                <list><li><text>B</text>
                    <list><li><text>C</text>
                        <list><li><text>D</text></li></list>
                    </li></list>
                </li></list>
            </li></list>"#,
            None,
        );
        let shape_of = |list: &Node| {
            let shape = list.children[0].children[0].children[0].shape().cloned().expect("a shape marker");
            (shape.kind, shape.fill.is_some(), shape.stroke.is_some())
        };
        let level = |list: &Node| list.children[0].children[1].children[1].clone();
        let (second, third) = (level(&list), level(&level(&list)));
        let fourth = level(&third);
        assert_eq!(shape_of(&list),   (ShapeKind::Circle, true,  false));
        assert_eq!(shape_of(&second), (ShapeKind::Circle, false, true));
        assert_eq!(shape_of(&third),  (ShapeKind::Rect,   true,  false));
        assert_eq!(shape_of(&fourth), (ShapeKind::Circle, true,  false));
    }

    #[test]
    fn a_dash_is_a_wide_thin_rectangle() {
        let list = first_node(r#"<list bullet="dash"><li><text>A</text></li></list>"#, None);
        let marker = &list.children[0].children[0];
        let dash = &marker.children[0];
        assert_eq!(dash.shape().map(|shape| shape.kind), Some(ShapeKind::Rect));
        assert!(dash.width_constraint.unwrap() > 3.0 * dash.img_height_constraint.unwrap());
    }

    #[test]
    fn the_marker_takes_the_size_and_color_of_the_first_text_of_its_item() {
        let list = first_node(
            r##"<list type="number" color="#336699"><li><text font-size="20pt" color="#ff0000">A</text></li><li><column><text>B</text></column></li></list>"##,
            None,
        );
        let first = &list.children[0].children[0];
        assert_eq!((first.font_size, first.text_color.as_deref()), (20.0, Some("#ff0000")));
        // An item that does not start with a text takes the list's own look.
        let second = &list.children[1].children[0];
        assert_eq!((second.font_size, second.text_color.as_deref()), (11.0, Some("#336699")));
    }

    /// The shape in the marker of an item, with the box's tick: `Some(true)` for a checked checkbox, `Some(false)` for
    /// an open one, and none for a bullet.
    fn tick_of(item: &Node) -> Option<bool> {
        let shape = item.children[0].children.first()?.shape()?;
        (shape.kind == ShapeKind::Checkbox).then(|| shape.tick.is_some())
    }

    #[test]
    fn a_checked_item_has_a_checkbox_for_a_marker_that_is_ticked_when_it_is_true() {
        let list = first_node(
            r#"<list><li checked="true"><text>A</text></li><li checked="false"><text>B</text></li><li><text>C</text></li></list>"#,
            None,
        );
        let ticks: Vec<Option<bool>> = list.children.iter().map(tick_of).collect();
        assert_eq!(ticks, [Some(true), Some(false), None]);
    }

    #[test]
    fn a_checkbox_takes_the_outline_of_the_line_color_and_the_tick_of_the_words_of_its_item() {
        let list = first_node(r##"<list><li checked="true"><text color="#cc0000" font-size="20pt">A</text></li></list>"##, None);
        let marker = &list.children[0].children[0];
        let shape = marker.children[0].shape().expect("a shape");
        assert_eq!((shape.stroke.as_deref(), shape.tick.as_deref(), shape.fill.as_ref()), (Some("#737373"), Some("#cc0000"), None));
        // A box of a little less than the font size, which the font size scales.
        assert_eq!(marker.children[0].width_constraint, Some(CHECKBOX_SIDE * 20.0));
    }

    #[test]
    fn the_words_of_a_checked_item_start_where_those_of_a_bullet_do() {
        let list = first_node(r#"<list indent="30pt"><li checked="false"><text>A</text></li><li><text>B</text></li></list>"#, None);
        let widths: Vec<f32> = list.children.iter().map(|item| item.children[0].width_constraint.unwrap() + item.gap).collect();
        assert_eq!(widths, [30.0, 30.0]);
    }

    #[test]
    fn a_checked_item_of_a_number_list_takes_its_number_and_the_next_item_goes_on() {
        let list = first_node(r#"<list type="number"><li checked="true"><text>A</text></li><li><text>B</text></li></list>"#, None);
        assert_eq!(tick_of(&list.children[0]), Some(true));
        assert_eq!(marker_words(&list.children[1]), "2.");
    }

    #[test]
    fn checked_is_true_or_false() {
        let xml = r#"<lpdf version="1"><document size="a4"><section><layout><list><li checked="yes"><text>A</text></li></list></layout></section></document></lpdf>"#;
        let err = parse(xml).unwrap_err();
        assert!(err.contains(r#"<li> at line 1, col 62: checked must be true or false, not "yes""#), "{err}");
    }

    #[test]
    fn letters_count_like_a_spreadsheets_columns() {
        let letters: Vec<String> = [1, 2, 26, 27, 28, 52, 53, 702, 703].iter().map(|n| alpha(*n)).collect();
        assert_eq!(letters, ["a", "b", "z", "aa", "ab", "az", "ba", "zz", "aaa"]);
    }

    #[test]
    fn numerals_follow_the_subtractive_rule() {
        let numerals: Vec<String> = [1, 4, 9, 14, 40, 90, 400, 1994, 2024, 3999].iter().map(|n| roman(*n)).collect();
        assert_eq!(numerals, ["i", "iv", "ix", "xiv", "xl", "xc", "cd", "mcmxciv", "mmxxiv", "mmmcmxcix"]);
    }

    #[test]
    fn a_number_a_format_cannot_write_is_written_in_decimal() {
        assert_eq!(format_number(ListFormat::LowerAlpha, 0), "0");
        assert_eq!(format_number(ListFormat::UpperAlpha, -3), "-3");
        assert_eq!(format_number(ListFormat::LowerRoman, 4000), "4000");
        assert_eq!(format_number(ListFormat::UpperRoman, 0), "0");
        assert_eq!(format_number(ListFormat::UpperRoman, 14), "XIV");
        assert_eq!(format_number(ListFormat::UpperAlpha, 28), "AB");
        assert_eq!(format_number(ListFormat::Decimal, 7), "7");
    }

    #[test]
    fn bullets_go_disc_circle_square_by_depth_and_then_again() {
        let by_depth: Vec<ListBullet> = (0..4).map(bullet_by_depth).collect();
        assert_eq!(by_depth, [ListBullet::Disc, ListBullet::Circle, ListBullet::Square, ListBullet::Disc]);
    }
}
