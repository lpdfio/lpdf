//! Rulings: lines on every page of a section, for ruled or squared paper, or a grid to place things by.
//!
//! A ruling is defined once in the tokens (`<rulings>`) and checked when it is read (`read_lines`, `read_ruling`). A
//! section names it with `ruling`, and once the size of its pages and its margin are known, which they are when the
//! section is read, its lines are worked out (`draw`) and the section gets a layer under everything else that holds
//! them (`layer`). Every page of the section draws the same `Drawing`, which the PDF writes once, as a form XObject the
//! pages invoke, so a file doesn't grow with how dense the lines are (`pdf.rs`).

use std::collections::HashMap;
use std::rc::Rc;

use crate::canvas::{CanvasLayer, CanvasPrimitive};
use crate::parse::Ocg;
use crate::tokens::Tokens;

/// The narrowest a cell may be, in points, whether a step gives it or a count makes it: 1mm graph paper (2.83pt)
/// passes.
pub const MIN_CELL: f32 = 2.5;

/// The most parts a count may cut one direction into.
pub const MAX_COUNT: u32 = 100;

/// The color of lines that set none: the built-in token for lines that recede.
const DEFAULT_COLOR: &str = "line-muted";

/// The thickness of lines that set none, in points.
const DEFAULT_THICKNESS: f32 = 0.4;

/// How close, in points, a step has to come to the far edge to land on it. The page sizes are rounded to a hundredth
/// of a point, so a step of 5mm reaches A4's 595.28pt at 595.2756pt.
const LANDS: f32 = 0.01;

/// The space a ruling's lines fill and a count divides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// The whole page, up to its trim edge.
    #[default]
    Page,
    /// The area inside the margin of the section's layout or markdown.
    Margin,
}

/// How a set of lines cuts one direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cut {
    /// A line every this many points from the start edge: cells of a fixed size, and what is left at the far edge.
    Step(f32),
    /// This many equal parts, so lines land on both edges whatever the size of the space.
    Count(u32),
}

/// One `<lines>`, checked: the lines of one color and thickness.
#[derive(Debug, Clone, PartialEq)]
pub struct LineSet {
    /// The lines that run down the page, spaced from left to right; `None` for none.
    pub x: Option<Cut>,
    /// The lines that run across the page, spaced from top to bottom; `None` for none.
    pub y: Option<Cut>,
    /// A normalised color: hex, or `cmyk(…)`.
    pub color: String,
    pub thickness: f32,
}

/// A ruling of the tokens, checked: its sets in the order they are drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Ruling {
    pub origin: Origin,
    /// Whether the lines on the edges of the space are drawn.
    pub edges: bool,
    pub ocg: Option<Ocg>,
    /// At least one.
    pub sets: Vec<LineSet>,
}

/// The attributes of a `<lines>` as the document writes them, before they are checked.
#[derive(Debug, Default)]
pub struct LinesSpec<'a> {
    pub step: Option<&'a str>,
    pub step_x: Option<&'a str>,
    pub step_y: Option<&'a str>,
    pub count: Option<&'a str>,
    pub count_x: Option<&'a str>,
    pub count_y: Option<&'a str>,
    pub color: Option<&'a str>,
    pub thickness: Option<&'a str>,
}

impl<'a> LinesSpec<'a> {
    /// The attributes `get` finds by the schema's names.
    pub fn read(get: impl Fn(&str) -> Option<&'a str>) -> Self {
        LinesSpec {
            step: get("step"),
            step_x: get("step-x"),
            step_y: get("step-y"),
            count: get("count"),
            count_x: get("count-x"),
            count_y: get("count-y"),
            color: get("color"),
            thickness: get("thickness"),
        }
    }
}

/// Check a `<lines>` and look up its color and thickness.
pub fn read_lines(spec: &LinesSpec, tokens: &Tokens) -> Result<LineSet, String> {
    let x = cut(Axis::X, spec.step_x, spec.step, spec.count_x, spec.count)?;
    let y = cut(Axis::Y, spec.step_y, spec.step, spec.count_y, spec.count)?;
    if x.is_none() && y.is_none() {
        return Err("it draws no lines: give a step or a count, in x, in y or in both, such as step=\"5mm\"".into());
    }
    let color = tokens.resolve_color(spec.color.unwrap_or(DEFAULT_COLOR)).map_err(|e| format!("color: {e}"))?;
    let thickness = match spec.thickness {
        Some(value) => tokens.resolve_border_thickness(value).map_err(|e| format!("thickness: {e}"))?,
        None => DEFAULT_THICKNESS,
    };
    Ok(LineSet { x, y, color, thickness })
}

