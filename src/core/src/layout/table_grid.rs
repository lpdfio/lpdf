//! Where the cells of a table go: the columns each cell covers, and the rows it spans.
//!
//! A cell takes the first free columns after the cell before it in its row. A cell with a `rowspan` holds its
//! columns in the rows under it, so the cells of those rows start after them. The parser places the rows of a table
//! with `Held` to check them, and layout places them again, from the tree that data binding left, to draw them.

use std::fmt;
use std::ops::Range;

use super::Node;

/// A cell's place in a table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placed {
    /// The row the cell starts in, and its place among that row's cells.
    pub row: usize,
    pub cell: usize,
    /// The columns it covers.
    pub columns: Range<usize>,
    /// The rows it covers, its own included.
    pub rows: usize,
}

impl Placed {
    /// The last row the cell covers.
    pub(crate) fn last_row(&self) -> usize {
        self.row + self.rows - 1
    }
}

/// What is wrong with a row whose cells do not line up with the columns.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowProblem {
    /// The cells span more columns than the table has free for them: all of its columns, less those that cells in
    /// the rows above hold.
    TooManyColumns { spanned: usize, free: usize, columns: usize },
    /// A cell with a `colspan` of `span` has no run of free columns that wide, because a cell in the rows above holds
    /// `column` (counted from 0).
    HeldColumn { span: usize, column: usize },
}

impl fmt::Display for RowProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RowProblem::TooManyColumns { spanned, free, columns } if free == columns => {
                write!(f, "its cells span {spanned} columns, but the table has {columns}")
            }
            RowProblem::TooManyColumns { spanned, free, columns } => write!(
                f,
                "its cells span {spanned} columns, but only {free} of the table's {columns} are free: \
                 a rowspan above holds the others"
            ),
            RowProblem::HeldColumn { span, column } => write!(
                f,
                "a cell with colspan=\"{span}\" does not fit before column {}, which a rowspan above holds",
                column + 1
            ),
        }
    }
}

/// The columns of a table that cells in the rows above still hold, kept while the rows are placed one after another.
pub(crate) struct Held {
    /// For each column, how many of the rows from the next one to be placed a cell above holds it for.
    left: Vec<usize>,
}

impl Held {
    pub(crate) fn new(n_cols: usize) -> Held {
        Held { left: vec![0; n_cols] }
    }

    /// Places the cells of the next row, `row`, which are given as `(colspan, rowspan)` in the order they are
    /// written, and appends where each went to `out`. Returns what is wrong with the row, if anything.
    ///
    /// A row that is wrong is placed as well as it can be, so that layout, which has no one to tell, has something to
    /// draw. A cell that does not fit where it starts moves on to the next free columns that are wide enough, and a
    /// cell with no such columns is left out of `out`.
    pub(crate) fn place_row(&mut self, row: usize, cells: &[(u16, u16)], out: &mut Vec<Placed>) -> Option<RowProblem> {
        let n_cols = self.left.len();
        let free = self.left.iter().filter(|rows| **rows == 0).count();
        let spanned: usize = cells.iter().map(|(colspan, _)| usize::from(*colspan)).sum();
        let too_many = RowProblem::TooManyColumns { spanned, free, columns: n_cols };
        let mut problem = (spanned > free).then(|| too_many.clone());

        let first = out.len();
        let mut next = 0;
        for (cell, &(colspan, rowspan)) in cells.iter().enumerate() {
            // The parser takes spans from 1; a node made by hand with a span of 0 covers one.
            let span = usize::from(colspan).max(1);
            let mut start = next;
            let start = loop {
                while start < n_cols && self.left[start] > 0 {
                    start += 1;
                }
                if start + span > n_cols {
                    problem.get_or_insert_with(|| too_many.clone());
                    break None;
                }
                match self.left[start..start + span].iter().position(|rows| *rows > 0) {
                    None => break Some(start),
                    Some(held) => {
                        problem.get_or_insert(RowProblem::HeldColumn { span, column: start + held });
                        start += held + 1;
                    }
                }
            };
            let Some(start) = start else { continue };
            out.push(Placed { row, cell, columns: start..start + span, rows: usize::from(rowspan).max(1) });
            next = start + span;
        }

        // The row is over: a cell above is held one row less, and the cells of this row hold their columns for the
        // rows they span after it.
        for rows in &mut self.left {
            *rows = rows.saturating_sub(1);
        }
        for placed in &out[first..] {
            for column in placed.columns.clone() {
                self.left[column] = placed.rows - 1;
            }
        }
        problem
    }

