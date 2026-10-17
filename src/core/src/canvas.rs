/// Layers — coordinate-based overlay/underlay rendering.
///
/// This module owns the layer type model, XML and JSON parsers, and the
/// function that stamps layers onto already-laid-out `RenderPage`s.
///
/// Layers use top-left origin, y-down coordinates.  The PDF render layer
/// (`pdf.rs`) flips to bottom-up when emitting content streams.

use std::collections::HashMap;
use crate::tokens::Tokens;
use crate::parse::{
    PageScope,
    parse_measurement, parse_signed_measurement, parse_page_scope,
    elems, validate_img_asset, jattr, node_loc,
};
use crate::page_scope::page_scope_indices;
use crate::render::{
    RenderCanvasEllipse, RenderCanvasImage, RenderCanvasLayer,
    RenderCanvasLine, RenderCanvasPath, RenderCanvasRect, RenderCanvasRun, RenderCanvasText,
    RenderNode, RenderPage,
};

// ── Canvas types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Anchor {
    TopLeft, TopCenter, TopRight,
    CenterLeft, Center, CenterRight,
    BottomLeft, BottomCenter, BottomRight,
}

#[derive(Debug, Clone)]
pub struct CanvasRect {
    pub x:            f32,
    pub y:            f32,
    pub w:            f32,
    pub h:            f32,
    pub fill:         Option<String>,
    pub stroke:       Option<String>,
    pub stroke_width: Option<f32>,
    pub stroke_dash:  Option<String>,
    pub radius:       Option<f32>,
}

/// Circle: `cx`/`cy` is the centre point, worked out from the box `x` and `y` place.
#[derive(Debug, Clone)]
pub struct CanvasCircle {
    pub cx:           f32,
    pub cy:           f32,
    pub r:            f32,
    pub fill:         Option<String>,
    pub stroke:       Option<String>,
    pub stroke_width: Option<f32>,
    pub stroke_dash:  Option<String>,
}

/// Ellipse: `cx`/`cy` is the centre point, worked out from the box `x` and `y` place.
#[derive(Debug, Clone)]
pub struct CanvasEllipse {
    pub cx:           f32,
    pub cy:           f32,
    pub rx:           f32,
    pub ry:           f32,
    pub fill:         Option<String>,
    pub stroke:       Option<String>,
    pub stroke_width: Option<f32>,
    pub stroke_dash:  Option<String>,
}

/// Line: no anchor — absolute coordinates always required.
#[derive(Debug, Clone)]
pub struct CanvasLine {
    pub x1:           f32,
    pub y1:           f32,
    pub x2:           f32,
    pub y2:           f32,
    pub stroke:       Option<String>,
    pub stroke_width: Option<f32>,
    pub stroke_dash:  Option<String>,
    pub line_cap:     Option<String>,
}

/// Path: no anchor — coordinates are embedded in the SVG `d` string.
#[derive(Debug, Clone)]
pub struct CanvasPath {
    pub d:            String,
    pub fill:         Option<String>,
    pub stroke:       Option<String>,
    pub fill_rule:    Option<String>,
    pub stroke_width: Option<f32>,
    pub stroke_dash:  Option<String>,
    pub line_cap:     Option<String>,
}

