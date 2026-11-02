//! Columns: the flow of a section with `columns` (`parse::Columns`) fills each page as that many narrow pages, left
//! to right, each with the page's height at the width of a column, which the same `fill_room` fills as it fills a
//! page. Headers, footers, footnotes and layers keep the full width.
//!
//! An inset (`parse::Inset`) cuts each column it covers into pieces, the room above it and the room below it, which
//! the column fills in turn, as it fills the next column. Its rectangle is known before the page is filled, from
//! the document, except on which page one on `last` goes: the flow is filled without it, and the page that takes
//! the last of the flow is filled again with it. If text no longer fits there, it goes on to a new page with no
//! inset, and the inset stays where it was.
//!
//! The notes of the footnotes go under the columns at the full width, as on a page in one column: one fill of
//! `footnotes::settle` fills all the columns of the page, with the room for the notes taken off the foot of each.
//! The notes' area doesn't rise above the foot of an inset: when the notes need more, the line that calls the
//! first note that doesn't fit goes on to the next page, with its note, and the page ends there.
//!
//! With `column-fill="balance"`, the page that takes the last of the flow is levelled: the least height at which its
//! columns hold what is left, found by bisection. A node that isn't cut makes "fits" not strictly rise with the
//! height, so the search keeps the least height that fit.

use super::*;
use crate::parse::{ColumnFill, Columns, FootnoteCounter, Inset};

/// The least height of a piece worth filling.
const MIN_PIECE: f32 = 1.0;
/// How many heights the levelling of a balanced page tries: the room of a page over 2^12, a fraction of a point.
const BALANCE_TRIES: usize = 12;

/// Where the columns of a page are, across a flow `avail_w` wide: how many, how wide each is and the gap between two.
/// Every width a column's content is cut at and laid out at comes from here.
#[derive(Clone, Copy)]
pub(super) struct Grid {
    count: usize,
    w:     f32,
    gap:   f32,
}

impl Grid {
    pub(super) fn new(columns: &Columns, avail_w: f32) -> Self {
        let count = columns.count.max(1);
        let gap = if count > 1 { columns.gap } else { 0.0 };
        let w = ((avail_w - gap * (count - 1) as f32) / count as f32).max(0.0);
        Grid { count, w, gap }
    }

    /// The left edge of column `col`, from the left of the flow.
    fn x(&self, col: usize) -> f32 {
        col as f32 * (self.w + self.gap)
    }

    /// The width of `span` columns and the gaps between them.
    fn span_w(&self, span: usize) -> f32 {
        span as f32 * self.w + span.saturating_sub(1) as f32 * self.gap
    }
}

/// A stretch of a page that the flow fills in turn: a column, or its part above an inset, between two or below one.
/// `top` is from the top of the flow, `h` the height it is filled to, and `bottom` where it ends, at the next inset
/// or the foot of the column, which it is laid out and clipped to. A balanced page fills it to less than that.
#[derive(Clone, Copy, Debug)]
struct Piece {
    col:    usize,
    top:    f32,
    h:      f32,
    bottom: f32,
    /// Nothing cuts the column, so a node taller than it is placed anyway, as on a page.
    whole:  bool,
}

/// The pieces of a page whose columns end at `room`, around `insets`, column by column and each top to bottom, filled
/// to `fill_to` at most.
fn pieces(grid: &Grid, insets: &[&Inset], fill_to: f32, room: f32) -> Vec<Piece> {
    let mut out = Vec::new();
    for col in 0..grid.count {
        let mut cuts: Vec<(f32, f32)> = insets.iter()
            .filter(|inset| inset.at.col <= col && col < inset.at.col + inset.at.span)
            .map(|inset| (inset.at.top, inset.at.top + inset.at.height))
            .collect();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let whole = cuts.is_empty();
        let mut top = 0.0_f32;
        for (start, end) in cuts {
            let bottom = start.min(room);
            let h = bottom.min(fill_to) - top;
            if h >= MIN_PIECE {
                out.push(Piece { col, top, h, bottom, whole: false });
            }
            top = top.max(end);
        }
        let h = room.min(fill_to) - top;
        if h >= MIN_PIECE {
            out.push(Piece { col, top, h, bottom: room, whole });
        }
    }
    out
}

