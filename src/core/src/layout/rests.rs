//! What is left of a long container that pages cut, and the window of it each page splits.
//!
//! A column, a list, a table, a grid or a row longer than a page is split on each page it runs over: the part that
//! fits stays, and what is left goes on to the next. Copied whole for each page, and laid out whole to find that it
//! didn't fit, what was left made the time of a long container grow with the square of its children. It is kept as a
//! `Rest` instead, whose children no page has reached yet are shared and not copied, and a page splits a window of it
//! (`Rest::window`): a copy of the children the page can reach, which the split decides on as it would on all of them.

use std::rc::Rc;

use super::{
    col_units, line_widths, measure_height, own_width, reads_given_height, row_child_w, splittable, table_col_widths,
    Align, Bulk, ColUnit, HeightMode, Node, NodeKind, Paginate, RowHeights, RowKeep, TableGrid, MIN_NESTING_SIZE,
};

/// How many children a container has left at most for its rest to go on as a node, copied for each page: a rest
/// this short costs a page little, and a window of it would hold it whole.
const SHORT: usize = 32;

#[cfg(test)]
thread_local! {
    /// Whether a rest is split whole, as pages split all of what was left of a container before windows: the tests
    /// lay a document out both ways and compare.
    pub(super) static WHOLE_RESTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// How many windows that leave children behind were made, counted for the tests.
    pub(super) static WINDOWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A node of the flow waiting for a page: a node, what is left of a long container, or what is left of a bulk
/// container (`bulk`). The rests are boxed, so that a node, which every page takes many of, moves no more than it did.
#[derive(Clone)]
pub(super) enum Waiting {
    Node(Node),
    Rest(Box<Rest>),
    Bulk(Box<Bulk>),
}

impl Waiting {
    /// A node of the flow as it starts to wait: a bulk container whose records are in as a `Bulk`, any other as it is.
    pub(super) fn of(node: Node) -> Waiting {
        match node.bulk_records().cloned() {
            Some(records) => Waiting::Bulk(Box::new(Bulk::new(node, records))),
            None => Waiting::Node(node),
        }
    }

    /// What is left of a node a page split, as it waits for the next: a long container as a `Rest`, and anything
    /// else as it is.
    pub(super) fn left(node: Node) -> Waiting {
        if goes_on_as_rest(&node) { Waiting::Rest(Box::new(Rest::new(node))) } else { Waiting::Node(node) }
    }

    /// The node, or the container of a rest: what pagination reads of it before splitting it.
    pub(super) fn node(&self) -> &Node {
        match self {
            Waiting::Node(node) => node,
            Waiting::Rest(rest) => &rest.shell,
            Waiting::Bulk(bulk) => bulk.shell(),
        }
    }

    /// The same, with `paginate` set.
    pub(super) fn paginated(self, paginate: Paginate) -> Waiting {
        match self {
            Waiting::Node(node) => Waiting::Node(Node { paginate, ..node }),
            Waiting::Rest(mut rest) => {
                rest.shell.paginate = paginate;
                Waiting::Rest(rest)
            }
            Waiting::Bulk(mut bulk) => {
                bulk.shell_mut().paginate = paginate;
                Waiting::Bulk(bulk)
            }
        }
    }

    /// The node it stands for, a rest with every child left.
    fn into_node(self) -> Node {
        match self {
            Waiting::Node(node) => node,
            Waiting::Rest(rest) => rest.into_node(),
            Waiting::Bulk(bulk) => bulk.into_node(),
        }
    }
}

/// Whether what is left of `node` goes on as a `Rest`: a container a window can be split in place of, with more
/// children left than `SHORT`, or with a child it leads with that goes on as one (`lead_goes_on_as_rest`). A table
/// whose columns are sized by its cells doesn't, since a window of its rows would size them otherwise: the part a
/// page leaves of it has its columns fixed.
pub(super) fn goes_on_as_rest(node: &Node) -> bool {
    let windowed = splittable(node)
        && match node.kind {
            NodeKind::Column | NodeKind::Link | NodeKind::Grid | NodeKind::Row => true,
            NodeKind::Table => !col_units(&node.table_cols).contains(&ColUnit::Auto),
            _ => false,
        };
    windowed
        && (node.children.len() > SHORT
            || match node.kind {
                NodeKind::Row => node.children.iter().any(|child| lead_goes_on_as_rest(node, child)),
                _ => has_leads(node) && node.children.first().is_some_and(|child| lead_goes_on_as_rest(node, child)),
            })
}

/// Whether a page can cut through children of `node` and leave what is left of them first in what is left of
/// `node`: the first child of a column or a link, and the children of a line of a row. A table and a grid are cut
/// between their rows.
fn has_leads(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Column | NodeKind::Link | NodeKind::Row)
}

