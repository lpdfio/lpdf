use crate::parse::{Align, BarcodeEcLevel, BarcodeType, Direction, HeightMode, Justify, Node, NodeKind, Page, Repeat, TextAlign, TextRun};
use crate::render::{RenderBarcode, RenderBox, RenderImage, RenderLine, RenderLink, RenderNode, RenderPage, RenderText, RenderedBarcodeKind};
use crate::tokens::FontWidths;
use std::cell::RefCell;
use std::collections::HashMap;

// ── Per-call custom font width tables ────────────────────────────────────────
// Set by LpdfEngine before calling layout_page so that text_width can use
// real glyph metrics instead of the 0.44 constant fallback.
thread_local! {
    static FONT_WIDTHS: RefCell<HashMap<String, FontWidths>> = RefCell::new(HashMap::new());
}

/// Install caller-supplied width tables for the current layout call.
/// Must be called before `layout_page`. WASM is single-threaded so the
/// thread_local is safe to use as a per-call context.
pub fn set_font_widths(widths: HashMap<String, FontWidths>) {
    FONT_WIDTHS.with(|fw| *fw.borrow_mut() = widths);
}

/// Natural pixel dimensions for declared images: name → (width_px, height_px).
pub type ImageMeta = HashMap<String, (u32, u32)>;

/// Fill in concrete `width_constraint` and `img_height_constraint` for every
/// `NodeKind::Img` node in a page tree, using `meta` for aspect-ratio derivation.
///
/// Call this on every `Page` *before* calling `layout_page`. After the pass,
/// `layout_img` can read `.width_constraint.unwrap_or(avail_w)` and
/// `.img_height_constraint.unwrap_or(w)` safely.
pub fn prefill_image_sizes(nodes: &mut Vec<Node>, meta: &ImageMeta) {
    for node in nodes.iter_mut() {
        prefill_node(node, meta);
    }
}

fn prefill_node(node: &mut Node, meta: &ImageMeta) {
    if node.kind == NodeKind::Img {
        let name = node.image_name.as_deref().unwrap_or("");
        let (nat_w, nat_h) = meta.get(name)
            .map(|&(w, h)| (w as f32, h as f32))
            .unwrap_or((100.0, 100.0));
        let aspect = if nat_h > 0.0 { nat_w / nat_h } else { 1.0 };
        let (w, h) = match (node.width_constraint, node.img_height_constraint) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None)    => (w, if aspect > 0.0 { w / aspect } else { w }),
            (None, Some(h))    => (if aspect > 0.0 { h * aspect } else { h }, h),
            (None, None)       => (nat_w, nat_h),
        };
        node.width_constraint      = Some(w);
        node.img_height_constraint = Some(h);
    } else {
        for child in &mut node.children {
            prefill_node(child, meta);
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn layout_page(page: &Page) -> Vec<RenderPage> {
    let mut pages = layout_page_impl(page);
    for rp in &mut pages {
        apply_debug_overlay(&mut rp.nodes);
        if page.debug {
            let [mt, mr, mb, ml] = rp.margin;
            let cx = ml;
            let cy = mt;
            let cw = rp.width - ml - mr;
            let ch = rp.height - mt - mb;
            let mut overlay = debug_rect_lines(cx, cy, cw, ch, "#ff0033");
            for node in rp.nodes.iter() {
                if let RenderNode::Box(b) = node {
                    if !b.debug_self {
                        overlay.extend(debug_rect_lines(b.x, b.y, b.width, b.height, "#0066ff"));
                    }
                }
            }
            rp.nodes.extend(overlay);
        }
    }
    pages
}

fn layout_page_impl(page: &Page) -> Vec<RenderPage> {
    let [mt, mr, mb, ml] = page.margin;
    let avail_x = ml;
    let avail_y = mt;
    let avail_w = (page.width - ml - mr).max(0.0);
    let avail_h = (page.height - mt - mb).max(0.0);

    // Exemption: a single Full- or Fill-height child always occupies exactly
    // one page — skip pagination entirely.
    if page.children.len() == 1
        && matches!(
            page.children[0].height_mode,
            HeightMode::Full | HeightMode::Fill
        )
        && page.children[0].repeat == Repeat::None
    {
        let (nodes, _) = layout_stack(
            &page.children, avail_x, avail_y, avail_w, Some(avail_h),
            0.0, &Align::Stretch, &Justify::Start,
        );
        return vec![RenderPage {
            width: page.width,
            height: page.height,
            background: page.background.clone(),
            margin: page.margin,
            nodes,
        }];
    }

    // ── Partition direct children into chrome (top/bottom) and flow ──────────
    let mut lead = 0usize;
    while lead < page.children.len() && page.children[lead].repeat != Repeat::None {
        lead += 1;
    }
    let mut trail_start = page.children.len();
    while trail_start > lead && page.children[trail_start - 1].repeat != Repeat::None {
        trail_start -= 1;
    }
    let top_chrome: &[Node] = &page.children[..lead];
    let flow: Vec<Node> = page.children[lead..trail_start]
        .iter()
        // Defensive: a repeat child stuck in the middle is treated as flow
        // (acts like an atomic node that appears once, not chrome).
        .cloned()
        .collect();
    let bot_chrome: &[Node] = &page.children[trail_start..];

    // Fast path: no chrome — standard pagination.
    if top_chrome.is_empty() && bot_chrome.is_empty() {
        let chunks = split_into_pages(&flow, avail_w, avail_h, avail_h, 0.0);
        return chunks
            .into_iter()
            .map(|chunk| {
                let (nodes, _) = layout_stack(
                    &chunk, avail_x, avail_y, avail_w, Some(avail_h),
                    0.0, &Align::Stretch, &Justify::Start,
                );
                RenderPage {
                    width: page.width,
                    height: page.height,
                    background: page.background.clone(),
                    margin: page.margin,
                    nodes,
                }
            })
            .collect();
    }

    // ── Measure chrome heights ────────────────────────────────────────────────
    // "always" = sum of page-repeat only (used on every page)
    // "first"  = sum of every chrome (used on page 1 if repeat="first" is present)
    let top_always_h = measure_chrome_height(top_chrome, avail_w, false);
    let top_first_h  = measure_chrome_height(top_chrome, avail_w, true);
    let bot_always_h = measure_chrome_height(bot_chrome, avail_w, false);
    let bot_first_h  = measure_chrome_height(bot_chrome, avail_w, true);

    let budget_first = (avail_h - top_first_h - bot_first_h).max(0.0);
    let budget_rest  = (avail_h - top_always_h - bot_always_h).max(0.0);

    // ── Paginate flow with per-page budgets ──────────────────────────────────
    let chunks = split_into_pages(&flow, avail_w, budget_first, budget_rest, 0.0);
    let total_pages = chunks.len();

    // ── Build each output page ───────────────────────────────────────────────
    let mut pages_out: Vec<RenderPage> = Vec::with_capacity(total_pages);
    for (idx, chunk) in chunks.into_iter().enumerate() {
        let is_first = idx == 0;
        let page_num = idx + 1;

        let top_here: Vec<Node> = top_chrome.iter()
            .filter(|n| n.repeat == Repeat::Page || (is_first && n.repeat == Repeat::First))
            .map(|n| substitute_page_tokens(n, page_num, total_pages))
            .collect();
        let bot_here: Vec<Node> = bot_chrome.iter()
            .filter(|n| n.repeat == Repeat::Page || (is_first && n.repeat == Repeat::First))
            .map(|n| substitute_page_tokens(n, page_num, total_pages))
            .collect();

        // Top chrome laid out from y = mt down.
        let (top_nodes, top_h) = layout_stack(
            &top_here, avail_x, avail_y, avail_w, None,
            0.0, &Align::Stretch, &Justify::Start,
        );

        // Bottom chrome anchored to the bottom.
        let bot_h = measure_stack_height(&bot_here, avail_w);
        let bot_y = avail_y + avail_h - bot_h;
        let (bot_nodes, _) = layout_stack(
            &bot_here, avail_x, bot_y, avail_w, None,
            0.0, &Align::Stretch, &Justify::Start,
        );

        // Flow between chromes.
        let flow_y = avail_y + top_h;
        let flow_budget = if is_first { budget_first } else { budget_rest };
        let (flow_nodes, _) = layout_stack(
            &chunk, avail_x, flow_y, avail_w, Some(flow_budget),
            0.0, &Align::Stretch, &Justify::Start,
        );

        let mut all_nodes = top_nodes;
        all_nodes.extend(flow_nodes);
        all_nodes.extend(bot_nodes);

        pages_out.push(RenderPage {
            width: page.width,
            height: page.height,
            background: page.background.clone(),
            margin: page.margin,
            nodes: all_nodes,
        });
    }

    // Guarantee at least one page, even if flow is empty but chrome exists.
    if pages_out.is_empty() {
        let top_here: Vec<Node> = top_chrome.iter()
            .map(|n| substitute_page_tokens(n, 1, 1))
            .collect();
        let bot_here: Vec<Node> = bot_chrome.iter()
            .map(|n| substitute_page_tokens(n, 1, 1))
            .collect();
        let (top_nodes, _) = layout_stack(&top_here, avail_x, avail_y, avail_w, None, 0.0, &Align::Stretch, &Justify::Start);
        let bot_h = measure_stack_height(&bot_here, avail_w);
        let bot_y = avail_y + avail_h - bot_h;
        let (bot_nodes, _) = layout_stack(&bot_here, avail_x, bot_y, avail_w, None, 0.0, &Align::Stretch, &Justify::Start);
        let mut all = top_nodes;
        all.extend(bot_nodes);
        pages_out.push(RenderPage {
            width: page.width, height: page.height,
            background: page.background.clone(), margin: page.margin,
            nodes: all,
        });
    }

    pages_out
}

/// Total stack height of a list of nodes at a given width (uses the standard
/// stack layout). Used to reserve top/bottom chrome space.
fn measure_stack_height(nodes: &[Node], avail_w: f32) -> f32 {
    if nodes.is_empty() { return 0.0; }
    let (_, h) = layout_stack(
        nodes, 0.0, 0.0, avail_w, None,
        0.0, &Align::Stretch, &Justify::Start,
    );
    h
}

/// Height of chrome nodes on a given page. When `include_first` is false,
/// repeat="first" nodes are excluded (i.e. what later pages look like).
fn measure_chrome_height(chrome: &[Node], avail_w: f32, include_first: bool) -> f32 {
    if chrome.is_empty() { return 0.0; }
    let selected: Vec<Node> = chrome.iter()
        .filter(|n| n.repeat == Repeat::Page || (include_first && n.repeat == Repeat::First))
        .cloned()
        .collect();
    measure_stack_height(&selected, avail_w)
}

// ── Page-number token substitution ────────────────────────────────────────────

/// Produce a copy of `node` with `{page}` / `{pages}` tokens inside any
/// descendant `<text>` replaced by the given page index / total pages.
fn substitute_page_tokens(node: &Node, page: usize, total: usize) -> Node {
    let mut n = node.clone();
    substitute_in_place(&mut n, page, total);
    n
}

fn substitute_in_place(n: &mut Node, page: usize, total: usize) {
    if n.kind == NodeKind::Text {
        for run in n.text_runs.iter_mut() {
            substitute_in_text(&mut run.text, page, total);
        }
    }
    for c in n.children.iter_mut() {
        substitute_in_place(c, page, total);
    }
}

fn substitute_in_text(s: &mut String, page: usize, total: usize) {
    if !s.contains('{') { return; }
    *s = s
        .replace("{page}", &page.to_string())
        .replace("{pages}", &total.to_string());
}

// ── Source-level pagination ───────────────────────────────────────────────────

/// Packs `nodes` (stacked vertically with `gap` between adjacent items) into
/// page-sized chunks. The first chunk gets `budget_first` of vertical space,
/// every subsequent chunk gets `budget_rest` (this lets callers reserve
/// different chrome on page 1 vs. later pages).
///
/// Splittable: Stack (Auto), Grid, Cluster, Text.
/// Atomic: Frame, Flank, Split, Divider, Link, plus anything with Fixed/Full/Fill.
///
/// When a node doesn't fit and can't be split, it moves wholesale to the next
/// page. If a single atomic node is taller than a full page, it is force-placed.
fn split_into_pages(
    nodes: &[Node],
    avail_w: f32,
    budget_first: f32,
    budget_rest: f32,
    gap: f32,
) -> Vec<Vec<Node>> {
    if nodes.is_empty() {
        return vec![vec![]];
    }

    let mut pages: Vec<Vec<Node>> = vec![vec![]];
    let mut used_h: f32 = 0.0;
    let mut queue: std::collections::VecDeque<Node> = nodes.iter().cloned().collect();

    let page_budget = |idx: usize| -> f32 {
        if idx == 0 { budget_first } else { budget_rest }
    };

    while let Some(node) = queue.pop_front() {
        let cur_idx = pages.len() - 1;
        let budget = page_budget(cur_idx);
        let gap_before = if pages.last().unwrap().is_empty() { 0.0 } else { gap };
        let h = measure_height(&node, avail_w, budget);
        let remaining = budget - used_h - gap_before;

        if h <= remaining + 0.5 {
            pages.last_mut().unwrap().push(node);
            used_h += gap_before + h;
        } else {
            let target = remaining.max(0.0);
            match split_node_at(&node, avail_w, target, budget) {
                SplitOutcome::First(first, rest) => {
                    pages.last_mut().unwrap().push(first);
                    pages.push(vec![]);
                    used_h = 0.0;
                    for item in rest.into_iter().rev() {
                        queue.push_front(item);
                    }
                }
                SplitOutcome::NothingFits(rest) => {
                    if pages.last().unwrap().is_empty() {
                        // Already on a fresh page and still can't split — force-place
                        // the first piece to prevent an infinite loop.
                        let first = rest.into_iter().next().unwrap_or(node);
                        let fh = measure_height(&first, avail_w, budget);
                        pages.last_mut().unwrap().push(first);
                        used_h = fh.min(budget);
                    } else {
                        pages.push(vec![]);
                        used_h = 0.0;
                        for item in rest.into_iter().rev() {
                            queue.push_front(item);
                        }
                    }
                }
                SplitOutcome::Atomic => {
                    if pages.last().unwrap().is_empty() {
                        pages.last_mut().unwrap().push(node);
                        used_h = budget;
                    } else {
                        pages.push(vec![]);
                        used_h = 0.0;
                        queue.push_front(node);
                    }
                }
            }
        }
    }

    while pages.len() > 1 && pages.last().map_or(true, |p| p.is_empty()) {
        pages.pop();
    }

    pages
}

/// The result of trying to split a node at a given height target.
enum SplitOutcome {
    /// Split succeeded: `first` fits within `target_h`; `rest` are the overflow nodes.
    First(Node, Vec<Node>),
    /// Nothing of this node fits within `target_h`; `rest` contains the node(s)
    /// that must be placed on the next page.
    NothingFits(Vec<Node>),
    /// Node is atomic and cannot be split at all.
    Atomic,
}

/// Try to split `node` so that a first part fits within `target_h` and the
/// remainder continues on subsequent pages.
///
/// Splittable node types (Auto height only):
/// - **Stack**: split child-by-child; recurses into first child if that's what overflows.
/// - **Grid**: split row-by-row (rows are atomic; children inside a row are atomic).
/// - **Cluster**: split between wrapped rows (items within a row are atomic).
/// - **Text**: split line-by-line, reconstructing runs on each half.
///
/// Everything else (Frame, Flank, Split, Divider, Link, or any Fixed/Full/Fill
/// height node) returns `Atomic`. Frame is atomic by design — it represents a
/// card-like enclosure that should not be cut.
fn split_node_at(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    if node.height_mode != HeightMode::Auto {
        return SplitOutcome::Atomic;
    }
    if node.kind != NodeKind::Text && node.children.is_empty() {
        return SplitOutcome::Atomic;
    }

    match node.kind {
        NodeKind::Stack   => split_stack(node, avail_w, target_h, full_page_h),
        NodeKind::Grid    => split_grid(node, avail_w, target_h, full_page_h),
        NodeKind::Cluster => split_cluster(node, avail_w, target_h),
        NodeKind::Text    => split_text(node, avail_w, target_h),
        NodeKind::Table   => split_table(node, avail_w, target_h, full_page_h),
        // Frame, Flank, Split, Divider, Link — atomic by design.
        _                 => SplitOutcome::Atomic,
    }
}

fn split_stack(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w      - pl - pr).max(0.0);
    let inner_target = (target_h     - pt - pb).max(0.0);
    let inner_full   = (full_page_h  - pt - pb).max(0.0);
    let gap = node.gap;
    let n   = node.children.len();

    let mut split_idx = 0usize;
    let mut chunk_h   = 0.0_f32;

    for (i, child) in node.children.iter().enumerate() {
        let h = measure_height(child, inner_w, inner_full);
        let g = if i == 0 { 0.0 } else { gap };
        if chunk_h + g + h > inner_target + 0.5 {
            break;
        }
        chunk_h += g + h;
        split_idx = i + 1;
    }

    if split_idx == n {
        return SplitOutcome::Atomic;
    }

    if split_idx == 0 {
        // First child itself overflows — try to recurse into it.
        match split_node_at(&node.children[0], inner_w, inner_target, inner_full) {
            SplitOutcome::First(child_first, child_rest) => {
                let first_node = Node { children: vec![child_first], ..node.clone() };
                let mut rest_children = child_rest;
                rest_children.extend_from_slice(&node.children[1..]);
                let rest_node = Node { children: rest_children, ..node.clone() };
                SplitOutcome::First(first_node, vec![rest_node])
            }
            SplitOutcome::NothingFits(_) | SplitOutcome::Atomic => {
                SplitOutcome::NothingFits(vec![node.clone()])
            }
        }
    } else {
        let first = Node { children: node.children[..split_idx].to_vec(), ..node.clone() };
        let rest  = Node { children: node.children[split_idx..].to_vec(), ..node.clone() };
        SplitOutcome::First(first, vec![rest])
    }
}

fn split_grid(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w      - pl - pr).max(0.0);
    let inner_target = (target_h     - pt - pb).max(0.0);
    let inner_full   = (full_page_h  - pt - pb).max(0.0);
    let gap  = node.gap;

    let cols = if let Some(min_w) = node.col_width {
        let n = ((inner_w + gap) / (min_w + gap)).floor() as usize;
        n.max(1)
    } else {
        (node.cols as usize).max(1)
    };
    let col_w = ((inner_w - gap * (cols - 1) as f32) / cols as f32).max(0.0);

    let n_rows = (node.children.len() + cols - 1) / cols;
    let mut split_row = 0usize;
    let mut chunk_h   = 0.0_f32;

    for row in 0..n_rows {
        let row_start = row * cols;
        let row_end   = (row_start + cols).min(node.children.len());
        let rh = node.children[row_start..row_end]
            .iter()
            .map(|c| measure_height(c, col_w, inner_full))
            .fold(0.0_f32, f32::max);
        let g = if row == 0 { 0.0 } else { gap };
        if chunk_h + g + rh > inner_target + 0.5 {
            break;
        }
        chunk_h += g + rh;
        split_row = row + 1;
    }

    if split_row == n_rows { return SplitOutcome::Atomic; }
    if split_row == 0      { return SplitOutcome::NothingFits(vec![node.clone()]); }

    let item_split = (split_row * cols).min(node.children.len());
    let first = Node { children: node.children[..item_split].to_vec(), ..node.clone() };
    let rest  = Node { children: node.children[item_split..].to_vec(), ..node.clone() };
    SplitOutcome::First(first, vec![rest])
}

