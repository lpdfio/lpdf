//! The `<chart>` of the layout: what it says (`ChartSpec`), its series and categories once they are read (`ChartData`),
//! and how it is drawn (`draw`).
//!
//! A chart is written like a table: `<series>` are its columns, each `<category>` a row and each `<value>` a cell. They
//! stay nodes of the tree until the data is bound, so that `data-source` repeats them as it repeats any element, and
//! are then read into a `ChartData` (`lower`), which the layout draws: a static chart when it is parsed, a bound one
//! right after its data is bound, and one with data attributes rendered without data when it is laid out. The engine
//! draws the chart as the rects, lines and text it already draws, by fixed rules for the axis, the steps, the labels
//! and the legend.

use crate::layout::{text_width, ELLIPSIS};
use crate::parse::{Attrs, Detail, Node, NodeKind};
use crate::render::{RenderBars, RenderBox, RenderCanvasRect, RenderLine, RenderNode, RenderText};
use crate::tokens::Tokens;

/// The most categories a chart holds, counted after the data is bound.
pub(crate) const MAX_CATEGORIES: usize = 10_000;
/// The most series a chart holds, counted after the data is bound.
pub(crate) const MAX_SERIES: usize = 100;

/// The fixed palette the series take their colors from, in order, when they set none. It starts over past the eighth.
pub(crate) const PALETTE: [&str; 8] = [
    "#1763cf", "#e8590c", "#2f9e44", "#c2255c", "#6741d9", "#0c8599", "#f59f00", "#868e96",
];

/// The most lines a category label wraps onto before it ends in an ellipsis.
const LABEL_LINES: usize = 3;
/// The share of a category's room between it and the next, when `category-gap` sets none.
const CATEGORY_GAP_SHARE: f32 = 0.25;
/// The gap between the bars of a category, as a share of a bar, when `bar-gap` sets none.
const BAR_GAP_SHARE: f32 = 0.1;
/// The distance between lines of the chart's text, as a factor of its size.
const LINE_HEIGHT: f32 = 1.2;
/// The thickness of a gridline and of the zero line, in pt.
const GRIDLINE: f32 = 0.5;
const ZERO_LINE: f32 = 0.75;
/// The most decimals a value is written with when the chart sets none and it was written with more.
const MAX_WRITTEN_DECIMALS: u8 = 12;

/// The kind of chart: `type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
}

/// Where the legend goes: `legend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Legend {
    Bottom,
    Top,
    Right,
    None,
}

/// The characters a number is written with: between the groups of three digits, if any, and before the decimals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marks {
    pub thousands: Option<char>,
    pub decimal:   char,
}

/// A value of a chart, and the decimals it was written with, which its label keeps when the chart sets none.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Number {
    pub value:    f64,
    pub decimals: u8,
}

/// A series: its entry in the legend and its color, `None` for the palette's.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub label: String,
    pub color: Option<String>,
}

/// A category: its label, its color, which a chart of one series gives its bar, and a value for each series, `None`
/// where there is no value.
#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub label:  String,
    pub color:  Option<String>,
    pub values: Vec<Option<Number>>,
}

/// The series and the categories of a chart, read from its children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartData {
    /// Empty for a chart that writes no `<series>`, which has one.
    pub series:     Vec<Series>,
    pub categories: Vec<Category>,
}

/// What a `<chart>` says, besides its box: the node keeps the width, the height, the gap, the font and the color.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSpec {
    pub kind:         ChartKind,
    pub horizontal:   bool,
    pub stacked:      bool,
    pub prefix:       String,
    pub suffix:       String,
    pub decimals:     Option<u8>,
    pub min:          Option<f64>,
    pub max:          Option<f64>,
    pub marks:        Marks,
    pub bar_gap:      Option<f32>,
    pub category_gap: Option<f32>,
    pub legend:       Option<Legend>,
    pub value_labels: bool,
    /// What the chart shows, for a tagged PDF; empty marks it as decoration.
    pub alt:          Option<String>,
    /// The color of the gridlines: the `line` color of the tokens.
    pub grid_color:   String,
    /// The series and categories once read (`lower`); `None` while they are the node's children.
    pub data:         Option<ChartData>,
}

/// What a `<series>` or a `<category>` adds to its node: its label, and a category's values once its `<value>` children
/// are read (`fold_values`), so a chart of many categories keeps numbers and not a node for each value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartPart {
    pub label:  String,
    pub values: Option<Vec<Option<Number>>>,
}

// ── Reading ───────────────────────────────────────────────────────────────────

