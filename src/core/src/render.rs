use serde_json::{json, Value};

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
}

pub struct RenderImage {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub name: String,
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

/// Returns true for a box with no dimensions, no fill, no border, and no children.
/// These are produced by empty <text> elements and add nothing to the output.
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
    }
}

/// Round to 2 decimal places for compact JSON output.
fn r2(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}