/// Whether `child`, which `node` leads with, goes on as a rest of its own. A row's child must set its width or fill:
/// the row breaks its lines by the widths of its children, and that of any other child comes from all of its own
/// children (`row_child_w`). What is left of a child a page cut keeps the width its line gave it.
fn lead_goes_on_as_rest(node: &Node, child: &Node) -> bool {
    (node.kind != NodeKind::Row || child.width_fill || own_width(child).is_some()) && goes_on_as_rest(child)
}

/// `child` as a lead of `node`: a rest when it goes on as one.
fn as_lead(node: &Node, child: Node) -> Waiting {
    if lead_goes_on_as_rest(node, &child) { Waiting::Rest(Box::new(Rest::new(child))) } else { Waiting::Node(child) }
}

/// What is left of a long container a page cut: the container without its children, `shell`, and its children left.
/// These are `leads`, the children it leads with, each a node or what is left of a long container as a rest of its
/// own: the first child of a column or a link, and for a row the children a page left of its window, which its next
/// lines start with. Then `head`, children a page copied and left, then those of `tail` from `from` on, which no page
/// has reached and every copy of the rest shares.
#[derive(Clone)]
pub(super) struct Rest {
    shell: Node,
    leads: Vec<Waiting>,
    head:  Vec<Node>,
    tail:  Rc<Vec<Node>>,
    from:  usize,
}

/// A window of a rest (`Rest::window`).
enum Window {
    /// It would hold every child left: the rest is split whole (`Rest::into_node`).
    Whole,
    /// The container with the children a page can reach, how far into the rest they go, and whether it is known to be
    /// taller than `reach` laid out: when the window measured its children at the heights the layout lays them out at
    /// (`sized_alike`), the heights that pass `reach` are those.
    Part { node: Node, reached: Reached, tall: bool },
}

/// Whether the layout of a column, or of a grid's row, gives `child` the height a split measures it at
/// (`measure_height`): a set height, or one of `auto` that it doesn't take from the height it is given.
fn sized_alike(child: &Node) -> bool {
    match child.height_mode {
        HeightMode::Fixed(_) => true,
        HeightMode::Auto => !reads_given_height(child),
        _ => false,
    }
}

/// How far a window reached into a rest: for each lead it holds, how far the window of it reached when that leaves
/// children behind, then the children of its `head` and of its `tail` it copied.
pub(super) struct Reached {
    leads: Vec<Option<Reached>>,
    head:  usize,
    tail:  usize,
}

impl Reached {
    /// Which child of the window the first of the `left` children a split left of it stands for. What a split leaves
    /// of a window is its last children, the first of them what is left of those it cut through (`split_column`,
    /// `split_row`). A table's starts with its header again, and keeps a row for it, so it is no longer.
    fn first_left(&self, left: usize) -> usize {
        let held = self.leads.len() + self.head + self.tail;
        debug_assert!(left <= held, "a split leaves no more children than its window holds");
        held.saturating_sub(left)
    }

    /// Whether the page cut through each lead the window held a window of, which leaves children behind: what the
    /// split left of the window holds a part of each. A window of a lead is taller than any room, but a page can
    /// still take it whole, cut nowhere, as `cut_child` does a child of a row's line that can't be split where the
    /// line is cut, such as a column whose first child is taller than a page: the page takes all of that child's
    /// rest then, past its foot, and a window of it would leave the children it didn't reach for the next page.
    pub(super) fn cuts_leads(&self, left: Option<&Node>) -> bool {
        let left = left.map_or(&[][..], |left| left.children.as_slice());
        let first = self.first_left(left.len());
        self.leads.iter().enumerate().filter(|(_, lead)| lead.is_some()).all(|(i, _)| {
            i.checked_sub(first).and_then(|i| left.get(i)).is_some_and(|part| !part.children.is_empty())
        })
    }
}

