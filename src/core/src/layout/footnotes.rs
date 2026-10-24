//! Footnotes: the notes of the `<fn>` in a flow, and the area that shows them at the bottom of their page.
//!
//! A note goes on the page where its anchor lands, and takes room that the text of that page then lacks, which can
//! push the anchor to the next page. The flow is paginated one page at a time to settle that. A page is filled
//! without room for notes, and the notes of what landed on it are measured; it is filled again with that much
//! room kept back. The second fill is the first one cut short, so it holds no more notes than the first, and they
//! fit the room. Up to two more fills look for a tighter fit, with the room for fewer notes (see `settle_page`).
//! A line whose note does not fit moves to the next page, with its anchor.

use super::*;
use crate::parse::FootnoteCounter;

/// The size of a note, as a share of the size of the text it is in, and the smallest it gets.
const NOTE_SIZE_SCALE: f32 = 0.8;
const NOTE_SIZE_MIN:   f32 = 6.0;
/// The room above the rule, the rule, and the room between it and the first note.
const GAP_ABOVE:       f32 = 6.0;
const RULE_THICKNESS:  f32 = 0.5;
const RULE_COLOR:      &str = "#888888";
const GAP_BELOW_RULE:  f32 = 4.0;
/// The room between two notes, and between a note's number and its words.
const GAP_BETWEEN:     f32 = 2.0;
const GAP_LABEL:       f32 = 3.0;
/// The rule is this share of the width.
const RULE_SHARE:      f32 = 1.0 / 3.0;

/// A note on a page: its number, its words and the type it is set in, which is the text's it belongs to.
pub(super) struct Note {
    number:      u32,
    runs:        Vec<TextRun>,
    font:        String,
    size:        f32,
    line_height: f32,
    color:       Option<String>,
}

/// One page of the flow: the nodes on it, the notes of their anchors and the room kept back for the notes.
pub(super) struct FlowPage {
    pub nodes:   Vec<Node>,
    pub notes:   Vec<Note>,
    pub reserve: f32,
}

/// Whether a node or any below it holds a footnote.
pub(super) fn has_footnotes(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| node.text_runs.iter().any(|run| run.footnote.is_some()) || has_footnotes(&node.children))
}

/// Give the anchors of a flow their numbers, in reading order. A counter that starts over on every page numbers
/// them when the page is known; until then every anchor is a 1. `start` is how many notes the earlier sections
/// that count across the document hold.
pub(super) fn number_flow(flow: &mut [Node], counter: FootnoteCounter, start: u32) {
    fn walk(nodes: &mut [Node], next: &mut u32, counter: FootnoteCounter) {
        for node in nodes {
            for run in node.text_runs.iter_mut() {
                if let Some(note) = run.footnote.as_mut() {
                    note.number = if counter == FootnoteCounter::Page { 1 } else { *next };
                    *next += 1;
                }
            }
            walk(&mut node.children, next, counter);
        }
    }
    let mut next = if counter == FootnoteCounter::Document { start + 1 } else { 1 };
    walk(flow, &mut next, counter);
}

