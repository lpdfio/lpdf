//! A bulk container laid out a page at a time from its records (`data::BulkRecords`), with no node for a record that
//! no page has reached.
//!
//! The container waits in the flow as a `Bulk`: its box, the children it holds before its next record, which are the
//! ones written before the template, or a table's header row again with what a page left, then the records from the
//! next one on, which stay in the data, then the children written after them. Each page binds the records it reaches,
//! and cuts them where the split of the whole container would cut all of its children (`split_column`, `split_grid`,
//! `split_table`), at the heights that split measures them at, so the pages are the ones the container would have laid
//! out whole. A record takes the height of the first record of its shape, when its texts hold one line each (`shape`),
//! and is measured otherwise. Once what is left of it reaches no further than a page, it is the container with those
//! children, which the flow takes from there as it takes any node.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use super::{
    fill_sinks, forget_captures, measure_height, record_captures, refuse, table_col_widths, text_width, Node, NodeKind,
    PageCaptures, RowHeights, TableGrid,
};
use crate::data::BulkRecords;

/// How many records are bound at a time as a page looks for where it ends.
const BATCH: usize = 64;

#[cfg(test)]
thread_local! {
    /// How many parts pages cut off bulk containers, counted for the tests.
    pub(super) static PARTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// What `Bulk::take` gives a page.
pub(super) enum Take {
    /// Every child left, which reach no further than a page: the container, an ordinary node from here on.
    Whole(Node),
    /// The part the page holds, and what is left after it.
    First(Node, Box<Bulk>),
    /// Not even the first record fits the room, with a table's header row: it waits for the next.
    Nothing(Box<Bulk>),
}

/// What is left of a bulk container (see the module).
#[derive(Clone)]
pub(super) struct Bulk {
    /// The container, without its children and its records.
    shell:   Node,
    records: Rc<BulkRecords>,
    /// The children before the next record, each with whether it is a record bound already.
    head:    VecDeque<(Node, bool)>,
    /// The first record not bound yet.
    next:    usize,
    /// The children written after the template.
    tail:    VecDeque<Node>,
    /// A table's header row as the pages after its first show it, capturing nothing again.
    header:  Option<Node>,
    /// The height of each shape of record measured so far, at each width, shared by every copy of the rest.
    heights: Rc<RefCell<HashMap<Vec<u32>, f32>>>,
}

/// A child of a window, as `Bulk::gather` takes it off the front: the node, its height as the split measures it, and
/// whether it is a record.
type Held = (Node, f32, bool);

/// How the split of the container measures and cuts it in a room: its kind's widths, and the room and the page inside
/// its padding (`split_column`, `split_grid`, `split_table`).
struct Measure {
    kind:         NodeKind,
    gap:          f32,
    inner_w:      f32,
    inner_target: f32,
    inner_full:   f32,
    /// A grid's columns and the width of each.
    cols:         usize,
    col_w:        f32,
    /// A table's columns.
    col_widths:   Vec<f32>,
}

impl Measure {
    fn new(shell: &Node, avail_w: f32, target: f32, page_h: f32) -> Measure {
        let [pt, pr, pb, pl] = shell.padding;
        let inner_w = (avail_w - pl - pr).max(0.0);
        let gap = shell.gap;
        let (cols, col_w) = match shell.col_width {
            _ if shell.kind != NodeKind::Grid => (1, inner_w),
            Some(min_w) => {
                let cols = (((inner_w + gap) / (min_w + gap)).floor() as usize).max(1);
                (cols, ((inner_w - gap * (cols - 1) as f32) / cols as f32).max(0.0))
            }
            None => {
                let cols = (shell.cols as usize).max(1);
                (cols, ((inner_w - gap * (cols - 1) as f32) / cols as f32).max(0.0))
            }
        };
        let col_widths = if shell.kind == NodeKind::Table {
            // A bulk table's columns are set in fr, pt and %, so they don't depend on its rows.
            table_col_widths(&[], &shell.table_cols, inner_w, gap)
        } else {
            Vec::new()
        };
        Measure {
            kind: shell.kind.clone(),
            gap,
            inner_w,
            inner_target: (target - pt - pb).max(0.0),
            inner_full: (page_h - pt - pb).max(0.0),
            cols,
            col_w,
            col_widths,
        }
    }

    /// The height of `child` as the split measures it: a row of a table by its cells, which span no rows, a cell of a
    /// grid at the width of a column, and a child of a column or a list at the width inside it.
    fn height(&self, child: &Node) -> f32 {
        match self.kind {
            NodeKind::Table => {
                let rows = std::slice::from_ref(child);
                let grid = TableGrid::new(rows, self.col_widths.len());
                RowHeights::new(rows, &grid, &self.col_widths, self.gap).get(0)
            }
            NodeKind::Grid => measure_height(child, self.col_w, self.inner_full),
            _ => measure_height(child, self.inner_w, self.inner_full),
        }
    }

    /// The start of the key of a record's height (`shape`): the width and the page it is measured in.
    fn key(&self) -> Vec<u32> {
        let width = if self.kind == NodeKind::Grid { self.col_w } else { self.inner_w };
        vec![width.to_bits(), self.inner_full.to_bits()]
    }

    /// Whether children `heights` high, `count` of them or rows of a grid, are past `reach` and at least two.
    fn reached(&self, height: f32, count: usize, reach: f32) -> bool {
        height > reach && count >= 2
    }

    /// Where the split of the whole container would cut the `window` of its children, which reaches past the room.
    fn cut(&self, window: &[Held]) -> Cut {
        let target = self.inner_target + 0.5;
        if self.kind == NodeKind::Grid {
            let rows: Vec<f32> = window.chunks(self.cols)
                .map(|row| row.iter().map(|&(_, h, _)| h).fold(0.0_f32, f32::max))
                .collect();
            let fit = fit(&rows, self.gap, target);
            if fit > 0 {
                return Cut::At((fit * self.cols).min(window.len()));
            }
            // A first row taller than a page stays on a full page in `split_grid`, which a bulk grid refuses.
            return if self.inner_target + 0.5 < self.inner_full { Cut::Nothing } else { Cut::TooTall };
        }
        let heights: Vec<f32> = window.iter().map(|&(_, h, _)| h).collect();
        let fit = fit(&heights, self.gap, target);
        if self.kind == NodeKind::Table {
            // A table's header row needs a row under it.
            let header = usize::from(window.first().is_some_and(|(row, _, _)| row.kind == NodeKind::TableHead));
            return if fit > header { Cut::At(fit) } else { Cut::Nothing };
        }
        match fit {
            0 if self.inner_target + 0.5 >= self.inner_full => Cut::TooTall,
            0 => Cut::Nothing,
            fit => Cut::At(fit),
        }
    }
}

/// How many of `heights`, one after another `gap` apart, fit in `target`, added up as the splits add them.
fn fit(heights: &[f32], gap: f32, target: f32) -> usize {
    let mut chunk_h = 0.0_f32;
    for (i, &h) in heights.iter().enumerate() {
        let g = if i == 0 { 0.0 } else { gap };
        if chunk_h + g + h > target {
            return i;
        }
        chunk_h += g + h;
    }
    heights.len()
}

/// Where a page cuts a window of a bulk container.
enum Cut {
    /// After this many children.
    At(usize),
    /// Before the first.
    Nothing,
    /// Nowhere: the first child doesn't fit a whole page.
    TooTall,
}

impl Bulk {
    /// What is left of `node`, a bulk container whose `records` no page has reached.
    pub(super) fn new(mut node: Node, records: Rc<BulkRecords>) -> Bulk {
        node.data_attrs = None;
        let mut head: Vec<Node> = std::mem::take(&mut node.children);
        let tail: VecDeque<Node> = head.split_off(records.at.min(head.len())).into();
        let header = head.first()
            .filter(|row| node.kind == NodeKind::Table && row.kind == NodeKind::TableHead)
            .cloned()
            .map(|mut header| {
                forget_captures(&mut header);
                header
            });
        let head = head.into_iter().map(|node| (node, false)).collect();
        Bulk { shell: node, records, head, next: 0, tail, header, heights: Rc::default() }
    }

    /// The container, which pagination reads before splitting it.
    pub(super) fn shell(&self) -> &Node {
        &self.shell
    }

    pub(super) fn shell_mut(&mut self) -> &mut Node {
        &mut self.shell
    }

    fn is_empty(&self) -> bool {
        self.head.is_empty() && self.next >= self.records.len() && self.tail.is_empty()
    }

    /// The next child, off the front: one held before the next record, a record bound with those after it in its
    /// batch, or a child written after the template.
    fn pop(&mut self) -> Result<Option<(Node, bool)>, String> {
        while self.head.is_empty() && self.next < self.records.len() {
            let end = (self.next + BATCH).min(self.records.len());
            let bound = self.records.bind(self.next..end)?;
            if bound.is_empty() {
                // The data holds fewer records than it did when they were counted.
                self.next = self.records.len();
                break;
            }
            self.next += bound.len();
            for mut node in bound {
                // As the flow's own nodes are before the pages are filled (`fill_pages`).
                record_captures(&mut node);
                fill_sinks(&mut node, &PageCaptures::new(), &HashMap::new());
                self.head.push_back((node, true));
            }
        }
        Ok(self.head.pop_front().or_else(|| self.tail.pop_front().map(|node| (node, false))))
    }

    /// The height of a child as `measure` measures it, a record's by its shape when it has one.
    fn height(&self, node: &Node, record: bool, measure: &Measure) -> f32 {
        let mut key = measure.key();
        if !record || !shape(node, &mut key) {
            return measure.height(node);
        }
        if let Some(&h) = self.heights.borrow().get(&key) {
            return h;
        }
        let h = measure.height(node);
        self.heights.borrow_mut().insert(key, h);
        h
    }

    /// Take the children off the front, with their heights, until they reach past `reach` and are at least two, or
    /// two rows of a grid, or none is left.
    fn gather(&mut self, measure: &Measure, reach: f32) -> Result<Vec<Held>, String> {
        let gap = measure.gap;
        let mut window: Vec<Held> = Vec::new();
        let mut height = 0.0_f32;
        let mut count = 0;
        while !measure.reached(height, count, reach) {
            if measure.kind == NodeKind::Grid {
                let mut row_h = 0.0_f32;
                let mut cells = 0;
                while cells < measure.cols {
                    let Some((node, record)) = self.pop()? else { break };
                    let h = self.height(&node, record, measure);
                    row_h = row_h.max(h);
                    window.push((node, h, record));
                    cells += 1;
                }
                if cells == 0 {
                    break;
                }
                height += if count == 0 { row_h } else { gap + row_h };
            } else {
                let Some((node, record)) = self.pop()? else { break };
                let h = self.height(&node, record, measure);
                height += if count == 0 { h } else { gap + h };
                window.push((node, h, record));
            }
            count += 1;
        }
        Ok(window)
    }

    /// Put `left` back at the front, in its order.
    fn put_back(&mut self, left: Vec<Held>) {
        for (node, _, record) in left.into_iter().rev() {
            self.head.push_front((node, record));
        }
    }

    /// The container with `children`.
    fn with(&self, children: Vec<Held>) -> Node {
        self.shell.with_children(children.into_iter().map(|(node, _, _)| node).collect())
    }

    /// What a page whose room for it is `room` high, `avail_w` wide, on a page `page_h` high, takes of it. The window
    /// it splits reaches past `reach`, which is past the room and the page after it.
    pub(super) fn take(mut self: Box<Self>, avail_w: f32, room: f32, page_h: f32, reach: f32) -> Take {
        let measure = Measure::new(&self.shell, avail_w, room, page_h);
        let mut window = match self.gather(&measure, reach) {
            Ok(window) => window,
            Err(error) => {
                refuse(error);
                return Take::Whole(self.shell.with_children(Vec::new()));
            }
        };
        if self.is_empty() {
            return Take::Whole(self.with(window));
        }
        match measure.cut(&window) {
            Cut::At(n) => {
                let left = window.split_off(n);
                self.put_back(left);
                if let Some(header) = &self.header {
                    self.head.push_front((header.clone(), false));
                }
                let part = self.with(window);
                #[cfg(test)]
                PARTS.with(|parts| parts.set(parts.get() + 1));
                Take::First(part, self)
            }
            Cut::Nothing => {
                self.put_back(window);
                Take::Nothing(self)
            }
            Cut::TooTall => {
                refuse(self.too_tall());
                Take::Whole(self.shell.with_children(Vec::new()))
            }
        }
    }

    /// The error of a record, with a table's header row, that doesn't fit an empty page, which a bulk container
    /// can't place: it never splits a record, and no page has more room.
    pub(super) fn too_tall(&self) -> String {
        let (container, record) = match self.shell.kind {
            NodeKind::Table => ("table", "a row, with the header row,"),
            NodeKind::Grid  => ("grid", "a row of its cells"),
            NodeKind::Column if self.shell.role == crate::parse::Role::List => ("list", "an item"),
            _ => ("column", "a text"),
        };
        format!("a bulk {container} never splits a record across pages, and {record} is taller than its page: give it fewer lines")
    }

    /// The node `fill_room` measures in place of it, to see whether the node before it can keep with it: the window
    /// a page would split, which reaches past `reach`, or every child left when they reach no further.
    pub(super) fn peek(&self, avail_w: f32, reach: f32, page_h: f32) -> Node {
        let mut copy = self.clone();
        let measure = Measure::new(&self.shell, avail_w, page_h, page_h);
        match copy.gather(&measure, reach) {
            Ok(window) => copy.with(window),
            Err(error) => {
                refuse(error);
                self.shell.with_children(Vec::new())
            }
        }
    }

    /// The node it stands for: the container with every child left, every record bound.
    pub(super) fn into_node(mut self) -> Node {
        let mut children = Vec::new();
        loop {
            match self.pop() {
                Ok(Some((node, _))) => children.push(node),
                Ok(None) => break,
                Err(error) => {
                    refuse(error);
                    break;
                }
            }
        }
        self.shell.with_children(children)
    }
}

/// Add to `key` what the height of `node`, a record, depends on besides its template and the width: how many children
/// each node of it has, and whether each text shows words. False when a text can take more lines than one, so that
/// its words decide its height: one that sets `max-lines` above 1, or the number of a list's item that is wider than
/// its marker.
fn shape(node: &Node, key: &mut Vec<u32>) -> bool {
    if node.kind != NodeKind::Text {
        // Above the 0 and 1 of a text, so that a box and a text never read the same.
        key.push(node.children.len() as u32 + 2);
        return node.children.iter().all(|child| shape(child, key));
    }
    let one_line = node.max_lines == Some(1) || match (node.text_runs.as_slice(), node.width_constraint) {
        // A number, which a marker holds on one line while it is narrower than the marker.
        ([run], Some(width)) => {
            !run.text.contains(char::is_whitespace) && text_width(&node.font, &run.text, node.font_size) <= width - 1.0
        }
        _ => false,
    };
    key.push(u32::from(node.text_runs.iter().any(|run| run.text.split_whitespace().next().is_some())));
    one_line
}