/// Read the attributes of a `<chart>` that its node doesn't keep. `width`, `gap`, `font`, `font-size` and `color` are
/// read with the box attributes, and `height` by the parser.
pub(crate) fn read_spec(a: &impl Attrs, tokens: &Tokens) -> Result<ChartSpec, String> {
    let kind = match a.get("type") {
        Some("bar") => ChartKind::Bar,
        Some(other) => return Err(format!("invalid chart type '{other}': expected bar")),
        None => return Err("missing required attribute 'type'".into()),
    };
    let mut spec = ChartSpec {
        kind,
        horizontal:   false,
        stacked:      false,
        prefix:       String::new(),
        suffix:       String::new(),
        decimals:     None,
        min:          None,
        max:          None,
        marks:        Marks { thousands: Some(','), decimal: '.' },
        bar_gap:      None,
        category_gap: None,
        legend:       None,
        value_labels: false,
        alt:          None,
        grid_color:   tokens.resolve_color("line")?,
        data:         None,
    };
    for (name, v) in a.each() {
        match name {
            "direction" => spec.horizontal = match v {
                "vertical"   => false,
                "horizontal" => true,
                other        => return Err(format!("invalid direction '{other}': expected vertical or horizontal")),
            },
            "stacked"        => spec.stacked = v == "true",
            "prefix"         => spec.prefix = v.to_string(),
            "suffix"         => spec.suffix = v.to_string(),
            "decimals"       => spec.decimals = Some(read_decimals(v)?),
            "min"            => spec.min = Some(read_bound("min", v)?),
            "max"            => spec.max = Some(read_bound("max", v)?),
            "thousands-mark" => spec.marks.thousands = read_mark("thousands-mark", v, true)?,
            "decimal-mark"   => spec.marks.decimal = read_mark("decimal-mark", v, false)?.unwrap_or('.'),
            "bar-gap"        => spec.bar_gap = Some(tokens.resolve_space(v)?),
            "category-gap"   => spec.category_gap = Some(tokens.resolve_space(v)?),
            "legend"         => spec.legend = Some(match v {
                "bottom" => Legend::Bottom,
                "top"    => Legend::Top,
                "right"  => Legend::Right,
                "none"   => Legend::None,
                other    => return Err(format!("invalid legend '{other}': expected bottom, top, right or none")),
            }),
            "value-labels"   => spec.value_labels = v == "true",
            "alt"            => spec.alt = Some(v.to_string()),
            _ => {}
        }
    }
    if spec.marks.thousands == Some(spec.marks.decimal) {
        return Err(format!(
            "thousands-mark and decimal-mark are both '{}', which would make 1234.5 unreadable: give them two characters",
            spec.marks.decimal
        ));
    }
    Ok(spec)
}

fn read_decimals(v: &str) -> Result<u8, String> {
    v.parse::<u8>().ok().filter(|d| *d <= 6).ok_or_else(|| format!("invalid decimals '{v}': expected a whole number from 0 to 6"))
}

fn read_bound(name: &str, v: &str) -> Result<f64, String> {
    match read_number(v) {
        Ok(Some(number)) => Ok(number.value),
        _ => Err(format!("invalid {name} '{v}': expected a number, such as 100 or -50")),
    }
}

/// One character, or none where `empty` lets the mark be left out.
fn read_mark(name: &str, v: &str, empty: bool) -> Result<Option<char>, String> {
    let mut chars = v.chars();
    match (chars.next(), chars.next()) {
        (None, _) if empty => Ok(None),
        (Some(c), None) => Ok(Some(c)),
        _ if empty => Err(format!("invalid {name} '{v}': expected one character, or none")),
        _ => Err(format!("invalid {name} '{v}': expected one character")),
    }
}

/// The value a `<value>`'s text is: a plain number, such as `41200`, `-3.5` or `0.25`, or `None` for no text, which is
/// no value. Anything else, such as `41,200`, `$41200` or `12%`, is an error that says what to write.
pub(crate) fn read_number(text: &str) -> Result<Option<Number>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let plain = !(whole.is_empty() && fraction.is_empty())
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.bytes().all(|b| b.is_ascii_digit());
    match text.parse::<f64>() {
        Ok(value) if plain && value.is_finite() => Ok(Some(Number {
            value,
            decimals: u8::try_from(fraction.len()).unwrap_or(u8::MAX).min(MAX_WRITTEN_DECIMALS),
        })),
        _ => Err(format!(
            "'{text}' is not a plain number: write digits, with a - before them for a negative value and a . before \
             the decimals, such as 41200 or -3.5, and leave marks, currency and % to the chart's prefix and suffix"
        )),
    }
}

/// A number of the data as the text of a `<value>`: as it is, with no exponent, which a plain number has none of.
pub(crate) fn plain_number(number: &serde_json::Number) -> String {
    if let Some(whole) = number.as_i64() {
        return whole.to_string();
    }
    if let Some(whole) = number.as_u64() {
        return whole.to_string();
    }
    // Rust writes a float in full, without an exponent: 1e-7 is 0.0000001.
    number.as_f64().map(|value| value.to_string()).unwrap_or_default()
}

// ── Lowering ──────────────────────────────────────────────────────────────────

/// The words of a `<value>` node: what its text, or its data, says.
fn value_text(node: &Node) -> String {
    node.text_runs.iter().map(|run| run.text.as_str()).collect()
}

/// What a `<series>`, a `<category>` or a `<value>` node adds: its label, and its values once they are read.
fn part(node: &Node) -> Option<&ChartPart> {
    match node.detail.as_deref() {
        Some(Detail::ChartPart(part)) => Some(part),
        _ => None,
    }
}

/// The values of the `<value>` children of a `<category>`, in order: the number each one's text is, or the numbers of a
/// list the data repeats one of, which the binding read (`data::bind_values`).
fn values_of(category: &Node) -> Result<Vec<Option<Number>>, String> {
    let mut values = Vec::with_capacity(category.children.len());
    for value in &category.children {
        match part(value).and_then(|part| part.values.as_ref()) {
            Some(list) => values.extend_from_slice(list),
            None => values.push(read_number(&value_text(value)).map_err(|e| format!("<value>: {e}"))?),
        }
    }
    Ok(values)
}

/// Read the `<value>` children of a `<category>` into its numbers, which the chart keeps in their place.
pub(crate) fn fold_values(category: &mut Node) -> Result<(), String> {
    if part(category).is_some_and(|part| part.values.is_some()) {
        return Ok(());
    }
    let values = values_of(category)?;
    category.children = Vec::new();
    match category.detail.as_deref_mut() {
        Some(Detail::ChartPart(part)) => part.values = Some(values),
        _ => category.detail = Some(Box::new(Detail::ChartPart(ChartPart { label: String::new(), values: Some(values) }))),
    }
    Ok(())
}

