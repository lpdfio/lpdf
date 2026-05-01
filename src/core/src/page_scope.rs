/// Shared page-scope index resolution used by both `canvas.rs` and `layout.rs`.

use crate::parse::PageScope;

/// Return the 0-based indices of all render pages within a section that a
/// given page-scope targets.  `n` is the total number of section pages.
///
/// Semantics:
/// - `None` or `Some(Each)` → all pages
/// - `Some(First)` → `[0]`
/// - `Some(Last)` → `[n-1]`
/// - `Some(Odd)` → even 0-based indices (pages 1, 3, 5 …)
/// - `Some(Even)` → odd 0-based indices (pages 2, 4, 6 …)
/// - `Some(Pages(ranges))` → 1-based inclusive ranges, clamped to `[0, n)`
pub fn page_scope_indices(scope: &Option<PageScope>, n: usize) -> Vec<usize> {
    if n == 0 { return Vec::new(); }
    match scope {
        None | Some(PageScope::Each) => (0..n).collect(),
        Some(PageScope::First)       => vec![0],
        Some(PageScope::Last)        => vec![n - 1],
        Some(PageScope::Odd)         => (0..n).filter(|i| i % 2 == 0).collect(),
        Some(PageScope::Even)        => (0..n).filter(|i| i % 2 == 1).collect(),
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

    #[test]
    fn none_and_each_return_all() {
        assert_eq!(page_scope_indices(&None, 3), vec![0_usize, 1, 2]);
        assert_eq!(page_scope_indices(&Some(PageScope::Each), 3), vec![0_usize, 1, 2]);
    }

    #[test]
    fn first_returns_zero() {
        assert_eq!(page_scope_indices(&Some(PageScope::First), 3), vec![0_usize]);
    }

    #[test]
    fn last_returns_n_minus_1() {
        assert_eq!(page_scope_indices(&Some(PageScope::Last), 3), vec![2_usize]);
    }

    #[test]
    fn odd_returns_even_indices() {
        assert_eq!(page_scope_indices(&Some(PageScope::Odd), 4), vec![0_usize, 2]);
    }

    #[test]
    fn even_returns_odd_indices() {
        assert_eq!(page_scope_indices(&Some(PageScope::Even), 4), vec![1_usize, 3]);
    }

    #[test]
    fn numeric_range() {
        let p = Some(PageScope::Pages(vec![PageRange { start: 2, end: Some(3) }]));
        assert_eq!(page_scope_indices(&p, 5), vec![1_usize, 2]);
    }

    #[test]
    fn n_last_range() {
        let p = Some(PageScope::Pages(vec![PageRange { start: 3, end: None }]));
        assert_eq!(page_scope_indices(&p, 5), vec![2_usize, 3, 4]);
    }

    #[test]
    fn n_zero_guard() {
        let empty: Vec<usize> = vec![];
        assert_eq!(page_scope_indices(&Some(PageScope::First), 0), empty);
    }

    #[test]
    fn out_of_range_page() {
        let p = Some(PageScope::Pages(vec![PageRange { start: 10, end: Some(20) }]));
        let empty: Vec<usize> = vec![];
        assert_eq!(page_scope_indices(&p, 3), empty);
    }
}
