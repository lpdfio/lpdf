use std::collections::HashSet;

use serde_json::{json, Value};
use crate::parse::{FieldKind, Shadow};

// ── Render tree types ─────────────────────────────────────────────────────────

pub struct RenderPage {
    pub width: f32,
    pub height: f32,
    pub background: Option<String>,
    pub margin: [f32; 4],
    pub nodes: Vec<RenderNode>,
}

pub enum RenderNode {
    Box(RenderBox),
    Line(RenderLine),
    Text(RenderText),
    Link(RenderLink),
    Image(RenderImage),
    Barcode(RenderBarcode),
    Field(RenderField),
    // What a layer draws — constructed by canvas.rs. Its text, images and barcodes are the
    // layout's own nodes (`Box`, `Text`, `Image`, `Barcode`), put in `CanvasLayer`.
    CanvasRect(RenderCanvasRect),
    CanvasLine(RenderCanvasLine),
    CanvasEllipse(RenderCanvasEllipse),
    CanvasPath(RenderCanvasPath),
    CanvasLayer(RenderCanvasLayer),
    Anchor(RenderAnchor),
    FootnoteMark(RenderFootnoteMark),
}

/// What a drawn node is to a reader of a tagged PDF: the structure element it is drawn in, or decoration. Only the
/// layout of a document with `<output pdfua>` sets one (`layout::tag_of`); `pdf.rs` writes them as the structure tree
/// and the marked content of the pages. Boxed where it is held, so a document that is not tagged pays 8 bytes a node.
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// The element. The parts of a node that is split across lines or pages share it, so they stay one element;
    /// 0 is an element of its own.
    pub id:   u32,
    pub kind: TagKind,
}

/// The kinds of structure element the layout tags, and decoration.
#[derive(Debug, Clone, PartialEq)]
pub enum TagKind {
    Paragraph,
    /// A heading, its level 1 to 6.
    Heading(u8),
    Table,
    /// The header row of a table: a `THead` with its one row.
    TableHead,
    /// A row of the table's body.
    TableRow,
    /// A cell: a header cell of the header row, or a data cell, the columns and rows it spans, and the column it starts
    /// in, which keeps the cells of a row in order when a cell that spans rows is drawn after the rows it spans.
    TableCell { header: bool, col_span: u16, row_span: u16, column: u16 },
    List,
    ListItem,
    ListLabel,
    ListBody,
    /// An image, a barcode or a layer, with what it shows.
    Figure(String),
    /// A footnote's note, at the foot of its page.
    Note,
    /// An entry of the table of contents.
    TocItem,
    /// Drawn, not read: a header, a footer, a layer without `alt`, an image with `alt=""`.
    Artifact,
}

/// The place a bookmark marks: an entry of the PDF outline, and a named destination for `href="#id"` links.
/// It draws nothing.
pub struct RenderAnchor {
    pub id:    String,
    pub label: String,
    /// Outline depth, 1 to 6.
    pub level: u8,
    /// Top of the marked element, from the top of the page.
    pub y:     f32,
}

/// Where the number of a footnote is drawn. It draws nothing: pagination reads it to leave out the note of a number
/// that the bottom of the page cuts off (`footnotes`).
pub struct RenderFootnoteMark {
    /// The footnote's place among those of its flow (`Footnote::index`).
    pub index: u32,
    /// Top of the number, from the top of the page.
    pub y:     f32,
}

/// An anchor and the index of the page it is on.
pub struct PlacedAnchor<'a> {
    pub page:   usize,
    pub anchor: &'a RenderAnchor,
}

/// The anchors of `pages` in document order. An id seen before is skipped: a node repeated on several pages,
/// or split across them, marks the place it starts.
pub fn placed_anchors(pages: &[RenderPage]) -> Vec<PlacedAnchor<'_>> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for (page, render_page) in pages.iter().enumerate() {
        collect_anchors(&render_page.nodes, page, &mut seen, &mut out);
    }
    out
}