/// Bucket items into wrapped rows first; the cluster splits between rows.
/// Items inside a row are treated as atomic (the user's rule: cluster children
/// never split).
fn split_cluster(node: &Node, avail_w: f32, target_h: f32) -> SplitOutcome {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w  - pl - pr).max(0.0);
    let inner_target = (target_h - pt - pb).max(0.0);
    let gap = node.gap;

    // Bucket by width, same logic as layout_cluster's pass 1.
    let mut rows: Vec<(usize, usize, f32)> = Vec::new(); // (start, end_exclusive, row_h)
    let mut row_start = 0usize;
    let mut cur_x     = 0.0_f32;
    let mut row_h     = 0.0_f32;
    for (i, child) in node.children.iter().enumerate() {
        let cw  = child.width_constraint
            .unwrap_or_else(|| measure_natural_w(child))
            .min(inner_w);
        let ch  = measure_height(child, cw, inner_target);
        let adv = if i == row_start { cw } else { gap + cw };
        if i > row_start && cur_x + adv > inner_w + 0.01 {
            rows.push((row_start, i, row_h));
            row_start = i;
            cur_x = cw;
            row_h = ch;
        } else {
            cur_x += adv;
            row_h = row_h.max(ch);
        }
    }
    if row_start < node.children.len() {
        rows.push((row_start, node.children.len(), row_h));
    }

    let n_rows = rows.len();
    let mut split_row = 0usize;
    let mut chunk_h   = 0.0_f32;
    for (r, (_, _, rh)) in rows.iter().enumerate() {
        let g = if r == 0 { 0.0 } else { gap };
        if chunk_h + g + rh > inner_target + 0.5 {
            break;
        }
        chunk_h += g + rh;
        split_row = r + 1;
    }

    if split_row == n_rows { return SplitOutcome::Atomic; }
    if split_row == 0      { return SplitOutcome::NothingFits(vec![node.clone()]); }

    let item_split = rows[split_row].0;
    let first = Node { children: node.children[..item_split].to_vec(), ..node.clone() };
    let rest  = Node { children: node.children[item_split..].to_vec(), ..node.clone() };
    SplitOutcome::First(first, vec![rest])
}

// ── Table ─────────────────────────────────────────────────────────────────────

/// Resolve a space-separated column-width spec into concrete pt values.
///
/// Units:
/// - `Nfr`  — fractional share of remaining width after fixed/percent columns.
/// - `Npt`  — absolute pt value.
/// - `N%`   — percentage of `avail_w`.
///
/// Example: `"2fr 1fr 120pt 20%"` with `avail_w=400` →
///   fixed = 120 + 80 = 200, remaining = 200, total_fr = 3,
///   fr_unit = 200/3 ≈ 66.7 → [133.3, 66.7, 120, 80].
fn resolve_col_widths(spec: &str, avail_w: f32) -> Vec<f32> {
    if spec.is_empty() {
        return vec![avail_w];
    }
    enum Unit { Fr(f32), Pt(f32), Pct(f32) }
    let units: Vec<Unit> = spec.split_whitespace().filter_map(|tok| {
        if let Some(s) = tok.strip_suffix("fr") {
            s.parse::<f32>().ok().map(Unit::Fr)
        } else if let Some(s) = tok.strip_suffix("pt") {
            s.parse::<f32>().ok().map(Unit::Pt)
        } else if let Some(s) = tok.strip_suffix('%') {
            s.parse::<f32>().ok().map(Unit::Pct)
        } else {
            tok.parse::<f32>().ok().map(Unit::Fr)
        }
    }).collect();
    if units.is_empty() {
        return vec![avail_w];
    }
    let fixed: f32 = units.iter().map(|u| match u {
        Unit::Pt(v)  => *v,
        Unit::Pct(v) => avail_w * v / 100.0,
        Unit::Fr(_)  => 0.0,
    }).sum();
    let total_fr: f32 = units.iter().map(|u| match u {
        Unit::Fr(v) => *v,
        _           => 0.0,
    }).sum();
    let remaining = (avail_w - fixed).max(0.0);
    let fr_unit = if total_fr > 0.0 { remaining / total_fr } else { 0.0 };
    units.iter().map(|u| match u {
        Unit::Fr(v)  => (v * fr_unit).max(0.0),
        Unit::Pt(v)  => *v,
        Unit::Pct(v) => (avail_w * v / 100.0).max(0.0),
    }).collect()
}

/// Measure the tallest cell height in a row at the given column widths.
fn measure_row_height(row: &Node, col_widths: &[f32], _avail_h: f32) -> f32 {
    row.children.iter().enumerate()
        .filter(|(j, _)| *j < col_widths.len())
        .map(|(j, cell)| {
            let (_, h) = layout_node(cell, 0.0, 0.0, col_widths[j], None);
            h
        })
        .fold(0.0_f32, f32::max)
}

fn layout_table(
    rows:      &[Node],
    x:         f32,
    y:         f32,
    avail_w:   f32,
    gap:       f32,
    cols_spec: &str,
    border:    Option<&(f32, String)>,
    stripe:    Option<&str>,
) -> (Vec<RenderNode>, f32) {
    if rows.is_empty() {
        return (vec![], 0.0);
    }

    let col_widths  = resolve_col_widths(cols_spec, avail_w);
    let n_cols      = col_widths.len();

    let mut nodes:       Vec<RenderNode> = Vec::new();
    let mut row_y        = y;
    let mut data_row_idx = 0usize;

    // Vectors to track geometry for border drawing.
    let mut row_ys:      Vec<f32> = Vec::with_capacity(rows.len());
    let mut row_heights: Vec<f32> = Vec::with_capacity(rows.len());

    for row in rows {
        let is_thead = row.kind == NodeKind::TableHead;
        row_ys.push(row_y);

        // Pass 1: measure the row's height (max over all cells).
        let row_h = measure_row_height(row, &col_widths, f32::MAX);
        row_heights.push(row_h);

        // Determine row background (stripe overrides only when no explicit bg).
        let row_bg: Option<String> = if is_thead {
            row.background.clone()
        } else {
            let stripe_bg = if data_row_idx % 2 == 1 {
                stripe.filter(|_| row.background.is_none()).map(str::to_string)
            } else {
                None
            };
            row.background.clone().or(stripe_bg)
        };

        // Emit row background box (must be below cells in draw order).
        if let Some(bg) = row_bg {
            nodes.push(RenderNode::Box(RenderBox {
                x, y: row_y, width: avail_w, height: row_h,
                fill: Some(bg),
                border_width: 0.0, border_color: None, radius: 0.0,
                debug_self: false, children: vec![],
            }));
        }

        // Pass 2: lay out each cell at its column position, stretched to row_h.
        let mut cell_x = x;
        for (j, cell) in row.children.iter().enumerate().take(n_cols) {
            let col_w   = col_widths[j];
            let stretched = Node { height_mode: crate::parse::HeightMode::Full, ..cell.clone() };
            let (rn, _) = layout_node(&stretched, cell_x, row_y, col_w, Some(row_h));
            nodes.push(rn);
            cell_x += col_w + gap;
        }

        row_y += row_h + gap;
        if !is_thead { data_row_idx += 1; }
    }

    let total_h: f32 = row_heights.iter().sum::<f32>()
        + gap * rows.len().saturating_sub(1) as f32;

    // Draw grid borders.
    if let Some((bw, bclr)) = border {
        let half         = bw / 2.0;
        let table_bottom = y + total_h;

        // Outer left / right verticals (full table height).
        nodes.push(RenderNode::Line(RenderLine {
            x1: x + half, y1: y, x2: x + half, y2: table_bottom,
            color: bclr.clone(), thickness: *bw, dash: None,
        }));
        nodes.push(RenderNode::Line(RenderLine {
            x1: x + avail_w - half, y1: y, x2: x + avail_w - half, y2: table_bottom,
            color: bclr.clone(), thickness: *bw, dash: None,
        }));

        // Horizontal: top of table + bottom of each row.
        nodes.push(RenderNode::Line(RenderLine {
            x1: x, y1: y + half, x2: x + avail_w, y2: y + half,
            color: bclr.clone(), thickness: *bw, dash: None,
        }));
        let mut ry = y;
        for rh in &row_heights {
            ry += rh;
            nodes.push(RenderNode::Line(RenderLine {
                x1: x, y1: ry - half, x2: x + avail_w, y2: ry - half,
                color: bclr.clone(), thickness: *bw, dash: None,
            }));
            ry += gap;
        }

        // Vertical column separators: one per adjacent column pair, per row.
        for (&ry_start, &rh) in row_ys.iter().zip(row_heights.iter()) {
            let mut vx = x;
            for j in 0..n_cols {
                vx += col_widths[j];
                if j < n_cols - 1 {
                    let sep_x = vx + gap / 2.0;
                    nodes.push(RenderNode::Line(RenderLine {
                        x1: sep_x, y1: ry_start,
                        x2: sep_x, y2: ry_start + rh,
                        color: bclr.clone(), thickness: *bw, dash: None,
                    }));
                    vx += gap;
                }
            }
        }
    }

    (nodes, total_h)
}

fn split_table(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w     - pl - pr).max(0.0);
    let inner_target = (target_h    - pt - pb).max(0.0);
    let inner_full   = (full_page_h - pt - pb).max(0.0);
    let gap          = node.gap;

    let col_widths  = resolve_col_widths(&node.table_cols, inner_w);
    let has_thead   = node.children.first()
        .map_or(false, |c| c.kind == NodeKind::TableHead);

    let n_rows = node.children.len();
    let mut split_idx = 0usize;
    let mut chunk_h   = 0.0_f32;

    for (i, row) in node.children.iter().enumerate() {
        let rh = measure_row_height(row, &col_widths, inner_full);
        let g  = if i == 0 { 0.0 } else { gap };
        if chunk_h + g + rh > inner_target + 0.5 {
            break;
        }
        chunk_h   += g + rh;
        split_idx  = i + 1;
    }

    if split_idx == n_rows { return SplitOutcome::Atomic; }

    // Nothing useful fits: need at least thead (if present) + one data row.
    let min_useful = if has_thead { 2 } else { 1 };
    if split_idx < min_useful {
        return SplitOutcome::NothingFits(vec![node.clone()]);
    }

    let first = Node { children: node.children[..split_idx].to_vec(), ..node.clone() };

    // Carry thead onto the continuation page.
    let mut rest_children = node.children[split_idx..].to_vec();
    if has_thead {
        rest_children.insert(0, node.children[0].clone());
    }
    let rest = Node { children: rest_children, ..node.clone() };
    SplitOutcome::First(first, vec![rest])
}