/// Check the name and the attributes of a `<ruling>`, and put its sets of lines with them. Each error names the ruling.
pub fn read_ruling(name: &str, origin: Option<&str>, edges: Option<&str>, ocg: Option<&str>, sets: Vec<LineSet>) -> Result<Ruling, String> {
    let fail = |reason: String| format!("ruling '{name}': {reason}");
    if !crate::tokens::is_token_name(name) {
        return Err(fail("the name must be lowercase letters, digits and hyphens, starting with a letter".into()));
    }
    let origin = match origin {
        None | Some("page") => Origin::Page,
        Some("margin") => Origin::Margin,
        Some(other) => return Err(fail(format!("invalid origin '{other}': expected page or margin"))),
    };
    let edges = match edges {
        None | Some("true") => true,
        Some("false") => false,
        Some(other) => return Err(fail(format!("invalid edges '{other}': expected true or false"))),
    };
    let ocg = ocg.map(Ocg::of).transpose().map_err(fail)?;
    if sets.is_empty() {
        return Err(fail("it has no <lines>: give one or more, such as <lines step=\"5mm\"/>".into()));
    }
    Ok(Ruling { origin, edges, ocg, sets })
}

/// The ruling a section names, or an error that lists the ones the tokens declare.
pub fn named<'r>(rulings: &'r HashMap<String, Ruling>, name: &str) -> Result<&'r Ruling, String> {
    rulings.get(name).ok_or_else(|| {
        let mut declared: Vec<&str> = rulings.keys().map(String::as_str).collect();
        declared.sort_unstable();
        match declared.as_slice() {
            [] => format!("unknown ruling '{name}': the tokens declare no rulings; add it to <rulings>"),
            [one] => format!("unknown ruling '{name}': the tokens declare one, {one}"),
            _ => format!("unknown ruling '{name}': the tokens declare {}", declared.join(", ")),
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn letter(self) -> char {
        match self {
            Axis::X => 'x',
            Axis::Y => 'y',
        }
    }

    /// The lines this direction spaces, for an error.
    fn lines(self) -> &'static str {
        match self {
            Axis::X => "the lines that run down the page",
            Axis::Y => "the lines that run across the page",
        }
    }

    /// The parts a count cuts this direction into, for an error.
    fn parts(self) -> &'static str {
        match self {
            Axis::X => "columns",
            Axis::Y => "rows",
        }
    }
}

/// How a set cuts the direction `axis`: by `step-x` or else `step`, or by `count-x` or else `count` (`-y` for y). A
/// step and a count in the same direction is an error that names both.
fn cut(axis: Axis, own_step: Option<&str>, step: Option<&str>, own_count: Option<&str>, count: Option<&str>) -> Result<Option<Cut>, String> {
    let letter = axis.letter();
    let step = own_step.map(|value| (format!("step-{letter}"), value)).or(step.map(|value| ("step".to_string(), value)));
    let count = own_count.map(|value| (format!("count-{letter}"), value)).or(count.map(|value| ("count".to_string(), value)));
    match (step, count) {
        (Some((step, _)), Some((count, _))) => Err(format!(
            "{step} and {count} both space {}: a direction is cut by a step or by a count, so remove one of them",
            axis.lines()
        )),
        (Some((name, value)), None) => parse_step(&name, value).map(|step| Some(Cut::Step(step))),
        (None, Some((name, value))) => parse_count(&name, value).map(|count| Some(Cut::Count(count))),
        (None, None) => Ok(None),
    }
}