fn collect_anchors<'a>(
    nodes: &'a [RenderNode],
    page:  usize,
    seen:  &mut HashSet<&'a str>,
    out:   &mut Vec<PlacedAnchor<'a>>,
) {
    for node in nodes {
        match node {
            RenderNode::Anchor(anchor) => {
                if seen.insert(anchor.id.as_str()) {
                    out.push(PlacedAnchor { page, anchor });
                }
            }
            RenderNode::Box(b)  => collect_anchors(&b.children, page, seen, out),
            RenderNode::Link(l) => collect_anchors(&l.children, page, seen, out),
            RenderNode::CanvasLayer(l) => collect_anchors(&l.children, page, seen, out),
            _ => {}
        }
    }
}

pub struct RenderBarcode {
    pub x:          f32,
    pub y:          f32,
    pub width:      f32,
    pub height:     f32,
    pub kind:       RenderedBarcodeKind,
    pub color:      String,
    pub bg:         Option<String>,
    pub debug_self: bool,
    pub tag:        Option<Box<Tag>>,
}

pub enum RenderedBarcodeKind {
    /// Flat row-major grid of dark (true) / light (false) modules.
    Qr { modules: Vec<bool>, size: u32 },
    /// Alternating bar/space run-lengths (even index = bar), plus optional
    /// human-readable text string.
    Code128 { bars: Vec<u8>, hrt: Option<String> },
    /// Same run-length encoding for EAN-13, plus the 13-digit string.
    Ean13 { bars: Vec<u8>, digits: String, hrt: bool },
}

pub struct RenderImage {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub name: String,
    /// The corner radius the picture is clipped to; 0 draws it as a rectangle.
    pub radius: f32,
    pub tag: Option<Box<Tag>>,
}

#[derive(Clone)]
pub struct RenderField {
    pub x:            f32,
    pub y:            f32,
    pub width:        f32,
    pub height:       f32,
    pub kind:         FieldKind,
    pub name:         String,
    pub value:        String,
    pub label:        String,
    pub options:      Vec<String>,
    pub required:     bool,
    pub readonly:     bool,
    pub checked:      bool,
    pub max_len:      Option<u32>,
    pub group:        Option<String>,
    pub action_url:   Option<String>,
    pub background:   Option<String>,
    pub border:       Option<(f32, String)>,
}

pub struct RenderBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub fill: Option<String>,
    pub border_width: f32,
    pub border_color: Option<String>,
    pub radius: f32,
    /// 0.0–1.0 opacity of the box and its children, drawn as one (1.0 = fully opaque).
    pub opacity: f32,
    /// A copy of the box's outline, offset and filled, drawn under the box. Faded with the box.
    pub shadow: Option<Shadow>,
    pub debug_self: bool,
    pub tag: Option<Box<Tag>>,
    pub children: Vec<RenderNode>,
}

pub struct RenderLine {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub color: String,
    pub thickness: f32,
    pub dash: Option<Vec<f32>>,
}

pub struct RenderText {
    /// Anchor x position. Meaning depends on `text_align`:
    /// - "left"   → left edge of the text run
    /// - "center" → horizontal centre of the available line
    /// - "right"  → right edge of the available line
    /// The renderer must subtract the actual text width (or half of it) to
    /// obtain the true draw origin so that real font metrics are used there.
    pub x: f32,
    pub y: f32,
    pub content: String,
    pub font: String,
    pub size: f32,
    pub color: String,
    pub text_align: String,
    /// Available line width. Set to 0.0 for non-layout text nodes.
    pub line_width: f32,
    /// What a justified line adds to each space of `content`, which layout worked out from the words
    /// and spans of the whole line. The renderer draws it as a `TJ` adjustment at each space. 0.0 for
    /// every other text.
    pub word_gap: f32,
}

pub struct RenderLink {
    pub url: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub debug_self: bool,
    pub children: Vec<RenderNode>,
}

// ── Canvas primitives ─────────────────────────────────────────────────────────

pub struct RenderCanvasRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width: f32,
    pub stroke_dash: Option<Vec<f32>>,
    pub border_radius: f32,
    pub shadow: Option<Shadow>,
}

pub struct RenderCanvasLine {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub stroke: String,
    pub stroke_width: f32,
    pub stroke_dash: Option<Vec<f32>>,
    /// 0 = butt, 1 = round, 2 = square
    pub line_cap: u8,
    /// 0 = miter, 1 = round, 2 = bevel
    pub line_join: u8,
}