fn split_text(node: &Node, avail_w: f32, target_h: f32) -> SplitOutcome {
    let line_h  = node.font_size * 1.2;
    let lines   = wrap_text_split(node, avail_w);
    let n_lines = lines.len();
    if n_lines <= 1 || line_h <= 0.0 {
        return SplitOutcome::Atomic;
    }
    let max_lines = ((target_h / line_h).floor().max(0.0)) as usize;
    if max_lines >= n_lines {
        return SplitOutcome::Atomic;
    }
    if max_lines == 0 {
        return SplitOutcome::NothingFits(vec![node.clone()]);
    }
    let first_atoms: Vec<SplitAtom> = lines[..max_lines].iter().flatten().cloned().collect();
    let rest_atoms:  Vec<SplitAtom> = lines[max_lines..].iter().flatten().cloned().collect();
    let first = Node { text_runs: atoms_to_runs(&first_atoms), ..node.clone() };
    let rest  = Node { text_runs: atoms_to_runs(&rest_atoms),  ..node.clone() };
    SplitOutcome::First(first, vec![rest])
}

// ── Text splitting helpers ────────────────────────────────────────────────────

#[derive(Clone)]
struct SplitAtom {
    text:           String,
    font_override:  Option<String>,
    color_override: Option<String>,
    href:           Option<String>,
    underline:      bool,
    strike:         bool,
    /// Whether this atom needs a leading space when concatenated after the
    /// previous atom in the output. For the first atom in a line, ignore.
    leading_space:  bool,
    /// True if this atom came from a `<span>`; spans stay as single atoms
    /// (not word-sliced) to keep formatting intact.
    is_span:        bool,
}

/// Tokenize text_runs + wrap into lines. Mirrors the shape of `layout_text`'s
/// wrap step but preserves the overrides needed to rebuild `TextRun`s.
fn wrap_text_split(node: &Node, avail_w: f32) -> Vec<Vec<SplitAtom>> {
    let font_size   = node.font_size;
    let parent_font = node.font.as_str();
    let space_w     = text_width(parent_font, " ", font_size);

    let mut atoms: Vec<SplitAtom> = Vec::new();
    let mut pending_leading = false;
    for (ri, run) in node.text_runs.iter().enumerate() {
        let is_span = run.font.is_some() || run.color.is_some()
                   || run.href.is_some() || run.underline || run.strike;

        let run_leading = ri > 0 && run.leading_space;

        if is_span {
            if !run.text.is_empty() {
                atoms.push(SplitAtom {
                    text: run.text.clone(),
                    font_override: run.font.clone(),
                    color_override: run.color.clone(),
                    href: run.href.clone(),
                    underline: run.underline,
                    strike: run.strike,
                    leading_space: pending_leading || run_leading,
                    is_span: true,
                });
                pending_leading = false;
            }
        } else {
            for (j, word) in run.text.split_whitespace().enumerate() {
                atoms.push(SplitAtom {
                    text: word.to_string(),
                    font_override: None,
                    color_override: None,
                    href: None,
                    underline: false,
                    strike: false,
                    leading_space: if j == 0 { pending_leading || run_leading } else { true },
                    is_span: false,
                });
                pending_leading = false;
            }
        }
    }

    if atoms.is_empty() { return vec![]; }

    // Greedy wrap.
    let mut lines: Vec<Vec<SplitAtom>> = Vec::new();
    let mut cur_line: Vec<SplitAtom> = Vec::new();
    let mut cur_w: f32 = 0.0;

    for atom in atoms {
        let font_for_atom = atom.font_override.as_deref().unwrap_or(parent_font);
        let aw = text_width(font_for_atom, &atom.text, font_size);
        let gap = if cur_line.is_empty() || !atom.leading_space { 0.0 } else { space_w };
        if cur_line.is_empty() || cur_w + gap + aw <= avail_w + 0.01 {
            cur_w += gap + aw;
            cur_line.push(atom);
        } else {
            lines.push(std::mem::take(&mut cur_line));
            cur_w = aw;
            // First atom of a fresh line has no leading space visually.
            let mut first = atom;
            first.leading_space = false;
            cur_line.push(first);
        }
    }
    if !cur_line.is_empty() { lines.push(cur_line); }
    lines
}

/// Rebuild `TextRun`s from a flat list of atoms. Consecutive plain atoms with
/// the same (empty) override set are coalesced into one run; span atoms become
/// one run each (they carry arbitrary overrides that can't be coalesced safely).
fn atoms_to_runs(atoms: &[SplitAtom]) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut first_emitted = false;

    for atom in atoms {
        let leading = if !first_emitted { false } else { atom.leading_space };

        if atom.is_span {
            runs.push(TextRun {
                text: atom.text.clone(),
                leading_space: leading,
                font: atom.font_override.clone(),
                color: atom.color_override.clone(),
                href: atom.href.clone(),
                underline: atom.underline,
                strike: atom.strike,
            });
        } else {
            // Try to append to the last plain run.
            let can_append = runs.last().map_or(false, |r| {
                r.font.is_none() && r.color.is_none() && r.href.is_none()
                    && !r.underline && !r.strike
            });
            if can_append {
                let r = runs.last_mut().unwrap();
                if leading { r.text.push(' '); }
                r.text.push_str(&atom.text);
            } else {
                runs.push(TextRun {
                    text: atom.text.clone(),
                    leading_space: leading,
                    font: None,
                    color: None,
                    href: None,
                    underline: false,
                    strike: false,
                });
            }
        }
        first_emitted = true;
    }
    runs
}

/// Return the height that `node` would occupy when laid out at `avail_w`
/// with `avail_h` as the reference for Full/Fill children.
fn measure_height(node: &Node, avail_w: f32, avail_h: f32) -> f32 {
    match &node.height_mode {
        HeightMode::Fixed(h) => *h,
        HeightMode::Full | HeightMode::Fill => avail_h,
        HeightMode::Auto => {
            let cw = node
                .width_constraint
                .map(|w| w.min(avail_w))
                .unwrap_or(avail_w);
            let (_, h) = layout_node(node, 0.0, 0.0, cw, Some(avail_h));
            h
        }
    }
}

// ── Per-node layout ───────────────────────────────────────────────────────────

/// Returns (rendered node, actual height).
fn layout_node(
    node: &Node,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) -> (RenderNode, f32) {
    // Resolve actual width from constraint or available
    let (node_w, node_x) = match node.width_constraint {
        Some(w) => (w.min(avail_w), x),
        None => (avail_w, x),
    };

    match node.kind {
        NodeKind::Divider => layout_divider(node, node_x, y, node_w, avail_h),
        NodeKind::Text => layout_text(node, node_x, y, node_w),
        NodeKind::Link => layout_link(node, node_x, y, node_w, avail_h),
        NodeKind::Img => layout_img(node, node_x, y, node_w),
        NodeKind::Barcode => layout_barcode(node, node_x, y, node_w),
        _ => layout_container(node, node_x, y, node_w, avail_h),
    }
}

// ── Container layout (box + inner children) ───────────────────────────────────

fn layout_container(
    node: &Node,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) -> (RenderNode, f32) {
    let [pt, pr, pb, pl] = node.padding;
    let inner_x = x + pl;
    let inner_y = y + pt;
    let inner_w = (avail_w - pl - pr).max(0.0);
    let inner_h = match &node.height_mode {
        HeightMode::Auto => None,
        HeightMode::Fixed(h) => Some((*h - pt - pb).max(0.0)),
        HeightMode::Full | HeightMode::Fill => avail_h.map(|h| (h - pt - pb).max(0.0)),
    };

    let (children, content_h) = dispatch_children(node, inner_x, inner_y, inner_w, inner_h);

    let outer_h = match &node.height_mode {
        HeightMode::Fixed(h) => *h,
        HeightMode::Full | HeightMode::Fill => avail_h.unwrap_or(content_h + pt + pb),
        HeightMode::Auto => content_h + pt + pb,
    };

    let (border_width, border_color) = if node.kind == NodeKind::Table {
        // Table border controls cell grid lines (drawn by layout_table), not the outer box.
        (0.0, None)
    } else {
        node.border
            .as_ref()
            .map(|(t, c)| (*t, Some(c.clone())))
            .unwrap_or((0.0, None))
    };

    (
        RenderNode::Box(RenderBox {
            x,
            y,
            width: avail_w,
            height: outer_h,
            fill: node.background.clone(),
            border_width,
            border_color,
            radius: node.radius,
            debug_self: node.debug,
            children,
        }),
        outer_h,
    )
}

fn dispatch_children(
    node: &Node,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) -> (Vec<RenderNode>, f32) {
    match node.kind {
        NodeKind::Stack => layout_stack(
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.align, &node.justify,
        ),
        NodeKind::Flank => layout_flank(
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.align, node.end,
        ),
        NodeKind::Split => layout_split(
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.align, node.equal,
        ),
        NodeKind::Cluster => layout_cluster(
            &node.children, x, y, avail_w,
            node.gap, &node.justify, &node.align,
        ),
        NodeKind::Grid => layout_grid(
            &node.children, x, y, avail_w,
            node.gap, node.cols, node.col_width,
        ),
        NodeKind::Frame => layout_stack(
            // Frame always centers its single child (horizontally via Align::Center,
            // vertically via Justify::Center when avail_h is known).
            &node.children, x, y, avail_w, avail_h,
            0.0, &Align::Center, &Justify::Center,
        ),
        NodeKind::Link => layout_stack(
            &node.children, x, y, avail_w, avail_h,
            0.0, &Align::Stretch, &Justify::Start,
        ),
        NodeKind::Table => layout_table(
            &node.children, x, y, avail_w,
            node.gap, &node.table_cols, node.border.as_ref(), node.stripe.as_deref(),
        ),
        NodeKind::TableHead | NodeKind::TableRow | NodeKind::TableCell => layout_stack(
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.align, &node.justify,
        ),
        _ => (vec![], 0.0),
    }
}

// ── Stack ─────────────────────────────────────────────────────────────────────

fn layout_stack(
    children: &[Node],
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    gap: f32,
    align: &Align,
    justify: &Justify,
) -> (Vec<RenderNode>, f32) {
    if children.is_empty() {
        return (vec![], 0.0);
    }

    let n = children.len();
    let gap_total = gap * (n - 1) as f32;

    // ── Pass 1: measure non-fill children ─────────────────────────────────────
    let mut sized: Vec<Option<f32>> = vec![None; n];
    let mut sum_fixed = 0.0_f32;
    let mut n_fill = 0usize;

    for (i, child) in children.iter().enumerate() {
        match &child.height_mode {
            HeightMode::Fill => n_fill += 1,
            HeightMode::Full => {
                // Deferred — needs avail_h
            }
            HeightMode::Fixed(h) => {
                sized[i] = Some(*h);
                sum_fixed += h;
            }
            HeightMode::Auto => {
                let cw = resolve_child_w(child, avail_w, align);
                let (_, h) = layout_node(child, 0.0, 0.0, cw, None);
                sized[i] = Some(h);
                sum_fixed += h;
            }
        }
    }

    // ── Compute fill / full heights ───────────────────────────────────────────
    let fill_h = if n_fill > 0 {
        let budget = avail_h.unwrap_or(sum_fixed + gap_total);
        ((budget - sum_fixed - gap_total) / n_fill as f32).max(0.0)
    } else {
        0.0
    };

    let full_h = avail_h.unwrap_or(sum_fixed + gap_total);

    for (i, child) in children.iter().enumerate() {
        if sized[i].is_none() {
            sized[i] = Some(match &child.height_mode {
                HeightMode::Fill => fill_h,
                HeightMode::Full => full_h,
                _ => unreachable!(),
            });
        }
    }

    let heights: Vec<f32> = sized.into_iter().map(|h| h.unwrap_or(0.0)).collect();
    let total_h = heights.iter().sum::<f32>() + gap_total;

    // ── Justify: starting y and extra gap ────────────────────────────────────
    let avail = avail_h.unwrap_or(total_h);
    let remaining = (avail - total_h).max(0.0);

    let (start_y, extra_gap) = match justify {
        Justify::Start => (y, 0.0),
        Justify::Center => (y + remaining / 2.0, 0.0),
        Justify::End => (y + remaining, 0.0),
        Justify::Between => {
            if n > 1 {
                (y, remaining / (n - 1) as f32)
            } else {
                (y, 0.0)
            }
        }
    };

    // ── Pass 2: position children ────────────────────────────────────────────
    let mut nodes = Vec::with_capacity(n);
    let mut cur_y = start_y;

    for (i, child) in children.iter().enumerate() {
        let h = heights[i];
        let cw = resolve_child_w(child, avail_w, align);
        // Frames with an explicit width constraint centre horizontally by default
        let effective_align =
            if matches!(child.kind, NodeKind::Frame) && child.width_constraint.is_some() {
                &Align::Center
            } else {
                align
            };
        let cx = cross_x(x, avail_w, cw, effective_align);

        // For Text nodes the box always spans the full available width, so
        // cross_x has no visible effect.  Translate the parent's cross-axis
        // alignment into text_align so the text actually appears centred/right.
        let text_align_override = if child.kind == NodeKind::Text {
            match align {
                Align::Center => Some(TextAlign::Center),
                Align::End    => Some(TextAlign::Right),
                _             => None,
            }
        } else {
            None
        };
        let tmp_node: Node;
        let child_ref: &Node = if let Some(ta) = text_align_override {
            tmp_node = Node { text_align: ta, ..child.clone() };
            &tmp_node
        } else {
            child
        };
        let (rn, _) = layout_node(child_ref, cx, cur_y, cw, Some(h));
        nodes.push(rn);
        cur_y += h + gap + extra_gap;
    }

    (nodes, total_h)
}

// ── Flank ─────────────────────────────────────────────────────────────────────
//
// end=false (default, flanks at START):
//   children[0..n-2] are the flank children (natural/explicit width) placed at the left;
//   children[n-1] is the fill child and takes all remaining width on the right.
//
// end=true (flanks at END):
//   children[0] is the fill child on the left;
//   children[1..n] are flank children (natural/explicit width) placed at the right.
//
// The gap value is applied between every adjacent pair of items, so the total
// gap consumed equals gap × n_flanks (one between each flank pair plus one
// between the flank group and the fill child).