/// The notes of the anchors in `nodes`, in reading order. On a page that counts from 1, the anchors are numbered
/// here.
fn take_notes(nodes: &mut [Node], counter: FootnoteCounter) -> Vec<Note> {
    fn walk(nodes: &mut [Node], page_count: &mut u32, counter: FootnoteCounter, out: &mut Vec<Note>) {
        for node in nodes {
            if node.kind == NodeKind::Text {
                let size = (node.font_size * NOTE_SIZE_SCALE).max(NOTE_SIZE_MIN);
                for run in node.text_runs.iter_mut() {
                    if let Some(note) = run.footnote.as_mut() {
                        if counter == FootnoteCounter::Page {
                            *page_count += 1;
                            note.number = *page_count;
                        }
                        out.push(Note {
                            number:      note.number,
                            runs:        note.runs.clone(),
                            font:        node.font.clone(),
                            size,
                            line_height: node.line_height,
                            color:       node.text_color.clone(),
                        });
                    }
                }
            }
            walk(&mut node.children, page_count, counter, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut 0, counter, &mut out);
    out
}

/// The pages of a flow, with the notes of each. A flow with no footnotes is paginated as it always was.
pub(super) fn split_flow(
    flow:         &[Node],
    avail_w:      f32,
    budget_first: f32,
    budget_rest:  f32,
    counter:      FootnoteCounter,
    has_notes:    bool,
) -> Vec<FlowPage> {
    if !has_notes {
        return split_into_pages(flow, avail_w, budget_first, budget_rest, 0.0)
            .into_iter()
            .map(|nodes| FlowPage { nodes, notes: Vec::new(), reserve: 0.0 })
            .collect();
    }
    let mut pending = Pending::new(flow);
    let mut pages: Vec<FlowPage> = Vec::new();
    while !pending.is_empty() {
        let budget = if pages.is_empty() { budget_first } else { budget_rest };
        pages.push(settle_page(&mut pending, avail_w, budget, budget_rest, counter));
    }
    pages
}

/// One try at a page: what is left of the flow, the page, its notes and the height they need.
struct Fill<'a> {
    rest:  Pending<'a>,
    nodes: Vec<Node>,
    notes: Vec<Note>,
    area:  f32,
}

/// Fill the next page with `reserve` kept back for notes.
fn fill<'a>(pending: &Pending<'a>, avail_w: f32, budget: f32, next_budget: f32, counter: FootnoteCounter, reserve: f32) -> Fill<'a> {
    let mut rest = pending.clone();
    let mut nodes = fill_page(&mut rest, avail_w, (budget - reserve).max(0.0), next_budget, 0.0);
    let notes = take_notes(&mut nodes, counter);
    let area = area_height(&notes, avail_w, budget);
    Fill { rest, nodes, notes, area }
}

/// Fill the next page so that its notes fit under it, and take its nodes off `pending`.
///
/// The first fill has no room kept back and holds `n` notes. A fill with the room for the first `j` of them
/// holds `k(j)` notes, and `k` falls as `j` grows, since more room kept back leaves less of the flow for the page.
/// The room fits when `k(j) <= j`, and `j = n` always does. The tightest page is the least `j` that fits, which this
/// looks for in two steps after the fill with `j = n`, for four fills at most: the `j` that fill held, and if its
/// notes did not fit, the number it held, which does.
fn settle_page(pending: &mut Pending, avail_w: f32, budget: f32, next_budget: f32, counter: FootnoteCounter) -> FlowPage {
    let first = fill(pending, avail_w, budget, next_budget, counter, 0.0);
    if first.area <= 0.0 {
        *pending = first.rest;
        return FlowPage { nodes: first.nodes, notes: first.notes, reserve: 0.0 };
    }
    let n = first.notes.len();
    // Fill with the room the first `j` notes of the first fill need.
    let with_room = |j: usize| {
        let reserve = area_height(&first.notes[..j], avail_w, budget);
        (fill(pending, avail_w, budget, next_budget, counter, reserve), reserve)
    };
    let fits = |(fill, reserve): &(Fill, f32)| fill.area <= *reserve + 0.01;

    let mut chosen = with_room(n);
    if !fits(&chosen) {
        // Not a start of the first fill after all: keep back what this one needs, and take the result as it is.
        let reserve = chosen.0.area;
        chosen = (fill(pending, avail_w, budget, next_budget, counter, reserve), reserve);
    } else if chosen.0.notes.len() < n {
        let held = chosen.0.notes.len();
        let tight = with_room(held);
        if fits(&tight) {
            chosen = tight;
        } else if tight.0.notes.len() < n {
            let more = with_room(tight.0.notes.len());
            if fits(&more) {
                chosen = more;
            }
        }
    }
    let (fill, reserve) = chosen;
    *pending = fill.rest;
    FlowPage { nodes: fill.nodes, notes: fill.notes, reserve }
}