/// The spec of a `<chart>` node.
pub(crate) fn spec_of(node: &Node) -> Option<&ChartSpec> {
    match node.detail.as_deref() {
        Some(Detail::Chart(spec)) => Some(spec),
        _ => None,
    }
}

/// The series and categories of a `<chart>` node, read from its children: what `lower` keeps.
fn read_data(node: &Node) -> Result<ChartData, String> {
    let mut data = ChartData::default();
    for child in &node.children {
        let label = part(child).map(|part| part.label.clone()).unwrap_or_default();
        match child.kind {
            NodeKind::ChartSeries => data.series.push(Series { label, color: child.color.clone() }),
            NodeKind::ChartCategory => {
                let values = match part(child).and_then(|part| part.values.clone()) {
                    Some(values) => values,
                    None => values_of(child)?,
                };
                data.categories.push(Category { label, color: child.color.clone(), values });
            }
            _ => {}
        }
    }
    let spec = spec_of(node).ok_or("a chart without its attributes")?;
    check(spec, &data)?;
    Ok(data)
}

/// Read the series and categories of a `<chart>` node into its spec, and drop its children: what the layout draws.
pub(crate) fn lower(node: &mut Node) -> Result<(), String> {
    // A chart that reads no data was read when it was parsed.
    if spec_of(node).is_some_and(|spec| spec.data.is_some()) {
        return Ok(());
    }
    let data = read_data(node).map_err(|e| format!("<chart>: {e}"))?;
    node.children = Vec::new();
    if let Some(Detail::Chart(spec)) = node.detail.as_deref_mut() {
        spec.data = Some(data);
    }
    Ok(())
}

/// The series and categories the layout draws: those `lower` read, or else read from the children now, for a chart
/// with data attributes rendered without data.
pub(crate) fn data_of(node: &Node) -> Result<std::borrow::Cow<'_, ChartData>, String> {
    match spec_of(node).and_then(|spec| spec.data.as_ref()) {
        Some(data) => Ok(std::borrow::Cow::Borrowed(data)),
        None => read_data(node).map(std::borrow::Cow::Owned).map_err(|e| format!("<chart>: {e}")),
    }
}

/// How a category is named in an error: by its label, or by its place.
fn category_name(category: &Category, index: usize) -> String {
    if category.label.is_empty() { format!("category {}", index + 1) } else { format!("category '{}'", category.label) }
}

/// Refuse what the chart can't draw, once its series and categories are known.
pub(crate) fn check(spec: &ChartSpec, data: &ChartData) -> Result<(), String> {
    if data.series.len() > MAX_SERIES {
        return Err(too_many("series", MAX_SERIES, data.series.len()));
    }
    if data.categories.len() > MAX_CATEGORIES {
        return Err(too_many("categories", MAX_CATEGORIES, data.categories.len()));
    }
    let series = data.series.len().max(1);
    for (index, category) in data.categories.iter().enumerate() {
        if category.values.len() != series {
            return Err(format!(
                "{} has {} value{}, and the chart has {series} series: a category holds a value for each series, in \
                 their order",
                category_name(category, index), category.values.len(), if category.values.len() == 1 { "" } else { "s" },
            ));
        }
        if spec.stacked
            && let Some(number) = category.values.iter().flatten().find(|number| number.value < 0.0)
        {
            return Err(format!(
                "{} has the value {}, and a stacked chart can't hold a negative value",
                category_name(category, index), number.value,
            ));
        }
    }
    Ok(())
}

/// The error for more parts than a chart holds.
pub(crate) fn too_many(parts: &str, most: usize, found: usize) -> String {
    format!("a chart holds {} {parts} at most, and this one has {}", thousands(most), thousands(found))
}

/// The error for a list of the data that repeats more parts than a chart holds.
pub(crate) fn too_many_in_data(parts: &str, most: usize, path: &str, found: usize) -> String {
    format!("a chart holds {} {parts} at most; the data of '{path}' gives {}", thousands(most), thousands(found))
}

/// A count with a comma between its groups of three digits, for an error.
fn thousands(count: usize) -> String {
    group_digits(&count.to_string(), Some(','))
}

// ── Numbers ───────────────────────────────────────────────────────────────────

/// `digits` with `mark` between its groups of three, from the right.
fn group_digits(digits: &str, mark: Option<char>) -> String {
    let Some(mark) = mark else { return digits.to_string() };
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(mark);
        }
        out.push(c);
    }
    out
}

/// The digits of `value`, without its sign, with `decimals` decimals, rounded half up on the number as it reads: 72.25
/// to one decimal is 72.3, as a reader rounds it, where the float's own rounding gives the even 72.2.
fn fixed(value: f64, decimals: usize) -> String {
    // Rust writes the shortest digits that read back as the float, and never an exponent.
    let written = value.abs().to_string();
    let (whole, fraction) = written.split_once('.').unwrap_or((&written, ""));
    if fraction.len() <= decimals {
        return if decimals == 0 { whole.to_string() } else { format!("{whole}.{fraction:0<decimals$}") };
    }
    let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes().take(decimals)).collect();
    if fraction.as_bytes()[decimals] >= b'5' {
        let mut carry = true;
        for digit in digits.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            digits.insert(0, b'1');
        }
    }
    let (whole, fraction) = digits.split_at(digits.len() - decimals);
    let whole = String::from_utf8_lossy(whole);
    if decimals == 0 { whole.into_owned() } else { format!("{whole}.{}", String::from_utf8_lossy(fraction)) }
}