fn layout_flank(
    children: &[Node],
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    gap: f32,
    align: &Align,
    end: bool,
) -> (Vec<RenderNode>, f32) {
    if children.is_empty() {
        return (vec![], 0.0);
    }
    if children.len() == 1 {
        return layout_stack(children, x, y, avail_w, avail_h, gap, align, &Justify::Start);
    }

    let (fill_child, flank_children): (&Node, &[Node]) = if end {
        // fill on left, flanks on right
        (&children[0], &children[1..])
    } else {
        // flanks on left, fill on right
        (&children[children.len() - 1], &children[..children.len() - 1])
    };

    let n_flanks = flank_children.len();

    // Measure each flank child's width (explicit constraint or natural content width).
    let flank_ws: Vec<f32> = flank_children
        .iter()
        .map(|c| c.width_constraint.unwrap_or_else(|| measure_natural_w(c)).min(avail_w))
        .collect();

    // Total space consumed by flanks: their widths + (n_flanks - 1) internal gaps
    // + 1 gap between the flank group and the fill child.
    let flank_w_sum: f32 = flank_ws.iter().sum();
    let total_gap = gap * n_flanks as f32; // (n_flanks - 1) + 1
    let fill_w = (avail_w - flank_w_sum - total_gap).max(0.0);

    let (fill_x, flanks_start_x) = if end {
        (x, x + fill_w + gap)
    } else {
        (x + flank_w_sum + total_gap, x)
    };

    let mut render_nodes: Vec<RenderNode> = Vec::with_capacity(children.len());
    let (fill_node, fill_h) = layout_node(fill_child, fill_x, y, fill_w, avail_h);

    let mut flank_results: Vec<(RenderNode, f32)> = Vec::with_capacity(n_flanks);
    let mut cur_x = flanks_start_x;
    for (flank_child, &fw) in flank_children.iter().zip(flank_ws.iter()) {
        let (node, h) = layout_node(flank_child, cur_x, y, fw, avail_h);
        flank_results.push((node, h));
        cur_x += fw + gap;
    }

    let row_h = flank_results
        .iter()
        .fold(fill_h, |acc, (_, h)| acc.max(*h));

    let fill_node = shift_y_cross(fill_node, fill_h, row_h, align);

    if end {
        render_nodes.push(fill_node);
        for (node, h) in flank_results {
            render_nodes.push(shift_y_cross(node, h, row_h, align));
        }
    } else {
        for (node, h) in flank_results {
            render_nodes.push(shift_y_cross(node, h, row_h, align));
        }
        render_nodes.push(fill_node);
    }

    (render_nodes, row_h)
}

// ── Split ─────────────────────────────────────────────────────────────────────

fn layout_split(
    children: &[Node],
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    gap: f32,
    align: &Align,
    equal: bool,
) -> (Vec<RenderNode>, f32) {
    if children.len() < 2 {
        return layout_stack(children, x, y, avail_w, avail_h, gap, align, &Justify::Start);
    }

    let (first, second) = (&children[0], &children[1]);

    let (first_w, second_w, second_x) = if equal {
        let w = ((avail_w - gap) / 2.0).max(0.0);
        (w, w, x + w + gap)
    } else {
        // Each child takes its natural width (or its explicit width constraint).
        // First child is left-aligned; second child is right-aligned (spread apart).
        let fw = first
            .width_constraint
            .unwrap_or_else(|| measure_natural_w(first))
            .min(avail_w);
        // Cap second child to the space remaining after the first child, so the
        // pair can never overflow avail_w regardless of natural content width.
        let sw_remaining = (avail_w - fw - gap).max(0.0);
        let sw = second
            .width_constraint
            .unwrap_or_else(|| measure_natural_w(second))
            .min(sw_remaining);
        // Second child is pushed to the right edge; first keeps the minimum gap.
        let sx = (x + avail_w - sw).max(x + fw + gap);
        (fw, sw, sx)
    };

    let (first_node, first_h) = layout_node(first, x, y, first_w, avail_h);
    let (second_node, second_h) =
        layout_node(second, second_x, y, second_w, avail_h);

    let row_h = first_h.max(second_h);
    let first_node = shift_y_cross(first_node, first_h, row_h, align);
    let second_node = shift_y_cross(second_node, second_h, row_h, align);

    (vec![first_node, second_node], row_h)
}

// ── Cluster ───────────────────────────────────────────────────────────────────

fn layout_cluster(
    children: &[Node],
    x: f32,
    y: f32,
    avail_w: f32,
    gap: f32,
    justify: &Justify,
    align: &Align,
) -> (Vec<RenderNode>, f32) {
    if children.is_empty() {
        return (vec![], 0.0);
    }

    // ── Pass 1: measure children and bucket into rows ─────────────────────────
    struct RowItem {
        node:     RenderNode,
        w:        f32,
        h:        f32,
        layout_x: f32, // x used when layout_node was called (for repositioning)
    }

    let mut rows: Vec<Vec<RowItem>> = Vec::new();
    let mut current_row: Vec<RowItem> = Vec::new();
    let mut cur_x = x;

    for child in children {
        let cw = child.width_constraint
            .unwrap_or_else(|| measure_natural_w(child))
            .min(avail_w);

        // Wrap check before layout so we use the correct x position
        if !current_row.is_empty() && cur_x + cw > x + avail_w + 0.01 {
            rows.push(std::mem::take(&mut current_row));
            cur_x = x;
        }

        let layout_x = cur_x;
        let (rn, h) = layout_node(child, layout_x, y, cw, None);
        current_row.push(RowItem { node: rn, w: cw, h, layout_x });
        cur_x += cw + gap;
    }
    if !current_row.is_empty() {
        rows.push(current_row);
    }

    // ── Pass 2: apply justify + align and collect final nodes ─────────────────
    let mut nodes = Vec::new();
    let mut cur_y = y;
    let mut total_h = 0.0_f32;

    for row in rows {
        let row_h = row.iter().map(|i| i.h).fold(0.0_f32, f32::max);
        let row_w: f32 = row.iter().map(|i| i.w).sum::<f32>()
            + gap * (row.len().saturating_sub(1)) as f32;

        // Justify: compute starting x for this row
        let remaining_w = (avail_w - row_w).max(0.0);
        let row_start_x = match justify {
            Justify::Start   => x,
            Justify::Center  => x + remaining_w / 2.0,
            Justify::End     => x + remaining_w,
            Justify::Between => x, // not reachable for cluster; parse rejects it
        };

        let mut item_x = row_start_x;
        for item in row {
            // Cross-axis (vertical) alignment within this row
            let dy_align = match align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (row_h - item.h) / 2.0,
                Align::End    => row_h - item.h,
            };

            // Compute deltas relative to where layout_node placed the node
            let dx = item_x - item.layout_x;
            let dy = (cur_y + dy_align) - y; // pass-1 used y as the y placeholder

            let repositioned = shift_x(shift_y(item.node, dy), dx);
            nodes.push(repositioned);
            item_x += item.w + gap;
        }

        cur_y += row_h + gap;
        total_h += row_h + gap;
    }

    if total_h > 0.0 {
        total_h -= gap;
    }

    (nodes, total_h)
}

// ── Grid ──────────────────────────────────────────────────────────────────────

fn layout_grid(
    children: &[Node],
    x: f32,
    y: f32,
    avail_w: f32,
    gap: f32,
    cols: u32,
    col_width: Option<f32>,
) -> (Vec<RenderNode>, f32) {
    if children.is_empty() {
        return (vec![], 0.0);
    }

    let cols = if let Some(min_w) = col_width {
        // auto-fit: pack as many columns as fit at >= min_w each, then stretch to fill
        let n = ((avail_w + gap) / (min_w + gap)).floor() as usize;
        n.max(1)
    } else {
        cols.max(1) as usize
    };
    let col_w = ((avail_w - gap * (cols - 1) as f32) / cols as f32).max(0.0);

    let mut nodes = Vec::new();
    let mut total_h = 0.0_f32;
    let mut row_y = y;

    for chunk in children.chunks(cols) {
        // Pass 1: measure every item to find the tallest in this row
        let mut row_h = 0.0_f32;
        for (j, child) in chunk.iter().enumerate() {
            let cx = x + j as f32 * (col_w + gap);
            let (_, h) = layout_node(child, cx, row_y, col_w, None);
            row_h = row_h.max(h);
        }

        // Pass 2: re-layout with avail_h = row_h so every item stretches to row height
        // (only Auto items are promoted — Fixed/Full/Fill keep their own height mode)
        for (j, child) in chunk.iter().enumerate() {
            let cx = x + j as f32 * (col_w + gap);
            let stretched;
            let child_ref: &Node = if child.height_mode == HeightMode::Auto {
                stretched = Node { height_mode: HeightMode::Full, ..child.clone() };
                &stretched
            } else {
                child
            };
            let (rn, _) = layout_node(child_ref, cx, row_y, col_w, Some(row_h));
            nodes.push(rn);
        }

        row_y += row_h + gap;
        total_h += row_h + gap;
    }

    if total_h > 0.0 {
        total_h -= gap; // remove trailing gap after last row
    }

    (nodes, total_h)
}

// ── Link ──────────────────────────────────────────────────────────────────────

fn layout_link(
    node: &Node,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) -> (RenderNode, f32) {
    let url = node.url.clone().unwrap_or_default();
    let (children, content_h) = layout_stack(
        &node.children, x, y, avail_w, avail_h,
        node.gap, &node.align, &Justify::Start,
    );
    let height = match &node.height_mode {
        HeightMode::Fixed(h) => *h,
        HeightMode::Full | HeightMode::Fill => avail_h.unwrap_or(content_h),
        HeightMode::Auto => content_h,
    };
    (
        RenderNode::Link(RenderLink { url, x, y, width: avail_w, height, debug_self: node.debug, children }),
        height,
    )
}

// ── Image ─────────────────────────────────────────────────────────────────────

fn layout_img(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32) {
    // width_constraint and img_height_constraint are pre-filled by prefill_image_sizes.
    // If somehow missing, fall back to a square box at available width.
    let w = node.width_constraint.unwrap_or(avail_w).min(avail_w);
    let h = node.img_height_constraint.unwrap_or(w);
    let name = node.image_name.clone().unwrap_or_default();
    (RenderNode::Image(RenderImage { x, y, width: w, height: h, name }), h)
}

// ── Barcode ───────────────────────────────────────────────────────────────────

fn layout_barcode(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32) {
    let btype = match &node.barcode_type {
        Some(t) => t.clone(),
        None    => return fallback_barcode(x, y, avail_w, "missing barcode type"),
    };
    let data = match &node.barcode_data {
        Some(d) => d.clone(),
        None    => return fallback_barcode(x, y, avail_w, "missing barcode data"),
    };

    let color = node.barcode_color.clone().unwrap_or_else(|| "#000000".to_string());
    let bg    = node.barcode_bg.clone();

    match btype {
        BarcodeType::Qr => {
            let size_pt = node.width_constraint.unwrap_or(80.0).min(avail_w);
            let ec = match node.barcode_ec {
                BarcodeEcLevel::L => qrcode::EcLevel::L,
                BarcodeEcLevel::M => qrcode::EcLevel::M,
                BarcodeEcLevel::Q => qrcode::EcLevel::Q,
                BarcodeEcLevel::H => qrcode::EcLevel::H,
            };
            match qrcode::QrCode::with_error_correction_level(data.as_bytes(), ec) {
                Ok(code) => {
                    let sz = code.width() as u32;
                    let mut modules = Vec::with_capacity((sz * sz) as usize);
                    for row in 0..sz as usize {
                        for col in 0..sz as usize {
                            modules.push(code[(col, row)] == qrcode::Color::Dark);
                        }
                    }
                    let bc = RenderBarcode {
                        x, y, width: size_pt, height: size_pt,
                        kind: RenderedBarcodeKind::Qr { modules, size: sz },
                        color, bg, debug_self: node.debug,
                    };
                    (RenderNode::Barcode(bc), size_pt)
                }
                Err(_) => fallback_barcode(x, y, size_pt, "QR encoding failed"),
            }
        }

        BarcodeType::Code128 => {
            let w = node.width_constraint.unwrap_or(avail_w).min(avail_w);
            let hrt_text = if node.barcode_hrt { Some(data.clone()) } else { None };
            let hrt_h    = if node.barcode_hrt { 12.0_f32 } else { 0.0 };
            let h = node.img_height_constraint.unwrap_or(40.0) + hrt_h;

            match encode_code128(&data) {
                Ok(bars) => {
                    let bc = RenderBarcode {
                        x, y, width: w, height: h,
                        kind: RenderedBarcodeKind::Code128 { bars, hrt: hrt_text },
                        color, bg, debug_self: node.debug,
                    };
                    (RenderNode::Barcode(bc), h)
                }
                Err(msg) => fallback_barcode(x, y, w, &msg),
            }
        }

        BarcodeType::Ean13 => {
            let w = node.width_constraint.unwrap_or(avail_w).min(avail_w);
            let h = node.img_height_constraint.unwrap_or(60.0);

            match encode_ean13(&data) {
                Ok(bars) => {
                    let bc = RenderBarcode {
                        x, y, width: w, height: h,
                        kind: RenderedBarcodeKind::Ean13 { bars, digits: data, hrt: node.barcode_hrt },
                        color, bg, debug_self: node.debug,
                    };
                    (RenderNode::Barcode(bc), h)
                }
                Err(msg) => fallback_barcode(x, y, w, &msg),
            }
        }
    }
}

/// Return an empty box placeholder for an invalid/unencodable barcode.
fn fallback_barcode(x: f32, y: f32, w: f32, _reason: &str) -> (RenderNode, f32) {
    let h = 40.0_f32;
    let b = crate::render::RenderBox {
        x, y, width: w, height: h,
        fill: Some("#eeeeee".to_string()),
        border_width: 0.5,
        border_color: Some("#ff0000".to_string()),
        radius: 0.0,
        debug_self: false,
        children: vec![],
    };
    (RenderNode::Box(b), h)
}

// ── Code 128 encoder ─────────────────────────────────────────────────────────
//
// Encodes a string using subset B (printable ASCII 32–126) into a flat
// alternating bar/space run-length array starting with a bar.
// Layout: START_B | data symbols | checksum | STOP

