use serde_json::{json, Value};
use crate::parse::FieldKind;

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
    // Canvas primitives — constructed by canvas.rs; allow dead_code for WASI build.
    #[allow(dead_code)] CanvasText(RenderCanvasText),
    #[allow(dead_code)] CanvasRect(RenderCanvasRect),
    #[allow(dead_code)] CanvasLine(RenderCanvasLine),
    #[allow(dead_code)] CanvasEllipse(RenderCanvasEllipse),
    #[allow(dead_code)] CanvasPath(RenderCanvasPath),
    #[allow(dead_code)] CanvasImage(RenderCanvasImage),
    #[allow(dead_code)] CanvasLayer(RenderCanvasLayer),
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
    pub debug_self: bool,
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

/// A text block placed at an absolute canvas position.
/// Coordinates are top-left origin, y-down (flipped at PDF render time).
pub struct RenderCanvasText {
    pub x: f32,
    pub y: f32,
    pub font: String,
    pub size: f32,
    pub color: String,
    /// "left" | "center" | "right" | "justify"
    pub align: String,
    pub line_height: f32,
    /// Explicit wrap width (None = content width − x).
    pub width: Option<f32>,
    /// Plain text content; lines split on `\n`.
    pub content: String,
    /// Mixed-style runs (if non-empty, overrides `content`).
    pub runs: Vec<RenderCanvasRun>,
}

pub struct RenderCanvasRun {
    pub text: String,
    pub font: Option<String>,
    #[allow(dead_code)]
    pub size: Option<f32>,
    #[allow(dead_code)]
    pub color: Option<String>,
}

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

pub struct RenderCanvasImage {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub src: String,
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

/// Serialise a slice of nodes, skipping invisible zero-height empty boxes.
fn nodes_to_json(nodes: &[RenderNode]) -> Vec<Value> {
    nodes.iter().filter(|n| !is_invisible(n)).map(node_to_json).collect()
}

fn is_invisible(node: &RenderNode) -> bool {
    if let RenderNode::Box(b) = node {
        b.height == 0.0 && b.width == 0.0
            && b.fill.is_none()
            && b.border_width == 0.0
            && b.children.is_empty()
            && !b.debug_self
    } else {
        false
    }
}

fn node_to_json(node: &RenderNode) -> Value {
    match node {
        RenderNode::Box(b) => json!({
            "type":         "box",
            "x":            r2(b.x),
            "y":            r2(b.y),
            "width":        r2(b.width),
            "height":       r2(b.height),
            "fill":         b.fill,
            "border_width": r2(b.border_width),
            "border_color": b.border_color,
            "radius":       r2(b.radius),
            "children":     nodes_to_json(&b.children),
        }),
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
        }),
        RenderNode::Link(l) => json!({
            "type":     "link",
            "url":      l.url,
            "x":        r2(l.x),
            "y":        r2(l.y),
            "width":    r2(l.width),
            "height":   r2(l.height),
            "children": nodes_to_json(&l.children),
        }),
        RenderNode::Image(i) => json!({
            "type":   "image",
            "x":      r2(i.x),
            "y":      r2(i.y),
            "width":  r2(i.width),
            "height": r2(i.height),
            "name":   i.name,
        }),
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
        RenderNode::CanvasText(t) => json!({
            "type":  "canvas-text",
            "x":     r2(t.x),
            "y":     r2(t.y),
            "font":  t.font,
            "size":  r2(t.size),
            "color": t.color,
            "align": t.align,
        }),
        RenderNode::CanvasRect(r) => json!({
            "type":   "canvas-rect",
            "x":      r2(r.x),
            "y":      r2(r.y),
            "w":      r2(r.w),
            "h":      r2(r.h),
            "fill":   r.fill,
            "stroke": r.stroke,
        }),
        RenderNode::CanvasLine(l) => json!({
            "type":   "canvas-line",
            "x1":     r2(l.x1),
            "y1":     r2(l.y1),
            "x2":     r2(l.x2),
            "y2":     r2(l.y2),
            "stroke": l.stroke,
        }),
        RenderNode::CanvasEllipse(e) => json!({
            "type":   "canvas-ellipse",
            "cx":     r2(e.cx),
            "cy":     r2(e.cy),
            "rx":     r2(e.rx),
            "ry":     r2(e.ry),
            "fill":   e.fill,
            "stroke": e.stroke,
        }),
        RenderNode::CanvasPath(p) => json!({
            "type":   "canvas-path",
            "d":      p.d,
            "fill":   p.fill,
            "stroke": p.stroke,
        }),
        RenderNode::CanvasImage(i) => json!({
            "type": "canvas-image",
            "x":    r2(i.x),
            "y":    r2(i.y),
            "w":    r2(i.w),
            "h":    r2(i.h),
            "src":  i.src,
        }),
        RenderNode::CanvasLayer(l) => json!({
            "type":     "canvas-layer",
            "opacity":  l.opacity,
            "children": nodes_to_json(&l.children),
        }),
    }
}

/// Round to 2 decimal places for compact JSON output.
fn r2(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}