/// `value` written with `decimals` decimals and the chart's marks, `prefix` before it and `suffix` after it, and a
/// minus before them all for a negative value: `-$1,200.50`.
pub(crate) fn format_number(value: f64, decimals: u8, spec: &ChartSpec) -> String {
    let fixed = fixed(value, usize::from(decimals));
    let (whole, fraction) = fixed.split_once('.').unwrap_or((&fixed, ""));
    // A value that rounds to zero is written without a sign.
    let negative = value < 0.0 && fixed.bytes().any(|b| b.is_ascii_digit() && b != b'0');
    let mut out = String::with_capacity(fixed.len() + spec.prefix.len() + spec.suffix.len() + 4);
    if negative {
        out.push('-');
    }
    out.push_str(&spec.prefix);
    out.push_str(&group_digits(whole, spec.marks.thousands));
    if !fraction.is_empty() {
        out.push(spec.marks.decimal);
        out.push_str(fraction);
    }
    out.push_str(&spec.suffix);
    out
}

// ── The value axis ────────────────────────────────────────────────────────────

/// The value axis: from `lo` to `hi` in steps of `step`, each written with `decimals` decimals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Axis {
    pub lo:       f64,
    pub hi:       f64,
    pub step:     f64,
    pub decimals: u8,
}

/// The round step at or above `raw`: 1, 2 or 5 times a power of ten.
fn round_step(raw: f64) -> f64 {
    let power = 10f64.powf(raw.log10().floor());
    let factor = raw / power;
    let round = if factor <= 1.0 { 1.0 } else if factor <= 2.0 { 2.0 } else if factor <= 5.0 { 5.0 } else { 10.0 };
    round * power
}

/// The steps the axis aims for.
const STEPS: usize = 5;

/// The round step after `step`: 1 to 2, 2 to 5, 5 to 10.
fn next_step(step: f64) -> f64 {
    round_step(step * 1.5)
}

/// The decimals a step needs: none from 1 up, one for 0.5, two for 0.05.
fn step_decimals(step: f64) -> u8 {
    let power = step.log10().floor();
    if power >= 0.0 { 0 } else { (-power).min(f64::from(MAX_WRITTEN_DECIMALS)) as u8 }
}

impl Axis {
    /// The axis of values from `low` to `high`, with zero on it and about five steps, each at least `spacing` pt apart
    /// on an axis `length` pt long.
    pub(crate) fn fit(low: f64, high: f64, length: f32, spacing: f32) -> Axis {
        let low = low.min(0.0);
        let mut high = high.max(0.0);
        if high - low <= 0.0 {
            high = 1.0;
        }
        // The round step whose count is nearest five, the larger of two as near.
        let power = 10f64.powf(((high - low) / STEPS as f64).log10().floor());
        let mut axis = [0.1, 1.0, 10.0]
            .into_iter()
            .flat_map(|scale| [1.0, 2.0, 5.0].map(|round| round * power * scale))
            .map(|step| Axis::with_step(low, high, step))
            .min_by(|a, b| a.count().abs_diff(STEPS).cmp(&b.count().abs_diff(STEPS)).then(b.step.total_cmp(&a.step)))
            .expect("nine steps to choose from");
        // Fewer steps where the labels would crowd: as many as `spacing` lets the length hold, one at least.
        while axis.count() > 1 && f64::from(length) * axis.step / (axis.hi - axis.lo) < f64::from(spacing) {
            axis = Axis::with_step(low, high, next_step(axis.step));
        }
        axis
    }

    fn with_step(low: f64, high: f64, step: f64) -> Axis {
        // A small allowance, so a value that is a whole number of steps doesn't round up to one more.
        let lo = ((low / step) + 1e-9).floor() * step;
        let hi = ((high / step) - 1e-9).ceil() * step;
        Axis { lo, hi: if hi > lo { hi } else { lo + step }, step, decimals: step_decimals(step) }
    }

    /// How many steps the axis has.
    fn count(&self) -> usize {
        ((self.hi - self.lo) / self.step).round() as usize
    }

    /// The values of the steps, from the lowest.
    pub(crate) fn steps(&self) -> impl Iterator<Item = f64> + '_ {
        (0..=self.count()).map(|i| self.lo + i as f64 * self.step)
    }

    /// Where `value` is along an axis from `start` (its `lo`) to `end` (its `hi`).
    fn place(&self, value: f64, start: f32, end: f32) -> f32 {
        start + (end - start) * ((value - self.lo) / (self.hi - self.lo)) as f32
    }
}

// ── Text ──────────────────────────────────────────────────────────────────────

/// What the chart's text is set in.
#[derive(Debug, Clone, Copy)]
struct Type<'a> {
    font:  &'a str,
    size:  f32,
    color: &'a str,
}