/// Whether an inset on the pages `scope` is on the page `index` of its section, counted from 0, which prints
/// `number`, as `page_scope_indices` reads a scope. One on `last` is placed apart (`fill_pages`).
fn on_page(scope: &PageScope, index: usize, number: u64) -> bool {
    let page = index as u32 + 1;
    match scope {
        PageScope::Each          => true,
        PageScope::First         => index == 0,
        PageScope::Last          => false,
        PageScope::Odd           => number % 2 == 1,
        PageScope::Even          => number % 2 == 0,
        PageScope::Pages(ranges) => ranges.iter().any(|r| page >= r.start && r.end.is_none_or(|end| page <= end)),
    }
}

/// What a piece took off the flow: its nodes with their trees, and whether it may run past its room, as
/// `fill_room` says.
struct Part {
    piece:     Piece,
    nodes:     Vec<Node>,
    kept:      Vec<Option<Kept>>,
    runs_past: bool,
}

/// A page in columns, filled: its parts, its notes and the room kept back for them at the foot of the columns.
struct PageFill {
    parts:   Vec<Part>,
    notes:   Vec<footnotes::Note>,
    reserve: f32,
}

/// The flow of a section in columns and what it is filled in: the width `w` of the flow, the height of the
/// first page's flow and of the pages after it, where it starts when the header is as high on every page as it was
/// measured, and the number of its first page, which an inset on odd or even pages needs.
pub(super) struct ColumnFlow<'a> {
    pub section:      &'a SectionLayout,
    pub nodes:        &'a [Node],
    pub w:            f32,
    pub budget_first: f32,
    pub budget_rest:  f32,
    pub start:        Option<FlowStart>,
    pub has_notes:    bool,
    pub first_number: u64,
}

/// Fill the pages of a section in columns. Each page's flow is laid out as soon as it is filled when where it starts
/// is known, and waits for the header above it otherwise (`PageFlow::Columns`); its insets wait for the numbers of
/// the pages, which they can show (`draw_insets`).
pub(super) fn fill_pages(flow: &ColumnFlow) -> Vec<FilledPage> {
    let section = flow.section;
    let grid = Grid::new(&section.columns, flow.w);
    let mut pending = Pending::new(flow.nodes);
    let mut last_placed = !section.insets.iter().any(|inset| inset.page == PageScope::Last);
    let mut pages: Vec<FilledPage> = Vec::new();
    // An empty flow is one empty page, as in one column.
    while pages.is_empty() || !(pending.is_empty() || crate::memory::passed()) {
        let index = pages.len();
        let budget = if index == 0 { flow.budget_first } else { flow.budget_rest };
        let at = flow.start.map(|start| start.on_page(index == 0));
        let number = flow.first_number + index as u64;
        let mut here: Vec<usize> = (0..section.insets.len())
            .filter(|&i| on_page(&section.insets[i].page, index, number))
            .collect();
        let before = pending.clone();
        let mut filled = fill_page(flow, &grid, &mut pending, &here, at, budget);
        if pending.is_empty() && !last_placed {
            // The last page of the flow filled without the insets on `last`, filled again with them.
            last_placed = true;
            here.extend((0..section.insets.len()).filter(|&i| section.insets[i].page == PageScope::Last));
            here.sort_unstable();
            pending = before.clone();
            filled = fill_page(flow, &grid, &mut pending, &here, at, budget);
        }
        if pending.is_empty() && section.columns.fill == ColumnFill::Balance && grid.count > 1 {
            filled = balance(flow, &grid, &before, &here, filled, budget);
        }
        refuse_insets_below(section, &here, budget, index);
        pages.push(finish_fill(flow, &grid, filled, here, at, budget));
    }
    pages
}