#[derive(Debug, Clone)]
pub struct CanvasTextRun {
    pub text:  String,
    pub font:  Option<String>,
    /// Per-run color is parsed for schema compliance; per-run color rendering
    /// is not yet implemented in the PDF engine (uses block-level color).
    #[allow(dead_code)]
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CanvasText {
    pub x:           f32,
    pub y:           f32,
    pub font:        Option<String>,
    pub font_size:   Option<f32>,
    pub color:       Option<String>,
    pub align:       Option<String>,
    pub w:           Option<f32>,
    pub line_height: Option<f32>,
    /// Plain text content (empty when `runs` is non-empty).
    pub content:     String,
    /// Mixed-run children from `<span>` elements.
    pub runs:        Vec<CanvasTextRun>,
    /// How `x` should be interpreted at render time:
    /// 0 = left edge (default), 1 = horizontal centre, 2 = right edge.
    pub anchor_col:  u8,
    /// Per-element fill opacity (1.0 = fully opaque).
    pub opacity:     f32,
}

#[derive(Debug, Clone)]
pub struct CanvasImg {
    pub name: String,
    pub x:    f32,
    pub y:    f32,
    pub w:    f32,
    pub h:    f32,
}

#[derive(Debug, Clone)]
pub enum CanvasPrimitive {
    Rect(CanvasRect),
    Circle(CanvasCircle),
    Ellipse(CanvasEllipse),
    Line(CanvasLine),
    Path(CanvasPath),
    Text(CanvasText),
    Img(CanvasImg),
}

/// A `<layer>`: a graphics-state scope, a direct child of `<section>`.  Document
/// order of layers is paint order (first = bottom).  Anchors and transforms are
/// resolved at parse time.
#[derive(Debug, Clone)]
pub struct CanvasLayer {
    pub children:  Vec<CanvasPrimitive>,
    pub page:      Option<PageScope>,
    pub opacity:   Option<f32>,
    /// Optional 6-element CTM [a b c d e f] in canvas (top-down) space.
    pub transform: Option<[f32; 6]>,
}

// ── Anchor helpers ────────────────────────────────────────────────────────────

pub fn parse_anchor(s: &str) -> Result<Anchor, String> {
    match s {
        "top-left"      => Ok(Anchor::TopLeft),
        "top-center"    => Ok(Anchor::TopCenter),
        "top-right"     => Ok(Anchor::TopRight),
        "center-left"   => Ok(Anchor::CenterLeft),
        "center"        => Ok(Anchor::Center),
        "center-right"  => Ok(Anchor::CenterRight),
        "bottom-left"   => Ok(Anchor::BottomLeft),
        "bottom-center" => Ok(Anchor::BottomCenter),
        "bottom-right"  => Ok(Anchor::BottomRight),
        other => Err(format!("invalid anchor: '{other}'")),
    }
}

fn resolve_anchor(anchor: &Anchor, page_w: f32, page_h: f32) -> (f32, f32) {
    match anchor {
        Anchor::TopLeft      => (0.0,          0.0),
        Anchor::TopCenter    => (page_w / 2.0, 0.0),
        Anchor::TopRight     => (page_w,       0.0),
        Anchor::CenterLeft   => (0.0,          page_h / 2.0),
        Anchor::Center       => (page_w / 2.0, page_h / 2.0),
        Anchor::CenterRight  => (page_w,       page_h / 2.0),
        Anchor::BottomLeft   => (0.0,          page_h),
        Anchor::BottomCenter => (page_w / 2.0, page_h),
        Anchor::BottomRight  => (page_w,       page_h),
    }
}

// ── XML parse helpers ─────────────────────────────────────────────────────────

/// Adjust `(x, y)` so that the named anchor point of a sized element (w × h)
/// lands on the resolved coordinate.  For example, `anchor="center"` means the
/// centre of the element should be at `(x, y)`, so we shift by `(-w/2, -h/2)`.
fn apply_anchor_offset(anchor: &Anchor, x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
    let ox = match anchor {
        Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => w / 2.0,
        Anchor::TopRight  | Anchor::CenterRight | Anchor::BottomRight => w,
        _ => 0.0,
    };
    let oy = match anchor {
        Anchor::CenterLeft | Anchor::Center | Anchor::CenterRight => h / 2.0,
        Anchor::BottomLeft | Anchor::BottomCenter | Anchor::BottomRight => h,
        _ => 0.0,
    };
    (x - ox, y - oy)
}

/// Parse canvas position from element attributes, resolving anchors immediately.
/// Returns `(x, y)` in canvas top-down coordinates.  With an anchor, `x` and `y` are
/// offsets from that point of the page.
fn parse_canvas_position(elem: &roxmltree::Node, page_w: f32, page_h: f32) -> Result<(f32, f32), String> {
    let dx = elem.attribute("x").map(parse_signed_measurement).transpose()?.unwrap_or(0.0);
    let dy = elem.attribute("y").map(parse_signed_measurement).transpose()?.unwrap_or(0.0);
    match elem.attribute("anchor") {
        Some(a) => {
            let (bx, by) = resolve_anchor(&parse_anchor(a)?, page_w, page_h);
            Ok((bx + dx, by + dy))
        }
        None => Ok((dx, dy)),
    }
}

/// The top-left corner of a box of the given size, placed by `x` and `y`: with an anchor, the box's
/// own anchor point lands on the position.
fn parse_canvas_box_position(
    elem:   &roxmltree::Node,
    page_w: f32,
    page_h: f32,
    w:      f32,
    h:      f32,
) -> Result<(f32, f32), String> {
    let (x, y) = parse_canvas_position(elem, page_w, page_h)?;
    match elem.attribute("anchor") {
        Some(a) => Ok(apply_anchor_offset(&parse_anchor(a)?, x, y, w, h)),
        None => Ok((x, y)),
    }
}

/// What to write instead of an attribute a layer element no longer has.
fn removed_attr_hint(tag: &str, attr: &str) -> Option<&'static str> {
    match (tag, attr) {
        ("rect" | "text" | "img", "w") => Some("renamed to 'width'"),
        ("rect" | "img", "h") => Some("renamed to 'height'"),
        ("circle", "cx" | "cy") => Some(
            "removed; 'x' and 'y' place the circle's box, 2r square, so write x = cx - r and y = cy - r",
        ),
        ("ellipse", "cx" | "cy") => Some(
            "removed; 'x' and 'y' place the ellipse's box, 2rx wide and 2ry high, so write x = cx - rx and y = cy - ry",
        ),
        _ => None,
    }
}