    /// After the last row: the first column that a cell still holds, and how many rows it holds it for past the end
    /// of the table.
    pub(crate) fn unfinished(&self) -> Option<(usize, usize)> {
        self.left.iter().enumerate().find(|(_, rows)| **rows > 0).map(|(column, rows)| (column, *rows))
    }
}

/// Where each cell of the rows of a table goes.
pub(super) struct TableGrid {
    /// The cells of every row, row after row. `starts[r]..starts[r + 1]` is the part that starts in row `r`.
    placed: Vec<Placed>,
    starts: Vec<usize>,
    /// The cells that span more than one row, which are few.
    spans: Vec<Placed>,
    /// For each row, whether a cell spans from it to the row after it.
    crossed: Vec<bool>,
}

impl TableGrid {
    /// Places the cells of `rows`, in a table of `n_cols` columns. A row that does not line up is placed as well
    /// as it can be (see `Held::place_row`): the parser has already refused one in the document, and one that data
    /// binding made has no one to tell. A cell that spans past the last row is cut there.
    pub(super) fn new(rows: &[Node], n_cols: usize) -> TableGrid {
        let mut held = Held::new(n_cols);
        let mut placed = Vec::with_capacity(rows.iter().map(|row| row.children.len()).sum());
        let mut starts = Vec::with_capacity(rows.len() + 1);
        let mut cells: Vec<(u16, u16)> = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            starts.push(placed.len());
            cells.clear();
            cells.extend(row.children.iter().map(|cell| (cell.col_span, cell.row_span)));
            let _ = held.place_row(r, &cells, &mut placed);
        }
        starts.push(placed.len());