/// A child of a rest as its window reads it: a lead, or a child after them.
#[derive(Clone, Copy)]
enum Child<'a> {
    Lead(&'a Waiting),
    Node(&'a Node),
}

impl Child<'_> {
    /// The node, or the container of a rest: what sets the width a row gives it.
    fn node(&self) -> &Node {
        match self {
            Child::Lead(lead) => lead.node(),
            Child::Node(node) => node,
        }
    }
}

/// The window of `lead` laid out `width` wide on a page `page_h` high: the node, how far it reached when it leaves
/// children behind, and its height measured as a split measures it. A window that leaves children behind is taller
/// than `reach` by itself, and isn't measured. `tall` stays true only while the layout measures it alike.
fn lead_window(lead: &Waiting, width: f32, reach: f32, page_h: f32, tall: &mut bool) -> (Node, Option<Reached>, f32) {
    match lead {
        Waiting::Rest(rest) => match rest.window(width, reach, page_h) {
            Window::Part { node, reached, tall: lead_tall } => {
                *tall &= lead_tall;
                (node, Some(reached), f32::INFINITY)
            }
            Window::Whole => {
                let node = (**rest).clone().into_node();
                *tall &= sized_alike(&node);
                let height = measure_height(&node, width, page_h);
                (node, None, height)
            }
        },
        Waiting::Node(node) => {
            *tall &= sized_alike(node);
            (node.clone(), None, measure_height(node, width, page_h))
        }
        // A bulk container stands directly in the layout, so it leads nothing: should it, it is all of its children.
        Waiting::Bulk(bulk) => {
            let node = (**bulk).clone().into_node();
            *tall &= sized_alike(&node);
            let height = measure_height(&node, width, page_h);
            (node, None, height)
        }
    }
}

/// A window of a row being made, line by line: the nodes it holds, how far the window of each lead it holds reached,
/// and whether the layout measures it alike (`lead_window`).
struct Held {
    children: Vec<Node>,
    leads:    Vec<Option<Reached>>,
    tall:     bool,
}

impl Held {
    /// Hold the children of `line` of a row `inner_w` wide inside, each as wide as the line gives it, on a page
    /// `page_h` high, and give the height of the line: that of its tallest child.
    fn line(&mut self, line: &[(Child, Option<f32>)], inner_w: f32, gap: f32, reach: f32, page_h: f32) -> f32 {
        let widths: Vec<Option<f32>> = line.iter().map(|&(_, w)| w).collect();
        let mut line_h = 0.0_f32;
        for (&(child, _), width) in line.iter().zip(line_widths(&widths, inner_w, gap)) {
            let (node, h) = match child {
                Child::Lead(lead) => {
                    let (node, reached, h) = lead_window(lead, width, reach, page_h, &mut self.tall);
                    self.leads.push(reached);
                    (node, h)
                }
                Child::Node(node) => (node.clone(), measure_height(node, width, page_h)),
            };
            line_h = line_h.max(h);
            self.children.push(node);
        }
        line_h
    }
}

impl Rest {
    pub(super) fn new(mut node: Node) -> Rest {
        let mut tail = std::mem::take(&mut node.children);
        let mut leads = Vec::new();
        let mut from = 0;
        if node.kind == NodeKind::Row && tail.len() <= SHORT {
            // A row this short goes on as a rest for a child it leads with: it leads with all of them.
            leads = tail.drain(..).map(|child| as_lead(&node, child)).collect();
        } else if has_leads(&node) && tail.first().is_some_and(|child| lead_goes_on_as_rest(&node, child)) {
            from = 1;
            // The place it leaves is never read: the rest's children start after it.
            let first = std::mem::replace(&mut tail[0], Node::layout_default());
            leads.push(Waiting::Rest(Box::new(Rest::new(first))));
        }
        Rest { shell: node, leads, head: Vec::new(), tail: Rc::new(tail), from }
    }

