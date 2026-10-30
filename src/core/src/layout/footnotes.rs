//! Footnotes: the notes of the `<fn>` in a flow, and the area that shows them at the bottom of their page.
//!
//! A note goes on the page where its anchor lands, and takes room that the text of that page then lacks, which can
//! push the anchor to the next page. The flow is paginated one page at a time to settle that. A page is filled
//! without room for notes, and the notes of what landed on it are measured; it is filled again with that much
//! room kept back. The second fill is the first one cut short, so it holds no more notes than the first, and they
//! fit the room. Up to two more fills look for a tighter fit, with the room for fewer notes (see `settle_page`).
//! A line whose note does not fit moves to the next page, with its anchor.
//!
//! A node taller than a page is cut at the bottom of it (`clip_overflow`), and a number below the cut has no note,
//! on that page or any other. A fill whose page may run past its room lays it out to find where its numbers are
//! drawn, which the laid-out tree marks, and the notes of those below the cut take no room. More room kept back
//! moves the cut up and shows fewer numbers, so the search of `settle_page` holds.

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

/// One page of the flow: the nodes on it with the trees `fill_page` kept for them, the notes of their anchors and
/// the room kept back for the notes.
pub(super) struct FlowPage {
    pub nodes:   Vec<Node>,
    pub kept:    Vec<Option<Kept>>,
    pub notes:   Vec<Note>,
    pub reserve: f32,
}

/// Whether a node or any below it holds a footnote.
pub(super) fn has_footnotes(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| node.text_runs.iter().any(|run| run.footnote.is_some()) || has_footnotes(&node.children))
}

/// Give the anchors of a flow their places among them and their numbers, in reading order. A counter that starts
/// over on every page numbers them when the page is known; until then every anchor is a 1. `start` is how many
/// notes the earlier sections that count across the document hold.
pub(super) fn number_flow(flow: &mut [Node], counter: FootnoteCounter, start: u32) {
    fn walk(nodes: &mut [Node], index: &mut u32, first: u32, counter: FootnoteCounter) {
        for node in nodes {
            for run in node.text_runs.iter_mut() {
                if let Some(note) = run.footnote.as_mut() {
                    note.index  = *index;
                    note.number = if counter == FootnoteCounter::Page { 1 } else { first + *index };
                    *index += 1;
                }
            }
            walk(&mut node.children, index, first, counter);
        }
    }
    let first = if counter == FootnoteCounter::Document { start + 1 } else { 1 };
    walk(flow, &mut 0, first, counter);
}

/// The notes of the anchors in `nodes`, in reading order, but for the numbers `cut_off` by the bottom of the
/// page. On a page that counts from 1, the anchors shown are numbered here.
fn take_notes(nodes: &mut [Node], counter: FootnoteCounter, cut_off: &[u32]) -> Vec<Note> {
    fn walk(nodes: &mut [Node], page_count: &mut u32, counter: FootnoteCounter, cut_off: &[u32], out: &mut Vec<Note>) {
        for node in nodes {
            if node.kind == NodeKind::Text {
                let size = (node.font_size * NOTE_SIZE_SCALE).max(NOTE_SIZE_MIN);
                for run in node.text_runs.iter_mut() {
                    if let Some(note) = run.footnote.as_mut().filter(|note| !cut_off.contains(&note.index)) {
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
            walk(&mut node.children, page_count, counter, cut_off, out);
        }
    }
    let mut out = Vec::new();
    walk(nodes, &mut 0, counter, cut_off, &mut out);
    out
}

/// The footnotes of a page's `nodes` whose numbers the bottom of the page cuts off, by index: the nodes laid out
/// as the page lays them out, and when they run past `room`, every number whose top is at or below it.
fn numbers_cut_off(nodes: &[Node], avail_w: f32, room: f32, gap: f32) -> Vec<u32> {
    fn below(tree: &[RenderNode], bottom: f32, out: &mut Vec<u32>) {
        for node in tree {
            match node {
                RenderNode::FootnoteMark(mark) if mark.y >= bottom => out.push(mark.index),
                RenderNode::Box(b)         => below(&b.children, bottom, out),
                RenderNode::Link(l)        => below(&l.children, bottom, out),
                RenderNode::CanvasLayer(l) => below(&l.children, bottom, out),
                _ => {}
            }
        }
    }
    let (tree, used) = layout_column(nodes, 0.0, 0.0, avail_w, Some(room), gap, &Align::Stretch, &Justify::Start);
    let mut out = Vec::new();
    if flow_runs_past(used, room) {
        below(&tree, room, &mut out);
    }
    out
}

/// The pages of a flow, with the notes of each, filled one at a time as the iterator is read, so that a page can be
/// laid out before the next is filled. The first page has `budget_first` of height and starts at `start`'s first y,
/// every page after it `budget_rest` and the rest y. An empty flow is one empty page. A flow with no footnotes is
/// paginated as it always was.
#[allow(clippy::too_many_arguments)]
pub(super) fn split_flow<'a>(
    flow:         &'a [Node],
    start:        Option<FlowStart>,
    avail_w:      f32,
    budget_first: f32,
    budget_rest:  f32,
    gap:          f32,
    counter:      FootnoteCounter,
    has_notes:    bool,
) -> impl Iterator<Item = FlowPage> + 'a {
    let mut pending = Pending::new(flow);
    let mut first = true;
    std::iter::from_fn(move || {
        if pending.is_empty() && !first {
            return None;
        }
        let at = start.map(|start| start.on_page(first));
        let budget = if first { budget_first } else { budget_rest };
        first = false;
        if has_notes {
            return Some(settle_page(&mut pending, at, avail_w, budget, budget_rest, gap, counter));
        }
        let (nodes, kept) = fill_page(&mut pending, at, avail_w, budget, budget_rest, gap).0.into_iter().unzip();
        Some(FlowPage { nodes, kept, notes: Vec::new(), reserve: 0.0 })
    })
}

/// One try at a page: what is left of the flow, the page, its notes and the height they need.
struct Fill<'a> {
    rest:  Pending<'a>,
    nodes: Vec<Node>,
    kept:  Vec<Option<Kept>>,
    notes: Vec<Note>,
    area:  f32,
}