/// Fail on an attribute the element used to take, with what to write instead.
fn reject_removed_attrs(tag: &str, has: impl Fn(&str) -> bool) -> Result<(), String> {
    for attr in ["w", "h", "cx", "cy"] {
        if let (true, Some(hint)) = (has(attr), removed_attr_hint(tag, attr)) {
            return Err(format!("'{attr}' {hint}"));
        }
    }
    Ok(())
}

/// Parse a transform string such as `"translate(10pt 20pt)"`, `"scale(2)"`,
/// `"rotate(45 297.64 421)"`, or `"matrix(a b c d e f)"` into a 6-element CTM.
/// Numbers may be separated by commas or whitespace.  Returns `None` for
/// the identity (no-op) transform or unrecognised strings.
fn parse_transform(s: &str) -> Option<[f32; 6]> {
    fn nums(inner: &str) -> Vec<f32> {
        inner
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f32>().ok())
            .collect()
    }
    let s = s.trim();
    if s.starts_with("matrix(") {
        let inner = &s["matrix(".len()..s.len() - 1];
        let p = nums(inner);
        if p.len() == 6 { return Some([p[0], p[1], p[2], p[3], p[4], p[5]]); }
    } else if s.starts_with("translate(") {
        let inner = &s["translate(".len()..s.len() - 1];
        let p = nums(inner);
        let tx = p.first().copied().unwrap_or(0.0);
        let ty = p.get(1).copied().unwrap_or(0.0);
        if tx != 0.0 || ty != 0.0 { return Some([1.0, 0.0, 0.0, 1.0, tx, ty]); }
    } else if s.starts_with("scale(") {
        let inner = &s["scale(".len()..s.len() - 1];
        let p = nums(inner);
        let sx = p.first().copied().unwrap_or(1.0);
        let sy = p.get(1).copied().unwrap_or(sx);
        if (sx - 1.0).abs() > 1e-6 || (sy - 1.0).abs() > 1e-6 {
            return Some([sx, 0.0, 0.0, sy, 0.0, 0.0]);
        }
    } else if s.starts_with("rotate(") {
        let inner = &s["rotate(".len()..s.len() - 1];
        let p = nums(inner);
        let deg = p.first().copied().unwrap_or(0.0);
        if deg.abs() > 1e-6 {
            let cx  = p.get(1).copied().unwrap_or(0.0);
            let cy  = p.get(2).copied().unwrap_or(0.0);
            let rad = deg.to_radians();
            let cos = rad.cos();
            let sin = rad.sin();
            let e = cx - cx * cos + cy * sin;
            let f = cy - cx * sin - cy * cos;
            return Some([cos, sin, -sin, cos, e, f]);
        }
    }
    None
}

fn opt_canvas_color(
    elem:   &roxmltree::Node,
    attr:   &str,
    tokens: &Tokens,
) -> Result<Option<String>, String> {
    elem.attribute(attr).map(|v| tokens.resolve_color(v)).transpose()
}