/// Code 128 symbol patterns indexed 0–105 (each 6 elements, 11 units total).
/// Elements alternate: bar, space, bar, space, bar, space.
static CODE128: [[u8; 6]; 106] = [
    [2,1,2,2,2,2], [2,2,2,1,2,2], [2,2,2,2,2,1], [1,2,1,2,2,3],
    [1,2,1,3,2,2], [1,3,1,2,2,2], [1,2,2,2,1,3], [1,2,2,3,1,2],
    [1,3,2,2,1,2], [2,2,1,2,1,3], [2,2,1,3,1,2], [2,3,1,2,1,2],
    [1,1,2,2,3,2], [1,2,2,1,3,2], [1,2,2,2,3,1], [1,1,3,2,2,2],
    [1,2,3,1,2,2], [1,2,3,2,2,1], [2,2,3,2,1,1], [2,2,1,1,3,2],
    [2,2,1,2,3,1], [2,1,3,2,1,2], [2,2,3,1,1,2], [3,1,2,1,3,1],
    [3,1,1,2,2,2], [3,2,1,1,2,2], [3,2,1,2,2,1], [3,1,2,2,1,2],
    [3,2,2,1,1,2], [3,2,2,2,1,1], [2,1,2,1,2,3], [2,1,2,3,2,1],
    [2,3,2,1,2,1], [1,1,1,3,2,3], [1,3,1,1,2,3], [1,3,1,3,2,1],
    [1,1,2,3,1,3], [1,3,2,1,1,3], [1,3,2,3,1,1], [2,1,1,3,1,3],
    [2,3,1,1,1,3], [2,3,1,3,1,1], [1,1,2,1,3,3], [1,1,2,3,3,1],
    [1,3,2,1,3,1], [1,1,3,1,2,3], [1,1,3,3,2,1], [1,3,3,1,2,1],
    [3,1,3,1,2,1], [2,1,1,3,3,1], [2,3,1,1,3,1], [2,1,3,1,1,3],
    [2,1,3,3,1,1], [2,1,3,1,3,1], [3,1,1,1,2,3], [3,1,1,3,2,1],
    [3,3,1,1,2,1], [3,1,2,1,1,3], [3,1,2,3,1,1], [3,3,2,1,1,1],
    [3,1,4,1,1,1], [2,2,4,2,1,1], [4,3,1,1,1,1], [1,1,1,2,2,4],
    [1,1,1,4,2,2], [1,2,1,1,2,4], [1,2,1,4,2,1], [1,4,1,1,2,2],
    [1,4,1,2,2,1], [1,1,2,2,1,4], [1,1,2,4,1,2], [1,2,2,1,1,4],
    [1,2,2,4,1,1], [1,4,2,1,1,2], [1,4,2,2,1,1], [2,4,1,2,1,1],
    [2,2,1,1,1,4], [4,1,3,1,1,1], [2,4,1,1,1,2], [1,3,4,1,1,1],
    [1,1,1,2,4,2], [1,2,1,1,4,2], [1,2,1,2,4,1], [1,1,4,2,1,2],
    [1,2,4,1,1,2], [1,2,4,2,1,1], [4,1,1,2,1,2], [4,2,1,1,1,2],
    [4,2,1,2,1,1], [2,1,2,1,4,1], [2,1,4,1,2,1], [4,1,2,1,2,1],
    [1,1,1,1,4,3], [1,1,1,3,4,1], [1,3,1,1,4,1], [1,1,4,1,1,3],
    [1,1,4,3,1,1], [4,1,1,1,1,3], [4,1,1,3,1,1], [1,1,3,1,4,1],
    [1,1,4,1,3,1], [3,1,1,1,4,1], [4,1,1,1,3,1],                 // 100-102
    [2,1,1,4,1,2], [2,1,1,2,1,4], [2,1,1,2,3,2],                 // 103=START A, 104=START B, 105=START C
];

/// Code 128 STOP symbol (7 elements, 13 units total).
static CODE128_STOP: [u8; 7] = [2,3,3,1,1,1,2];

fn encode_code128(data: &str) -> Result<Vec<u8>, String> {
    // Validate: only subset B (printable ASCII 32–126).
    for c in data.chars() {
        if !(32u8..=126u8).contains(&(c as u8)) {
            return Err(format!(
                "Code 128: character '{}' (U+{:04X}) is not in subset B (ASCII 32\u{2013}126)",
                c, c as u32
            ));
        }
    }

    // Build symbol list: START B + data symbols + checksum.
    let start_b_value: u32 = 104;

    let mut bars: Vec<u8> = Vec::new();
    // START B (symbol 104)
    bars.extend_from_slice(&CODE128[104]);

    // Checksum: start_value + Σ(i+1 × symbol_value) mod 103
    let check_sum: u32 = {
        let data_sum: u32 = data.chars().enumerate()
            .map(|(i, c)| (i + 1) as u32 * (c as u32 - 32))
            .sum();
        (start_b_value + data_sum) % 103
    };

    for c in data.chars() {
        let sym = c as u8 - 32;
        bars.extend_from_slice(&CODE128[sym as usize]);
    }

    // Checksum symbol
    bars.extend_from_slice(&CODE128[check_sum as usize]);

    // STOP symbol
    bars.extend_from_slice(&CODE128_STOP);

    Ok(bars)
}

// ── EAN-13 encoder ────────────────────────────────────────────────────────────
//
// Produces 95 modules as a flat alternating bar/space run-length array
// starting with a bar (left guard starts with a bar module).

/// EAN-13 parity pattern for the first digit (0–9).
/// Bit i (from MSB=bit5) = 1 → G-code, 0 → L-code for left-group digit i.
/// Left-group digits are positions 1–6 (second through seventh digit).
static EAN13_PARITY: [u8; 10] = [
    0b000000, // 0 → LLLLLL
    0b001011, // 1 → LLGLGG
    0b001101, // 2 → LLGGLG
    0b001110, // 3 → LLGGGL
    0b010011, // 4 → LGLLGG
    0b011001, // 5 → LGGLLG
    0b011100, // 6 → LGGGLL
    0b010101, // 7 → LGLGLG
    0b010110, // 8 → LGLGGL
    0b011010, // 9 → LGGLGL
];

/// L-code bit patterns for digits 0–9 (7 bits, MSB first, 0=space 1=bar).
static EAN13_L_PAT: [u8; 10] = [
    0b0001101, // 0: s3 b2 s1 b1
    0b0011001, // 1: s2 b2 s2 b1
    0b0010011, // 2: s2 b1 s2 b2
    0b0111101, // 3: s1 b4 s1 b1
    0b0100011, // 4: s1 b1 s3 b2
    0b0110001, // 5: s1 b2 s3 b1
    0b0101111, // 6: s1 b1 s1 b4
    0b0111011, // 7: s1 b3 s1 b2
    0b0110111, // 8: s1 b2 s1 b3
    0b0001011, // 9: s3 b1 s1 b2
];

fn ean_modules(pat: u8) -> [bool; 7] {
    let mut out = [false; 7];
    for i in 0..7usize {
        out[i] = (pat >> (6 - i)) & 1 == 1;
    }
    out
}

fn ean13_check(d: &[u8]) -> u8 {
    let sum: u32 = d[..12].iter().enumerate()
        .map(|(i, &v)| if i % 2 == 0 { v as u32 } else { v as u32 * 3 })
        .sum();
    ((10 - (sum % 10)) % 10) as u8
}

fn encode_ean13(data: &str) -> Result<Vec<u8>, String> {
    let digits_raw: Vec<u8> = data.trim().chars().map(|c| {
        if c.is_ascii_digit() { c as u8 - b'0' } else { 255 }
    }).collect();

    if digits_raw.len() != 12 && digits_raw.len() != 13 {
        return Err(format!(
            "EAN-13: data must be 12 or 13 digits, got {}",
            digits_raw.len()
        ));
    }
    for (i, &d) in digits_raw.iter().enumerate() {
        if d > 9 {
            return Err(format!("EAN-13: non-digit at position {i}"));
        }
    }

    let mut digits = digits_raw.clone();
    let check = ean13_check(&digits);
    if digits.len() == 13 {
        if digits[12] != check {
            return Err(format!(
                "EAN-13: invalid check digit (got {}, expected {check})",
                digits[12]
            ));
        }
    } else {
        digits.push(check);
    }

    // Build 95-module sequence.
    let mut modules: Vec<bool> = Vec::with_capacity(95);

    // Left guard: bar space bar
    modules.extend_from_slice(&[true, false, true]);

    // Left 6 digits (digits[1] through digits[6])
    let parity = EAN13_PARITY[digits[0] as usize];
    for i in 0..6usize {
        let d   = digits[i + 1] as usize;
        let use_g = (parity >> (5 - i)) & 1 == 1;
        let l_mods = ean_modules(EAN13_L_PAT[d]);
        if use_g {
            // G-code = bit-reversal of L-code
            let mut rev = l_mods;
            rev.reverse();
            modules.extend_from_slice(&rev);
        } else {
            modules.extend_from_slice(&l_mods);
        }
    }

    // Center guard: space bar space bar space
    modules.extend_from_slice(&[false, true, false, true, false]);

    // Right 6 digits (digits[7] through digits[12]), R-code = complement of L-code
    for i in 0..6usize {
        let d      = digits[i + 7] as usize;
        let l_mods = ean_modules(EAN13_L_PAT[d]);
        let r_mods: [bool; 7] = std::array::from_fn(|j| !l_mods[j]);
        modules.extend_from_slice(&r_mods);
    }

    // Right guard: bar space bar
    modules.extend_from_slice(&[true, false, true]);

    // Sequence starts with true (bar from left guard) — safe to RLE directly.
    Ok(rle_modules(&modules))
}

/// Run-length encode a module sequence into alternating bar/space widths.
/// The input must start with a bar (true); even-indexed elements are bars.
fn rle_modules(modules: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    if modules.is_empty() { return out; }
    let mut cur = modules[0];
    let mut run = 0u8;
    for &m in modules {
        if m == cur {
            run += 1;
        } else {
            out.push(run);
            cur = m;
            run = 1;
        }
    }
    out.push(run);
    out
}

// ── Divider ───────────────────────────────────────────────────────────────────

fn layout_divider(
    node: &Node,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) -> (RenderNode, f32) {
    let color = node.color.clone().unwrap_or_else(|| "#000000".into());
    let t = node.thickness;

    match node.direction {
        Direction::Horizontal => (
            RenderNode::Line(RenderLine {
                x1: x,
                y1: y + t / 2.0,
                x2: x + avail_w,
                y2: y + t / 2.0,
                color,
                thickness: t,
                dash: None,
            }),
            t,
        ),
        Direction::Vertical => {
            let h = avail_h.unwrap_or(t);
            (
                RenderNode::Line(RenderLine {
                    x1: x + t / 2.0,
                    y1: y,
                    x2: x + t / 2.0,
                    y2: y + h,
                    color,
                    thickness: t,
                    dash: None,
                }),
                h,
            )
        }
    }
}

// ── Text ──────────────────────────────────────────────────────────────────────

// ── Built-in font metrics (Adobe AFM data) ────────────────────────────────────
//
// Glyph advance widths in units of 1/1000 of the point size, indexed by
// (char_code − 32) for ASCII printable characters 32–126 (95 entries).
// Source: Adobe AFM files for the 14 standard PDF built-in fonts.

pub(crate) fn builtin_char_advance(font: &str, c: char) -> Option<u16> {
    let idx = (c as u32).checked_sub(32)? as usize;
    if idx >= 95 { return None; }

    #[rustfmt::skip]
    static HELVETICA: [u16; 95] = [
    //  sp    !    "    #    $    %    &    '    (    )    *    +    ,    -    .    /
       278, 278, 355, 556, 556, 889, 667, 222, 333, 333, 389, 584, 278, 333, 278, 278,
    //   0    1    2    3    4    5    6    7    8    9    :    ;    <    =    >    ?
       556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    //   @    A    B    C    D    E    F    G    H    I    J    K    L    M    N    O
      1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    //   P    Q    R    S    T    U    V    W    X    Y    Z    [    \    ]    ^    _
       667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    //   `    a    b    c    d    e    f    g    h    i    j    k    l    m    n    o
       222, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    //   p    q    r    s    t    u    v    w    x    y    z    {    |    }    ~
       556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
    ];

    #[rustfmt::skip]
    static HELVETICA_BOLD: [u16; 95] = [
    //  sp    !    "    #    $    %    &    '    (    )    *    +    ,    -    .    /
       278, 333, 474, 556, 556, 889, 722, 278, 333, 333, 389, 584, 278, 333, 278, 278,
    //   0    1    2    3    4    5    6    7    8    9    :    ;    <    =    >    ?
       556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    //   @    A    B    C    D    E    F    G    H    I    J    K    L    M    N    O
       975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    //   P    Q    R    S    T    U    V    W    X    Y    Z    [    \    ]    ^    _
       667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    //   `    a    b    c    d    e    f    g    h    i    j    k    l    m    n    o
       278, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    //   p    q    r    s    t    u    v    w    x    y    z    {    |    }    ~
       611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
    ];

    #[rustfmt::skip]
    static TIMES_ROMAN: [u16; 95] = [
    //  sp    !    "    #    $    %    &    '    (    )    *    +    ,    -    .    /
       250, 333, 408, 500, 500, 833, 778, 333, 333, 333, 500, 564, 250, 333, 250, 278,
    //   0    1    2    3    4    5    6    7    8    9    :    ;    <    =    >    ?
       500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444,
    //   @    A    B    C    D    E    F    G    H    I    J    K    L    M    N    O
       921, 722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889, 722, 722,
    //   P    Q    R    S    T    U    V    W    X    Y    Z    [    \    ]    ^    _
       556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611, 333, 278, 333, 469, 500,
    //   `    a    b    c    d    e    f    g    h    i    j    k    l    m    n    o
       333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500,
    //   p    q    r    s    t    u    v    w    x    y    z    {    |    }    ~
       500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541,
    ];

    #[rustfmt::skip]
    static TIMES_BOLD: [u16; 95] = [
    //  sp    !    "    #    $    %    &    '    (    )    *    +    ,    -    .    /
       250, 333, 555, 500, 500,1000, 833, 333, 333, 333, 500, 570, 250, 333, 250, 278,
    //   0    1    2    3    4    5    6    7    8    9    :    ;    <    =    >    ?
       500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500,
    //   @    A    B    C    D    E    F    G    H    I    J    K    L    M    N    O
       930, 722, 667, 722, 722, 667, 611, 778, 778, 389, 500, 778, 667, 944, 722, 778,
    //   P    Q    R    S    T    U    V    W    X    Y    Z    [    \    ]    ^    _
       611, 778, 722, 556, 667, 722, 722,1000, 722, 722, 667, 333, 278, 333, 581, 500,
    //   `    a    b    c    d    e    f    g    h    i    j    k    l    m    n    o
       333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556, 278, 833, 556, 500,
    //   p    q    r    s    t    u    v    w    x    y    z    {    |    }    ~
       556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520,
    ];

    #[rustfmt::skip]
    static TIMES_ITALIC: [u16; 95] = [
    //  sp    !    "    #    $    %    &    '    (    )    *    +    ,    -    .    /
       250, 333, 420, 500, 500, 833, 778, 333, 333, 333, 500, 675, 250, 333, 250, 278,
    //   0    1    2    3    4    5    6    7    8    9    :    ;    <    =    >    ?
       500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 675, 675, 675, 500,
    //   @    A    B    C    D    E    F    G    H    I    J    K    L    M    N    O
       920, 611, 611, 667, 722, 611, 611, 722, 722, 333, 444, 667, 556, 833, 667, 722,
    //   P    Q    R    S    T    U    V    W    X    Y    Z    [    \    ]    ^    _
       611, 722, 611, 500, 556, 722, 611, 833, 611, 556, 556, 389, 278, 389, 422, 500,
    //   `    a    b    c    d    e    f    g    h    i    j    k    l    m    n    o
       333, 500, 500, 444, 500, 444, 278, 500, 500, 278, 278, 444, 278, 722, 500, 500,
    //   p    q    r    s    t    u    v    w    x    y    z    {    |    }    ~
       500, 500, 389, 389, 278, 500, 444, 667, 444, 444, 389, 400, 275, 400, 541,
    ];

    #[rustfmt::skip]
    static TIMES_BOLD_ITALIC: [u16; 95] = [
    //  sp    !    "    #    $    %    &    '    (    )    *    +    ,    -    .    /
       250, 389, 555, 500, 500, 833, 778, 333, 333, 333, 500, 570, 250, 333, 250, 278,
    //   0    1    2    3    4    5    6    7    8    9    :    ;    <    =    >    ?
       500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500,
    //   @    A    B    C    D    E    F    G    H    I    J    K    L    M    N    O
       832, 667, 667, 667, 722, 667, 667, 722, 778, 389, 500, 667, 611, 889, 722, 722,
    //   P    Q    R    S    T    U    V    W    X    Y    Z    [    \    ]    ^    _
       611, 722, 667, 556, 611, 722, 667, 889, 667, 611, 611, 333, 278, 333, 570, 500,
    //   `    a    b    c    d    e    f    g    h    i    j    k    l    m    n    o
       333, 500, 500, 444, 500, 444, 333, 556, 556, 278, 278, 500, 278, 778, 556, 500,
    //   p    q    r    s    t    u    v    w    x    y    z    {    |    }    ~
       500, 500, 389, 389, 278, 556, 444, 667, 500, 444, 389, 348, 220, 348, 570,
    ];

    let table: &[u16; 95] = match font {
        "Helvetica" | "Helvetica-Oblique"         => &HELVETICA,
        "Helvetica-Bold" | "Helvetica-BoldOblique" => &HELVETICA_BOLD,
        "Times-Roman"                              => &TIMES_ROMAN,
        "Times-Bold"                               => &TIMES_BOLD,
        "Times-Italic"                             => &TIMES_ITALIC,
        "Times-BoldItalic"                         => &TIMES_BOLD_ITALIC,
        _ => return None,
    };
    Some(table[idx])
}