/// Fill the next page, whose flow starts `at` when that is known, with `reserve` kept back for notes.
#[allow(clippy::too_many_arguments)]
fn fill<'a>(pending: &Pending<'a>, at: Option<(f32, f32)>, avail_w: f32, budget: f32, next_budget: f32, gap: f32, counter: FootnoteCounter, reserve: f32) -> Fill<'a> {
    let mut rest = pending.clone();
    let room = (budget - reserve).max(0.0);
    let (page, runs_past) = fill_page(&mut rest, at, avail_w, room, next_budget, gap);
    let (mut nodes, mut kept): (Vec<Node>, Vec<Option<Kept>>) = page.into_iter().unzip();
    // Only a page cut at its bottom can hide a number, and only a page whose first node did not fit is cut.
    let cut_off = if runs_past && has_footnotes(&nodes) {
        numbers_cut_off(&nodes, avail_w, room, gap)
    } else {
        Vec::new()
    };
    let notes = take_notes(&mut nodes, counter, &cut_off);
    if counter == FootnoteCounter::Page {
        // The anchors of the page got their numbers here, after the trees of their texts were laid out.
        for (node, tree) in nodes.iter().zip(kept.iter_mut()) {
            if has_footnotes(std::slice::from_ref(node)) { *tree = None; }
        }
    }
    let area = area_height(&notes, avail_w, budget);
    Fill { rest, nodes, kept, notes, area }
}

/// Fill the next page so that its notes fit under it, and take its nodes off `pending`.
///
/// The first fill has no room kept back and holds `n` notes. A fill with the room for the first `j` of them
/// holds `k(j)` notes, and `k` falls as `j` grows, since more room kept back leaves less of the flow for the page,
/// or moves up the cut of a node taller than the page. The room fits when `k(j) <= j`, and `j = n` always does. The
/// tightest page is the least `j` that fits, which this looks for in two steps after the fill with `j = n`, for four
/// fills at most: the `j` that fill held, and if its notes did not fit, the number it held, which does.
fn settle_page(pending: &mut Pending, at: Option<(f32, f32)>, avail_w: f32, budget: f32, next_budget: f32, gap: f32, counter: FootnoteCounter) -> FlowPage {
    let mut first = fill(pending, at, avail_w, budget, next_budget, gap, counter, 0.0);
    if first.area <= 0.0 {
        *pending = first.rest;
        return FlowPage { nodes: first.nodes, kept: first.kept, notes: first.notes, reserve: 0.0 };
    }
    // The fills after it need only its notes. Its nodes and their trees go, and so does each fill as soon as it
    // can't be the page, so that no more than two fills are alive at a time.
    let first_notes = std::mem::take(&mut first.notes);
    drop(first);
    let n = first_notes.len();
    // Fill with the room the first `j` notes of the first fill need.
    let with_room = |j: usize| {
        let reserve = area_height(&first_notes[..j], avail_w, budget);
        (fill(pending, at, avail_w, budget, next_budget, gap, counter, reserve), reserve)
    };
    let fits = |(fill, reserve): &(Fill, f32)| fill.area <= *reserve + 0.01;

    let mut chosen = with_room(n);
    if !fits(&chosen) {
        // Not a start of the first fill after all: keep back what this one needs, and take the result as it is.
        let reserve = chosen.0.area;
        drop(chosen);
        chosen = (fill(pending, at, avail_w, budget, next_budget, gap, counter, reserve), reserve);
    } else if chosen.0.notes.len() < n {
        let held = chosen.0.notes.len();
        let tight = with_room(held);
        if fits(&tight) {
            chosen = tight;
        } else if tight.0.notes.len() < n {
            let next = tight.0.notes.len();
            drop(tight);
            let more = with_room(next);
            if fits(&more) {
                chosen = more;
            }
        }
    }
    let (fill, reserve) = chosen;
    *pending = fill.rest;
    FlowPage { nodes: fill.nodes, kept: fill.kept, notes: fill.notes, reserve }
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
        .map(|n| word_width(Metrics::of(&n.font), &n.number.to_string(), n.size, true))
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
