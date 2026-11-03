//! What is left of a long container that pages cut, and the window of it each page splits.
//!
//! A column, a list, a table or a grid longer than a page is split on each page it runs over: the part that fits
//! stays, and what is left goes on to the next. Copied whole for each page, and laid out whole to find that it didn't
//! fit, what was left made the time of a long container grow with the square of its children. It is kept as a `Rest`
//! instead, whose children no page has reached yet are shared and not copied, and a page splits a window of it
//! (`Rest::window`): a copy of the children the page can reach, which the split decides on as it would on all of them.

use std::rc::Rc;

use super::{
    col_units, measure_height, own_width, reads_given_height, splittable, table_col_widths, Align, ColUnit, HeightMode,
    Node, NodeKind, Paginate, RowHeights, RowKeep, TableGrid, MIN_NESTING_SIZE,
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

/// A node of the flow waiting for a page: a node, or what is left of a long container. The rest is boxed, so that a
/// node, which every page takes many of, moves no more than it did.
#[derive(Clone)]
pub(super) enum Waiting {
    Node(Node),
    Rest(Box<Rest>),
}

impl Waiting {
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
        }
    }
}

/// Whether what is left of `node` goes on as a `Rest`: a container a window can be split in place of, with more
/// children left than `SHORT`, or whose first child goes on as one. A table whose columns are sized by its cells
/// doesn't, since a window of its rows would size them otherwise: the part a page leaves of it has its columns fixed.
pub(super) fn goes_on_as_rest(node: &Node) -> bool {
    let windowed = splittable(node)
        && match node.kind {
            NodeKind::Column | NodeKind::Link | NodeKind::Grid => true,
            NodeKind::Table => !col_units(&node.table_cols).contains(&ColUnit::Auto),
            _ => false,
        };
    windowed && (node.children.len() > SHORT || (leads(node) && node.children.first().is_some_and(goes_on_as_rest)))
}

/// Whether a page can cut through the first child of `node` and leave what is left of it first in what is left of
/// `node`: a column's or a link's. A table and a grid are cut between their rows.
fn leads(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Column | NodeKind::Link)
}

/// What is left of a long container a page cut: the container without its children, `shell`, and its children left.
/// These are `lead`, what is left of the first child when it goes on as a rest too, then `head`, children a page
/// copied and left, then those of `tail` from `from` on, which no page has reached and every copy of the rest shares.
#[derive(Clone)]
pub(super) struct Rest {
    shell: Node,
    lead:  Option<Box<Rest>>,
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

/// How far a window reached into a rest: the children of its `head` and of its `tail` it copied, and the window of
/// its lead, which a window holds first.
pub(super) struct Reached {
    head: usize,
    tail: usize,
    lead: Option<Box<Reached>>,
}

impl Rest {
    pub(super) fn new(mut node: Node) -> Rest {
        let mut tail = std::mem::take(&mut node.children);
        let mut from = 0;
        let lead = if leads(&node) && tail.first().is_some_and(goes_on_as_rest) {
            from = 1;
            // The place it leaves is never read: the rest's children start after it.
            Some(Box::new(Rest::new(std::mem::replace(&mut tail[0], Node::layout_default()))))
        } else {
            None
        };
        Rest { shell: node, lead, head: Vec::new(), tail: Rc::new(tail), from }
    }

    /// The number of children left.
    fn len(&self) -> usize {
        usize::from(self.lead.is_some()) + self.head.len() + self.tail.len() - self.from
    }

    /// The children left after the lead.
    fn after_lead(&self) -> impl Iterator<Item = &Node> {
        self.head.iter().chain(&self.tail[self.from..])
    }

    /// The node it stands for: the container with every child left.
    pub(super) fn into_node(self) -> Node {
        let mut children = Vec::with_capacity(self.len());
        children.extend(self.lead.map(|lead| lead.into_node()));
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
    /// would on every child left. The lead is the window of it, which leaves children behind, as the window does.
    ///
    /// A container too narrow to hold containers splits as it is laid out, without them (`without_inner_containers`),
    /// which can drop its lead: it is split whole.
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

    /// `window` for a column or a link, measured as `split_column` measures them. A lead whose window leaves children
    /// behind is taller than `reach` by itself. The layout measures them alike when the container is as wide as the
    /// room, which a split takes it to be, and stretches each child to its width.
    fn column_window(&self, inner_w: f32, inner_full: f32, reach: f32) -> Option<(Vec<Node>, Reached, bool)> {
        let mut children = Vec::new();
        let mut lead = None;
        let mut height = 0.0_f32;
        let mut tall = own_width(&self.shell).is_none()
            && (self.shell.kind == NodeKind::Link || matches!(self.shell.align, Align::Stretch));
        if let Some(rest) = &self.lead {
            match rest.window(inner_w, reach, inner_full) {
                Window::Part { node, reached, tall: lead_tall } => {
                    children.push(node);
                    lead = Some(Box::new(reached));
                    height = f32::INFINITY;
                    tall &= lead_tall;
                }
                Window::Whole => {
                    let node = (**rest).clone().into_node();
                    height = measure_height(&node, inner_w, inner_full);
                    tall &= sized_alike(&node);
                    children.push(node);
                }
            }
        }
        let mut taken = 0;
        for child in self.after_lead() {
            if height > reach && children.len() + taken >= 2 {
                break;
            }
            let gap = if children.is_empty() && taken == 0 { 0.0 } else { self.shell.gap };
            height += gap + measure_height(child, inner_w, inner_full);
            tall &= sized_alike(child);
            taken += 1;
        }
        let reached = self.reached(taken, lead)?;
        children.extend(self.after_lead().take(taken).cloned());
        Some((children, reached, tall))
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
        let mut left = self.after_lead();
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
        let reached = self.reached(taken, None)?;
        Some((self.after_lead().take(taken).cloned().collect(), reached, tall))
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
            let rows: Vec<Node> = self.after_lead().take(n).cloned().collect();
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
                let reached = self.reached(n, None)?;
                return Some((rows, reached, own_width(&self.shell).is_none()));
            }
            n = (2 * n).min(left);
        }
    }

    /// How far a window reached that holds the lead's window, when `lead` says how far that reached, and the first
    /// `taken` children after it, or `None` when they are every child left.
    fn reached(&self, taken: usize, lead: Option<Box<Reached>>) -> Option<Reached> {
        if lead.is_none() && taken == self.head.len() + self.tail.len() - self.from {
            return None;
        }
        let head = taken.min(self.head.len());
        Some(Reached { head, tail: taken - head, lead })
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
    /// window, then the children the window didn't reach. A window that holds the window of the lead can't fit, so
    /// the page cut through the lead, and what is left of it is the first of what is left of the window.
    pub(super) fn advance(mut self, reached: Reached, left: Option<Node>) -> Rest {
        let mut node = left.unwrap_or_else(|| self.shell.with_children(Vec::new()));
        let mut children = std::mem::take(&mut node.children);
        let lead = match (self.lead, reached.lead) {
            (Some(lead), Some(lead_reached)) => {
                let first = (!children.is_empty()).then(|| children.remove(0));
                Some(Box::new(lead.advance(*lead_reached, first)))
            }
            _ if leads(&node) && children.first().is_some_and(goes_on_as_rest) => {
                Some(Box::new(Rest::new(children.remove(0))))
            }
            _ => None,
        };
        children.extend(self.head.drain(reached.head..));
        Rest { shell: node, lead, head: children, tail: self.tail, from: self.from + reached.tail }
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