/// The height of the area for `notes` in a box `width` wide: the room above the rule, the rule and the notes.
/// It is never more than `cap`, the height of a page: a note is cut where the page ends.
pub(super) fn area_height(notes: &[Note], width: f32, cap: f32) -> f32 {
    if notes.is_empty() { 0.0 } else { build_area(notes, 0.0, 0.0, width, cap).1 }
}

/// Draw the area for `notes` with its bottom edge at `bottom`.
pub(super) fn draw_area(notes: &[Note], x: f32, bottom: f32, width: f32, cap: f32) -> Vec<RenderNode> {
    if notes.is_empty() { return Vec::new(); }
    let height = area_height(notes, width, cap);
    build_area(notes, x, bottom - height, width, cap).0
}

/// Lay out the area with its top at `y`: the rule, then each note with its number hanging at the left.
fn build_area(notes: &[Note], x: f32, y: f32, width: f32, cap: f32) -> (Vec<RenderNode>, f32) {
    let label_w = notes.iter()
        .map(|n| word_width(&n.font, &n.number.to_string(), n.size, true))
        .fold(0.0_f32, f32::max) + GAP_LABEL;
    let text_w = (width - label_w).max(0.0);

    // How many lines each note takes at full length.
    let note_text = |n: &Note, max_lines: Option<usize>| Node {
        kind:        NodeKind::Text,
        text_runs:   n.runs.clone(),
        font:        n.font.clone(),
        font_size:   n.size,
        line_height: n.line_height,
        text_color:  n.color.clone(),
        max_lines,
        ellipsis:    max_lines.is_some(),
        ..Node::layout_default()
    };
    let natural: Vec<usize> = notes.iter()
        .map(|n| ((layout_text(&note_text(n, None), 0.0, 0.0, text_w).1 / (n.size * n.line_height).max(0.01)).round() as usize).max(1))
        .collect();

    let fixed = GAP_ABOVE + RULE_THICKNESS + GAP_BELOW_RULE + GAP_BETWEEN * (notes.len() - 1) as f32;
    let full: f32 = fixed + notes.iter().zip(&natural).map(|(n, lines)| *lines as f32 * n.size * n.line_height).sum::<f32>();

    // Over a page: cut lines off the last notes, down to one line each, until it is not.
    let mut lines = natural.clone();
    let mut over = full - cap;
    for (i, n) in notes.iter().enumerate().rev() {
        if over <= 0.0 { break; }
        let line_h = (n.size * n.line_height).max(0.01);
        let cut = ((over / line_h).ceil() as usize).min(lines[i] - 1);
        lines[i] -= cut;
        over -= cut as f32 * line_h;
    }

    let mut out: Vec<RenderNode> = Vec::new();
    let rule_y = y + GAP_ABOVE + RULE_THICKNESS / 2.0;
    out.push(RenderNode::Line(RenderLine {
        x1: x, y1: rule_y, x2: x + width * RULE_SHARE, y2: rule_y,
        color: RULE_COLOR.to_string(), thickness: RULE_THICKNESS, dash: None,
    }));
    let mut cursor = y + GAP_ABOVE + RULE_THICKNESS + GAP_BELOW_RULE;
    for (i, n) in notes.iter().enumerate() {
        if i > 0 { cursor += GAP_BETWEEN; }
        let max_lines = (lines[i] < natural[i]).then_some(lines[i]);
        let (body, h) = layout_text(&note_text(n, max_lines), x + label_w, cursor, text_w);
        let (sup_size, dy) = sup_geometry(n.size);
        out.push(RenderNode::Text(RenderText {
            x, y: cursor + dy,
            content: n.number.to_string(), font: n.font.clone(), size: sup_size,
            color: n.color.clone().unwrap_or_else(|| "#1a1a1a".to_string()),
            text_align: "left".to_string(), line_width: 0.0, word_gap: 0.0,
        }));
        out.push(body);
        cursor += h;
    }
    (out, cursor - y)
}
