//! A section whose page grows to fit its content (`grow`), from its `size` as the starting size, the smallest the
//! page can be: in width until its content fits without being squeezed, and in height until its flow is one page.
//!
//! The size is worked out here, from the same nodes `fill_pages` fills (`section_flow`), and the section is then
//! filled and finished at it as any other section is (`fill_section`). So what depends on the size of the page takes
//! the final one: the footer and the footnotes sit at its foot, the flow is laid out at its width, a layer's anchor
//! places against it, and the background and the ruling cover it.

use std::rc::Rc;

use super::*;
use crate::parse::MAX_PAGE;

/// The section `page` at the size its page grows to. A page that would grow past `MAX_PAGE` in either size stops at
/// it, and the layout fails with an error that names the section.
pub(super) fn grown(page: &SectionLayout) -> SectionLayout {
    let mut grown = page.clone();
    bind_bulk_records(&mut grown);
    // The nodes the page is filled from, whatever its size.
    let parts = section_flow(&grown);
    if page.grow.width {
        let wanted = content_width(&grown, &parts);
        if wanted > page.width {
            grown.width = within_limit(page, wanted, "in width");
        }
    }
    if page.grow.height {
        // At the final width, which the flow wraps at.
        let (wanted, start_room) = needed_height(&grown, parts);
        if wanted > page.height {
            grown.height = within_limit(page, wanted, "in height");
            let range = flow_range(&grown.children);
            for node in &mut grown.children[range] {
                as_grown(node, start_room);
            }
        }
    }
    let (dw, dh) = (grown.width - page.width, grown.height - page.height);
    if dw > 0.0 || dh > 0.0 {
        for layer in grown.underlays.iter_mut().chain(grown.overlays.iter_mut()) {
            layer.follow_grown_page(dw, dh);
        }
        redraw_ruling(&mut grown);
    }
    grown
}

/// Bind every record of each bulk container in the flow of `page`, once: the width of a page that grows is measured
/// over all of them, and a page that grows in height holds them all.
fn bind_bulk_records(page: &mut SectionLayout) {
    let range = flow_range(&page.children);
    for node in &mut page.children[range] {
        if let Some(records) = node.bulk_records().cloned() {
            let whole = std::mem::replace(node, Node::layout_default());
            *node = Bulk::new(whole, records).into_node();
        }
    }
}

/// `wanted`, the size the page of `page` grows to, or `MAX_PAGE` when it is larger, which fails the layout.
fn within_limit(page: &SectionLayout, wanted: f32, size: &str) -> f32 {
    if wanted <= MAX_PAGE {
        return wanted;
    }
    refuse(format!(
        "the page of section {} would grow past 14,400 pt (200 in) {size}, the largest page PDF 1.7 recommends",
        page.number,
    ));
    MAX_PAGE
}

/// A node of the flow as it is on a page that grew in height: of `fill` height, as high as its content, and of `full`
/// height, as high as the room of a page of the starting size, `start_room`. Neither grows the page by itself, and on
/// a page that keeps its starting size both take what they take on any page.
fn as_grown(node: &mut Node, start_room: f32) {
    match node.height_mode {
        HeightMode::Fill => node.height_mode = HeightMode::Auto,
        HeightMode::Full => node.height_mode = HeightMode::Fixed(start_room),
        _ => {}
    }
}

/// The width the page of `page` needs for its content, `parts`, to fit without being squeezed: its widest node, in the
/// flow or in the header or the footer, and the margins.
fn content_width(page: &SectionLayout, parts: &SectionFlow) -> f32 {
    let [_, mr, _, ml] = page.margin;
    let SectionFlow { top, flow, bottom, .. } = parts;
    let widest = top.iter().chain(flow).chain(bottom).map(grow_width).fold(0.0_f32, f32::max);
    ml + widest + mr
}