impl Type<'_> {
    fn width(&self, text: &str) -> f32 {
        text_width(self.font, text, self.size)
    }

    fn line_height(&self) -> f32 {
        self.size * LINE_HEIGHT
    }

    /// A line of text, `x` its left edge, its center or its right edge as `align` says, `top` the top of its line.
    fn text(&self, content: String, x: f32, top: f32, align: &str) -> RenderNode {
        RenderNode::Text(RenderText {
            x, y: top, content,
            font: self.font.to_string(), size: self.size, color: self.color.to_string(),
            text_align: align.to_string(), line_width: 0.0, word_gap: 0.0,
        })
    }

    /// `text` shortened until it and an ellipsis fit in `width`: the text alone when it fits.
    fn shortened(&self, text: &str, width: f32) -> String {
        if self.width(text) <= width + 0.01 {
            return text.to_string();
        }
        let mut cut = text.trim_end().to_string();
        while !cut.is_empty() {
            cut.pop();
            let trimmed = cut.trim_end();
            if self.width(trimmed) + self.width(ELLIPSIS) <= width + 0.01 {
                return format!("{trimmed}{ELLIPSIS}");
            }
        }
        if self.width(ELLIPSIS) <= width + 0.01 { ELLIPSIS.to_string() } else { String::new() }
    }

    /// `text` wrapped at spaces into lines no wider than `width`, `lines` at most: the last one that holds what is
    /// left ends in an ellipsis, and so does a word wider than a whole line.
    fn wrapped(&self, text: &str, width: f32, lines: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut words = text.split_whitespace().peekable();
        while let Some(word) = words.next() {
            let mut line = word.to_string();
            while let Some(next) = words.peek() {
                let longer = format!("{line} {next}");
                if self.width(&longer) > width + 0.01 {
                    break;
                }
                line = longer;
                words.next();
            }
            if out.len() + 1 == lines.max(1) {
                // The last line holds the rest, shortened.
                let rest: Vec<&str> = words.collect();
                let whole = if rest.is_empty() { line } else { format!("{line} {}", rest.join(" ")) };
                out.push(self.shortened(&whole, width));
                return out;
            }
            out.push(self.shortened(&line, width));
        }
        out
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

/// The color of series `index`.
fn series_color(data: &ChartData, index: usize) -> &str {
    data.series.get(index).and_then(|s| s.color.as_deref()).unwrap_or(PALETTE[index % PALETTE.len()])
}

/// The color of the bar of series `series` in `category`: the category's own in a chart of one series.
fn bar_color<'a>(data: &'a ChartData, category: &'a Category, series: usize) -> &'a str {
    match (&category.color, data.series.len() <= 1) {
        (Some(color), true) => color,
        _ => series_color(data, series),
    }
}

/// Where the legend goes: where `legend` says, or under a chart of two series or more.
fn legend_place(spec: &ChartSpec, data: &ChartData) -> Legend {
    spec.legend.unwrap_or(if data.series.len() >= 2 { Legend::Bottom } else { Legend::None })
}

/// The entries of the legend: a swatch of each series' color and its label.
fn legend_entries(data: &ChartData) -> Vec<(&str, &str)> {
    let count = data.series.len().max(1);
    (0..count).map(|i| (series_color(data, i), data.series.get(i).map_or("", |s| s.label.as_str()))).collect()
}

/// A filled rectangle.
fn rect(x: f32, y: f32, w: f32, h: f32, color: &str) -> RenderNode {
    RenderNode::CanvasRect(RenderCanvasRect {
        x, y, w, h, fill: Some(color.to_string()), stroke: None, stroke_width: 0.0, stroke_dash: None,
        border_radius: 0.0, shadow: None,
    })
}

/// The bars of a chart, by color in the order each color is first drawn: one node and one fill for each color,
/// however many bars there are.
#[derive(Default)]
struct BarSets(Vec<RenderBars>);

impl BarSets {
    fn add(&mut self, color: &str, x: f32, y: f32, w: f32, h: f32) {
        match self.0.iter_mut().find(|set| set.fill == color) {
            Some(set) => set.rects.push([x, y, w, h]),
            None => self.0.push(RenderBars { fill: color.to_string(), rects: vec![[x, y, w, h]] }),
        }
    }

    fn drawn(self) -> impl Iterator<Item = RenderNode> {
        self.0.into_iter().map(RenderNode::Bars)
    }
}

/// A straight line.
fn line(x1: f32, y1: f32, x2: f32, y2: f32, color: &str, thickness: f32) -> RenderNode {
    RenderNode::Line(RenderLine { x1, y1, x2, y2, color: color.to_string(), thickness, dash: None })
}

/// The room a legend takes, and what it draws once placed.
struct LegendLayout {
    width:  f32,
    height: f32,
    /// Each entry: its swatch color, its label shortened to fit, and its place from the legend's top left.
    entries: Vec<(String, String, f32, f32)>,
}

/// The legend of `entries`: in rows that wrap at `width` and are centered in it, or, `in_column`, one entry a line in
/// a column at most `width` wide.
fn legend_layout(entries: &[(&str, &str)], text: Type, width: f32, in_column: bool) -> LegendLayout {
    let swatch = text.size * 0.8;
    let swatch_gap = text.size * 0.4;
    let between = text.size * 1.2;
    let lh = text.line_height();
    let room = (width - swatch - swatch_gap).max(0.0);
    let measured: Vec<(String, String, f32)> = entries.iter().map(|(color, label)| {
        let label = text.shortened(label, room);
        let w = swatch + if label.is_empty() { 0.0 } else { swatch_gap + text.width(&label) };
        (color.to_string(), label, w)
    }).collect();
    if in_column {
        let widest = measured.iter().map(|(_, _, w)| *w).fold(0.0, f32::max);
        let entries = measured.into_iter().enumerate()
            .map(|(i, (color, label, _))| (color, label, 0.0, i as f32 * lh))
            .collect::<Vec<_>>();
        return LegendLayout { width: widest, height: entries.len() as f32 * lh, entries };
    }
    // Rows: as many entries as fit, each row centered.
    let mut rows: Vec<Vec<(String, String, f32)>> = vec![Vec::new()];
    let mut used = 0.0;
    for entry in measured {
        let row = rows.last_mut().expect("rows starts with one");
        let need = if row.is_empty() { entry.2 } else { used + between + entry.2 };
        if !row.is_empty() && need > width + 0.01 {
            used = entry.2;
            rows.push(vec![entry]);
        } else {
            used = need;
            row.push(entry);
        }
    }
    let mut placed = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let row_w: f32 = row.iter().map(|e| e.2).sum::<f32>() + between * row.len().saturating_sub(1) as f32;
        let mut x = ((width - row_w) / 2.0).max(0.0);
        for (color, label, w) in row {
            placed.push((color.clone(), label.clone(), x, index as f32 * lh));
            x += w + between;
        }
    }
    LegendLayout { width, height: rows.len() as f32 * lh, entries: placed }
}