pub struct RenderCanvasEllipse {
    pub cx: f32,
    pub cy: f32,
    pub rx: f32,
    pub ry: f32,
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width: f32,
    pub stroke_dash: Option<Vec<f32>>,
    pub shadow: Option<Shadow>,
}

/// A canvas path using SVG-like command string (M, L, C, Z).
pub struct RenderCanvasPath {
    pub d: String,
    pub fill: Option<String>,
    pub stroke: Option<String>,
    pub stroke_width: f32,
    pub stroke_dash: Option<Vec<f32>>,
    /// false = nonzero, true = evenodd
    pub fill_rule_evenodd: bool,
    pub line_cap: u8,
    pub line_join: u8,
}

pub struct RenderCanvasClip {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub border_radius: f32,
}

pub struct RenderCanvasLayer {
    /// Optional 6-element CTM [a b c d e f] (PDF coordinate space).
    pub transform: Option<[f32; 6]>,
    pub clip: Option<RenderCanvasClip>,
    /// 0.0–1.0 opacity (1.0 = fully opaque).
    pub opacity: f32,
    pub tag: Option<Box<Tag>>,
    pub children: Vec<RenderNode>,
}

// ── Serialisation ─────────────────────────────────────────────────────────────

pub fn pages_to_json(pages: &[RenderPage], meta: Value, watermark: Value) -> String {
    let pages_json: Vec<Value> = pages.iter().map(page_to_json).collect();
    json!({
        "version": 1,
        "meta": meta,
        "pages": pages_json,
        "watermark": watermark,
    })
    .to_string()
}

fn page_to_json(page: &RenderPage) -> Value {
    let [mt, mr, mb, ml] = page.margin;
    json!({
        "width":      r2(page.width),
        "height":     r2(page.height),
        "background": page.background,
        "margin":     [r2(mt), r2(mr), r2(mb), r2(ml)],
        "nodes":      nodes_to_json(&page.nodes),
    })
}

/// Serialise a slice of nodes, skipping invisible zero-height empty boxes and anchors.
fn nodes_to_json(nodes: &[RenderNode]) -> Vec<Value> {
    nodes.iter().filter(|n| !is_invisible(n)).map(node_to_json).collect()
}

fn is_invisible(node: &RenderNode) -> bool {
    match node {
        RenderNode::Box(b) => {
            b.height == 0.0 && b.width == 0.0
                && b.fill.is_none()
                && b.border_width == 0.0
                && b.children.is_empty()
                && !b.debug_self
        }
        RenderNode::Anchor(_) | RenderNode::FootnoteMark(_) => true,
        _ => false,
    }
}

fn shadow_to_json(s: &Shadow) -> Value {
    json!({ "x": r2(s.dx), "y": r2(s.dy), "color": s.color, "opacity": r2(s.opacity) })
}

