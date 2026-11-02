//! Shared page-scope index resolution used by both `canvas.rs` and `layout.rs`.

use crate::parse::PageScope;

/// Return the 0-based indices of all render pages within a section that a
/// given page-scope targets.  `n` is the total number of section pages, and
/// `first_number` the number printed on its first page.
///
/// Semantics:
/// - `None` or `Some(Each)` → all pages
/// - `Some(First)` → `[0]`
/// - `Some(Last)` → `[n-1]`
/// - `Some(Odd)` → the pages whose printed number is odd
/// - `Some(Even)` → the pages whose printed number is even
/// - `Some(Pages(ranges))` → 1-based inclusive ranges, clamped to `[0, n)`
///
/// `odd` and `even` follow the printed number, so that a book's left and right running heads stay on their side
/// in a chapter that starts on an even page. `first`, `last` and ranges name a place in the section, whatever its
/// numbers: a range that followed them would put a running head back on a chapter's opening page.
pub fn page_scope_indices(scope: &Option<PageScope>, n: usize, first_number: u64) -> Vec<usize> {
    if n == 0 { return Vec::new(); }
    let printed_odd = |i: &usize| (first_number + *i as u64) % 2 == 1;
    match scope {
        None | Some(PageScope::Each) => (0..n).collect(),
        Some(PageScope::First)       => vec![0],
        Some(PageScope::Last)        => vec![n - 1],
        Some(PageScope::Odd)         => (0..n).filter(printed_odd).collect(),
        Some(PageScope::Even)        => (0..n).filter(|i| !printed_odd(i)).collect(),
        Some(PageScope::Pages(ranges)) => {
            let mut out = Vec::new();
            for range in ranges {
                let start = (range.start as usize).saturating_sub(1);
                let end   = range.end.map(|e| e as usize).unwrap_or(n);
                for i in start..end.min(n) {
                    out.push(i);
                }
            }
            out
        }
    }
}

/// Returns whether a chrome node with the given `page_scope` contributes to
/// the page-height budget when `include_first` controls whether first-page-only
/// chrome is counted.
///
/// Used by the layout engine to pre-compute per-page height budgets *before*
/// pagination when the total page count is not yet known.
///
/// Conservative rule for unknown-count scopes (`Last`/`Odd`/`Even`/`Pages`):
/// they are always included in the budget estimate to prevent overflow.
pub fn chrome_in_budget(scope: &Option<PageScope>, include_first: bool) -> bool {
    match scope {
        None | Some(PageScope::Each) => true,
        Some(PageScope::First)       => include_first,
        _                            => true, // Last, Odd, Even, Pages — conservative
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::PageRange;

    fn range(start: u32, end: Option<u32>) -> Option<PageScope> {
        Some(PageScope::Pages(vec![PageRange { start, end }]))
    }

    #[test]
    fn none_and_each_return_all() {
        assert_eq!(page_scope_indices(&None, 3, 1), vec![0_usize, 1, 2]);
        assert_eq!(page_scope_indices(&Some(PageScope::Each), 3, 1), vec![0_usize, 1, 2]);
    }

    #[test]
    fn first_returns_zero() {
        assert_eq!(page_scope_indices(&Some(PageScope::First), 3, 1), vec![0_usize]);
    }

    #[test]
    fn last_returns_n_minus_1() {
        assert_eq!(page_scope_indices(&Some(PageScope::Last), 3, 1), vec![2_usize]);
    }

    #[test]
    fn odd_returns_even_indices_in_a_section_from_page_1() {
        assert_eq!(page_scope_indices(&Some(PageScope::Odd), 4, 1), vec![0_usize, 2]);
    }

    #[test]
    fn even_returns_odd_indices_in_a_section_from_page_1() {
        assert_eq!(page_scope_indices(&Some(PageScope::Even), 4, 1), vec![1_usize, 3]);
    }

    #[test]
    fn odd_and_even_follow_the_printed_number_in_a_section_that_starts_on_an_even_page() {
        // Printed 4, 5, 6, 7.
        assert_eq!(page_scope_indices(&Some(PageScope::Odd), 4, 4), vec![1_usize, 3]);
        assert_eq!(page_scope_indices(&Some(PageScope::Even), 4, 4), vec![0_usize, 2]);
    }

    #[test]
    fn odd_and_even_follow_the_printed_number_in_a_section_that_starts_on_an_odd_page() {
        // Printed 5, 6, 7, 8.
        assert_eq!(page_scope_indices(&Some(PageScope::Odd), 4, 5), vec![0_usize, 2]);
        assert_eq!(page_scope_indices(&Some(PageScope::Even), 4, 5), vec![1_usize, 3]);
    }

    #[test]
    fn a_lone_page_printed_even_shows_even_and_not_odd() {
        let empty: Vec<usize> = vec![];
        assert_eq!(page_scope_indices(&Some(PageScope::Even), 1, 4), vec![0_usize]);
        assert_eq!(page_scope_indices(&Some(PageScope::Odd), 1, 4), empty);
    }

    #[test]
    fn first_last_and_ranges_count_the_section_in_one_that_starts_above_1() {
        // Printed 4, 5, 6, 7: the places in the section, not the printed numbers.
        assert_eq!(page_scope_indices(&Some(PageScope::First), 4, 4), vec![0_usize]);
        assert_eq!(page_scope_indices(&Some(PageScope::Last), 4, 4), vec![3_usize]);
        assert_eq!(page_scope_indices(&range(2, None), 4, 4), vec![1_usize, 2, 3]);
        assert_eq!(page_scope_indices(&range(2, Some(3)), 4, 4), vec![1_usize, 2]);
        assert_eq!(page_scope_indices(&range(5, None), 4, 4), Vec::<usize>::new());
    }

    #[test]
    fn numeric_range() {
        assert_eq!(page_scope_indices(&range(2, Some(3)), 5, 1), vec![1_usize, 2]);
    }

    #[test]
    fn n_last_range() {
        assert_eq!(page_scope_indices(&range(3, None), 5, 1), vec![2_usize, 3, 4]);
    }

    #[test]
    fn n_zero_guard() {
        let empty: Vec<usize> = vec![];
        assert_eq!(page_scope_indices(&Some(PageScope::First), 0, 1), empty);
    }

    #[test]
    fn out_of_range_page() {
        let empty: Vec<usize> = vec![];
        assert_eq!(page_scope_indices(&range(10, Some(20)), 3, 1), empty);
    }
}
