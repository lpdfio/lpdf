use crate::parse::{Align, Direction, HeightMode, Justify, Node, NodeKind, Page, TextAlign};
use crate::render::{RenderBox, RenderLine, RenderLink, RenderNode, RenderPage, RenderText};

// ── Public entry point ────────────────────────────────────────────────────────

pub fn layout_page(page: &Page) -> Vec<RenderPage> {
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

    // Build per-page chunks at the source-node level so that each chunk is
    // independently laid out from y = avail_y — no post-layout y adjustment.
    let chunks = split_into_pages(&page.children, avail_w, avail_h, 0.0);

    chunks
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
        .collect()
}

// ── Source-level pagination ───────────────────────────────────────────────────

/// Packs `nodes` (stacked vertically with `gap` between adjacent items) into
/// page-sized chunks, each of which will be independently laid out from
/// y = top_margin on a fresh page.
///
/// The algorithm is stateful: it tracks how much vertical space has been used
/// on the current page and tries to *partially* place splittable containers
/// (Stack, Frame, Grid) to fill the remaining space before starting a new page.
/// Atomic nodes (Flank, Split, Cluster, Divider, Text, Link, …) are never cut;
/// if one doesn't fit in the remaining space it moves wholesale to the next page.
///
/// Recursion through `split_node_at` handles arbitrary nesting depth — e.g. a
/// stack containing a grid inside another stack.
fn split_into_pages(nodes: &[Node], avail_w: f32, avail_h: f32, gap: f32) -> Vec<Vec<Node>> {
    if nodes.is_empty() {
        return vec![vec![]];
    }

    let mut pages: Vec<Vec<Node>> = vec![vec![]];
    let mut used_h: f32 = 0.0;
    let mut queue: std::collections::VecDeque<Node> = nodes.iter().cloned().collect();

    while let Some(node) = queue.pop_front() {
        let gap_before = if pages.last().unwrap().is_empty() { 0.0 } else { gap };
        let h = measure_height(&node, avail_w, avail_h);
        let remaining = avail_h - used_h - gap_before;

        if h <= remaining + 0.5 {
            // Fits on the current page.
            pages.last_mut().unwrap().push(node);
            used_h += gap_before + h;
        } else {
            // Doesn't fit. Try to split the node at the remaining height.
            let target = remaining.max(0.0);
            match split_node_at(&node, avail_w, target, avail_h) {
                SplitOutcome::First(first, rest) => {
                    // `first` fills the rest of the current page; `rest` goes on the next.
                    pages.last_mut().unwrap().push(first);
                    pages.push(vec![]);
                    used_h = 0.0;
                    for item in rest.into_iter().rev() {
                        queue.push_front(item);
                    }
                }
                SplitOutcome::NothingFits(rest) => {
                    // Nothing of this node fits in the remaining space.
                    if pages.last().unwrap().is_empty() {
                        // Already on a fresh page and still can't split — force-place
                        // the first piece to prevent an infinite loop.
                        let first = rest.into_iter().next().unwrap_or(node);
                        let fh = measure_height(&first, avail_w, avail_h);
                        pages.last_mut().unwrap().push(first);
                        used_h = fh.min(avail_h);
                    } else {
                        pages.push(vec![]);
                        used_h = 0.0;
                        for item in rest.into_iter().rev() {
                            queue.push_front(item);
                        }
                    }
                }
                SplitOutcome::Atomic => {
                    // Indivisible node: move to the next page.
                    if pages.last().unwrap().is_empty() {
                        // Force-place on an empty page even if it overflows.
                        pages.last_mut().unwrap().push(node);
                        used_h = avail_h; // treat page as full so next item starts a new page
                    } else {
                        pages.push(vec![]);
                        used_h = 0.0;
                        queue.push_front(node);
                    }
                }
            }
        }
    }

    // Drop any trailing empty page that may have been created by the last split.
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
/// Splittable node types:
/// - **Stack / Frame (Auto)**: split child-by-child.  If the very first child
///   overflows `target_h` the function recurses into that child.
/// - **Grid (Auto)**: split row-by-row (whole rows only).
///
/// Everything else (Flank, Split, Cluster, Divider, Text, Link, Fixed/Full/Fill
/// height nodes) returns `Atomic`.
fn split_node_at(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    if node.height_mode != HeightMode::Auto || node.children.is_empty() {
        return SplitOutcome::Atomic;
    }

    match node.kind {
        // ── Stack / Frame ─────────────────────────────────────────────────────
        NodeKind::Stack | NodeKind::Frame => {
            let [pt, pr, pb, pl] = node.padding;
            let inner_w = (avail_w - pl - pr).max(0.0);
            let inner_target = (target_h - pt - pb).max(0.0);
            let inner_full = (full_page_h - pt - pb).max(0.0);
            let gap = node.gap;
            let n = node.children.len();

            // Greedy-fill: find how many children fit within inner_target.
            let mut split_idx = 0usize;
            let mut chunk_h = 0.0_f32;

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
                return SplitOutcome::Atomic; // all children fit
            }

            if split_idx == 0 {
                // The very first child already exceeds inner_target.
                // Recurse into that child to fill whatever space remains.
                match split_node_at(&node.children[0], inner_w, inner_target, inner_full) {
                    SplitOutcome::First(child_first, child_rest) => {
                        let first_node = Node {
                            children: vec![child_first],
                            ..node.clone()
                        };
                        let mut rest_children = child_rest;
                        rest_children.extend_from_slice(&node.children[1..]);
                        let rest_node = Node { children: rest_children, ..node.clone() };
                        SplitOutcome::First(first_node, vec![rest_node])
                    }
                    SplitOutcome::NothingFits(_) | SplitOutcome::Atomic => {
                        // Can't fit anything on the current page.
                        SplitOutcome::NothingFits(vec![node.clone()])
                    }
                }
            } else {
                // Some children fit, the rest go to the next page.
                let first = Node {
                    children: node.children[..split_idx].to_vec(),
                    ..node.clone()
                };
                let rest = Node {
                    children: node.children[split_idx..].to_vec(),
                    ..node.clone()
                };
                SplitOutcome::First(first, vec![rest])
            }
        }

        // ── Grid ──────────────────────────────────────────────────────────────
        NodeKind::Grid => {
            let [pt, pr, pb, pl] = node.padding;
            let inner_w = (avail_w - pl - pr).max(0.0);
            let inner_target = (target_h - pt - pb).max(0.0);
            let inner_full = (full_page_h - pt - pb).max(0.0);
            let gap = node.gap;

            let cols = if let Some(min_w) = node.col_width {
                let n = ((inner_w + gap) / (min_w + gap)).floor() as usize;
                n.max(1)
            } else {
                (node.cols as usize).max(1)
            };
            let col_w = ((inner_w - gap * (cols - 1) as f32) / cols as f32).max(0.0);

            let n_rows = (node.children.len() + cols - 1) / cols;
            let mut split_row = 0usize;
            let mut chunk_h = 0.0_f32;

            for row in 0..n_rows {
                let row_start = row * cols;
                let row_end = (row_start + cols).min(node.children.len());
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

            if split_row == n_rows {
                return SplitOutcome::Atomic; // all rows fit
            }
            if split_row == 0 {
                return SplitOutcome::NothingFits(vec![node.clone()]);
            }

            let item_split = (split_row * cols).min(node.children.len());
            let first = Node {
                children: node.children[..item_split].to_vec(),
                ..node.clone()
            };
            let rest = Node {
                children: node.children[item_split..].to_vec(),
                ..node.clone()
            };
            SplitOutcome::First(first, vec![rest])
        }

        // All other kinds (Flank, Split, Cluster, Divider, Text, Link) are atomic.
        _ => SplitOutcome::Atomic,
    }
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

    let (border_width, border_color) = node
        .border
        .as_ref()
        .map(|(t, c)| (*t, Some(c.clone())))
        .unwrap_or((0.0, None));

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
            node.gap, node.wrap, &node.justify, &node.align,
        ),
        NodeKind::Grid => layout_grid(
            &node.children, x, y, avail_w,
            node.gap, node.cols, node.col_width,
        ),
        NodeKind::Frame => layout_stack(
            // Frame centres its children by default (overridable via align/justify attrs)
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.align, &node.justify,
        ),
        NodeKind::Link => layout_stack(
            &node.children, x, y, avail_w, avail_h,
            0.0, &Align::Stretch, &Justify::Start,
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

    let (first_w, second_w) = if equal {
        let w = ((avail_w - gap) / 2.0).max(0.0);
        (w, w)
    } else {
        match (first.width_constraint, second.width_constraint) {
            (Some(fw), _) => {
                let fw = fw.min(avail_w - gap);
                (fw, (avail_w - gap - fw).max(0.0))
            }
            (None, Some(sw)) => {
                let sw = sw.min(avail_w - gap);
                ((avail_w - gap - sw).max(0.0), sw)
            }
            (None, None) => {
                let w = ((avail_w - gap) / 2.0).max(0.0);
                (w, w)
            }
        }
    };

    let (first_node, first_h) = layout_node(first, x, y, first_w, avail_h);
    let (second_node, second_h) =
        layout_node(second, x + first_w + gap, y, second_w, avail_h);

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
    wrap: bool,
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
        if wrap && !current_row.is_empty() && cur_x + cw > x + avail_w + 0.01 {
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

        // Justify: compute starting x and extra inter-item gap for this row
        let remaining_w = (avail_w - row_w).max(0.0);
        let (row_start_x, item_extra_gap) = match justify {
            Justify::Start   => (x, 0.0),
            Justify::Center  => (x + remaining_w / 2.0, 0.0),
            Justify::End     => (x + remaining_w, 0.0),
            Justify::Between => {
                if row.len() > 1 {
                    (x, remaining_w / (row.len() - 1) as f32)
                } else {
                    (x, 0.0)
                }
            }
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
            item_x += item.w + gap + item_extra_gap;
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
        RenderNode::Link(RenderLink { url, x, y, width: avail_w, height, children }),
        height,
    )
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

fn builtin_char_advance(font: &str, c: char) -> Option<u16> {
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
fn text_width(font: &str, text: &str, size: f32) -> f32 {
    // Courier family: monospace, 600/1000 per glyph.
    if font.contains("Courier") {
        return text.chars().count() as f32 * size * 0.6;
    }
    // For known builtins use per-character AFM widths; for unknown fonts fall
    // back to a constant multiplier (same as before).  We detect "known" by
    // checking whether the space glyph (always ASCII 32) has a table entry.
    if builtin_char_advance(font, ' ').is_none() {
        return text.chars().count() as f32 * size * 0.44;
    }
    text.chars()
        .map(|c| {
            // 500/1000 = 0.5em for characters outside ASCII printable range.
            builtin_char_advance(font, c).unwrap_or(500) as f32 * size / 1000.0
        })
        .sum()
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
            border_width: 0.0, border_color: None, radius: 0.0, children: vec![],
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
                    }));
                }
                if atom.strike {
                    let sy = line_y + font_size * 0.5;
                    seg.push(RenderNode::Line(RenderLine {
                        x1: cur_x, y1: sy, x2: cur_x + aw, y2: sy,
                        color: atom.color.clone(), thickness: 0.75,
                    }));
                }

                match &atom.href {
                    Some(href) => all_nodes.push(RenderNode::Link(RenderLink {
                        url: href.clone(), x: cur_x, y: line_y,
                        width: aw, height: line_height, children: seg,
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
            children: all_nodes,
        }),
        total_h,
    )
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve child width given parent's available width and alignment.
fn resolve_child_w(child: &Node, avail_w: f32, _align: &Align) -> f32 {
    if let Some(w) = child.width_constraint {
        return w.min(avail_w);
    }
    avail_w
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
    }
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
}