/// Draw the legend's entries with its top left at `x`, `y`.
fn draw_legend(legend: &LegendLayout, text: Type, x: f32, y: f32, out: &mut Vec<RenderNode>) {
    let swatch = text.size * 0.8;
    let swatch_gap = text.size * 0.4;
    let lh = text.line_height();
    for (color, label, ex, ey) in &legend.entries {
        let top = y + ey + (lh - swatch) / 2.0;
        out.push(rect(x + ex, top, swatch, swatch, color));
        if !label.is_empty() {
            out.push(text.text(label.clone(), x + ex + swatch + swatch_gap, y + ey + (lh - text.size) / 2.0, "left"));
        }
    }
}

/// The top of a line of text whose capitals sit centered on `y`.
fn centered_top(y: f32, size: f32) -> f32 {
    y - size * 0.65
}

/// The lowest and the highest a bar reaches: the values, or the totals of the stacks, with `min` and `max`.
fn extent(spec: &ChartSpec, data: &ChartData) -> (f64, f64) {
    let mut low = spec.min.unwrap_or(0.0).min(0.0);
    let mut high = spec.max.unwrap_or(0.0).max(0.0);
    for category in &data.categories {
        if spec.stacked {
            high = high.max(category.values.iter().flatten().map(|n| n.value).sum());
        } else {
            for number in category.values.iter().flatten() {
                low = low.min(number.value);
                high = high.max(number.value);
            }
        }
    }
    (low, high)
}

/// What the value label of a bar says: the value, or the total of a stack, with `decimals`, or else as written.
fn value_label(spec: &ChartSpec, numbers: &[Number]) -> String {
    let total: f64 = numbers.iter().map(|n| n.value).sum();
    let decimals = spec.decimals.unwrap_or_else(|| numbers.iter().map(|n| n.decimals).max().unwrap_or(0));
    format_number(total, decimals, spec)
}

/// The bars of a category in the room from `start`, `room` long across the bars: for each series, where its bar
/// starts and how thick it is. A stack is one bar.
fn bar_slots(spec: &ChartSpec, series: usize, start: f32, room: f32) -> Vec<(f32, f32)> {
    let gap = spec.category_gap.unwrap_or(room * CATEGORY_GAP_SHARE).min(room);
    let bars = room - gap;
    let start = start + gap / 2.0;
    if spec.stacked || series <= 1 {
        return vec![(start, bars.max(0.0)); series.max(1)];
    }
    let n = series as f32;
    let (thickness, bar_gap) = match spec.bar_gap {
        Some(bar_gap) => (((bars - bar_gap * (n - 1.0)) / n).max(0.0), bar_gap),
        None => {
            let thickness = bars / (n + BAR_GAP_SHARE * (n - 1.0));
            (thickness.max(0.0), thickness * BAR_GAP_SHARE)
        }
    };
    (0..series).map(|i| (start + i as f32 * (thickness + bar_gap), thickness)).collect()
}

/// The parts of the bars of a category along the value axis: for each series with a value, from where to where it
/// runs, in values, and its number. Side by side each starts at zero; stacked, each starts where the one before ends.
fn bar_spans(spec: &ChartSpec, category: &Category) -> Vec<Option<(f64, f64, Number)>> {
    let mut base = 0.0;
    category.values.iter().map(|value| value.map(|number| {
        if spec.stacked {
            let span = (base, base + number.value, number);
            base += number.value;
            span
        } else {
            (0.0, number.value, number)
        }
    })).collect()
}

/// Lay out a chart in a box `width` wide at `x`, `y`: its height is its own. Each part takes room inside the box,
/// and the bars get the rest.
pub(crate) fn draw(node: &Node, spec: &ChartSpec, data: &ChartData, x: f32, y: f32, width: f32) -> (RenderNode, f32) {
    let height = node.img_height_constraint.unwrap_or(0.0);
    let text = Type {
        font: &node.font,
        size: node.font_size,
        color: node.text_color.as_deref().unwrap_or("#1a1a1a"),
    };
    let gap = node.gap;
    let mut out = Vec::new();

    // The legend first: it takes its room from the box, and the plot the rest.
    let (px, mut py, mut pw, mut ph) = (x, y, width, height);
    match legend_place(spec, data) {
        Legend::None => {}
        place @ (Legend::Bottom | Legend::Top) => {
            let legend = legend_layout(&legend_entries(data), text, width, false);
            ph = (ph - legend.height - gap).max(0.0);
            let top = if place == Legend::Top {
                py += legend.height + gap;
                y
            } else {
                y + height - legend.height
            };
            draw_legend(&legend, text, x, top, &mut out);
        }
        Legend::Right => {
            let legend = legend_layout(&legend_entries(data), text, width / 3.0, true);
            pw = (pw - legend.width - gap).max(0.0);
            draw_legend(&legend, text, x + width - legend.width, y, &mut out);
        }
    }

    if spec.horizontal {
        draw_horizontal(spec, data, text, gap, (px, py, pw, ph), &mut out);
    } else {
        draw_vertical(spec, data, text, gap, (px, py, pw, ph), &mut out);
    }
    let chart = RenderNode::Box(RenderBox {
        x, y, width, height,
        fill: None, border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None,
        debug_self: node.debug, tag: None, ocg: None, children: out,
    });
    (chart, height)
}