/// An inset of `here` that reaches below the room the headers and footers leave the flow of its page, `budget`,
/// is an error: it is never cut, and the template sets both heights.
fn refuse_insets_below(section: &SectionLayout, here: &[usize], budget: f32, index: usize) {
    for inset in here.iter().map(|&i| &section.insets[i]) {
        let reach = inset.at.top + inset.at.height;
        if reach > budget + 0.5 {
            refuse(format!(
                "{}: the inset reaches {} below the top of the columns, past the {} the header and footer leave on page {} of its section; give it a smaller top or height",
                inset.at.place, pt(reach), pt(budget), index + 1,
            ));
        }
    }
}

/// A length for an error: in points, to two places at most.
fn pt(v: f32) -> String {
    format!("{}pt", (v * 100.0).round() / 100.0)
}

/// Fill the next page of `flow` around the insets `here`, keeping back the room its notes need, and take what it
/// holds off `pending`.
fn fill_page<'a>(flow: &ColumnFlow, grid: &Grid, pending: &mut Pending<'a>, here: &[usize], at: Option<(f32, f32)>, budget: f32) -> PageFill {
    let section = flow.section;
    let insets: Vec<&Inset> = here.iter().map(|&i| &section.insets[i]).collect();
    if !flow.has_notes {
        let parts = fill_parts(pending, grid, &pieces(grid, &insets, budget, budget), at, budget, section.gap, None);
        return PageFill { parts, notes: Vec::new(), reserve: 0.0 };
    }
    // The notes' area doesn't rise above the foot of an inset.
    let lowest = insets.iter().map(|inset| inset.at.top + inset.at.height).fold(0.0_f32, f32::max);
    let most = (budget - lowest).max(0.0);
    let counter = section.footnote_counter;
    let before = pending.clone();
    let (parts, notes, reserve) = footnotes::settle(pending, flow.w, budget, |pending, reserve| {
        let mut rest = pending.clone();
        let room = budget - reserve.min(most);
        let mut parts = fill_parts(&mut rest, grid, &pieces(grid, &insets, room, room), at, budget, section.gap, None);
        let notes = take_notes(&mut parts, grid, counter, section.gap);
        let area = footnotes::area_height(&notes, flow.w, budget);
        footnotes::Fill { rest, page: parts, notes, area }
    });
    if footnotes::area_height(&notes, flow.w, budget) <= most + 0.01 {
        return PageFill { parts, notes, reserve: reserve.min(most) };
    }
    give_way(flow, grid, pending, &before, &insets, at, budget, most, parts, notes)
}

/// The page of `parts` and `notes` whose notes would rise above the foot of an inset, which leaves them `most` of
/// height, filled again from `before` so that it ends before the line that calls the first note that doesn't fit:
/// that line goes on to the next page with its note.
#[allow(clippy::too_many_arguments)]
fn give_way<'a>(
    flow: &ColumnFlow, grid: &Grid, pending: &mut Pending<'a>, before: &Pending<'a>, insets: &[&Inset],
    at: Option<(f32, f32)>, budget: f32, most: f32, parts: Vec<Part>, notes: Vec<footnotes::Note>,
) -> PageFill {
    let section = flow.section;
    let fit = (0..notes.len()).rev()
        .find(|&k| footnotes::area_height(&notes[..k], flow.w, budget) <= most + 0.01)
        .unwrap_or(0);
    let index = notes[fit].index;
    let mut cut = pieces(grid, insets, budget - most, budget - most);
    let found = parts.iter().find_map(|part| {
        let room = part.piece.bottom - part.piece.top;
        mark_top(&part.nodes, grid.w, room, section.gap, index).map(|y| (part.piece.col, part.piece.top, y))
    });
    if let Some((col, top, y)) = found {
        // The pieces before the one that holds the line, and that one down to the line, which a piece that is too
        // short to be one with the room the notes leave doesn't hold.
        let n = cut.iter().position(|p| p.col > col || (p.col == col && p.top > top - 0.01)).unwrap_or(cut.len());
        let same = cut.get(n).is_some_and(|p| p.col == col && (p.top - top).abs() < 0.01);
        cut.truncate(n + usize::from(same));
        if same {
            cut[n].h = y.min(cut[n].h);
            cut[n].whole = false;
        }
    }
    drop(parts);
    let mut rest = before.clone();
    let mut parts = fill_parts(&mut rest, grid, &cut, at, budget, section.gap, None);
    let notes = take_notes(&mut parts, grid, section.footnote_counter, section.gap);
    *pending = rest;
    PageFill { parts, notes, reserve: most }
}