/// Measure the advance width (in pt) of a string at the given font and size.
///
/// Uses per-character AFM glyph metrics for the standard PDF built-in fonts.
/// Falls back to a constant-ratio approximation for Courier (monospace) and
/// any unknown / custom font.
pub(crate) fn text_width(font: &str, text: &str, size: f32) -> f32 {
    // Courier family: monospace, 600/1000 per glyph.
    if font.contains("Courier") {
        return text.chars().count() as f32 * size * 0.6;
    }
    // Check caller-supplied width table first (custom fonts via set_font_metrics).
    let custom = FONT_WIDTHS.with(|fw| fw.borrow().get(font).cloned());
    if let Some(w) = custom {
        return text.chars().map(|c| {
            let cp = c as u32;
            let advance = if cp >= 32 && cp <= 126 && !w.ascii.is_empty() {
                w.ascii[(cp - 32) as usize]
            } else {
                w.default
            };
            advance as f32 * size / 1000.0
        }).sum();
    }
    // For known builtins use per-character AFM widths.
    if builtin_char_advance(font, ' ').is_some() {
        return text.chars()
            .map(|c| builtin_char_advance(font, c).unwrap_or(500) as f32 * size / 1000.0)
            .sum();
    }
    // Unknown font (undefined alias, unsupported builtin, or custom font with no
    // width table). Use Helvetica metrics as the fallback — this matches what
    // the renderer does (it also falls back to Helvetica for unknown fonts), so
    // layout and rendering stay in sync.
    text_width("Helvetica", text, size)
}

/// Measure the natural (preferred/unconstrained) width of a node.
///
/// For text nodes this is the width of all runs placed on a single line.
/// For container nodes it is the widest child's natural width plus padding.
/// This is used by `layout_flank` to shrink-wrap the flanking child when no
/// explicit `width` is set.
fn measure_natural_w(node: &Node) -> f32 {
    match node.kind {
        NodeKind::Text => {
            let size  = node.font_size;
            let space = text_width(&node.font, " ", size);
            let mut total = 0.0_f32;
            let mut first = true;
            for run in &node.text_runs {
                let font = run.font.as_deref().unwrap_or(&node.font);
                for (j, word) in run.text.split_whitespace().enumerate() {
                    if !first || j > 0 { total += space; }
                    total += text_width(font, word, size);
                    first = false;
                }
            }
            total
        }
        NodeKind::Divider => node.thickness,
        NodeKind::Img => node.width_constraint.unwrap_or(100.0),
        NodeKind::Barcode => node.width_constraint.unwrap_or(80.0),
        _ => {
            let [_pt, pr, _pb, pl] = node.padding;
            let inner = match node.kind {
                NodeKind::Stack => {
                    // vertical: widest child
                    node.children.iter().map(measure_natural_w).fold(0.0_f32, f32::max)
                }
                NodeKind::Flank => {
                    // flank is horizontal: fill child takes natural width like the flanks for
                    // measurement purposes (we can't shrink it to zero here).
                    let children_w: f32 = node.children.iter().map(measure_natural_w).sum();
                    let gap_total = node.gap * (node.children.len().saturating_sub(1)) as f32;
                    children_w + gap_total
                }
                NodeKind::Split => {
                    // horizontal: sum of children + gaps
                    let children_w: f32 = node.children.iter().map(measure_natural_w).sum();
                    let gap_total = node.gap * (node.children.len().saturating_sub(1)) as f32;
                    children_w + gap_total
                }
                NodeKind::Cluster => {
                    // treat as horizontal row (no wrap assumed for natural width)
                    let children_w: f32 = node.children.iter().map(measure_natural_w).sum();
                    let gap_total = node.gap * (node.children.len().saturating_sub(1)) as f32;
                    children_w + gap_total
                }
                _ => {
                    // Frame and others: widest child
                    node.children.iter().map(measure_natural_w).fold(0.0_f32, f32::max)
                }
            };
            // honour an explicit width constraint if set
            match node.width_constraint {
                Some(w) => w,
                None    => inner + pl + pr,
            }
        }
    }
}