        let mut spans = Vec::new();
        let mut crossed = vec![false; rows.len()];
        for cell in &mut placed {
            cell.rows = cell.rows.min(rows.len() - cell.row);
            if cell.rows > 1 {
                crossed[cell.row..cell.last_row()].fill(true);
                spans.push(cell.clone());
            }
        }
        TableGrid { placed, starts, spans, crossed }
    }

    /// The cells that start in `row`, left to right.
    pub(super) fn starting(&self, row: usize) -> &[Placed] {
        &self.placed[self.starts[row]..self.starts[row + 1]]
    }

    /// The cells that span rows and end in `row`.
    pub(super) fn spans_ending(&self, row: usize) -> impl Iterator<Item = &Placed> {
        self.spans.iter().filter(move |cell| cell.last_row() == row)
    }

    /// Whether a cell spans from `row` to the row after it, so that a page break between them would cut it.
    pub(super) fn crosses(&self, row: usize) -> bool {
        self.crossed.get(row).copied().unwrap_or(false)
    }

    /// The columns of the cells that span from `row` to the row after it.
    pub(super) fn crossing_columns(&self, row: usize) -> Vec<Range<usize>> {
        self.spans.iter().filter(|cell| cell.row <= row && row < cell.last_row()).map(|cell| cell.columns.clone()).collect()
    }

    /// The columns of each cell that covers more than one in `row`, whether it starts there or in a row above.
    pub(super) fn wide_cells_in(&self, row: usize) -> Vec<Range<usize>> {
        let above = self.spans.iter().filter(|cell| cell.row < row && row <= cell.last_row());
        self.starting(row).iter().chain(above).filter(|cell| cell.columns.len() > 1).map(|cell| cell.columns.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The columns each cell of `rows` goes to, as `(first, last + 1, rows)`, and the first thing that is wrong.
    fn place(n_cols: usize, rows: &[&[(u16, u16)]]) -> (Vec<Vec<(usize, usize, usize)>>, Option<RowProblem>) {
        let mut held = Held::new(n_cols);
        let mut placed = Vec::new();
        let mut problem = None;
        for (r, cells) in rows.iter().enumerate() {
            let mut out = Vec::new();
            let found = held.place_row(r, cells, &mut out);
            problem = problem.or(found);
            placed.push(out.iter().map(|p| (p.columns.start, p.columns.end, p.rows)).collect());
        }
        (placed, problem)
    }

    #[test]
    fn cells_without_spans_fill_the_columns_in_order() {
        let (placed, problem) = place(3, &[&[(1, 1), (1, 1), (1, 1)], &[(1, 1)]]);
        assert_eq!(placed, vec![vec![(0, 1, 1), (1, 2, 1), (2, 3, 1)], vec![(0, 1, 1)]]);
        assert_eq!(problem, None);
    }

    #[test]
    fn a_cell_that_spans_rows_holds_its_column_in_the_rows_under_it() {
        let (placed, problem) = place(3, &[&[(1, 3), (1, 1), (1, 1)], &[(1, 1), (1, 1)], &[(1, 1), (1, 1)], &[(1, 1), (1, 1), (1, 1)]]);
        assert_eq!(placed[1], vec![(1, 2, 1), (2, 3, 1)]);
        assert_eq!(placed[2], vec![(1, 2, 1), (2, 3, 1)]);
        assert_eq!(placed[3], vec![(0, 1, 1), (1, 2, 1), (2, 3, 1)], "the span ended with row 3");
        assert_eq!(problem, None);
    }

    #[test]
    fn a_cell_in_the_middle_of_a_row_holds_the_middle_column() {
        let (placed, problem) = place(3, &[&[(1, 1), (1, 2), (1, 1)], &[(1, 1), (1, 1)]]);
        assert_eq!(placed[1], vec![(0, 1, 1), (2, 3, 1)]);
        assert_eq!(problem, None);
    }

    #[test]
    fn a_cell_with_a_colspan_and_a_rowspan_holds_every_column_it_covers() {
        let (placed, problem) = place(4, &[&[(2, 2), (1, 1), (1, 1)], &[(1, 1), (1, 1)]]);
        assert_eq!(placed[0], vec![(0, 2, 2), (2, 3, 1), (3, 4, 1)]);
        assert_eq!(placed[1], vec![(2, 3, 1), (3, 4, 1)]);
        assert_eq!(problem, None);
    }

    #[test]
    fn a_row_with_a_cell_too_many_under_a_span_is_a_problem() {
        let (placed, problem) = place(3, &[&[(1, 2), (1, 1), (1, 1)], &[(1, 1), (1, 1), (1, 1)]]);
        assert_eq!(problem, Some(RowProblem::TooManyColumns { spanned: 3, free: 2, columns: 3 }));
        assert_eq!(placed[1], vec![(1, 2, 1), (2, 3, 1)], "the cell with no column is left out");
    }

    #[test]
    fn a_wide_cell_that_a_span_splits_from_its_columns_is_a_problem() {
        let (placed, problem) = place(3, &[&[(1, 1), (1, 2), (1, 1)], &[(2, 1)]]);
        assert_eq!(problem, Some(RowProblem::HeldColumn { span: 2, column: 1 }));
        assert_eq!(placed[1], vec![], "no two free columns are next to each other");
    }

    #[test]
    fn a_span_that_runs_past_the_last_row_is_left_unfinished() {
        let mut held = Held::new(2);
        let mut out = Vec::new();
        held.place_row(0, &[(1, 3), (1, 1)], &mut out);
        held.place_row(1, &[(1, 1)], &mut out);
        assert_eq!(held.unfinished(), Some((0, 1)), "one more row is needed");
    }

    #[test]
    fn a_span_that_ends_with_the_last_row_leaves_nothing_unfinished() {
        let mut held = Held::new(2);
        let mut out = Vec::new();
        held.place_row(0, &[(1, 2), (1, 1)], &mut out);
        held.place_row(1, &[(1, 1)], &mut out);
        assert_eq!(held.unfinished(), None);
    }

    #[test]
    fn the_problems_say_which_cells_and_columns() {
        let too_many = RowProblem::TooManyColumns { spanned: 3, free: 2, columns: 2 };
        assert_eq!(too_many.to_string(), "its cells span 3 columns, but the table has 2");
        let under_span = RowProblem::TooManyColumns { spanned: 3, free: 2, columns: 3 };
        assert!(under_span.to_string().contains("only 2 of the table's 3 are free"), "{under_span}");
        let held = RowProblem::HeldColumn { span: 2, column: 1 };
        assert!(held.to_string().contains("before column 2"), "{held}");
    }
}