/// The width `node` needs not to be squeezed. A node of a fixed width needs it, as a box, an image or a column of a
/// table that sets one does. A table needs each of its columns as wide as its cells on one line, and a column that is
/// a share of the page (`fr`, `%`) is measured so too. A paragraph and a code need nothing: they wrap at the width of
/// the page, and so does anything else that takes the width it is given.
fn grow_width(node: &Node) -> f32 {
    if !node.width_fill && let Some(width) = own_width(node) {
        return width;
    }
    let [_, pr, _, pl] = node.padding;
    let gaps = |count: usize| node.gap * count.saturating_sub(1) as f32;
    match node.kind {
        NodeKind::Text | NodeKind::Img | NodeKind::Shape | NodeKind::Barcode | NodeKind::Field | NodeKind::Divider => 0.0,
        NodeKind::Table => table_width(node),
        NodeKind::Row => pl + pr + node.children.iter().map(grow_width).sum::<f32>() + gaps(node.children.len()),
        NodeKind::Grid => {
            let widest = node.children.iter().map(grow_width).fold(0.0_f32, f32::max);
            match node.col_width {
                // A grid that fits as many columns as the width holds needs one.
                Some(least) => pl + pr + widest.max(least),
                None => {
                    let cols = node.cols.max(1) as usize;
                    pl + pr + cols as f32 * widest + gaps(cols)
                }
            }
        }
        _ => pl + pr + node.children.iter().map(grow_width).fold(0.0_f32, f32::max),
    }
}

/// The width a table needs not to be squeezed: each column with a width of its own at it, and each other one, a share
/// of the page (`fr`, `%`) as an `auto` one, as wide as its cells on one line. A cell that spans columns gives what it
/// lacks to the ones without a width of their own, as the table lays them out (`table_col_widths`), so a long cell
/// over a fixed and an `auto` column widens the `auto` one.
fn table_width(node: &Node) -> f32 {
    let units: Vec<ColUnit> = col_units(&node.table_cols).into_iter()
        .map(|unit| match unit {
            ColUnit::Pt(width) => ColUnit::Pt(width),
            _ => ColUnit::Auto,
        })
        .collect();
    if units.is_empty() {
        return 0.0;
    }
    let extents = column_extents(&node.children, &units, 0.0, node.gap, false);
    let columns: f32 = units.iter().zip(&extents).map(|(unit, extent)| match unit {
        ColUnit::Pt(width) => *width,
        _ => extent.max,
    }).sum();
    let [_, pr, _, pl] = node.padding;
    columns + node.gap * (units.len() - 1) as f32 + pl + pr
}

/// The height the page of `page` needs at its width for its content, `parts`, to be one page, as `fill_pages` fills it:
/// the margins, the header and the footer, the flow and the notes of all its footnotes. With it, the room for the flow
/// on a page of the starting size, which a node of `full` height keeps on a page that grows (`as_grown`).
fn needed_height(page: &SectionLayout, parts: SectionFlow) -> (f32, f32) {
    let [mt, mr, mb, ml] = page.margin;
    let avail_w = (page.width - ml - mr).max(0.0);
    let SectionFlow { top, mut flow, bottom, has_notes } = parts;
    // The header and the footer as `fill_pages` measures them for the room of the flow on the first page.
    let chrome = measure_chrome_height(&top, avail_w, true) + measure_chrome_height(&bottom, avail_w, true);
    let start_room = (page.height - mt - mb - chrome).max(0.0);
    for node in &mut flow {
        as_grown(node, start_room);
    }
    // The flow as the page lays it out above its notes (`flow_pages`), each anchor with the number it shows there.
    let one = footnotes::one_page(&flow, avail_w, f32::INFINITY, page.footnote_counter, has_notes);
    let (_, flow_h) = layout_column(&one.nodes, 0.0, 0.0, avail_w, None, page.gap, &Align::Stretch, &Justify::Start);
    (mt + chrome + flow_h + one.reserve + mb, start_room)
}

/// Draw the lines of the ruling of `page` again at the size its page grew to, filling the grown page or the area
/// inside its margin.
fn redraw_ruling(page: &mut SectionLayout) {
    let Some(ruling) = &page.ruling else { return };
    let drawing = match crate::ruling::draw(ruling, page.width, page.height, page.margin) {
        Ok(drawing) => Rc::new(drawing),
        // A page that grows only widens the parts a count cuts it into, which the ruling was checked with at the
        // starting size, so this is not seen.
        Err(error) => return refuse(format!("the ruling of section {}: {error}", page.number)),
    };
    for layer in &mut page.underlays {
        for child in &mut layer.children {
            if let canvas::CanvasPrimitive::Ruling(lines) = child {
                *lines = Rc::clone(&drawing);
            }
        }
    }
}
