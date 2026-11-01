use crate::parse::{Align, BarcodeEcLevel, BarcodeType, Bookmark, Capture, CaptureMode, Direction, FieldKind, Footnote, HeightMode, Justify, Node, NodeKind, Ocg, PageScope, Role, RowKeep, SectionLayout, Paginate, TextAlign, TextRun};

// The path is spelled out because the WASI build includes this file from another crate.
#[path = "layout/footnotes.rs"]
mod footnotes;
#[path = "layout/table_grid.rs"]
mod table_grid;
#[cfg(test)]
#[path = "layout/row_split_tests.rs"]
mod row_split_tests;
#[cfg(test)]
#[path = "layout/footnote_clip_tests.rs"]
mod footnote_clip_tests;
pub(crate) use table_grid::{Held, Placed};
use table_grid::TableGrid;
use crate::page_scope::{chrome_in_budget, page_scope_indices};
use crate::render::{RenderAnchor, RenderBarcode, RenderBox, RenderCanvasClip, RenderCanvasLayer, RenderField, RenderFootnoteMark, RenderImage, RenderLine, RenderLink, RenderNode, RenderPage, RenderText, RenderedBarcodeKind, Tag, TagKind};
use crate::tokens::FontWidths;
use crate::canvas;
use crate::toc;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;

// ── Per-call custom font width tables ────────────────────────────────────────
// Set by LpdfEngine before calling layout_page so that text_width can use
// real glyph metrics instead of the 0.44 constant fallback.
thread_local! {
    static FONT_WIDTHS: RefCell<HashMap<String, FontWidths>> = RefCell::new(HashMap::new());
}

// ── Tags of a tagged PDF ─────────────────────────────────────────────────────
// A section of a document with `<output pdfua>` is laid out with tags, which say what each drawn thing is to a
// reader (`render::Tag`). Every other section is laid out as it always was, and pays for nothing.
thread_local! {
    /// Whether the section being laid out is tagged, for what draws nodes that have no tag id of their own: the notes
    /// of footnotes, and layers.
    static TAGGING: Cell<bool> = const { Cell::new(false) };
    /// The last tag id given. Ids count on across the sections of a document, and the passes of a table of contents.
    static LAST_TAG_ID: Cell<u32> = const { Cell::new(0) };
}

/// Whether the section being laid out is tagged.
pub(crate) fn tagging() -> bool {
    TAGGING.with(Cell::get)
}

/// Give `node` and every node below it a tag id of its own, so that the parts a node is split into keep it, and the
/// cells of a table's header row their role, as `header_row` says they are in one.
fn number_for_tags(node: &mut Node, header_row: bool) {
    node.tag_id = LAST_TAG_ID.with(|last| {
        let id = last.get() + 1;
        last.set(id);
        id
    });
    if header_row && node.kind == NodeKind::TableCell && node.role == Role::Auto {
        node.role = Role::HeaderCell;
    }
    let head = node.kind == NodeKind::TableHead;
    for child in &mut node.children {
        number_for_tags(child, head);
    }
}

/// What `node` is to a reader of a tagged PDF: its role, or what its kind makes it. `None` for a node that groups
/// others and adds nothing a reader needs, such as a column, and for one whose tag is set where it is drawn: a link.
pub(crate) fn tag_of(node: &Node) -> Option<TagKind> {
    let cell = |header: bool| TagKind::TableCell { header, col_span: node.col_span, row_span: node.row_span, column: 0 };
    Some(match node.role {
        Role::Artifact      => TagKind::Artifact,
        Role::Heading(level) => TagKind::Heading(level),
        Role::List          => TagKind::List,
        Role::ListItem      => TagKind::ListItem,
        Role::ListLabel     => TagKind::ListLabel,
        Role::ListBody      => TagKind::ListBody,
        Role::HeaderCell    => cell(true),
        Role::TocItem       => TagKind::TocItem,
        Role::Auto => match node.kind {
            NodeKind::Text      => TagKind::Paragraph,
            NodeKind::Table     => TagKind::Table,
            NodeKind::TableHead => TagKind::TableHead,
            NodeKind::TableRow  => TagKind::TableRow,
            NodeKind::TableCell => cell(false),
            // An image or a barcode with words in its alt shows something; with alt="" it is decoration.
            NodeKind::Img | NodeKind::Barcode => match node.alt() {
                Some(alt) if !alt.trim().is_empty() => TagKind::Figure(alt.to_string()),
                _ => TagKind::Artifact,
            },
            _ => return None,
        },
    })
}

/// `rendered` with `tag` on it: on the box, the image or the barcode it is, or on a box that draws nothing around it.
fn with_tag(rendered: RenderNode, tag: Tag, x: f32, y: f32, width: f32, height: f32) -> RenderNode {
    let tag = Some(Box::new(tag));
    match rendered {
        RenderNode::Box(mut b) if b.tag.is_none() => {
            b.tag = tag;
            RenderNode::Box(b)
        }
        RenderNode::Image(mut i) => {
            i.tag = tag;
            RenderNode::Image(i)
        }
        RenderNode::Barcode(mut bc) => {
            bc.tag = tag;
            RenderNode::Barcode(bc)
        }
        other => tagged_box(tag, x, y, width, height, vec![other]),
    }
}

/// A box that draws nothing of its own, around `children`, with `tag` on it.
fn tagged_box(tag: Option<Box<Tag>>, x: f32, y: f32, width: f32, height: f32, children: Vec<RenderNode>) -> RenderNode {
    RenderNode::Box(RenderBox {
        x, y, width, height,
        fill: None, border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None, debug_self: false,
        tag, ocg: None, children,
    })
}

// ── Optional content ─────────────────────────────────────────────────────────
// A node with `ocg` is drawn in a box that names the group, so the group holds everything the node draws, and a node
// in it with a group of its own is a box inside that box, which shows only when both groups do. The box takes no room
// of its own: hidden content keeps its place, and nothing moves.

/// `rendered` in the optional content group `ocg`: the group goes on the box it is, or on a box that draws nothing
/// around it, `x`, `y`, `width`, `height`. `None` leaves it as it is.
pub(crate) fn in_group(rendered: RenderNode, ocg: Option<Ocg>, x: f32, y: f32, width: f32, height: f32) -> RenderNode {
    let Some(ocg) = ocg else {
        return rendered;
    };
    match rendered {
        RenderNode::Box(mut b) if b.ocg.is_none() => {
            b.ocg = Some(ocg);
            RenderNode::Box(b)
        }
        other => RenderNode::Box(RenderBox {
            x, y, width, height,
            fill: None, border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None, debug_self: false,
            tag: None, ocg: Some(ocg), children: vec![other],
        }),
    }
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
/// Call this on every `SectionLayout` *before* calling `layout_page`. After the pass,
/// `layout_img` can read `.width_constraint.unwrap_or(avail_w)` and
/// `.img_height_constraint.unwrap_or(w)` safely.
pub fn prefill_image_sizes(nodes: &mut Vec<Node>, meta: &ImageMeta) {
    for node in nodes.iter_mut() {
        prefill_node(node, meta);
    }
}

/// `prefill_image_sizes` for a whole section: its layout, and the `img-at` of its layers.
pub fn prefill_section_image_sizes(section: &mut SectionLayout, meta: &ImageMeta) {
    prefill_image_sizes(&mut section.children, meta);
    for layer in section.underlays.iter_mut().chain(section.overlays.iter_mut()) {
        for child in &mut layer.children {
            if let canvas::CanvasPrimitive::Placed(placed) = child {
                prefill_node(&mut placed.node, meta);
            }
        }
    }
}

/// The size of a content with the proportions `natural`, inside the `width` and `height` it is given.
///
/// One size given: the other follows the proportions. Both: they are a maximum, and the content takes the
/// largest size that fits in them with its own proportions, so it is never stretched. Neither: the natural size.
fn fit_within(natural: (f32, f32), width: Option<f32>, height: Option<f32>) -> (f32, f32) {
    let (nat_w, nat_h) = natural;
    let aspect = if nat_h > 0.0 && nat_w > 0.0 { nat_w / nat_h } else { 1.0 };
    match (width, height) {
        (Some(w), Some(h)) if h > 0.0 && w / h > aspect => (h * aspect, h),
        (Some(w), Some(_))                              => (w, w / aspect),
        (Some(w), None)                                 => (w, w / aspect),
        (None, Some(h))                                 => (h * aspect, h),
        (None, None)                                    => natural,
    }
}

/// Whether an image sets a `padding`, a `background` or a `border`, and so is a box around its picture.
fn sets_box(img: &Node) -> bool {
    img.padding != [0.0; 4] || img.background.is_some() || img.border.is_some()
}

/// An image that sets a `padding`, a `background` or a `border` is a box around its picture. `width` and `height`
/// size the picture, as they do for an image without them, and the box grows by the padding. Its border is drawn
/// on its edge and takes no room, as on every box. The picture is clipped to the box's `radius` less the smallest
/// padding, which is how far the picture is from the edge.
fn box_around_picture(img: Node) -> Node {
    let [pt, pr, pb, pl] = img.padding;
    let least_padding = pt.min(pr).min(pb).min(pl);
    // A picture that fills takes what the padding leaves of the box, and the box takes its slot.
    let width_constraint = img.width_constraint.map(|picture_w| picture_w + pl + pr);
    let picture = Node {
        padding: [0.0; 4],
        background: None,
        border: None,
        radius: (img.radius - least_padding).max(0.0),
        paginate: Paginate::None,
        debug: false,
        bookmarks: Vec::new(),
        ..img.clone()
    };
    Node {
        kind: NodeKind::Column,
        children: vec![picture],
        width_constraint,
        image_name: None,
        img_height_constraint: None,
        img_aspect: None,
        ..img
    }
}

fn prefill_node(node: &mut Node, meta: &ImageMeta) {
    if node.kind == NodeKind::Img && node.width_fill {
        // The slot gives a picture with `width="fill"` its width, which is not known yet: keep the proportions
        // and the `height` it may not pass, for `layout_img`.
        let name = node.image_name.as_deref().unwrap_or("");
        let (nat_w, nat_h) = meta.get(name).map_or((100.0, 100.0), |&(w, h)| (w as f32, h as f32));
        node.img_aspect = Some(if nat_w > 0.0 && nat_h > 0.0 { nat_w / nat_h } else { 1.0 });
        if sets_box(node) {
            *node = box_around_picture(std::mem::replace(node, Node::layout_default()));
        }
    } else if node.kind == NodeKind::Img {
        let name = node.image_name.as_deref().unwrap_or("");
        let known = meta.get(name).map(|&(w, h)| (w as f32, h as f32));
        let (w, h) = match (node.width_constraint, node.img_height_constraint, known) {
            // An image that is not loaded has no proportions to keep, so a box stays as it was given.
            (Some(w), Some(h), None) => (w, h),
            _ => fit_within(known.unwrap_or((100.0, 100.0)), node.width_constraint, node.img_height_constraint),
        };
        node.width_constraint      = Some(w);
        node.img_height_constraint = Some(h);
        if sets_box(node) {
            *node = box_around_picture(std::mem::replace(node, Node::layout_default()));
        }
    } else {
        for child in &mut node.children {
            prefill_node(child, meta);
        }
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn layout_page(page: &SectionLayout) -> Vec<RenderPage> {
    if !page.tagged {
        return layout_section(page);
    }
    // The section's nodes are numbered once its data is in, so a node that `data-source` repeats is one of its own.
    let mut tagged = page.clone();
    for node in &mut tagged.children {
        number_for_tags(node, false);
    }
    TAGGING.with(|on| on.set(true));
    let pages = layout_section(&tagged);
    TAGGING.with(|on| on.set(false));
    pages
}

fn layout_section(page: &SectionLayout) -> Vec<RenderPage> {
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
            for node in rp.nodes.iter_mut() {
                match node {
                    RenderNode::Box(b) if !b.debug_self => {
                        overlay.extend(debug_rect_lines(b.x, b.y, b.width, b.height, "#0066ff"));
                    }
                    // A flow cut at the bottom of the page (`clip_overflow`): its outlines are cut with it.
                    RenderNode::CanvasLayer(l) => {
                        let lines: Vec<RenderNode> = l.children.iter()
                            .filter_map(|c| match c {
                                RenderNode::Box(b) if !b.debug_self => Some(debug_rect_lines(b.x, b.y, b.width, b.height, "#0066ff")),
                                _ => None,
                            })
                            .flatten()
                            .collect();
                        l.children.extend(lines);
                    }
                    _ => {}
                }
            }
            rp.nodes.extend(overlay);
        }
    }
    canvas::apply_canvas_layers(&mut pages, &page.underlays, true);
    canvas::apply_canvas_layers(&mut pages, &page.overlays, false);
    // The section's own bookmark points at the top of its first page, ahead of the bookmarks in its layout.
    if let (Some(bookmark), Some(first)) = (&page.bookmark, pages.first_mut()) {
        first.nodes.insert(0, anchor(bookmark, 0.0));
    }
    pages
}

/// Lay out every section in document order. A table of contents section is laid out after the others, once
/// their pages are known; see `toc`.
pub fn layout_document(sections: &[SectionLayout]) -> Vec<RenderPage> {
    LAST_TAG_ID.with(|last| last.set(0));
    let toc = sections.iter().enumerate().find_map(|(i, s)| s.toc.as_ref().map(|options| (i, options)));
    match toc {
        Some((toc_index, options)) => toc::layout_with_toc(sections, toc_index, options),
        None                       => sections.iter().flat_map(layout_page).collect(),
    }
}

fn layout_page_impl(page: &SectionLayout) -> Vec<RenderPage> {
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
        && page.children[0].page_scope.is_none()
        && !footnotes::has_footnotes(&page.children)
    {
        let (nodes, _) = layout_column(
            &page.children, avail_x, avail_y, avail_w, Some(avail_h),
            0.0, &Align::Stretch, &Justify::Start,
        );
        return vec![RenderPage {
            width: page.width,
            height: page.height,
            bleed: page.bleed,
            background: page.background.clone(),
            margin: page.margin,
            nodes,
        }];
    }

    // ── Partition direct children into chrome (top/bottom) and flow ──────────
    let mut lead = 0usize;
    while lead < page.children.len() && page.children[lead].page_scope.is_some() {
        lead += 1;
    }
    let mut trail_start = page.children.len();
    while trail_start > lead && page.children[trail_start - 1].page_scope.is_some() {
        trail_start -= 1;
    }
    // A text with `capture-ref` takes its lines whatever it shows, so the chrome is measured before it is filled.
    let top_chrome: Vec<Node> = page.children[..lead].iter().cloned().map(reserve_sink_lines).collect();
    let mut flow: Vec<Node> = page.children[lead..trail_start]
        .iter()
        // Defensive: a repeat child stuck in the middle is treated as flow
        // (acts like an atomic node that appears once, not chrome).
        .cloned()
        .collect();
    let bot_chrome: Vec<Node> = page.children[trail_start..].iter().cloned().map(reserve_sink_lines).collect();
    // Texts that capture say their words whole before the flow is split; one in the flow that shows a capture
    // shows its fallback.
    for node in flow.iter_mut() {
        record_captures(node);
        fill_sinks(node, &PageCaptures::new(), &HashMap::new());
    }

    // The notes of the footnotes take room at the bottom of their page, and are numbered once the counter is known.
    let has_notes = footnotes::has_footnotes(&flow);
    if has_notes {
        footnotes::number_flow(&mut flow, page.footnote_counter, page.footnote_start);
    }

    // Fast path: no chrome — standard pagination.
    if top_chrome.is_empty() && bot_chrome.is_empty() {
        // Each page is laid out as soon as it is filled, so that its nodes and the trees kept for them don't wait
        // for the pages after it.
        let start = FlowStart { x: avail_x, first_y: avail_y, rest_y: avail_y };
        return footnotes::split_flow(&flow, Some(start), avail_w, avail_h, avail_h, page.gap, page.footnote_counter, has_notes)
            .map(|chunk| {
                let room = avail_h - chunk.reserve;
                let (nodes, used) = layout_column_kept(
                    &chunk.nodes, chunk.kept, avail_x, avail_y, avail_w, Some(room),
                    page.gap, &Align::Stretch, &Justify::Start,
                );
                let mut nodes = clip_overflow(nodes, used, avail_y + room, room, page.width);
                nodes.extend(footnotes::draw_area(&chunk.notes, avail_x, avail_y + avail_h, avail_w, avail_h));
                RenderPage {
                    width: page.width,
                    height: page.height,
                    bleed: page.bleed,
                    background: page.background.clone(),
                    margin: page.margin,
                    nodes,
                }
            })
            .collect();
    }

    // ── Measure chrome heights ────────────────────────────────────────────────
    // "always" = sum of chrome that appears on pages 2+ (exclude First-only nodes)
    // "first"  = sum of all chrome (used on page 1)
    let top_always_h = measure_chrome_height(&top_chrome, avail_w, false);
    let top_first_h  = measure_chrome_height(&top_chrome, avail_w, true);
    let bot_always_h = measure_chrome_height(&bot_chrome, avail_w, false);
    let bot_first_h  = measure_chrome_height(&bot_chrome, avail_w, true);

    let budget_first = (avail_h - top_first_h - bot_first_h).max(0.0);
    let budget_rest  = (avail_h - top_always_h - bot_always_h).max(0.0);

    // ── Paginate flow with per-page budgets ──────────────────────────────────
    // Where the flow's start is known, a page's flow is laid out as soon as the page is filled, so that its nodes
    // and the trees kept for them don't wait for the pages after it. The header and the footer wait for the number
    // of pages, and so does the flow below a header that can be another height on some page.
    let start = header_height_fixed(&top_chrome)
        .then_some(FlowStart { x: avail_x, first_y: avail_y + top_first_h, rest_y: avail_y + top_always_h });
    let chunks: Vec<FilledPage> = footnotes::split_flow(&flow, start, avail_w, budget_first, budget_rest, page.gap, page.footnote_counter, has_notes)
        .enumerate()
        .map(|(idx, chunk)| {
            let flow_budget = if idx == 0 { budget_first } else { budget_rest };
            let room = flow_budget - chunk.reserve;
            let captured = page_captures(&chunk.nodes);
            let flow = match start {
                Some(start) => {
                    let (x, y) = start.on_page(idx == 0);
                    let (nodes, used) = layout_column_kept(
                        &chunk.nodes, chunk.kept, x, y, avail_w, Some(room),
                        page.gap, &Align::Stretch, &Justify::Start,
                    );
                    PageFlow::Laid { nodes: clip_overflow(nodes, used, y + room, room, page.width), y }
                }
                None => PageFlow::Nodes(chunk.nodes),
            };
            FilledPage { flow, captured, notes: chunk.notes, room }
        })
        .collect();
    let total_pages = chunks.len();

    // ── Build each output page ───────────────────────────────────────────────
    let mut pages_out: Vec<RenderPage> = Vec::with_capacity(total_pages);
    // The latest value captured under each name on the pages before this one.
    let mut latest: HashMap<String, String> = HashMap::new();
    for (idx, chunk) in chunks.into_iter().enumerate() {
        let is_first = idx == 0;
        let page_num = idx + 1;

        let captured = chunk.captured;
        let page_chrome = |chrome: &[Node]| -> Vec<Node> {
            chrome.iter()
                .filter(|n| page_scope_indices(&n.page_scope, total_pages).contains(&idx))
                .map(|n| {
                    let mut node = substitute_page_tokens(n, page_num, total_pages);
                    fill_sinks(&mut node, &captured, &latest);
                    node
                })
                .collect()
        };
        let top_here = page_chrome(&top_chrome);
        let bot_here = page_chrome(&bot_chrome);
        for (key, values) in &captured {
            if let Some(last) = values.last() { latest.insert(key.clone(), last.clone()); }
        }

        // Top chrome laid out from y = mt down.
        let (top_nodes, top_h) = layout_column(
            &top_here, avail_x, avail_y, avail_w, None,
            0.0, &Align::Stretch, &Justify::Start,
        );

        // Bottom chrome anchored to the bottom.
        let bot_h = measure_column_height(&bot_here, avail_w);
        let bot_y = avail_y + avail_h - bot_h;
        let (bot_nodes, _) = layout_column(
            &bot_here, avail_x, bot_y, avail_w, None,
            0.0, &Align::Stretch, &Justify::Start,
        );

        // Flow between chromes.
        let flow_y = avail_y + top_h;
        let flow_budget = if is_first { budget_first } else { budget_rest };
        let room = chunk.room;
        let flow_nodes = match chunk.flow {
            PageFlow::Laid { nodes, y } => {
                debug_assert_eq!(y, flow_y, "the header of page {page_num} is another height than the budgets measured");
                nodes
            }
            PageFlow::Nodes(nodes) => {
                let (flow_nodes, used) = layout_column(
                    &nodes, avail_x, flow_y, avail_w, Some(room),
                    page.gap, &Align::Stretch, &Justify::Start,
                );
                clip_overflow(flow_nodes, used, flow_y + room, room, page.width)
            }
        };

        let mut all_nodes = top_nodes;
        all_nodes.extend(flow_nodes);
        all_nodes.extend(footnotes::draw_area(&chunk.notes, avail_x, bot_y, avail_w, flow_budget));
        all_nodes.extend(bot_nodes);

        pages_out.push(RenderPage {
            width: page.width,
            height: page.height,
            bleed: page.bleed,
            background: page.background.clone(),
            margin: page.margin,
            nodes: all_nodes,
        });
    }

    // Guarantee at least one page, even if flow is empty but chrome exists.
    if pages_out.is_empty() {
        let page_chrome = |chrome: &[Node]| -> Vec<Node> {
            chrome.iter()
                .filter(|n| page_scope_indices(&n.page_scope, 1).contains(&0))
                .map(|n| {
                    let mut node = substitute_page_tokens(n, 1, 1);
                    fill_sinks(&mut node, &PageCaptures::new(), &HashMap::new());
                    node
                })
                .collect()
        };
        let top_here = page_chrome(&top_chrome);
        let bot_here = page_chrome(&bot_chrome);
        let (top_nodes, _) = layout_column(&top_here, avail_x, avail_y, avail_w, None, 0.0, &Align::Stretch, &Justify::Start);
        let bot_h = measure_column_height(&bot_here, avail_w);
        let bot_y = avail_y + avail_h - bot_h;
        let (bot_nodes, _) = layout_column(&bot_here, avail_x, bot_y, avail_w, None, 0.0, &Align::Stretch, &Justify::Start);
        let mut all = top_nodes;
        all.extend(bot_nodes);
        pages_out.push(RenderPage {
            width: page.width, height: page.height, bleed: page.bleed,
            background: page.background.clone(), margin: page.margin,
            nodes: all,
        });
    }

    pages_out
}

/// A page of a section with a header or a footer, filled: its flow, what its texts captured, its notes and the
/// height its flow has.
struct FilledPage {
    flow:     PageFlow,
    captured: PageCaptures,
    notes:    Vec<footnotes::Note>,
    room:     f32,
}

/// The flow of a filled page: laid out at `y` when the page was filled, or its nodes, which wait for the header
/// above them.
enum PageFlow {
    Laid { nodes: Vec<RenderNode>, y: f32 },
    Nodes(Vec<Node>),
}

/// The flow of a page, laid out `used` high in a `room` that ends at `bottom`. A flow that runs past it holds a node
/// taller than a page, placed whole: the flow is clipped at `bottom`, so that it draws nothing over the footer, the
/// margin or the notes, and what the clip can't hide is fitted to it (`fit_to_clip`). A flow that fits is returned
/// as it is.
fn clip_overflow(mut nodes: Vec<RenderNode>, used: f32, bottom: f32, room: f32, page_w: f32) -> Vec<RenderNode> {
    if !flow_runs_past(used, room) {
        return nodes;
    }
    fit_to_clip(&mut nodes, bottom);
    vec![RenderNode::CanvasLayer(RenderCanvasLayer { tag: None,
        transform: None,
        clip:      Some(RenderCanvasClip { x: 0.0, y: 0.0, w: page_w, h: bottom, border_radius: 0.0 }),
        opacity:   1.0,
        children:  nodes,
    })]
}

/// Whether a page's flow laid out `used` high runs past its `room`, and is cut at the bottom of it (`clip_overflow`).
fn flow_runs_past(used: f32, room: f32) -> bool {
    used > room + 0.5
}

/// Fit to a clip at `bottom` what the clip in the content stream doesn't hide: a field that doesn't fit above it is
/// dropped, a link is cut off at it, and a bookmark below it marks it instead.
fn fit_to_clip(nodes: &mut Vec<RenderNode>, bottom: f32) {
    nodes.retain(|node| !matches!(node, RenderNode::Field(f) if f.y + f.height > bottom + 0.5));
    for node in nodes.iter_mut() {
        match node {
            RenderNode::Box(b)         => fit_to_clip(&mut b.children, bottom),
            RenderNode::CanvasLayer(l) => fit_to_clip(&mut l.children, bottom),
            // A link wholly below the clip keeps its content, which the clip hides, and loses its annotation.
            RenderNode::Link(l) if l.y >= bottom => {
                let mut children = std::mem::take(&mut l.children);
                fit_to_clip(&mut children, bottom);
                let (x, y, width, height) = (l.x, l.y, l.width, l.height);
                *node = RenderNode::Box(RenderBox { tag: None, ocg: None,
                    x, y, width, height,
                    fill: None, border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None,
                    debug_self: false, children,
                });
            }
            RenderNode::Link(l) => {
                fit_to_clip(&mut l.children, bottom);
                l.height = l.height.min(bottom - l.y);
            }
            RenderNode::Anchor(a) => a.y = a.y.min(bottom),
            _ => {}
        }
    }
}

/// Total column height of a list of nodes at a given width (uses the standard
/// column layout). Used to reserve top/bottom chrome space.
fn measure_column_height(nodes: &[Node], avail_w: f32) -> f32 {
    if nodes.is_empty() { return 0.0; }
    let (_, h) = layout_column(
        nodes, 0.0, 0.0, avail_w, None,
        0.0, &Align::Stretch, &Justify::Start,
    );
    h
}

/// Height of chrome nodes for budget estimation.  When `include_first` is false,
/// `First`-scoped nodes are excluded (i.e. what later pages look like).
/// Unknown-count scopes (`Last`/`Odd`/`Even`/`Pages`) are always included
/// conservatively to prevent page overflow.
fn measure_chrome_height(chrome: &[Node], avail_w: f32, include_first: bool) -> f32 {
    if chrome.is_empty() { return 0.0; }
    let selected: Vec<Node> = chrome.iter()
        .filter(|n| chrome_in_budget(&n.page_scope, include_first))
        .cloned()
        .collect();
    measure_column_height(&selected, avail_w)
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

// ── Running headers ───────────────────────────────────────────────────────────

/// What the texts of a page captured, by name, in reading order.
type PageCaptures = HashMap<String, Vec<String>>;

/// The words of a text, as it draws them: the runs of its spans joined, with the white space collapsed.
fn text_words(node: &Node) -> String {
    let mut out = String::new();
    for (i, run) in node.text_runs.iter().enumerate().filter(|(_, run)| run.footnote.is_none()) {
        if i > 0 && run.leading_space { out.push(' '); }
        out.push_str(&run.text);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Let every text with a `capture` below `node` say its words, whole. A text that splits across pages keeps
/// them with its first part.
fn record_captures(node: &mut Node) {
    if node.kind == NodeKind::Text && node.capture.as_ref().is_some_and(|c| c.key.is_some()) {
        let words = text_words(node);
        if let Some(c) = node.capture.as_mut() { c.words = Some(words); }
    }
    for child in node.children.iter_mut() { record_captures(child); }
}

/// Make the texts below `node` capture nothing: a copy of a node for a later page, whose words were recorded with
/// the node on the page it started on.
fn forget_captures(node: &mut Node) {
    if let Some(c) = node.capture.as_mut() { c.key = None; c.words = None; }
    for child in node.children.iter_mut() { forget_captures(child); }
}

/// Add what the texts below `node` captured to `out`.
fn collect_captures(node: &Node, out: &mut PageCaptures) {
    if let Some(Capture { key: Some(key), words: Some(words), .. }) = node.capture.as_deref() {
        out.entry(key.clone()).or_default().push(words.clone());
    }
    for child in &node.children { collect_captures(child, out); }
}

/// What the nodes that fell on one page captured.
fn page_captures(nodes: &[Node]) -> PageCaptures {
    let mut out = PageCaptures::new();
    for node in nodes { collect_captures(node, &mut out); }
    out
}

/// Give every text with `capture-ref` below `node` what it shows on a page that captured `here`, after the pages
/// that left `before` as the latest value of each name: the pick of its mode among what the page captured,
/// else the latest value, else its fallback.
fn fill_sinks(node: &mut Node, here: &PageCaptures, before: &HashMap<String, String>) {
    if let (NodeKind::Text, Some(sink)) = (&node.kind, node.capture_ref()) {
        let picked = here.get(&sink.key).and_then(|values| match sink.mode {
            CaptureMode::First => values.first(),
            CaptureMode::Last  => values.last(),
        });
        let words = picked.or_else(|| before.get(&sink.key)).unwrap_or(&sink.fallback).clone();
        node.text_runs = vec![TextRun {
            text: words, leading_space: false, font: None, color: None, href: None,
            underline: false, strike: false, bold: false, background: None, footnote: None,
        }];
    }
    for child in node.children.iter_mut() { fill_sinks(child, here, before); }
}

/// A text with `capture-ref` is one line unless `max-lines` says more, and takes that many lines whatever it
/// shows: a header or a footer is measured before the pages are filled, and its height must not depend on them.
fn reserve_sink_lines(mut node: Node) -> Node {
    fn walk(node: &mut Node) {
        if node.kind == NodeKind::Text && node.capture_ref().is_some() {
            let lines = *node.max_lines.get_or_insert(1);
            if let Some(c) = node.capture.as_mut() { c.reserve_lines = lines; }
        }
        for child in node.children.iter_mut() { walk(child); }
    }
    walk(&mut node);
    node
}

// ── Source-level pagination ───────────────────────────────────────────────────

/// The nodes still to be placed: the rest of the flow not yet touched, behind the pieces of nodes that were split
/// or sent on to the next page. Cloning one is cheap, so a page can be filled to try it and the try thrown away.
#[derive(Clone)]
struct Pending<'a> {
    flow:  &'a [Node],
    next:  usize,
    front: std::collections::VecDeque<Node>,
}

impl<'a> Pending<'a> {
    fn new(flow: &'a [Node]) -> Self {
        Pending { flow, next: 0, front: std::collections::VecDeque::new() }
    }

    fn is_empty(&self) -> bool {
        self.front.is_empty() && self.next >= self.flow.len()
    }

    fn pop(&mut self) -> Option<Node> {
        if let Some(node) = self.front.pop_front() { return Some(node); }
        let node = self.flow.get(self.next).cloned();
        if node.is_some() { self.next += 1; }
        node
    }

    fn push_front(&mut self, node: Node) {
        self.front.push_front(node);
    }

    fn peek(&self) -> Option<&Node> {
        self.front.front().or_else(|| self.flow.get(self.next))
    }
}

/// Where the flow of a section's pages starts: its x, and its y on the first page and on the pages after it,
/// below a header as high as the budgets measured it. It is known while the pages are filled when the header is that
/// high on every page (`header_height_fixed`).
#[derive(Clone, Copy)]
pub(super) struct FlowStart {
    x:       f32,
    first_y: f32,
    rest_y:  f32,
}

impl FlowStart {
    fn on_page(&self, first: bool) -> (f32, f32) {
        (self.x, if first { self.first_y } else { self.rest_y })
    }
}

/// Whether the header of a section is as high on every page as measured for the budgets: each of its nodes on
/// every page or on the first one only, with no page number in it. A node for the last page, the odd or the even
/// ones or a range of them is on some pages and not on others, which the budgets leave room for, and a
/// `{page}` or `{pages}` can wrap a text to another number of lines. A `capture-ref` text takes its lines whatever
/// it shows (`reserve_sink_lines`).
fn header_height_fixed(top_chrome: &[Node]) -> bool {
    fn has_page_tokens(node: &Node) -> bool {
        node.text_runs.iter().any(|run| run.text.contains("{page")) || node.children.iter().any(has_page_tokens)
    }
    top_chrome.iter().all(|node| {
        matches!(node.page_scope, None | Some(PageScope::Each) | Some(PageScope::First)) && !has_page_tokens(node)
    })
}

/// The tree of a node of a page's flow, which `fill_page` laid out where the page's `layout_column` places it, to
/// measure it: the column takes it as it is (`layout_column_kept`), instead of laying the node out again. `at` is
/// the x of the column and the y of the node.
pub(super) struct Kept {
    at:   (f32, f32),
    tree: RenderNode,
    h:    f32,
}

/// The height `fill_page` measures `node` at, and its tree when the node's place on the page is known: laid out
/// there, as the page's `layout_column` lays out an auto-height child, it is the tree that column needs. The height
/// is the same either way, since a node of auto height doesn't read the height it is given. A link, which passes
/// it on to its children, and a vertical divider, which takes it, are the two that do, and are measured as before.
fn measure_on_page(node: &Node, x: f32, place_y: Option<f32>, avail_w: f32, budget: f32) -> (f32, Option<Kept>) {
    let at_place = node.height_mode == HeightMode::Auto
        && node.kind != NodeKind::Link
        && !(node.kind == NodeKind::Divider && node.direction == Direction::Vertical);
    match place_y {
        Some(y) if at_place => {
            let (cw, cx) = column_child_place(node, x, avail_w, &Align::Stretch);
            let (tree, h) = layout_column_child(node, cx, y, cw, None, &Align::Stretch);
            (h, Some(Kept { at: (x, y), tree, h }))
        }
        _ => (measure_height(node, avail_w, budget), None),
    }
}

/// Where the page's `layout_column` places the node after one placed at `place_y`, `h` high: the same sum, while
/// the column's height for the node is known. That is a node laid out at its place, or one of a fixed height. A
/// `fill` or `full` node takes what the others leave, and a link or a vertical divider gets another height in the
/// column than the one `fill_page` measured.
fn next_place(place_y: Option<f32>, node: &Node, laid_out: bool, h: f32, gap: f32) -> Option<f32> {
    let known = laid_out || matches!(node.height_mode, HeightMode::Fixed(_));
    place_y.filter(|_| known).map(|py| py + (h + gap + 0.0))
}

/// Takes the nodes of one page off `pending`: as many as fit in `budget`, a node that does not fit being split
/// or sent on whole. `next_budget` is what the page after it has, which keep-next looks at. `at` is the x and y the
/// page's flow starts at, when it is known: a node placed whole then comes with its tree, laid out at its place
/// there (`Kept`). With the nodes comes whether the page may run past `budget`: its first node was placed whole
/// without fitting the empty page, or split there with a first part that can be taller than it (`may_run_past`).
///
/// Splittable: Column (Auto), Grid, Row, Text, Table, Link. Atomic: Frame, Divider, plus anything with
/// Fixed/Full/Fill. When a node doesn't fit and can't be split, it moves wholesale to the next page. If a single
/// atomic node is taller than a full page, it is force-placed, and the bottom of the page cuts it (`clip_overflow`).
fn fill_page(
    pending: &mut Pending,
    at: Option<(f32, f32)>,
    avail_w: f32,
    budget: f32,
    next_budget: f32,
    gap: f32,
) -> (Vec<(Node, Option<Kept>)>, bool) {
    let mut page: Vec<(Node, Option<Kept>)> = Vec::new();
    let mut runs_past = false;
    let mut used_h: f32 = 0.0;
    let x = at.map_or(0.0, |(x, _)| x);
    let mut place_y = at.map(|(_, y)| y);

    while let Some(node) = pending.pop() {
        // break-before: end the page before this node. Requeue without the flag to avoid an infinite loop on
        // a fresh page.
        if node.paginate == Paginate::BreakBefore && !page.is_empty() {
            pending.push_front(Node { paginate: Paginate::None, ..node });
            break;
        }

        let gap_before = if page.is_empty() { 0.0 } else { gap };
        let (h, tree) = measure_on_page(&node, x, place_y, avail_w, budget);
        let remaining = budget - used_h - gap_before;

        let needs_break_after;

        if h <= remaining + 0.5 {
            // keep-next: if the immediately following sibling would not fit on this
            // page after placing this node, and both fit together on the next page,
            // bump this node to the next page so they stay together. A sibling that
            // can start on this page, a table with its header and a row or a text with
            // a few lines, is split there, so the node is not parted from it and the
            // page is not left half empty.
            let keep_next_h = if node.paginate == Paginate::KeepNext && !page.is_empty() {
                pending.peek().and_then(|next| {
                    let next_h = measure_height(next, avail_w, budget);
                    let remaining_for_next = remaining - h - gap;
                    (next_h > remaining_for_next + 0.5
                        && h + gap + next_h <= next_budget + 0.5
                        && !starts_within(next, avail_w, remaining_for_next, budget))
                    .then_some(next_h)
                })
            } else {
                None
            };

            if let Some(next_h) = keep_next_h {
                // The nodes before it that keep with the one after them go on with it, as far as the next page
                // holds them: a heading above a heading that moves on is not left alone at the foot of the page.
                let mut room = next_budget + 0.5 - (h + gap + next_h);
                let mut back = 0;
                for (kept, _) in page.iter().rev().take_while(|(n, _)| n.paginate == Paginate::KeepNext) {
                    let kept_h = gap + measure_height(kept, avail_w, budget);
                    if kept_h > room { break; }
                    room -= kept_h;
                    back += 1;
                }
                // A page that would be left empty keeps them: they are already at the top of it.
                if back == page.len() { back = 0; }
                let moved = page.split_off(page.len() - back);
                pending.push_front(node);
                for (kept, _) in moved.into_iter().rev() {
                    pending.push_front(kept);
                }
                break;
            }
            needs_break_after = node.paginate == Paginate::BreakAfter;
            // Detect silently-ignored keep-next (only when debug=true): the
            // next sibling doesn't fit after this node AND both together
            // exceed a full page, so the constraint cannot be honoured.
            let warn_keep_next = node.debug
                && node.paginate == Paginate::KeepNext
                && pending.peek().map_or(false, |next| {
                    let next_h = measure_height(next, avail_w, next_budget);
                    let remaining_for_next = remaining - h - gap;
                    next_h > remaining_for_next + 0.5
                        && h + gap + next_h > next_budget + 0.5
                });
            place_y = next_place(place_y, &node, tree.is_some(), h, gap);
            page.push((node, tree));
            used_h += gap_before + h;
            if warn_keep_next {
                pending.push_front(make_keep_next_warning());
            }
        } else {
            // A node that doesn't fit keeps its tree only to be placed whole on an empty page, which a node that
            // splits never is: its tree goes before its parts are measured.
            let tree = tree.filter(|_| page.is_empty() && !splittable(&node));
            let target = remaining.max(0.0);
            match split_node_at(&node, avail_w, target, budget) {
                SplitOutcome::First(first, mut rest) => {
                    // Propagate break-after to the last continuation so the
                    // break fires after the final fragment, not the first.
                    if node.paginate == Paginate::BreakAfter {
                        if let Some(last) = rest.last_mut() {
                            last.paginate = Paginate::BreakAfter;
                        }
                    }
                    // A part can still be taller than the page, as a table's spanned rows or a row's child can.
                    runs_past |= page.is_empty() && may_run_past(&first);
                    let cut = node.debug && measure_height(&first, avail_w, budget) > target + 0.5;
                    page.push((first, None));
                    for item in rest.into_iter().rev() {
                        pending.push_front(item);
                    }
                    if cut {
                        pending.push_front(make_clip_warning());
                    }
                    break;
                }
                SplitOutcome::NothingFits(rest) => {
                    if page.is_empty() {
                        // Already on a fresh page and still can't split — force-place
                        // the first piece to prevent an infinite loop.
                        let first = rest.into_iter().next().unwrap_or(node);
                        needs_break_after = first.paginate == Paginate::BreakAfter;
                        let fh = measure_height(&first, avail_w, budget);
                        runs_past |= fh > budget + 0.5;
                        if first.debug && fh > budget + 0.5 {
                            pending.push_front(make_clip_warning());
                        }
                        page.push((first, None));
                        used_h = fh.min(budget);
                        place_y = None;
                    } else {
                        // paginate flags are preserved on rest items via clone
                        for item in rest.into_iter().rev() {
                            pending.push_front(item);
                        }
                        break;
                    }
                }
                SplitOutcome::Atomic => {
                    if page.is_empty() {
                        needs_break_after = node.paginate == Paginate::BreakAfter;
                        runs_past |= h > budget + 0.5;
                        if node.debug && h > budget + 0.5 {
                            pending.push_front(make_clip_warning());
                        }
                        place_y = next_place(place_y, &node, tree.is_some(), h, gap);
                        page.push((node, tree));
                        used_h = budget;
                    } else {
                        pending.push_front(node);
                        break;
                    }
                }
            }
        }

        // break-after: end the page after this node (or its last fragment) is placed.
        if needs_break_after && !page.is_empty() {
            break;
        }
    }

    (page, runs_past)
}

/// Construct a small red diagnostic text node injected into the layout stream
/// when `paginate="keep-next"` is set on a `debug="true"` node but cannot be
/// honoured because both this node and its next sibling together exceed a full
/// page height.
fn make_keep_next_warning() -> Node {
    debug_note("[lpdf] keep-next ignored: both nodes exceed page height")
}

/// The note injected into the layout stream after a `debug="true"` node that is taller than its page, which the
/// bottom of the page cuts (`clip_overflow`). The page is full, so the note starts the next one.
fn make_clip_warning() -> Node {
    debug_note("[lpdf] cut at the bottom of the page: content taller than the page")
}

/// A small red diagnostic text, injected into the layout stream for a `debug="true"` node.
fn debug_note(text: &str) -> Node {
    Node {
        kind: NodeKind::Text,
        text_runs: vec![TextRun {
            text: text.to_string(),
            leading_space: false,
            font: None,
            color: Some("#cc0000".to_string()),
            href: None,
            underline: false,
            strike: false,
            bold: false,
            background: None,
            footnote: None,
        }],
        font: "Helvetica".to_string(),
        font_size: 7.0,
        text_color: Some("#cc0000".to_string()),
        ..Node::layout_default()
    }
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
/// - **Column**: split child-by-child; recurses into first child if that's what overflows.
/// - **Grid**: split row-by-row (rows are atomic; children inside a row are atomic).
/// - **Row**: split between wrapped lines, or through the line the page edge falls in (`split_row`).
/// - **Text**: split line-by-line, reconstructing runs on each half.
///
/// Everything else (Frame, Divider, or any Fixed/Full/Fill
/// height node) returns `Atomic`. Frame is atomic by design — it represents a
/// card-like enclosure that should not be cut.
fn split_node_at(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    if !splittable(node) {
        return SplitOutcome::Atomic;
    }

    match node.kind {
        NodeKind::Column  => split_column(node, avail_w, target_h, full_page_h),
        NodeKind::Grid    => split_grid(node, avail_w, target_h, full_page_h),
        NodeKind::Row     => split_row(node, avail_w, target_h, full_page_h),
        NodeKind::Text    => split_text(node, avail_w, target_h, full_page_h),
        NodeKind::Table   => split_table(node, avail_w, target_h, full_page_h),
        NodeKind::Link    => split_column(node, avail_w, target_h, full_page_h),
        // Frame, Divider — atomic by design.
        _                 => SplitOutcome::Atomic,
    }
}

/// Whether `node` can put a first part of itself in `room`: it splits there, and what it leaves on the page is
/// something, not a header alone or a line alone, as `split_node_at` sees it.
fn starts_within(node: &Node, avail_w: f32, room: f32, full_page_h: f32) -> bool {
    matches!(split_node_at(node, avail_w, room.max(0.0), full_page_h), SplitOutcome::First(..))
}

/// Whether a first part that `split_node_at` made on an empty page can be taller than the page: it holds something
/// the cut can't go through, which is placed whole however tall it is. That is a box that can't split, or a row of a
/// grid or a table. A text ends at its last line that fits, and so does a column or a row of texts.
fn may_run_past(part: &Node) -> bool {
    if !splittable(part) {
        return true;
    }
    match part.kind {
        NodeKind::Text => false,
        NodeKind::Grid | NodeKind::Table => true,
        _ => part.children.iter().any(may_run_past),
    }
}

/// Whether `split_node_at` can split `node` at all: a text, or a box of `auto` height with children, of a kind
/// that splits, and without `paginate="no"`.
fn splittable(node: &Node) -> bool {
    node.paginate != Paginate::No
        && node.height_mode == HeightMode::Auto
        && (node.kind == NodeKind::Text || !node.children.is_empty())
        && matches!(
            node.kind,
            NodeKind::Column | NodeKind::Grid | NodeKind::Row | NodeKind::Text | NodeKind::Table | NodeKind::Link
        )
}

fn split_column(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
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
                if rest_children.is_empty() {
                    return SplitOutcome::First(first_node, Vec::new());
                }
                let rest_node = Node { children: rest_children, ..node.clone() };
                SplitOutcome::First(first_node, vec![rest_node])
            }
            // A first child taller than a page has nowhere better to go. On a full page it stays, cut at the
            // bottom of the page (`clip_overflow`), and the children after it go on to the next.
            SplitOutcome::NothingFits(_) | SplitOutcome::Atomic if n > 1 && inner_target + 0.5 >= inner_full => {
                let first = Node { children: node.children[..1].to_vec(), ..node.clone() };
                let rest  = Node { children: node.children[1..].to_vec(), ..node.clone() };
                SplitOutcome::First(first, vec![rest])
            }
            SplitOutcome::NothingFits(_) | SplitOutcome::Atomic => {
                SplitOutcome::NothingFits(vec![node.clone()])
            }
        }
    } else {
        // Some children fit; attempt to split the overflowing child into the
        // remaining space before falling back to a hard cut at the boundary.
        let remaining_for_child = (inner_target - chunk_h - gap).max(0.0);
        match split_node_at(&node.children[split_idx], inner_w, remaining_for_child, inner_full) {
            SplitOutcome::First(child_first, child_rest) => {
                let mut first_children = node.children[..split_idx].to_vec();
                first_children.push(child_first);
                let mut rest_children: Vec<Node> = child_rest;
                rest_children.extend_from_slice(&node.children[split_idx + 1..]);
                let first = Node { children: first_children, ..node.clone() };
                if rest_children.is_empty() {
                    return SplitOutcome::First(first, Vec::new());
                }
                let rest  = Node { children: rest_children, ..node.clone() };
                SplitOutcome::First(first, vec![rest])
            }
            // Unsplittable or nothing fits — fall back to hard cut before it.
            SplitOutcome::NothingFits(_) | SplitOutcome::Atomic => {
                let first = Node { children: node.children[..split_idx].to_vec(), ..node.clone() };
                let rest  = Node { children: node.children[split_idx..].to_vec(), ..node.clone() };
                SplitOutcome::First(first, vec![rest])
            }
        }
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
    if split_row == 0 {
        // A first row taller than a page stays on a full page, cut at the bottom of the page, and the rows after
        // it go on to the next.
        if n_rows == 1 || inner_target + 0.5 < inner_full {
            return SplitOutcome::NothingFits(vec![node.clone()]);
        }
        split_row = 1;
    }

    let item_split = (split_row * cols).min(node.children.len());
    let first = Node { children: node.children[..item_split].to_vec(), ..node.clone() };
    let rest  = Node { children: node.children[item_split..].to_vec(), ..node.clone() };
    SplitOutcome::First(first, vec![rest])
}

/// A line of a row's children: the children `start..end`, the width the line gives each, and the height of the
/// tallest.
struct RowLine {
    start:  usize,
    end:    usize,
    widths: Vec<f32>,
    height: f32,
}

/// The lines `layout_row` breaks `node`'s children into, `inner_w` wide, each child measured with `avail_h`. A fill
/// child counts as no width when the lines are formed, and gets the width its line leaves.
fn row_lines(node: &Node, inner_w: f32, avail_h: f32) -> Vec<RowLine> {
    let gap = node.gap;
    let widths: Vec<Option<f32>> = node.children.iter().map(|c| row_child_w(c, inner_w)).collect();
    let mut bounds: Vec<(usize, usize)> = Vec::new(); // (start, end_exclusive)
    let mut row_start = 0usize;
    let mut cur_x     = 0.0_f32;
    for (i, w) in widths.iter().enumerate() {
        let cw  = w.unwrap_or(0.0);
        let adv = if i == row_start { cw } else { gap + cw };
        if i > row_start && cur_x + adv > inner_w + 0.01 {
            bounds.push((row_start, i));
            row_start = i;
            cur_x = cw;
        } else {
            cur_x += adv;
        }
    }
    if row_start < node.children.len() {
        bounds.push((row_start, node.children.len()));
    }

    bounds
        .into_iter()
        .map(|(start, end)| {
            let line = &widths[start..end];
            let fixed_w: f32 = line.iter().flatten().sum();
            let n_fill = line.iter().filter(|w| w.is_none()).count();
            let fill_w = if n_fill > 0 {
                (inner_w - fixed_w - gap * (end - start - 1) as f32).max(0.0) / n_fill as f32
            } else {
                0.0
            };
            let widths: Vec<f32> = line.iter().map(|w| w.unwrap_or(fill_w)).collect();
            let height = node.children[start..end]
                .iter()
                .zip(&widths)
                .map(|(child, &w)| measure_height(child, w, avail_h))
                .fold(0.0_f32, f32::max);
            RowLine { start, end, widths, height }
        })
        .collect()
}

/// Splits a row between its lines, or through the line the page edge falls in. The lines above the edge stay on
/// this page and the lines below it go on to the next. The line the edge falls in is cut where each of its
/// children can stop (`line_cut`), and each child is split there (`cut_child`): what is left of them starts the
/// row's next part. A line with no such cut goes on to the next page whole, and the row with it when it is the
/// first.
fn split_row(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w     - pl - pr).max(0.0);
    let inner_target = (target_h    - pt - pb).max(0.0);
    let inner_full   = (full_page_h - pt - pb).max(0.0);
    let gap = node.gap;

    let lines = row_lines(node, inner_w, inner_target);
    let mut fit     = 0usize;
    let mut chunk_h = 0.0_f32;
    for (r, line) in lines.iter().enumerate() {
        let g = if r == 0 { 0.0 } else { gap };
        if chunk_h + g + line.height > inner_target + 0.5 {
            break;
        }
        chunk_h += g + line.height;
        fit = r + 1;
    }
    if fit == lines.len() { return SplitOutcome::Atomic; }

    let line = &lines[fit];
    let line_top = if fit == 0 { 0.0 } else { chunk_h + gap };
    // Only the first line of a row with a full page is at the top of a page, where a child taller than a page
    // can't do better.
    let full_page = fit == 0 && inner_target + 0.5 >= inner_full;
    let cut = line_cut(node, line, inner_target - line_top, full_page, inner_full);
    if cut <= 0.0 {
        if fit == 0 { return SplitOutcome::NothingFits(vec![node.clone()]); }
        let first = Node { children: node.children[..line.start].to_vec(), ..node.clone() };
        let rest  = Node { children: node.children[line.start..].to_vec(), ..node.clone() };
        return SplitOutcome::First(first, vec![rest]);
    }

    let mut first_children = node.children[..line.start].to_vec();
    let mut rests = Vec::with_capacity(line.end - line.start);
    for (child, &width) in node.children[line.start..line.end].iter().zip(&line.widths) {
        let (first, rest) = cut_child(child, width, cut, inner_full);
        first_children.push(first);
        rests.push((rest, child.width_fill, width));
    }
    let first = Node { children: first_children, ..node.clone() };
    // What is left of the line starts the row's next part, with an empty box in the place of each child that
    // ended. A line with nothing left, such as a child taller than the page beside others that ended, leaves none.
    let mut rest_children: Vec<Node> = if rests.iter().any(|(rest, ..)| rest.is_some()) {
        rests.into_iter().map(|(rest, fill, width)| rest.unwrap_or_else(|| place_of(fill, width))).collect()
    } else {
        Vec::new()
    };
    rest_children.extend_from_slice(&node.children[line.end..]);
    if rest_children.is_empty() {
        return SplitOutcome::First(first, Vec::new());
    }
    let rest = Node { children: rest_children, ..node.clone() };
    SplitOutcome::First(first, vec![rest])
}

/// The highest cut through `line`, at most `room` below its top, that suits every child of it. Each child is asked
/// (`child_cut`), and the lowest answer is asked again until no child lowers it. An answer is the cut asked, or
/// the top of something in the child, so it ends. `0` when only the top of the line suits them all.
fn line_cut(node: &Node, line: &RowLine, room: f32, full_page: bool, full_page_h: f32) -> f32 {
    let children = &node.children[line.start..line.end];
    let mut cut = room;
    while cut > 0.0 {
        let lowest = children
            .iter()
            .zip(&line.widths)
            .map(|(child, &width)| child_cut(child, width, cut, full_page, full_page_h))
            .fold(cut, f32::min);
        // However close to the top the cut is, a child that suits only its top keeps the line whole: a cut a
        // rounding error below the top keeps nothing of it on this page, and a child that can't split there would
        // be placed whole with nothing left for the next.
        if lowest <= 0.0 {
            return 0.0;
        }
        if lowest + 0.01 >= cut {
            break;
        }
        cut = lowest;
    }
    cut.max(0.0)
}

/// The highest cut at or above `y`, from the top of `node` laid out `avail_w` wide, that suits it: one where it
/// keeps something on this page and cuts nothing it can't, or one below all of it. `0` when only its top does.
///
/// - A text suits any cut where `split_text` keeps a line on this page. It ends at its last whole line above the
///   cut, so texts of different sizes side by side don't push the cut up to each other's lines.
/// - A node that can't be split suits a cut below it. When it is taller than a page and starts at the top of a
///   full page (`full_page`), any cut suits it: no page would hold more of it, and the bottom of the page clips it
///   (`clip_overflow`).
/// - A column asks the child the cut falls in, and a row the line (`column_cut`, `row_cut`).
/// - A grid or a table ends between its rows, at the bottom of the part it would keep.
fn child_cut(node: &Node, avail_w: f32, y: f32, full_page: bool, full_page_h: f32) -> f32 {
    let h = measure_height(node, avail_w, full_page_h);
    if h <= y + 0.5 {
        return y;
    }
    let taller_than_page = full_page && h > full_page_h + 0.5;
    if !splittable(node) {
        return if taller_than_page { y } else { 0.0 };
    }
    match node.kind {
        NodeKind::Column | NodeKind::Link => column_cut(node, avail_w, y, full_page, full_page_h),
        NodeKind::Row => row_cut(node, avail_w, y, full_page, full_page_h),
        NodeKind::Text => match split_text(node, avail_w, y, full_page_h) {
            SplitOutcome::First(..) => y,
            _ if taller_than_page => y,
            _ => 0.0,
        },
        _ => match split_node_at(node, avail_w, y, full_page_h) {
            SplitOutcome::First(first, _) => measure_height(&first, avail_w, full_page_h).min(y),
            _ if taller_than_page => y,
            _ => 0.0,
        },
    }
}

/// `child_cut` for a column: the cut of the child it falls in, from that child's top. A child whose only cut is
/// its top moves the cut up to it, or keeps the column for the next page when it is the first.
fn column_cut(node: &Node, avail_w: f32, y: f32, full_page: bool, full_page_h: f32) -> f32 {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w     - pl - pr).max(0.0);
    let inner_target = y - pt - pb;
    let inner_full   = (full_page_h - pt - pb).max(0.0);
    if inner_target <= 0.0 {
        return 0.0;
    }
    let mut top = 0.0_f32;
    for (i, child) in node.children.iter().enumerate() {
        if i > 0 {
            top += node.gap;
            // The cut falls between two children.
            if inner_target <= top { return y; }
        }
        let h = measure_height(child, inner_w, inner_full);
        if top + h <= inner_target + 0.5 {
            top += h;
            continue;
        }
        let cut = child_cut(child, inner_w, inner_target - top, full_page && i == 0, inner_full);
        return if cut > 0.0 { pt + top + cut + pb } else if i == 0 { 0.0 } else { pt + top + pb };
    }
    y
}

/// `child_cut` for a row in a row: the cut through the line it falls in (`line_cut`). A line whose only cut is its
/// top moves the cut up to it, or keeps the row for the next page when it is the first.
fn row_cut(node: &Node, avail_w: f32, y: f32, full_page: bool, full_page_h: f32) -> f32 {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w     - pl - pr).max(0.0);
    let inner_target = y - pt - pb;
    let inner_full   = (full_page_h - pt - pb).max(0.0);
    if inner_target <= 0.0 {
        return 0.0;
    }
    let mut top = 0.0_f32;
    for (r, line) in row_lines(node, inner_w, inner_target).iter().enumerate() {
        if r > 0 {
            top += node.gap;
            // The cut falls between two lines.
            if inner_target <= top { return y; }
        }
        if top + line.height <= inner_target + 0.5 {
            top += line.height;
            continue;
        }
        let cut = line_cut(node, line, inner_target - top, full_page && r == 0, inner_full);
        return if cut > 0.0 { pt + top + cut + pb } else if r == 0 { 0.0 } else { pt + top + pb };
    }
    y
}

/// `child` of a line cut at `cut`: its part on this page and what is left of it, if anything, both as wide as
/// `width`, the width the line gave it, so that the line breaks and wraps as it did. A child that ends above the
/// cut, or can't be split there, is placed whole.
fn cut_child(child: &Node, width: f32, cut: f32, full_page_h: f32) -> (Node, Option<Node>) {
    if measure_height(child, width, full_page_h) > cut + 0.5 {
        if let SplitOutcome::First(first, rest) = split_node_at(child, width, cut, full_page_h) {
            let rest = match <[Node; 1]>::try_from(rest) {
                Ok([rest]) => Some(rest),
                Err(parts) if parts.is_empty() => None,
                Err(parts) => Some(Node { kind: NodeKind::Column, children: parts, width_fill: child.width_fill, ..Node::layout_default() }),
            };
            return (keep_width(first, width), rest.map(|rest| keep_width(rest, width)));
        }
    }
    (child.clone(), None)
}

/// An empty box `width` wide, which keeps the place of a child that ended in the row's next part.
fn place_of(fill: bool, width: f32) -> Node {
    keep_width(Node { kind: NodeKind::Column, width_fill: fill, ..Node::layout_default() }, width)
}

/// `node` at `width`, the width its line gave it. A fill child still fills, which comes to the same width while
/// the others keep theirs.
fn keep_width(node: Node, width: f32) -> Node {
    if node.width_fill { node } else { Node { width_constraint: Some(width), ..node } }
}

// ── Table ─────────────────────────────────────────────────────────────────────

/// One entry of a table's `cols`.
#[derive(Clone, Copy, PartialEq)]
enum ColUnit {
    /// A share of what the fixed and the percent columns leave.
    Fr(f32),
    Pt(f32),
    /// A percentage of the width of the table.
    Pct(f32),
    /// The width the cells of the column need.
    Auto,
}

fn col_units(spec: &str) -> Vec<ColUnit> {
    spec.split_whitespace().filter_map(|tok| {
        if tok == "auto" {
            Some(ColUnit::Auto)
        } else if let Some(s) = tok.strip_suffix("fr") {
            s.parse::<f32>().ok().map(ColUnit::Fr)
        } else if let Some(s) = tok.strip_suffix("pt") {
            s.parse::<f32>().ok().map(ColUnit::Pt)
        } else if let Some(s) = tok.strip_suffix('%') {
            s.parse::<f32>().ok().map(ColUnit::Pct)
        } else {
            tok.parse::<f32>().ok().map(ColUnit::Fr)
        }
    }).collect()
}

/// What is wrong with the `cols` of a table, if anything: an `auto` column takes the width its cells need and the
/// rest is shared among the `auto` columns, so there is no rest for an `fr` column to share.
pub(crate) fn check_cols(spec: &str) -> Result<(), String> {
    let units = col_units(spec);
    if units.contains(&ColUnit::Auto) && units.iter().any(|u| matches!(u, ColUnit::Fr(_))) {
        return Err(format!(
            "cols=\"{spec}\" has auto and fr columns: an auto column takes the width its cells need and the auto columns \
             share the rest, so use pt or % for the columns that are fixed"
        ));
    }
    Ok(())
}

/// Resolve a space-separated column-width spec into concrete pt values.
///
/// Units:
/// - `Nfr`  — fractional share of remaining width after fixed/percent columns.
/// - `Npt`  — absolute pt value.
/// - `N%`   — percentage of `avail_w`.
/// - `auto` — as `1fr` here, where there are no cells to measure: see `table_col_widths`.
///
/// Example: `"2fr 1fr 120pt 20%"` with `avail_w=400` →
///   fixed = 120 + 80 = 200, remaining = 200, total_fr = 3,
///   fr_unit = 200/3 ≈ 66.7 → [133.3, 66.7, 120, 80].
fn resolve_col_widths(spec: &str, avail_w: f32) -> Vec<f32> {
    if spec.is_empty() {
        return vec![avail_w];
    }
    let units: Vec<ColUnit> = col_units(spec).into_iter()
        .map(|u| if u == ColUnit::Auto { ColUnit::Fr(1.0) } else { u })
        .collect();
    if units.is_empty() {
        return vec![avail_w];
    }
    let fixed: f32 = units.iter().map(|u| match u {
        ColUnit::Pt(v)  => *v,
        ColUnit::Pct(v) => avail_w * v / 100.0,
        _               => 0.0,
    }).sum();
    let total_fr: f32 = units.iter().map(|u| match u {
        ColUnit::Fr(v) => *v,
        _              => 0.0,
    }).sum();
    let remaining = (avail_w - fixed).max(0.0);
    let fr_unit = if total_fr > 0.0 { remaining / total_fr } else { 0.0 };
    units.iter().map(|u| match u {
        ColUnit::Fr(v)  => (v * fr_unit).max(0.0),
        ColUnit::Pt(v)  => *v,
        ColUnit::Pct(v) => (avail_w * v / 100.0).max(0.0),
        ColUnit::Auto   => 0.0,
    }).collect()
}

/// How many columns a table's `cols` spec makes.
pub(crate) fn table_col_count(spec: &str) -> usize {
    col_units(spec).len().max(1)
}

/// The least a column can be narrowed to, which is its longest word, and the width it takes with each cell on one
/// line.
#[derive(Clone, Copy, Default)]
struct Extent {
    min: f32,
    max: f32,
}

/// The extents of the columns of `rows`. Those of an `auto` column are what its cells need, or those of every
/// column when `all` is set. A cell that spans columns gives the `auto` columns it covers what the others do not.
fn column_extents(rows: &[Node], units: &[ColUnit], avail_w: f32, gap: f32, all: bool) -> Vec<Extent> {
    let n = units.len();
    let grid = TableGrid::new(rows, n);
    let measured = |c: usize| all || units[c] == ColUnit::Auto;
    let mut extents = vec![Extent::default(); n];
    for (r, row) in rows.iter().enumerate() {
        for placed in grid.starting(r).iter().filter(|p| p.columns.len() == 1 && measured(p.columns.start)) {
            let cell = &row.children[placed.cell];
            let extent = &mut extents[placed.columns.start];
            extent.min = extent.min.max(measure_min_w(cell));
            extent.max = extent.max.max(measure_natural_w(cell));
        }
    }
    for (r, row) in rows.iter().enumerate() {
        for placed in grid.starting(r).iter().filter(|p| p.columns.len() > 1) {
            let sized: Vec<usize> = placed.columns.clone().filter(|&c| measured(c)).collect();
            if sized.is_empty() { continue; }
            let cell = &row.children[placed.cell];
            let others: f32 = placed.columns.clone().filter(|&c| !measured(c)).map(|c| match units[c] {
                ColUnit::Pt(v)  => v,
                ColUnit::Pct(v) => avail_w * v / 100.0,
                _               => 0.0,
            }).sum::<f32>() + gap * (placed.columns.len() - 1) as f32;
            let have_min = others + sized.iter().map(|&c| extents[c].min).sum::<f32>();
            let have_max = others + sized.iter().map(|&c| extents[c].max).sum::<f32>();
            let (need_min, need_max) = (measure_min_w(cell), measure_natural_w(cell));
            for &c in &sized {
                extents[c].min += (need_min - have_min).max(0.0) / sized.len() as f32;
                extents[c].max += (need_max - have_max).max(0.0) / sized.len() as f32;
            }
        }
    }
    extents
}

/// `mins` cut down to the level at which they add up to `total`: the longest are cut, and a shorter one keeps its
/// width. `total` is less than their sum.
fn cut_to_level(mins: &[f32], total: f32) -> Vec<f32> {
    let mut sorted = mins.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut taken = 0.0_f32;
    let mut level = 0.0_f32;
    for (i, min) in sorted.iter().enumerate() {
        let left = (sorted.len() - i) as f32;
        if taken + min * left >= total {
            level = (total - taken) / left;
            break;
        }
        taken += min;
    }
    mins.iter().map(|min| min.min(level)).collect()
}

/// The widths of the columns of a table of `avail_w`. A column with a width of its own has it, and the `auto`
/// columns share what is left, with a column of short cells keeping the width it needs:
/// - when every `auto` column fits on one line, each takes that, and the room that is left goes to them in
///   proportion to it;
/// - when they do not, each keeps at least its longest word and shares what is left in proportion to how much more
///   it could use;
/// - when not even the longest words fit, the longest are cut down to a level, and the shorter keep theirs, so a
///   name breaks and a number does not.
fn table_col_widths(rows: &[Node], spec: &str, avail_w: f32, gap: f32) -> Vec<f32> {
    let units = col_units(spec);
    if !units.contains(&ColUnit::Auto) {
        return resolve_col_widths(spec, avail_w);
    }
    let n = units.len();
    let extents = column_extents(rows, &units, avail_w, gap, false);
    let fixed: f32 = units.iter().map(|u| match u {
        ColUnit::Pt(v)  => *v,
        ColUnit::Pct(v) => avail_w * v / 100.0,
        _               => 0.0,
    }).sum();
    let rest = (avail_w - fixed - gap * (n - 1) as f32).max(0.0);
    let autos: Vec<usize> = (0..n).filter(|&c| units[c] == ColUnit::Auto).collect();
    let wanted: f32 = autos.iter().map(|&c| extents[c].max).sum();
    let least: f32 = autos.iter().map(|&c| extents[c].min).sum();

    let mut shares = vec![0.0_f32; n];
    if wanted <= rest {
        for &c in &autos {
            shares[c] = if wanted > 0.0 {
                extents[c].max + (rest - wanted) * extents[c].max / wanted
            } else {
                rest / autos.len() as f32
            };
        }
    } else if least >= rest {
        let mins: Vec<f32> = autos.iter().map(|&c| extents[c].min).collect();
        for (&c, width) in autos.iter().zip(cut_to_level(&mins, rest)) {
            shares[c] = width;
        }
    } else {
        let spread = wanted - least;
        for &c in &autos {
            shares[c] = extents[c].min + (rest - least) * (extents[c].max - extents[c].min) / spread;
        }
    }
    units.iter().enumerate().map(|(c, u)| match u {
        ColUnit::Pt(v)  => *v,
        ColUnit::Pct(v) => (avail_w * v / 100.0).max(0.0),
        ColUnit::Fr(_)  => 0.0,
        ColUnit::Auto   => shares[c],
    }).collect()
}

/// The width of a table that is measured as a part of a box around it: its columns and the gaps between them,
/// each column as wide as its cells need, or as its longest word when `min` is set, and its padding.
fn measure_table_w(node: &Node, min: bool) -> f32 {
    let units = col_units(&node.table_cols);
    if units.is_empty() { return 0.0; }
    let extents = column_extents(&node.children, &units, 0.0, node.gap, true);
    let columns: f32 = units.iter().zip(&extents).map(|(u, e)| match u {
        ColUnit::Pt(v) => *v,
        _              => if min { e.min } else { e.max },
    }).sum();
    let [_pt, pr, _pb, pl] = node.padding;
    match node.width_constraint {
        Some(w) => w,
        None    => columns + node.gap * (units.len() - 1) as f32 + pl + pr,
    }
}

/// The width of a cell over `columns`: the columns and the gaps between them.
fn span_width(col_widths: &[f32], columns: &Range<usize>, gap: f32) -> f32 {
    col_widths[columns.clone()].iter().sum::<f32>() + gap * (columns.len() - 1) as f32
}

/// The left edge of column `column`, in a table that starts at `x`.
fn column_x(col_widths: &[f32], column: usize, gap: f32, x: f32) -> f32 {
    col_widths[..column].iter().fold(x, |left, width| left + (width + gap))
}

/// The height of each row of a table, worked out one row after another. A cell that spans rows takes the room of
/// the rows it spans, and when its content is taller than they are together, the last of them grows by the
/// difference. The height of a row depends only on the rows up to it, so a table that breaks across pages measures
/// only the rows it places.
struct RowHeights<'a> {
    rows:       &'a [Node],
    grid:       &'a TableGrid,
    col_widths: &'a [f32],
    gap:        f32,
    heights:    Vec<f32>,
}

impl<'a> RowHeights<'a> {
    fn new(rows: &'a [Node], grid: &'a TableGrid, col_widths: &'a [f32], gap: f32) -> Self {
        RowHeights { rows, grid, col_widths, gap, heights: Vec::with_capacity(rows.len()) }
    }

    /// The height of row `i`, measuring the rows up to it that are not measured yet.
    fn get(&mut self, i: usize) -> f32 {
        while self.heights.len() <= i {
            let height = self.next_height();
            self.heights.push(height);
        }
        self.heights[i]
    }

    fn next_height(&self) -> f32 {
        let row = self.heights.len();
        let mut height = self.grid.starting(row).iter()
            .filter(|cell| cell.rows == 1)
            .map(|cell| self.cell_height(cell))
            .fold(0.0_f32, f32::max);
        for cell in self.grid.spans_ending(row) {
            let above = self.heights[cell.row..row].iter().sum::<f32>() + self.gap * (row - cell.row) as f32;
            height = height.max(self.cell_height(cell) - above);
        }
        height
    }

    /// The height a cell's content takes at the width of its columns.
    fn cell_height(&self, placed: &Placed) -> f32 {
        let cell = &self.rows[placed.row].children[placed.cell];
        layout_node(cell, 0.0, 0.0, span_width(self.col_widths, &placed.columns, self.gap), None).1
    }
}

/// The lines of a table row's bottom edge, as the `(left, right)` of each: the whole width of the table, which
/// starts at `x`, except where a cell in `spanned` columns goes on into the next row.
fn row_rules(spanned: &[Range<usize>], col_widths: &[f32], gap: f32, x: f32, width: f32) -> Vec<(f32, f32)> {
    let mut gaps: Vec<(f32, f32)> = spanned.iter()
        .map(|columns| {
            let left = column_x(col_widths, columns.start, gap, x);
            (left, left + span_width(col_widths, columns, gap))
        })
        .collect();
    gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
    // Two cells side by side leave no rule between them, not even the gap's worth.
    let mut merged: Vec<(f32, f32)> = Vec::with_capacity(gaps.len());
    for (left, right) in gaps {
        match merged.last_mut() {
            Some(last) if left - last.1 <= gap + 0.01 => last.1 = last.1.max(right),
            _ => merged.push((left, right)),
        }
    }
    let mut rules = Vec::with_capacity(merged.len() + 1);
    let mut from = x;
    for (left, right) in merged {
        if left > from + 0.01 {
            rules.push((from, left));
        }
        from = right;
    }
    if x + width > from + 0.01 {
        rules.push((from, x + width));
    }
    rules
}

fn layout_table(
    rows:      &[Node],
    x:         f32,
    y:         f32,
    avail_w:   f32,
    gap:       f32,
    cols_spec: &str,
    border:    Option<&(f32, String)>,
    style:     Option<&crate::parse::TableStyle>,
) -> (Vec<RenderNode>, f32) {
    if rows.is_empty() {
        return (vec![], 0.0);
    }

    let col_widths  = table_col_widths(rows, cols_spec, avail_w, gap);
    let n_cols      = col_widths.len();
    let grid        = TableGrid::new(rows, n_cols);

    let mut nodes:       Vec<RenderNode> = Vec::new();
    let mut row_y        = y;
    let mut data_row_idx = 0usize;

    // Pass 1: measure the height of each row (the tallest of its cells, and room for the cells that span into it).
    let mut measured = RowHeights::new(rows, &grid, &col_widths, gap);
    let row_heights: Vec<f32> = (0..rows.len()).map(|i| measured.get(i)).collect();
    // The top of each row, for the geometry of the border and of the cells that span rows.
    let mut row_ys: Vec<f32> = Vec::with_capacity(rows.len());

    for (i, row) in rows.iter().enumerate() {
        let is_thead = row.kind == NodeKind::TableHead;
        row_ys.push(row_y);
        let row_h = row_heights[i];

        // Determine row background (stripe overrides only when no explicit bg).
        let row_bg: Option<String> = if is_thead {
            row.background.clone()
        } else {
            let stripe_bg = if data_row_idx % 2 == 1 {
                style.and_then(|s| s.stripe.as_deref()).filter(|_| row.background.is_none()).map(str::to_string)
            } else {
                None
            };
            row.background.clone().or(stripe_bg)
        };

        // Emit row background box (must be below cells in draw order). A row in an optional content group has its
        // background in it with its cells.
        if let Some(bg) = row_bg {
            nodes.push(RenderNode::Box(RenderBox { tag: None, ocg: row.ocg,
                x, y: row_y, width: avail_w, height: row_h,
                fill: Some(bg),
                border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None,
                debug_self: false, children: vec![],
            }));
        }

        // Pass 2: lay out each cell at its column position, stretched to row_h. A cell that spans rows is laid out
        // once the last of its rows has its background, so that no background drawn later covers it.
        //
        // In a tagged section the cells of a row are drawn in a box tagged as the row, which the layout does not
        // draw otherwise, and a cell that spans rows in a box tagged as the row it starts in.
        let tags_rows = row.tag_id != 0;
        let mut row_cells = Vec::new();
        let mut cell_x   = x;
        let mut next_col = 0;
        for placed in grid.starting(i) {
            if placed.columns.start != next_col {
                // Columns were skipped: cells in the rows above hold them.
                cell_x = column_x(&col_widths, placed.columns.start, gap, x);
            }
            let cell_w = span_width(&col_widths, &placed.columns, gap);
            if placed.rows == 1 {
                let stretched = Node { height_mode: crate::parse::HeightMode::Full, ..row.children[placed.cell].clone() };
                let (mut rn, _) = layout_node(&stretched, cell_x, row_y, cell_w, Some(row_h));
                if tags_rows {
                    set_cell_column(&mut rn, placed.columns.start);
                }
                row_cells.push(rn);
            }
            cell_x  += cell_w + gap;
            next_col = placed.columns.end;
        }
        if tags_rows {
            nodes.push(tagged_box(row_tag(row), x, row_y, avail_w, row_h, row_cells));
        } else if row.ocg.is_some() {
            nodes.push(in_group(tagged_box(None, x, row_y, avail_w, row_h, row_cells), row.ocg, x, row_y, avail_w, row_h));
        } else {
            nodes.extend(row_cells);
        }
        for placed in grid.spans_ending(i) {
            let top       = row_ys[placed.row];
            let stretched = Node { height_mode: crate::parse::HeightMode::Full, ..rows[placed.row].children[placed.cell].clone() };
            let cell_x    = column_x(&col_widths, placed.columns.start, gap, x);
            let cell_w    = span_width(&col_widths, &placed.columns, gap);
            let (mut rn, _) = layout_node(&stretched, cell_x, top, cell_w, Some(row_y + row_h - top));
            if tags_rows {
                set_cell_column(&mut rn, placed.columns.start);
                rn = tagged_box(row_tag(&rows[placed.row]), cell_x, top, cell_w, row_y + row_h - top, vec![rn]);
            }
            nodes.push(in_group(rn, rows[placed.row].ocg, cell_x, top, cell_w, row_y + row_h - top));
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
        for (i, rh) in row_heights.iter().enumerate() {
            ry += rh;
            nodes.extend(rule_under_row(&grid, i, ry, &col_widths, gap, x, avail_w, (*bw, bclr)));
            ry += gap;
        }

        // Vertical column separators: one per adjacent column pair, per row, except inside a cell that spans both.
        for (i, (&ry_start, &rh)) in row_ys.iter().zip(row_heights.iter()).enumerate() {
            let covered = grid.wide_cells_in(i);
            let mut vx = x;
            for j in 0..n_cols {
                vx += col_widths[j];
                if j < n_cols - 1 {
                    if !covered.iter().any(|columns| columns.start <= j && j + 1 < columns.end) {
                        let sep_x = vx + gap / 2.0;
                        nodes.push(RenderNode::Line(RenderLine {
                            x1: sep_x, y1: ry_start,
                            x2: sep_x, y2: ry_start + rh,
                            color: bclr.clone(), thickness: *bw, dash: None,
                        }));
                    }
                    vx += gap;
                }
            }
        }
    }

    // The line under the header row, and the line under each of the other rows.
    if let Some(style) = style {
        let mut ry = y;
        for (i, rh) in row_heights.iter().enumerate() {
            ry += rh;
            let is_thead = rows[i].kind == NodeKind::TableHead;
            let line = if is_thead { style.head_border.as_ref().or(style.row_border.as_ref()) } else { style.row_border.as_ref() };
            if let Some((bw, bclr)) = line {
                nodes.extend(rule_under_row(&grid, i, ry, &col_widths, gap, x, avail_w, (*bw, bclr)));
            }
            ry += gap;
        }
    }

    (nodes, total_h)
}

/// The tag of a table's row in a tagged section: the header row, or a row of the body.
fn row_tag(row: &Node) -> Option<Box<Tag>> {
    let kind = if row.kind == NodeKind::TableHead { TagKind::TableHead } else { TagKind::TableRow };
    Some(Box::new(Tag { id: row.tag_id, kind }))
}

/// Note on a tagged cell the column it starts in.
fn set_cell_column(rendered: &mut RenderNode, column: usize) {
    if let RenderNode::Box(b) = rendered
        && let Some(tag) = b.tag.as_deref_mut()
        && let TagKind::TableCell { column: at, .. } = &mut tag.kind
    {
        *at = u16::try_from(column).unwrap_or(u16::MAX);
    }
}

/// The rule along the bottom edge of the row `i`, whose bottom is at `bottom`: one line across the table, or
/// pieces of it that stop where a cell spans on into the next row.
fn rule_under_row(
    grid:       &TableGrid,
    i:          usize,
    bottom:     f32,
    col_widths: &[f32],
    gap:        f32,
    x:          f32,
    width:      f32,
    line:       (f32, &String),
) -> Vec<RenderNode> {
    let (thickness, color) = line;
    let y = bottom - thickness / 2.0;
    let pieces = if grid.crosses(i) {
        row_rules(&grid.crossing_columns(i), col_widths, gap, x, width)
    } else {
        vec![(x, x + width)]
    };
    pieces.into_iter()
        .map(|(x1, x2)| RenderNode::Line(RenderLine { x1, y1: y, x2, y2: y, color: color.clone(), thickness, dash: None }))
        .collect()
}

/// Splits a table between rows: a row that does not fit goes to the next page whole, and so does the whole table
/// when only its header would stay behind. A run of rows with `keep="with-next"` moves on together with the row
/// that ends it when the break would fall inside it, and so do the rows that a cell spans.
fn split_table(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    let [pt, pr, pb, pl] = node.padding;
    let inner_w      = (avail_w     - pl - pr).max(0.0);
    let inner_target = (target_h    - pt - pb).max(0.0);
    let inner_full   = (full_page_h - pt - pb).max(0.0);
    let gap          = node.gap;

    let col_widths  = table_col_widths(&node.children, &node.table_cols, inner_w, gap);
    let has_thead   = node.children.first()
        .map_or(false, |c| c.kind == NodeKind::TableHead);

    let grid = TableGrid::new(&node.children, col_widths.len());
    // The height of each row, measured as far as the rows go that fit, the one that did not, and a run that moves on.
    let mut heights = RowHeights::new(&node.children, &grid, &col_widths, gap);

    let n_rows = node.children.len();
    let mut split_idx = 0usize;
    let mut chunk_h   = 0.0_f32;

    for i in 0..n_rows {
        let rh = heights.get(i);
        let g  = if i == 0 { 0.0 } else { gap };
        if chunk_h + g + rh > inner_target + 0.5 {
            break;
        }
        chunk_h   += g + rh;
        split_idx  = i + 1;
    }

    if split_idx == n_rows { return SplitOutcome::Atomic; }

    // A later page shows the header again above the rows, so a run has the page less the header to fit in.
    let header_h = if has_thead { heights.get(0) + gap } else { 0.0 };
    split_idx = keep_rows_with_next(&node.children, &grid, &mut heights, split_idx, gap, inner_full - header_h);
    split_idx = out_of_spans(&grid, split_idx, usize::from(has_thead), inner_target + 0.5 >= inner_full);
    if split_idx >= n_rows { return SplitOutcome::Atomic; }

    // Nothing useful fits: need at least thead (if present) + one data row.
    let min_useful = if has_thead { 2 } else { 1 };
    if split_idx < min_useful {
        return SplitOutcome::NothingFits(vec![node.clone()]);
    }

    // The columns of a table sized by its cells stay as wide on the next page as they are on this one.
    let table_cols = if col_units(&node.table_cols).contains(&ColUnit::Auto) {
        col_widths.iter().map(|w| format!("{w:.3}pt")).collect::<Vec<_>>().join(" ")
    } else {
        node.table_cols.clone()
    };
    let first = Node { children: node.children[..split_idx].to_vec(), table_cols: table_cols.clone(), ..node.clone() };

    // Carry thead onto the continuation page. Its texts were captured with the first copy, on the page the table
    // started on, so the copy captures nothing.
    let mut rest_children = node.children[split_idx..].to_vec();
    if has_thead {
        let mut header = node.children[0].clone();
        forget_captures(&mut header);
        rest_children.insert(0, header);
    }
    let rest = Node { children: rest_children, table_cols, ..node.clone() };
    SplitOutcome::First(first, vec![rest])
}

/// Moves a page break at `break_idx` out of a run of rows with `keep="with-next"`, back to the row the run starts
/// at, so the run and the row that ends it go on to the next page together. A break between rows that do not keep
/// stays. `room` is the height a page has for rows.
///
/// A row is also kept with the next one when a cell spans from one to the other, so a run can be made of both.
///
/// A run taller than `room` cannot stay on one page wherever it starts. The break stays where it was then, and the
/// run splits between its rows as if it had no keep, because moving it on would change nothing. `out_of_spans`
/// then moves it, if a cell would be cut.
fn keep_rows_with_next(
    rows:      &[Node],
    grid:      &TableGrid,
    heights:   &mut RowHeights,
    break_idx: usize,
    gap:       f32,
    room:      f32,
) -> usize {
    let kept = |i: usize| rows[i].keep == RowKeep::WithNext || grid.crosses(i);
    let start = (0..break_idx).rev().find(|&i| !kept(i)).map_or(0, |i| i + 1);
    if start == break_idx {
        return break_idx;
    }
    // The run ends at the first row that does not keep with the next, or at the last row of the table.
    let end = (break_idx..rows.len()).find(|&i| !kept(i)).unwrap_or(rows.len() - 1);

    let mut run_h = 0.0_f32;
    for i in start..=end {
        let rh = heights.get(i);
        run_h += if i == start { rh } else { gap + rh };
        if run_h > room + 0.5 {
            return break_idx;
        }
    }
    start
}

/// Moves a page break at `break_idx` out of a group of rows that a cell spans, which cannot be cut. The break goes
/// back to the row the group starts at, so the group moves on to the next page whole. `first_body` is the row after
/// the header, if the table has one.
///
/// A group that starts the table on a page of its own cannot go anywhere else, and the page clips what runs past its
/// end, as it does for a row taller than a page. The break goes to after the group then. When the table does not
/// start its page (`at_page_top` is false), the break goes back to its start whatever it is, and the table, or what
/// is before the group, stays on this one.
fn out_of_spans(grid: &TableGrid, break_idx: usize, first_body: usize, at_page_top: bool) -> usize {
    if break_idx == 0 || !grid.crosses(break_idx - 1) {
        return break_idx;
    }
    let start = (0..break_idx).rev().find(|&i| !grid.crosses(i)).map_or(0, |i| i + 1);
    if start > first_body || !at_page_top {
        return start;
    }
    (break_idx..).find(|&i| !grid.crosses(i)).map_or(break_idx, |i| i + 1)
}

/// Splits a text between lines, leaving at least `widows` lines for the next page and keeping at least
/// `orphans` on this one. When this page cannot do both, the text moves on to the next page, where it fits
/// whole or has more room. Only a text taller than a page, on a page too short to keep both rules, splits
/// where the lines fit, because moving it on would change nothing and it would run past the end of the page.
///
/// A text with `max-lines` has only the lines of its cap to split. The part on this page is drawn whole, and
/// the rest keeps what is left of the cap, so it still drops the text past it and ends in its ellipsis.
fn split_text(node: &Node, avail_w: f32, target_h: f32, full_page_h: f32) -> SplitOutcome {
    let line_h  = node.font_size * node.line_height;
    let pre     = (node.pre_tab > 0).then(|| pre_lines(node, avail_w));
    let lines   = if pre.is_none() { wrap_text_split(node, avail_w) } else { Vec::new() };
    let total   = pre.as_ref().map_or(lines.len(), Vec::len);
    let n_lines = node.max_lines.map_or(total, |max| total.min(max));
    if n_lines <= 1 || line_h <= 0.0 {
        return SplitOutcome::Atomic;
    }
    let room = ((target_h / line_h).floor().max(0.0)) as usize;
    if room >= n_lines {
        return SplitOutcome::Atomic;
    }
    if room == 0 {
        return SplitOutcome::NothingFits(vec![node.clone()]);
    }
    // Most lines this page may take and still leave `widows` for the next.
    let widow_limit = n_lines.saturating_sub(usize::from(node.widows));
    let orphans     = usize::from(node.orphans).max(1);
    let mut split_at = room.min(widow_limit);
    if split_at < orphans {
        let page_lines = ((full_page_h / line_h).floor().max(0.0)) as usize;
        if n_lines <= page_lines || page_lines.min(widow_limit) >= orphans {
            return SplitOutcome::NothingFits(vec![node.clone()]);
        }
        split_at = room;
    }
    let (first_runs, rest_runs) = match &pre {
        Some(lines) => (pre_runs(node, &lines[..split_at]), pre_runs(node, &lines[split_at..])),
        None => {
            let first_atoms: Vec<SplitAtom> = lines[..split_at].iter().flatten().cloned().collect();
            let rest_atoms:  Vec<SplitAtom> = lines[split_at..].iter().flatten().cloned().collect();
            (atoms_to_runs(&first_atoms), atoms_to_runs(&rest_atoms))
        }
    };
    let first = Node { text_runs: first_runs, max_lines: None, ellipsis: false, ..node.clone() };
    // The text is captured on the page where it starts, whole: `captured` stays with the first part.
    let rest  = Node {
        text_runs: rest_runs,
        max_lines: node.max_lines.map(|max| max - split_at),
        capture: node.capture.as_ref().map(|c| Box::new(Capture { key: None, ..(**c).clone() })),
        ..node.clone()
    };
    SplitOutcome::First(first, vec![rest])
}

// ── Text splitting helpers ────────────────────────────────────────────────────

/// How an atom goes on from the one before it, when a word or a styled run was broken to fit a line.
#[derive(Clone, Copy, PartialEq)]
enum Join {
    /// It is a word or a run of the text.
    No,
    /// It is the rest of a word that was cut at the edge of the line.
    Chars,
    /// It is the next word of a styled run that was broken at a space, so a space of the run's font leads it.
    Space,
}

#[derive(Clone)]
struct SplitAtom {
    text:           String,
    font_override:  Option<String>,
    color_override: Option<String>,
    href:           Option<String>,
    underline:      bool,
    strike:         bool,
    /// The tint behind the atom: a `<code-span>`'s.
    background:     Option<String>,
    /// Whether this atom needs a leading space when concatenated after the
    /// previous atom in the output. For the first atom in a line, ignore.
    leading_space:  bool,
    /// True if this atom came from a `<span>`; spans stay as single atoms
    /// (not word-sliced) to keep formatting intact.
    is_span:        bool,
    /// The note, on the atom that stands for the anchor of a footnote. Its `text` is the number it shows.
    footnote:       Option<Box<Footnote>>,
    /// Whether it goes on from the atom before it, a piece of a word or a styled run that was broken to fit.
    join:           Join,
}

impl SplitAtom {
    /// The metrics of the atom's font: its run's own, else `parent`, the text's.
    fn metrics<'a>(&'a self, parent: Metrics<'a>) -> Metrics<'a> {
        self.font_override.as_deref().map_or(parent, Metrics::of)
    }
}

/// A footnote's number is set at this share of the text's size, and its baseline raised by this share of it.
const SUP_SCALE: f32 = 0.62;
const SUP_RISE:  f32 = 0.33;

/// The size of a raised number in a line of `font_size`, and how far below the line's top its box starts.
fn sup_geometry(font_size: f32) -> (f32, f32) {
    let size = font_size * SUP_SCALE;
    (size, font_size - font_size * SUP_RISE - size)
}

/// The width of a word of a line, set in `font_size`, or at the size of a raised number when `sup`.
fn word_width(metrics: Metrics, text: &str, font_size: f32, sup: bool) -> f32 {
    metrics.text_width(text, if sup { font_size * SUP_SCALE } else { font_size })
}

/// Where each word of `text` is, between its spaces.
fn space_separated(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = 0;
    text.split(' ').map(move |word| {
        let range = start..start + word.len();
        start = range.end + 1;
        range
    })
}

/// The pieces of a word, or of a run of words, that is wider than a whole line: its words, and each word that is
/// still wider cut after the last character that fits, so nothing runs past the box. Each piece is where it is in
/// `text`, with whether a space leads it.
fn pieces_of_wide(metrics: Metrics, text: &str, font_size: f32, wrap_w: f32) -> Vec<(Range<usize>, bool)> {
    let fits = |s: &str| metrics.text_width(s, font_size) <= wrap_w + 0.01;
    let mut pieces = Vec::new();
    for (i, word) in space_separated(text).filter(|word| !word.is_empty()).enumerate() {
        let mut rest = word;
        let mut first = true;
        while !fits(&text[rest.clone()]) {
            // The longest start that fits, and at least one character, so the word always gets shorter.
            let mut end = rest.start;
            for (at, ch) in text[rest.clone()].char_indices() {
                let next = rest.start + at + ch.len_utf8();
                if end > rest.start && !fits(&text[rest.start..next]) { break; }
                end = next;
            }
            pieces.push((rest.start..end, i > 0 && first));
            rest = end..rest.end;
            first = false;
        }
        pieces.push((rest, i > 0 && first));
    }
    pieces
}

/// Break the words and the styled runs of `atoms` that are wider than a whole line into pieces that fit.
fn break_wide_atoms<'a>(atoms: Vec<Atom<'a>>, font_size: f32, wrap_w: f32) -> Vec<Atom<'a>> {
    let is_wide = |atom: &Atom| !atom.sup && atom.width > wrap_w + 0.01;
    if !atoms.iter().any(is_wide) {
        return atoms;
    }
    let mut out = Vec::with_capacity(atoms.len());
    for atom in atoms {
        if !is_wide(&atom) {
            out.push(atom);
            continue;
        }
        let metrics = Metrics::of(atom.font);
        for (i, (range, space)) in pieces_of_wide(metrics, &atom.text, font_size, wrap_w).into_iter().enumerate() {
            let text = atom.piece(range);
            let join = if i == 0 { Join::No } else if space { Join::Space } else { Join::Chars };
            let width = metrics.text_width(&text, font_size);
            out.push(Atom { text, join, width, space_before: i == 0 && atom.space_before, ..atom.clone() });
        }
    }
    out
}

/// `break_wide_atoms` for the atoms of a text that is split across pages, which must find the same lines.
fn break_wide_split_atoms(atoms: Vec<SplitAtom>, parent: Metrics, font_size: f32, wrap_w: f32) -> Vec<SplitAtom> {
    let mut out = Vec::with_capacity(atoms.len());
    for atom in atoms {
        let metrics = atom.metrics(parent);
        if atom.footnote.is_some() || metrics.text_width(&atom.text, font_size) <= wrap_w + 0.01 {
            out.push(atom);
            continue;
        }
        for (i, (range, space)) in pieces_of_wide(metrics, &atom.text, font_size, wrap_w).into_iter().enumerate() {
            let mut piece = atom.clone();
            piece.text = atom.text[range].to_string();
            if i > 0 {
                piece.leading_space = space;
                piece.join = if space { Join::Space } else { Join::Chars };
            }
            out.push(piece);
        }
    }
    out
}

/// Tokenize text_runs + wrap into lines. Mirrors the shape of `layout_text`'s
/// wrap step but preserves the overrides needed to rebuild `TextRun`s.
fn wrap_text_split(node: &Node, avail_w: f32) -> Vec<Vec<SplitAtom>> {
    let font_size      = node.font_size;
    let parent_metrics = Metrics::of(&node.font);
    let space_w        = parent_metrics.text_width(" ", font_size);

    let mut atoms: Vec<SplitAtom> = Vec::new();
    let mut pending_leading = false;
    for (ri, run) in node.text_runs.iter().enumerate() {
        let is_span = run.font.is_some() || run.color.is_some()
                   || run.href.is_some() || run.underline || run.strike || run.background.is_some();

        let run_leading = ri > 0 && run.leading_space;

        if let Some(note) = &run.footnote {
            atoms.push(SplitAtom {
                text: note.number.to_string(),
                font_override: None,
                color_override: None,
                href: None,
                underline: false,
                strike: false,
                background: None,
                leading_space: pending_leading || run_leading,
                is_span: true,
                footnote: Some(note.clone()),
                join: Join::No,
            });
            pending_leading = false;
        } else if is_span {
            if !run.text.is_empty() {
                atoms.push(SplitAtom {
                    text: run.text.clone(),
                    font_override: run.font.clone(),
                    color_override: run.color.clone(),
                    href: run.href.clone(),
                    underline: run.underline,
                    strike: run.strike,
                    background: run.background.clone(),
                    leading_space: pending_leading || run_leading,
                    is_span: true,
                    footnote: None,
                    join: Join::No,
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
                    background: None,
                    leading_space: if j == 0 { pending_leading || run_leading } else { true },
                    is_span: false,
                    footnote: None,
                    join: Join::No,
                });
                pending_leading = false;
            }
        }
    }

    if atoms.is_empty() { return vec![]; }

    // The same pieces as the lines of the text itself are made of.
    let atoms = break_wide_split_atoms(atoms, parent_metrics, font_size, avail_w);

    // Greedy wrap.
    let mut lines: Vec<Vec<SplitAtom>> = Vec::new();
    let mut cur_line: Vec<SplitAtom> = Vec::new();
    let mut cur_w: f32 = 0.0;

    let mut atoms: std::collections::VecDeque<SplitAtom> = atoms.into();
    while let Some(atom) = atoms.pop_front() {
        let metrics = atom.metrics(parent_metrics);
        let aw = word_width(metrics, &atom.text, font_size, atom.footnote.is_some());
        let gap = if cur_line.is_empty() {
            0.0
        } else {
            match atom.join {
                Join::Space => metrics.text_width(" ", font_size),
                Join::Chars => 0.0,
                Join::No    => if atom.leading_space { space_w } else { 0.0 },
            }
        };
        if cur_line.is_empty() || cur_w + gap + aw <= avail_w + 0.01 {
            cur_w += gap + aw;
            cur_line.push(atom);
        } else if node.flow_runs && atom.is_span && atom.footnote.is_none() && atom.text.contains(' ') {
            // The same as the lines of the text itself make: a styled run goes on in its next words.
            for (i, word) in atom.text.split(' ').enumerate().collect::<Vec<_>>().into_iter().rev() {
                let mut piece = atom.clone();
                piece.text = word.to_string();
                if i > 0 {
                    piece.leading_space = true;
                    piece.join = Join::Space;
                }
                atoms.push_front(piece);
            }
            continue;
        } else {
            // An atom with no space before it stays with the ones it is glued to, which go down to the next line
            // with it, as the lines of the text itself have it.
            let breakable = |a: &SplitAtom| a.join == Join::Space || (a.join == Join::No && a.leading_space);
            let carried = if atom.join == Join::No && !atom.leading_space {
                match cur_line.iter().rposition(|a| breakable(a)) {
                    Some(start) if start > 0 => cur_line.split_off(start),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            lines.push(std::mem::take(&mut cur_line));
            cur_w = aw;
            for prev in carried {
                cur_w += word_width(prev.metrics(parent_metrics), &prev.text, font_size, prev.footnote.is_some());
                cur_line.push(prev);
            }
            // Do NOT strip leading_space here. The gap calculation above already
            // handles "no space before the first atom on an empty line" via the
            // `cur_line.is_empty()` guard.  Stripping leading_space would cause
            // atoms_to_runs to silently merge this atom with the previous plain
            // run without a space when reconstructing split-fragment TextRuns
            // (e.g. "tokens," + "this" → "tokens,this").
            cur_line.push(atom);
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

        // The pieces of a styled run that was broken to fit a line are the run again.
        if atom.join != Join::No && atom.is_span && atom.footnote.is_none() {
            let is_run = |r: &TextRun| r.footnote.is_none() && (r.font.is_some() || r.color.is_some()
                || r.href.is_some() || r.underline || r.strike || r.background.is_some());
            if let Some(last) = runs.last_mut().filter(|r| is_run(r)) {
                if atom.join == Join::Space { last.text.push(' '); }
                last.text.push_str(&atom.text);
                first_emitted = true;
                continue;
            }
        }

        if let Some(note) = &atom.footnote {
            runs.push(TextRun {
                text: String::new(),
                leading_space: leading,
                font: None, color: None, href: None,
                underline: false, strike: false, bold: false, background: None,
                footnote: Some(note.clone()),
            });
        } else if atom.is_span {
            runs.push(TextRun {
                text: atom.text.clone(),
                leading_space: leading,
                font: atom.font_override.clone(),
                color: atom.color_override.clone(),
                href: atom.href.clone(),
                underline: atom.underline,
                strike: atom.strike,
                bold: false,
                background: atom.background.clone(),
                footnote: None,
            });
        } else {
            // Try to append to the last plain run.
            let can_append = runs.last().map_or(false, |r| {
                r.font.is_none() && r.color.is_none() && r.href.is_none()
                    && !r.underline && !r.strike && r.background.is_none() && r.footnote.is_none()
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
                    bold: false,
                    background: None,
                    footnote: None,
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
            let cw = own_width(node)
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
    let (node_w, node_x) = match own_width(node) {
        Some(w) => (w.min(avail_w), x),
        None => (avail_w, x),
    };

    let (rendered, h) = match node.kind {
        NodeKind::Divider => layout_divider(node, node_x, y, node_w, avail_h),
        NodeKind::Text => layout_text(node, node_x, y, node_w),
        NodeKind::Link => layout_link(node, node_x, y, node_w, avail_h),
        NodeKind::Img => layout_img(node, node_x, y, node_w),
        NodeKind::Shape => layout_shape(node, node_x, y, node_w),
        NodeKind::Barcode => layout_barcode(node, node_x, y, node_w),
        NodeKind::Field => layout_field(node, node_x, y, node_w),
        _ => layout_container(node, node_x, y, node_w, avail_h),
    };
    // In a tagged section, a node says what it is: a node with no tag id is one the layout made, as a note of a
    // debug node is.
    let rendered = match (node.tag_id != 0).then(|| tag_of(node)).flatten() {
        Some(kind) => with_tag(rendered, Tag { id: node.tag_id, kind }, node_x, y, node_w, h),
        None => rendered,
    };
    let rendered = if node.bookmarks.is_empty() {
        rendered
    } else {
        with_anchors(rendered, &node.bookmarks, node_x, y, node_w, h)
    };
    (in_group(rendered, node.ocg, node_x, y, node_w, h), h)
}

fn anchor(bookmark: &Bookmark, y: f32) -> RenderNode {
    RenderNode::Anchor(RenderAnchor {
        id:    bookmark.id.clone(),
        label: bookmark.label.clone(),
        level: bookmark.level,
        y,
    })
}

/// Put an anchor for each bookmark at the top of `rendered`, inside it, so that the anchors move with it when
/// its container shifts it. A leaf node is wrapped in a box that draws nothing.
fn with_anchors(rendered: RenderNode, bookmarks: &[Bookmark], x: f32, y: f32, width: f32, height: f32) -> RenderNode {
    let anchors = bookmarks.iter().map(|bookmark| anchor(bookmark, y));
    match rendered {
        RenderNode::Box(mut b) => {
            b.children.splice(0..0, anchors);
            RenderNode::Box(b)
        }
        RenderNode::Link(mut l) => {
            l.children.splice(0..0, anchors);
            RenderNode::Link(l)
        }
        leaf => RenderNode::Box(RenderBox { tag: None, ocg: None,
            x, y, width, height,
            fill: None, border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None, debug_self: false,
            children: anchors.chain(std::iter::once(leaf)).collect(),
        }),
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
        RenderNode::Box(RenderBox { tag: None, ocg: None,
            x,
            y,
            width: avail_w,
            height: outer_h,
            fill: node.background.clone(),
            border_width,
            border_color,
            radius: node.radius,
            opacity: node.opacity,
            shadow: node.shadow.as_deref().cloned(),
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
        NodeKind::Column => layout_column(
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.align, &node.justify,
        ),
        NodeKind::Row => layout_row(
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.justify, &node.align,
        ),
        NodeKind::Grid => layout_grid(
            &node.children, x, y, avail_w,
            node.gap, node.cols, node.col_width,
        ),
        NodeKind::Frame => layout_column(
            // Frame always centers its single child (horizontally via Align::Center,
            // vertically via Justify::Center when avail_h is known).
            &node.children, x, y, avail_w, avail_h,
            0.0, &Align::Center, &Justify::Center,
        ),
        NodeKind::Link => layout_column(
            &node.children, x, y, avail_w, avail_h,
            0.0, &Align::Stretch, &Justify::Start,
        ),
        NodeKind::Table => layout_table(
            &node.children, x, y, avail_w,
            node.gap, &node.table_cols, node.border.as_ref(), node.table_style.as_deref(),
        ),
        NodeKind::TableHead | NodeKind::TableRow | NodeKind::TableCell => layout_column(
            &node.children, x, y, avail_w, avail_h,
            node.gap, &node.align, &node.justify,
        ),
        _ => (vec![], 0.0),
    }
}

// ── Column ────────────────────────────────────────────────────────────────────

fn layout_column(
    children: &[Node],
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    gap: f32,
    align: &Align,
    justify: &Justify,
) -> (Vec<RenderNode>, f32) {
    layout_column_kept(children, Vec::new(), x, y, avail_w, avail_h, gap, align, justify)
}

/// `layout_column` with the trees `fill_page` kept for the children of a page's flow (`Kept`). A child found at
/// the place its tree was laid out at takes it, and any other is laid out as `layout_column` lays it out.
#[allow(clippy::too_many_arguments)]
fn layout_column_kept(
    children: &[Node],
    mut kept: Vec<Option<Kept>>,
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

    // While every child so far has a known height and the column starts its
    // children at `y`, a child's final place is known before it is measured:
    // an auto-height child is then laid out there once and its tree kept, so
    // pass 2 doesn't lay it out again (and each nesting level doesn't double
    // the work below it). With no avail_h, every justify starts at `y`.
    let starts_at_y = matches!(justify, Justify::Start) || avail_h.is_none();
    let mut placed: Vec<Option<RenderNode>> = Vec::new();
    placed.resize_with(n, || None);
    let mut place_y = Some(y);

    for (i, child) in children.iter().enumerate() {
        match &child.height_mode {
            HeightMode::Fill => {
                n_fill += 1;
                place_y = None;
            }
            HeightMode::Full => {
                // Deferred — needs avail_h
                place_y = None;
            }
            HeightMode::Fixed(h) => {
                sized[i] = Some(*h);
                sum_fixed += h;
            }
            // A link passes its avail_h on to its own children, so it is
            // measured without one and placed with one, as before.
            HeightMode::Auto if starts_at_y && place_y.is_some() && child.kind != NodeKind::Link => {
                let py = place_y.unwrap_or(y);
                let (rn, h) = match kept.get_mut(i).and_then(Option::take) {
                    // The same place to the last bit, so the tree is the one laid out here.
                    Some(k) if k.at == (x, py) => (k.tree, k.h),
                    _ => {
                        let (cw, cx) = column_child_place(child, x, avail_w, align);
                        layout_column_child(child, cx, py, cw, None, align)
                    }
                };
                placed[i] = Some(rn);
                sized[i] = Some(h);
                sum_fixed += h;
            }
            HeightMode::Auto => {
                let cw = resolve_child_w(child, avail_w, align);
                let (_, h) = layout_node(child, 0.0, 0.0, cw, None);
                sized[i] = Some(h);
                sum_fixed += h;
            }
        }
        if let (Some(py), Some(h)) = (place_y, sized[i]) {
            // The same sum as pass 2, so that the place is the same to the last bit.
            place_y = Some(py + (h + gap + 0.0));
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

    for (i, (child, kept)) in children.iter().zip(placed).enumerate() {
        let h = heights[i];
        let rn = match kept {
            Some(rn) => rn,
            None => {
                let (cw, cx) = column_child_place(child, x, avail_w, align);
                layout_column_child(child, cx, cur_y, cw, Some(h), align).0
            }
        };
        nodes.push(rn);
        cur_y += h + gap + extra_gap;
    }

    (nodes, total_h)
}

/// The width and x a column gives `child`.
fn column_child_place(child: &Node, x: f32, avail_w: f32, align: &Align) -> (f32, f32) {
    let cw = resolve_child_w(child, avail_w, align);
    // Frames with an explicit width constraint centre horizontally by default
    let effective_align =
        if matches!(child.kind, NodeKind::Frame) && child.width_constraint.is_some() {
            &Align::Center
        } else {
            align
        };
    (cw, cross_x(x, avail_w, cw, effective_align))
}

/// Lay out a child of a column at its place.
fn layout_column_child(
    child: &Node,
    cx: f32,
    y: f32,
    cw: f32,
    avail_h: Option<f32>,
    align: &Align,
) -> (RenderNode, f32) {
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
    match text_align_override {
        Some(ta) => layout_node(&Node { text_align: ta, ..child.clone() }, cx, y, cw, avail_h),
        None => layout_node(child, cx, y, cw, avail_h),
    }
}

// ── Row ───────────────────────────────────────────────────────────────────────
//
// Children sit side by side and wrap onto a new line when they run out of width.
// A child with width="fill" counts as no width when lines are broken, so it never
// wraps; it takes the width its line leaves, shared equally with the other fill
// children of the line. justify="between" spreads a line's children from edge to
// edge.
//
// A line with a fill child or justify="between" is laid out by
// `layout_row_line_spread`, and any other line by `layout_row_line`. They place
// children with different arithmetic, kept from the flank, split and cluster
// layouts that the row replaced, so documents converted from them render the same
// PDF byte for byte.

/// A child of a row on its line.
struct RowItem<'a> {
    node:     &'a Node,
    /// Its width, or `None` for a fill child.
    w:        Option<f32>,
    /// Where line breaking put it, from the row's left edge.
    layout_x: f32,
}

/// The width a row gives `child`, or `None` when it fills the rest of its line.
fn row_child_w(child: &Node, avail_w: f32) -> Option<f32> {
    if child.width_fill {
        return None;
    }
    Some(own_width(child).unwrap_or_else(|| measure_natural_w(child)).min(avail_w))
}

fn layout_row(
    children: &[Node],
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    gap: f32,
    justify: &Justify,
    align: &Align,
) -> (Vec<RenderNode>, f32) {
    if children.is_empty() {
        return (vec![], 0.0);
    }

    // ── Pass 1: break the children into lines ─────────────────────────────────
    let mut lines: Vec<Vec<RowItem>> = Vec::new();
    let mut current: Vec<RowItem> = Vec::new();
    let mut cur_x = x;

    for child in children {
        let w = row_child_w(child, avail_w);
        let cw = w.unwrap_or(0.0);
        if !current.is_empty() && cur_x + cw > x + avail_w + 0.01 {
            lines.push(std::mem::take(&mut current));
            cur_x = x;
        }
        current.push(RowItem { node: child, w, layout_x: cur_x });
        cur_x += cw + gap;
    }
    lines.push(current);

    // ── Pass 2: lay out each line ─────────────────────────────────────────────
    let mut nodes = Vec::new();
    let mut line_hs = Vec::with_capacity(lines.len());
    let mut cur_y = y;

    for line in &lines {
        let spread = *justify == Justify::Between || line.iter().any(|item| item.w.is_none());
        let line_h = if spread {
            layout_row_line_spread(line, x, cur_y, avail_w, avail_h, gap, justify, align, &mut nodes)
        } else {
            layout_row_line(line, x, y, cur_y, avail_w, gap, justify, align, &mut nodes)
        };
        line_hs.push(line_h);
        cur_y += line_h + gap;
    }

    let total_h = match line_hs.as_slice() {
        [line_h] => *line_h,
        _ => line_hs.iter().map(|h| h + gap).sum::<f32>() - gap,
    };
    (nodes, total_h)
}

/// Lay out a line of children of fixed width, justified start, center or end.
/// Each child is laid out at the top of the row where line breaking put it, then
/// moved to its line and its place in it.
#[allow(clippy::too_many_arguments)]
fn layout_row_line(
    line: &[RowItem],
    x: f32,
    row_y: f32,
    line_y: f32,
    avail_w: f32,
    gap: f32,
    justify: &Justify,
    align: &Align,
    nodes: &mut Vec<RenderNode>,
) -> f32 {
    let laid: Vec<(RenderNode, f32, f32, f32)> = line
        .iter()
        .map(|item| {
            let w = item.w.unwrap_or(0.0);
            let (rn, h) = layout_node(item.node, item.layout_x, row_y, w, None);
            (rn, w, h, item.layout_x)
        })
        .collect();

    let line_h = laid.iter().map(|(_, _, h, _)| *h).fold(0.0_f32, f32::max);
    let line_w: f32 = laid.iter().map(|(_, w, _, _)| *w).sum::<f32>()
        + gap * (laid.len().saturating_sub(1)) as f32;

    let remaining_w = (avail_w - line_w).max(0.0);
    let mut item_x = match justify {
        Justify::Start | Justify::Between => x,
        Justify::Center => x + remaining_w / 2.0,
        Justify::End    => x + remaining_w,
    };

    for (rn, w, h, layout_x) in laid {
        let dy_align = match align {
            Align::Start | Align::Stretch => 0.0,
            Align::Center => (line_h - h) / 2.0,
            Align::End    => line_h - h,
        };
        let dx = item_x - layout_x;
        let dy = (line_y + dy_align) - row_y;
        nodes.push(shift_x(shift_y(rn, dy), dx));
        item_x += w + gap;
    }

    line_h
}

/// Lay out a line with fill children, or justified between, at its final place.
/// The fill children share the width the others leave. Without them,
/// justify="between" puts the first child at the left edge, the last at the right
/// edge, and the others evenly between.
#[allow(clippy::too_many_arguments)]
fn layout_row_line_spread(
    line: &[RowItem],
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    gap: f32,
    justify: &Justify,
    align: &Align,
    nodes: &mut Vec<RenderNode>,
) -> f32 {
    let n = line.len();
    let fixed_w: f32 = line.iter().filter_map(|item| item.w).sum();
    let n_fill = line.iter().filter(|item| item.w.is_none()).count();
    let left_w = (avail_w - fixed_w - gap * (n - 1) as f32).max(0.0);
    let fill_w = if n_fill > 0 { left_w / n_fill as f32 } else { 0.0 };
    let between = n_fill == 0 && *justify == Justify::Between && n > 1;
    let extra_gap = if between { left_w / (n - 1) as f32 } else { 0.0 };

    let mut laid: Vec<(RenderNode, f32)> = Vec::with_capacity(n);
    let mut next_x = x;
    let mut width_before = 0.0_f32;
    let mut prev_end = x;

    for (i, item) in line.iter().enumerate() {
        let w = item.w.unwrap_or(fill_w);
        let item_x = match item.w {
            None => (x + width_before) + gap * i as f32,
            Some(_) if between && i == n - 1 => ((x + avail_w) - w).max(prev_end + gap),
            Some(_) => next_x,
        };
        laid.push(layout_node(item.node, item_x, y, w, avail_h));
        next_x = match item.w {
            None => (item_x + w) + gap,
            Some(_) => next_x + (w + gap) + extra_gap,
        };
        width_before += w;
        prev_end = item_x + w;
    }

    let line_h = laid.iter().fold(0.0_f32, |acc, (_, h)| acc.max(*h));
    nodes.extend(laid.into_iter().map(|(rn, h)| shift_y_cross(rn, h, line_h, align)));
    line_h
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
    // A link has no padding attribute; the padding is the indent a table of contents gives its entries.
    let [pt, pr, pb, pl] = node.padding;
    let (children, content_h) = layout_column(
        &node.children, x + pl, y + pt, (avail_w - pl - pr).max(0.0), avail_h.map(|h| (h - pt - pb).max(0.0)),
        node.gap, &node.align, &Justify::Start,
    );
    let height = match &node.height_mode {
        HeightMode::Fixed(h) => *h,
        HeightMode::Full | HeightMode::Fill => avail_h.unwrap_or(content_h + pt + pb),
        HeightMode::Auto => content_h + pt + pb,
    };
    (
        RenderNode::Link(RenderLink { url, x, y, width: avail_w, height, debug_self: node.debug, tag_id: node.tag_id, children }),
        height,
    )
}

// ── Image ─────────────────────────────────────────────────────────────────────

fn layout_img(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32) {
    let (w, h) = match node.img_aspect {
        // `width="fill"`: the slot is the width, and a `height` is a limit, as for any picture given both.
        Some(aspect) if node.width_fill && avail_w < f32::MAX => fit_within((aspect, 1.0), Some(avail_w), node.img_height_constraint),
        _ => {
            // width_constraint and img_height_constraint are pre-filled by prefill_image_sizes.
            // If somehow missing, fall back to a square box at available width.
            let pre_w = node.width_constraint.unwrap_or(avail_w);
            let pre_h = node.img_height_constraint.unwrap_or(pre_w);
            let w = pre_w.min(avail_w);
            // If the width was clamped, scale height proportionally to preserve aspect ratio.
            (w, if pre_w > 0.0 { pre_h * (w / pre_w) } else { pre_h })
        }
    };
    let name = node.image_name.clone().unwrap_or_default();
    (RenderNode::Image(RenderImage { tag: None, x, y, width: w, height: h, name, radius: node.radius }), h)
}

// ── Shape ─────────────────────────────────────────────────────────────────────

/// A `<shape>`: a box of the size it was given, as an image has, with the rect, circle or ellipse of a layer drawn
/// in it. Narrower room than its width narrows the shape and keeps its proportions, as it does an image. The
/// drawing sits in a box of its own, so that the box can fade it and the parent's alignment can move it.
fn layout_shape(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32) {
    let pre_w = node.width_constraint.unwrap_or(0.0);
    let pre_h = node.img_height_constraint.unwrap_or(0.0);
    let w = pre_w.min(avail_w);
    let h = if pre_w > 0.0 { pre_h * (w / pre_w) } else { pre_h };
    let drawing = node.shape().map(|shape| canvas::draw_flow_shape(shape, node.radius, x, y, w, h));
    let shape_box = RenderBox { tag: None, ocg: None,
        x, y, width: w, height: h,
        fill: None, border_width: 0.0, border_color: None, radius: 0.0,
        opacity: node.opacity, shadow: None,
        debug_self: node.debug,
        children: drawing.into_iter().collect(),
    };
    (RenderNode::Box(shape_box), h)
}

// ── Form field ────────────────────────────────────────────────────────────────

fn layout_field(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32) {
    let w = node.width_constraint.unwrap_or(avail_w).min(avail_w);
    let h = node.img_height_constraint.unwrap_or(20.0);
    let field = RenderField {
        x, y, width: w, height: h,
        kind:       node.field_kind.clone().unwrap_or(FieldKind::Text),
        name:       node.field_name.clone().unwrap_or_default(),
        value:      node.field_value.clone().unwrap_or_default(),
        label:      node.field_label.clone().unwrap_or_default(),
        options:    node.field_options.clone(),
        required:   node.field_required,
        readonly:   node.field_readonly,
        checked:    node.field_checked,
        max_len:    node.field_max_len,
        group:      node.field_group.clone(),
        action_url: node.field_action_url.clone(),
        background: node.background.clone(),
        border:     node.border.clone(),
    };
    (RenderNode::Field(field), h)
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
                    let bc = RenderBarcode { tag: None,
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
            let hrt_text = if node.barcode_hrt { Some(data.clone()) } else { None };

            match encode_code128(&data) {
                Ok(bars) => {
                    let (w, h) = fit_linear_barcode(node, &bars, avail_w);
                    let bc = RenderBarcode { tag: None,
                        x, y, width: w, height: h,
                        kind: RenderedBarcodeKind::Code128 { bars, hrt: hrt_text },
                        color, bg, debug_self: node.debug,
                    };
                    (RenderNode::Barcode(bc), h)
                }
                Err(msg) => fallback_barcode(x, y, node.width_constraint.unwrap_or(avail_w).min(avail_w), &msg),
            }
        }

        BarcodeType::Ean13 => {
            match encode_ean13(&data) {
                Ok(bars) => {
                    let (w, h) = fit_linear_barcode(node, &bars, avail_w);
                    let bc = RenderBarcode { tag: None,
                        x, y, width: w, height: h,
                        kind: RenderedBarcodeKind::Ean13 { bars, digits: data, hrt: node.barcode_hrt },
                        color, bg, debug_self: node.debug,
                    };
                    (RenderNode::Barcode(bc), h)
                }
                Err(msg) => fallback_barcode(x, y, node.width_constraint.unwrap_or(avail_w).min(avail_w), &msg),
            }
        }
    }
}

/// A Code 128 bar at its default width, in points. A Code 128 with no size of its own is as wide as its
/// bars, and the bars of one with a height only follow that height.
const CODE128_MODULE: f32 = 1.0;
/// The height of the bars of a Code 128 at its default size.
const CODE128_BAR_H: f32 = 40.0;
/// The size of an EAN-13 at the standard magnification of 1: bars 0.33mm wide and 22.85mm high.
const EAN13_MODULE: f32 = 0.33 * 72.0 / 25.4;
const EAN13_BAR_H: f32 = 22.85 * 72.0 / 25.4;
/// The digits of `hrt` are inside the barcode's height and take this much of it.
const HRT_H: f32 = 12.0;

/// The size of a Code 128 or an EAN-13 inside the `width` and `height` it is given, or its natural size at
/// the default bar width. Both given: they are a maximum, and the barcode keeps its proportions.
/// `bars` is the encoded barcode, its run lengths adding up to the number of modules across.
fn linear_barcode_size(
    node: &Node,
    bars: &[u8],
    width: Option<f32>,
    height: Option<f32>,
) -> (f32, f32) {
    let (module, bar_h) = match node.barcode_type {
        Some(BarcodeType::Ean13) => (EAN13_MODULE, EAN13_BAR_H),
        _                        => (CODE128_MODULE, CODE128_BAR_H),
    };
    let modules: u32 = bars.iter().map(|&run| run as u32).sum();
    let hrt_h = if node.barcode_hrt { HRT_H } else { 0.0 };
    fit_within((modules as f32 * module, bar_h + hrt_h), width, height)
}

/// The box a Code 128 or an EAN-13 takes in a slot `avail_w` wide: inside its `width` and `height`, which
/// `width="fill"` makes the slot, and not wider than the slot.
fn fit_linear_barcode(node: &Node, bars: &[u8], avail_w: f32) -> (f32, f32) {
    let width = if node.width_fill { Some(avail_w) } else { node.width_constraint };
    let (w, h) = linear_barcode_size(node, bars, width, node.img_height_constraint);
    if w > avail_w && w > 0.0 { (avail_w, h * avail_w / w) } else { (w, h) }
}

/// The width a node takes in the flow when it sets one itself: its `width`, and for a Code 128 or an EAN-13 the
/// width of the box it fits inside its sizes, which can be narrower than the `width`.
fn own_width(node: &Node) -> Option<f32> {
    barcode_flow_width(node).or(node.width_constraint)
}

/// The width a Code 128 or an EAN-13 takes in the flow when it isn't `width="fill"`: the box it keeps inside
/// its sizes, so that the parent's `align` places that box. `None` for any other node, or data that can't be encoded.
fn barcode_flow_width(node: &Node) -> Option<f32> {
    let bars = match (&node.barcode_type, &node.barcode_data) {
        (Some(BarcodeType::Code128), Some(data)) => encode_code128(data).ok()?,
        (Some(BarcodeType::Ean13),   Some(data)) => encode_ean13(data).ok()?,
        _                                        => return None,
    };
    Some(linear_barcode_size(node, &bars, node.width_constraint, node.img_height_constraint).0)
}

/// Return an empty box placeholder for an invalid/unencodable barcode.
fn fallback_barcode(x: f32, y: f32, w: f32, _reason: &str) -> (RenderNode, f32) {
    let h = 40.0_f32;
    let b = crate::render::RenderBox { tag: None, ocg: None,
        x, y, width: w, height: h,
        fill: Some("#eeeeee".to_string()),
        border_width: 0.5,
        border_color: Some("#ff0000".to_string()),
        radius: 0.0,
        opacity: 1.0,
        shadow: None,
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

// ── The widths of the codes of WinAnsiEncoding in the Adobe metrics ─────────────
//
// How far a viewer advances for each code from the space to 255 when a built-in font has no `/Widths`, in
// thousandths of the size: the AFM metrics of the Helvetica and Times families (an Oblique has the widths of its
// upright). WinAnsiEncoding draws a bullet for the codes it leaves empty, so they have the bullet's width.
// Courier is 600 for every code.
//
// Layout measures a character by the width of the code it is drawn with (`table_char_advance`), and a font
// written with a `/Widths`, as the one that draws comparison signs is (`pdf::write_sign_encoding`), takes these
// too, so that a viewer advances as layout measured.

#[rustfmt::skip]
static AFM_HELVETICA: [u16; 224] = [
    //  32- 47
     278,  278,  355,  556,  556,  889,  667,  191,  333,  333,  389,  584,  278,  333,  278,  278,
    //  48- 63
     556,  556,  556,  556,  556,  556,  556,  556,  556,  556,  278,  278,  584,  584,  584,  556,
    //  64- 79
    1015,  667,  667,  722,  722,  667,  611,  778,  722,  278,  500,  667,  556,  833,  722,  778,
    //  80- 95
     667,  778,  722,  667,  611,  722,  667,  944,  667,  667,  611,  278,  278,  278,  469,  556,
    //  96-111
     333,  556,  556,  500,  556,  556,  278,  556,  556,  222,  222,  500,  222,  833,  556,  556,
    // 112-127
     556,  556,  333,  500,  278,  556,  500,  722,  500,  500,  500,  334,  260,  334,  584,  350,
    // 128-143
     556,  350,  222,  556,  333, 1000,  556,  556,  333, 1000,  667,  333, 1000,  350,  611,  350,
    // 144-159
     350,  222,  222,  333,  333,  350,  556, 1000,  333, 1000,  500,  333,  944,  350,  500,  667,
    // 160-175
     278,  333,  556,  556,  556,  556,  260,  556,  333,  737,  370,  556,  584,  333,  737,  333,
    // 176-191
     400,  584,  333,  333,  333,  556,  537,  278,  333,  333,  365,  556,  834,  834,  834,  611,
    // 192-207
     667,  667,  667,  667,  667,  667, 1000,  722,  667,  667,  667,  667,  278,  278,  278,  278,
    // 208-223
     722,  722,  778,  778,  778,  778,  778,  584,  778,  722,  722,  722,  722,  667,  667,  611,
    // 224-239
     556,  556,  556,  556,  556,  556,  889,  500,  556,  556,  556,  556,  278,  278,  278,  278,
    // 240-255
     556,  556,  556,  556,  556,  556,  556,  584,  611,  556,  556,  556,  556,  500,  556,  500,
];

#[rustfmt::skip]
static AFM_HELVETICA_BOLD: [u16; 224] = [
    //  32- 47
     278,  333,  474,  556,  556,  889,  722,  238,  333,  333,  389,  584,  278,  333,  278,  278,
    //  48- 63
     556,  556,  556,  556,  556,  556,  556,  556,  556,  556,  333,  333,  584,  584,  584,  611,
    //  64- 79
     975,  722,  722,  722,  722,  667,  611,  778,  722,  278,  556,  722,  611,  833,  722,  778,
    //  80- 95
     667,  778,  722,  667,  611,  722,  667,  944,  667,  667,  611,  333,  278,  333,  584,  556,
    //  96-111
     333,  556,  611,  556,  611,  556,  333,  611,  611,  278,  278,  556,  278,  889,  611,  611,
    // 112-127
     611,  611,  389,  556,  333,  611,  556,  778,  556,  556,  500,  389,  280,  389,  584,  350,
    // 128-143
     556,  350,  278,  556,  500, 1000,  556,  556,  333, 1000,  667,  333, 1000,  350,  611,  350,
    // 144-159
     350,  278,  278,  500,  500,  350,  556, 1000,  333, 1000,  556,  333,  944,  350,  500,  667,
    // 160-175
     278,  333,  556,  556,  556,  556,  280,  556,  333,  737,  370,  556,  584,  333,  737,  333,
    // 176-191
     400,  584,  333,  333,  333,  611,  556,  278,  333,  333,  365,  556,  834,  834,  834,  611,
    // 192-207
     722,  722,  722,  722,  722,  722, 1000,  722,  667,  667,  667,  667,  278,  278,  278,  278,
    // 208-223
     722,  722,  778,  778,  778,  778,  778,  584,  778,  722,  722,  722,  722,  667,  667,  611,
    // 224-239
     556,  556,  556,  556,  556,  556,  889,  556,  556,  556,  556,  556,  278,  278,  278,  278,
    // 240-255
     611,  611,  611,  611,  611,  611,  611,  584,  611,  611,  611,  611,  611,  556,  611,  556,
];

#[rustfmt::skip]
static AFM_TIMES_ROMAN: [u16; 224] = [
    //  32- 47
     250,  333,  408,  500,  500,  833,  778,  180,  333,  333,  500,  564,  250,  333,  250,  278,
    //  48- 63
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  278,  278,  564,  564,  564,  444,
    //  64- 79
     921,  722,  667,  667,  722,  611,  556,  722,  722,  333,  389,  722,  611,  889,  722,  722,
    //  80- 95
     556,  722,  667,  556,  611,  722,  722,  944,  722,  722,  611,  333,  278,  333,  469,  500,
    //  96-111
     333,  444,  500,  444,  500,  444,  333,  500,  500,  278,  278,  500,  278,  778,  500,  500,
    // 112-127
     500,  500,  333,  389,  278,  500,  500,  722,  500,  500,  444,  480,  200,  480,  541,  350,
    // 128-143
     500,  350,  333,  500,  444, 1000,  500,  500,  333, 1000,  556,  333,  889,  350,  611,  350,
    // 144-159
     350,  333,  333,  444,  444,  350,  500, 1000,  333,  980,  389,  333,  722,  350,  444,  722,
    // 160-175
     250,  333,  500,  500,  500,  500,  200,  500,  333,  760,  276,  500,  564,  333,  760,  333,
    // 176-191
     400,  564,  300,  300,  333,  500,  453,  250,  333,  300,  310,  500,  750,  750,  750,  444,
    // 192-207
     722,  722,  722,  722,  722,  722,  889,  667,  611,  611,  611,  611,  333,  333,  333,  333,
    // 208-223
     722,  722,  722,  722,  722,  722,  722,  564,  722,  722,  722,  722,  722,  722,  556,  500,
    // 224-239
     444,  444,  444,  444,  444,  444,  667,  444,  444,  444,  444,  444,  278,  278,  278,  278,
    // 240-255
     500,  500,  500,  500,  500,  500,  500,  564,  500,  500,  500,  500,  500,  500,  500,  500,
];

#[rustfmt::skip]
static AFM_TIMES_BOLD: [u16; 224] = [
    //  32- 47
     250,  333,  555,  500,  500, 1000,  833,  278,  333,  333,  500,  570,  250,  333,  250,  278,
    //  48- 63
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  333,  333,  570,  570,  570,  500,
    //  64- 79
     930,  722,  667,  722,  722,  667,  611,  778,  778,  389,  500,  778,  667,  944,  722,  778,
    //  80- 95
     611,  778,  722,  556,  667,  722,  722, 1000,  722,  722,  667,  333,  278,  333,  581,  500,
    //  96-111
     333,  500,  556,  444,  556,  444,  333,  500,  556,  278,  333,  556,  278,  833,  556,  500,
    // 112-127
     556,  556,  444,  389,  333,  556,  500,  722,  500,  500,  444,  394,  220,  394,  520,  350,
    // 128-143
     500,  350,  333,  500,  500, 1000,  500,  500,  333, 1000,  556,  333, 1000,  350,  667,  350,
    // 144-159
     350,  333,  333,  500,  500,  350,  500, 1000,  333, 1000,  389,  333,  722,  350,  444,  722,
    // 160-175
     250,  333,  500,  500,  500,  500,  220,  500,  333,  747,  300,  500,  570,  333,  747,  333,
    // 176-191
     400,  570,  300,  300,  333,  556,  540,  250,  333,  300,  330,  500,  750,  750,  750,  500,
    // 192-207
     722,  722,  722,  722,  722,  722, 1000,  722,  667,  667,  667,  667,  389,  389,  389,  389,
    // 208-223
     722,  722,  778,  778,  778,  778,  778,  570,  778,  722,  722,  722,  722,  722,  611,  556,
    // 224-239
     500,  500,  500,  500,  500,  500,  722,  444,  444,  444,  444,  444,  278,  278,  278,  278,
    // 240-255
     500,  556,  500,  500,  500,  500,  500,  570,  500,  556,  556,  556,  556,  500,  556,  500,
];

#[rustfmt::skip]
static AFM_TIMES_ITALIC: [u16; 224] = [
    //  32- 47
     250,  333,  420,  500,  500,  833,  778,  214,  333,  333,  500,  675,  250,  333,  250,  278,
    //  48- 63
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  333,  333,  675,  675,  675,  500,
    //  64- 79
     920,  611,  611,  667,  722,  611,  611,  722,  722,  333,  444,  667,  556,  833,  667,  722,
    //  80- 95
     611,  722,  611,  500,  556,  722,  611,  833,  611,  556,  556,  389,  278,  389,  422,  500,
    //  96-111
     333,  500,  500,  444,  500,  444,  278,  500,  500,  278,  278,  444,  278,  722,  500,  500,
    // 112-127
     500,  500,  389,  389,  278,  500,  444,  667,  444,  444,  389,  400,  275,  400,  541,  350,
    // 128-143
     500,  350,  333,  500,  556,  889,  500,  500,  333, 1000,  500,  333,  944,  350,  556,  350,
    // 144-159
     350,  333,  333,  556,  556,  350,  500,  889,  333,  980,  389,  333,  667,  350,  389,  556,
    // 160-175
     250,  389,  500,  500,  500,  500,  275,  500,  333,  760,  276,  500,  675,  333,  760,  333,
    // 176-191
     400,  675,  300,  300,  333,  500,  523,  250,  333,  300,  310,  500,  750,  750,  750,  500,
    // 192-207
     611,  611,  611,  611,  611,  611,  889,  667,  611,  611,  611,  611,  333,  333,  333,  333,
    // 208-223
     722,  667,  722,  722,  722,  722,  722,  675,  722,  722,  722,  722,  722,  556,  611,  500,
    // 224-239
     500,  500,  500,  500,  500,  500,  667,  444,  444,  444,  444,  444,  278,  278,  278,  278,
    // 240-255
     500,  500,  500,  500,  500,  500,  500,  675,  500,  500,  500,  500,  500,  444,  500,  444,
];

#[rustfmt::skip]
static AFM_TIMES_BOLD_ITALIC: [u16; 224] = [
    //  32- 47
     250,  389,  555,  500,  500,  833,  778,  278,  333,  333,  500,  570,  250,  333,  250,  278,
    //  48- 63
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  333,  333,  570,  570,  570,  500,
    //  64- 79
     832,  667,  667,  667,  722,  667,  667,  722,  778,  389,  500,  667,  611,  889,  722,  722,
    //  80- 95
     611,  722,  667,  556,  611,  722,  667,  889,  667,  611,  611,  333,  278,  333,  570,  500,
    //  96-111
     333,  500,  500,  444,  500,  444,  333,  500,  556,  278,  278,  500,  278,  778,  556,  500,
    // 112-127
     500,  500,  389,  389,  278,  556,  444,  667,  500,  444,  389,  348,  220,  348,  570,  350,
    // 128-143
     500,  350,  333,  500,  500, 1000,  500,  500,  333, 1000,  556,  333,  944,  350,  611,  350,
    // 144-159
     350,  333,  333,  500,  500,  350,  500, 1000,  333, 1000,  389,  333,  722,  350,  389,  611,
    // 160-175
     250,  389,  500,  500,  500,  500,  220,  500,  333,  747,  266,  500,  606,  333,  747,  333,
    // 176-191
     400,  570,  300,  300,  333,  576,  500,  250,  333,  300,  300,  500,  750,  750,  750,  500,
    // 192-207
     667,  667,  667,  667,  667,  667,  944,  667,  667,  667,  667,  667,  389,  389,  389,  389,
    // 208-223
     722,  722,  722,  722,  722,  722,  722,  570,  722,  722,  722,  722,  722,  611,  611,  500,
    // 224-239
     500,  500,  500,  500,  500,  500,  722,  444,  444,  444,  444,  444,  278,  278,  278,  278,
    // 240-255
     500,  556,  500,  500,  500,  500,  500,  570,  500,  556,  556,  556,  556,  444,  500,  444,
];

/// How far a viewer advances for a code of WinAnsiEncoding in a built-in font, in thousandths of the size.
/// None for a code below the space, and for a font that is not of the Helvetica, Times or Courier families.
pub(crate) fn afm_code_advance(font: &str, code: u8) -> Option<u16> {
    let index = usize::from(code).checked_sub(32)?;
    if font.contains("Courier") {
        return Some(600);
    }
    afm_table(font).map(|table| table[index])
}

/// The advances of a built-in font of the Helvetica or Times families, for the codes from the space up. None for
/// any other font, Courier's included.
fn afm_table(font: &str) -> Option<&'static [u16; 224]> {
    Some(match font {
        "Helvetica" | "Helvetica-Oblique"          => &AFM_HELVETICA,
        "Helvetica-Bold" | "Helvetica-BoldOblique" => &AFM_HELVETICA_BOLD,
        "Times-Roman"                              => &AFM_TIMES_ROMAN,
        "Times-Bold"                               => &AFM_TIMES_BOLD,
        "Times-Italic"                             => &AFM_TIMES_ITALIC,
        "Times-BoldItalic"                         => &AFM_TIMES_BOLD_ITALIC,
        _ => return None,
    })
}

/// How far a character advances in the font of `table`, in thousandths of the size: the code of Windows-1252 it is
/// drawn with, else a comparison sign's own width, else the width of the character it is drawn as. The width of `?`
/// when none applies, since that is what `pdf::encode_latin1` draws then.
fn table_char_advance(table: &[u16; 224], c: char) -> u16 {
    if c.is_ascii() && c >= ' ' {
        return table[c as usize - 32];
    }
    table_char_fallback(table, c)
}

/// `table_char_advance` for a character outside printable ASCII, which few texts have.
#[cold]
#[inline(never)]
fn table_char_fallback(table: &[u16; 224], c: char) -> u16 {
    let in_table = |c: char| crate::pdf::win1252_byte(c)
        .and_then(|code| usize::from(code).checked_sub(32))
        .map(|index| table[index]);
    in_table(c)
        .or_else(|| crate::pdf::sign_advance(c))
        .or_else(|| crate::pdf::look_alike(c).and_then(in_table))
        .unwrap_or(table[usize::from(b'?') - 32])
}

/// How the text of a font is measured, found from its name. A loop over the words or the characters of a run
/// finds it once (`Metrics::of`), so the name is not tested for each of them.
#[derive(Clone, Copy)]
enum Metrics<'a> {
    /// The Courier family, or any font with Courier in its name: monospace, 600/1000 per glyph.
    Courier,
    /// A font with a width table of its own (`set_font_widths`), looked up by its name.
    Embedded(&'a str),
    /// A built-in font of the Helvetica or Times families, or Helvetica's widths for a font Lpdf has none of.
    Afm(&'static [u16; 224]),
}

impl<'a> Metrics<'a> {
    fn of(font: &'a str) -> Self {
        if font.contains("Courier") {
            return Metrics::Courier;
        }
        // The width table of an embedded font comes first (read from its bytes, or set_font_metrics).
        if FONT_WIDTHS.with(|fw| fw.borrow().contains_key(font)) {
            return Metrics::Embedded(font);
        }
        // Unknown font (undefined alias, unsupported builtin, or custom font with no
        // width table). Use Helvetica metrics as the fallback — this matches what
        // the renderer does (it also falls back to Helvetica for unknown fonts), so
        // layout and rendering stay in sync.
        afm_table(font).map_or_else(|| Metrics::of("Helvetica"), Metrics::Afm)
    }

    /// The advance width of `text` at `size`, in pt.
    fn text_width(self, text: &str, size: f32) -> f32 {
        match self {
            Metrics::Courier => text.chars().count() as f32 * size * 0.6,
            Metrics::Embedded(font) => FONT_WIDTHS
                .with(|fw| fw.borrow().get(font).map(|widths| widths.text_width(text, size)))
                // The tables were set again since the font was found: measure it as it is now.
                .unwrap_or_else(|| Metrics::of(font).text_width(text, size)),
            Metrics::Afm(table) => text.chars()
                .map(|c| table_char_advance(table, c) as f32 * size / 1000.0)
                .sum(),
        }
    }
}

/// Measure the advance width (in pt) of a string at the given font and size.
///
/// Uses per-character AFM glyph metrics for the standard PDF built-in fonts, 600/1000 per glyph for Courier,
/// and the width table of an embedded font. An unknown font is measured as Helvetica.
pub(crate) fn text_width(font: &str, text: &str, size: f32) -> f32 {
    Metrics::of(font).text_width(text, size)
}

/// Measure the natural (preferred/unconstrained) width of a node.
///
/// For text nodes this is the width of all runs placed on a single line.
/// For container nodes it is the widest child's natural width plus padding.
/// A row uses it for the width of a child without an explicit `width`.
fn measure_natural_w(node: &Node) -> f32 {
    match node.kind {
        NodeKind::Text if node.pre_tab > 0 => pre_natural_w(node),
        NodeKind::Text => {
            let size  = node.font_size;
            let space = text_width(&node.font, " ", size);
            let mut total = 0.0_f32;
            let mut first = true;
            for run in &node.text_runs {
                let metrics = Metrics::of(run.font.as_deref().unwrap_or(&node.font));
                if let Some(note) = &run.footnote {
                    total += word_width(metrics, &note.number.to_string(), size, true);
                    first = false;
                    continue;
                }
                for (j, word) in run.text.split_whitespace().enumerate() {
                    if !first || j > 0 { total += space; }
                    total += metrics.text_width(word, size);
                    first = false;
                }
            }
            total
        }
        NodeKind::Table => measure_table_w(node, false),
        NodeKind::Divider => node.thickness,
        NodeKind::Img => node.width_constraint.unwrap_or(100.0),
        NodeKind::Shape => node.width_constraint.unwrap_or(0.0),
        NodeKind::Barcode => own_width(node).unwrap_or(80.0),
        NodeKind::Field => node.width_constraint.unwrap_or(100.0),
        _ => {
            let [_pt, pr, _pb, pl] = node.padding;
            let inner = match node.kind {
                NodeKind::Column => {
                    // vertical: widest child
                    node.children.iter().map(measure_natural_w).fold(0.0_f32, f32::max)
                }
                NodeKind::Row => {
                    // treat as horizontal row (no wrap assumed for natural width); a fill
                    // child counts its own natural width, since it can't shrink to zero here
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

/// The width of the longest word of a text, or the width of the widest run that stays whole: what the text can be
/// narrowed to before something runs past it.
fn text_min_w(node: &Node) -> f32 {
    let size = node.font_size;
    let mut widest = 0.0_f32;
    for run in &node.text_runs {
        let metrics = Metrics::of(run.font.as_deref().unwrap_or(&node.font));
        if let Some(note) = &run.footnote {
            widest = widest.max(word_width(metrics, &note.number.to_string(), size, true));
            continue;
        }
        let is_span = run.font.is_some() || run.color.is_some()
                   || run.href.is_some() || run.underline || run.strike || run.background.is_some();
        // A styled run stays whole, unless the text fills a line with the first words of one.
        if is_span && !node.flow_runs && node.pre_tab == 0 {
            widest = widest.max(metrics.text_width(&run.text, size));
        } else {
            for word in run.text.split_whitespace() {
                widest = widest.max(metrics.text_width(word, size));
            }
        }
    }
    // A text breaks what is wider than its box, so it does not hold a column wider than this for a long name, which
    // would leave the columns beside it a word to a line.
    widest.min(MAX_WORD_EMS * size)
}

/// How many ems wide a word is taken to be, for the least width of a text.
const MAX_WORD_EMS: f32 = 12.0;

/// The least width of a node: what it can be narrowed to before something in it runs past it. A text breaks at its
/// spaces, so it is its longest word; a picture of a fixed width, a shape and a barcode do not narrow.
fn measure_min_w(node: &Node) -> f32 {
    match node.kind {
        NodeKind::Text => text_min_w(node),
        NodeKind::Img | NodeKind::Shape | NodeKind::Barcode | NodeKind::Field if node.width_fill => 0.0,
        NodeKind::Divider | NodeKind::Img | NodeKind::Shape | NodeKind::Barcode | NodeKind::Field => measure_natural_w(node),
        NodeKind::Table => measure_table_w(node, true),
        _ => {
            let [_pt, pr, _pb, pl] = node.padding;
            let inner = match node.kind {
                NodeKind::Row => {
                    let children_w: f32 = node.children.iter().map(measure_min_w).sum();
                    children_w + node.gap * (node.children.len().saturating_sub(1)) as f32
                }
                _ => node.children.iter().map(measure_min_w).fold(0.0_f32, f32::max),
            };
            match node.width_constraint {
                Some(w) => w,
                None    => inner + pl + pr,
            }
        }
    }
}

/// A word or a styled run of a text, with the style it is drawn in. It borrows its text and style from the text's
/// runs, so laying a text out allocates nothing for a word until it is drawn.
#[derive(Clone)]
struct Atom<'a> {
    text:      Cow<'a, str>,
    font:      &'a str,
    color:     &'a str,
    /// The styled run it is from, which says whether it is a link, underlined, struck or tinted, or for the number
    /// of a footnote the run of its anchor. None for a word of the text.
    span:      Option<&'a TextRun>,
    /// The number of a footnote, drawn small and raised.
    sup:       bool,
    /// Whether a space comes before it in the text.
    space_before: bool,
    /// Whether it goes on from the atom before it, a piece of a word or a run that was broken to fit a line, so a
    /// tint or a line under it reaches back across the gap.
    join:      Join,
    /// Its width on the line (`word_width`), measured once when it is made.
    width:     f32,
}

impl<'a> Atom<'a> {
    /// Measure the atom's text again, after it changed.
    fn measure(&mut self, font_size: f32) {
        self.width = word_width(Metrics::of(self.font), &self.text, font_size, self.sup);
    }

    /// A piece of the atom's text, `range` of it, borrowed as the text is.
    fn piece(&self, range: Range<usize>) -> Cow<'a, str> {
        match &self.text {
            Cow::Borrowed(text) => Cow::Borrowed(&text[range]),
            Cow::Owned(text) => Cow::Owned(text[range].to_string()),
        }
    }

    fn href(&self) -> Option<&'a str> {
        self.span.and_then(|run| run.href.as_deref())
    }

    fn background(&self) -> Option<&'a str> {
        self.span.and_then(|run| run.background.as_deref())
    }

    fn underline(&self) -> bool {
        self.span.is_some_and(|run| run.underline)
    }

    fn strike(&self) -> bool {
        self.span.is_some_and(|run| run.strike)
    }
}

/// A line of a text: its atoms, each with the gap before it.
type TextLine<'a> = Vec<(Atom<'a>, f32)>;

/// How a text is cut to fit. A text in the flow wraps at its width and keeps every line. A `text-at` has a
/// different width to wrap at than to align in (it stays on one line unless `max-lines` says otherwise), keeps
/// at most `max_lines` lines, and can end the last in dots.
struct TextFit {
    /// A word that would end past this starts a new line.
    wrap_w:    f32,
    max_lines: Option<usize>,
    ellipsis:  bool,
}

/// The ending of a line cut short by its width or by `max-lines`.
const ELLIPSIS: &str = "...";

/// How far a gap of a justified line may be widened, in spaces. A line that would need more stays
/// left-aligned: two long words are not spread across the width, since there is no hyphenation to help.
const MAX_JUSTIFY_STRETCH: f32 = 2.0;

/// The spaces inside a word or a styled run of a line. They are gaps of the line like the ones between atoms.
fn atom_spaces(atom: &Atom) -> usize {
    if atom.sup { 0 } else { atom.text.matches(' ').count() }
}

/// What a justified `line` adds to each of its gaps to end at `avail_w`: the spare width, shared evenly by the
/// gaps between its words and inside its spans. 0.0 for a line with no gap, with no room to spare, or that
/// would need more than `MAX_JUSTIFY_STRETCH` spaces of it.
fn justify_gap(line: &TextLine, avail_w: f32, space_w: f32) -> f32 {
    let gaps: usize = line.iter().map(|(atom, gap)| usize::from(*gap > 0.0) + atom_spaces(atom)).sum();
    if gaps == 0 { return 0.0; }
    let per_gap = (avail_w - text_line_width(line)) / gaps as f32;
    if per_gap <= 0.01 || per_gap > MAX_JUSTIFY_STRETCH * space_w { 0.0 } else { per_gap }
}

fn text_line_width(line: &TextLine) -> f32 {
    line.iter().map(|(atom, gap)| gap + atom.width).sum()
}

/// Shorten `line` until it and the dots after it fit in `limit`, and end it in them. The last characters go
/// first, and a run with none left goes too.
fn end_in_ellipsis(line: &mut TextLine, limit: f32, font_size: f32) {
    while let Some((last, _)) = line.last() {
        let dots = text_width(last.font, ELLIPSIS, font_size);
        if text_line_width(line) + dots <= limit + 0.01 {
            if let Some((last, _)) = line.last_mut() {
                last.text.to_mut().push_str(ELLIPSIS);
                last.measure(font_size);
            }
            return;
        }
        let emptied = match line.last_mut() {
            Some((last, _)) => { last.text.to_mut().pop(); last.measure(font_size); last.text.is_empty() }
            None => false,
        };
        if emptied { line.pop(); }
    }
}

/// Lay out a text of the flow. Without `max-lines` it is never cut, so `ellipsis` has nothing to end: not even
/// a word wider than the box, which the shared code would end in dots.
fn layout_text(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32) {
    let fit = TextFit { wrap_w: avail_w, max_lines: node.max_lines, ellipsis: node.ellipsis && node.max_lines.is_some() };
    let (rendered, height, _) = layout_text_fit(node, x, y, avail_w, &fit);
    (rendered, height)
}

/// Lay out a text in a box `avail_w` wide, which is what its lines align in. Returns the box, its height, and
/// whether a line it keeps is wider than the box, which the caller cuts off.
fn layout_text_fit(node: &Node, x: f32, y: f32, avail_w: f32, fit: &TextFit) -> (RenderNode, f32, bool) {
    if node.pre_tab > 0 {
        return layout_pre_text(node, x, y, avail_w);
    }
    let font_size    = node.font_size;
    let line_height  = font_size * node.line_height;
    let parent_font  = node.font.as_str();
    let parent_color = node.text_color.as_deref().unwrap_or("#1a1a1a");
    let space_w      = text_width(parent_font, " ", font_size);

    // ── Build the atoms, each with whether a space comes before it ────────────
    let mut atoms: Vec<Atom> = Vec::new();
    // A space with no word after it still makes the text one to lay out, as an empty one is not.
    let mut any_space    = false;
    let mut space_before = false;
    for (ri, run) in node.text_runs.iter().enumerate() {
        let is_span = run.font.is_some() || run.color.is_some()
                   || run.href.is_some() || run.underline || run.strike || run.background.is_some();
        let font    = run.font.as_deref().unwrap_or(parent_font);
        let metrics = Metrics::of(font);
        let color   = run.color.as_deref().unwrap_or(parent_color);

        if ri > 0 && run.leading_space { space_before = true; any_space = true; }

        if let Some(note) = &run.footnote {
            let text = note.number.to_string();
            let width = word_width(metrics, &text, font_size, true);
            atoms.push(Atom {
                text: Cow::Owned(text), font, color, span: Some(run), sup: true, space_before, join: Join::No, width,
            });
            space_before = false;
        } else if is_span {
            if !run.text.is_empty() {
                atoms.push(Atom {
                    text: Cow::Borrowed(&run.text), font, color, span: Some(run), sup: false, space_before,
                    join: Join::No, width: metrics.text_width(&run.text, font_size),
                });
                space_before = false;
            }
        } else {
            for (j, word) in run.text.split_whitespace().enumerate() {
                if j > 0 { space_before = true; }
                atoms.push(Atom {
                    text: Cow::Borrowed(word), font, color, span: None, sup: false, space_before, join: Join::No,
                    width: metrics.text_width(word, font_size),
                });
                space_before = false;
            }
        }
    }

    if atoms.is_empty() && !any_space {
        let reserved = node.reserve_lines() as f32 * line_height;
        return (RenderNode::Box(RenderBox { tag: None, ocg: None,
            x, y, width: 0.0, height: reserved, fill: None,
            border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None, debug_self: false, children: vec![],
        }), reserved, false);
    }

    // A word, or a styled run, wider than the box would run past it: it is broken to fit.
    let atoms = break_wide_atoms(atoms, font_size, fit.wrap_w);

    // ── Word-wrap: build lines of (Atom, gap_before_pt) ──────────────────────
    let mut lines: Vec<TextLine> = Vec::new();
    let mut cur_line: TextLine   = Vec::new();
    let mut cur_line_w    = 0.0_f32;

    let mut atoms: std::collections::VecDeque<Atom> = atoms.into();
    while let Some(atom) = atoms.pop_front() {
        let aw  = atom.width;
        let gap = if cur_line.is_empty() {
            0.0
        } else {
            match atom.join {
                Join::Space => text_width(atom.font, " ", font_size),
                Join::Chars => 0.0,
                Join::No    => if atom.space_before { space_w } else { 0.0 },
            }
        };
        if cur_line.is_empty() || cur_line_w + gap + aw <= fit.wrap_w + 0.01 {
            cur_line_w += gap + aw;
            cur_line.push((atom, gap));
        } else if node.flow_runs && !atom.sup && atom.text.contains(' ') {
            // A styled run of several words that does not fit what is left of the line goes on in the next
            // words, so the line is filled and the run is not moved down whole.
            let words: Vec<Range<usize>> = space_separated(&atom.text).collect();
            let metrics = Metrics::of(atom.font);
            for (i, range) in words.into_iter().enumerate().rev() {
                let text = atom.piece(range);
                let width = metrics.text_width(&text, font_size);
                let (join, space_before) = if i == 0 { (atom.join, atom.space_before) } else { (Join::Space, false) };
                atoms.push_front(Atom { text, join, width, space_before, ..atom.clone() });
            }
        } else {
            // An atom with no space before it stays with the ones it is glued to, which go down to the next
            // line with it: the number of a footnote, or a bracket and the run of code after it.
            let glued = atom.join == Join::No && !atom.space_before;
            let carried = if glued {
                match cur_line.iter().rposition(|(_, gap)| *gap > 0.0) {
                    Some(start) => cur_line.split_off(start),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            };
            lines.push(std::mem::take(&mut cur_line));
            cur_line_w = aw;
            for (prev, _) in carried {
                cur_line_w += prev.width;
                cur_line.push((prev, 0.0));
            }
            cur_line.push((atom, 0.0));
        }
    }
    if !cur_line.is_empty() { lines.push(cur_line); }

    // Only whole lines are kept. A line that is still too wide, because it is cut short or is a word longer
    // than the box, ends in dots when the text asks for them.
    let cut = fit.max_lines.is_some_and(|max| lines.len() > max);
    if let Some(max) = fit.max_lines { lines.truncate(max); }
    if fit.ellipsis {
        if let Some(last) = lines.last_mut() {
            if cut || text_line_width(last) > avail_w + 0.01 {
                end_in_ellipsis(last, avail_w, font_size);
            }
        }
    }
    let overflow = fit.max_lines.is_some()
        && lines.iter().any(|line| text_line_width(line) > avail_w + 0.01);

    let total_h = lines.len().max(node.reserve_lines()) as f32 * line_height;
    let mut all_nodes: Vec<RenderNode> = Vec::new();

    let align_str = match node.text_align {
        TextAlign::Left    => "left",
        TextAlign::Center  => "center",
        TextAlign::Right   => "right",
        TextAlign::Justify => "justify",
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
            TextAlign::Left    => x,
            TextAlign::Center  => x + avail_w / 2.0,
            TextAlign::Right   => x + avail_w,
            TextAlign::Justify => x,
        };

        let mut cur_x = line_anchor_x;

        // For justify, the last line is left-aligned (not stretched).
        let is_last_line  = li + 1 == lines.len();
        // Every word of the line is placed here, so a span moves with its neighbours.
        let word_gap = if align_str == "justify" && !is_last_line { justify_gap(line, avail_w, space_w) } else { 0.0 };
        let eff_align_str = match (align_str, word_gap > 0.0) {
            ("justify", false) => "left",
            _                  => align_str,
        };

        // Merge consecutive plain (no href/underline/strike) atoms that share the
        // same font and color into a single RenderText.  This lets pdf-lib render
        // each group with its real glyph metrics instead of accumulating the error
        // from the char-width approximation word-by-word.
        let mut grp_x    = 0.0_f32;
        let mut grp_font = "";
        let mut grp_clr  = "";
        let mut grp_text = String::new();

        for (atom, gap) in line {
            cur_x += gap;
            if *gap > 0.0 { cur_x += word_gap; }
            let inner_gap = word_gap * atom_spaces(atom) as f32;
            let aw = atom.width + inner_gap;

            let plain = atom.href().is_none() && !atom.underline() && !atom.strike() && !atom.sup && atom.background().is_none();

            if plain {
                // If font or color changed, flush the current group first.
                if !grp_text.is_empty() && (atom.font != grp_font || atom.color != grp_clr) {
                    all_nodes.push(RenderNode::Text(RenderText {
                        x: grp_x, y: line_y,
                        content: std::mem::take(&mut grp_text),
                        font: grp_font.to_string(), size: font_size, color: grp_clr.to_string(),
                        text_align: eff_align_str.to_string(), line_width: avail_w, word_gap,
                    }));
                }
                if grp_text.is_empty() {
                    grp_x    = cur_x;
                    grp_font = atom.font;
                    grp_clr  = atom.color;
                    grp_text = atom.text.to_string();
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
                        font: grp_font.to_string(), size: font_size, color: grp_clr.to_string(),
                        text_align: eff_align_str.to_string(), line_width: avail_w, word_gap,
                    }));
                }

                let (atom_size, atom_y) = if atom.sup {
                    let (size, dy) = sup_geometry(font_size);
                    (size, line_y + dy)
                } else {
                    (font_size, line_y)
                };
                // The tint and the lines of a run broken across a gap reach back over it.
                let lead = if atom.join != Join::No && *gap > 0.0 { *gap + word_gap } else { 0.0 };
                let mut seg = Vec::new();
                if let Some(tint) = atom.background() {
                    seg.push(tint_behind(tint, cur_x - lead, line_y, aw + lead, line_height));
                }
                if let Some(note) = atom.span.and_then(|run| run.footnote.as_deref()) {
                    seg.push(RenderNode::FootnoteMark(RenderFootnoteMark { index: note.index, y: atom_y }));
                }
                seg.push(RenderNode::Text(RenderText {
                    x: cur_x, y: atom_y,
                    content: atom.text.to_string(), font: atom.font.to_string(),
                    size: atom_size, color: atom.color.to_string(),
                    // Decorated atoms (spans) always use their computed cur_x as a
                    // left-edge anchor; alignment has already been applied at the line level.
                    // The gaps inside one are widened like the rest of the line.
                    text_align: if inner_gap > 0.0 { "justify" } else { "left" }.to_string(),
                    line_width: 0.0, word_gap: if inner_gap > 0.0 { word_gap } else { 0.0 },
                }));

                if atom.underline() {
                    seg.push(RenderNode::Line(RenderLine {
                        x1: cur_x - lead, y1: line_y + font_size,
                        x2: cur_x + aw, y2: line_y + font_size,
                        color: atom.color.to_string(), thickness: 0.75,
                        dash: None,
                    }));
                }
                if atom.strike() {
                    let sy = line_y + font_size * 0.5;
                    seg.push(RenderNode::Line(RenderLine {
                        x1: cur_x - lead, y1: sy, x2: cur_x + aw, y2: sy,
                        color: atom.color.to_string(), thickness: 0.75,
                        dash: None,
                    }));
                }

                match atom.href() {
                    Some(href) => all_nodes.push(RenderNode::Link(RenderLink {
                        url: href.to_string(), x: cur_x - lead, y: line_y,
                        width: aw + lead, height: line_height, debug_self: false, tag_id: 0, children: seg,
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
                font: grp_font.to_string(), size: font_size, color: grp_clr.to_string(),
                text_align: eff_align_str.to_string(), line_width: avail_w, word_gap,
            }));
        }
    }

    (
        RenderNode::Box(RenderBox { tag: None, ocg: None,
            x, y, width: avail_w, height: total_h,
            fill: None, border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None,
            debug_self: node.debug,
            children: all_nodes,
        }),
        total_h,
        overflow,
    )
}

// ── Preformatted text ─────────────────────────────────────────────────────────

/// A rectangle of `tint` behind a run of a line.
fn tint_behind(tint: &str, x: f32, y: f32, width: f32, height: f32) -> RenderNode {
    RenderNode::Box(RenderBox { tag: None, ocg: None,
        x, y, width, height, fill: Some(tint.to_string()),
        border_width: 0.0, border_color: None, radius: 0.0, opacity: 1.0, shadow: None, debug_self: false,
        children: vec![],
    })
}

/// A stretch of one line of preformatted text. It comes from one run, so it has one style.
struct PreSeg {
    text: String,
    run:  usize,
}

/// A character of preformatted text, with the index of the run it came from and its width in the font of that run.
type PreCell = (char, usize, f32);

/// The characters of a preformatted text, a line at a time. A tab becomes the spaces up to the next multiple of
/// `pre_tab` columns. A newline ends a line, and the one that ends the text starts no line after it.
fn pre_cells(node: &Node) -> Vec<Vec<PreCell>> {
    if node.text_runs.iter().all(|run| run.text.is_empty()) {
        return Vec::new();
    }
    let tab = usize::from(node.pre_tab.max(1));
    let mut lines: Vec<Vec<PreCell>> = vec![Vec::new()];
    for (ri, run) in node.text_runs.iter().enumerate() {
        let metrics = Metrics::of(run.font.as_deref().unwrap_or(&node.font));
        for c in run.text.chars() {
            let line = lines.last_mut().expect("there is always a line");
            match c {
                '\n' => lines.push(Vec::new()),
                '\r' => {}
                '\t' => {
                    let fill = tab - line.len() % tab;
                    line.extend(std::iter::repeat_n((' ', ri, metrics.text_width(" ", node.font_size)), fill));
                }
                c => line.push((c, ri, metrics.text_width(c.encode_utf8(&mut [0; 4]), node.font_size))),
            }
        }
    }
    if lines.last().is_some_and(Vec::is_empty) {
        lines.pop();
    }
    lines
}

/// End the line being built, without the spaces at its end: they are the break and are not drawn. A line that
/// holds only spaces is not ended, since a break never makes a blank line.
fn end_pre_line(out: &mut Vec<Vec<PreCell>>, line: &mut Vec<PreCell>, line_w: &mut f32) {
    while line.last().is_some_and(|cell| cell.0 == ' ') {
        line.pop();
    }
    if !line.is_empty() {
        out.push(std::mem::take(line));
    }
    *line_w = 0.0;
}

/// Break a line of a preformatted text where it passes `wrap_w`. It breaks at spaces, and a word that does not
/// fit on a line of its own is broken at the last character that fits.
fn wrap_pre_line(cells: &[PreCell], wrap_w: f32) -> Vec<Vec<PreCell>> {
    let limit = wrap_w + 0.01;
    let mut out:  Vec<Vec<PreCell>> = Vec::new();
    let mut line: Vec<PreCell>      = Vec::new();
    let mut line_w = 0.0_f32;
    let mut rest = cells;
    while let Some(first) = rest.first() {
        let space = first.0 == ' ';
        let len = rest.iter().take_while(|cell| (cell.0 == ' ') == space).count();
        let (token, tail) = rest.split_at(len);
        rest = tail;
        let token_w: f32 = token.iter().map(|cell| cell.2).sum();
        if line_w + token_w <= limit {
            line.extend_from_slice(token);
            line_w += token_w;
        } else if space {
            end_pre_line(&mut out, &mut line, &mut line_w);
        } else if token_w <= limit && !line.is_empty() {
            end_pre_line(&mut out, &mut line, &mut line_w);
            line.extend_from_slice(token);
            line_w = token_w;
        } else {
            for cell in token {
                if !line.is_empty() && line_w + cell.2 > limit {
                    end_pre_line(&mut out, &mut line, &mut line_w);
                }
                line.push(*cell);
                line_w += cell.2;
            }
        }
    }
    if !line.is_empty() || out.is_empty() {
        out.push(line);
    }
    out
}

/// The lines of a preformatted text as they are drawn at `wrap_w`: a line for each one written, and more where
/// a long one wraps.
fn pre_lines(node: &Node, wrap_w: f32) -> Vec<Vec<PreSeg>> {
    let mut lines = Vec::new();
    for cells in pre_cells(node) {
        for visual in wrap_pre_line(&cells, wrap_w) {
            let mut segs: Vec<PreSeg> = Vec::new();
            for (c, ri, _) in visual {
                match segs.last_mut() {
                    Some(seg) if seg.run == ri => seg.text.push(c),
                    _ => segs.push(PreSeg { text: c.to_string(), run: ri }),
                }
            }
            lines.push(segs);
        }
    }
    lines
}

/// The width of the longest line of a preformatted text, with no line wrapped.
fn pre_natural_w(node: &Node) -> f32 {
    pre_cells(node).iter()
        .map(|line| line.iter().map(|cell| cell.2).sum::<f32>())
        .fold(0.0, f32::max)
}

/// The runs that draw `lines` of a preformatted text again as they are: the text of each stretch, and a newline
/// between the lines, so that a text split between two pages is laid out on each as it was.
fn pre_runs(node: &Node, lines: &[Vec<PreSeg>]) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        for seg in line {
            runs.push(TextRun { text: seg.text.clone(), ..node.text_runs[seg.run].clone() });
        }
        if li + 1 == lines.len() {
            continue;
        }
        match runs.last_mut().filter(|_| !line.is_empty()) {
            Some(last) => last.text.push('\n'),
            None => runs.push(TextRun {
                text: "\n".to_string(), leading_space: false, font: None, color: None, href: None,
                underline: false, strike: false, bold: false, background: None, footnote: None,
            }),
        }
    }
    runs
}

/// Lay out a preformatted text: every line written, wrapped at the width of the box. Its lines are at the left
/// edge, with every stretch placed where the ones before it end.
fn layout_pre_text(node: &Node, x: f32, y: f32, avail_w: f32) -> (RenderNode, f32, bool) {
    let font_size    = node.font_size;
    let line_height  = font_size * node.line_height;
    let parent_color = node.text_color.clone().unwrap_or_else(|| "#1a1a1a".into());
    let lines        = pre_lines(node, avail_w);
    // The metrics of each run, found once for all its stretches.
    let metrics: Vec<Metrics> = node.text_runs.iter()
        .map(|run| Metrics::of(run.font.as_deref().unwrap_or(&node.font)))
        .collect();
    let mut nodes: Vec<RenderNode> = Vec::new();

    for (li, line) in lines.iter().enumerate() {
        let line_y = y + li as f32 * line_height;
        let mut cur_x = x;
        for seg in line {
            let run   = &node.text_runs[seg.run];
            let font  = run.font.as_deref().unwrap_or(node.font.as_str());
            let color = run.color.clone().unwrap_or_else(|| parent_color.clone());
            let width = metrics[seg.run].text_width(&seg.text, font_size);
            let marked = run.href.is_some() || run.underline || run.strike || run.background.is_some();
            // Spaces alone leave nothing to draw, unless something is drawn under or over them.
            if marked || seg.text.chars().any(|c| c != ' ') {
                let mut parts = Vec::new();
                if let Some(tint) = &run.background {
                    parts.push(tint_behind(tint, cur_x, line_y, width, line_height));
                }
                parts.push(RenderNode::Text(RenderText {
                    x: cur_x, y: line_y, content: seg.text.clone(), font: font.to_string(),
                    size: font_size, color: color.clone(), text_align: "left".to_string(),
                    line_width: 0.0, word_gap: 0.0,
                }));
                if run.underline {
                    let uy = line_y + font_size;
                    parts.push(RenderNode::Line(RenderLine {
                        x1: cur_x, y1: uy, x2: cur_x + width, y2: uy, color: color.clone(), thickness: 0.75, dash: None,
                    }));
                }
                if run.strike {
                    let sy = line_y + font_size * 0.5;
                    parts.push(RenderNode::Line(RenderLine {
                        x1: cur_x, y1: sy, x2: cur_x + width, y2: sy, color: color.clone(), thickness: 0.75, dash: None,
                    }));
                }
                match &run.href {
                    Some(href) => nodes.push(RenderNode::Link(RenderLink {
                        url: href.clone(), x: cur_x, y: line_y, width, height: line_height,
                        debug_self: false, tag_id: 0, children: parts,
                    })),
                    None => nodes.extend(parts),
                }
            }
            cur_x += width;
        }
    }

    let total_h = lines.len() as f32 * line_height;
    (
        RenderNode::Box(RenderBox { tag: None, ocg: None,
            x, y, width: avail_w, height: total_h, fill: None, border_width: 0.0, border_color: None,
            radius: 0.0, opacity: 1.0, shadow: None, debug_self: node.debug, children: nodes,
        }),
        total_h,
        false,
    )
}

// ── Elements at fixed places ──────────────────────────────────────────────────

/// The width and height of a laid-out text, image or barcode.
fn render_size(node: &RenderNode) -> (f32, f32) {
    match node {
        RenderNode::Box(b)     => (b.width, b.height),
        RenderNode::Image(i)   => (i.width, i.height),
        RenderNode::Barcode(b) => (b.width, b.height),
        _                      => (0.0, 0.0),
    }
}

/// Lay out a layer's `text-at`, `img-at` or `barcode-at`: the node of the layout, through the layout's own
/// measuring, wrapping and drawing, and then put where the layer says. `page` is the number of the page it
/// is drawn on, and `pages` how many the section has, for `{page}` and `{pages}`.
///
/// The box is laid out at the origin and moved once its size is known, since an anchor other than the top
/// left corner needs it. A `text-at` with a line wider than its box is clipped to the box.
pub(crate) fn layout_placed(placed: &canvas::PlacedNode, page: usize, pages: usize) -> RenderNode {
    let node = substitute_page_tokens(&placed.node, page, pages);
    let (rendered, overflow) = match node.kind {
        NodeKind::Text => {
            let width = node.width_constraint.unwrap_or_else(|| text_natural_w(&node));
            let max_lines = node.max_lines.unwrap_or(1);
            // On one line a text never wraps, whatever its width: it is cut off at it.
            let wrap_w = if max_lines > 1 { width } else { f32::INFINITY };
            let fit = TextFit { wrap_w, max_lines: Some(max_lines), ellipsis: node.ellipsis };
            let (rendered, _, overflow) = layout_text_fit(&node, 0.0, 0.0, width, &fit);
            (rendered, overflow)
        }
        NodeKind::Img     => (layout_img(&node, 0.0, 0.0, f32::MAX).0, false),
        NodeKind::Barcode => (layout_barcode(&node, 0.0, 0.0, f32::MAX).0, false),
        _                 => unreachable!("a layer holds text, images and barcodes"),
    };
    let (w, h) = render_size(&rendered);
    let (left, top) = canvas::apply_anchor_offset(&placed.anchor, placed.x, placed.y, w, h);
    let moved = shift_x(shift_y(rendered, top), left);
    let placed_node = if !overflow {
        moved
    } else {
        RenderNode::CanvasLayer(RenderCanvasLayer { tag: None,
            transform: None,
            clip:      Some(RenderCanvasClip { x: left, y: top, w, h, border_radius: 0.0 }),
            opacity:   1.0,
            children:  vec![moved],
        })
    };
    in_group(placed_node, node.ocg, left, top, w, h)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve child width given parent's available width and alignment.
fn resolve_child_w(child: &Node, avail_w: f32, align: &Align) -> f32 {
    if child.width_fill {
        return avail_w;
    }
    if let Some(w) = own_width(child) {
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
    if node.pre_tab > 0 {
        return pre_natural_w(node);
    }
    let font_size   = node.font_size;
    let parent_font = node.font.as_str();
    let space_w     = text_width(parent_font, " ", font_size);
    let mut total          = 0.0_f32;
    let mut pending_space  = false;

    for (ri, run) in node.text_runs.iter().enumerate() {
        let is_span = run.font.is_some() || run.color.is_some()
                   || run.href.is_some() || run.underline || run.strike || run.background.is_some();
        let metrics = Metrics::of(run.font.as_deref().unwrap_or(parent_font));

        if ri > 0 && run.leading_space { pending_space = true; }

        if let Some(note) = &run.footnote {
            if pending_space { total += space_w; pending_space = false; }
            total += word_width(metrics, &note.number.to_string(), font_size, true);
        } else if is_span {
            if !run.text.is_empty() {
                if pending_space { total += space_w; pending_space = false; }
                total += metrics.text_width(&run.text, font_size);
            }
        } else {
            for (j, word) in run.text.split_whitespace().enumerate() {
                if j > 0 { pending_space = true; }
                if pending_space { total += space_w; pending_space = false; }
                total += metrics.text_width(word, font_size);
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
    shift_y(node, dy)
}

fn shift_y(node: RenderNode, dy: f32) -> RenderNode {
    if dy == 0.0 {
        return node;
    }
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
        RenderNode::Field(mut f) => {
            f.y += dy;
            RenderNode::Field(f)
        }
        RenderNode::Anchor(mut a) => {
            a.y += dy;
            RenderNode::Anchor(a)
        }
        RenderNode::FootnoteMark(mut m) => {
            m.y += dy;
            RenderNode::FootnoteMark(m)
        }
        // What a `<shape>` or the checkbox of an `<li>` draws moves with its box.
        RenderNode::CanvasRect(mut r) => {
            r.y += dy;
            RenderNode::CanvasRect(r)
        }
        RenderNode::CanvasEllipse(mut e) => {
            e.cy += dy;
            RenderNode::CanvasEllipse(e)
        }
        RenderNode::CanvasPath(mut p) => {
            p.d = shift_path(&p.d, 0.0, dy);
            RenderNode::CanvasPath(p)
        }
        // The other canvas nodes use absolute canvas coordinates; y-shifting is a no-op.
        other => other,
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
        RenderNode::Field(mut f) => {
            f.x += dx;
            RenderNode::Field(f)
        }
        // What a `<shape>` or the checkbox of an `<li>` draws moves with its box.
        RenderNode::CanvasRect(mut r) => {
            r.x += dx;
            RenderNode::CanvasRect(r)
        }
        RenderNode::CanvasEllipse(mut e) => {
            e.cx += dx;
            RenderNode::CanvasEllipse(e)
        }
        RenderNode::CanvasPath(mut p) => {
            p.d = shift_path(&p.d, dx, 0.0);
            RenderNode::CanvasPath(p)
        }
        // The other canvas nodes use absolute canvas coordinates; x-shifting is a no-op.
        other => other,
    }
}

/// The path `d` moved by `dx` and `dy`. In the flow only the tick of a checkbox is a path, drawn with `M` and `L`
/// (`canvas::draw_checkbox`), so its numbers are x and y in turn.
fn shift_path(d: &str, dx: f32, dy: f32) -> String {
    let mut is_x = true;
    d.split_whitespace()
        .map(|word| match word.parse::<f32>() {
            Ok(n) => {
                let offset = if is_x { dx } else { dy };
                is_x = !is_x;
                (n + offset).to_string()
            }
            Err(_) => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
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
            // The flow of a page that is cut at its bottom (`clip_overflow`).
            RenderNode::CanvasLayer(l) => apply_debug_overlay(&mut l.children),
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
        let mut doc = parse(xml).unwrap();
        let lp = doc.section_layouts();
        let pages: Vec<_> = lp.iter().flat_map(layout_page).collect();
        let json_str =
            crate::render::pages_to_json(&pages, serde_json::Value::Null, serde_json::Value::Null);
        serde_json::from_str(&json_str).unwrap()
    }

    #[test]
    fn an_atom_stays_small() {
        // Each word of a text is moved through the atoms, the queue and the lines of every layout of it.
        assert!(std::mem::size_of::<Atom>() <= 72, "Atom is {} bytes", std::mem::size_of::<Atom>());
    }

    fn minimal(body: &str) -> String {
        if body.is_empty() {
            r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt"/></section></document></lpdf>"#.to_string()
        } else {
            format!(
                r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
            )
        }
    }

    #[test]
    fn page_dimensions_correct() {
        let tree = engine_render(&minimal(""));
        assert_eq!(tree["pages"][0]["width"], 595.28);
        assert_eq!(tree["pages"][0]["height"], 841.89);
    }

    #[test]
    fn a_layout_without_a_margin_places_its_content_36pt_from_the_page_edges() {
        let tree = engine_render(
            r#"<lpdf version="1"><document size="a4"><section><layout><frame height="20pt" /></layout></section></document></lpdf>"#,
        );
        let page = &tree["pages"][0];
        assert_eq!(page["margin"], serde_json::json!([36.0, 36.0, 36.0, 36.0]));
        let frame = &page["nodes"][0];
        assert_eq!((frame["x"].as_f64(), frame["y"].as_f64()), (Some(36.0), Some(36.0)));
        let width = frame["width"].as_f64().unwrap();
        assert!((width - (595.28 - 72.0)).abs() < 0.01, "width={width}");
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
    fn a_box_is_as_high_as_the_height_slot_it_names() {
        let render = |tokens: &str, body: &str| {
            engine_render(&format!(
                r#"<lpdf version="1"><tokens>{tokens}</tokens><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
            ))
        };
        let tree = render("", r#"<frame height="s" /><frame height="xxl" /><frame height="40pt" />"#);
        let heights: Vec<f64> = tree["pages"][0]["nodes"].as_array().unwrap().iter().map(|n| n["height"].as_f64().unwrap()).collect();
        assert_eq!(heights, [24.0, 96.0, 40.0], "s and xxl of the default row, and a length");

        let row = r#"<height xs="10pt" s="70pt" m="30pt" l="40pt" xl="50pt" xxl="60pt"/>"#;
        let tree = render(row, r#"<frame height="s" />"#);
        assert_eq!(tree["pages"][0]["nodes"][0]["height"], 70.0, "the document's own row");
    }

    #[test]
    fn column_positions_children_sequentially() {
        // Two frames with fixed heights and a gap
        let tree = engine_render(&minimal(
            r#"<column gap="m"><frame height="20pt" /><frame height="30pt" /></column>"#,
        ));
        let column = &tree["pages"][0]["nodes"][0];
        let children = column["nodes"].as_array().unwrap();
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
        let children = grid["nodes"].as_array().unwrap();
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
        let tree = engine_render(&minimal(r#"<text>Hello world</text>"#));
        let node = &tree["pages"][0]["nodes"][0];
        assert_eq!(node["type"], "box");
        let kids = node["nodes"].as_array().unwrap();
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
    fn row_with_two_fill_children_gives_half_widths() {
        let body = r#"<row gap="m">
            <frame width="fill" height="20pt" />
            <frame width="fill" height="20pt" />
        </row>"#;
        let tree = engine_render(&minimal(body));
        let row = &tree["pages"][0]["nodes"][0];
        let children = row["nodes"].as_array().unwrap();
        let w0 = children[0]["width"].as_f64().unwrap();
        let w1 = children[1]["width"].as_f64().unwrap();
        let avail_w = 595.28 - 28.0 * 2.0;
        let expected = (avail_w - 8.0) / 2.0;
        assert!((w0 - expected).abs() < 0.5, "w0={w0} expected={expected}");
        assert!((w1 - expected).abs() < 0.5);
    }

    #[test]
    fn column_fill_child_gets_remaining_height() {
        let body = r#"<column gap="m" height="full">
            <frame height="50pt" />
            <frame height="fill" />
        </column>"#;
        let tree = engine_render(&minimal(body));
        let column = &tree["pages"][0]["nodes"][0];
        let children = column["nodes"].as_array().unwrap();
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
    fn wrapped_column_overflow_splits_into_pages() {
        // Common document pattern: a single <column> wrapping many paragraphs.
        // Each frame child is 120pt; avail_h ≈ 785.89pt — 7 frames fit, 8 would not.
        let item = r#"<frame height="120pt" />"#;
        let body = format!("<column>{}</column>", item.repeat(10));
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
        let body = format!(r#"<text>{paragraph}</text>"#);
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2, "expected text to split, got {} pages", pages.len());
        // Each page has the text at the top.
        for p in pages {
            assert!(!p["nodes"].as_array().unwrap().is_empty());
        }
    }

    #[test]
    fn row_splits_between_wrapped_lines() {
        // Many fixed-height frames in a row produce many wrapped rows; the
        // row breaks between rows rather than treating itself as atomic.
        let item = r##"<frame width="180pt" height="100pt" background="#ddd" />"##;
        let body = format!(r#"<row gap="m">{}</row>"#, item.repeat(40));
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2, "row should split, got {} pages", pages.len());
    }

    #[test]
    fn repeat_page_renders_on_every_page() {
        // A region pinned to bottom with no page= (defaults to each) must appear on every page.
        let filler = r#"<frame height="120pt" />"#;
        let body = format!(
            r#"<region pin="bottom"><text>FOOTER</text></region>{}"#,
            filler.repeat(10)
        );
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2);
        for (i, p) in pages.iter().enumerate() {
            let found = find_text_content(p).iter().any(|t| t.contains("FOOTER"));
            assert!(found, "page {} missing footer", i + 1);
        }
    }

    /// Helper: the `y` of every text node of a page, by its content.
    fn text_ys(page: &serde_json::Value) -> HashMap<String, f64> {
        let mut out = HashMap::new();
        fn walk(v: &serde_json::Value, out: &mut HashMap<String, f64>) {
            if v["type"] == "text" {
                if let (Some(s), Some(y)) = (v["content"].as_str(), v["y"].as_f64()) { out.insert(s.to_string(), y); }
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, out); }
            }
        }
        walk(page, &mut out);
        out
    }

    #[test]
    fn a_region_is_drawn_at_the_edge_its_pin_names_wherever_it_is_written() {
        // Written in the wrong order on purpose: the footer first, the header last.
        let body = r#"<region pin="bottom"><text>FOOT</text></region><text>BODY</text><region pin="top"><text>HEAD</text></region>"#;
        let tree = engine_render(&minimal(body));
        let ys = text_ys(&tree["pages"][0]);
        let (head, body, foot) = (ys["HEAD"], ys["BODY"], ys["FOOT"]);
        assert!(head < body && body < foot, "head {head}, body {body}, foot {foot}");
        assert!(foot > 700.0, "the footer is drawn at the bottom of the page, not at {foot}");
    }

    #[test]
    fn a_region_written_in_the_right_order_is_drawn_as_before() {
        let body = r#"<region pin="top"><text>HEAD</text></region><text>BODY</text><region pin="bottom"><text>FOOT</text></region>"#;
        let tree = engine_render(&minimal(body));
        let ys = text_ys(&tree["pages"][0]);
        assert!(ys["HEAD"] < ys["BODY"] && ys["BODY"] < ys["FOOT"] && ys["FOOT"] > 700.0, "{ys:?}");
    }

    #[test]
    fn repeat_first_renders_only_on_first_page() {
        let filler = r#"<frame height="120pt" />"#;
        let body = format!(
            r#"<region pin="top" page="first"><text>COVER HEADER</text></region>{}"#,
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
            r#"<region pin="bottom"><text>Page {{page}} of {{pages}}</text></region>{}"#,
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

    // ── paginate ──────────────────────────────────────────────────────────────

    /// The page, counted from 1, that holds the text `needle`, or None.
    fn page_with(pages: &[serde_json::Value], needle: &str) -> Option<usize> {
        pages.iter().position(|p| find_text_content(p).iter().any(|t| t.contains(needle))).map(|i| i + 1)
    }

    #[test]
    fn paginate_break_before_starts_a_new_page() {
        let flowing = engine_render(&minimal("<text>ONE</text><text>TWO</text>"));
        assert_eq!(flowing["pages"].as_array().unwrap().len(), 1, "without paginate both fit on one page");

        let tree = engine_render(&minimal(r#"<text>ONE</text><text paginate="break-before">TWO</text>"#));
        let pages = tree["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(page_with(pages, "ONE"), Some(1));
        assert_eq!(page_with(pages, "TWO"), Some(2));
    }

    #[test]
    fn paginate_break_after_starts_the_next_sibling_on_a_new_page() {
        let tree = engine_render(&minimal(r#"<text paginate="break-after">ONE</text><text>TWO</text>"#));
        let pages = tree["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(page_with(pages, "ONE"), Some(1));
        assert_eq!(page_with(pages, "TWO"), Some(2));
    }

    #[test]
    fn paginate_keep_next_moves_a_box_with_the_sibling_that_does_not_fit() {
        // 760pt of the page's 786 are used; the head fits, its sibling (200pt) does not, and both fit together on the next page.
        let body = |attr: &str| {
            format!(r#"<frame height="760pt" /><text {attr}>HEAD</text><frame height="200pt" /><text>TAIL</text>"#)
        };
        let loose = engine_render(&minimal(&body("")));
        assert_eq!(page_with(loose["pages"].as_array().unwrap(), "HEAD"), Some(1), "the head is stranded without keep-next");

        let kept = engine_render(&minimal(&body(r#"paginate="keep-next""#)));
        assert_eq!(page_with(kept["pages"].as_array().unwrap(), "HEAD"), Some(2), "keep-next brings the head to the sibling's page");
    }

    #[test]
    fn paginate_keep_next_moves_the_keep_next_nodes_before_it_along() {
        // Two heads in a row, each kept with the one after it: the second moves on with its sibling, and the first
        // is not left alone at the foot of the page.
        let body = |attr: &str| {
            format!(r#"<frame height="700pt" /><text {attr}>H1</text><text {attr}>H2</text><frame height="200pt" /><text>TAIL</text>"#)
        };
        let loose = engine_render(&minimal(&body("")));
        assert_eq!(page_with(loose["pages"].as_array().unwrap(), "H1"), Some(1));
        assert_eq!(page_with(loose["pages"].as_array().unwrap(), "H2"), Some(1));

        let kept = engine_render(&minimal(&body(r#"paginate="keep-next""#)));
        let pages = kept["pages"].as_array().unwrap();
        assert_eq!(page_with(pages, "H2"), Some(2), "the second head goes with its sibling");
        assert_eq!(page_with(pages, "H1"), Some(2), "and the first goes with the second");
    }

    /// A table of a header row and `rows` rows of about 41pt, which a page holds whole when it is empty.
    fn rows_table(rows: usize) -> String {
        let row = |text: &str| format!(r#"<tr><td padding="14pt"><text>{text}</text></td></tr>"#);
        let body: String = (1..=rows).map(|n| row(&format!("R{n}"))).collect();
        format!(r#"<table cols="1fr"><thead><td padding="14pt"><text>TH</text></td></thead>{body}</table>"#)
    }

    #[test]
    fn paginate_keep_next_keeps_the_head_with_the_first_rows_of_a_table_that_splits() {
        // 500pt are used, which leaves room for the head and the header and five rows of the table, not for all of
        // it, and the table is shorter than a page.
        let page = |attr: &str, frame: &str| {
            let tree = engine_render(&minimal(&format!(
                r#"<frame height="{frame}pt" /><text {attr}>HEAD</text>{}"#, rows_table(12)
            )));
            let pages = tree["pages"].as_array().unwrap().clone();
            (page_with(&pages, "HEAD"), page_with(&pages, "TH"), page_with(&pages, "R12"))
        };
        assert_eq!(page(r#"paginate="keep-next""#, "500"), (Some(1), Some(1), Some(2)), "the head stays, with the header on its page");
        assert_eq!(page("", "500"), (Some(1), Some(1), Some(2)), "as it does without keep-next");
    }

    #[test]
    fn paginate_keep_next_moves_the_head_with_a_table_that_cannot_start_on_the_page() {
        // 735pt are used: there is room for the head and not for the header and a row of the table.
        let tree = engine_render(&minimal(&format!(
            r#"<frame height="735pt" /><text paginate="keep-next">HEAD</text>{}"#, rows_table(12)
        )));
        let pages = tree["pages"].as_array().unwrap();
        assert_eq!((page_with(pages, "HEAD"), page_with(pages, "TH")), (Some(2), Some(2)));
    }

    #[test]
    fn paginate_keep_next_keeps_the_head_with_the_first_lines_of_a_text_that_splits() {
        let words = "word ".repeat(700);
        let tree = engine_render(&minimal(&format!(
            r#"<frame height="600pt" /><text paginate="keep-next">HEAD</text><text>FIRST {words}</text>"#
        )));
        let pages = tree["pages"].as_array().unwrap();
        assert_eq!((page_with(pages, "HEAD"), page_with(pages, "FIRST")), (Some(1), Some(1)));
        assert!(pages.len() > 1, "the paragraph goes on to the next page");
    }

    #[test]
    fn paginate_keep_next_leaves_a_page_that_it_would_empty_as_it_was() {
        // Both heads are already at the top of the page, so there is nothing to gain by moving them.
        let kept = engine_render(&minimal(r#"<text paginate="keep-next">H1</text><text paginate="keep-next">H2</text><frame height="900pt" />"#));
        let pages = kept["pages"].as_array().unwrap();
        assert_eq!(page_with(pages, "H1"), Some(1));
        assert_eq!(page_with(pages, "H2"), Some(1));
    }

    #[test]
    fn paginate_no_keeps_a_column_whole() {
        // 760pt used leaves room for one line of the column, which splits between its children unless it is kept whole.
        let body = |attr: &str| {
            format!(r#"<frame height="760pt" /><column {attr}><text>R1</text><text>R2</text><text>R3</text><text>R4</text></column>"#)
        };
        let split = engine_render(&minimal(&body("")));
        let pages = split["pages"].as_array().unwrap();
        assert_eq!(page_with(pages, "R1"), Some(1), "an unkept column splits, its first line on page 1");
        assert_eq!(page_with(pages, "R4"), Some(2));

        let whole = engine_render(&minimal(&body(r#"paginate="no""#)));
        let pages = whole["pages"].as_array().unwrap();
        assert_eq!(page_with(pages, "R1"), Some(2), "a column that does not fit moves whole to the next page");
        assert_eq!(page_with(pages, "R4"), Some(2));
    }

    #[test]
    fn paginate_breaks_are_not_applied_to_a_box_inside_another_box() {
        // The schema says so: break-before, break-after and keep-next act on the children of layout only.
        let tree = engine_render(&minimal(r#"<column><text>ONE</text><text paginate="break-before">TWO</text></column>"#));
        assert_eq!(tree["pages"].as_array().unwrap().len(), 1, "a break inside a column is ignored");
    }

    // ── widows and orphans ────────────────────────────────────────────────────

    /// The lines of a text on each page. The text has `lines` lines, each a word too wide to share its line, and a
    /// frame before it leaves room for `room` of them on the first page (a page holds 785.89pt of 13.2pt lines).
    fn page_lines(attrs: &str, lines: usize, room: usize) -> Vec<Vec<String>> {
        page_lines_of(attrs, lines, room, 13.2)
    }

    /// `page_lines` for a text whose lines are `line_h` high, which `attrs` has to say.
    fn page_lines_of(attrs: &str, lines: usize, room: usize, line_h: f32) -> Vec<Vec<String>> {
        let words: Vec<String> = (1..=lines).map(|n| format!("L{n:02}{}", "W".repeat(36))).collect();
        let frame = 785.89 - line_h * room as f32 - line_h / 2.0;
        let body = format!(r#"<frame height="{frame:.2}pt" /><text {attrs}>{}</text>"#, words.join(" "));
        let tree = engine_render(&minimal(&body));
        tree["pages"].as_array().unwrap().iter().map(find_text_content).collect()
    }

    /// How many lines of the text of `page_lines` each page has.
    fn lines_per_page(attrs: &str, lines: usize, room: usize) -> Vec<usize> {
        page_lines(attrs, lines, room).iter().map(Vec::len).collect()
    }

    #[test]
    fn widows_and_orphans_pull_a_lone_last_line_back_to_the_next_page() {
        // Room for 5 of 6 lines would leave 1 on the next page; by default it gets 2.
        assert_eq!(lines_per_page("", 6, 5), vec![4, 2]);
    }

    #[test]
    fn widows_and_orphans_move_a_text_whole_when_one_line_would_stay_behind() {
        // Room for 1 of 6 lines would leave 1 on the first page; by default it keeps 2 or moves on.
        assert_eq!(lines_per_page("", 6, 1), vec![0, 6]);
    }

    #[test]
    fn widows_and_orphans_of_one_turn_protection_off() {
        let off = r#"widows="1" orphans="1""#;
        assert_eq!(lines_per_page(off, 6, 5), vec![5, 1], "a lone last line stays on the next page");
        assert_eq!(lines_per_page(off, 6, 1), vec![1, 5], "a lone first line stays on the first page");
    }

    #[test]
    fn widows_and_orphans_leave_a_text_too_short_to_split_whole() {
        // 3 lines cannot give 2 to each page, so room for 2 of them moves all 3 on.
        assert_eq!(lines_per_page("", 3, 2), vec![0, 3]);
        assert_eq!(lines_per_page(r#"widows="1" orphans="1""#, 3, 2), vec![2, 1]);
    }

    #[test]
    fn widows_attribute_sets_the_lines_kept_for_the_next_page() {
        assert_eq!(lines_per_page("", 8, 6), vec![6, 2]);
        assert_eq!(lines_per_page(r#"widows="3""#, 8, 6), vec![5, 3]);
    }

    #[test]
    fn orphans_attribute_sets_the_lines_kept_on_the_first_page() {
        assert_eq!(lines_per_page("", 8, 2), vec![2, 6]);
        assert_eq!(lines_per_page(r#"orphans="3""#, 8, 2), vec![0, 8]);
    }

    #[test]
    fn widows_and_orphans_split_where_the_lines_fit_when_no_page_could_keep_them() {
        // At 300pt a page holds 2 lines, so 3 lines cannot be kept whole or split 2 and 2: moving on would only
        // push the text past the end of the next page.
        let tree = engine_render(&minimal(r#"<text font-size="300pt">L01 L02 L03</text>"#));
        let lines: Vec<usize> =
            tree["pages"].as_array().unwrap().iter().map(|p| find_text_content(p).len()).collect();
        assert_eq!(lines, vec![2, 1]);
    }

    // ── code ──────────────────────────────────────────────────────────────────

    /// Every text of a page tree with where it is: `(x, y, content)`, in the order they are drawn.
    fn placed_texts(page: &serde_json::Value) -> Vec<(f64, f64, String)> {
        fn walk(v: &serde_json::Value, out: &mut Vec<(f64, f64, String)>) {
            if v["type"] == "text" {
                out.push((v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap(), v["content"].as_str().unwrap().to_string()));
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, out); }
            }
        }
        let mut out = Vec::new();
        walk(page, &mut out);
        out
    }

    /// The boxes of a page tree that are filled with `fill`: `(y, height)`.
    fn filled_boxes(page: &serde_json::Value, fill: &str) -> Vec<(f64, f64)> {
        fn walk(v: &serde_json::Value, fill: &str, out: &mut Vec<(f64, f64)>) {
            if v["type"] == "box" && v["fill"] == fill {
                out.push((v["y"].as_f64().unwrap(), v["height"].as_f64().unwrap()));
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, fill, out); }
            }
        }
        let mut out = Vec::new();
        walk(page, fill, &mut out);
        out
    }

    /// Every text of a page tree: `(x, font, size, content)`.
    fn text_boxes(page: &serde_json::Value) -> Vec<(f64, String, f64, String)> {
        fn walk(v: &serde_json::Value, out: &mut Vec<(f64, String, f64, String)>) {
            if v["type"] == "text" {
                out.push((
                    v["x"].as_f64().unwrap(), v["font"].as_str().unwrap().to_string(),
                    v["size"].as_f64().unwrap(), v["content"].as_str().unwrap().to_string(),
                ));
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, out); }
            }
        }
        let mut out = Vec::new();
        walk(page, &mut out);
        out
    }

    /// The pages of a document with `markdown` in a section with 54pt of margin.
    fn markdown_pages(markdown: &str) -> Vec<serde_json::Value> {
        let xml = format!(
            r#"<lpdf version="1"><document size="a4"><section><markdown margin="54pt"><markdown-content><![CDATA[{markdown}]]></markdown-content></markdown></section></document></lpdf>"#
        );
        engine_render(&xml)["pages"].as_array().unwrap().clone()
    }

    const WIDE_NAME: &str = "Products__lpdf__Tiers__professional__PaddlePriceId";

    /// The right margin of `markdown_pages`.
    const MARKDOWN_RIGHT_EDGE: f64 = 595.28 - 54.0;

    #[test]
    fn a_markdown_breaks_a_name_wider_than_its_table_cell_so_it_stays_in_the_cell() {
        // Two names that are each wider than half the page: neither column can have its name on one line.
        let pages = markdown_pages(&format!("| Key | Other |\n|---|---|\n| `{WIDE_NAME}` | `{WIDE_NAME}` |\n"));
        let texts = text_boxes(&pages[0]);
        let other_x = texts.iter().find(|t| t.3 == "Other").unwrap().0;
        let pieces: Vec<&(f64, String, f64, String)> = texts.iter().filter(|t| t.3.contains('_') && t.0 < other_x).collect();
        assert!(pieces.len() >= 2, "the name is on more than one line: {pieces:?}");
        assert_eq!(pieces.iter().map(|t| t.3.as_str()).collect::<String>(), WIDE_NAME, "nothing is lost");
        for (x, font, size, content) in pieces {
            let right = x + text_width(font, content, *size as f32) as f64;
            assert!(right <= other_x, "{content} ends at {right}, in the next column, which starts at {other_x}");
        }
    }

    /// The pages of a layout of 28pt margins holding `body`, and the right edge of its content.
    fn plain_pages(body: &str) -> Vec<serde_json::Value> {
        engine_render(&minimal(body))["pages"].as_array().unwrap().clone()
    }

    const PLAIN_RIGHT_EDGE: f64 = 595.28 - 28.0;

    #[test]
    fn a_word_wider_than_its_box_breaks_at_the_last_character_that_fits_in_a_text_of_the_document() {
        let pages = plain_pages(&format!(r#"<column width="120pt"><text>See {WIDE_NAME} and more words</text></column>"#));
        let texts = text_boxes(&pages[0]);
        assert!(texts.len() >= 4, "the name is on several lines: {texts:?}");
        let all: String = texts.iter().map(|t| t.3.as_str()).collect::<Vec<_>>().join("").replace(' ', "");
        assert_eq!(all, format!("See{WIDE_NAME}andmorewords"), "nothing is lost");
        for (x, font, size, content) in texts {
            let right = x + text_width(&font, &content, size as f32) as f64;
            assert!(right <= 28.0 + 120.0 + 0.1, "{content} ends at {right}, past the box");
        }
    }

    #[test]
    fn a_styled_run_wider_than_a_whole_line_breaks_too() {
        let pages = plain_pages(&format!(
            r##"<column width="120pt"><text>See <span color="#cc0000">{WIDE_NAME}</span> and <span color="#00aa00">{WIDE_NAME} {WIDE_NAME}</span> here</text></column>"##
        ));
        let texts = text_boxes(&pages[0]);
        assert!(texts.len() > 6, "the runs are cut into pieces: {texts:?}");
        for (x, font, size, content) in texts {
            let right = x + text_width(&font, &content, size as f32) as f64;
            assert!(right <= 28.0 + 120.0 + 0.1, "{content} ends at {right}, past the box");
        }
    }

    #[test]
    fn a_text_of_the_document_with_wide_words_that_runs_onto_another_page_stays_within_its_margin() {
        let content = format!("{WIDE_NAME} and some words to fill the lines, ").repeat(80);
        let pages = plain_pages(&format!("<text>{content}</text>"));
        assert!(pages.len() > 1, "the text runs onto a second page");
        for page in &pages {
            for (x, font, size, content) in text_boxes(page) {
                let right = x + text_width(&font, &content, size as f32) as f64;
                assert!(right <= PLAIN_RIGHT_EDGE + 0.1, "{content} ends at {right}, past the margin");
            }
        }
    }

    #[test]
    fn a_name_wider_than_a_table_cell_breaks_in_the_cell_of_a_table_of_the_document() {
        let pages = plain_pages(&format!(
            r#"<table cols="auto auto"><tr><td><text>{WIDE_NAME}{WIDE_NAME}</text></td><td><text>{WIDE_NAME}{WIDE_NAME}</text></td></tr></table>"#
        ));
        let texts = text_boxes(&pages[0]);
        assert!(texts.len() >= 4, "each name is on more than one line: {texts:?}");
        for (x, font, size, content) in texts {
            let right = x + text_width(&font, &content, size as f32) as f64;
            assert!(right <= PLAIN_RIGHT_EDGE + 0.1, "{content} ends at {right}, past the margin");
        }
    }

    #[test]
    fn a_styled_run_that_fits_a_line_goes_down_whole_in_a_text_of_the_document() {
        // "gamma" would fit what is left of the first line, but the run is moved down whole, as it always was; only
        // a markdown fills the line with the first words of a run.
        let (_, lines, _) = flow_text("", r##"alpha beta <span color="#cc0000">gamma delta</span>"##);
        assert_eq!(lines, ["alpha beta", "gamma delta"]);
    }

    // ── Tables sized by their cells ───────────────────────────────────────────

    /// The `x` of the first text of each cell of the first row, which is where its column starts.
    fn column_starts(tree: &serde_json::Value, row_texts: &[&str]) -> Vec<f64> {
        let texts = placed_texts(&tree["pages"][0]);
        row_texts.iter().map(|word| texts.iter().find(|t| t.2 == *word).unwrap().0).collect()
    }

    fn table_of(cols: &str, rows: &[[&str; 3]]) -> String {
        let rows: String = rows.iter()
            .map(|r| format!("<tr>{}</tr>", r.iter().map(|c| format!("<td><text>{c}</text></td>")).collect::<String>()))
            .collect();
        format!(r#"<table cols="{cols}">{rows}</table>"#)
    }

    #[test]
    fn an_auto_column_takes_the_width_its_cells_need_and_the_room_left_goes_to_the_columns_in_proportion() {
        let rows = [["parse_xml/small/example1", "12,571", "590"], ["parse_xml/medium/example8", "273,184", "74,779"]];
        let tree = engine_render(&minimal(&table_of("auto auto auto", &rows)));
        let starts = column_starts(&tree, &["parse_xml/small/example1", "12,571", "590"]);
        let texts = placed_texts(&tree["pages"][0]);
        assert_eq!(texts.len(), 6, "each cell is on one line: {texts:?}");
        let widths = [starts[1] - starts[0], starts[2] - starts[1], 28.0 + 539.28 - starts[2]];
        assert!(widths[0] > widths[1] && widths[1] > widths[2], "{widths:?}");
        // The first column is as wide as its longer name, and more: the room left is shared in proportion.
        assert!(widths[0] >= text_width("Helvetica", "parse_xml/medium/example8", 11.0) as f64, "{widths:?}");
        assert!((widths.iter().sum::<f64>() - 539.28).abs() < 0.1, "the columns fill the table: {widths:?}");
    }

    #[test]
    fn a_column_of_short_cells_keeps_its_width_when_the_text_beside_it_wraps() {
        let prose = "words that run on and on until a line of this page is not long enough to hold them all, so they wrap ".repeat(3);
        let tree = engine_render(&minimal(&table_of("auto auto auto", &[[&prose, "12,571", "590"]])));
        let starts = column_starts(&tree, &["12,571", "590"]);
        let number_w = text_width("Helvetica", "12,571", 11.0) as f64;
        // The number column is as wide as the number, and its neighbor is as narrow as the room left allows.
        assert!((starts[1] - starts[0] - number_w).abs() < 0.1, "{starts:?}");
        let lines = placed_texts(&tree["pages"][0]).iter().filter(|t| t.0 < starts[0]).count();
        assert!(lines > 1, "the prose wraps");
    }

    #[test]
    fn auto_columns_share_what_the_fixed_columns_leave() {
        let tree = engine_render(&minimal(&table_of("auto 90pt 60pt", &[["name", "a", "b"]])));
        let starts = column_starts(&tree, &["name", "a", "b"]);
        assert!((starts[1] - 28.0 - (539.28 - 150.0)).abs() < 0.1, "{starts:?}");
        assert!((starts[2] - starts[1] - 90.0).abs() < 0.1, "{starts:?}");
    }

    #[test]
    fn the_longest_names_break_first_when_not_even_the_longest_words_fit() {
        let name = "x".repeat(120);
        let tree = engine_render(&minimal(&table_of("auto auto auto", &[[&name, "short", "also"]])));
        let starts = column_starts(&tree, &["short", "also"]);
        // The two short columns are as wide as their words, and the name has the rest.
        assert!((starts[1] - starts[0] - text_width("Helvetica", "short", 11.0) as f64).abs() < 0.1, "{starts:?}");
        assert!((starts[0] - 28.0 - (539.28 - text_width("Helvetica", "short", 11.0) as f64 - text_width("Helvetica", "also", 11.0) as f64)).abs() < 0.1, "{starts:?}");
    }

    #[test]
    fn a_table_sized_by_its_cells_keeps_its_columns_on_the_next_page() {
        let rows: Vec<[&str; 3]> = (0..120).map(|_| ["a name that is a bit long", "12,571", "590"]).collect();
        let tree = engine_render(&minimal(&table_of("auto auto auto", &rows)));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() > 1, "the table runs onto a second page");
        let second_column = |page: &serde_json::Value| placed_texts(page).iter().find(|t| t.2 == "12,571").unwrap().0;
        assert!((second_column(&pages[0]) - second_column(&pages[1])).abs() < 0.01);
    }

    #[test]
    fn auto_beside_fr_is_an_error_that_says_what_to_use() {
        let xml = minimal(&table_of("auto 1fr", &[["a", "b", "c"]]));
        let err = match crate::parse::parse(&xml) { Err(e) => e, Ok(_) => panic!("auto beside fr was accepted") };
        assert!(err.contains("auto and fr") && err.contains("pt or %"), "{err}");
    }

    #[test]
    fn a_markdown_wraps_a_long_link_a_code_span_with_spaces_and_a_long_word_within_its_margin() {
        let pages = markdown_pages(&format!(
            "A [link whose text is long enough that it has to wrap onto a second line of the page](https://lpdf.io) and \
             `code with spaces that is long enough to need a second line on this page of text, and more` and {WIDE_NAME}{WIDE_NAME}.\n"
        ));
        let texts = text_boxes(&pages[0]);
        assert!(texts.iter().any(|t| t.3.contains("wrap")) && texts.iter().any(|t| t.3.contains("need")), "{texts:?}");
        for (x, font, size, content) in &texts {
            let right = x + text_width(font, content, *size as f32) as f64;
            assert!(right <= MARKDOWN_RIGHT_EDGE + 0.1, "{content} ends at {right}, past the margin at {MARKDOWN_RIGHT_EDGE}");
        }
    }

    /// The tinted boxes of a page tree: `(x, y, width)`.
    fn tints(page: &serde_json::Value, fill: &str) -> Vec<(f64, f64, f64)> {
        fn walk(v: &serde_json::Value, fill: &str, out: &mut Vec<(f64, f64, f64)>) {
            if v["type"] == "box" && v["fill"] == fill {
                out.push((v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap(), v["width"].as_f64().unwrap()));
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, fill, out); }
            }
        }
        let mut out = Vec::new();
        walk(page, fill, &mut out);
        out
    }

    /// A paragraph of plain words that ends where `span` has room for its first word and not for all of it.
    fn paragraph_ending_before(span: &str) -> String {
        let first = span.split(' ').next().unwrap();
        let room = 595.28 - 108.0;
        let courier = |text: &str| text_width("Courier", text, 11.0) as f32;
        let space = text_width("Helvetica", " ", 11.0);
        (1..300)
            .map(|n| vec!["word"; n].join(" "))
            .find(|filler| {
                let used = text_width("Helvetica", filler, 11.0) + space;
                used + courier(first) <= room && used + courier(span) > room
            })
            .map(|filler| format!("{filler} `{span}` end."))
            .expect("a filler that leaves room for the first word only")
    }

    #[test]
    fn a_markdown_code_span_that_does_not_fit_what_is_left_of_the_line_goes_on_in_its_next_word() {
        let pages = markdown_pages(&paragraph_ending_before("extractable: false"));
        let texts = placed_texts(&pages[0]);
        let y_of = |content: &str| texts.iter().find(|t| t.2 == content).unwrap_or_else(|| panic!("{content} in {texts:?}")).1;
        assert_eq!(y_of("extractable:"), y_of(texts[0].2.as_str()), "the first word is on the line with the words before it");
        assert!(y_of("false") > y_of("extractable:"), "and the rest of the run goes down");
    }

    #[test]
    fn a_bracket_with_no_space_before_a_code_span_goes_down_with_it() {
        let room = 595.28 - 108.0;
        let space = text_width("Helvetica", " ", 11.0);
        let bracket = text_width("Helvetica", "(", 11.0);
        let code = text_width("Courier", "libconseal_core.so", 11.0);
        let filler = (1..300)
            .map(|n| vec!["word"; n].join(" "))
            .find(|f| {
                let used = text_width("Helvetica", f, 11.0) + space;
                used + bracket <= room && used + bracket + code > room
            })
            .expect("a filler with room for the bracket and not for the code after it");
        let pages = markdown_pages(&format!("{filler} (`libconseal_core.so`) end."));
        let texts = placed_texts(&pages[0]);
        assert!(!texts[0].2.contains('('), "the line of words does not end in the bracket: {texts:?}");
        let y_of = |content: &str| texts.iter().find(|t| t.2.contains(content)).unwrap().1;
        assert_eq!(y_of("("), y_of("libconseal_core.so"), "the bracket is on the line of the code");
    }

    #[test]
    fn the_tint_of_a_code_span_is_one_stretch_across_the_gaps_of_a_line_it_is_broken_on() {
        let pages = markdown_pages(&format!("{} more", paragraph_ending_before("alpha beta gamma").trim_end_matches(" end.")));
        let boxes = tints(&pages[0], "#ffffff");
        let on_first_line: Vec<_> = boxes.iter().filter(|b| (b.1 - boxes[0].1).abs() < 0.01).collect();
        assert!(on_first_line.len() >= 2, "the run has more than one word on the line: {boxes:?}");
        for pair in on_first_line.windows(2) {
            assert!((pair[0].0 + pair[0].2 - pair[1].0).abs() < 0.01, "the second reaches back to where the first ends: {pair:?}");
        }
    }

    #[test]
    fn a_markdown_paragraph_with_wide_words_that_runs_onto_another_page_stays_within_its_margin() {
        let paragraph = format!("{WIDE_NAME} and some words to fill the lines, `{WIDE_NAME}` too, ").repeat(60);
        let pages = markdown_pages(&format!("{paragraph}\n"));
        assert!(pages.len() > 1, "the paragraph runs onto a second page");
        for page in &pages {
            for (x, font, size, content) in text_boxes(page) {
                let right = x + text_width(&font, &content, size as f32) as f64;
                assert!(right <= MARKDOWN_RIGHT_EDGE + 0.1, "{content} ends at {right}, past the margin");
            }
        }
    }

    /// The contents of the texts of the first page of a layout holding `body`.
    fn first_page_lines(body: &str) -> Vec<String> {
        find_text_content(&engine_render(&minimal(body))["pages"][0])
    }

    #[test]
    fn a_code_keeps_its_spaces_its_lines_and_its_blank_lines() {
        let tree = engine_render(&minimal("<code>\nfn a() {\n    b();\n\n}\n</code>"));
        let texts = placed_texts(&tree["pages"][0]);
        let contents: Vec<&str> = texts.iter().map(|t| t.2.as_str()).collect();
        assert_eq!(contents, vec!["fn a() {", "    b();", "}"]);
        // The blank line takes a line of its own: the closing brace is three lines below the first line.
        let line_h = 11.0 * 1.2;
        assert!((texts[2].1 - texts[0].1 - 3.0 * line_h as f64).abs() < 0.05, "{texts:?}");
    }

    #[test]
    fn a_code_is_set_in_courier_on_a_tint_with_padding() {
        let tree = engine_render(&minimal("<code>x</code>"));
        let page = &tree["pages"][0];
        let boxes = filled_boxes(page, "#f3f4f6");
        assert_eq!(boxes.len(), 1, "one tinted box");
        let texts = placed_texts(page);
        assert_eq!(texts[0].2, "x");
        assert!(page.to_string().contains("\"font\":\"Courier\""));
        // 8pt of padding above the line, and the box is the line and the padding below it.
        assert!((boxes[0].1 - (11.0 * 1.2 + 16.0)).abs() < 0.05, "{boxes:?}");
        assert!((texts[0].1 - boxes[0].0 - 8.0).abs() < 0.05, "{texts:?} {boxes:?}");
    }

    #[test]
    fn a_tab_fills_to_the_next_multiple_of_the_tab_size() {
        assert_eq!(first_page_lines("<code>\n\tx\nab\tc\n</code>"), vec!["    x", "ab  c"]);
        assert_eq!(first_page_lines("<code tab-size=\"8\">\n\tx\nab\tc\n</code>"), vec!["        x", "ab      c"]);
        assert_eq!(first_page_lines("<code tab-size=\"2\">\n\tx\nabc\td\n</code>"), vec!["  x", "abc d"]);
    }

    #[test]
    fn a_line_wider_than_the_box_wraps_at_a_space() {
        // Courier at 11pt is 6.6pt a character, and 100pt less 16pt of padding leaves room for 12.
        let lines = first_page_lines(r#"<code width="100pt">alpha beta gamma delta</code>"#);
        assert_eq!(lines, vec!["alpha beta", "gamma delta"]);
    }

    #[test]
    fn a_word_wider_than_a_line_breaks_at_the_edge() {
        let word = "x".repeat(30);
        let lines = first_page_lines(&format!(r#"<code width="100pt">{word}</code>"#));
        assert_eq!(lines.iter().map(String::len).collect::<Vec<_>>(), vec![12, 12, 6]);
        assert_eq!(lines.concat(), word, "nothing is lost");
    }

    #[test]
    fn a_word_that_fits_a_line_of_its_own_moves_down_instead_of_breaking() {
        let lines = first_page_lines(r#"<code width="100pt">abcdef ghijklmnopq</code>"#);
        assert_eq!(lines, vec!["abcdef", "ghijklmnopq"]);
    }

    #[test]
    fn a_word_with_no_room_left_on_its_line_breaks_where_the_line_ends() {
        // "ab " leaves room for nine characters, and the word is longer than a whole line.
        let lines = first_page_lines(&format!(r#"<code width="100pt">ab {}</code>"#, "y".repeat(20)));
        assert_eq!(lines, vec!["ab yyyyyyyyy", "yyyyyyyyyyy"]);
    }

    #[test]
    fn the_indent_and_the_tabs_before_a_wrap_are_kept_and_the_break_is_not_drawn() {
        let lines = first_page_lines(r#"<code width="100pt">  one two three</code>"#);
        assert_eq!(lines, vec!["  one two", "three"]);
    }

    #[test]
    fn a_span_colors_a_stretch_of_a_line_of_code() {
        let tree = engine_render(&minimal("<code>let <span color=\"#c0392b\">x</span> = 1;</code>"));
        let texts = placed_texts(&tree["pages"][0]);
        assert_eq!(texts.iter().map(|t| t.2.as_str()).collect::<Vec<_>>(), vec!["let ", "x", " = 1;"]);
        // Each stretch starts where the one before it ends: four characters of 6.6pt, then one.
        assert!((texts[1].0 - texts[0].0 - 4.0 * 6.6).abs() < 0.05, "{texts:?}");
        assert!((texts[2].0 - texts[1].0 - 6.6).abs() < 0.05, "{texts:?}");
        assert!(tree["pages"][0].to_string().contains("#c0392b"));
    }

    /// The height of a frame that leaves room for `room` lines of a code on a page, and half a line more. The box
    /// has 8pt of padding above its lines and below them, and a page holds 785.89pt.
    fn frame_before_code(room: usize) -> f32 {
        let line_h = 11.0_f32 * 1.2;
        785.89 - 16.0 - line_h * room as f32 - line_h / 2.0
    }

    /// A code of `lines` lines, `L001` and on, after a frame that leaves room for `room` of them on the first page.
    fn code_pages(attrs: &str, lines: usize, room: usize) -> Vec<Vec<String>> {
        let code: Vec<String> = (1..=lines).map(|n| format!("L{n:03}")).collect();
        let body = format!(r#"<frame height="{:.2}pt"/><code {attrs}>{}</code>"#, frame_before_code(room), code.join("\n"));
        let tree = engine_render(&minimal(&body));
        tree["pages"].as_array().unwrap().iter().map(find_text_content).collect()
    }

    #[test]
    fn a_code_taller_than_the_page_splits_between_lines_and_loses_none() {
        let tree = engine_render(&minimal(&format!(
            "<code>{}</code>",
            (1..=150).map(|n| format!("L{n:03}")).collect::<Vec<_>>().join("\n")
        )));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2, "it needs more than one page");
        let all: Vec<String> = pages.iter().flat_map(find_text_content).collect();
        assert_eq!(all, (1..=150).map(|n| format!("L{n:03}")).collect::<Vec<_>>());
        // Every page that has lines of it has its own tinted box, padded at the top and the bottom.
        for page in pages {
            let boxes = filled_boxes(page, "#f3f4f6");
            assert_eq!(boxes.len(), 1, "a box for the lines on the page");
            let lines = placed_texts(page);
            let (box_y, box_h) = boxes[0];
            assert!((lines[0].1 - box_y - 8.0).abs() < 0.05, "padded at the top: {lines:?} {boxes:?}");
            let last_bottom = lines.last().unwrap().1 + 11.0 * 1.2;
            assert!((box_y + box_h - last_bottom - 8.0).abs() < 0.05, "padded at the bottom: {boxes:?}");
        }
    }

    #[test]
    fn a_code_keeps_widows_and_orphans_like_a_text() {
        // Room for 5 of 6 lines leaves 1 for the next page, and by default it gets 2.
        assert_eq!(code_pages("", 6, 5).iter().map(Vec::len).collect::<Vec<_>>(), vec![4, 2]);
        // With the protection off, it stays alone.
        assert_eq!(
            code_pages(r#"widows="1" orphans="1""#, 6, 5).iter().map(Vec::len).collect::<Vec<_>>(),
            vec![5, 1]
        );
    }

    #[test]
    fn the_lines_of_a_wrapped_line_stay_together_in_order_across_a_split() {
        // Two long lines, each wrapping to several, and room for the first page to end in the middle of the second.
        let long = "word ".repeat(40);
        let body = format!("<frame height=\"700pt\"/><code width=\"200pt\">{long}\n{long}</code>");
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2);
        let words: Vec<String> = pages.iter().flat_map(find_text_content).flat_map(|l| {
            l.split_whitespace().map(str::to_string).collect::<Vec<_>>()
        }).collect();
        assert_eq!(words.len(), 80, "every word is drawn once");
        assert!(words.iter().all(|w| w == "word"));
    }

    #[test]
    fn a_blank_line_that_a_split_leaves_at_the_top_of_a_page_is_kept() {
        // L1, blank, blank, L4 with room for two lines: the second page starts with the second blank line.
        let tree = engine_render(&minimal(&format!(
            "<frame height=\"{:.2}pt\"/><code widows=\"1\" orphans=\"1\">L1\n\n\nL4</code>",
            frame_before_code(2)
        )));
        let all: Vec<String> = tree["pages"].as_array().unwrap().iter().flat_map(find_text_content).collect();
        assert_eq!(all, vec!["L1", "L4"]);
        let second = placed_texts(&tree["pages"][1]);
        let box_y = filled_boxes(&tree["pages"][1], "#f3f4f6")[0].0;
        assert!((second[0].1 - box_y - 8.0 - 13.2).abs() < 0.05, "one blank line above L4: {second:?}");
    }

    #[test]
    fn a_code_span_is_drawn_in_courier_on_a_tint_inside_a_text() {
        let tree = engine_render(&minimal("<text>Run <code-span>cargo test</code-span> now</text>"));
        let page = &tree["pages"][0];
        let boxes = filled_boxes(page, "#f3f4f6");
        assert_eq!(boxes.len(), 1);
        let texts = placed_texts(page);
        assert!(texts.iter().any(|t| t.2 == "cargo test"));
        assert!(page.to_string().contains("\"font\":\"Courier\""));
    }

    #[test]
    fn a_code_span_takes_its_own_font_color_and_tint() {
        let tree = engine_render(&minimal(
            r##"<text>Run <code-span font="Times-Roman" color="#336699" background="#ffff00">it</code-span></text>"##,
        ));
        let page = &tree["pages"][0];
        assert_eq!(filled_boxes(page, "#ffff00").len(), 1);
        let json = page.to_string();
        assert!(json.contains("\"font\":\"Times-Roman\""));
        assert!(json.contains("#336699"));
    }

    #[test]
    fn a_code_span_stays_with_its_words_when_a_text_splits_across_pages() {
        // The text splits between lines on the page edge; the tint must follow the code-span into the rest.
        let words = "alpha ".repeat(2000);
        let tree = engine_render(&minimal(&format!("<text>{words}<code-span>kept</code-span> end</text>")));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 2);
        let last = pages.last().unwrap();
        assert_eq!(filled_boxes(last, "#f3f4f6").len(), 1, "the tint is on the page that has the code-span");
    }

    #[test]
    fn a_code_in_a_row_takes_the_width_of_its_longest_line() {
        let tree = engine_render(&minimal(
            r#"<row><code>ab
abcdef</code><text>after</text></row>"#,
        ));
        let boxes = filled_boxes(&tree["pages"][0], "#f3f4f6");
        // Six characters of 6.6pt and 8pt of padding on each side.
        assert!((boxes[0].1 - (2.0 * 13.2 + 16.0)).abs() < 0.05);
        let width = tree["pages"][0]["nodes"].to_string();
        assert!(width.contains("\"width\":55.6"), "{width}");
    }

    // ── line-height in the flow ───────────────────────────────────────────────

    #[test]
    fn line_height_sets_the_distance_between_the_lines_of_a_flow_text_and_its_height() {
        let height = |attrs: &str| {
            let (b, lines, _) = flow_text(attrs, CAPPED_WORDS);
            (lines.len(), b["height"].as_f64().unwrap())
        };
        let (lines, plain) = height("");
        assert!(lines > 2, "the text wraps");
        assert_eq!(plain, 12.0 * lines as f64, "1.2 × the 10pt font size, as before");
        assert_eq!(height(r#"line-height="2""#), (lines, 20.0 * lines as f64));
        assert_eq!(height(r#"line-height="1""#), (lines, 10.0 * lines as f64));
        assert_eq!(height(r#"line-height="m""#), (lines, plain), "m is the default factor");
        assert_eq!(height(r#"line-height="l""#), (lines, 14.0 * lines as f64), "l is 1.4");
    }

    #[test]
    fn line_height_places_each_line_of_a_flow_text_one_line_height_below_the_last() {
        let tree = engine_render(&minimal(&format!(r#"<text width="90pt" font-size="10pt" line-height="2">{CAPPED_WORDS}</text>"#)));
        let runs = tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap().clone();
        let mut ys: Vec<f64> = runs.iter().map(|r| r["y"].as_f64().unwrap()).collect();
        ys.dedup();
        assert!(ys.len() > 2, "{ys:?}");
        for pair in ys.windows(2) {
            assert_eq!(pair[1] - pair[0], 20.0, "{ys:?}");
        }
    }

    #[test]
    fn a_line_height_token_follows_the_documents_row() {
        let xml = |attr: &str| {
            format!(
                r#"<lpdf version="1"><tokens><line-height xs="1" s="1.5" m="2" l="2.5" xl="3" xxl="4"/></tokens><document size="a4"><section><layout margin="28pt">
                    <text width="90pt" font-size="10pt" {attr}>alpha beta gamma delta epsilon</text></layout></section></document></lpdf>"#
            )
        };
        let height = |attr: &str| engine_render(&xml(attr))["pages"][0]["nodes"][0]["height"].as_f64().unwrap();
        let lines = height(r#"line-height="1""#) / 10.0;
        assert_eq!(height(r#"line-height="xl""#), 30.0 * lines, "xl is 3 in this row");
        assert_eq!(height(r#"line-height="3""#), 30.0 * lines, "a number is the factor itself");
        assert_eq!(height(""), 12.0 * lines, "a text without line-height does not follow m");
    }

    #[test]
    fn a_text_splits_across_pages_by_its_own_line_height() {
        // The 11pt text is 22pt a line at line-height 2, and the frame leaves room for `room` such lines.
        let per_page = |attrs: &str, lines: usize, room: usize| -> Vec<usize> {
            page_lines_of(attrs, lines, room, 22.0).iter().map(Vec::len).collect()
        };
        assert_eq!(per_page(r#"line-height="2""#, 8, 4), vec![4, 4]);
        assert_eq!(per_page(r#"line-height="2""#, 8, 6), vec![6, 2], "widows keep two for the next page");
        // The same frame holds more lines that are closer together: 99pt is 4 lines of 22pt or 9 of 11pt.
        assert_eq!(per_page(r#"line-height="1""#, 12, 4), vec![9, 3]);
    }

    #[test]
    fn a_capped_text_splits_by_its_own_line_height() {
        let pages = page_lines_of(r#"line-height="2" max-lines="6""#, 8, 4, 22.0);
        assert_eq!(pages.iter().map(Vec::len).collect::<Vec<_>>(), vec![4, 2], "{pages:?}");
    }

    // ── max-lines and ellipsis in the flow ────────────────────────────────────

    const CAPPED_WORDS: &str = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";

    /// The text box of `<text {attrs}>` at 90pt and 10pt, followed by a text "After": the box, the content of
    /// each of its lines, and where "After" starts.
    fn flow_text(attrs: &str, content: &str) -> (serde_json::Value, Vec<String>, f64) {
        let tree = engine_render(&minimal(&format!(
            r#"<text width="90pt" font-size="10pt" {attrs}>{content}</text><text>After</text>"#
        )));
        let nodes = tree["pages"][0]["nodes"].as_array().unwrap();
        let b = nodes[0].clone();
        let mut lines: Vec<(f64, String)> = Vec::new();
        for run in b["nodes"].as_array().unwrap() {
            let (y, text) = (run["y"].as_f64().unwrap(), run["content"].as_str().unwrap().to_string());
            match lines.last_mut() {
                Some((last_y, line)) if *last_y == y => { line.push(' '); line.push_str(&text); }
                _ => lines.push((y, text)),
            }
        }
        let after = nodes[1]["y"].as_f64().unwrap();
        (b, lines.into_iter().map(|(_, line)| line).collect(), after)
    }

    // ── Opacity of a box ──────────────────────────────────────────────────────

    #[test]
    fn a_faded_box_carries_its_opacity_in_the_render_tree() {
        let tree = engine_render(&minimal(
            r##"<column opacity="0.5" background="#cccccc"><row opacity="0.25"><text>In</text></row></column><column><text>Out</text></column>"##,
        ));
        let nodes = tree["pages"][0]["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["opacity"], 0.5);
        assert_eq!(nodes[0]["nodes"][0]["opacity"], 0.25);
        assert!(nodes[1].get("opacity").is_none(), "an opaque box says nothing: {}", nodes[1]);
    }

    #[test]
    fn a_box_with_a_shadow_carries_it_in_the_render_tree() {
        let tree = engine_render(&minimal(
            r##"<column shadow="2pt 3pt #112233 0.5" background="#cccccc"><text>In</text></column><column><text>Out</text></column>"##,
        ));
        let nodes = tree["pages"][0]["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["shadow"], serde_json::json!({ "x": 2.0, "y": 3.0, "color": "#112233", "opacity": 0.5 }));
        assert!(nodes[1].get("shadow").is_none(), "a box without a shadow says nothing: {}", nodes[1]);
    }

    #[test]
    fn a_table_cell_with_a_shadow_carries_it() {
        let tree = engine_render(&minimal(
            r##"<table cols="1fr 1fr"><tr><td shadow="2pt 3pt #112233"><text>A</text></td><td><text>B</text></td></tr></table>"##,
        ));
        let table = tree["pages"][0]["nodes"][0].to_string();
        assert_eq!(table.matches("\"shadow\"").count(), 1, "{table}");
    }

    #[test]
    fn a_box_with_a_shadow_that_splits_across_pages_carries_it_on_each_part() {
        let lines: String = (1..=120).map(|n| format!("<text>Line {n}</text>")).collect();
        let tree = engine_render(&minimal(&format!(r##"<column shadow="2pt 3pt #112233">{lines}</column>"##)));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() > 1);
        for page in pages {
            assert_eq!(page["nodes"][0]["shadow"]["x"], 2.0, "{}", page["nodes"][0]);
        }
    }

    #[test]
    fn a_faded_box_that_splits_across_pages_fades_each_part() {
        let lines: String = (1..=120).map(|n| format!("<text>Line {n}</text>")).collect();
        let tree = engine_render(&minimal(&format!(r#"<column opacity="0.5">{lines}</column>"#)));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() > 1, "the column fills more than a page");
        for page in pages {
            assert_eq!(page["nodes"][0]["opacity"], 0.5, "{}", page["nodes"][0]);
        }
    }

    #[test]
    fn a_faded_table_cell_carries_its_opacity() {
        let tree = engine_render(&minimal(
            r#"<table cols="1fr 1fr"><tr><td opacity="0.5"><text>A</text></td><td><text>B</text></td></tr></table>"#,
        ));
        let table = &tree["pages"][0]["nodes"][0];
        let cells: Vec<&serde_json::Value> = table["nodes"].as_array().unwrap().iter().filter(|n| n["type"] == "box").collect();
        assert_eq!(cells.iter().filter(|c| c["opacity"] == 0.5).count(), 1, "{table}");
    }

    #[test]
    fn max_lines_stops_a_flow_text_after_its_lines_and_counts_only_those() {
        let end = |b: &serde_json::Value| b["y"].as_f64().unwrap() + b["height"].as_f64().unwrap();
        let (whole, lines, after_whole) = flow_text("", CAPPED_WORDS);
        assert!(lines.len() > 3, "the text wraps to more lines than the cap: {lines:?}");
        assert_eq!(whole["height"].as_f64().unwrap(), 12.0 * lines.len() as f64);

        let (capped, lines, after) = flow_text(r#"max-lines="2""#, CAPPED_WORDS);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(capped["height"].as_f64().unwrap(), 24.0, "only the lines drawn take room");
        assert_eq!(after - end(&capped), after_whole - end(&whole), "the next text follows the lines drawn");
        assert!(!lines.join(" ").contains("lambda"), "the text past the cap is dropped: {lines:?}");
        assert!(!lines.join(" ").ends_with("..."), "without ellipsis it stops at the end of a line: {lines:?}");
    }

    #[test]
    fn max_lines_above_what_a_flow_text_needs_changes_nothing() {
        assert_eq!(flow_text(r#"max-lines="20""#, CAPPED_WORDS).0, flow_text("", CAPPED_WORDS).0);
    }

    #[test]
    fn ellipsis_ends_a_capped_flow_text_in_dots_within_its_width() {
        let (_, lines, _) = flow_text(r#"max-lines="2" ellipsis="true""#, CAPPED_WORDS);
        assert_eq!(lines.len(), 2, "{lines:?}");
        let last = lines.last().unwrap();
        assert!(last.ends_with("..."), "{last}");
        assert!(text_width("Helvetica", last, 10.0) <= 90.01, "the line and its dots fit the width: {last}");
    }

    #[test]
    fn ellipsis_without_max_lines_changes_nothing_in_the_flow() {
        // The last word is wider than the box, which the text of a layer would end in dots.
        let content = format!("{CAPPED_WORDS} Incomprehensibilities");
        let (plain, lines, _) = flow_text("", &content);
        assert_eq!(flow_text(r#"ellipsis="true""#, &content).0, plain);
        // It is broken at the last character that fits, in no dots, and every line of it is within the box.
        assert!(lines.last().unwrap().len() < "Incomprehensibilities".len(), "the long word is broken: {lines:?}");
        assert!(lines.join("").contains("Incomprehensibilities") && !lines.join("").contains("..."), "{lines:?}");
        assert!(lines.iter().all(|line| text_width("Helvetica", line, 10.0) <= 90.01), "no line runs past the box: {lines:?}");
    }

    #[test]
    fn a_capped_flow_text_that_fits_its_page_is_not_split() {
        assert_eq!(lines_per_page(r#"max-lines="3""#, 8, 3), vec![3]);
    }

    #[test]
    fn a_capped_flow_text_splits_across_pages_within_its_cap() {
        // 5 of the 8 lines are kept and the first page has room for 3: 2 go on, not the 5 an uncapped text would take.
        assert_eq!(lines_per_page("", 8, 3), vec![3, 5]);
        assert_eq!(lines_per_page(r#"max-lines="5""#, 8, 3), vec![3, 2]);
        // Widows count within the cap: 5 kept lines with room for 4 give the next page 2.
        assert_eq!(lines_per_page(r#"max-lines="5""#, 8, 4), vec![3, 2]);

        let pages = page_lines(r#"max-lines="5" ellipsis="true""#, 8, 3);
        let last = pages[1].last().unwrap();
        assert!(last.starts_with("L05") && last.ends_with("..."), "the last line kept ends in dots: {pages:?}");
        assert!(pages.iter().flatten().all(|line| !line.ends_with("...") || line == last), "{pages:?}");
    }

    /// Helper: collect every "content" string of text nodes in a page tree.
    fn find_text_content(page: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(v: &serde_json::Value, out: &mut Vec<String>) {
            if v["type"] == "text" {
                if let Some(s) = v["content"].as_str() { out.push(s.to_string()); }
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, out); }
            }
        }
        walk(page, &mut out);
        out
    }

    // ── Debug overlay tests ───────────────────────────────────────────────────

    /// A column with debug="true" containing two frames produces:
    /// - the original 2 frame children
    /// - 4 red self-outline lines appended after content
    /// - 8 blue child-outline lines (4 per frame)
    #[test]
    fn debug_overlay_golden() {
        let body = r##"<column debug="true" gap="0pt">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" />
        </column>"##;
        let tree = engine_render(&minimal(body));
        let column = &tree["pages"][0]["nodes"][0];
        assert_eq!(column["type"], "box");
        let children = column["nodes"].as_array().unwrap();
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
        let body_no_debug = r##"<column gap="m">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" />
        </column>"##;
        let body_debug = r##"<column debug="true" gap="m">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" />
        </column>"##;

        let t_clean = engine_render(&minimal(body_no_debug));
        let t_debug = engine_render(&minimal(body_debug));

        let column_clean = &t_clean["pages"][0]["nodes"][0];
        let column_debug = &t_debug["pages"][0]["nodes"][0];

        // Outer box geometry identical
        assert_eq!(column_clean["x"], column_debug["x"]);
        assert_eq!(column_clean["y"], column_debug["y"]);
        assert_eq!(column_clean["width"], column_debug["width"]);
        assert_eq!(column_clean["height"], column_debug["height"]);

        // Frame children geometry identical
        for i in 0..2 {
            let c_clean = &column_clean["nodes"][i];
            let c_debug = &column_debug["nodes"][i];
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
        let body = r##"<column debug="true" gap="0pt">
            <frame height="20pt" background="#aaa" />
            <frame height="30pt" background="#bbb" debug="true" />
        </column>"##;
        let tree = engine_render(&minimal(body));
        let column = &tree["pages"][0]["nodes"][0];
        let children = column["nodes"].as_array().unwrap();

        // column: 2 frames + 4 red self + 4 blue (only frame[0], not frame[1] since it's debug_self)
        assert_eq!(children.len(), 10, "expected 10 children (2 frames + 4 red + 4 blue), got {}", children.len());

        // red self-lines for column
        for i in 2..6 {
            assert_eq!(children[i]["color"], "#ff0033", "child[{i}] should be red column self-line");
        }
        // blue child-lines only for frame[0] (4 lines)
        for i in 6..10 {
            assert_eq!(children[i]["color"], "#0066ff", "child[{i}] should be blue child-line");
        }

        // The flagged frame child (index 1) must have its own 4 red self-lines
        let frame2 = &children[1];
        let frame2_children = frame2["nodes"].as_array().unwrap();
        assert_eq!(frame2_children.len(), 4, "flagged frame should have 4 self-lines");
        for i in 0..4 {
            assert_eq!(frame2_children[i]["color"], "#ff0033", "frame2 child[{i}] should be red");
        }

        // The unflagged frame child (index 0) must have no children
        let frame1 = &children[0];
        assert!(frame1["nodes"].as_array().map(|a| a.is_empty()).unwrap_or(true),
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
                r#"<lpdf version="1"><document size="{size}"><section><layout margin="0pt"/></section></document></lpdf>"#
            );
            let tree = engine_render(&xml);
            let w = tree["pages"][0]["width"].as_f64().unwrap();
            let h = tree["pages"][0]["height"].as_f64().unwrap();
            assert!((w - exp_w).abs() < 0.1, "{size}: width {w} != {exp_w}");
            assert!((h - exp_h).abs() < 0.1, "{size}: height {h} != {exp_h}");
        }
    }

    // ── Row: fill and between ─────────────────────────────────────────────────

    /// The x, y and width of each child of the first node of the first page.
    fn child_boxes(body: &str) -> Vec<(f64, f64, f64)> {
        let tree = engine_render(&minimal(body));
        tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap().iter()
            .map(|c| (c["x"].as_f64().unwrap(), c["y"].as_f64().unwrap(), c["width"].as_f64().unwrap()))
            .collect()
    }

    const MARGIN: f64 = 28.0;
    const AVAIL_W: f64 = 595.28 - 2.0 * MARGIN;

    #[test]
    fn row_fill_child_takes_the_width_the_others_leave() {
        let boxes = child_boxes(r#"<row gap="0pt"><frame width="100pt" height="20pt" /><frame width="fill" height="20pt" /></row>"#);
        let [(x0, _, w0), (x1, _, w1)] = boxes[..] else { panic!("{boxes:?}") };
        assert!((x0 - MARGIN).abs() < 0.1, "x0={x0}");
        assert!((w0 - 100.0).abs() < 0.1, "w0={w0}");
        assert!((x1 - (MARGIN + 100.0)).abs() < 0.1, "x1={x1}");
        assert!((w1 - (AVAIL_W - 100.0)).abs() < 0.5, "w1={w1}");
    }

    #[test]
    fn row_fill_child_first_pushes_the_others_to_the_right() {
        let boxes = child_boxes(r#"<row gap="0pt"><frame width="fill" height="20pt" /><frame width="100pt" height="20pt" /></row>"#);
        let [(x0, _, w0), (x1, _, w1)] = boxes[..] else { panic!("{boxes:?}") };
        assert!((x0 - MARGIN).abs() < 0.1, "x0={x0}");
        assert!((w0 - (AVAIL_W - 100.0)).abs() < 0.5, "w0={w0}");
        assert!((x1 - (MARGIN + AVAIL_W - 100.0)).abs() < 0.5, "x1={x1}");
        assert!((w1 - 100.0).abs() < 0.1, "w1={w1}");
    }

    #[test]
    fn row_fill_child_does_not_wrap() {
        // 500pt leave 39.28pt on the line: the fill child takes them rather than a line of its own.
        let boxes = child_boxes(r#"<row gap="0pt"><frame width="500pt" height="20pt" /><frame width="fill" height="20pt" /></row>"#);
        let [(_, y0, _), (x1, y1, w1)] = boxes[..] else { panic!("{boxes:?}") };
        assert_eq!(y0, y1, "the fill child is on the first line");
        assert!((x1 - (MARGIN + 500.0)).abs() < 0.1, "x1={x1}");
        assert!((w1 - (AVAIL_W - 500.0)).abs() < 0.5, "w1={w1}");
    }

    #[test]
    fn row_fill_children_share_what_their_line_leaves() {
        // The second 400pt frame wraps, and the two fill children share what it leaves on the second line.
        let boxes = child_boxes(r#"<row gap="0pt">
            <frame width="400pt" height="20pt" /><frame width="400pt" height="20pt" />
            <frame width="fill" height="20pt" /><frame width="fill" height="20pt" />
        </row>"#);
        let [_, (x1, y1, _), (x2, y2, w2), (x3, y3, w3)] = boxes[..] else { panic!("{boxes:?}") };
        assert!((y1 - (MARGIN + 20.0)).abs() < 0.1, "y1={y1}");
        assert_eq!((y2, y3), (y1, y1), "the fill children are on the second line");
        let share = (AVAIL_W - 400.0) / 2.0;
        assert!((w2 - share).abs() < 0.5 && (w3 - share).abs() < 0.5, "w2={w2} w3={w3}");
        assert!((x2 - (x1 + 400.0)).abs() < 0.1 && (x3 - (x2 + w2)).abs() < 0.1, "x2={x2} x3={x3}");
    }

    #[test]
    fn row_justify_between_puts_the_children_at_the_edges() {
        let boxes = child_boxes(r#"<row justify="between"><frame width="100pt" height="20pt" /><frame width="50pt" height="20pt" /><frame width="100pt" height="20pt" /></row>"#);
        let [(x0, _, _), (x1, _, _), (x2, _, w2)] = boxes[..] else { panic!("{boxes:?}") };
        assert!((x0 - MARGIN).abs() < 0.1, "x0={x0}");
        assert!((x2 + w2 - (MARGIN + AVAIL_W)).abs() < 0.1, "the last child ends at the right edge, x2={x2}");
        let middle = MARGIN + 100.0 + (AVAIL_W - 250.0) / 2.0;
        assert!((x1 - middle).abs() < 0.1, "the middle child is halfway between, x1={x1}");
    }

    // ── Span decoration ───────────────────────────────────────────────────────

    #[test]
    fn span_underline_emits_line_decoration() {
        let body = r#"<text><span underline="true">hello</span></text>"#;
        let tree = engine_render(&minimal(body));
        let kids = tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap();
        // span atom produces a text node + an underline line node
        let types: Vec<&str> = kids.iter().filter_map(|k| k["type"].as_str()).collect();
        assert!(types.contains(&"text"), "no text node; got {:?}", types);
        assert!(types.contains(&"line"), "no underline line; got {:?}", types);
    }

    #[test]
    fn span_strike_emits_line_decoration() {
        let body = r#"<text><span strike="true">hello</span></text>"#;
        let tree = engine_render(&minimal(body));
        let kids = tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap();
        let types: Vec<&str> = kids.iter().filter_map(|k| k["type"].as_str()).collect();
        assert!(types.contains(&"text"), "no text node; got {:?}", types);
        assert!(types.contains(&"line"), "no strikethrough line; got {:?}", types);
    }

    #[test]
    fn span_href_wraps_in_link_node() {
        let body = r#"<text><span href="https://example.com">click here</span></text>"#;
        let tree = engine_render(&minimal(body));
        let kids = tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap();
        let link = kids.iter().find(|k| k["type"] == "link").expect("no link node in children");
        assert_eq!(link["url"], "https://example.com");
        assert!(!link["nodes"].as_array().unwrap().is_empty());
    }

    // ── Custom document tokens ────────────────────────────────────────────────

    #[test]
    fn custom_tokens_space_scale_overrides_default() {
        // Override the "m" space token to 50pt. A column gap="m" should produce
        // a 50pt gap, not the default 8pt.
        let xml = r#"<lpdf version="1">
            <tokens>
                <space xs="1pt" s="2pt" m="50pt" l="100pt" xl="200pt" xxl="400pt" />
            </tokens>
            <document size="a4">
                <section><layout margin="0pt">
                    <column gap="m">
                        <frame height="20pt" />
                        <frame height="20pt" />
                    </column>
                </layout></section>
            </document>
        </lpdf>"#;
        let tree = engine_render(xml);
        let children = tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap();
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
            <document size="a4">
                <section><layout margin="0pt">
                    <frame height="20pt" background="primary" />
                </layout></section>
            </document>
        </lpdf>"##;
        let tree = engine_render(xml);
        let fill = tree["pages"][0]["nodes"][0]["fill"].as_str().unwrap();
        assert_eq!(fill, "#abcdef");
    }

    // ── Bookmarks ─────────────────────────────────────────────────────────────

    fn render_pages(xml: &str) -> Vec<RenderPage> {
        layout_document(&parse(xml).unwrap().section_layouts())
    }

    // ── Layers ────────────────────────────────────────────────────────────────

    #[test]
    fn layers_before_the_layout_paint_under_it_and_layers_after_it_over_it() {
        let lines: String = (1..=120).map(|n| format!("<text>Line {n}</text>")).collect();
        let pages = render_pages(&format!(
            r##"<lpdf version="1"><document size="a4"><section>
                <layer opacity="0.1"><rect width="10pt" height="10pt" fill="#000000"/></layer>
                <layer opacity="0.2"><rect width="10pt" height="10pt" fill="#000000"/></layer>
                <layout margin="28pt">{lines}</layout>
                <layer opacity="0.3"><rect width="10pt" height="10pt" fill="#000000"/></layer>
                <layer opacity="0.4" page="last"><rect width="10pt" height="10pt" fill="#000000"/></layer>
            </section></document></lpdf>"##
        ));
        assert!(pages.len() > 1, "the layout should run to several pages");
        for (i, page) in pages.iter().enumerate() {
            let content: Vec<usize> = page.nodes.iter().enumerate()
                .filter_map(|(k, n)| (!matches!(n, RenderNode::CanvasLayer(_))).then_some(k))
                .collect();
            let layers: Vec<(usize, f32)> = page.nodes.iter().enumerate()
                .filter_map(|(k, n)| match n { RenderNode::CanvasLayer(l) => Some((k, l.opacity)), _ => None })
                .collect();
            let expected: &[f32] = if i + 1 == pages.len() { &[0.1, 0.2, 0.3, 0.4] } else { &[0.1, 0.2, 0.3] };
            assert_eq!(layers.iter().map(|l| l.1).collect::<Vec<_>>(), expected, "page {i}");
            let (first, last) = (content[0], content[content.len() - 1]);
            assert!(layers[..2].iter().all(|l| l.0 < first), "page {i}: a layer before the layout is under it");
            assert!(layers[2..].iter().all(|l| l.0 > last), "page {i}: a layer after the layout is over it");
        }
    }

    /// The page index and top of the first text whose content is `content`.
    fn text_at(pages: &[RenderPage], content: &str) -> (usize, f32) {
        fn find(nodes: &[RenderNode], content: &str) -> Option<f32> {
            nodes.iter().find_map(|node| match node {
                RenderNode::Text(t) if t.content == content => Some(t.y),
                RenderNode::Box(b) => find(&b.children, content),
                RenderNode::Link(l) => find(&l.children, content),
                _ => None,
            })
        }
        pages.iter().enumerate()
            .find_map(|(i, page)| find(&page.nodes, content).map(|y| (i, y)))
            .unwrap_or_else(|| panic!("no text {content}"))
    }

    #[test]
    fn bookmark_marks_the_page_and_height_where_its_element_starts() {
        let lines: String = (1..=200).map(|n| {
            let mark = if n == 150 { r#"<bookmark label="Line 150"/>"# } else { "" };
            format!("{mark}<text>Line {n}</text>")
        }).collect();
        let pages = render_pages(&minimal(&lines));
        let anchors = crate::render::placed_anchors(&pages);
        assert_eq!(anchors.len(), 1);
        let (page, y) = text_at(&pages, "Line 150");
        assert!(page > 0, "the document needs more than a page for this test");
        assert_eq!(anchors[0].page, page);
        // The anchor is the top of the text's box; the text's first line starts there too.
        assert!((anchors[0].anchor.y - y).abs() < 15.0, "anchor {} vs text {y}", anchors[0].anchor.y);
    }

    #[test]
    fn bookmark_moves_to_the_next_page_with_its_element() {
        // The filler leaves no room for the framed text, which moves to the second page with its bookmark.
        let pages = render_pages(&minimal(
            r#"<frame height="770pt"/><bookmark label="Moved"/><frame height="100pt"><text>Moved</text></frame>"#,
        ));
        let anchors = crate::render::placed_anchors(&pages);
        assert_eq!(text_at(&pages, "Moved").0, 1);
        assert_eq!(anchors[0].page, 1);
        assert_eq!(anchors[0].anchor.y, 28.0, "the top margin of the second page");
    }

    #[test]
    fn bookmark_on_a_leaf_moves_with_it_when_its_container_shifts_it() {
        // A divider is a leaf, so its anchor sits in a box of its own around it. The row centers the short
        // column against the tall frame after laying it out, which shifts the box down.
        let pages = render_pages(&minimal(
            r#"<row align="center"><frame width="50pt" height="100pt"/><column width="fill"><bookmark label="Rule"/><divider/></column></row>"#,
        ));
        fn holder(nodes: &[RenderNode]) -> Option<(f32, f32)> {
            nodes.iter().find_map(|node| match node {
                RenderNode::Box(b) => b.children.iter()
                    .find_map(|c| match c { RenderNode::Anchor(a) => Some((b.y, a.y)), _ => None })
                    .or_else(|| holder(&b.children)),
                _ => None,
            })
        }
        let (box_y, anchor_y) = holder(&pages[0].nodes).expect("an anchor inside a box");
        assert!(box_y > 28.0 + 30.0, "the column is centered, at {box_y}");
        assert_eq!(anchor_y, box_y);
    }

    #[test]
    fn section_bookmark_marks_the_top_of_its_first_page() {
        let pages = render_pages(
            r#"<lpdf version="1"><document>
                <section><layout><text>One</text></layout></section>
                <section><bookmark label="Second"/><layout><text>Two</text></layout></section>
            </document></lpdf>"#,
        );
        let anchors = crate::render::placed_anchors(&pages);
        assert_eq!((anchors[0].page, anchors[0].anchor.y), (1, 0.0));
    }

    /// The width and height of the first barcode in a layout, as the render tree reports them.
    fn barcode_size(body: &str) -> (f64, f64) {
        fn find(nodes: &serde_json::Value) -> Option<(f64, f64)> {
            nodes.as_array()?.iter().find_map(|node| {
                if node["type"] == "barcode" {
                    Some((node["width"].as_f64()?, node["height"].as_f64()?))
                } else {
                    find(&node["nodes"])
                }
            })
        }
        let tree = engine_render(&minimal(body));
        find(&tree["pages"][0]["nodes"]).expect("a barcode in the tree")
    }

    #[test]
    fn a_qr_code_is_as_wide_and_as_high_as_its_width_or_its_height() {
        let qr = |size: &str| format!(r#"<barcode type="qr" data="x" {size}/>"#);
        assert_eq!(barcode_size(&qr(r#"width="60pt""#)), (60.0, 60.0));
        assert_eq!(barcode_size(&qr(r#"height="50pt""#)), (50.0, 50.0));
    }

    #[test]
    fn a_qr_code_given_both_takes_the_smaller_so_it_stays_square() {
        assert_eq!(barcode_size(r#"<barcode type="qr" data="x" width="90pt" height="50pt"/>"#), (50.0, 50.0));
        assert_eq!(barcode_size(r#"<barcode type="qr" data="x" width="40pt" height="70pt"/>"#), (40.0, 40.0));
    }

    #[test]
    fn a_qr_code_in_a_filled_slot_takes_its_size_from_its_height() {
        let row = r#"<row><barcode type="qr" data="x" width="fill" height="80pt"/><frame width="fill" height="10pt"/></row>"#;
        assert_eq!(barcode_size(row), (80.0, 80.0));
    }

    fn near(a: (f64, f64), b: (f64, f64)) {
        assert!((a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01, "{a:?} is not {b:?}");
    }

    #[test]
    fn a_code_128_with_no_size_is_as_wide_as_its_bars() {
        // "ABC-123": a start symbol, 7 characters, the check symbol and the stop: 11 × 9 + 13 modules of 1pt.
        assert_eq!(barcode_size(r#"<barcode type="code128" data="ABC-123"/>"#), (112.0, 40.0));
        assert_eq!(barcode_size(r#"<barcode type="code128" data="ABC-123" hrt="true"/>"#), (112.0, 52.0));
    }

    #[test]
    fn a_code_128_given_one_size_keeps_its_proportions() {
        let code = |attrs: &str| format!(r#"<barcode type="code128" data="ABC-123" {attrs}/>"#);
        assert_eq!(barcode_size(&code(r#"width="224pt""#)), (224.0, 80.0));
        assert_eq!(barcode_size(&code(r#"height="20pt""#)), (56.0, 20.0));
    }

    #[test]
    fn a_code_128_given_both_sizes_fits_inside_them() {
        let code = |attrs: &str| format!(r#"<barcode type="code128" data="ABC-123" {attrs}/>"#);
        // Wider than its proportions allow: the height sets the size.
        assert_eq!(barcode_size(&code(r#"width="200pt" height="40pt""#)), (112.0, 40.0));
        assert_eq!(barcode_size(&code(r#"width="200pt" height="20pt""#)), (56.0, 20.0));
        // Taller than its proportions allow: the width sets it.
        assert_eq!(barcode_size(&code(r#"width="56pt" height="100pt""#)), (56.0, 20.0));
    }

    #[test]
    fn the_digits_of_a_code_128_sit_inside_its_height() {
        let code = |attrs: &str| format!(r#"<barcode type="code128" data="ABC-123" {attrs}/>"#);
        assert_eq!(barcode_size(&code(r#"height="52pt" hrt="true""#)), (112.0, 52.0));
        // The digits come on top of the 40pt bars of a barcode with no size.
        assert_eq!(barcode_size(&code(r#"hrt="true""#)), (112.0, 52.0));
        // The box keeps the proportions it has with the digits in it.
        assert_eq!(barcode_size(&code(r#"width="224pt" hrt="true""#)), (224.0, 104.0));
    }

    #[test]
    fn an_ean_13_with_no_size_is_the_standard_symbol() {
        // 95 modules of 0.33mm, and bars 22.85mm high: about 88.9 × 64.8pt.
        near(barcode_size(r#"<barcode type="ean13" data="590123412345"/>"#), (88.87, 64.77));
        near(barcode_size(r#"<barcode type="ean13" data="590123412345" hrt="true"/>"#), (88.87, 76.77));
    }

    #[test]
    fn an_ean_13_given_one_size_keeps_its_proportions() {
        let code = |attrs: &str| format!(r#"<barcode type="ean13" data="590123412345" {attrs}/>"#);
        near(barcode_size(&code(r#"width="177.72pt""#)), (177.72, 129.54));
        near(barcode_size(&code(r#"height="32.385pt""#)), (44.43, 32.385));
    }

    #[test]
    fn an_ean_13_given_both_sizes_fits_inside_them() {
        let code = |attrs: &str| format!(r#"<barcode type="ean13" data="590123412345" {attrs}/>"#);
        // 120 × 60 is wider than the symbol: the height sets the size.
        near(barcode_size(&code(r#"width="120pt" height="60pt""#)), (82.32, 60.0));
        // 60 × 120 is taller: the width sets it.
        near(barcode_size(&code(r#"width="60pt" height="120pt""#)), (60.0, 43.73));
        // The digits are inside the box, and the box keeps the proportions with them.
        near(barcode_size(&code(r#"width="100pt" height="45pt" hrt="true""#)), (52.09, 45.0));
    }

    #[test]
    fn a_barcode_in_a_filled_slot_fits_inside_the_slot_and_its_height() {
        // The slot is wider than the 56pt a 20pt high Code 128 needs, so the height sets the size.
        let row = r#"<row><barcode type="code128" data="ABC-123" width="fill" height="20pt"/><frame width="fill" height="10pt"/></row>"#;
        assert_eq!(barcode_size(row), (56.0, 20.0));
    }

    // ── Elements at fixed places ──────────────────────────────────────────────

    /// The render tree of a document with the given layers and a layout of `lines` lines of text.
    fn layer_tree(layers: &str, lines: usize) -> serde_json::Value {
        let body: String = (1..=lines).map(|n| format!("<text>Line {n}</text>")).collect();
        engine_render(&format!(
            r#"<lpdf version="1"><document size="a4"><section>{layers}<layout margin="28pt">{body}</layout></section></document></lpdf>"#
        ))
    }

    /// The layers a page draws, from the bottom up.
    fn page_layers(tree: &serde_json::Value, page: usize) -> Vec<&serde_json::Value> {
        tree["pages"][page]["nodes"].as_array().unwrap().iter().filter(|n| n["type"] == "canvas-layer").collect()
    }

    /// Every text a node draws, as (content, font, size, colour).
    fn texts(node: &serde_json::Value) -> Vec<(String, String, f64, String)> {
        let mut out = Vec::new();
        fn walk(node: &serde_json::Value, out: &mut Vec<(String, String, f64, String)>) {
            if node["type"] == "text" {
                out.push((
                    node["content"].as_str().unwrap().to_string(),
                    node["font"].as_str().unwrap().to_string(),
                    node["size"].as_f64().unwrap(),
                    node["color"].as_str().unwrap().to_string(),
                ));
            }
            for child in node["nodes"].as_array().into_iter().flatten() {
                walk(child, out);
            }
        }
        walk(node, &mut out);
        out
    }

    #[test]
    fn a_text_at_is_drawn_like_the_same_text_in_the_layout() {
        let markup = r##"Plain <span font="Helvetica-Bold" color="#ff0000">bold red</span> and <span underline="true">underlined</span> text"##;
        let tree = layer_tree(&format!(r#"<layer><text-at x="40pt" y="40pt">{markup}</text-at></layer>"#), 0);
        let layer = page_layers(&tree, 0)[0];
        let flow = engine_render(&minimal(&format!("<text>{markup}</text>")));
        assert_eq!(texts(layer), texts(&flow["pages"][0]["nodes"][0]));
        assert_eq!(texts(layer).len(), 5, "every word and run is drawn: {:?}", texts(layer));
    }

    #[test]
    fn a_text_at_without_a_width_is_one_line_as_wide_as_its_text() {
        let long = "word ".repeat(40);
        let tree = layer_tree(&format!(r#"<layer><text-at x="40pt" y="40pt">{long}</text-at></layer>"#), 0);
        let layer = page_layers(&tree, 0)[0];
        let b = &layer["nodes"][0];
        assert!((b["height"].as_f64().unwrap() - 13.2).abs() < 0.01, "a line is 1.2 × the 11pt of text");
        let right = b["x"].as_f64().unwrap() + b["width"].as_f64().unwrap();
        assert!(right > 595.28, "it runs on past the page edge: {right}");
        assert!(layer["nodes"][0]["type"] == "box", "and is not clipped: {layer}");
        assert_eq!(layer["clip"], serde_json::Value::Null);
    }

    #[test]
    fn a_text_at_anchor_attaches_its_box_to_the_page_vertically_too() {
        let tree = layer_tree(
            r#"<layer>
                <text-at anchor="bottom-right" x="-10pt" y="-10pt" font-size="10pt">Corner</text-at>
                <text-at anchor="center" font-size="10pt" line-height="2">Middle</text-at>
            </layer>"#,
            0,
        );
        let layer = page_layers(&tree, 0)[0];
        let corner = &layer["nodes"][0];
        assert!((corner["x"].as_f64().unwrap() + corner["width"].as_f64().unwrap() - (595.28 - 10.0)).abs() < 0.02);
        assert!((corner["y"].as_f64().unwrap() + corner["height"].as_f64().unwrap() - (841.89 - 10.0)).abs() < 0.02);
        let middle = &layer["nodes"][1];
        assert_eq!(middle["height"].as_f64().unwrap(), 20.0, "a line is 2 × the font size high");
        assert!((middle["y"].as_f64().unwrap() + 10.0 - 841.89 / 2.0).abs() < 0.02, "its middle is the page's");
        assert!((middle["x"].as_f64().unwrap() + middle["width"].as_f64().unwrap() / 2.0 - 595.28 / 2.0).abs() < 0.02);
    }

    #[test]
    fn a_text_at_with_a_width_aligns_in_it() {
        let tree = layer_tree(
            r#"<layer><text-at x="100pt" y="40pt" width="200pt" align="right" font-size="10pt">Right</text-at></layer>"#,
            0,
        );
        let b = &page_layers(&tree, 0)[0]["nodes"][0];
        assert_eq!((b["x"].as_f64().unwrap(), b["width"].as_f64().unwrap()), (100.0, 200.0));
        let run = &b["nodes"][0];
        assert_eq!(run["text_align"], "right");
        assert_eq!(run["x"].as_f64().unwrap() + 0.0, 300.0, "a right-aligned run is anchored at the right edge of the box");
    }

    #[test]
    fn max_lines_wraps_a_text_at_within_its_width_and_drops_the_lines_after_them() {
        let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
        let lines = |max: &str| {
            let tree = layer_tree(
                &format!(r#"<layer><text-at x="40pt" y="40pt" width="90pt" {max} font-size="10pt">{words}</text-at></layer>"#),
                0,
            );
            let mut b = page_layers(&tree, 0)[0]["nodes"][0].clone();
            // A line past the width is cut off at it, by a layer of its own around the box.
            if b["type"] == "canvas-layer" { b = b["nodes"][0].clone(); }
            let mut ys: Vec<String> = b["nodes"].as_array().unwrap().iter().map(|t| t["y"].to_string()).collect();
            ys.dedup();
            (ys.len(), b["height"].as_f64().unwrap())
        };
        assert_eq!(lines(r#"max-lines="2""#), (2, 24.0));
        assert_eq!(lines(r#"max-lines="3""#), (3, 36.0));
        // One line, by default: it does not wrap.
        assert_eq!(lines(""), (1, 12.0));
    }

    #[test]
    fn a_text_at_line_past_its_width_is_clipped_to_its_box_unless_it_ends_in_dots() {
        let layer_of = |ellipsis: &str| {
            let tree = layer_tree(
                &format!(r#"<layer><text-at x="40pt" y="40pt" width="60pt" {ellipsis} font-size="10pt">A line that is far too long</text-at></layer>"#),
                0,
            );
            page_layers(&tree, 0)[0].clone()
        };
        let clipped = layer_of("");
        let inner = &clipped["nodes"][0];
        assert_eq!(inner["type"], "canvas-layer");
        assert_eq!(inner["clip"], serde_json::json!([40.0, 40.0, 60.0, 12.0]));
        assert!(texts(inner).iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(" ").contains("too long"));

        let dotted = layer_of(r#"ellipsis="true""#);
        assert_eq!(dotted["nodes"][0]["type"], "box", "nothing is left to cut off");
        let shown: String = texts(&dotted).iter().map(|t| t.0.clone()).collect::<Vec<_>>().join(" ");
        assert!(shown.ends_with("..."), "{shown}");
        assert!(!shown.contains("too long"), "{shown}");
        let width = text_width("Helvetica", &shown, 10.0);
        assert!(width <= 60.01, "the line and its dots fit the width: {width}");
    }

    #[test]
    fn ellipsis_ends_the_last_line_when_max_lines_drops_text() {
        let tree = layer_tree(
            r#"<layer><text-at x="40pt" y="40pt" width="90pt" max-lines="2" ellipsis="true" font-size="10pt">alpha beta gamma delta epsilon zeta eta theta iota</text-at></layer>"#,
            0,
        );
        let b = &page_layers(&tree, 0)[0]["nodes"][0];
        let runs = b["nodes"].as_array().unwrap();
        let last_y = runs.last().unwrap()["y"].clone();
        let last_line: String = runs.iter().filter(|r| r["y"] == last_y).map(|r| r["content"].as_str().unwrap()).collect::<Vec<_>>().join(" ");
        assert!(last_line.ends_with("..."), "{last_line}");
        assert!(text_width("Helvetica", &last_line, 10.0) <= 90.01);
    }

    #[test]
    fn page_and_pages_in_a_text_at_count_within_the_section() {
        let tree = layer_tree(
            r#"<layer page="each"><text-at x="40pt" y="800pt">Page {page} of {pages}</text-at></layer>"#,
            120,
        );
        let pages = tree["pages"].as_array().unwrap().len();
        assert!(pages >= 3, "{pages} pages");
        for page in 0..pages {
            let layer = page_layers(&tree, page)[0];
            let said: String = texts(layer).iter().map(|t| t.0.clone()).collect::<Vec<_>>().join(" ");
            assert_eq!(said, format!("Page {} of {}", page + 1, pages));
        }
    }

    // ── Fitting images and barcodes ───────────────────────────────────────────

    /// The render tree of `body` in a layout, and of `layers` over it, with the image `logo` loaded at `px`
    /// pixels, or not loaded when `px` is `None`.
    fn tree_with_logo(layers: &str, body: &str, px: Option<(u32, u32)>) -> serde_json::Value {
        let xml = format!(
            r#"<lpdf version="1"><assets><image name="logo"/></assets><document size="a4"><section>{layers}<layout margin="28pt">{body}</layout></section></document></lpdf>"#
        );
        let mut doc = parse(&xml).unwrap();
        let mut lp = doc.section_layouts();
        let meta: ImageMeta = px.into_iter().map(|px| ("logo".to_string(), px)).collect();
        for section in &mut lp {
            prefill_section_image_sizes(section, &meta);
        }
        let pages: Vec<_> = lp.iter().flat_map(layout_page).collect();
        let json = crate::render::pages_to_json(&pages, serde_json::Value::Null, serde_json::Value::Null);
        serde_json::from_str(&json).unwrap()
    }

    /// The x, y, width and height of the first image in a page, in the layout or in a layer.
    fn first_image(tree: &serde_json::Value) -> (f64, f64, f64, f64) {
        fn find(nodes: &serde_json::Value) -> Option<(f64, f64, f64, f64)> {
            nodes.as_array()?.iter().find_map(|node| {
                if node["type"] == "image" {
                    Some((node["x"].as_f64()?, node["y"].as_f64()?, node["width"].as_f64()?, node["height"].as_f64()?))
                } else {
                    find(&node["nodes"])
                }
            })
        }
        find(&tree["pages"][0]["nodes"]).expect("an image in the tree")
    }

    fn logo_size(img: &str, px: (u32, u32)) -> (f64, f64) {
        let (_, _, w, h) = first_image(&tree_with_logo("", img, Some(px)));
        (w, h)
    }

    #[test]
    fn an_image_given_one_size_follows_the_proportions_of_the_image() {
        assert_eq!(logo_size(r#"<img name="logo" width="80pt"/>"#, (200, 100)), (80.0, 40.0));
        assert_eq!(logo_size(r#"<img name="logo" height="30pt"/>"#, (200, 100)), (60.0, 30.0));
    }

    #[test]
    fn an_image_given_both_sizes_fits_inside_them() {
        // A wide image in a square: the width sets the size.
        assert_eq!(logo_size(r#"<img name="logo" width="100pt" height="100pt"/>"#, (200, 100)), (100.0, 50.0));
        // A tall one: the height sets it.
        assert_eq!(logo_size(r#"<img name="logo" width="100pt" height="100pt"/>"#, (100, 200)), (50.0, 100.0));
        // One with the proportions of the box fills it, and a small one grows to it.
        assert_eq!(logo_size(r#"<img name="logo" width="100pt" height="50pt"/>"#, (200, 100)), (100.0, 50.0));
        assert_eq!(logo_size(r#"<img name="logo" width="100pt" height="100pt"/>"#, (20, 10)), (100.0, 50.0));
    }

    #[test]
    fn an_image_that_is_not_loaded_keeps_the_box_it_is_given() {
        let tree = tree_with_logo("", r#"<img name="logo" width="84pt" height="164pt"/>"#, None);
        let (_, _, w, h) = first_image(&tree);
        assert_eq!((w, h), (84.0, 164.0));
    }

    #[test]
    fn the_align_of_the_parent_places_the_fitted_image() {
        // A tall image in a square box is 50pt wide, and the column centres that, not the 100pt it was given.
        let img = r#"<column align="center"><img name="logo" width="100pt" height="100pt"/></column>"#;
        let (x, _, w, _) = first_image(&tree_with_logo("", img, Some((100, 200))));
        assert_eq!(w, 50.0);
        assert!((x - (28.0 + (595.28 - 56.0 - 50.0) / 2.0)).abs() < 0.02, "x is {x}");
    }

    #[test]
    fn the_align_of_the_parent_places_the_fitted_barcode() {
        let code = r#"<column align="center"><barcode type="ean13" data="590123412345" width="100pt" height="45pt" hrt="true"/></column>"#;
        let tree = engine_render(&minimal(code));
        let barcode = &tree["pages"][0]["nodes"][0]["nodes"][0];
        let (x, w) = (barcode["x"].as_f64().unwrap(), barcode["width"].as_f64().unwrap());
        assert!((w - 52.09).abs() < 0.01, "w is {w}");
        assert!((x - (28.0 + (595.28 - 56.0 - w) / 2.0)).abs() < 0.02, "x is {x}");
    }

    #[test]
    fn a_row_gives_a_fitted_barcode_the_width_it_fits_to() {
        // The EAN-13 of 100 × 45pt is 52.09pt wide, and the text after it starts 10pt past that, not past 100pt.
        let row = r#"<row gap="10pt"><barcode type="ean13" data="590123412345" width="100pt" height="45pt" hrt="true"/><text width="80pt">After</text></row>"#;
        let tree = engine_render(&minimal(row));
        let items = &tree["pages"][0]["nodes"][0]["nodes"];
        let (code, text) = (&items[0], &items[1]);
        assert!((code["width"].as_f64().unwrap() - 52.09).abs() < 0.01);
        assert!((text["x"].as_f64().unwrap() - (28.0 + 52.09 + 10.0)).abs() < 0.02, "the text is at {}", text["x"]);
    }

    #[test]
    fn an_img_at_in_a_corner_touches_it_whatever_its_proportions() {
        let layer = |name: &str| format!(
            r#"<layer><img-at name="{name}" anchor="top-right" width="100pt" height="100pt"/></layer>"#
        );
        for px in [(200, 100), (100, 200), (100, 100)] {
            let (x, y, w, h) = first_image(&tree_with_logo(&layer("logo"), "", Some(px)));
            assert!((x + w - 595.28).abs() < 0.02 && y == 0.0, "{px:?} is at {x},{y} {w}×{h}");
            assert!(w <= 100.0 && h <= 100.0 && (w == 100.0 || h == 100.0), "{px:?} is {w}×{h}");
            assert!((w / h - px.0 as f64 / px.1 as f64).abs() < 0.001, "{px:?} is {w}×{h}");
        }
    }

    #[test]
    fn a_barcode_at_in_a_corner_touches_it_whatever_its_proportions() {
        for (width, height) in [("100pt", "45pt"), ("60pt", "120pt")] {
            let tree = layer_tree(
                &format!(r#"<layer><barcode-at type="ean13" data="590123412345" anchor="bottom-right" width="{width}" height="{height}"/></layer>"#),
                0,
            );
            let code = &page_layers(&tree, 0)[0]["nodes"][0];
            let (x, y) = (code["x"].as_f64().unwrap(), code["y"].as_f64().unwrap());
            let (w, h) = (code["width"].as_f64().unwrap(), code["height"].as_f64().unwrap());
            assert!((x + w - 595.28).abs() < 0.02 && (y + h - 841.89).abs() < 0.02, "{width} × {height}: {x},{y} {w}×{h}");
            // 95 modules of 0.33mm across, and bars 22.85mm high.
            assert!((w / h - 95.0 * 0.33 / 22.85).abs() < 0.001, "{width} × {height}: {w}×{h}");
        }
    }

    #[test]
    fn a_barcode_at_takes_a_width_or_a_height_and_a_code_128_follows_the_other() {
        let tree = layer_tree(r#"<layer><barcode-at type="code128" data="ABC-123" height="20pt"/></layer>"#, 0);
        let code = &page_layers(&tree, 0)[0]["nodes"][0];
        assert_eq!((code["width"].as_f64().unwrap(), code["height"].as_f64().unwrap()), (56.0, 20.0));
    }

    #[test]
    fn a_barcode_at_is_placed_by_its_anchor() {
        let tree = layer_tree(
            r#"<layer><barcode-at type="qr" data="x" anchor="top-right" x="-20pt" y="20pt" width="60pt"/></layer>"#,
            0,
        );
        let code = &page_layers(&tree, 0)[0]["nodes"][0];
        assert_eq!(code["type"], "barcode");
        assert_eq!((code["width"].as_f64().unwrap(), code["height"].as_f64().unwrap()), (60.0, 60.0));
        assert!((code["x"].as_f64().unwrap() - (595.28 - 20.0 - 60.0)).abs() < 0.02);
        assert_eq!(code["y"].as_f64().unwrap(), 20.0);
    }

    #[test]
    fn a_layer_with_a_clip_tells_the_page_to_cut_off_what_it_draws() {
        let tree = layer_tree(
            r##"<layer clip="10pt 20pt 100pt 50pt"><rect x="0pt" y="0pt" width="300pt" height="300pt" fill="#ff0000"/></layer>"##,
            0,
        );
        assert_eq!(page_layers(&tree, 0)[0]["clip"], serde_json::json!([10.0, 20.0, 100.0, 50.0]));
    }

    // ── Running headers ───────────────────────────────────────────────────────

    /// Every text of a page tree with the `y` it is drawn at, from the top of the page down.
    fn texts_from_the_top(page: &serde_json::Value) -> Vec<(f64, String)> {
        let mut texts: Vec<(f64, String)> = Vec::new();
        fn walk(v: &serde_json::Value, out: &mut Vec<(f64, String)>) {
            if v["type"] == "text" {
                if let (Some(s), Some(y)) = (v["content"].as_str(), v["y"].as_f64()) { out.push((y, s.to_string())); }
            }
            if let Some(arr) = v["nodes"].as_array() {
                for c in arr { walk(c, out); }
            }
        }
        walk(page, &mut texts);
        texts.sort_by(|a, b| a.0.total_cmp(&b.0));
        texts
    }

    /// The words of the text nearest the top of a page: a header, when the page has one.
    fn header_of(page: &serde_json::Value) -> String {
        texts_from_the_top(page).first().map(|t| t.1.clone()).unwrap_or_default()
    }

    fn headers_of(xml: &str) -> Vec<String> {
        let tree = engine_render(xml);
        tree["pages"].as_array().unwrap().iter().map(header_of).collect()
    }

    /// A header with a sink for `k`, then a column on a page of its own for each of `pages`: a text that captures
    /// under `k` when it starts with `+`.
    fn captured_pages(sink: &str, pages: &[&str]) -> Vec<String> {
        let flow: String = pages.iter().enumerate().map(|(i, p)| {
            let (capture, words) = match p.strip_prefix('+') {
                Some(words) => (r#" capture="k""#, words),
                None        => ("", *p),
            };
            let brk = if i == 0 { "" } else { r#" paginate="break-before""# };
            format!("<column{brk}><text{capture}>{words}</text></column>")
        }).collect();
        headers_of(&minimal(&format!(r#"<region pin="top"><text capture-ref="k" {sink}/></region>{flow}"#)))
    }

    #[test]
    fn a_sink_shows_the_page_it_is_on_then_the_latest_value_then_its_fallback() {
        let headers = captured_pages(r#"capture-fallback="none""#, &["Intro", "+Alpha", "Body", "Body", "+Beta", "Body"]);
        assert_eq!(headers, ["none", "Alpha", "Alpha", "Alpha", "Beta", "Beta"]);
    }

    #[test]
    fn a_sink_without_a_fallback_shows_nothing_before_the_first_capture() {
        // The first page has no header text, so the text nearest its top is the body.
        assert_eq!(captured_pages("", &["Intro", "+Alpha"]), ["Intro", "Alpha"]);
    }

    #[test]
    fn capture_mode_picks_the_first_or_the_last_text_the_page_captured() {
        // Two texts capture on the first page, one on the second, and none on the third.
        let flow = concat!(
            r#"<text capture="k">One</text><text capture="k">Two</text>"#,
            r#"<column paginate="break-before"><text capture="k">Three</text></column>"#,
            r#"<column paginate="break-before"><text>None</text></column>"#,
        );
        for (mode, want) in [(r#"capture-mode="first""#, ["One", "Three", "Three"]), (r#"capture-mode="last""#, ["Two", "Three", "Three"]), ("", ["Two", "Three", "Three"])] {
            let body = format!(r#"<region pin="top"><text capture-ref="k" {mode}/></region>{flow}"#);
            assert_eq!(headers_of(&minimal(&body)), want, "{mode:?}");
        }
    }

    #[test]
    fn a_capture_reads_the_words_of_a_text_with_spans() {
        let body = r#"<region pin="top"><text capture-ref="k"/></region><text capture="k">Chapter <span font="Helvetica-Bold">Two</span>, the   Road</text>"#;
        assert_eq!(headers_of(&minimal(body)), ["Chapter Two, the Road"]);
    }

    #[test]
    fn capture_is_kept_apart_in_each_section() {
        let section = |body: &str| {
            format!(r#"<section><layout margin="28pt"><region pin="top"><text capture-ref="k" capture-fallback="none"/></region>{body}</layout></section>"#)
        };
        let xml = format!(
            r#"<lpdf version="1"><document size="a4">{}{}</document></lpdf>"#,
            section(r#"<text capture="k">Alpha</text>"#),
            section("<text>Plain</text>"),
        );
        assert_eq!(headers_of(&xml), ["Alpha", "none"]);
    }

    #[test]
    fn a_text_that_splits_across_pages_is_captured_once_where_it_starts() {
        // The long text runs from the first page onto the second, where one more text captures. The part on the
        // second page must not capture again: the first text captured on that page is "Late".
        let long = "word ".repeat(1500);
        let body = format!(
            r#"<region pin="top"><text capture-ref="k" capture-mode="first"/></region><text capture="k">Early</text><text capture="k">{long}</text><text capture="k">Late</text>"#
        );
        let headers = headers_of(&minimal(&body));
        assert!(headers.len() >= 2, "the text should run onto a second page");
        assert_eq!((headers[0].as_str(), headers.last().unwrap().as_str()), ("Early", "Late"));
    }

    #[test]
    fn a_table_header_repeated_on_a_later_page_does_not_capture_again() {
        // The header captures once, on the page the table starts, and the third row captures after it. Every page
        // shows "Row", the latest capture: a repeated copy of the header must not put "Head" back on a later page.
        let rows: String = (1..=150)
            .map(|n| if n == 3 { r#"<tr><td><text capture="k">Row</text></td></tr>"#.to_string() } else { "<tr><td><text>cell</text></td></tr>".to_string() })
            .collect();
        let body = format!(
            r#"<region pin="top"><text capture-ref="k"/></region><table cols="1fr"><thead><td><text capture="k">Head</text></td></thead>{rows}</table>"#
        );
        let tree = engine_render(&minimal(&body));
        let pages = tree["pages"].as_array().unwrap();
        assert!(pages.len() >= 3, "the table should run over three pages");
        for (i, page) in pages.iter().enumerate() {
            assert_eq!(header_of(page), "Row", "page {}", i + 1);
            // The header is still drawn on every page.
            assert!(texts_from_the_top(page).iter().any(|t| t.1 == "Head"), "page {} has no table header", i + 1);
        }
    }

    #[test]
    fn a_sink_takes_its_line_whatever_it_shows_so_the_flow_stays_on_its_page() {
        // The same flow under a header that shows a heading and one that shows nothing, because nothing is captured
        // under its name: the flow starts at the same place and runs to the same number of pages.
        let flow_start = |sink_key: &str| {
            let lines: String = (1..=120).map(|n| format!("<text>Line {n}</text>")).collect();
            let body = format!(
                r#"<region pin="top"><text capture-ref="{sink_key}" font-size="20pt"/></region><text capture="k">A heading</text>{lines}"#
            );
            let tree = engine_render(&minimal(&body));
            let pages = tree["pages"].as_array().unwrap();
            let y = texts_from_the_top(&pages[0]).into_iter().find(|t| t.1 == "Line 1").unwrap().0;
            (y, pages.len())
        };
        assert_eq!(flow_start("k"), flow_start("nothing-captured-under-this-name"));
    }

    #[test]
    fn a_sink_is_one_line_and_cuts_a_longer_value_in_dots() {
        let long = "A heading that is far too long to fit on the line that the header of this page has to hold it in";
        let body = format!(
            r#"<region pin="top"><text capture-ref="k" width="150pt" ellipsis="true"/></region><text capture="k">{long}</text>"#
        );
        let tree = engine_render(&minimal(&body));
        let page = &tree["pages"][0];
        let header = header_of(page);
        assert!(header.ends_with("...") && header.len() < long.len(), "{header}");
        let dotted = texts_from_the_top(page).into_iter().filter(|t| t.1.ends_with("...")).count();
        assert_eq!(dotted, 1, "one line, cut in dots");
    }

    #[test]
    fn a_sink_in_the_flow_shows_its_fallback() {
        let body = r#"<text capture="k">Alpha</text><text capture-ref="k" capture-fallback="from the flow"/>"#;
        let tree = engine_render(&minimal(body));
        assert!(find_text_content(&tree["pages"][0]).contains(&"from the flow".to_string()));
    }

    // ── An image with box attributes ──────────────────────────────────────────

    /// The first node of a page tree, at any depth, that `pick` returns a value for.
    fn find_in<T>(tree: &serde_json::Value, pick: &dyn Fn(&serde_json::Value) -> Option<T>) -> Option<T> {
        fn walk<T>(nodes: &serde_json::Value, pick: &dyn Fn(&serde_json::Value) -> Option<T>) -> Option<T> {
            nodes.as_array()?.iter().find_map(|node| pick(node).or_else(|| walk(&node["nodes"], pick)))
        }
        walk(&tree["pages"][0]["nodes"], pick)
    }

    /// The first box of the page that holds an image, and that image.
    fn box_with_image(tree: &serde_json::Value) -> Option<(serde_json::Value, serde_json::Value)> {
        find_in(tree, &|node| {
            if node["type"] != "box" { return None; }
            let image = node["nodes"].as_array()?.iter().find(|n| n["type"] == "image")?;
            Some((node.clone(), image.clone()))
        })
    }

    #[test]
    fn an_image_with_padding_a_background_and_a_border_is_a_box_around_its_picture() {
        let img = r##"<img name="logo" width="80pt" padding="10pt" background="#eeeeee" border="2pt #112233" radius="12pt"/>"##;
        let tree = tree_with_logo("", img, Some((200, 100)));
        let (frame, picture) = box_with_image(&tree).expect("a box around the picture");
        // The picture is 80 by 40, and the box grows by the padding on each side. The border takes no room.
        assert_eq!((picture["width"].as_f64(), picture["height"].as_f64()), (Some(80.0), Some(40.0)));
        assert_eq!((frame["width"].as_f64(), frame["height"].as_f64()), (Some(100.0), Some(60.0)));
        assert_eq!(frame["fill"], "#eeeeee");
        assert_eq!((frame["border_width"].as_f64(), frame["border_color"].as_str()), (Some(2.0), Some("#112233")));
        assert_eq!(frame["radius"].as_f64(), Some(12.0));
        // The picture sits the padding in from the edge, and its corners are the box's less that.
        let inset = picture["x"].as_f64().unwrap() - frame["x"].as_f64().unwrap();
        assert_eq!(inset, 10.0);
        assert_eq!(picture["radius"].as_f64(), Some(2.0));
    }

    #[test]
    fn the_picture_is_rounded_to_the_radius_less_the_smallest_padding_and_never_below_nothing() {
        let radius = |padding: &str| {
            let img = format!(r##"<img name="logo" width="80pt" padding="{padding}" border="2pt #112233" radius="20pt"/>"##);
            let (_, picture) = box_with_image(&tree_with_logo("", &img, Some((200, 100)))).expect("a box around the picture");
            picture["radius"].as_f64()
        };
        assert_eq!(radius("4pt"), Some(16.0));
        assert_eq!(radius("4pt 10pt"), Some(16.0), "the smallest side counts");
        assert_eq!(radius("30pt"), None, "a padding past the radius leaves the picture square");
    }

    #[test]
    fn a_radius_alone_clips_the_picture_and_adds_no_box() {
        let tree = tree_with_logo("", r#"<img name="logo" width="48pt" height="48pt" radius="12pt"/>"#, Some((100, 100)));
        assert!(box_with_image(&tree).is_none(), "a box was added");
        let picture = find_in(&tree, &|n| (n["type"] == "image").then(|| n.clone())).expect("an image");
        assert_eq!((picture["width"].as_f64(), picture["radius"].as_f64()), (Some(48.0), Some(12.0)));
    }

    #[test]
    fn an_image_without_box_attributes_is_drawn_as_before() {
        let tree = tree_with_logo("", r#"<img name="logo" width="80pt"/>"#, Some((200, 100)));
        assert!(box_with_image(&tree).is_none());
        let picture = find_in(&tree, &|n| (n["type"] == "image").then(|| n.clone())).expect("an image");
        assert!(picture.get("radius").is_none());
        assert_eq!(first_image(&tree), (28.0, 28.0, 80.0, 40.0));
    }

    #[test]
    fn a_box_around_a_picture_takes_its_width_in_a_row() {
        let body = r##"<row gap="10pt"><img name="logo" width="80pt" padding="10pt" background="#eeeeee"/><text>Next</text></row>"##;
        let tree = tree_with_logo("", body, Some((200, 100)));
        let (frame, _) = box_with_image(&tree).expect("a box around the picture");
        let next = find_in(&tree, &|n| (n["type"] == "text" && n["content"] == "Next").then(|| n["x"].as_f64()).flatten())
            .expect("the text after the image");
        // The box is 100 wide, so the text starts a gap after it.
        assert_eq!(frame["width"].as_f64(), Some(100.0));
        assert_eq!(next, frame["x"].as_f64().unwrap() + 100.0 + 10.0);
    }

    #[test]
    fn a_box_around_a_picture_stays_a_box_in_a_table_cell() {
        let body = r##"<table cols="1fr 1fr"><tr><td><img name="logo" width="40pt" padding="5pt" background="#eeeeee"/></td><td><text>Cell</text></td></tr></table>"##;
        let tree = tree_with_logo("", body, Some((200, 100)));
        let (frame, picture) = box_with_image(&tree).expect("a box around the picture in the cell");
        assert_eq!((frame["width"].as_f64(), frame["height"].as_f64()), (Some(50.0), Some(30.0)));
        assert_eq!(picture["width"].as_f64(), Some(40.0));
    }

    // ── An image with width fill ──────────────────────────────────────────────

    /// The width of the layout's column: an A4 page less a 28pt margin on each side.
    const COLUMN_W: f64 = 595.28 - 56.0;

    fn fill_size(body: &str, px: (u32, u32)) -> (f64, f64) {
        let (_, _, w, h) = first_image(&tree_with_logo("", body, Some(px)));
        (w, h)
    }

    fn close(got: (f64, f64), want: (f64, f64)) {
        assert!((got.0 - want.0).abs() < 0.02 && (got.1 - want.1).abs() < 0.02, "got {got:?}, want {want:?}");
    }

    #[test]
    fn an_image_with_width_fill_takes_the_width_of_its_column_and_the_height_follows() {
        close(fill_size(r#"<img name="logo" width="fill"/>"#, (200, 100)), (COLUMN_W, COLUMN_W / 2.0));
        // A picture smaller than the column grows to it, as one given a width does.
        close(fill_size(r#"<img name="logo" width="fill"/>"#, (20, 10)), (COLUMN_W, COLUMN_W / 2.0));
        // Inside a narrower box, it takes that box.
        close(fill_size(r#"<column width="100pt"><img name="logo" width="fill"/></column>"#, (200, 100)), (100.0, 50.0));
    }

    #[test]
    fn an_image_with_width_fill_and_a_height_fits_inside_both() {
        // The height is the limit: the picture is as tall as given, and not as wide as the column.
        close(fill_size(r#"<img name="logo" width="fill" height="60pt"/>"#, (200, 100)), (120.0, 60.0));
        // The width is the limit: the column is 100pt wide, and the height allows more.
        close(fill_size(r#"<column width="100pt"><img name="logo" width="fill" height="200pt"/></column>"#, (200, 100)), (100.0, 50.0));
    }

    #[test]
    fn an_image_with_width_fill_in_a_row_takes_the_room_the_line_leaves() {
        let body = r#"<row gap="10pt"><img name="logo" width="fill"/><text width="100pt">Next</text></row>"#;
        let left = COLUMN_W - 100.0 - 10.0;
        close(fill_size(body, (200, 100)), (left, left / 2.0));
        // Two fill images share the line.
        let two = r#"<row gap="10pt"><img name="logo" width="fill"/><img name="logo" width="fill"/></row>"#;
        let half = (COLUMN_W - 10.0) / 2.0;
        close(fill_size(two, (200, 100)), (half, half / 2.0));
    }

    #[test]
    fn an_image_with_width_fill_in_a_table_cell_takes_the_cell() {
        let body = r#"<table cols="1fr 1fr"><tr><td><img name="logo" width="fill"/></td><td><text>Cell</text></td></tr></table>"#;
        let tree = tree_with_logo("", body, Some((200, 100)));
        let (x, _, w, h) = first_image(&tree);
        let next_cell = find_in(&tree, &|n| (n["type"] == "text" && n["content"] == "Cell").then(|| n["x"].as_f64()).flatten()).unwrap();
        // Two equal cells: the picture is half the column, and the next cell starts where it ends.
        close((w, h), (COLUMN_W / 2.0, COLUMN_W / 4.0));
        assert!((x + w - next_cell).abs() < 0.02, "the picture ends at {}, the next cell starts at {next_cell}", x + w);
    }

    #[test]
    fn a_box_around_an_image_with_width_fill_takes_the_column_and_the_picture_what_the_padding_leaves() {
        let img = r##"<img name="logo" width="fill" padding="10pt" background="#eeeeee" border="1pt #112233"/>"##;
        let tree = tree_with_logo("", img, Some((200, 100)));
        let (frame, picture) = box_with_image(&tree).expect("a box around the picture");
        let inner = COLUMN_W - 20.0;
        close((frame["width"].as_f64().unwrap(), frame["height"].as_f64().unwrap()), (COLUMN_W, inner / 2.0 + 20.0));
        close((picture["width"].as_f64().unwrap(), picture["height"].as_f64().unwrap()), (inner, inner / 2.0));
    }

    #[test]
    fn an_image_without_width_fill_is_sized_as_before() {
        close(fill_size(r#"<img name="logo"/>"#, (200, 100)), (200.0, 100.0));
        close(fill_size(r#"<img name="logo" width="80pt"/>"#, (200, 100)), (80.0, 40.0));
    }

    #[test]
    fn a_box_around_a_picture_is_not_split_across_a_page_break() {
        let filler = r#"<frame height="760pt"/>"#;
        let img = r##"<img name="logo" width="80pt" padding="10pt" background="#eeeeee"/>"##;
        let tree = tree_with_logo("", &format!("{filler}{img}"), Some((200, 100)));
        let on_page = |page: usize| {
            let has = |kind: &str| tree["pages"][page].to_string().contains(&format!(r#""type":"{kind}""#));
            (has("image"), tree["pages"][page]["nodes"].to_string().contains(r##""fill":"#eeeeee""##))
        };
        assert_eq!(on_page(0), (false, false), "nothing of the box stays on the first page");
        assert_eq!(on_page(1), (true, true), "the box and its picture are together on the second");
    }

    // ── Shapes ───────────────────────────────────────────────────────────────

    /// Every node of `kind` in the tree, in document order.
    fn nodes_of(tree: &serde_json::Value, kind: &str) -> Vec<serde_json::Value> {
        fn walk(value: &serde_json::Value, kind: &str, found: &mut Vec<serde_json::Value>) {
            match value {
                serde_json::Value::Object(map) => {
                    if map.get("type").and_then(|t| t.as_str()) == Some(kind) {
                        found.push(value.clone());
                    }
                    map.values().for_each(|v| walk(v, kind, found));
                }
                serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, kind, found)),
                _ => {}
            }
        }
        let mut found = Vec::new();
        walk(tree, kind, &mut found);
        found
    }

    fn numbers(node: &serde_json::Value, keys: &[&str]) -> Vec<f64> {
        keys.iter().map(|k| node[*k].as_f64().unwrap_or_else(|| panic!("{k} in {node}"))).collect()
    }

    fn all_near(actual: Vec<f64>, expected: &[f64]) {
        assert!(
            actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, e)| (a - e).abs() < 0.01),
            "{actual:?} is not {expected:?}"
        );
    }

    /// The `y` of the first text, which follows what the document puts before it.
    fn text_y(tree: &serde_json::Value) -> f64 {
        nodes_of(tree, "text")[0]["y"].as_f64().unwrap()
    }

    #[test]
    fn a_rect_shape_takes_its_box_in_the_flow() {
        let tree = engine_render(&minimal(r##"<shape type="rect" width="60pt" height="20pt" fill="#ff0000"/><text>After</text>"##));
        let rect = &nodes_of(&tree, "canvas-rect")[0];
        all_near(numbers(rect, &["x", "y", "w", "h"]), &[28.0, 28.0, 60.0, 20.0]);

        let frame = engine_render(&minimal(r#"<frame width="60pt" height="20pt"/><text>After</text>"#));
        assert_eq!(text_y(&tree), text_y(&frame), "the text after it starts where the 20pt box ends");
    }

    #[test]
    fn a_shape_with_no_fill_and_no_stroke_keeps_its_box() {
        let tree = engine_render(&minimal(r#"<shape type="rect" width="60pt" height="20pt"/><text>After</text>"#));
        let frame = engine_render(&minimal(r#"<frame width="60pt" height="20pt"/><text>After</text>"#));
        assert_eq!(text_y(&tree), text_y(&frame));
    }

    #[test]
    fn a_circle_shape_is_drawn_in_the_square_of_side_2r() {
        let tree = engine_render(&minimal(r##"<shape type="circle" r="10pt" fill="#00ff00"/><text>After</text>"##));
        let circle = &nodes_of(&tree, "canvas-ellipse")[0];
        all_near(numbers(circle, &["cx", "cy", "rx", "ry"]), &[38.0, 38.0, 10.0, 10.0]);

        let frame = engine_render(&minimal(r#"<frame width="20pt" height="20pt"/><text>After</text>"#));
        assert_eq!(text_y(&tree), text_y(&frame));
    }

    #[test]
    fn an_ellipse_shape_is_drawn_in_its_box() {
        let tree = engine_render(&minimal(r##"<shape type="ellipse" width="40pt" height="20pt" fill="#00ff00"/>"##));
        all_near(numbers(&nodes_of(&tree, "canvas-ellipse")[0], &["cx", "cy", "rx", "ry"]), &[48.0, 38.0, 20.0, 10.0]);
    }

    #[test]
    fn the_parent_places_a_shape_by_its_align_and_the_drawing_moves_with_the_box() {
        let page_w = 595.28 - 56.0;
        let in_column = engine_render(&minimal(
            r##"<column align="center"><shape type="rect" width="100pt" height="20pt" fill="#ff0000"/></column>"##,
        ));
        all_near(numbers(&nodes_of(&in_column, "canvas-rect")[0], &["x", "y"]), &[28.0 + (page_w - 100.0) / 2.0, 28.0]);

        let in_row = engine_render(&minimal(
            r##"<row align="center"><frame width="10pt" height="60pt"/><shape type="circle" r="10pt" fill="#00ff00"/></row>"##,
        ));
        all_near(numbers(&nodes_of(&in_row, "canvas-ellipse")[0], &["cx", "cy"]), &[38.0 + 10.0, 28.0 + 20.0 + 10.0]);

        let at_end = engine_render(&minimal(
            r##"<row justify="end"><shape type="rect" width="50pt" height="10pt" fill="#ff0000"/></row>"##,
        ));
        all_near(numbers(&nodes_of(&at_end, "canvas-rect")[0], &["x"]), &[28.0 + page_w - 50.0]);
    }

    #[test]
    fn a_shape_wider_than_its_room_is_narrowed_with_its_proportions() {
        let tree = engine_render(&minimal(
            r##"<column width="50pt"><shape type="rect" width="100pt" height="20pt" fill="#ff0000"/><text>After</text></column>"##,
        ));
        all_near(numbers(&nodes_of(&tree, "canvas-rect")[0], &["w", "h"]), &[50.0, 10.0]);
    }

    #[test]
    fn a_shape_reads_its_size_from_the_width_and_height_rows_of_the_tokens() {
        let tree = engine_render(&minimal(r##"<shape type="rect" width="xs" height="m" fill="#ff0000"/>"##));
        all_near(numbers(&nodes_of(&tree, "canvas-rect")[0], &["w", "h"]), &[120.0, 32.0]);
    }

    #[test]
    fn a_shape_with_opacity_is_faded_as_one_and_one_without_is_not() {
        let faded = engine_render(&minimal(r##"<shape type="rect" width="60pt" height="20pt" fill="#ff0000" opacity="0.5"/>"##));
        let boxed = nodes_of(&faded, "box");
        let shape_box = boxed.iter().find(|b| b["opacity"] == 0.5).expect("a box with the opacity");
        assert_eq!(nodes_of(shape_box, "canvas-rect").len(), 1);

        let opaque = engine_render(&minimal(r##"<shape type="rect" width="60pt" height="20pt" fill="#ff0000"/>"##));
        assert!(nodes_of(&opaque, "box").iter().all(|b| b.get("opacity").is_none()));
    }

    #[test]
    fn a_shape_in_a_table_cell_is_drawn_inside_the_cell() {
        let tree = engine_render(&minimal(
            r##"<table cols="1fr 1fr"><tr><td><row gap="s" align="center"><shape type="circle" r="5pt" fill="#00aa00"/><text>Active</text></row></td><td><text>x</text></td></tr></table>"##,
        ));
        let dot = &nodes_of(&tree, "canvas-ellipse")[0];
        let cx = dot["cx"].as_f64().unwrap();
        assert!(cx > 28.0 && cx < 28.0 + 539.28 / 2.0, "cx={cx}");
    }

    /// The numbers of a path's `d`, in order.
    fn path_numbers(d: &str) -> Vec<f32> {
        d.split_whitespace().filter_map(|word| word.parse().ok()).collect()
    }

    #[test]
    fn a_checked_item_that_a_row_moves_draws_its_tick_inside_its_checkbox() {
        let list = r#"<column width="200pt"><list><li checked="true"><text>First</text></li></list></column>"#;
        let rows = [
            ("centered", format!(r#"<row align="center"><text font-size="40pt">Tall</text>{list}</row>"#)),
            ("on a later line", format!(r#"<row><frame width="400pt" height="20pt"/>{list}</row>"#)),
        ];
        for (case, row) in rows {
            let tree = engine_render(&minimal(&row));
            let outline = &nodes_of(&tree, "canvas-rect")[0];
            let [x, y, w, h] = ["x", "y", "w", "h"].map(|k| outline[k].as_f64().unwrap() as f32);
            let d = nodes_of(&tree, "canvas-path")[0]["d"].as_str().unwrap().to_string();
            let points = path_numbers(&d);
            assert_eq!(points.len(), 6, "{case}: {d}");
            let inside = points.chunks(2).all(|p| (x..=x + w).contains(&p[0]) && (y..=y + h).contains(&p[1]));
            assert!(inside, "{case}: the tick {d} is outside its box at {x} {y} {w} {h}");
        }
    }

    #[test]
    fn a_checkbox_laid_out_at_the_origin_and_moved_has_the_tick_of_one_laid_out_in_place() {
        let shape = crate::parse::Shape {
            kind: crate::parse::ShapeKind::Checkbox, fill: None, stroke: Some("#737373".into()), stroke_width: Some(1.0),
            stroke_dash: None, tick: Some("#1a1a1a".into()),
        };
        let tick_of = |node: RenderNode| {
            let RenderNode::Box(checkbox) = node else { panic!("a box") };
            let Some(RenderNode::CanvasPath(tick)) = checkbox.children.into_iter().nth(1) else { panic!("a tick") };
            path_numbers(&tick.d)
        };
        let in_place = tick_of(canvas::draw_flow_shape(&shape, 2.0, 37.3, 120.7, 8.8, 8.8));
        let moved = tick_of(shift_x(shift_y(canvas::draw_flow_shape(&shape, 2.0, 0.0, 0.0, 8.8, 8.8), 120.7), 37.3));
        assert_eq!(in_place.len(), 6);
        assert!(in_place.iter().zip(&moved).all(|(a, b)| (a - b).abs() < 0.001), "{in_place:?} is not {moved:?}");
    }

    // ── Footnotes ─────────────────────────────────────────────────────────────

    /// Every text of a page in draw order: its content, x, y and size.
    fn texts_on(tree: &serde_json::Value, page: usize) -> Vec<(String, f64, f64, f64)> {
        fn walk(v: &serde_json::Value, out: &mut Vec<(String, f64, f64, f64)>) {
            if v["type"] == "text" {
                out.push((
                    v["content"].as_str().unwrap_or("").to_string(),
                    v["x"].as_f64().unwrap_or(0.0), v["y"].as_f64().unwrap_or(0.0), v["size"].as_f64().unwrap_or(0.0),
                ));
            }
            if let Some(arr) = v["nodes"].as_array() { for c in arr { walk(c, out); } }
        }
        let mut out = Vec::new();
        walk(&tree["pages"][page], &mut out);
        out
    }

    /// A document of sections, each `(attributes, layout children)`, with a margin of 28pt.
    fn sections_doc(document_attrs: &str, sections: &[(&str, &str)]) -> String {
        let sections: String = sections.iter()
            .map(|(attrs, body)| format!(r#"<section {attrs}><layout margin="28pt">{body}</layout></section>"#))
            .collect();
        format!(r#"<lpdf version="1"><document size="a4" {document_attrs}>{sections}</document></lpdf>"#)
    }

    /// The numbers of the notes of a page, which are the small texts at its foot (size below the body's).
    fn note_labels(tree: &serde_json::Value, page: usize) -> Vec<String> {
        texts_on(tree, page).into_iter()
            .filter(|(_, x, y, size)| *x == 28.0 && *y > 600.0 && *size < 6.0)
            .map(|(c, ..)| c)
            .collect()
    }

    /// The numbers of the anchors in the text of a page: the raised texts above the notes.
    fn anchor_numbers(tree: &serde_json::Value, page: usize) -> Vec<String> {
        texts_on(tree, page).into_iter()
            .filter(|(_, _, y, size)| (*size - 6.82).abs() < 0.01 && *y < 600.0)
            .map(|(c, ..)| c)
            .collect()
    }

    #[test]
    fn a_footnote_shows_a_raised_number_in_its_line_and_the_note_at_the_foot_of_the_page() {
        let tree = engine_render(&minimal(r#"<text>The standard model<fn>First proposed in the 1960s.</fn> describes three forces.</text>"#));
        let texts = texts_on(&tree, 0);
        let word = texts.iter().find(|t| t.0 == "The standard model").unwrap();
        let anchor = texts.iter().find(|t| t.0 == "1" && t.3 < 8.0 && t.2 < 100.0).expect("the anchor");
        let after = texts.iter().find(|t| t.0 == "describes three forces.").unwrap();
        assert!((anchor.3 - 11.0 * SUP_SCALE as f64).abs() < 0.01, "the anchor is small: {}", anchor.3);
        // The anchor sits right after the word, a little below the line's top so that its baseline is above the line's.
        assert!((anchor.1 - (word.1 + text_width("Helvetica", "The standard model", 11.0) as f64)).abs() < 0.01);
        assert!(anchor.2 > word.2 && anchor.2 + anchor.3 < word.2 + word.3, "{anchor:?} against {word:?}");
        assert!(after.1 > anchor.1, "the text goes on after the anchor");
        // The note stands at the foot of the page, under the body, with its number hanging at the left.
        let note = texts.iter().find(|t| t.0 == "First proposed in the 1960s.").expect("the note");
        let label = texts.iter().find(|t| t.0 == "1" && t.2 > 700.0).expect("the note's number");
        assert!(note.2 > 700.0 && note.2 + note.3 < 841.89 - 28.0 + 1.0, "{note:?}");
        assert_eq!(label.1, 28.0);
        assert!(note.1 > label.1, "the words stand to the right of the number");
        assert_eq!(nodes_of(&tree, "line").len(), 1, "one rule above the notes");
    }

    #[test]
    fn several_notes_on_a_page_stack_in_order_under_one_rule() {
        let tree = engine_render(&minimal(
            r#"<text>One<fn>Note A.</fn> two<fn>Note B.</fn> three<fn>Note C.</fn>.</text>"#,
        ));
        let texts = texts_on(&tree, 0);
        let ys: Vec<f64> = ["Note A.", "Note B.", "Note C."].iter()
            .map(|n| texts.iter().find(|t| t.0 == *n).unwrap_or_else(|| panic!("{n} missing")).2)
            .collect();
        assert!(ys[0] < ys[1] && ys[1] < ys[2], "{ys:?}");
        assert_eq!(note_labels(&tree, 0), ["1", "2", "3"]);
        assert_eq!(anchor_numbers(&tree, 0), ["1", "2", "3"]);
        let rules = nodes_of(&tree, "line");
        assert_eq!(rules.len(), 1);
        assert!(rules[0]["y1"].as_f64().unwrap() < ys[0], "the rule is above the first note");
    }

    #[test]
    fn the_notes_stand_above_a_region_at_the_bottom() {
        let tree = engine_render(&minimal(
            r#"<region pin="bottom"><text>FOOT</text></region><text>Body<fn>The note.</fn> text.</text>"#,
        ));
        let ys = text_ys(&tree["pages"][0]);
        assert!(ys["The note."] < ys["FOOT"], "the note {} is above the footer {}", ys["The note."], ys["FOOT"]);
        assert!(ys["The note."] > 700.0);
    }

    #[test]
    fn a_line_whose_note_does_not_fit_moves_to_the_next_page_with_its_note() {
        // 760pt of frame leave room for one more line, but not for the line and its note.
        let with = engine_render(&minimal(r#"<frame height="760pt"/><text>Last line<fn>The note.</fn> here.</text>"#));
        assert_eq!(with["pages"].as_array().unwrap().len(), 2);
        assert!(texts_on(&with, 0).is_empty(), "nothing on the first page: {:?}", texts_on(&with, 0));
        let second = texts_on(&with, 1);
        assert!(second.iter().any(|t| t.0 == "Last line") && second.iter().any(|t| t.0 == "The note."), "{second:?}");

        let without = engine_render(&minimal(r#"<frame height="760pt"/><text>Last line here.</text>"#));
        assert_eq!(without["pages"].as_array().unwrap().len(), 1, "without the note the line fits");
    }

    #[test]
    fn the_note_of_an_anchor_in_the_second_part_of_a_split_text_is_on_the_second_page() {
        let words = "word ".repeat(2400);
        let tree = engine_render(&minimal(&format!(r#"<text>{words}end<fn>Late note.</fn>.</text>"#)));
        let pages = tree["pages"].as_array().unwrap().len();
        assert!(pages >= 2);
        for page in 0..pages - 1 {
            assert!(!texts_on(&tree, page).iter().any(|t| t.0 == "Late note."), "page {} has the note", page + 1);
        }
        let last = texts_on(&tree, pages - 1);
        assert!(last.iter().any(|t| t.0 == "Late note."), "{last:?}");
        assert_eq!(anchor_numbers(&tree, pages - 1), ["1"]);
    }

    #[test]
    fn the_number_of_a_footnote_stays_with_the_word_before_it() {
        // The line has room for "alpha beta gamma", and not for the number after it.
        let width = text_width("Helvetica", "alpha beta gamma", 11.0) + 1.0;
        let tree = engine_render(&minimal(&format!(
            r#"<text width="{width}pt">alpha beta gamma<fn>The note.</fn> delta</text>"#
        )));
        let body: Vec<String> = texts_on(&tree, 0).into_iter().filter(|t| t.2 < 100.0 && t.3 > 10.0).map(|t| t.0).collect();
        assert_eq!(body[0], "alpha beta", "{body:?}");
        assert_eq!(body[1], "gamma", "{body:?}");
    }

    #[test]
    fn the_counter_set_to_page_numbers_each_page_from_one() {
        let tree = engine_render(&sections_doc("footnote-counter=\"page\"", &[(
            "",
            r#"<column><text>A<fn>a1</fn> b<fn>a2</fn></text></column><column paginate="break-before"><text>C<fn>b1</fn> d<fn>b2</fn> e<fn>b3</fn></text></column>"#,
        )]));
        assert_eq!(anchor_numbers(&tree, 0), ["1", "2"]);
        assert_eq!(note_labels(&tree, 0), ["1", "2"]);
        assert_eq!(anchor_numbers(&tree, 1), ["1", "2", "3"]);
        assert_eq!(note_labels(&tree, 1), ["1", "2", "3"]);
    }

    #[test]
    fn the_counter_set_to_section_numbers_across_pages_and_starts_over_in_the_next_section() {
        let page_break = r#"<column paginate="break-before"><text>C<fn>b1</fn></text></column>"#;
        let tree = engine_render(&sections_doc("", &[
            ("", &format!("<column><text>A<fn>a1</fn> b<fn>a2</fn></text></column>{page_break}")),
            ("footnote-counter=\"section\"", "<text>D<fn>c1</fn></text>"),
        ]));
        assert_eq!(note_labels(&tree, 0), ["1", "2"]);
        assert_eq!(note_labels(&tree, 1), ["3"], "the numbers go on over the page");
        assert_eq!(note_labels(&tree, 2), ["1"], "and start over in the next section");
    }

    #[test]
    fn the_counter_set_to_document_goes_on_across_the_sections_that_count_so() {
        let tree = engine_render(&sections_doc("footnote-counter=\"document\"", &[
            ("", "<text>A<fn>a1</fn> b<fn>a2</fn></text>"),
            ("", "<text>C<fn>b1</fn></text>"),
            ("footnote-counter=\"section\"", "<text>D<fn>c1</fn></text>"),
            ("", "<text>E<fn>d1</fn></text>"),
        ]));
        assert_eq!(note_labels(&tree, 0), ["1", "2"]);
        assert_eq!(note_labels(&tree, 1), ["3"]);
        assert_eq!(note_labels(&tree, 2), ["1"], "a section with a counter of its own starts over");
        assert_eq!(note_labels(&tree, 3), ["4"], "the next one goes on from the sections that count across the document");
        assert_eq!(anchor_numbers(&tree, 3), ["4"]);
    }

    #[test]
    fn a_footnote_in_a_single_full_height_child_still_shows_its_note() {
        let tree = engine_render(&minimal(r#"<column height="full"><text>Body<fn>The note.</fn> text.</text></column>"#));
        assert!(texts_on(&tree, 0).iter().any(|t| t.0 == "The note."));
    }

    #[test]
    fn a_note_taller_than_the_page_is_cut_at_the_page() {
        let words = "word ".repeat(3000);
        let tree = engine_render(&minimal(&format!(r#"<text>Body<fn>{words}</fn> text.</text>"#)));
        let page_h = 841.89;
        for (c, _, y, size) in texts_on(&tree, 0) {
            assert!(y + size <= page_h, "{c} is drawn at {y}, past the page");
        }
        assert!(texts_on(&tree, 0).iter().any(|t| t.0 == "Body"), "the body is still drawn");
    }

    #[test]
    fn a_page_keeps_back_only_the_room_its_notes_need() {
        // The text of the fixture that ends page 1 early: with the room for all six notes of the first fill a third
        // line does not fit, and with the room for the five that stand on the page it does. Only the search for
        // the tightest room finds it. The default widow rule moves the last two lines together, and fits neither.
        let xml = include_str!("../../../test/fixtures/showcase-footnote.xml");
        let first_lines = |xml: &str, page: usize| -> Vec<String> {
            texts_on(&engine_render(xml), page).into_iter().filter(|t| t.3 > 10.0).map(|t| t.0).collect()
        };
        let labels = |xml: &str| -> Vec<String> {
            texts_on(&engine_render(xml), 0).into_iter().filter(|t| t.3 < 6.0 && t.2 > 600.0 && t.1 == 40.0).map(|t| t.0).collect()
        };
        assert!(first_lines(xml, 1)[0].starts_with("worth remembering. The last column"));
        assert_eq!(labels(xml), ["1", "2", "3", "4"]);

        let tight = xml.replace("<text>Keepers who", "<text widows=\"1\">Keepers who");
        assert!(first_lines(&tight, 0).iter().any(|l| l.starts_with("worth remembering. The last column")), "the line fits");
        assert_eq!(labels(&tight), ["1", "2", "3", "4", "5"]);
    }

    #[test]
    fn the_notes_never_overlap_the_text_over_a_range_of_page_fills() {
        let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau";
        let text = format!(
            r#"<text width="220pt">{words}<fn>First note, a little longer than one line of the note area so that it wraps onto a second line.</fn> {words}<fn>Second.</fn> {words}<fn>Third note.</fn> {words}</text>"#
        );
        for height in (400..=770).step_by(7) {
            let tree = engine_render(&minimal(&format!(r#"<frame height="{height}pt"/>{text}<frame height="{height}pt"/>{text}"#)));
            for page in 0..tree["pages"].as_array().unwrap().len() {
                let rules = nodes_of(&tree["pages"][page], "line");
                let Some(rule) = rules.first() else { continue };
                let rule_y = rule["y1"].as_f64().unwrap();
                for (content, _, y, size) in texts_on(&tree, page) {
                    if size > 10.0 {
                        assert!(y + size * 1.2 <= rule_y + 0.01, "height {height}, page {}: {content:?} ends at {} past the rule at {rule_y}", page + 1, y + size * 1.2);
                    }
                }
            }
        }
    }

    #[test]
    fn a_document_without_footnotes_has_no_rule_and_no_notes() {
        let tree = engine_render(&minimal(r#"<text>Plain text.</text>"#));
        assert!(nodes_of(&tree, "line").is_empty());
        assert_eq!(texts_on(&tree, 0).len(), 1);
    }

    // ── Table rows that keep together ─────────────────────────────────────────

    /// The words on each page of a flow, from the top of the page down.
    fn words_by_page(body: &str) -> Vec<Vec<String>> {
        let tree = engine_render(&minimal(body));
        tree["pages"].as_array().unwrap().iter()
            .map(|page| texts_from_the_top(page).into_iter().map(|t| t.1).collect())
            .collect()
    }

    /// A table of one column, a header `HEAD` and a row for each of `rows` (the words of its cell, or its content when
    /// that starts with `<`, and the attributes of its `tr`), after a frame of `spacer` pt. A page of `minimal` has
    /// 785.89pt of room and a row of one line is 13.2pt, so the spacer decides how many rows the first page has left.
    fn table_after(spacer: u32, rows: &[(&str, &str)]) -> Vec<Vec<String>> {
        let rows: String = rows.iter()
            .map(|(cell, attrs)| {
                let content = if cell.starts_with('<') { cell.to_string() } else { format!("<text>{cell}</text>") };
                format!(r#"<tr {attrs}><td>{content}</td></tr>"#)
            })
            .collect();
        words_by_page(&format!(
            r#"<frame height="{spacer}pt"/><table cols="1fr"><thead><td><text>HEAD</text></td></thead>{rows}</table>"#
        ))
    }

    const KEEP: &str = r#"keep="with-next""#;

    fn words(of: &[&str]) -> Vec<String> {
        of.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_table_header_is_not_left_alone_at_the_bottom_of_a_page() {
        // 20.89pt are left: the header fits and the first row does not, so the table starts on the next page.
        let pages = table_after(765, &[("R1", ""), ("R2", "")]);
        assert_eq!(pages, vec![words(&[]), words(&["HEAD", "R1", "R2"])]);
    }

    #[test]
    fn a_table_row_that_does_not_fit_moves_to_the_next_page_whole() {
        // 35.89pt are left: the header and one row fit, and the three lines of the second do not.
        let pages = table_after(750, &[("R1", ""), ("<text>R2</text><text>b</text><text>c</text>", "")]);
        assert_eq!(pages, vec![words(&["HEAD", "R1"]), words(&["HEAD", "R2", "b", "c"])]);
    }

    #[test]
    fn a_row_that_keeps_with_the_next_moves_on_with_it() {
        // 45.89pt are left: the header and two rows fit, and the break falls between R2 and R3.
        let pages = table_after(740, &[("R1", ""), ("R2", KEEP), ("R3", ""), ("R4", "")]);
        assert_eq!(pages, vec![words(&["HEAD", "R1"]), words(&["HEAD", "R2", "R3", "R4"])]);
    }

    #[test]
    fn a_run_of_rows_that_keep_with_the_next_moves_on_as_one_group() {
        // The same break falls between R2 and R3 of a run of R2, R3 and R4, which ends at R4.
        let pages = table_after(740, &[("R1", ""), ("R2", KEEP), ("R3", KEEP), ("R4", ""), ("R5", "")]);
        assert_eq!(pages, vec![words(&["HEAD", "R1"]), words(&["HEAD", "R2", "R3", "R4", "R5"])]);
    }

    #[test]
    fn a_break_that_falls_between_rows_that_do_not_keep_stays() {
        // R3 keeps with R4, and the break falls before R3 and after R2: nothing is inside the run.
        let pages = table_after(740, &[("R1", ""), ("R2", ""), ("R3", KEEP), ("R4", "")]);
        assert_eq!(pages, vec![words(&["HEAD", "R1", "R2"]), words(&["HEAD", "R3", "R4"])]);
    }

    #[test]
    fn a_run_that_fits_before_the_break_stays_on_the_page() {
        // R1 keeps with R2, and both fit with room to spare: the break comes after R2.
        let pages = table_after(740, &[("R1", KEEP), ("R2", ""), ("R3", ""), ("R4", "")]);
        assert_eq!(pages, vec![words(&["HEAD", "R1", "R2"]), words(&["HEAD", "R3", "R4"])]);
    }

    #[test]
    fn a_kept_run_that_would_leave_only_the_header_moves_the_whole_table() {
        // R1 keeps with R2 and the break falls between them, so no row is left to stay with the header.
        let pages = table_after(750, &[("R1", KEEP), ("R2", ""), ("R3", "")]);
        assert_eq!(pages, vec![words(&[]), words(&["HEAD", "R1", "R2", "R3"])]);
    }

    #[test]
    fn keep_on_the_last_row_of_a_table_has_no_effect() {
        let kept  = table_after(740, &[("R1", ""), ("R2", ""), ("R3", KEEP)]);
        let plain = table_after(740, &[("R1", ""), ("R2", ""), ("R3", "")]);
        assert_eq!(kept, plain);
    }

    #[test]
    fn a_run_taller_than_a_page_breaks_where_its_rows_fit() {
        // Two rows of 400pt that keep together cannot stand on one page with the header. The run splits between
        // them, as it would without the keep.
        let tall = |name: &str| format!(r#"<text>{name}</text><frame height="400pt"/>"#);
        let (a, b, c) = (tall("A"), tall("B"), tall("C"));
        let kept  = table_after(100, &[(&a, KEEP), (&b, ""), (&c, "")]);
        let plain = table_after(100, &[(&a, ""), (&b, ""), (&c, "")]);
        assert_eq!(kept, plain);
        assert_eq!(kept, vec![words(&["HEAD", "A"]), words(&["HEAD", "B"]), words(&["HEAD", "C"])]);
    }

    #[test]
    fn the_table_keep_showcase_moves_a_group_heading_to_the_page_of_its_first_item() {
        // Without the keep, the page would end on the heading "Launch and support".
        let xml = include_str!("../../../test/fixtures/showcase-table-keep.xml");
        let tree = engine_render(xml);
        let pages: Vec<Vec<String>> = tree["pages"].as_array().unwrap().iter()
            .map(|page| texts_from_the_top(page).into_iter().map(|t| t.1).collect())
            .collect();
        assert_eq!(pages.len(), 2, "the showcase should run over two pages");
        let heading = "Launch and support".to_string();
        assert!(!pages[0].contains(&heading), "the heading ends the first page alone");
        let heading_at = pages[1].iter().position(|w| *w == heading).expect("the heading moves to page 2");
        let header_at = pages[1].iter().position(|w| w == "Description").expect("the header repeats on page 2");
        assert!(header_at < heading_at, "{:?}", &pages[1][..8]);
    }

    #[test]
    fn a_kept_run_goes_to_the_next_page_under_the_header() {
        let pages = table_after(740, &[("R1", ""), ("R2", KEEP), ("R3", "")]);
        let second = &pages[1];
        assert_eq!(second.first().map(String::as_str), Some("HEAD"), "{pages:?}");
        assert_eq!(second[1..], words(&["R2", "R3"])[..]);
    }

    // ── Table cells that span columns ─────────────────────────────────────────

    /// A table of four columns of 100pt with 10pt between them, from the page margin at 28pt, and one row whose cells
    /// span `spans` columns each and are filled, so that each draws a box.
    fn span_table(attrs: &str, spans: &[u16]) -> String {
        let cells: String = spans.iter()
            .map(|span| format!(r##"<td colspan="{span}" background="#cccccc"><text>cell</text></td>"##))
            .collect();
        format!(r#"<table cols="100pt 100pt 100pt 100pt" gap="10pt" {attrs}><tr>{cells}</tr></table>"#)
    }

    /// The boxes of the cells of the table that `body` starts the page with.
    fn cells_of(tree: &serde_json::Value) -> Vec<&serde_json::Value> {
        tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap().iter().filter(|n| n["type"] == "box").collect()
    }

    /// The `(x, width)` of the cells of the table that `body` starts the page with, left to right.
    fn cell_boxes(body: &str) -> Vec<(f64, f64)> {
        let tree = engine_render(&minimal(body));
        let mut boxes: Vec<(f64, f64)> = cells_of(&tree).iter()
            .map(|b| (b["x"].as_f64().unwrap(), b["width"].as_f64().unwrap()))
            .collect();
        boxes.sort_by(|a, b| a.0.total_cmp(&b.0));
        boxes
    }

    fn assert_boxes(actual: Vec<(f64, f64)>, expected: &[(f64, f64)]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?}");
        for ((x, w), (ex, ew)) in actual.iter().zip(expected) {
            assert!((x - ex).abs() < 0.01 && (w - ew).abs() < 0.01, "{actual:?} against {expected:?}");
        }
    }

    #[test]
    fn a_cell_without_colspan_takes_one_column() {
        assert_boxes(cell_boxes(&span_table("", &[1, 1, 1, 1])), &[(28.0, 100.0), (138.0, 100.0), (248.0, 100.0), (358.0, 100.0)]);
    }

    #[test]
    fn a_cell_that_spans_columns_at_the_start_of_a_row_is_as_wide_as_them_and_their_gaps() {
        assert_boxes(cell_boxes(&span_table("", &[2, 1, 1])), &[(28.0, 210.0), (248.0, 100.0), (358.0, 100.0)]);
    }

    #[test]
    fn a_cell_that_spans_columns_in_the_middle_of_a_row_pushes_the_cells_after_it() {
        assert_boxes(cell_boxes(&span_table("", &[1, 2, 1])), &[(28.0, 100.0), (138.0, 210.0), (358.0, 100.0)]);
    }

    #[test]
    fn a_cell_that_spans_columns_at_the_end_of_a_row_reaches_the_last_column() {
        assert_boxes(cell_boxes(&span_table("", &[1, 1, 2])), &[(28.0, 100.0), (138.0, 100.0), (248.0, 210.0)]);
    }

    #[test]
    fn a_cell_that_spans_every_column_is_as_wide_as_the_table() {
        assert_boxes(cell_boxes(&span_table("", &[4])), &[(28.0, 430.0)]);
    }

    #[test]
    fn a_row_with_fewer_columns_than_the_table_leaves_the_last_ones_empty() {
        assert_boxes(cell_boxes(&span_table("", &[1, 2])), &[(28.0, 100.0), (138.0, 210.0)]);
    }

    #[test]
    fn a_spanning_cell_lays_its_text_out_at_the_width_it_spans() {
        let lines = |spans: &[u16]| {
            let cells: String = spans.iter()
                .map(|span| format!(r#"<td colspan="{span}"><text>alpha beta gamma delta epsilon zeta</text></td>"#))
                .collect();
            let tree = engine_render(&minimal(&format!(
                r#"<table cols="100pt 100pt 100pt" gap="10pt"><tr>{cells}</tr></table>"#
            )));
            texts_on(&tree, 0).len()
        };
        assert!(lines(&[1]) > 1, "the words need more than one line in a column of 100pt");
        assert_eq!(lines(&[3]), 1, "and one line in the 320pt of three columns");
    }

    #[test]
    fn a_spanning_cell_does_not_change_the_height_of_a_row_of_plain_cells() {
        let height = |spans: &[u16]| {
            let tree = engine_render(&minimal(&span_table("", spans)));
            cells_of(&tree)[0]["height"].as_f64().unwrap()
        };
        assert_eq!(height(&[1, 1, 1, 1]), height(&[2, 1, 1]));
    }

    #[test]
    fn a_grid_border_has_no_column_rule_inside_a_spanning_cell() {
        let rules = |spans: &[u16]| -> Vec<f64> {
            let tree = engine_render(&minimal(&span_table(r##"border="1pt #000000""##, spans)));
            let mut xs: Vec<f64> = nodes_of(&tree["pages"][0], "line").iter()
                .filter(|l| l["x1"] == l["x2"])
                .map(|l| (l["x1"].as_f64().unwrap() * 100.0).round() / 100.0)
                .collect();
            xs.sort_by(f64::total_cmp);
            xs
        };
        // The edges of the table, which is as wide as the page allows, and a rule in the middle of each gap between
        // two cells.
        let plain = rules(&[1, 1, 1, 1]);
        assert_eq!(plain, vec![28.5, 133.0, 243.0, 353.0, 566.78], "{plain:?}");
        let spanned = rules(&[2, 1, 1]);
        assert_eq!(spanned, vec![28.5, 243.0, 353.0, 566.78], "the rule at 133 is inside the first cell: {spanned:?}");
    }

    #[test]
    fn a_spanning_cell_does_not_change_where_a_table_breaks() {
        // The span changes widths only: the rows break at the same places as in a table without one.
        let pages = |span: u16| -> Vec<Vec<String>> {
            let rows: String = (1..=120).map(|n| format!(r#"<tr><td colspan="{span}"><text>Row {n}</text></td></tr>"#)).collect();
            words_by_page(&format!(r#"<table cols="1fr 1fr">{rows}</table>"#))
        };
        assert!(pages(2).len() > 1, "the table fills more than a page");
        assert_eq!(pages(1), pages(2));
    }

    #[test]
    fn the_table_colspan_showcase_puts_a_subtotal_label_across_the_columns_before_the_amount() {
        let xml = include_str!("../../../test/fixtures/showcase-table-colspan.xml");
        let tree = engine_render(xml);
        assert_eq!(tree["pages"].as_array().unwrap().len(), 1, "the showcase fits on one page");
        let x_of = |word: &str| texts_on(&tree, 0).into_iter().find(|t| t.0 == word).unwrap_or_else(|| panic!("no {word}")).1;
        // Right-aligned in one column, the label would end before the Qty column; across three, it ends where the
        // third does, so it starts to the right of the Qty header.
        assert!(x_of("Subtotal") > x_of("Qty"), "Subtotal at {}, Qty at {}", x_of("Subtotal"), x_of("Qty"));
    }

    // ── Table cells that span rows ────────────────────────────────────────────

    /// The `[x, y, width, height]` of the cell boxes of the table that `body` starts the page with, from the top left.
    fn cell_rects(body: &str) -> Vec<[f64; 4]> {
        let tree = engine_render(&minimal(body));
        let mut rects: Vec<[f64; 4]> = cells_of(&tree).iter()
            .map(|b| ["x", "y", "width", "height"].map(|key| b[key].as_f64().unwrap()))
            .collect();
        rects.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
        rects
    }

    fn assert_rects(actual: Vec<[f64; 4]>, expected: &[[f64; 4]]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?}");
        for (rect, want) in actual.iter().zip(expected) {
            assert!(rect.iter().zip(want).all(|(a, b)| (a - b).abs() < 0.01), "{actual:?} against {expected:?}");
        }
    }

    /// A cell with a background, so that it draws a box, and the attributes and the words in it.
    fn cell(attrs: &str, content: &str) -> String {
        let content = if content.starts_with('<') { content.to_string() } else { format!("<text>{content}</text>") };
        format!(r##"<td {attrs} background="#cccccc">{content}</td>"##)
    }

    // A row of one line is 13.2pt high.

    #[test]
    fn a_cell_that_spans_rows_is_as_tall_as_them_and_the_gaps_between_them() {
        let body = format!(
            r#"<table cols="100pt 100pt" gap="10pt"><tr>{}{}</tr><tr>{}</tr><tr>{}</tr></table>"#,
            cell(r#"rowspan="3""#, "A"), cell("", "B"), cell("", "C"), cell("", "D"),
        );
        assert_rects(cell_rects(&body), &[
            [28.0, 28.0, 100.0, 59.6],
            [138.0, 28.0, 100.0, 13.2],
            [138.0, 51.2, 100.0, 13.2],
            [138.0, 74.4, 100.0, 13.2],
        ]);
    }

    #[test]
    fn the_cells_of_the_rows_under_a_span_start_after_the_columns_it_holds() {
        // The span is in the middle column: the cells of the next row go to the first and the last.
        let body = format!(
            r#"<table cols="100pt 100pt 100pt"><tr>{}{}{}</tr><tr>{}{}</tr></table>"#,
            cell("", "A"), cell(r#"rowspan="2""#, "B"), cell("", "C"), cell("", "D"), cell("", "E"),
        );
        assert_rects(cell_rects(&body), &[
            [28.0, 28.0, 100.0, 13.2],
            [128.0, 28.0, 100.0, 26.4],
            [228.0, 28.0, 100.0, 13.2],
            [28.0, 41.2, 100.0, 13.2],
            [228.0, 41.2, 100.0, 13.2],
        ]);
    }

    #[test]
    fn a_cell_with_a_colspan_and_a_rowspan_covers_the_columns_and_the_rows() {
        let body = format!(
            r#"<table cols="100pt 100pt 100pt 100pt" gap="10pt"><tr>{}{}{}</tr><tr>{}{}</tr></table>"#,
            cell(r#"colspan="2" rowspan="2""#, "A"), cell("", "B"), cell("", "C"), cell("", "D"), cell("", "E"),
        );
        assert_rects(cell_rects(&body), &[
            [28.0, 28.0, 210.0, 36.4],
            [248.0, 28.0, 100.0, 13.2],
            [358.0, 28.0, 100.0, 13.2],
            [248.0, 51.2, 100.0, 13.2],
            [358.0, 51.2, 100.0, 13.2],
        ]);
    }

    #[test]
    fn a_cell_taller_than_the_rows_it_spans_makes_the_last_of_them_grow() {
        // Five lines are 66pt, and the two rows are 26.4pt: the second one grows by what is missing.
        let five = "<text>1</text><text>2</text><text>3</text><text>4</text><text>5</text>";
        let body = format!(
            r#"<table cols="100pt 100pt"><tr>{}{}</tr><tr>{}</tr></table>"#,
            cell(r#"rowspan="2""#, five), cell("", "B"), cell("", "C"),
        );
        assert_rects(cell_rects(&body), &[
            [28.0, 28.0, 100.0, 66.0],
            [128.0, 28.0, 100.0, 13.2],
            [128.0, 41.2, 100.0, 52.8],
        ]);
    }

    #[test]
    fn a_cell_shorter_than_the_rows_it_spans_is_stretched_to_them() {
        let body = format!(
            r#"<table cols="100pt 100pt"><tr>{}{}</tr><tr>{}</tr></table>"#,
            cell(r#"rowspan="2""#, "A"), cell("", "<text>1</text><text>2</text><text>3</text>"), cell("", "<text>4</text><text>5</text>"),
        );
        assert_rects(cell_rects(&body), &[
            [28.0, 28.0, 100.0, 66.0],
            [128.0, 28.0, 100.0, 39.6],
            [128.0, 67.6, 100.0, 26.4],
        ]);
    }

    #[test]
    fn a_spanning_cell_is_drawn_after_the_backgrounds_of_the_rows_it_spans() {
        let tree = engine_render(&minimal(&format!(
            r##"<table cols="100pt 100pt"><tr background="#eeeeee">{}{}</tr><tr background="#dddddd">{}</tr></table>"##,
            cell(r#"rowspan="2""#, "A"), cell("", "B"), cell("", "C"),
        )));
        let nodes = tree["pages"][0]["nodes"][0]["nodes"].as_array().unwrap();
        let at = |fill: &str| nodes.iter().position(|n| n["fill"] == fill).unwrap_or_else(|| panic!("no {fill}: {nodes:?}"));
        // The span's own fill is #cccccc, and it is the box that is two rows tall.
        let span = nodes.iter().position(|n| n["fill"] == "#cccccc" && n["height"].as_f64().unwrap() > 20.0).unwrap();
        assert!(span > at("#dddddd"), "the span is drawn before the background of the second row, which covers it");
    }

    #[test]
    fn the_grid_has_no_rule_under_a_row_where_a_cell_spans_on() {
        let tree = engine_render(&minimal(&format!(
            r##"<table cols="100pt 100pt" border="1pt #000000"><tr>{}{}</tr><tr>{}</tr></table>"##,
            cell(r#"rowspan="2""#, "A"), cell("", "B"), cell("", "C"),
        )));
        let round = |v: &serde_json::Value| (v.as_f64().unwrap() * 100.0).round() / 100.0;
        let mut rules: Vec<(f64, f64, f64)> = nodes_of(&tree["pages"][0], "line").iter()
            .filter(|l| l["y1"] == l["y2"])
            .map(|l| (round(&l["y1"]), round(&l["x1"]), round(&l["x2"])))
            .collect();
        rules.sort_by(|a, b| a.0.total_cmp(&b.0));
        // The top and the bottom of the table cross it whole, and the rule between the rows starts where the span ends.
        assert_eq!(rules, vec![(28.5, 28.0, 567.28), (40.7, 128.0, 567.28), (53.9, 28.0, 567.28)], "{rules:?}");
    }

    /// The horizontal lines of a table with a header row and two rows, as (thickness, color, x1, x2), top to bottom.
    fn lines_of_table(attrs: &str, body: &str) -> Vec<(f64, String, f64, f64)> {
        let tree = engine_render(&minimal(&format!(r#"<table cols="100pt 100pt" {attrs}>{body}</table>"#)));
        let round = |v: &serde_json::Value| (v.as_f64().unwrap() * 100.0).round() / 100.0;
        let mut lines: Vec<(f64, f64, String, f64, f64)> = nodes_of(&tree["pages"][0], "line").iter()
            .map(|l| (round(&l["y1"]), round(&l["thickness"]), l["color"].as_str().unwrap().to_string(), round(&l["x1"]), round(&l["x2"])))
            .collect();
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        lines.into_iter().map(|(_, thickness, color, x1, x2)| (thickness, color, x1, x2)).collect()
    }

    fn header_and_two_rows() -> String {
        format!("<thead>{}{}</thead><tr>{}{}</tr><tr>{}{}</tr>", cell("", "H1"), cell("", "H2"), cell("", "A1"), cell("", "A2"), cell("", "B1"), cell("", "B2"))
    }

    #[test]
    fn a_table_has_a_line_under_its_header_and_under_each_of_its_rows() {
        let lines = lines_of_table(r##"head-border="1pt #111111" row-border="0.5pt #dddddd""##, &header_and_two_rows());
        // The width of the table is the page less the margins, as the grid border has it.
        let across = (28.0, 567.28);
        assert_eq!(
            lines,
            vec![
                (1.0, "#111111".to_string(), across.0, across.1),
                (0.5, "#dddddd".to_string(), across.0, across.1),
                (0.5, "#dddddd".to_string(), across.0, across.1),
            ],
            "the header line takes the place of the row line under the header: {lines:?}"
        );
    }

    #[test]
    fn a_header_line_alone_leaves_the_rows_without_lines_and_a_row_line_alone_goes_under_the_header_too() {
        let body = header_and_two_rows();
        assert_eq!(lines_of_table(r##"head-border="1pt #111111""##, &body).len(), 1);
        let lines = lines_of_table(r##"row-border="0.5pt #dddddd""##, &body);
        assert_eq!(lines.iter().map(|l| l.1.as_str()).collect::<Vec<_>>(), ["#dddddd"; 3]);
        assert!(lines_of_table("", &body).is_empty(), "a table with neither has no lines");
    }

    #[test]
    fn none_turns_a_line_off() {
        let body = header_and_two_rows();
        assert!(lines_of_table(r#"head-border="none" row-border="none""#, &body).is_empty());
    }

    #[test]
    fn a_row_line_stops_where_a_cell_spans_on_into_the_next_row() {
        let lines = lines_of_table(
            r##"row-border="0.5pt #dddddd""##,
            &format!("<tr>{}{}</tr><tr>{}</tr>", cell(r#"rowspan="2""#, "A"), cell("", "B"), cell("", "C")),
        );
        // The line under the first row starts where the span ends; the one under the last row crosses the table.
        let from: Vec<f64> = lines.iter().map(|l| l.2).collect();
        assert_eq!(from, vec![128.0, 28.0], "{lines:?}");
    }

    #[test]
    fn the_grid_has_no_column_rule_inside_a_cell_that_spans_rows_and_columns() {
        let tree = engine_render(&minimal(&format!(
            r##"<table cols="100pt 100pt 100pt" border="1pt #000000"><tr>{}{}</tr><tr>{}</tr></table>"##,
            cell(r#"colspan="2" rowspan="2""#, "A"), cell("", "B"), cell("", "C"),
        )));
        let rule_xs: Vec<f64> = nodes_of(&tree["pages"][0], "line").iter()
            .filter(|l| l["x1"] == l["x2"])
            .map(|l| (l["x1"].as_f64().unwrap() * 100.0).round() / 100.0)
            .collect();
        // The edges of the table are drawn once, and the rule between the second and the third column twice, once for
        // each row. The one in the middle of the cell, at 128, is not drawn in either row.
        assert!(!rule_xs.contains(&128.0), "{rule_xs:?}");
        assert_eq!(rule_xs.iter().filter(|x| **x == 228.0).count(), 2, "{rule_xs:?}");
    }

    /// The words on each page of a table, after a frame of `spacer` pt, with a header and the `rows` given.
    fn span_table_after(spacer: u32, rows: &str) -> Vec<Vec<String>> {
        let frame = if spacer == 0 { String::new() } else { format!(r#"<frame height="{spacer}pt"/>"#) };
        let mut pages = words_by_page(&format!(
            r#"{frame}<table cols="1fr 1fr"><thead><td><text>HEAD</text></td><td><text>H2</text></td></thead>{rows}</table>"#
        ));
        pages.iter_mut().for_each(|page| page.sort());
        pages
    }

    fn sorted(of: &[&str]) -> Vec<String> {
        let mut words = words(of);
        words.sort();
        words
    }

    const R1: &str = "<tr><td><text>R1</text></td><td><text>x</text></td></tr>";
    const GROUP: &str = r#"<tr><td rowspan="2"><text>Group</text></td><td><text>G1</text></td></tr><tr><td><text>G2</text></td></tr>"#;
    const R4: &str = "<tr><td><text>R4</text></td><td><text>y</text></td></tr>";

    #[test]
    fn rows_that_a_cell_spans_move_to_the_next_page_together_when_they_do_not_fit() {
        // 45.89pt are left: the header, R1 and the first row of the group fit, and the second row of the group does not.
        let pages = span_table_after(740, &format!("{R1}{GROUP}{R4}"));
        assert_eq!(pages, vec![
            sorted(&["HEAD", "H2", "R1", "x"]),
            sorted(&["HEAD", "H2", "Group", "G1", "G2", "R4", "y"]),
        ]);
    }

    #[test]
    fn rows_that_a_cell_spans_stay_on_the_page_when_they_fit() {
        // 85.89pt are left, which is room for the header, R1 and both rows of the group.
        let pages = span_table_after(700, &format!("{R1}{GROUP}{R4}"));
        assert_eq!(pages, vec![sorted(&["HEAD", "H2", "R1", "x", "Group", "G1", "G2", "R4", "y"])]);
    }

    #[test]
    fn a_span_that_a_keep_run_leads_into_moves_with_the_run() {
        // R1 keeps with the group, so the break that falls inside the group takes R1 along.
        let kept = R1.replace("<tr>", r#"<tr keep="with-next">"#);
        let pages = span_table_after(740, &format!("{kept}{GROUP}{R4}"));
        assert_eq!(pages, vec![
            sorted(&[]),
            sorted(&["HEAD", "H2", "R1", "x", "Group", "G1", "G2", "R4", "y"]),
        ]);
    }

    /// Two rows of 400pt that a cell spans: together they are taller than the room a page has for rows.
    fn tall_group() -> String {
        let tall = |name: &str| format!(r#"<text>{name}</text><frame height="400pt"/>"#);
        format!(
            r#"<tr><td rowspan="2"><text>Group</text></td><td>{}</td></tr><tr><td>{}</td></tr>"#,
            tall("G1"), tall("G2")
        )
    }

    #[test]
    fn rows_that_a_cell_spans_and_that_are_taller_than_a_page_are_placed_whole_where_they_start_a_page() {
        // The group is placed whole and the page clips it at its end. The rows after it go on.
        let pages = span_table_after(0, &format!("{}{R4}", tall_group()));
        assert_eq!(pages, vec![
            sorted(&["HEAD", "H2", "Group", "G1", "G2"]),
            sorted(&["HEAD", "H2", "R4", "y"]),
        ]);
    }

    #[test]
    fn rows_that_a_cell_spans_and_that_are_taller_than_a_page_start_the_next_page_when_something_is_before_them() {
        let pages = span_table_after(0, &format!("{R1}{}{R4}", tall_group()));
        assert_eq!(pages, vec![
            sorted(&["HEAD", "H2", "R1", "x"]),
            sorted(&["HEAD", "H2", "Group", "G1", "G2"]),
            sorted(&["HEAD", "H2", "R4", "y"]),
        ]);
    }

    #[test]
    fn a_group_of_rows_that_a_cell_spans_is_not_split_between_pages() {
        // Sixty rows of one line are 792pt, a little more than a page has. Row 60 does not fit, and it ends the group
        // that starts at row 58, so without the span only row 60 would be on the second page.
        let rows = |spanning: bool| -> String {
            (1..=60).map(|n| {
                let span = if spanning && n % 3 == 1 { r#" rowspan="3""# } else { "" };
                let second = if spanning && n % 3 != 1 { String::new() } else { format!("<td><text>b{n}</text></td>") };
                format!("<tr><td{span}><text>a{n}</text></td>{second}</tr>")
            }).collect()
        };
        let on_page = |pages: &[Vec<String>], word: &str| pages.iter().position(|page| page.iter().any(|w| w == word));
        let plain = words_by_page(&format!(r#"<table cols="1fr 1fr">{}</table>"#, rows(false)));
        assert_eq!((on_page(&plain, "a58"), on_page(&plain, "a60")), (Some(0), Some(1)), "the table breaks inside what is the group");
        let spans = words_by_page(&format!(r#"<table cols="1fr 1fr">{}</table>"#, rows(true)));
        assert_eq!((on_page(&spans, "a58"), on_page(&spans, "a60")), (Some(1), Some(1)), "the group moves on whole");
        assert_eq!(on_page(&spans, "a57"), Some(0));
    }

    #[test]
    fn the_table_rowspan_showcase_moves_the_quarter_that_does_not_fit_to_the_second_page_whole() {
        // Page 1 has room left after Q1 2025, but not for the four rows of Q2 2025, whose label and notes span them.
        let xml = include_str!("../../../test/fixtures/showcase-table-rowspan.xml");
        let tree = engine_render(xml);
        let pages: Vec<Vec<String>> = tree["pages"].as_array().unwrap().iter()
            .map(|page| texts_from_the_top(page).into_iter().map(|t| t.1).collect())
            .collect();
        assert_eq!(pages.len(), 2, "the showcase should run over two pages");
        let q2 = ["Q2 2025", "$58,400.00", "$61,950.00", "$64,300.00", "$184,650.00"];
        for word in q2 {
            assert!(!pages[0].iter().any(|w| w == word), "{word} is on the first page");
            assert!(pages[1].iter().any(|w| w == word), "{word} is not on the second page");
        }
        assert!(pages[0].iter().any(|w| w == "$158,600.00"), "Q1 2025 stays on the first page");
        let header_at = pages[1].iter().position(|w| w == "Quarter").expect("the header repeats on page 2");
        let label_at = pages[1].iter().position(|w| w == "Q2 2025").unwrap();
        assert!(header_at < label_at, "{:?}", &pages[1][..8]);
    }

    #[test]
    fn a_table_whose_rows_data_binding_changed_still_lays_out_without_the_cells_that_have_no_column() {
        // The template is fine: the row with the span has a row after it. With three items there are three of the
        // first row, whose cells then run into the span above: the cell that has no free column is left out.
        let xml = minimal(
            r#"<table cols="1fr 1fr"><tr data-source="items"><td rowspan="2"><text data-value="name">n</text></td><td><text data-value="name">n</text></td></tr><tr><td><text>last</text></td></tr></table>"#,
        );
        let mut doc = parse(&xml).unwrap();
        crate::data::apply(&mut doc, r#"{"items":[{"name":"a"},{"name":"b"},{"name":"c"}]}"#).unwrap();
        let layouts = doc.section_layouts();
        let pages: Vec<_> = layouts.iter().flat_map(layout_page).collect();
        assert!(!pages.is_empty());
    }

    #[test]
    fn bound_attributes_reach_the_rendered_page_for_each_row() {
        let xml = r##"<lpdf version="1"><tokens><colors><color name="late" value="#cc0000"/></colors></tokens>
            <document size="a4"><section><layout margin="28pt"><column data-source="items">
                <shape type="rect" width="60pt" height="10pt" fill="#999999" data-fill="color"/>
                <text color="#000000" data-color="tone" data-value="label">Label</text>
            </column></layout></section></document></lpdf>"##;
        let mut doc = parse(xml).unwrap();
        crate::data::apply(
            &mut doc,
            r##"{"items":[{"color":"#00ff00","tone":"late","label":"One"},{"label":"Two"}]}"##,
        )
        .unwrap();
        let lp = doc.section_layouts();
        let pages: Vec<_> = lp.iter().flat_map(layout_page).collect();
        let tree: serde_json::Value =
            serde_json::from_str(&crate::render::pages_to_json(&pages, serde_json::Value::Null, serde_json::Value::Null)).unwrap();

        let fills: Vec<_> = nodes_of(&tree, "canvas-rect").iter().map(|rect| rect["fill"].as_str().map(str::to_owned)).collect();
        assert_eq!(fills, vec![Some("#00ff00".to_owned()), Some("#999999".to_owned())]);
        let colors: Vec<_> = nodes_of(&tree, "text").iter().map(|text| text["color"].as_str().map(str::to_owned)).collect();
        assert_eq!(colors, vec![Some("#cc0000".to_owned()), Some("#000000".to_owned())]);
    }
}