/// The widest of the labels of an axis's steps.
fn widest_step(spec: &ChartSpec, axis: &Axis, text: Type) -> f32 {
    axis.steps().map(|value| text.width(&format_number(value, axis.decimals, spec))).fold(0.0, f32::max)
}

/// Bars that stand up from the value axis on the left, the categories along the bottom.
fn draw_vertical(
    spec: &ChartSpec, data: &ChartData, text: Type, gap: f32, area: (f32, f32, f32, f32), out: &mut Vec<RenderNode>,
) {
    let (x, y, width, height) = area;
    let lh = text.line_height();
    let (low, high) = extent(spec, data);
    let categories = data.categories.len();
    let series = data.series.len().max(1);
    let labeled = data.categories.iter().any(|c| !c.label.trim().is_empty());
    // A value label above the highest bar, and below the lowest when it is negative, needs a line of room.
    let label_room = text.size * 1.5;
    let top = if spec.value_labels { label_room } else { text.size * 0.7 };
    let bottom = if spec.value_labels && low < 0.0 { label_room } else { 0.0 };

    // The steps set the room of their labels, which sets the room of the categories, which sets how many lines their
    // labels take, which sets the room left for the bars, which sets how many steps fit: twice round settles it.
    let mut axis = Axis::fit(low, high, height, lh * 1.2);
    let mut layout = (0.0, 0.0, 0.0, Vec::new());
    for _ in 0..2 {
        let axis_w = widest_step(spec, &axis, text);
        let plot_x = x + axis_w + gap;
        let plot_w = (width - axis_w - gap).max(0.0);
        let band = if categories == 0 { plot_w } else { plot_w / categories as f32 };
        let lines: Vec<Vec<String>> = data.categories.iter()
            .map(|c| text.wrapped(&c.label, band * 0.95, LABEL_LINES))
            .collect();
        let label_h = lines.iter().map(Vec::len).max().unwrap_or(0) as f32 * lh;
        let below = if labeled { gap + label_h } else { 0.0 };
        let plot_h = (height - top - bottom - below).max(0.0);
        axis = Axis::fit(low, high, plot_h, lh * 1.2);
        layout = (plot_x, plot_w, plot_h, lines);
    }
    let (plot_x, plot_w, plot_h, lines) = layout;
    let axis_w = widest_step(spec, &axis, text);
    let plot_x = plot_x.max(x + axis_w + gap);
    let plot_y = y + top;
    let plot_bottom = plot_y + plot_h;
    let at = |value: f64| axis.place(value, plot_bottom, plot_y);

    // Gridlines and the steps' labels.
    for value in axis.steps() {
        let line_y = at(value);
        out.push(line(plot_x, line_y, plot_x + plot_w, line_y, &spec.grid_color, GRIDLINE));
        out.push(text.text(format_number(value, axis.decimals, spec), plot_x - gap, centered_top(line_y, text.size), "right"));
    }

    // The bars, and their labels after them.
    let band = if categories == 0 { 0.0 } else { plot_w / categories as f32 };
    let mut bars = BarSets::default();
    let mut labels = Vec::new();
    for (index, category) in data.categories.iter().enumerate() {
        let band_x = plot_x + index as f32 * band;
        let slots = bar_slots(spec, series, band_x, band);
        let spans = bar_spans(spec, category);
        for (series_index, (slot, span)) in slots.iter().zip(&spans).enumerate() {
            let (Some((from, to, _)), (left, thickness)) = (span, slot) else { continue };
            let (top_y, bottom_y) = (at(from.max(*to)), at(from.min(*to)));
            if bottom_y - top_y > 0.0 && *thickness > 0.0 {
                bars.add(bar_color(data, category, series_index), *left, top_y, *thickness, bottom_y - top_y);
            }
        }
        let out = &mut labels;
        if spec.value_labels {
            let numbers: Vec<Number> = category.values.iter().flatten().copied().collect();
            if spec.stacked && !numbers.is_empty() {
                let (left, thickness) = slots[0];
                let total: f64 = numbers.iter().map(|n| n.value).sum();
                out.push(text.text(value_label(spec, &numbers), left + thickness / 2.0, at(total) - text.size * 1.25, "center"));
            } else if !spec.stacked {
                for ((left, thickness), span) in slots.iter().zip(&spans) {
                    let Some((_, value, number)) = span else { continue };
                    let top_y = if *value < 0.0 { at(*value) + text.size * 0.25 } else { at(*value) - text.size * 1.25 };
                    out.push(text.text(value_label(spec, &[*number]), left + thickness / 2.0, top_y, "center"));
                }
            }
        }
        for (line_index, label) in lines.get(index).into_iter().flatten().enumerate() {
            let top_y = plot_bottom + bottom + gap + line_index as f32 * lh + (lh - text.size) / 2.0;
            out.push(text.text(label.clone(), band_x + band / 2.0, top_y, "center"));
        }
    }
    out.extend(bars.drawn());
    out.append(&mut labels);

    // The zero line, over the bars, when the axis runs below zero.
    if axis.lo < 0.0 {
        let zero = at(0.0);
        out.push(line(plot_x, zero, plot_x + plot_w, zero, text.color, ZERO_LINE));
    }
}