/// The top of the number of the footnote `index` in `nodes`, laid out in a room `w` wide and `room` high, from the
/// top of it; `None` when it isn't among them.
fn mark_top(nodes: &[Node], w: f32, room: f32, gap: f32, index: u32) -> Option<f32> {
    fn find(tree: &[RenderNode], index: u32) -> Option<f32> {
        tree.iter().find_map(|node| match node {
            RenderNode::FootnoteMark(mark) if mark.index == index => Some(mark.y),
            RenderNode::Box(b)         => find(&b.children, index),
            RenderNode::Link(l)        => find(&l.children, index),
            RenderNode::CanvasLayer(l) => find(&l.children, index),
            _ => None,
        })
    }
    if !footnotes::has_footnotes(nodes) {
        return None;
    }
    let (tree, _) = layout_column(nodes, 0.0, 0.0, w, Some(room), gap, &Align::Stretch, &Justify::Start);
    find(&tree, index)
}

/// The notes of the anchors in `parts`, in reading order: column by column, as one page of notes.
fn take_notes(parts: &mut [Part], grid: &Grid, counter: FootnoteCounter, gap: f32) -> Vec<footnotes::Note> {
    let mut notes = Vec::new();
    let mut count = 0;
    for part in parts.iter_mut() {
        // Only a part cut at its bottom can hide a number, and only one whose first node did not fit is cut.
        let cut_off = if part.runs_past && footnotes::has_footnotes(&part.nodes) {
            footnotes::numbers_cut_off(&part.nodes, grid.w, part.piece.bottom - part.piece.top, gap)
        } else {
            Vec::new()
        };
        footnotes::take_notes_into(&mut part.nodes, counter, &cut_off, &mut count, &mut notes);
        if counter == FootnoteCounter::Page {
            footnotes::forget_numbered_trees(&part.nodes, &mut part.kept);
        }
    }
    notes
}

/// Fill `pieces` in turn from `pending`, each with `fill_room` at the width of a column, `col_h` being the height of
/// a whole column, which keep-next looks at. A stop that ends a page, a break or a keep-next that moves on, ends the
/// column: its pieces after it stay empty. A piece of a column an inset cuts leaves a node that doesn't fit it for
/// the piece after it, but the last piece of a page that holds nothing yet places it anyway, so that every page takes
/// something off the flow. `trial` is the room of a balanced page being levelled: a whole column then places a node
/// that doesn't fit only when it is taller than that room, as the page itself would.
fn fill_parts(pending: &mut Pending, grid: &Grid, pieces: &[Piece], at: Option<(f32, f32)>, col_h: f32, gap: f32, trial: Option<f32>) -> Vec<Part> {
    let mut parts: Vec<Part> = Vec::new();
    let mut ended: Option<usize> = None;
    let mut started: Option<usize> = None;
    for (slot, piece) in pieces.iter().enumerate() {
        if pending.is_empty() {
            break;
        }
        if ended == Some(piece.col) {
            continue;
        }
        let page_empty = parts.iter().all(|part| part.nodes.is_empty());
        let overflow = match trial {
            Some(room) if piece.whole => Overflow::PlaceOver(room),
            Some(_)                   => Overflow::Leave,
            None if piece.whole || (slot + 1 == pieces.len() && page_empty) => Overflow::Place,
            None                      => Overflow::Leave,
        };
        // A part of a column, or a trial height of a whole one, is not the page a node too tall for it is placed on.
        let page_h = match trial {
            Some(room) => Some(room),
            None       => (!piece.whole).then_some(col_h),
        };
        let rule = FillRule { overflow, follows: started == Some(piece.col), page_h };
        let place = at.map(|(x, y)| (x + grid.x(piece.col), y + piece.top));
        let filled = fill_room(pending, place, grid.w, piece.h, col_h, gap, rule);
        if filled.stop == Stop::Break {
            ended = Some(piece.col);
        }
        if !filled.nodes.is_empty() {
            started = Some(piece.col);
        }
        let (nodes, kept) = filled.nodes.into_iter().unzip();
        parts.push(Part { piece: *piece, nodes, kept, runs_past: filled.runs_past });
    }
    parts
}