    /// The number of children left.
    fn len(&self) -> usize {
        self.leads.len() + self.head.len() + self.tail.len() - self.from
    }

    /// The children left after the leads.
    fn after_leads(&self) -> impl Iterator<Item = &Node> {
        self.head.iter().chain(&self.tail[self.from..])
    }

    /// The children left, as a window reads them.
    fn children(&self) -> impl Iterator<Item = Child<'_>> {
        self.leads.iter().map(Child::Lead).chain(self.after_leads().map(Child::Node))
    }

    /// The node it stands for: the container with every child left.
    pub(super) fn into_node(self) -> Node {
        let mut children = Vec::with_capacity(self.len());
        children.extend(self.leads.into_iter().map(Waiting::into_node));
        children.extend(self.head);
        match Rc::try_unwrap(self.tail) {
            Ok(mut tail) => children.extend(tail.drain(self.from..)),
            Err(tail) => children.extend_from_slice(&tail[self.from..]),
        }
        Node { children, ..self.shell }
    }

    /// The container with the children a split of it `avail_w` wide, on a page `page_h` high, can reach in `reach` of
    /// height: the first children whose heights, measured as the split measures them, add up past `reach`, and at
    /// least two. The split cuts its first part where the heights pass the room it is given, which is less than
    /// `reach`, and moves a part on whole only when it is the only child left, so it decides on the window as it
    /// would on every child left. A lead that goes on as a rest is the window of it, which leaves children behind, as
    /// the window does.
    ///
    /// A container too narrow to hold containers splits as it is laid out, without them (`without_inner_containers`),
    /// which can drop its leads: it is split whole.
    fn window(&self, avail_w: f32, reach: f32, page_h: f32) -> Window {
        #[cfg(test)]
        if WHOLE_RESTS.with(std::cell::Cell::get) {
            return Window::Whole;
        }
        let width = own_width(&self.shell).map_or(avail_w, |w| w.min(avail_w));
        if width < MIN_NESTING_SIZE {
            return Window::Whole;
        }
        let [pt, pr, pb, pl] = self.shell.padding;
        let inner_w    = (avail_w - pl - pr).max(0.0);
        let inner_full = (page_h - pt - pb).max(0.0);
        let window = match self.shell.kind {
            NodeKind::Grid  => self.grid_window(inner_w, inner_full, reach),
            NodeKind::Table => self.table_window(inner_w, reach),
            NodeKind::Row   => self.row_window(inner_w, inner_full, reach),
            _               => self.column_window(inner_w, inner_full, reach),
        };
        match window {
            Some((children, reached, tall)) => {
                #[cfg(test)]
                WINDOWS.with(|windows| windows.set(windows.get() + 1));
                Window::Part { node: self.shell.with_children(children), reached, tall }
            }
            None => Window::Whole,
        }
    }

    /// `window` for a column or a link, measured as `split_column` measures them. The layout measures them alike when
    /// the container is as wide as the room, which a split takes it to be, and stretches each child to its width.
    fn column_window(&self, inner_w: f32, inner_full: f32, reach: f32) -> Option<(Vec<Node>, Reached, bool)> {
        let mut children = Vec::new();
        let mut leads = Vec::new();
        let mut height = 0.0_f32;
        let mut tall = own_width(&self.shell).is_none()
            && (self.shell.kind == NodeKind::Link || matches!(self.shell.align, Align::Stretch));
        for lead in &self.leads {
            let (node, reached, h) = lead_window(lead, inner_w, reach, inner_full, &mut tall);
            height += if children.is_empty() { h } else { self.shell.gap + h };
            children.push(node);
            leads.push(reached);
        }
        let mut taken = 0;
        for child in self.after_leads() {
            if height > reach && children.len() + taken >= 2 {
                break;
            }
            let gap = if children.is_empty() && taken == 0 { 0.0 } else { self.shell.gap };
            height += gap + measure_height(child, inner_w, inner_full);
            tall &= sized_alike(child);
            taken += 1;
        }
        let reached = self.reached(leads, taken)?;
        children.extend(self.after_leads().take(taken).cloned());
        Some((children, reached, tall))
    }

    /// `window` for a row, line by line, at least two: the lines `split_row` breaks the children into (`row_lines`),
    /// which come out the same for the first children as for all of them, each as tall as its tallest child. A lead
    /// that goes on as a rest is the window of it, as wide as its line gives it.
    fn row_window(&self, inner_w: f32, inner_full: f32, reach: f32) -> Option<(Vec<Node>, Reached, bool)> {
        let gap = self.shell.gap;
        // The layout can lay a line out otherwise than a split measures it (`layout_row_line_spread`): a window is
        // laid out to learn its height.
        let mut held = Held { children: Vec::new(), leads: Vec::new(), tall: false };
        let mut height = 0.0_f32;
        let mut lines = 0;
        let mut line: Vec<(Child, Option<f32>)> = Vec::new();
        let mut line_x = 0.0_f32;
        for child in self.children() {
            let w = row_child_w(child.node(), inner_w);
            let cw = w.unwrap_or(0.0);
            if !line.is_empty() && line_x + gap + cw > inner_w + 0.01 {
                let line_h = held.line(&line, inner_w, gap, reach, inner_full);
                height += if lines == 0 { line_h } else { gap + line_h };
                lines += 1;
                line.clear();
                if height > reach && lines >= 2 {
                    break;
                }
            }
            line_x = if line.is_empty() { cw } else { line_x + gap + cw };
            line.push((child, w));
        }
        if !line.is_empty() {
            held.line(&line, inner_w, gap, reach, inner_full);
        }
        let taken = held.children.len() - held.leads.len();
        let reached = self.reached(held.leads, taken)?;
        Some((held.children, reached, held.tall))
    }

    /// `window` for a grid, row by row, measured as `split_grid` measures them, and as the layout measures them when
    /// the grid is as wide as the room.
    fn grid_window(&self, inner_w: f32, inner_full: f32, reach: f32) -> Option<(Vec<Node>, Reached, bool)> {
        let gap  = self.shell.gap;
        let cols = match self.shell.col_width {
            Some(min_w) => (((inner_w + gap) / (min_w + gap)).floor() as usize).max(1),
            None        => (self.shell.cols as usize).max(1),
        };
        let col_w = ((inner_w - gap * (cols - 1) as f32) / cols as f32).max(0.0);
        let mut tall = own_width(&self.shell).is_none();
        let mut height = 0.0_f32;
        let mut rows = 0;
        let mut taken = 0;
        let mut left = self.after_leads();
        while !(height > reach && rows >= 2) {
            let mut row_h = 0.0_f32;
            let mut cells = 0;
            for cell in left.by_ref().take(cols) {
                row_h = row_h.max(measure_height(cell, col_w, inner_full));
                tall &= sized_alike(cell);
                cells += 1;
            }
            if cells == 0 {
                break;
            }
            height += if rows == 0 { row_h } else { gap + row_h };
            rows += 1;
            taken += cells;
        }
        let reached = self.reached(Vec::new(), taken)?;
        Some((self.after_leads().take(taken).cloned().collect(), reached, tall))
    }

    /// `window` for a table, measured as `split_table` measures its rows: with their columns, which the window sizes
    /// as the rest does, since they are fixed, and the cells that span rows, which it holds whole. Where the table
    /// has rows that keep with the next one, or cells that span rows, which `split_table` moves on together, the
    /// window reaches twice as far: a run that starts above the page's edge, a page high, ends in it. The layout
    /// measures the rows alike when the table is as wide as the room.
    fn table_window(&self, inner_w: f32, reach: f32) -> Option<(Vec<Node>, Reached, bool)> {
        let gap  = self.shell.gap;
        let left = self.len();
        let mut n = left.min(2 * SHORT);
        loop {
            let rows: Vec<Node> = self.after_leads().take(n).cloned().collect();
            let held = rows.iter().any(|row| {
                row.keep == RowKeep::WithNext || row.children.iter().any(|cell| cell.row_span > 1)
            });
            let closed = rows.iter().enumerate().all(|(r, row)| {
                row.children.iter().all(|cell| r + usize::from(cell.row_span.max(1)) <= n)
            });
            let height = {
                let col_widths = table_col_widths(&rows, &self.shell.table_cols, inner_w, gap);
                let grid = TableGrid::new(&rows, col_widths.len());
                let mut heights = RowHeights::new(&rows, &grid, &col_widths, gap);
                (0..n).fold(0.0_f32, |sum, i| if i == 0 { heights.get(i) } else { sum + gap + heights.get(i) })
            };
            let reaches = height > if held { 2.0 * reach } else { reach };
            if n == left || (reaches && closed && n >= 3) {
                let reached = self.reached(Vec::new(), n)?;
                return Some((rows, reached, own_width(&self.shell).is_none()));
            }
            n = (2 * n).min(left);
        }
    }

    /// How far a window reached that holds the first leads, with how far each of their windows reached (`leads`),
    /// then the first `taken` children after the leads, or `None` when they are every child left, whole.
    fn reached(&self, leads: Vec<Option<Reached>>, taken: usize) -> Option<Reached> {
        if leads.len() == self.leads.len()
            && leads.iter().all(Option::is_none)
            && taken == self.head.len() + self.tail.len() - self.from
        {
            return None;
        }
        let head = taken.min(self.head.len());
        Some(Reached { leads, head, tail: taken - head })
    }

    /// What `fill_room` splits in place of the rest: a window of it, with the rest to go on from (`advance`) and
    /// whether the window is known to be taller than `reach` laid out, or else the rest whole.
    pub(super) fn to_split(
        self: Box<Rest>,
        avail_w: f32,
        reach: f32,
        page_h: f32,
    ) -> (Node, Option<(Box<Rest>, Reached)>, bool) {
        match self.window(avail_w, reach, page_h) {
            Window::Part { node, reached, tall } => (node, Some((self, reached)), tall),
            Window::Whole => (self.into_node(), None, false),
        }
    }

    /// The rest after a page split a window of it that `reached` this far, and left `left` of it: what is left of the
    /// window, then the children the window didn't reach. The page cut through each lead the window held a window of
    /// (`Reached::cuts_leads`), which goes on from what is left of that window, in its place.
    pub(super) fn advance(mut self, reached: Reached, left: Option<Node>) -> Rest {
        let mut node = left.unwrap_or_else(|| self.shell.with_children(Vec::new()));
        let children = std::mem::take(&mut node.children);
        let first = reached.first_left(children.len());
        let mut unreached = self.leads.into_iter();
        let mut windowed: Vec<Option<(Box<Rest>, Reached)>> = reached
            .leads
            .into_iter()
            .zip(unreached.by_ref())
            .map(|(reached, lead)| match (lead, reached) {
                (Waiting::Rest(rest), Some(reached)) => Some((rest, reached)),
                _ => None,
            })
            .collect();
        let mut leads = Vec::new();
        let mut head = Vec::new();
        for (i, child) in children.into_iter().enumerate() {
            if let Some((rest, reached)) = windowed.get_mut(first + i).and_then(Option::take) {
                leads.push(Waiting::Rest(Box::new(rest.advance(reached, Some(child)))));
            } else if node.kind == NodeKind::Row {
                leads.push(as_lead(&node, child));
            } else if leads.is_empty() && head.is_empty() && has_leads(&node) && lead_goes_on_as_rest(&node, &child) {
                leads.push(Waiting::Rest(Box::new(Rest::new(child))));
            } else {
                head.push(child);
            }
        }
        // A row's window may not reach all of its leads; a column's holds its one.
        leads.extend(unreached);
        head.extend(self.head.drain(reached.head..));
        Rest { shell: node, leads, head, tail: self.tail, from: self.from + reached.tail }
    }

    /// The node `fill_room` measures and splits in place of the rest, to see whether a node that keeps with it can
    /// stay with it: its window, when that is taller than `reach` laid out `avail_w` wide on a page `page_h` high, so
    /// that it neither fits where the rest doesn't nor stays where the rest moves on, or else the rest whole.
    pub(super) fn peek(&self, avail_w: f32, reach: f32, page_h: f32) -> Node {
        match self.window(avail_w, reach, page_h) {
            Window::Part { node, .. } if measure_height(&node, avail_w, page_h) > reach => node,
            _ => self.clone().into_node(),
        }
    }
}