/// Bars that run right from the value axis along the bottom, the categories down the left side.
fn draw_horizontal(
    spec: &ChartSpec, data: &ChartData, text: Type, gap: f32, area: (f32, f32, f32, f32), out: &mut Vec<RenderNode>,
) {
    let (x, y, width, height) = area;
    let lh = text.line_height();
    let (low, high) = extent(spec, data);
    let categories = data.categories.len();
    let series = data.series.len().max(1);

    // The labels take a column as wide as the longest, a third of the chart at most.
    let label_w = data.categories.iter().map(|c| text.width(c.label.trim())).fold(0.0, f32::max).min(width / 3.0);
    let label_column = if label_w > 0.0 { label_w + gap } else { 0.0 };
    // The steps' row along the bottom.
    let plot_h = (height - gap - lh).max(0.0);
    let band = if categories == 0 { plot_h } else { plot_h / categories as f32 };

    // Room for the value labels beside the bars, and for half of the first and the last steps' labels.
    let value_room = |negative: bool| -> f32 {
        if !spec.value_labels {
            return 0.0;
        }
        data.categories.iter()
            .filter_map(|category| {
                let numbers: Vec<Number> = category.values.iter().flatten().copied().collect();
                if spec.stacked {
                    (!negative && !numbers.is_empty()).then(|| text.width(&value_label(spec, &numbers)))
                } else {
                    numbers.iter().filter(|n| (n.value < 0.0) == negative)
                        .map(|n| text.width(&value_label(spec, &[*n]))).reduce(f32::max)
                }
            })
            .fold(0.0, f32::max)
            .max(0.0) + if spec.value_labels { text.size * 0.25 } else { 0.0 }
    };
    let (left_values, right_values) = (if low < 0.0 { value_room(true) } else { 0.0 }, value_room(false));

    let mut axis = Axis::fit(low, high, width, text.size * 5.0);
    let mut plot = (0.0, 0.0);
    for _ in 0..2 {
        let first = text.width(&format_number(axis.lo, axis.decimals, spec)) / 2.0;
        let last = text.width(&format_number(axis.hi, axis.decimals, spec)) / 2.0;
        let plot_x = x + (label_column + left_values).max(first);
        let plot_w = (x + width - right_values.max(last) - plot_x).max(0.0);
        let spacing = widest_step(spec, &axis, text) + text.size;
        axis = Axis::fit(low, high, plot_w, spacing);
        plot = (plot_x, plot_w);
    }
    let (plot_x, plot_w) = plot;
    let plot_y = y;
    let at = |value: f64| axis.place(value, plot_x, plot_x + plot_w);

    for value in axis.steps() {
        let line_x = at(value);
        out.push(line(line_x, plot_y, line_x, plot_y + plot_h, &spec.grid_color, GRIDLINE));
        out.push(text.text(format_number(value, axis.decimals, spec), line_x, plot_y + plot_h + gap + (lh - text.size) / 2.0, "center"));
    }

    let lines_per_label = ((band / lh).floor() as usize).clamp(1, LABEL_LINES);
    let mut bars = BarSets::default();
    let mut labels = Vec::new();
    for (index, category) in data.categories.iter().enumerate() {
        let band_y = plot_y + index as f32 * band;
        let slots = bar_slots(spec, series, band_y, band);
        let spans = bar_spans(spec, category);
        for (series_index, (slot, span)) in slots.iter().zip(&spans).enumerate() {
            let (Some((from, to, _)), (top, thickness)) = (span, slot) else { continue };
            let (left, right) = (at(from.min(*to)), at(from.max(*to)));
            if right - left > 0.0 && *thickness > 0.0 {
                bars.add(bar_color(data, category, series_index), left, *top, right - left, *thickness);
            }
        }
        let out = &mut labels;
        if spec.value_labels {
            let numbers: Vec<Number> = category.values.iter().flatten().copied().collect();
            if spec.stacked && !numbers.is_empty() {
                let (top, thickness) = slots[0];
                let total: f64 = numbers.iter().map(|n| n.value).sum();
                let middle = top + thickness / 2.0;
                out.push(text.text(value_label(spec, &numbers), at(total) + text.size * 0.25, centered_top(middle, text.size), "left"));
            } else if !spec.stacked {
                for ((top, thickness), span) in slots.iter().zip(&spans) {
                    let Some((_, value, number)) = span else { continue };
                    let middle = centered_top(top + thickness / 2.0, text.size);
                    let label = value_label(spec, &[*number]);
                    if *value < 0.0 {
                        out.push(text.text(label, at(*value) - text.size * 0.25, middle, "right"));
                    } else {
                        out.push(text.text(label, at(*value) + text.size * 0.25, middle, "left"));
                    }
                }
            }
        }
        if label_w > 0.0 {
            let lines = text.wrapped(&category.label, label_w, lines_per_label);
            let block = lines.len() as f32 * lh;
            let first = band_y + (band - block) / 2.0;
            for (line_index, label) in lines.into_iter().enumerate() {
                out.push(text.text(label, x + label_w, first + line_index as f32 * lh + (lh - text.size) / 2.0, "right"));
            }
        }
    }
    out.extend(bars.drawn());
    out.append(&mut labels);

    if axis.lo < 0.0 {
        let zero = at(0.0);
        out.push(line(zero, plot_y, zero, plot_y + plot_h, text.color, ZERO_LINE));
    }
}

#[cfg(test)]
#[path = "chart_tests.rs"]
mod tests;