/// The last page of a balanced section, `filled` from `before` around the insets `here`, levelled: filled again to
/// the least height at which its columns hold all of what is left, if one below its room does. Its notes are the
/// same, so the room kept back for them is too.
fn balance(flow: &ColumnFlow, grid: &Grid, before: &Pending, here: &[usize], filled: PageFill, budget: f32) -> PageFill {
    let section = flow.section;
    let insets: Vec<&Inset> = here.iter().map(|&i| &section.insets[i]).collect();
    let room = budget - filled.reserve;
    let fits = |h: f32| {
        let mut rest = before.clone();
        let parts = fill_parts(&mut rest, grid, &pieces(grid, &insets, h, room), None, budget, section.gap, Some(room));
        rest.is_empty().then_some(parts)
    };
    let (mut low, mut high) = (0.0_f32, room);
    let mut best: Option<Vec<Part>> = None;
    for _ in 0..BALANCE_TRIES {
        let mid = (low + high) / 2.0;
        match fits(mid) {
            Some(parts) => {
                best = Some(parts);
                high = mid;
            }
            None => low = mid,
        }
    }
    match best {
        Some(mut parts) => {
            let notes = take_notes(&mut parts, grid, section.footnote_counter, section.gap);
            PageFill { parts, notes, reserve: filled.reserve }
        }
        None => filled,
    }
}

/// A page in columns as `fill_pages` returns it: its flow laid out when where it starts is known, and the insets on
/// it, which `draw_insets` lays out once the page's number is known.
fn finish_fill(flow: &ColumnFlow, grid: &Grid, filled: PageFill, here: Vec<usize>, at: Option<(f32, f32)>, budget: f32) -> FilledPage {
    let mut captured = PageCaptures::new();
    for part in &filled.parts {
        for node in &part.nodes {
            collect_captures(node, &mut captured);
        }
    }
    let laid = match at {
        Some((x, y)) => PageFlow::Laid { nodes: lay_out(flow.section, grid, filled.parts, &here, x, y), y },
        None         => PageFlow::Columns(ColumnParts(filled.parts)),
    };
    FilledPage { flow: laid, captured, notes: filled.notes, room: budget - filled.reserve, insets: here }
}

/// The parts of a page in columns that wait for the header above them to be laid out (`lay_out`).
pub(super) struct ColumnParts(Vec<Part>);

/// Lay out the parts of a page of `section` in columns whose flow starts at `x`, `y`, and the rule between its
/// columns. `here` are the insets on the page, which the rule skips.
pub(super) fn lay_out_parts(section: &SectionLayout, parts: ColumnParts, here: &[usize], x: f32, y: f32) -> Vec<RenderNode> {
    let [_, mr, _, ml] = section.margin;
    let grid = Grid::new(&section.columns, (section.width - ml - mr).max(0.0));
    lay_out(section, &grid, parts.0, here, x, y)
}