fn parse_canvas_rect(elem: &roxmltree::Node, tokens: &Tokens, page_w: f32, page_h: f32) -> Result<CanvasPrimitive, String> {
    let w            = parse_measurement(elem.attribute("width").unwrap_or("0pt"))?;
    let h            = parse_measurement(elem.attribute("height").unwrap_or("0pt"))?;
    let (x, y)       = parse_canvas_box_position(elem, page_w, page_h, w, h)?;
    let fill         = opt_canvas_color(elem, "fill", tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    let radius       = elem.attribute("radius").map(parse_measurement).transpose()?;
    Ok(CanvasPrimitive::Rect(CanvasRect { x, y, w, h, fill, stroke, stroke_width, stroke_dash, radius }))
}

fn parse_canvas_circle(elem: &roxmltree::Node, tokens: &Tokens, page_w: f32, page_h: f32) -> Result<CanvasPrimitive, String> {
    let r            = parse_measurement(elem.attribute("r").unwrap_or("0pt"))?;
    let (x, y)       = parse_canvas_box_position(elem, page_w, page_h, 2.0 * r, 2.0 * r)?;
    let (cx, cy)     = (x + r, y + r);
    let fill         = opt_canvas_color(elem, "fill", tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    Ok(CanvasPrimitive::Circle(CanvasCircle { cx, cy, r, fill, stroke, stroke_width, stroke_dash }))
}

fn parse_canvas_ellipse(elem: &roxmltree::Node, tokens: &Tokens, page_w: f32, page_h: f32) -> Result<CanvasPrimitive, String> {
    let rx           = parse_measurement(elem.attribute("rx").unwrap_or("0pt"))?;
    let ry           = parse_measurement(elem.attribute("ry").unwrap_or("0pt"))?;
    let (x, y)       = parse_canvas_box_position(elem, page_w, page_h, 2.0 * rx, 2.0 * ry)?;
    let (cx, cy)     = (x + rx, y + ry);
    let fill         = opt_canvas_color(elem, "fill", tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    Ok(CanvasPrimitive::Ellipse(CanvasEllipse { cx, cy, rx, ry, fill, stroke, stroke_width, stroke_dash }))
}

fn parse_canvas_line(elem: &roxmltree::Node, tokens: &Tokens) -> Result<CanvasPrimitive, String> {
    let x1           = parse_signed_measurement(elem.attribute("x1").unwrap_or("0pt"))?;
    let y1           = parse_signed_measurement(elem.attribute("y1").unwrap_or("0pt"))?;
    let x2           = parse_signed_measurement(elem.attribute("x2").unwrap_or("0pt"))?;
    let y2           = parse_signed_measurement(elem.attribute("y2").unwrap_or("0pt"))?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    let line_cap     = elem.attribute("line-cap").map(str::to_string);
    Ok(CanvasPrimitive::Line(CanvasLine { x1, y1, x2, y2, stroke, stroke_width, stroke_dash, line_cap }))
}

fn parse_canvas_path(elem: &roxmltree::Node, tokens: &Tokens) -> Result<CanvasPrimitive, String> {
    let d            = elem.attribute("d").unwrap_or("").to_string();
    let fill         = opt_canvas_color(elem, "fill", tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let fill_rule    = elem.attribute("fill-rule").map(str::to_string);
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    let line_cap     = elem.attribute("line-cap").map(str::to_string);
    Ok(CanvasPrimitive::Path(CanvasPath { d, fill, stroke, fill_rule, stroke_width, stroke_dash, line_cap }))
}

fn parse_canvas_text_elem(elem: &roxmltree::Node, tokens: &Tokens, page_w: f32, page_h: f32) -> Result<CanvasPrimitive, String> {
    let anchor_col = if let Some(a) = elem.attribute("anchor") {
        match parse_anchor(a)? {
            Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => 1,
            Anchor::TopRight  | Anchor::CenterRight | Anchor::BottomRight => 2,
            _ => 0,
        }
    } else { 0 };
    let (x, y)      = parse_canvas_position(elem, page_w, page_h)?;
    let font        = elem.attribute("font").map(str::to_string);
    let font_size   = elem.attribute("font-size").map(parse_measurement).transpose()?;
    let color       = opt_canvas_color(elem, "color", tokens)?;
    let align       = elem.attribute("align").map(str::to_string);
    let w           = elem.attribute("width").map(parse_measurement).transpose()?;
    let line_height = elem.attribute("line-height").map(parse_measurement).transpose()?;
    let opacity     = elem.attribute("opacity")
        .map(|v| v.parse::<f32>().map_err(|_| format!("invalid text opacity '{v}'")))
        .transpose()?.unwrap_or(1.0);

    let mut content = String::new();
    let mut runs    = Vec::new();

    for child in elem.children() {
        if child.is_text() {
            let txt = child.text().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
            if !txt.is_empty() { content.push_str(&txt); }
        } else if child.is_element() && child.tag_name().name() == "span" {
            let span_text = child.children()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect::<String>()
                .split_whitespace().collect::<Vec<_>>().join(" ");
            runs.push(CanvasTextRun {
                text:  span_text,
                font:  child.attribute("font").map(str::to_string),
                color: if let Some(c) = child.attribute("color") {
                    Some(tokens.resolve_color(c)?)
                } else { None },
            });
        }
    }

    Ok(CanvasPrimitive::Text(CanvasText {
        x, y, font, font_size, color, align, w, line_height,
        content, runs, anchor_col, opacity,
    }))
}

fn parse_canvas_img_elem(
    elem:         &roxmltree::Node,
    asset_images: &HashMap<String, String>,
    page_w:       f32,
    page_h:       f32,
) -> Result<CanvasPrimitive, String> {
    let name_raw = elem.attribute("name")
        .ok_or("<img> (canvas) missing required attribute 'name'")?;
    validate_img_asset(name_raw, asset_images, "<assets>")?;
    let name      = asset_images[name_raw].clone();
    let w         = parse_measurement(elem.attribute("width").unwrap_or("0pt"))?;
    let h         = parse_measurement(elem.attribute("height").unwrap_or("0pt"))?;
    let (x, y)    = parse_canvas_box_position(elem, page_w, page_h, w, h)?;
    Ok(CanvasPrimitive::Img(CanvasImg { name, x, y, w, h }))
}

fn parse_canvas_primitive_elem(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    page_w:       f32,
    page_h:       f32,
) -> Result<CanvasPrimitive, String> {
    let tag = elem.tag_name().name();
    reject_removed_attrs(tag, |a| elem.attribute(a).is_some())
        .map_err(|e| format!("<{tag}>{}: {e}", node_loc(elem)))?;
    match tag {
        "rect"    => parse_canvas_rect(elem, tokens, page_w, page_h),
        "circle"  => parse_canvas_circle(elem, tokens, page_w, page_h),
        "ellipse" => parse_canvas_ellipse(elem, tokens, page_w, page_h),
        "line"    => parse_canvas_line(elem, tokens),
        "path"    => parse_canvas_path(elem, tokens),
        "text"    => parse_canvas_text_elem(elem, tokens, page_w, page_h),
        "img"     => parse_canvas_img_elem(elem, asset_images, page_w, page_h),
        other => Err(format!("unknown canvas primitive: <{other}>")),
    }
}

/// Parse a `<layer>` element.  `page_w` and `page_h` are the dimensions of the
/// enclosing section page — needed to resolve anchor offsets.
pub(crate) fn parse_layer_elem(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    page_w:       f32,
    page_h:       f32,
) -> Result<CanvasLayer, String> {
    let page = elem.attribute("page")
        .map(parse_page_scope)
        .transpose()?;
    let opacity = elem.attribute("opacity")
        .map(|v| v.parse::<f32>().map_err(|_| format!("invalid layer opacity '{v}'")))
        .transpose()?;
    let transform = elem.attribute("transform").and_then(parse_transform);

    let mut children = Vec::new();
    for child in elems(elem) {
        children.push(parse_canvas_primitive_elem(&child, tokens, asset_images, page_w, page_h)?);
    }
    Ok(CanvasLayer { children, page, opacity, transform })
}

// ── JSON parse path ───────────────────────────────────────────────────────────

/// Parse a layer from a JSON tree node.  `page_w`/`page_h` are the
/// section page dimensions for anchor resolution.
pub(crate) fn parse_tree_layer(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    page_w:       f32,
    page_h:       f32,
) -> Result<CanvasLayer, String> {
    let page = jattr(json, "page").map(parse_page_scope).transpose()?;
    let opacity = jattr(json, "opacity")
        .map(|v| v.parse::<f32>().map_err(|_| format!("invalid layer opacity '{v}'")))
        .transpose()?;
    let transform = jattr(json, "transform").and_then(parse_transform);

    let mut children = Vec::new();
    if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
        for n in arr {
            children.push(parse_tree_canvas_primitive(n, tokens, asset_images, page_w, page_h)?);
        }
    }
    Ok(CanvasLayer { children, page, opacity, transform })
}

fn parse_tree_canvas_primitive(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    page_w:       f32,
    page_h:       f32,
) -> Result<CanvasPrimitive, String> {
    let type_str = json.get("type").and_then(|v| v.as_str())
        .ok_or("canvas primitive missing 'type'")?;

    let get_attr = |k: &str| jattr(json, k);
    let get_color = |k: &str| -> Result<Option<String>, String> {
        get_attr(k).map(|v| tokens.resolve_color(v)).transpose()
    };
    let get_f32 = |k: &str| -> Result<Option<f32>, String> {
        get_attr(k).map(parse_measurement).transpose()
    };
    let get_f32_signed = |k: &str| -> Result<Option<f32>, String> {
        get_attr(k).map(parse_signed_measurement).transpose()
    };
    reject_removed_attrs(type_str, |a| get_attr(a).is_some())
        .map_err(|e| format!("canvas primitive '{type_str}': {e}"))?;
    // The position of a primitive: `x` and `y`. With an anchor they are offsets from it.
    let get_pos = || -> Result<(f32, f32), String> {
        let dx = get_f32_signed("x")?.unwrap_or(0.0);
        let dy = get_f32_signed("y")?.unwrap_or(0.0);
        match get_attr("anchor") {
            Some(a) => {
                let (bx, by) = resolve_anchor(&parse_anchor(a)?, page_w, page_h);
                Ok((bx + dx, by + dy))
            }
            None => Ok((dx, dy)),
        }
    };
    // The top-left corner of a box of the given size: with an anchor, the box is placed so that its own
    // anchor point lands on the position, as in XML.
    let get_box_pos = |w: f32, h: f32| -> Result<(f32, f32), String> {
        let (x, y) = get_pos()?;
        match get_attr("anchor") {
            Some(a) => Ok(apply_anchor_offset(&parse_anchor(a)?, x, y, w, h)),
            None => Ok((x, y)),
        }
    };

    match type_str {
        "rect" => {
            let w = get_f32("width")?.unwrap_or(0.0);
            let h = get_f32("height")?.unwrap_or(0.0);
            let (x, y) = get_box_pos(w, h)?;
            Ok(CanvasPrimitive::Rect(CanvasRect {
                x, y, w, h,
                fill:         get_color("fill")?,
                stroke:       get_color("stroke")?,
                stroke_width: get_f32("stroke-width")?,
                stroke_dash:  get_attr("stroke-dash").map(str::to_string),
                radius:       get_f32("radius")?,
            }))
        }
        "circle" => {
            let r = get_f32("r")?.unwrap_or(0.0);
            let (x, y) = get_box_pos(2.0 * r, 2.0 * r)?;
            Ok(CanvasPrimitive::Circle(CanvasCircle {
                cx: x + r, cy: y + r, r,
                fill:         get_color("fill")?,
                stroke:       get_color("stroke")?,
                stroke_width: get_f32("stroke-width")?,
                stroke_dash:  get_attr("stroke-dash").map(str::to_string),
            }))
        }
        "ellipse" => {
            let rx = get_f32("rx")?.unwrap_or(0.0);
            let ry = get_f32("ry")?.unwrap_or(0.0);
            let (x, y) = get_box_pos(2.0 * rx, 2.0 * ry)?;
            Ok(CanvasPrimitive::Ellipse(CanvasEllipse {
                cx: x + rx, cy: y + ry, rx, ry,
                fill:         get_color("fill")?,
                stroke:       get_color("stroke")?,
                stroke_width: get_f32("stroke-width")?,
                stroke_dash:  get_attr("stroke-dash").map(str::to_string),
            }))
        }
        "line" => Ok(CanvasPrimitive::Line(CanvasLine {
            x1:           get_f32_signed("x1")?.unwrap_or(0.0),
            y1:           get_f32_signed("y1")?.unwrap_or(0.0),
            x2:           get_f32_signed("x2")?.unwrap_or(0.0),
            y2:           get_f32_signed("y2")?.unwrap_or(0.0),
            stroke:       get_color("stroke")?,
            stroke_width: get_f32("stroke-width")?,
            stroke_dash:  get_attr("stroke-dash").map(str::to_string),
            line_cap:     get_attr("line-cap").map(str::to_string),
        })),
        "path" => Ok(CanvasPrimitive::Path(CanvasPath {
            d:            get_attr("d").unwrap_or("").to_string(),
            fill:         get_color("fill")?,
            stroke:       get_color("stroke")?,
            fill_rule:    get_attr("fill-rule").map(str::to_string),
            stroke_width: get_f32("stroke-width")?,
            stroke_dash:  get_attr("stroke-dash").map(str::to_string),
            line_cap:     get_attr("line-cap").map(str::to_string),
        })),
        "text" => {
            let anchor_col = if let Some(a) = get_attr("anchor") {
                match parse_anchor(a)? {
                    Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => 1,
                    Anchor::TopRight  | Anchor::CenterRight | Anchor::BottomRight => 2,
                    _ => 0,
                }
            } else { 0 };
            let (x, y) = get_pos()?;
            let opacity = get_attr("opacity")
                .map(|v| v.parse::<f32>().map_err(|_| format!("invalid text opacity '{v}'")))
                .transpose()?.unwrap_or(1.0);

            // Content as in the XML: plain strings are the text, and each span child is a styled run.
            let mut content = String::new();
            let mut runs    = Vec::new();
            if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
                for child in arr {
                    if let Some(s) = child.as_str() {
                        content.push_str(&s.split_whitespace().collect::<Vec<_>>().join(" "));
                    } else if child.get("type").and_then(|v| v.as_str()) == Some("span") {
                        let text = child.get("nodes").and_then(|v| v.as_array())
                            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<String>())
                            .unwrap_or_default()
                            .split_whitespace().collect::<Vec<_>>().join(" ");
                        let color = jattr(child, "color").map(|c| tokens.resolve_color(c)).transpose()?;
                        runs.push(CanvasTextRun { text, font: jattr(child, "font").map(str::to_string), color });
                    }
                }
            }
            Ok(CanvasPrimitive::Text(CanvasText {
                x, y,
                font:        get_attr("font").map(str::to_string),
                font_size:   get_f32("font-size")?,
                color:       get_color("color")?,
                align:       get_attr("align").map(str::to_string),
                w:           get_f32("width")?,
                line_height: get_f32("line-height")?,
                content, runs, anchor_col, opacity,
            }))
        }
        "img" => {
            let w = get_f32("width")?.unwrap_or(0.0);
            let h = get_f32("height")?.unwrap_or(0.0);
            let (x, y) = get_box_pos(w, h)?;
            let name_raw = get_attr("name")
                .ok_or("<img> (canvas) missing required attribute 'name'")?;
            let name = asset_images.get(name_raw)
                .ok_or_else(|| format!("<img> (canvas) unknown asset '{name_raw}'"))?
                .clone();
            Ok(CanvasPrimitive::Img(CanvasImg { name, x, y, w, h }))
        }
        other => Err(format!("unknown canvas primitive type: '{other}'")),
    }
}

// ── Render conversion ─────────────────────────────────────────────────────────

fn stroke_dash_to_vec(s: &str) -> Option<Vec<f32>> {
    let v: Vec<f32> = s.split_whitespace().filter_map(|t| t.parse().ok()).collect();
    if v.is_empty() { None } else { Some(v) }
}

fn line_cap_to_u8(s: &str) -> u8 {
    match s { "round" => 1, "square" => 2, _ => 0 }
}

fn primitive_to_render_node(prim: &CanvasPrimitive, page_w: f32) -> RenderNode {
    match prim {
        CanvasPrimitive::Rect(r) => RenderNode::CanvasRect(RenderCanvasRect {
            x:             r.x,
            y:             r.y,
            w:             r.w,
            h:             r.h,
            fill:          r.fill.clone(),
            stroke:        r.stroke.clone(),
            stroke_width:  r.stroke_width.unwrap_or(1.0),
            stroke_dash:   r.stroke_dash.as_deref().and_then(stroke_dash_to_vec),
            border_radius: r.radius.unwrap_or(0.0),
        }),
        CanvasPrimitive::Circle(c) => RenderNode::CanvasEllipse(RenderCanvasEllipse {
            cx:           c.cx,
            cy:           c.cy,
            rx:           c.r,
            ry:           c.r,
            fill:         c.fill.clone(),
            stroke:       c.stroke.clone(),
            stroke_width: c.stroke_width.unwrap_or(1.0),
            stroke_dash:  c.stroke_dash.as_deref().and_then(stroke_dash_to_vec),
        }),
        CanvasPrimitive::Ellipse(e) => RenderNode::CanvasEllipse(RenderCanvasEllipse {
            cx:           e.cx,
            cy:           e.cy,
            rx:           e.rx,
            ry:           e.ry,
            fill:         e.fill.clone(),
            stroke:       e.stroke.clone(),
            stroke_width: e.stroke_width.unwrap_or(1.0),
            stroke_dash:  e.stroke_dash.as_deref().and_then(stroke_dash_to_vec),
        }),
        CanvasPrimitive::Line(l) => RenderNode::CanvasLine(RenderCanvasLine {
            x1:           l.x1,
            y1:           l.y1,
            x2:           l.x2,
            y2:           l.y2,
            stroke:       l.stroke.as_deref().unwrap_or("#000000").to_string(),
            stroke_width: l.stroke_width.unwrap_or(1.0),
            stroke_dash:  l.stroke_dash.as_deref().and_then(stroke_dash_to_vec),
            line_cap:     l.line_cap.as_deref().map(line_cap_to_u8).unwrap_or(0),
            line_join:    0,
        }),
        CanvasPrimitive::Path(p) => RenderNode::CanvasPath(RenderCanvasPath {
            d:                  p.d.clone(),
            fill:               p.fill.clone(),
            stroke:             p.stroke.clone(),
            stroke_width:       p.stroke_width.unwrap_or(1.0),
            stroke_dash:        p.stroke_dash.as_deref().and_then(stroke_dash_to_vec),
            fill_rule_evenodd:  p.fill_rule.as_deref() == Some("evenodd"),
            line_cap:           p.line_cap.as_deref().map(line_cap_to_u8).unwrap_or(0),
            line_join:          0,
        }),
        CanvasPrimitive::Text(t) => {
            let runs = t.runs.iter().map(|r| RenderCanvasRun {
                text:  r.text.clone(),
                font:  r.font.clone(),
            }).collect();
            RenderNode::CanvasText(RenderCanvasText {
                x:           t.x,
                y:           t.y,
                font:        t.font.as_deref().unwrap_or("Helvetica").to_string(),
                size:        t.font_size.unwrap_or(12.0),
                color:       t.color.as_deref().unwrap_or("#000000").to_string(),
                align:       t.align.as_deref().unwrap_or("left").to_string(),
                line_height: t.line_height.unwrap_or(1.2),
                width:       t.w.or(Some(page_w - t.x)),
                content:     t.content.clone(),
                anchor_col:  t.anchor_col,
                opacity:     t.opacity,
                runs,
            })
        }
        CanvasPrimitive::Img(i) => RenderNode::CanvasImage(RenderCanvasImage {
            x:   i.x,
            y:   i.y,
            w:   i.w,
            h:   i.h,
            src: i.name.clone(),
        }),
    }
}

fn layer_to_render_node(layer: &CanvasLayer, page_w: f32) -> RenderNode {
    let children = layer.children.iter()
        .map(|p| primitive_to_render_node(p, page_w))
        .collect();
    RenderNode::CanvasLayer(RenderCanvasLayer {
        transform: layer.transform,
        clip:      None,  // clip rendering deferred
        opacity:   layer.opacity.unwrap_or(1.0).clamp(0.0, 1.0),
        children,
    })
}

/// Stamp layers onto the render pages produced by `layout_page`.
///
/// When `prepend = true` (underlays) the layer's render node is inserted at
/// index 0 of the target page's node list so it paints beneath layout content.
/// Underlays are inserted in **reverse** document order so that the first layer
/// in the document ends up at index 0 after all inserts.
///
/// When `prepend = false` (overlays) the layer is appended.
///
/// `pages` must be the pages of a single section (the output of one
/// `layout_page` call).  Page-scope indices are relative to this slice.
pub(crate) fn apply_canvas_layers(
    pages:   &mut Vec<RenderPage>,
    layers:  &[CanvasLayer],
    prepend: bool,
) {
    let n = pages.len();
    if n == 0 || layers.is_empty() { return; }

    if prepend {
        for layer in layers.iter().rev() {
            for i in page_scope_indices(&layer.page, n) {
                let page_w = pages[i].width;
                let node = layer_to_render_node(layer, page_w);
                pages[i].nodes.insert(0, node);
            }
        }
    } else {
        for layer in layers {
            for i in page_scope_indices(&layer.page, n) {
                let page_w = pages[i].width;
                let node = layer_to_render_node(layer, page_w);
                pages[i].nodes.push(node);
            }
        }
    }
}