/// A step: a length such as 5mm, of at least `MIN_CELL`.
fn parse_step(name: &str, value: &str) -> Result<f32, String> {
    let step = crate::parse::parse_measurement(value)
        .ok()
        .filter(|step| step.is_finite() && *step >= 0.0)
        .ok_or_else(|| format!("{name} '{value}' is not a length: give one such as 5mm"))?;
    if step < MIN_CELL {
        return Err(format!(
            "{name} '{value}' makes cells narrower than {MIN_CELL}pt, the least a ruling allows: give {MIN_CELL}pt or more, such as 1mm"
        ));
    }
    Ok(step)
}

/// A count: a whole number from 1 to `MAX_COUNT`.
fn parse_count(name: &str, value: &str) -> Result<u32, String> {
    match value.trim().parse::<u32>() {
        Ok(count) if (1..=MAX_COUNT).contains(&count) => Ok(count),
        Ok(count) if count > MAX_COUNT => Err(format!(
            "{name} '{value}' is more than {MAX_COUNT}: cut the space into {MAX_COUNT} parts or fewer, or give a step for finer lines, such as step=\"2.5mm\""
        )),
        _ => Err(format!("{name} '{value}' is not a count: give a whole number from 1 to {MAX_COUNT}")),
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

/// The lines of a ruling on a page of one size: what the PDF writes once and every page of a section that names the
/// ruling draws. Two sections of the same ruling on pages of the same size and margin draw equal ones, which the
/// PDF writes once too.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawing {
    /// The trimmed page, which the lines stop at: the PDF clips the drawing to it, so none reaches into a bleed.
    pub width: f32,
    pub height: f32,
    /// In the order they are drawn.
    pub sets: Vec<DrawnSet>,
}

/// One set of lines on the page, in the layout's coordinates: from the top left corner of the page, y down.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnSet {
    pub color: String,
    pub thickness: f32,
    /// The space the lines fill: left, top, right and bottom. A line down the page runs from its top to its bottom,
    /// and one across the page from its left to its right.
    pub area: [f32; 4],
    /// The lines that run down the page, by their x.
    pub down: Run,
    /// The lines that run across the page, by their y.
    pub across: Run,
}

/// Lines at even distances: `count` of them, the first at `first` and each next one `step` further on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Run {
    pub first: f32,
    pub step: f32,
    pub count: u32,
}

impl Run {
    const NONE: Run = Run { first: 0.0, step: 0.0, count: 0 };

    /// Where each line is.
    #[cfg(test)]
    pub fn positions(self) -> impl Iterator<Item = f32> {
        (0..self.count).map(move |k| self.first + k as f32 * self.step)
    }
}