fn lay_out(section: &SectionLayout, grid: &Grid, parts: Vec<Part>, here: &[usize], x: f32, y: f32) -> Vec<RenderNode> {
    let mut out = Vec::new();
    // How far down each column holds something, from the top of the flow.
    let mut foot: Vec<Option<f32>> = vec![None; grid.count];
    for part in parts {
        if part.nodes.is_empty() {
            continue;
        }
        let room = part.piece.bottom - part.piece.top;
        let (px, py) = (x + grid.x(part.piece.col), y + part.piece.top);
        let (nodes, used) = layout_column_kept(
            &part.nodes, part.kept, px, py, grid.w, Some(room),
            section.gap, &Align::Stretch, &Justify::Start,
        );
        out.extend(clip_overflow(nodes, used, py + room, room, section.width));
        let bottom = part.piece.top + used.min(room);
        let col = &mut foot[part.piece.col];
        *col = Some(col.map_or(bottom, |f| f.max(bottom)));
    }
    if let Some((thickness, color)) = &section.columns.rule {
        let insets: Vec<&Inset> = here.iter().map(|&i| &section.insets[i]).collect();
        out.extend(rules(grid, &foot, &insets, x, y, *thickness, color));
    }
    out
}

/// The rule in the middle of each gap between two columns that hold something, from the top of the flow at `x`, `y`
/// to the foot of the tallest column of the page, `foot` saying how far down each holds something. It skips each
/// inset across the gap.
fn rules(grid: &Grid, foot: &[Option<f32>], insets: &[&Inset], x: f32, y: f32, thickness: f32, color: &str) -> Vec<RenderNode> {
    let tallest = foot.iter().flatten().fold(0.0_f32, |a, &b| a.max(b));
    let mut out = Vec::new();
    for gap in 0..grid.count.saturating_sub(1) {
        if foot[gap].is_none() || foot[gap + 1].is_none() {
            continue;
        }
        let line_x = x + grid.x(gap) + grid.w + grid.gap / 2.0;
        let mut cuts: Vec<(f32, f32)> = insets.iter()
            .filter(|inset| inset.at.col <= gap && gap + 1 < inset.at.col + inset.at.span)
            .map(|inset| (inset.at.top, inset.at.top + inset.at.height))
            .collect();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut top = 0.0_f32;
        for (start, end) in cuts.into_iter().chain(std::iter::once((tallest, tallest))) {
            let bottom = start.min(tallest);
            if bottom - top > 0.5 {
                out.push(RenderNode::Line(RenderLine {
                    x1: line_x, y1: y + top, x2: line_x, y2: y + bottom,
                    color: color.to_string(), thickness, dash: None,
                }));
            }
            top = top.max(end);
        }
    }
    out
}

/// Lay out the insets `here` of a page of `section` whose flow starts at `x`, `y`, with the page's numbers in their
/// texts (`tokens`). Content taller than an inset is clipped at its foot.
pub(super) fn draw_insets(section: &SectionLayout, insets: &[Inset], here: &[usize], x: f32, y: f32, tokens: PageTokens) -> Vec<RenderNode> {
    let [_, mr, _, ml] = section.margin;
    let grid = Grid::new(&section.columns, (section.width - ml - mr).max(0.0));
    let mut out = Vec::new();
    for inset in here.iter().map(|&i| &insets[i]) {
        let mut node = substitute_page_tokens(&inset.node, tokens);
        fill_sinks(&mut node, &PageCaptures::new(), &HashMap::new());
        let (ix, iy) = (x + grid.x(inset.at.col), y + inset.at.top);
        let (w, h) = (grid.span_w(inset.at.span), inset.at.height);
        let taller = measure_height(&node, w, h) > h + 0.5;
        node.height_mode = HeightMode::Fixed(h);
        let (mut nodes, _) = layout_column(std::slice::from_ref(&node), ix, iy, w, Some(h), 0.0, &Align::Stretch, &Justify::Start);
        if taller {
            fit_to_clip(&mut nodes, iy + h);
            nodes = vec![RenderNode::CanvasLayer(RenderCanvasLayer {
                tag: None, transform: None, opacity: 1.0, children: nodes,
                clip: Some(RenderCanvasClip { x: ix, y: iy, w, h, border_radius: 0.0 }),
            })];
        }
        out.extend(nodes);
    }
    out
}