fn node_to_json(node: &RenderNode) -> Value {
    match node {
        RenderNode::Box(b) => {
            let mut v = json!({
                "type":         "box",
                "x":            r2(b.x),
                "y":            r2(b.y),
                "width":        r2(b.width),
                "height":       r2(b.height),
                "fill":         b.fill,
                "border_width": r2(b.border_width),
                "border_color": b.border_color,
                "radius":       r2(b.radius),
                "nodes":        nodes_to_json(&b.children),
            });
            if b.opacity < 1.0 {
                v["opacity"] = json!(r2(b.opacity));
            }
            if let Some(s) = &b.shadow {
                v["shadow"] = shadow_to_json(s);
            }
            v
        }
        RenderNode::Line(l) => {
            let mut v = json!({
                "type":      "line",
                "x1":        r2(l.x1),
                "y1":        r2(l.y1),
                "x2":        r2(l.x2),
                "y2":        r2(l.y2),
                "color":     l.color,
                "thickness": r2(l.thickness),
            });
            if let Some(dash) = &l.dash {
                let dash_vals: Vec<f64> = dash.iter().map(|&d| r2(d as f32)).collect();
                v["dash"] = serde_json::json!(dash_vals);
            }
            v
        },
        RenderNode::Text(t) => json!({
            "type":       "text",
            "x":          r2(t.x),
            "y":          r2(t.y),
            "content":    t.content,
            "font":       t.font,
            "size":       r2(t.size),
            "color":      t.color,
            "text_align": t.text_align,
            "line_width": r2(t.line_width),
            "word_gap":   r2(t.word_gap),
        }),
        RenderNode::Link(l) => json!({
            "type":     "link",
            "url":      l.url,
            "x":        r2(l.x),
            "y":        r2(l.y),
            "width":    r2(l.width),
            "height":   r2(l.height),
            "nodes": nodes_to_json(&l.children),
        }),
        RenderNode::Image(i) => {
            let mut v = json!({
                "type":   "image",
                "x":      r2(i.x),
                "y":      r2(i.y),
                "width":  r2(i.width),
                "height": r2(i.height),
                "name":   i.name,
            });
            if i.radius > 0.0 { v["radius"] = json!(r2(i.radius)); }
            v
        }
        RenderNode::Barcode(bc) => {
            let kind_str = match &bc.kind {
                RenderedBarcodeKind::Qr { .. }      => "qr",
                RenderedBarcodeKind::Code128 { .. } => "code128",
                RenderedBarcodeKind::Ean13 { .. }   => "ean13",
            };
            json!({
                "type":   "barcode",
                "x":      r2(bc.x),
                "y":      r2(bc.y),
                "width":  r2(bc.width),
                "height": r2(bc.height),
                "kind":   kind_str,
                "color":  bc.color,
                "bg":     bc.bg,
            })
        }
        RenderNode::Field(f) => {
            let kind_str = match f.kind {
                FieldKind::Text     => "text",
                FieldKind::Checkbox => "checkbox",
                FieldKind::Dropdown => "dropdown",
                FieldKind::Radio    => "radio",
                FieldKind::Button   => "button",
            };
            json!({
                "type":       "field",
                "x":          r2(f.x),
                "y":          r2(f.y),
                "width":      r2(f.width),
                "height":     r2(f.height),
                "fieldType":  kind_str,
                "name":       f.name,
                "value":      f.value,
                "label":      f.label,
                "group":      f.group,
                "checked":    f.checked,
                "options":    f.options,
            })
        }
        RenderNode::CanvasRect(r) => {
            let mut v = json!({
                "type":   "canvas-rect",
                "x":      r2(r.x),
                "y":      r2(r.y),
                "w":      r2(r.w),
                "h":      r2(r.h),
                "fill":   r.fill,
                "stroke": r.stroke,
            });
            if let Some(s) = &r.shadow {
                v["shadow"] = shadow_to_json(s);
            }
            v
        }
        RenderNode::CanvasLine(l) => json!({
            "type":   "canvas-line",
            "x1":     r2(l.x1),
            "y1":     r2(l.y1),
            "x2":     r2(l.x2),
            "y2":     r2(l.y2),
            "stroke": l.stroke,
        }),
        RenderNode::CanvasEllipse(e) => {
            let mut v = json!({
                "type":   "canvas-ellipse",
                "cx":     r2(e.cx),
                "cy":     r2(e.cy),
                "rx":     r2(e.rx),
                "ry":     r2(e.ry),
                "fill":   e.fill,
                "stroke": e.stroke,
            });
            if let Some(s) = &e.shadow {
                v["shadow"] = shadow_to_json(s);
            }
            v
        }
        RenderNode::CanvasPath(p) => json!({
            "type":   "canvas-path",
            "d":      p.d,
            "fill":   p.fill,
            "stroke": p.stroke,
        }),
        RenderNode::CanvasLayer(l) => json!({
            "type":     "canvas-layer",
            "opacity":  l.opacity,
            "clip":     l.clip.as_ref().map(|c| [r2(c.x), r2(c.y), r2(c.w), r2(c.h)]),
            "nodes": nodes_to_json(&l.children),
        }),
        // Filtered out by `is_invisible`: the JSON pages are what is drawn.
        RenderNode::Anchor(_) | RenderNode::FootnoteMark(_) => Value::Null,
    }
}

/// Round to 2 decimal places for compact JSON output.
fn r2(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}