/// The lines of `ruling` on a page `width` by `height` whose layout keeps `margin` (top, right, bottom, left) from its
/// edges. A count that cuts the space into parts narrower than `MIN_CELL` is an error, as only now is the size of the
/// space known.
pub fn draw(ruling: &Ruling, width: f32, height: f32, margin: [f32; 4]) -> Result<Drawing, String> {
    let [top, right, bottom, left] = match ruling.origin {
        Origin::Page => [0.0; 4],
        Origin::Margin => margin,
    };
    let area = [left, top, width - right, height - bottom];
    let space = match ruling.origin {
        Origin::Page => "the page",
        Origin::Margin => "the area inside the margin",
    };
    let sets = ruling
        .sets
        .iter()
        .map(|set| {
            Ok(DrawnSet {
                color: set.color.clone(),
                thickness: set.thickness,
                area,
                down: run(set.x, area[0], area[2] - area[0], ruling.edges, Axis::X, space)?,
                across: run(set.y, area[1], area[3] - area[1], ruling.edges, Axis::Y, space)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Drawing { width, height, sets })
}

/// The lines `cut` puts across a span `span` long that starts at `start`. With `edges` there is a line at the start and
/// at every step, and at the far end when a step lands on it, so a count of 12 draws 13 lines; without, the lines on
/// the two ends are left out, and a count of 12 draws 11.
fn run(cut: Option<Cut>, start: f32, span: f32, edges: bool, axis: Axis, space: &str) -> Result<Run, String> {
    let Some(cut) = cut else { return Ok(Run::NONE) };
    if span <= 0.0 {
        return Ok(Run::NONE);
    }
    let (step, steps, lands) = match cut {
        Cut::Step(step) => {
            let steps = ((span + LANDS) / step).floor() as u32;
            (step, steps, (steps as f32 * step - span).abs() <= LANDS)
        }
        Cut::Count(count) => {
            let step = span / count as f32;
            if step < MIN_CELL {
                let most = (span / MIN_CELL).floor() as u32;
                return Err(format!(
                    "a count of {count} cuts {space} into {} {:.2}pt wide, narrower than the {MIN_CELL}pt a ruling allows: \
                    give a count of {most} or less in {}",
                    axis.parts(), step, axis.letter()
                ));
            }
            (step, count, true)
        }
    };
    let (first, last) = match (edges, lands) {
        (true, _) => (0, steps),
        (false, true) => (1, steps.saturating_sub(1)),
        (false, false) => (1, steps),
    };
    let count = (last + 1).saturating_sub(first);
    Ok(Run { first: start + first as f32 * step, step, count })
}

/// The layer a section that names a ruling gets under everything else: the drawing, on every page, in the ruling's
/// optional content group. It has no `alt`, so a tagged PDF marks it as decoration.
pub fn layer(drawing: Drawing, ocg: Option<Ocg>) -> CanvasLayer {
    CanvasLayer {
        children: vec![CanvasPrimitive::Ruling(Rc::new(drawing))],
        page: None,
        opacity: None,
        transform: None,
        clip: None,
        data_if: None,
        data_if_not: None,
        alt: None,
        ocg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A4: (f32, f32) = (595.28, 841.89);

    fn lines(attrs: &[(&str, &str)]) -> Result<LineSet, String> {
        let get = |name: &str| attrs.iter().find(|(key, _)| *key == name).map(|(_, value)| *value);
        read_lines(&LinesSpec::read(get), &Tokens::default())
    }

    fn ruling(origin: Origin, edges: bool, sets: Vec<LineSet>) -> Ruling {
        Ruling { origin, edges, ocg: None, sets }
    }

    fn drawn(attrs: &[(&str, &str)], origin: Origin, edges: bool) -> DrawnSet {
        let ruling = ruling(origin, edges, vec![lines(attrs).unwrap()]);
        draw(&ruling, A4.0, A4.1, [36.0, 40.0, 50.0, 30.0]).unwrap().sets.remove(0)
    }

    #[test]
    fn a_set_takes_line_muted_at_four_tenths_of_a_point_by_default() {
        let set = lines(&[("step", "5mm")]).unwrap();
        assert_eq!((set.color.as_str(), set.thickness), ("#d4d4d4", 0.4));
    }

    #[test]
    fn step_cuts_both_directions_and_step_y_alone_only_the_lines_across() {
        let squared = lines(&[("step", "10pt")]).unwrap();
        assert_eq!((squared.x, squared.y), (Some(Cut::Step(10.0)), Some(Cut::Step(10.0))));
        let ruled = lines(&[("step-y", "8mm")]).unwrap();
        assert_eq!(ruled.x, None);
        assert!(matches!(ruled.y, Some(Cut::Step(step)) if (step - 22.677).abs() < 0.01), "{ruled:?}");
    }

    #[test]
    fn step_x_beats_step_in_its_own_direction() {
        let set = lines(&[("step", "10pt"), ("step-x", "20pt")]).unwrap();
        assert_eq!((set.x, set.y), (Some(Cut::Step(20.0)), Some(Cut::Step(10.0))));
        let set = lines(&[("count", "4"), ("count-y", "6")]).unwrap();
        assert_eq!((set.x, set.y), (Some(Cut::Count(4)), Some(Cut::Count(6))));
    }

    #[test]
    fn a_step_and_a_count_in_one_direction_is_an_error_that_names_both() {
        assert_eq!(
            lines(&[("step-x", "5mm"), ("count-x", "12")]).unwrap_err(),
            "step-x and count-x both space the lines that run down the page: a direction is cut by a step or by a \
             count, so remove one of them"
        );
        let error = lines(&[("step", "5mm"), ("count-y", "12")]).unwrap_err();
        assert!(error.starts_with("step and count-y both space the lines that run across the page"), "{error}");
        // A step in one direction and a count in the other is a set of each.
        assert!(lines(&[("step-y", "5mm"), ("count-x", "12")]).is_ok());
    }

    #[test]
    fn a_set_that_cuts_no_direction_is_an_error() {
        assert!(lines(&[("color", "#000")]).unwrap_err().starts_with("it draws no lines"));
    }

    #[test]
    fn a_step_narrower_than_the_floor_is_an_error_and_a_millimetre_is_not() {
        assert_eq!(
            lines(&[("step-y", "2pt")]).unwrap_err(),
            "step-y '2pt' makes cells narrower than 2.5pt, the least a ruling allows: give 2.5pt or more, such as 1mm"
        );
        assert!(lines(&[("step", "1mm")]).is_ok());
        assert!(lines(&[("step", "2.5pt")]).is_ok());
    }

    #[test]
    fn a_step_must_be_a_length_and_a_count_a_whole_number_up_to_a_hundred() {
        assert!(lines(&[("step", "m")]).unwrap_err().starts_with("step 'm' is not a length"));
        assert_eq!(
            lines(&[("count-x", "150")]).unwrap_err(),
            "count-x '150' is more than 100: cut the space into 100 parts or fewer, or give a step for finer lines, such \
             as step=\"2.5mm\""
        );
        assert!(lines(&[("count", "0")]).unwrap_err().starts_with("count '0' is not a count"));
        assert!(lines(&[("count", "100")]).is_ok());
    }

    #[test]
    fn color_and_thickness_take_tokens() {
        let set = lines(&[("step", "5mm"), ("color", "line"), ("thickness", "s")]).unwrap();
        let tokens = Tokens::default();
        assert_eq!(set.color, tokens.resolve_color("line").unwrap());
        assert_eq!(set.thickness, tokens.resolve_border_thickness("s").unwrap());
        assert!(lines(&[("step", "5mm"), ("color", "paper")]).unwrap_err().starts_with("color: unknown color token"));
    }

    #[test]
    fn a_ruling_without_lines_or_with_an_unknown_origin_or_a_bad_name_is_an_error_naming_it() {
        let set = || vec![lines(&[("step", "5mm")]).unwrap()];
        assert!(read_ruling("graph", None, None, None, Vec::new()).unwrap_err().starts_with("ruling 'graph': it has no <lines>"));
        assert!(read_ruling("graph", Some("bleed"), None, None, set()).unwrap_err().starts_with("ruling 'graph': invalid origin 'bleed'"));
        assert!(read_ruling("Graph", None, None, None, set()).unwrap_err().starts_with("ruling 'Graph': the name must be"));
        let read = read_ruling("graph", Some("margin"), Some("false"), Some("screen"), set()).unwrap();
        assert_eq!((read.origin, read.edges, read.ocg), (Origin::Margin, false, Some(Ocg::Screen)));
        let default = read_ruling("graph", None, None, None, set()).unwrap();
        assert_eq!((default.origin, default.edges, default.ocg), (Origin::Page, true, None));
    }

    #[test]
    fn an_unknown_name_lists_the_declared_ones() {
        let mut rulings = HashMap::new();
        assert_eq!(
            named(&rulings, "graph").unwrap_err(),
            "unknown ruling 'graph': the tokens declare no rulings; add it to <rulings>"
        );
        let one = ruling(Origin::Page, true, vec![lines(&[("step", "5mm")]).unwrap()]);
        rulings.insert("notes".to_string(), one.clone());
        assert_eq!(named(&rulings, "graph").unwrap_err(), "unknown ruling 'graph': the tokens declare one, notes");
        rulings.insert("dots".to_string(), one);
        assert_eq!(named(&rulings, "grph").unwrap_err(), "unknown ruling 'grph': the tokens declare dots, notes");
    }

    #[test]
    fn a_count_draws_a_line_on_both_edges_and_one_between_each_part() {
        let set = drawn(&[("count-x", "12")], Origin::Page, true);
        assert_eq!(set.down.count, 13);
        assert_eq!(set.down.first, 0.0);
        let last = set.down.positions().last().unwrap();
        assert!((last - A4.0).abs() < 0.01, "the last line is on the far edge: {last}");
        assert_eq!(set.across.count, 0, "count-x alone draws no lines across");
    }

    #[test]
    fn without_edges_a_count_of_twelve_draws_eleven() {
        let set = drawn(&[("count", "12")], Origin::Page, false);
        assert_eq!((set.down.count, set.across.count), (11, 11));
        assert!((set.down.first - A4.0 / 12.0).abs() < 0.001);
    }

    #[test]
    fn a_step_starts_at_the_start_edge_and_leaves_the_rest_at_the_far_edge() {
        // 841.89 / 22.68 is 37.1: lines at 0 and 37 steps after it.
        let set = drawn(&[("step-y", "8mm")], Origin::Page, true);
        assert_eq!((set.across.first, set.across.count), (0.0, 38));
        let set = drawn(&[("step-y", "8mm")], Origin::Page, false);
        assert_eq!(set.across.count, 37, "none on the top edge, and the far edge has none to leave out");
        assert!((set.across.first - 22.677).abs() < 0.01);
    }

    #[test]
    fn a_step_that_lands_on_the_far_edge_draws_a_line_there_only_with_edges() {
        // 42 steps of 5mm make 210mm, A4's width.
        assert_eq!(drawn(&[("step-x", "5mm")], Origin::Page, true).down.count, 43);
        assert_eq!(drawn(&[("step-x", "5mm")], Origin::Page, false).down.count, 41);
    }

    #[test]
    fn origin_margin_fills_the_area_inside_the_margin() {
        let set = drawn(&[("count", "4")], Origin::Margin, true);
        assert_eq!(set.area, [30.0, 36.0, A4.0 - 40.0, A4.1 - 50.0]);
        assert_eq!(set.down.first, 30.0);
        assert!((set.down.step - (A4.0 - 70.0) / 4.0).abs() < 0.001);
        assert_eq!(set.across.first, 36.0);
        let page = drawn(&[("count", "4")], Origin::Page, true);
        assert_eq!(page.area, [0.0, 0.0, A4.0, A4.1]);
    }

    #[test]
    fn a_count_keeps_its_parts_and_a_step_its_size_on_a_smaller_page() {
        let ruling = ruling(Origin::Page, true, vec![lines(&[("count-x", "12"), ("step-y", "5mm")]).unwrap()]);
        let a4 = draw(&ruling, A4.0, A4.1, [0.0; 4]).unwrap();
        let a5 = draw(&ruling, 419.53, 595.28, [0.0; 4]).unwrap();
        assert_eq!((a4.sets[0].down.count, a5.sets[0].down.count), (13, 13));
        assert!(a5.sets[0].down.step < a4.sets[0].down.step);
        assert_eq!(a4.sets[0].across.step, a5.sets[0].across.step);
        assert!(a5.sets[0].across.count < a4.sets[0].across.count);
    }

    #[test]
    fn a_count_that_makes_parts_narrower_than_the_floor_is_an_error_with_the_count_that_fits() {
        let ruling = ruling(Origin::Margin, true, vec![lines(&[("count-x", "100")]).unwrap()]);
        // 200pt inside the margin of a page 260pt wide: parts of 2pt, and 80 parts of 2.5pt fit.
        assert_eq!(
            draw(&ruling, 260.0, 400.0, [30.0; 4]).unwrap_err(),
            "a count of 100 cuts the area inside the margin into columns 2.00pt wide, narrower than the 2.5pt a ruling \
             allows: give a count of 80 or less in x"
        );
    }

    #[test]
    fn two_sets_keep_their_order_color_and_thickness() {
        let fine = lines(&[("step", "5mm"), ("color", "#dde6f0"), ("thickness", "0.3pt")]).unwrap();
        let bold = lines(&[("step", "25mm"), ("color", "#b5c4d6"), ("thickness", "0.6pt")]).unwrap();
        let drawing = draw(&ruling(Origin::Page, true, vec![fine, bold]), A4.0, A4.1, [0.0; 4]).unwrap();
        let colors: Vec<(&str, f32)> = drawing.sets.iter().map(|s| (s.color.as_str(), s.thickness)).collect();
        assert_eq!(colors, [("#dde6f0", 0.3), ("#b5c4d6", 0.6)]);
        assert_eq!((drawing.width, drawing.height), A4);
    }
}