fn layout_text(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32) {
    struct Atom {
        text:      String,
        font:      String,
        color:     String,
        href:      Option<String>,
        underline: bool,
        strike:    bool,
    }
    enum Tok { Space, Word(Atom) }

    let font_size    = node.font_size;
    let line_height  = font_size * 1.2;
    let parent_font  = node.font.as_str();
    let parent_color = node.text_color.clone().unwrap_or_else(|| "#1a1a1a".into());
    let space_w      = text_width(parent_font, " ", font_size);

    // ── Build token stream ────────────────────────────────────────────────────
    let mut toks: Vec<Tok> = Vec::new();
    for (ri, run) in node.text_runs.iter().enumerate() {
        let is_span = run.font.is_some() || run.color.is_some()
                   || run.href.is_some() || run.underline || run.strike;
        let font  = run.font.as_deref().unwrap_or(parent_font);
        let color = run.color.clone().unwrap_or_else(|| parent_color.clone());

        if ri > 0 && run.leading_space { toks.push(Tok::Space); }

        if is_span {
            if !run.text.is_empty() {
                toks.push(Tok::Word(Atom {
                    text: run.text.clone(), font: font.to_string(), color,
                    href: run.href.clone(), underline: run.underline, strike: run.strike,
                }));
            }
        } else {
            for (j, word) in run.text.split_whitespace().enumerate() {
                if j > 0 { toks.push(Tok::Space); }
                toks.push(Tok::Word(Atom {
                    text: word.to_string(), font: font.to_string(), color: color.clone(),
                    href: None, underline: false, strike: false,
                }));
            }
        }
    }

    if toks.is_empty() {
        return (RenderNode::Box(RenderBox {
            x, y, width: 0.0, height: 0.0, fill: None,
            border_width: 0.0, border_color: None, radius: 0.0, debug_self: false, children: vec![],
        }), 0.0);
    }

    // ── Word-wrap: build lines of (Atom, gap_before_pt) ──────────────────────
    let mut lines: Vec<Vec<(Atom, f32)>> = Vec::new();
    let mut cur_line: Vec<(Atom, f32)>   = Vec::new();
    let mut cur_line_w    = 0.0_f32;
    let mut pending_space = false;

    for tok in toks {
        match tok {
            Tok::Space => { pending_space = true; }
            Tok::Word(atom) => {
                let aw  = text_width(&atom.font, &atom.text, font_size);
                let gap = if cur_line.is_empty() || !pending_space { 0.0 } else { space_w };
                if cur_line.is_empty() || cur_line_w + gap + aw <= avail_w + 0.01 {
                    cur_line_w += gap + aw;
                    cur_line.push((atom, gap));
                } else {
                    lines.push(std::mem::take(&mut cur_line));
                    cur_line.push((atom, 0.0));
                    cur_line_w = aw;
                }
                pending_space = false;
            }
        }
    }
    if !cur_line.is_empty() { lines.push(cur_line); }

    let total_h = lines.len() as f32 * line_height;
    let mut all_nodes: Vec<RenderNode> = Vec::new();

    let align_str = match node.text_align {
        TextAlign::Left   => "left",
        TextAlign::Center => "center",
        TextAlign::Right  => "right",
    };

    // ── Emit render nodes per line ────────────────────────────────────────────
    for (li, line) in lines.iter().enumerate() {
        let line_y = y + li as f32 * line_height;
        // Anchor x: the renderer uses real font metrics to convert this to the
        // actual draw origin, so we just pass the alignment edge here.
        // "left"  → left edge of the content area (x)
        // "center"→ horizontal centre of the content area (x + avail_w/2)
        // "right" → right edge of the content area (x + avail_w)
        // The approximate line_w is still needed for left-aligned group merging
        // (so we keep it) but no longer drives alignment position.
        let line_anchor_x = match node.text_align {
            TextAlign::Left   => x,
            TextAlign::Center => x + avail_w / 2.0,
            TextAlign::Right  => x + avail_w,
        };

        let mut cur_x = line_anchor_x;

        // Merge consecutive plain (no href/underline/strike) atoms that share the
        // same font and color into a single RenderText.  This lets pdf-lib render
        // each group with its real glyph metrics instead of accumulating the error
        // from the char-width approximation word-by-word.
        let mut grp_x    = 0.0_f32;
        let mut grp_font = String::new();
        let mut grp_clr  = String::new();
        let mut grp_text = String::new();

        for (atom, gap) in line {
            cur_x += gap;
            let aw = text_width(&atom.font, &atom.text, font_size);

            let plain = atom.href.is_none() && !atom.underline && !atom.strike;

            if plain {
                // If font or color changed, flush the current group first.
                if !grp_text.is_empty() && (atom.font != grp_font || atom.color != grp_clr) {
                    all_nodes.push(RenderNode::Text(RenderText {
                        x: grp_x, y: line_y,
                        content: std::mem::take(&mut grp_text),
                        font: grp_font.clone(), size: font_size, color: grp_clr.clone(),
                        text_align: align_str.to_string(),
                    }));
                }
                if grp_text.is_empty() {
                    grp_x    = cur_x;
                    grp_font = atom.font.clone();
                    grp_clr  = atom.color.clone();
                    grp_text = atom.text.clone();
                } else {
                    grp_text.push(' ');
                    grp_text.push_str(&atom.text);
                }
            } else {
                // Flush any pending plain group before emitting a decorated atom.
                if !grp_text.is_empty() {
                    all_nodes.push(RenderNode::Text(RenderText {
                        x: grp_x, y: line_y,
                        content: std::mem::take(&mut grp_text),
                        font: grp_font.clone(), size: font_size, color: grp_clr.clone(),
                        text_align: align_str.to_string(),
                    }));
                }

                let mut seg = vec![RenderNode::Text(RenderText {
                    x: cur_x, y: line_y,
                    content: atom.text.clone(), font: atom.font.clone(),
                    size: font_size, color: atom.color.clone(),
                    // Decorated atoms (spans) always use their computed cur_x as a
                    // left-edge anchor; alignment has already been applied at the line level.
                    text_align: "left".to_string(),
                })];

                if atom.underline {
                    seg.push(RenderNode::Line(RenderLine {
                        x1: cur_x, y1: line_y + font_size,
                        x2: cur_x + aw, y2: line_y + font_size,
                        color: atom.color.clone(), thickness: 0.75,
                        dash: None,
                    }));
                }
                if atom.strike {
                    let sy = line_y + font_size * 0.5;
                    seg.push(RenderNode::Line(RenderLine {
                        x1: cur_x, y1: sy, x2: cur_x + aw, y2: sy,
                        color: atom.color.clone(), thickness: 0.75,
                        dash: None,
                    }));
                }

                match &atom.href {
                    Some(href) => all_nodes.push(RenderNode::Link(RenderLink {
                        url: href.clone(), x: cur_x, y: line_y,
                        width: aw, height: line_height, debug_self: false, children: seg,
                    })),
                    None => all_nodes.extend(seg),
                }
            }

            cur_x += aw;
        }
        // Flush any group remaining at end of line.
        if !grp_text.is_empty() {
            all_nodes.push(RenderNode::Text(RenderText {
                x: grp_x, y: line_y,
                content: grp_text,
                font: grp_font, size: font_size, color: grp_clr,
                text_align: align_str.to_string(),
            }));
        }
    }

    (
        RenderNode::Box(RenderBox {
            x, y, width: avail_w, height: total_h,
            fill: None, border_width: 0.0, border_color: None, radius: 0.0,
            debug_self: node.debug,
            children: all_nodes,
        }),
        total_h,
    )
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve child width given parent's available width and alignment.
fn resolve_child_w(child: &Node, avail_w: f32, align: &Align) -> f32 {
    if let Some(w) = child.width_constraint {
        return w.min(avail_w);
    }
    match align {
        Align::Stretch => avail_w,
        _ => natural_w(child).min(avail_w),
    }
}

/// Intrinsic (content-driven, unwrapped) width of a node.
/// Used by `resolve_child_w` for non-Stretch cross-axis alignment so that
/// children shrink to their content instead of filling the container.
/// Returns `f32::MAX` for node types that always fill available width;
/// the caller clamps with `.min(avail_w)`.
fn natural_w(node: &Node) -> f32 {
    if let Some(w) = node.width_constraint {
        return w;
    }
    match node.kind {
        NodeKind::Text => text_natural_w(node),
        _ => f32::MAX, // layout containers fill available width
    }
}

/// Width of `node`'s text content laid out as a single unwrapped line.
/// Mirrors the token-building logic in `layout_text` but accumulates widths
/// instead of word-wrapping.
fn text_natural_w(node: &Node) -> f32 {
    let font_size   = node.font_size;
    let parent_font = node.font.as_str();
    let space_w     = text_width(parent_font, " ", font_size);
    let mut total          = 0.0_f32;
    let mut pending_space  = false;

    for (ri, run) in node.text_runs.iter().enumerate() {
        let is_span = run.font.is_some() || run.color.is_some()
                   || run.href.is_some() || run.underline || run.strike;
        let font = run.font.as_deref().unwrap_or(parent_font);

        if ri > 0 && run.leading_space { pending_space = true; }

        if is_span {
            if !run.text.is_empty() {
                if pending_space { total += space_w; pending_space = false; }
                total += text_width(font, &run.text, font_size);
            }
        } else {
            for (j, word) in run.text.split_whitespace().enumerate() {
                if j > 0 { pending_space = true; }
                if pending_space { total += space_w; pending_space = false; }
                total += text_width(font, word, font_size);
            }
        }
    }
    total
}

/// X position of child given parent x, available width, child width, and alignment.
fn cross_x(parent_x: f32, avail_w: f32, child_w: f32, align: &Align) -> f32 {
    match align {
        Align::Start | Align::Stretch => parent_x,
        Align::Center => parent_x + (avail_w - child_w) / 2.0,
        Align::End => parent_x + avail_w - child_w,
    }
}

/// Shift a node's y coordinate by the cross-axis alignment offset.
fn shift_y_cross(node: RenderNode, node_h: f32, row_h: f32, align: &Align) -> RenderNode {
    let dy = match align {
        Align::Start | Align::Stretch => 0.0,
        Align::Center => (row_h - node_h) / 2.0,
        Align::End => row_h - node_h,
    };
    if dy == 0.0 {
        return node;
    }
    shift_y(node, dy)
}

fn shift_y(node: RenderNode, dy: f32) -> RenderNode {
    match node {
        RenderNode::Box(mut b) => {
            b.y += dy;
            b.children = b.children.into_iter().map(|c| shift_y(c, dy)).collect();
            RenderNode::Box(b)
        }
        RenderNode::Line(mut l) => {
            l.y1 += dy;
            l.y2 += dy;
            RenderNode::Line(l)
        }
        RenderNode::Text(mut t) => {
            t.y += dy;
            RenderNode::Text(t)
        }
        RenderNode::Link(mut l) => {
            l.y += dy;
            l.children = l.children.into_iter().map(|c| shift_y(c, dy)).collect();
            RenderNode::Link(l)
        }
        RenderNode::Image(mut i) => {
            i.y += dy;
            RenderNode::Image(i)
        }
        RenderNode::Barcode(mut bc) => {
            bc.y += dy;
            RenderNode::Barcode(bc)
        }
    }
}

fn shift_x(node: RenderNode, dx: f32) -> RenderNode {
    if dx == 0.0 {
        return node;
    }
    match node {
        RenderNode::Box(mut b) => {
            b.x += dx;
            b.children = b.children.into_iter().map(|c| shift_x(c, dx)).collect();
            RenderNode::Box(b)
        }
        RenderNode::Line(mut l) => {
            l.x1 += dx;
            l.x2 += dx;
            RenderNode::Line(l)
        }
        RenderNode::Text(mut t) => {
            t.x += dx;
            RenderNode::Text(t)
        }
        RenderNode::Link(mut l) => {
            l.x += dx;
            l.children = l.children.into_iter().map(|c| shift_x(c, dx)).collect();
            RenderNode::Link(l)
        }
        RenderNode::Image(mut i) => {
            i.x += dx;
            RenderNode::Image(i)
        }
        RenderNode::Barcode(mut bc) => {
            bc.x += dx;
            RenderNode::Barcode(bc)
        }
    }
}

// ── Debug overlay ─────────────────────────────────────────────────────────────

/// Append debug overlay lines to any debug-flagged box or link in `nodes`,
/// recursing into children first so inner nodes are processed before outer.
fn apply_debug_overlay(nodes: &mut Vec<RenderNode>) {
    for node in nodes.iter_mut() {
        match node {
            RenderNode::Box(b) => {
                apply_debug_overlay(&mut b.children);
                if b.debug_self {
                    let self_lines = debug_rect_lines(b.x, b.y, b.width, b.height, "#ff0033");
                    let child_lines: Vec<RenderNode> = b.children.iter()
                        .filter_map(|c| match c {
                            RenderNode::Box(cb) if !cb.debug_self =>
                                Some(debug_rect_lines(cb.x, cb.y, cb.width, cb.height, "#0066ff")),
                            RenderNode::Barcode(cb) if !cb.debug_self =>
                                Some(debug_rect_lines(cb.x, cb.y, cb.width, cb.height, "#0066ff")),
                            _ => None,
                        })
                        .flatten()
                        .collect();
                    b.children.extend(self_lines);
                    b.children.extend(child_lines);
                }
            }
            RenderNode::Link(l) => {
                apply_debug_overlay(&mut l.children);
                if l.debug_self {
                    let self_lines = debug_rect_lines(l.x, l.y, l.width, l.height, "#ff0033");
                    let child_lines: Vec<RenderNode> = l.children.iter()
                        .filter_map(|c| match c {
                            RenderNode::Box(cb) if !cb.debug_self =>
                                Some(debug_rect_lines(cb.x, cb.y, cb.width, cb.height, "#0066ff")),
                            RenderNode::Barcode(cb) if !cb.debug_self =>
                                Some(debug_rect_lines(cb.x, cb.y, cb.width, cb.height, "#0066ff")),
                            _ => None,
                        })
                        .flatten()
                        .collect();
                    l.children.extend(self_lines);
                    l.children.extend(child_lines);
                }
            }
            _ => {}
        }
    }
    // Barcodes are leaf nodes (no children vec), so inject their debug lines
    // directly into this vec after the main pass.
    let barcode_lines: Vec<RenderNode> = nodes.iter()
        .filter_map(|n| {
            if let RenderNode::Barcode(bc) = n {
                if bc.debug_self {
                    return Some(debug_rect_lines(bc.x, bc.y, bc.width, bc.height, "#ff0033"));
                }
            }
            None
        })
        .flatten()
        .collect();
    nodes.extend(barcode_lines);
}

/// Generate 4 dashed `RenderLine`s tracing the rect (x, y, w, h) in `color`.
/// Stroke is centred on each edge (0.25pt inside, 0.25pt outside).
fn debug_rect_lines(x: f32, y: f32, w: f32, h: f32, color: &str) -> Vec<RenderNode> {
    let dash = Some(vec![1.5_f32, 1.5_f32]);
    vec![
        RenderNode::Line(RenderLine { x1: x, y1: y, x2: x + w, y2: y,
            color: color.to_string(), thickness: 0.5, dash: dash.clone() }),
        RenderNode::Line(RenderLine { x1: x + w, y1: y, x2: x + w, y2: y + h,
            color: color.to_string(), thickness: 0.5, dash: dash.clone() }),
        RenderNode::Line(RenderLine { x1: x, y1: y + h, x2: x + w, y2: y + h,
            color: color.to_string(), thickness: 0.5, dash: dash.clone() }),
        RenderNode::Line(RenderLine { x1: x, y1: y, x2: x, y2: y + h,
            color: color.to_string(), thickness: 0.5, dash }),
    ]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn engine_render(xml: &str) -> serde_json::Value {
        let doc = parse(xml).unwrap();
        let pages: Vec<_> = doc.pages.iter().flat_map(layout_page).collect();
        let json_str =
            crate::render::pages_to_json(&pages, serde_json::Value::Null, serde_json::Value::Null);
        serde_json::from_str(&json_str).unwrap()
    }

    fn minimal(body: &str) -> String {
        format!(
            r#"<lpdf version="1"><document size="a4" margin="28pt"><pages><page>{body}</page></pages></document></lpdf>"#
        )
    }

    #[test]
    fn page_dimensions_correct() {
        let tree = engine_render(&minimal(""));
        assert_eq!(tree["pages"][0]["width"], 595.28);
        assert_eq!(tree["pages"][0]["height"], 841.89);
    }

    #[test]
    fn frame_with_fixed_height() {
        let tree = engine_render(&minimal(r##"<frame height="50pt" background="#ff0000" />"##));
        let node = &tree["pages"][0]["nodes"][0];
        assert_eq!(node["type"], "box");
        assert_eq!(node["height"], 50.0);
        assert_eq!(node["fill"], "#ff0000");
    }

    #[test]
    fn stack_positions_children_sequentially() {
        // Two frames with fixed heights and a gap
        let tree = engine_render(&minimal(
            r#"<stack gap="m"><frame height="20pt" /><frame height="30pt" /></stack>"#,
        ));
        let stack = &tree["pages"][0]["nodes"][0];
        let children = stack["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);

        let first_y = children[0]["y"].as_f64().unwrap();
        let second_y = children[1]["y"].as_f64().unwrap();
        // Second child y = first y + first height + gap(8)
        assert!((second_y - first_y - 20.0 - 8.0).abs() < 0.1, "second_y={second_y}, first_y={first_y}");
    }

    #[test]
    fn grid_three_columns() {
        let body = r#"<grid cols="3" gap="m">
            <frame height="40pt" />
            <frame height="40pt" />
            <frame height="40pt" />
        </grid>"#;
        let tree = engine_render(&minimal(body));
        let grid = &tree["pages"][0]["nodes"][0];
        let children = grid["children"].as_array().unwrap();
        assert_eq!(children.len(), 3);

        // All three should be on the same row (same y)
        let y0 = children[0]["y"].as_f64().unwrap();
        let y1 = children[1]["y"].as_f64().unwrap();
        let y2 = children[2]["y"].as_f64().unwrap();
        assert!((y0 - y1).abs() < 0.1);
        assert!((y0 - y2).abs() < 0.1);

        // Columns should be offset by column_width + gap
        let x0 = children[0]["x"].as_f64().unwrap();
        let x1 = children[1]["x"].as_f64().unwrap();
        let x2 = children[2]["x"].as_f64().unwrap();
        let avail_w = 595.28 - 28.0 * 2.0;
        let col_w = (avail_w - 8.0 * 2.0) / 3.0;
        assert!((x1 - x0 - col_w - 8.0).abs() < 0.5, "x1={x1} x0={x0} col_w={col_w}");
        assert!((x2 - x1 - col_w - 8.0).abs() < 0.5);
    }

    #[test]
    fn divider_produces_line_node() {
        let tree = engine_render(&minimal(r##"<divider color="#e0e0e0" thickness="s" />"##));
        let node = &tree["pages"][0]["nodes"][0];
        assert_eq!(node["type"], "line");
        assert_eq!(node["color"], "#e0e0e0");
        assert_eq!(node["thickness"], 1.0);
    }

    #[test]
    fn text_produces_box_with_text_children() {
        let tree = engine_render(&minimal(r#"<text size="m">Hello world</text>"#));
        let node = &tree["pages"][0]["nodes"][0];
        assert_eq!(node["type"], "box");
        let kids = node["children"].as_array().unwrap();
        assert!(!kids.is_empty());
        assert_eq!(kids[0]["type"], "text");
        // Plain same-font words on the same line are merged into one RenderText
        // so that pdf-lib uses real glyph metrics (avoids accumulated width error).
        let words: Vec<&str> = kids.iter()
            .filter_map(|k| k["content"].as_str())
            .collect();
        assert_eq!(words, vec!["Hello world"]);
    }

    #[test]
    fn split_equal_gives_half_widths() {
        let body = r#"<split equal="true" gap="m">
            <frame height="20pt" />
            <frame height="20pt" />
        </split>"#;
        let tree = engine_render(&minimal(body));
        let split = &tree["pages"][0]["nodes"][0];
        let children = split["children"].as_array().unwrap();
        let w0 = children[0]["width"].as_f64().unwrap();
        let w1 = children[1]["width"].as_f64().unwrap();
        let avail_w = 595.28 - 28.0 * 2.0;
        let expected = (avail_w - 8.0) / 2.0;
        assert!((w0 - expected).abs() < 0.5, "w0={w0} expected={expected}");
        assert!((w1 - expected).abs() < 0.5);
    }

    #[test]
    fn stack_fill_child_gets_remaining_height() {
        let body = r#"<stack gap="m" height="full">
            <frame height="50pt" />
            <frame height="fill" />
        </stack>"#;
        let tree = engine_render(&minimal(body));
        let stack = &tree["pages"][0]["nodes"][0];
        let children = stack["children"].as_array().unwrap();
        let fill_h = children[1]["height"].as_f64().unwrap();
        // avail = 841.89 - 28*2 = 785.89; fixed=50; gap=8; fill = 785.89 - 50 - 8
        let expected = 841.89 - 56.0 - 50.0 - 8.0;
        assert!((fill_h - expected).abs() < 1.0, "fill_h={fill_h} expected={expected}");
    }

    #[test]
    fn overflow_content_splits_into_multiple_pages() {
        // 3 direct page children at 400pt each — overflow even without a wrapper.
        let body = r#"<frame height="400pt" /><frame height="400pt" /><frame height="400pt" />"#;
        let tree = engine_render(&minimal(body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2, "expected multiple pages, got {}", pages.len());
        assert!(!pages[0]["nodes"].as_array().unwrap().is_empty());
        assert!(!pages[pages.len() - 1]["nodes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn wrapped_stack_overflow_splits_into_pages() {
        // Common document pattern: a single <stack> wrapping many paragraphs.
        // Each frame child is 120pt; avail_h ≈ 785.89pt — 7 frames fit, 8 would not.
        let item = r#"<frame height="120pt" />"#;
        let body = format!("<stack>{}</stack>", item.repeat(10));
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        // 10 × 120pt = 1200pt > 785.89pt — must produce multiple pages.
        assert!(pages.len() >= 2, "expected multiple pages, got {}", pages.len());
    }

    #[test]
    fn single_full_height_child_no_overflow_split() {
        // One child with height="full" must never trigger overflow splitting.
        let body = r##"<frame height="full" background="#ff0000" />"##;
        let tree = engine_render(&minimal(body));
        let pages = tree["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 1, "expected exactly 1 page");
        assert_eq!(pages[0]["nodes"][0]["height"], 841.89 - 28.0 * 2.0);
    }

    #[test]
    fn grid_overflow_splits_by_rows_not_whole_grid() {
        // 3-column grid with 12 items each 200pt tall — 4 rows × 200pt = 800pt.
        // avail_h ≈ 785.89pt so the 4th row must spill to page 2, not the whole grid.
        let item = r#"<frame height="200pt" />"#;
        let body = format!(r#"<grid cols="3">{}</grid>"#, item.repeat(12));
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2, "expected grid to split across pages, got {}", pages.len());
        // Page 1 must have nodes (the first rows of the grid).
        assert!(!pages[0]["nodes"].as_array().unwrap().is_empty());
        // Page 2 must also have nodes (the overflowing rows).
        assert!(!pages[1]["nodes"].as_array().unwrap().is_empty());
    }

    // ── New behaviors (atomic_ctx, repeat, page numbering) ────────────────────

    #[test]
    fn frame_is_atomic_when_oversized() {
        // A frame taller than one page must not be split. It's force-placed on
        // its own page (may overflow visually but doesn't corrupt pagination).
        let body = r##"<frame height="900pt" background="#eeeeee" />"##;
        let tree = engine_render(&minimal(body));
        let pages = tree["pages"].as_array().unwrap();
        // Single oversized atomic → single page, not multiple.
        assert_eq!(pages.len(), 1);
    }

    #[test]
    fn frame_moves_to_next_page_instead_of_splitting() {
        // First frame fills most of the page; second frame doesn't fit in the
        // remaining space and is moved wholesale — not cut in half.
        let body = r##"<frame height="500pt" background="#ddd" /><frame height="400pt" background="#bbb" />"##;
        let tree = engine_render(&minimal(body));
        let pages = tree["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2);
        // Each page has exactly one frame; neither is cut.
        let p0_nodes = pages[0]["nodes"].as_array().unwrap();
        let p1_nodes = pages[1]["nodes"].as_array().unwrap();
        assert_eq!(p0_nodes.len(), 1);
        assert_eq!(p1_nodes.len(), 1);
        assert_eq!(p0_nodes[0]["height"], 500.0);
        assert_eq!(p1_nodes[0]["height"], 400.0);
    }

    #[test]
    fn long_text_splits_across_pages() {
        // Generate a text block tall enough to need at least 2 pages.
        let word = "lorem ipsum dolor sit amet ";
        let paragraph = word.repeat(400); // plenty of wrapped lines
        let body = format!(r#"<text size="m">{paragraph}</text>"#);
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2, "expected text to split, got {} pages", pages.len());
        // Each page has the text at the top.
        for p in pages {
            assert!(!p["nodes"].as_array().unwrap().is_empty());
        }
    }

    #[test]
    fn cluster_splits_between_wrapped_rows() {
        // Many fixed-height frames in a cluster produce many wrapped rows; the
        // cluster breaks between rows rather than treating itself as atomic.
        let item = r##"<frame width="180pt" height="100pt" background="#ddd" />"##;
        let body = format!(r#"<cluster gap="m">{}</cluster>"#, item.repeat(40));
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2, "cluster should split, got {} pages", pages.len());
    }

    #[test]
    fn repeat_page_renders_on_every_page() {
        // A footer marked repeat="page" must appear on every generated page.
        let filler = r#"<frame height="120pt" />"#;
        let body = format!(
            r#"{}<text repeat="page" size="s">FOOTER</text>"#,
            filler.repeat(10)
        );
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2);
        // Every page must contain a text node whose content starts with "FOOTER".
        for (i, p) in pages.iter().enumerate() {
            let found = find_text_content(p).iter().any(|t| t.contains("FOOTER"));
            assert!(found, "page {} missing footer", i + 1);
        }
    }

    #[test]
    fn repeat_first_renders_only_on_first_page() {
        let filler = r#"<frame height="120pt" />"#;
        let body = format!(
            r#"<text repeat="first" size="s">COVER HEADER</text>{}"#,
            filler.repeat(10)
        );
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2);
        let p1_has = find_text_content(&pages[0]).iter().any(|t| t.contains("COVER HEADER"));
        let p2_has = find_text_content(&pages[1]).iter().any(|t| t.contains("COVER HEADER"));
        assert!(p1_has, "first page must have cover header");
        assert!(!p2_has, "second page must not have cover header");
    }

    #[test]
    fn page_number_tokens_substituted_per_page() {
        let filler = r#"<frame height="120pt" />"#;
        let body = format!(
            r#"{}<text repeat="page" size="s">Page {{page}} of {{pages}}</text>"#,
            filler.repeat(10)
        );
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        let total = pages.len();
        assert!(total >= 2);
        for (i, p) in pages.iter().enumerate() {
            let expected = format!("Page {} of {}", i + 1, total);
            let texts = find_text_content(p);
            let found = texts.iter().any(|t| t.contains(&expected));
            assert!(found, "page {} should render '{}' but saw {:?}", i + 1, expected, texts);
        }
    }

    /// Helper: collect every "content" string of text nodes in a page tree.
    fn find_text_content(page: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
            if v["type"] == "text" {
                if let Some(s) = v["content"].as_str() { out.push(s.to_string()); }
            }
            if let Some(arr) = v["children"].as_array() {
                for c in arr { walk(c, out); }
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, out); }
            }
        }
        walk(page, &mut out);
        out
    }

    // ── Debug overlay tests ───────────────────────────────────────────────────

    /// A stack with debug="true" containing two frames produces:
    /// - the original 2 frame children
    /// - 4 red self-outline lines appended after content
    /// - 8 blue child-outline lines (4 per frame)
    #[test]
    fn debug_overlay_golden() {
        let body = r##"<stack debug="true" gap="0pt">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" />
        </stack>"##;
        let tree = engine_render(&minimal(body));
        let stack = &tree["pages"][0]["nodes"][0];
        assert_eq!(stack["type"], "box");
        let children = stack["children"].as_array().unwrap();
        // 2 frames + 4 red self lines + 8 blue child lines = 14
        assert_eq!(children.len(), 14, "expected 14 children, got {}", children.len());
        // The last 12 entries are lines
        for i in 2..14 {
            assert_eq!(children[i]["type"], "line", "child[{i}] should be a line");
            assert!(children[i]["dash"].is_array(), "debug lines must have dash array");
            assert_eq!(children[i]["thickness"], 0.5);
        }
        // First 4 lines are red
        for i in 2..6 {
            assert_eq!(children[i]["color"], "#ff0033", "child[{i}] should be red");
        }
        // Next 8 lines are blue (4 per frame child)
        for i in 6..14 {
            assert_eq!(children[i]["color"], "#0066ff", "child[{i}] should be blue");
        }
    }

    /// Layout geometry is identical with and without debug="true".
    #[test]
    fn debug_overlay_does_not_affect_layout() {
        let body_no_debug = r##"<stack gap="m">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" />
        </stack>"##;
        let body_debug = r##"<stack debug="true" gap="m">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" />
        </stack>"##;

        let t_clean = engine_render(&minimal(body_no_debug));
        let t_debug = engine_render(&minimal(body_debug));

        let stack_clean = &t_clean["pages"][0]["nodes"][0];
        let stack_debug = &t_debug["pages"][0]["nodes"][0];

        // Outer box geometry identical
        assert_eq!(stack_clean["x"], stack_debug["x"]);
        assert_eq!(stack_clean["y"], stack_debug["y"]);
        assert_eq!(stack_clean["width"], stack_debug["width"]);
        assert_eq!(stack_clean["height"], stack_debug["height"]);

        // Frame children geometry identical
        for i in 0..2 {
            let c_clean = &stack_clean["children"][i];
            let c_debug = &stack_debug["children"][i];
            assert_eq!(c_clean["x"], c_debug["x"], "child[{i}] x differs");
            assert_eq!(c_clean["y"], c_debug["y"], "child[{i}] y differs");
            assert_eq!(c_clean["height"], c_debug["height"], "child[{i}] height differs");
        }
    }

    /// When both parent and one child have debug="true":
    /// - parent gets 4 red self-lines
    /// - flagged child gets 4 red self-lines (not blue from parent)
    /// - unflagged child gets 4 blue lines from parent
    #[test]
    fn debug_overlay_composition() {
        let body = r##"<stack debug="true" gap="0pt">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" debug="true" />
        </stack>"##;
        let tree = engine_render(&minimal(body));
        let stack = &tree["pages"][0]["nodes"][0];
        let children = stack["children"].as_array().unwrap();

        // stack: 2 frames + 4 red self + 4 blue (only frame[0], not frame[1] since it's debug_self)
        assert_eq!(children.len(), 10, "expected 10 children (2 frames + 4 red + 4 blue), got {}", children.len());

        // red self-lines for stack
        for i in 2..6 {
            assert_eq!(children[i]["color"], "#ff0033", "child[{i}] should be red stack self-line");
        }
        // blue child-lines only for frame[0] (4 lines)
        for i in 6..10 {
            assert_eq!(children[i]["color"], "#0066ff", "child[{i}] should be blue child-line");
        }

        // The flagged frame child (index 1) must have its own 4 red self-lines
        let frame2 = &children[1];
        let frame2_children = frame2["children"].as_array().unwrap();
        assert_eq!(frame2_children.len(), 4, "flagged frame should have 4 self-lines");
        for i in 0..4 {
            assert_eq!(frame2_children[i]["color"], "#ff0033", "frame2 child[{i}] should be red");
        }

        // The unflagged frame child (index 0) must have no children
        let frame1 = &children[0];
        assert!(frame1["children"].as_array().map(|a| a.is_empty()).unwrap_or(true),
            "unflagged frame should have no children");
    }

    // ── Page size table ───────────────────────────────────────────────────────

    #[test]
    fn page_sizes_correct() {
        let cases: &[(&str, f64, f64)] = &[
            ("a3",     841.89, 1190.55),
            ("a5",     419.53,  595.28),
            ("letter", 612.0,   792.0),
            ("legal",  612.0,  1008.0),
        ];
        for &(size, exp_w, exp_h) in cases {
            let xml = format!(
                r#"<lpdf version="1"><document size="{size}" margin="0pt"><pages><page /></pages></document></lpdf>"#
            );
            let tree = engine_render(&xml);
            let w = tree["pages"][0]["width"].as_f64().unwrap();
            let h = tree["pages"][0]["height"].as_f64().unwrap();
            assert!((w - exp_w).abs() < 0.1, "{size}: width {w} != {exp_w}");
            assert!((h - exp_h).abs() < 0.1, "{size}: height {h} != {exp_h}");
        }
    }

    // ── Flank layout ──────────────────────────────────────────────────────────

    #[test]
    fn flank_default_places_flanks_left_fill_right() {
        // end=false (default): first child is the flank (explicit width=100pt),
        // last child is the fill and takes all remaining width on the right.
        let body = r#"<flank gap="0pt"><frame width="100pt" height="20pt" /><frame height="20pt" /></flank>"#;
        let tree = engine_render(&minimal(body));
        let children = tree["pages"][0]["nodes"][0]["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);

        let margin  = 28.0_f64;
        let avail_w = 595.28 - 2.0 * margin;

        let flank_x = children[0]["x"].as_f64().unwrap();
        let flank_w = children[0]["width"].as_f64().unwrap();
        let fill_x  = children[1]["x"].as_f64().unwrap();
        let fill_w  = children[1]["width"].as_f64().unwrap();

        assert!((flank_x - margin).abs() < 0.1,              "flank_x={flank_x}");
        assert!((flank_w - 100.0).abs() < 0.1,               "flank_w={flank_w}");
        assert!((fill_x  - (margin + 100.0)).abs() < 0.1,    "fill_x={fill_x}");
        assert!((fill_w  - (avail_w - 100.0)).abs() < 0.5,   "fill_w={fill_w}");
    }

    #[test]
    fn flank_end_true_places_fill_left_flanks_right() {
        // end=true: fill child (first in XML) goes left, flank child (second, 100pt) goes right.
        let body = r#"<flank gap="0pt" end="true"><frame height="20pt" /><frame width="100pt" height="20pt" /></flank>"#;
        let tree = engine_render(&minimal(body));
        let children = tree["pages"][0]["nodes"][0]["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);

        let margin  = 28.0_f64;
        let avail_w = 595.28 - 2.0 * margin;

        let fill_x  = children[0]["x"].as_f64().unwrap();
        let fill_w  = children[0]["width"].as_f64().unwrap();
        let flank_x = children[1]["x"].as_f64().unwrap();
        let flank_w = children[1]["width"].as_f64().unwrap();

        assert!((fill_x  - margin).abs() < 0.1,                      "fill_x={fill_x}");
        assert!((fill_w  - (avail_w - 100.0)).abs() < 0.5,           "fill_w={fill_w}");
        assert!((flank_x - (margin + avail_w - 100.0)).abs() < 0.5,  "flank_x={flank_x}");
        assert!((flank_w - 100.0).abs() < 0.1,                       "flank_w={flank_w}");
    }

    // ── Span decoration ───────────────────────────────────────────────────────

    #[test]
    fn span_underline_emits_line_decoration() {
        let body = r#"<text size="m"><span underline="true">hello</span></text>"#;
        let tree = engine_render(&minimal(body));
        let kids = tree["pages"][0]["nodes"][0]["children"].as_array().unwrap();
        // span atom produces a text node + an underline line node
        let types: Vec<&str> = kids.iter().filter_map(|k| k["type"].as_str()).collect();
        assert!(types.contains(&"text"), "no text node; got {:?}", types);
        assert!(types.contains(&"line"), "no underline line; got {:?}", types);
    }

    #[test]
    fn span_strike_emits_line_decoration() {
        let body = r#"<text size="m"><span strike="true">hello</span></text>"#;
        let tree = engine_render(&minimal(body));
        let kids = tree["pages"][0]["nodes"][0]["children"].as_array().unwrap();
        let types: Vec<&str> = kids.iter().filter_map(|k| k["type"].as_str()).collect();
        assert!(types.contains(&"text"), "no text node; got {:?}", types);
        assert!(types.contains(&"line"), "no strikethrough line; got {:?}", types);
    }

    #[test]
    fn span_href_wraps_in_link_node() {
        let body = r#"<text size="m"><span href="https://example.com">click here</span></text>"#;
        let tree = engine_render(&minimal(body));
        let kids = tree["pages"][0]["nodes"][0]["children"].as_array().unwrap();
        let link = kids.iter().find(|k| k["type"] == "link").expect("no link node in children");
        assert_eq!(link["url"], "https://example.com");
        assert!(!link["children"].as_array().unwrap().is_empty());
    }

    // ── Custom document tokens ────────────────────────────────────────────────

    #[test]
    fn custom_tokens_space_scale_overrides_default() {
        // Override the "m" space token to 50pt. A stack gap="m" should produce
        // a 50pt gap, not the default 8pt.
        let xml = r#"<lpdf version="1">
            <tokens>
                <space xs="1pt" s="2pt" m="50pt" l="100pt" xl="200pt" xxl="400pt" />
            </tokens>
            <document size="a4" margin="0pt">
                <pages><page>
                    <stack gap="m">
                        <frame height="20pt" />
                        <frame height="20pt" />
                    </stack>
                </page></pages>
            </document>
        </lpdf>"#;
        let tree = engine_render(xml);
        let children = tree["pages"][0]["nodes"][0]["children"].as_array().unwrap();
        let y0 = children[0]["y"].as_f64().unwrap();
        let y1 = children[1]["y"].as_f64().unwrap();
        // second child y = first y + first height (20) + custom gap (50)
        assert!((y1 - y0 - 20.0 - 50.0).abs() < 0.5,
            "expected 50pt gap but y0={y0} y1={y1}");
    }

    #[test]
    fn custom_tokens_color_overrides_default() {
        // Override the "primary" color token. The frame background should use
        // the custom value, not the built-in default.
        let xml = r##"<lpdf version="1">
            <tokens>
                <colors>
                    <color name="primary" value="#abcdef" />
                </colors>
            </tokens>
            <document size="a4" margin="0pt">
                <pages><page>
                    <frame height="20pt" background="primary" />
                </page></pages>
            </document>
        </lpdf>"##;
        let tree = engine_render(xml);
        let fill = tree["pages"][0]["nodes"][0]["fill"].as_str().unwrap();
        assert_eq!(fill, "#abcdef");
    }
}
