/// Layers — coordinate-based overlay/underlay rendering.
///
/// This module owns the layer type model, XML and JSON parsers, and the
/// function that stamps layers onto already-laid-out `RenderPage`s.
///
/// Layers use top-left origin, y-down coordinates.  The PDF render layer
/// (`pdf.rs`) flips to bottom-up when emitting content streams.

use std::collections::HashMap;
use crate::tokens::{FontDef, Tokens};
use crate::parse::{
    Node, PageScope, Shadow, Shape, ShapeKind, TextDefaults,
    parse_measurement, parse_shadow, parse_signed_measurement, parse_page_scope,
    elems, jattr, node_loc, at_element_for, parse_at_node, parse_tree_at_node,
};
use crate::page_scope::page_scope_indices;
use crate::render::{
    RenderCanvasClip, RenderCanvasEllipse, RenderCanvasLayer,
    RenderBox, RenderCanvasLine, RenderCanvasPath, RenderCanvasRect,
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
    pub shadow:       Option<Shadow>,
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
    pub shadow:       Option<Shadow>,
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
    pub shadow:       Option<Shadow>,
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

/// A `text-at`, `img-at` or `barcode-at`: the text, image or barcode of the layout, put at a fixed place. It is
/// the same node, and the layout's own code measures, wraps and draws it; only where it goes is the layer's.
#[derive(Debug, Clone)]
pub struct PlacedNode {
    pub node:   Node,
    /// Where `anchor` goes: the anchor point of the page, moved by the element's `x` and `y`.
    pub x:      f32,
    pub y:      f32,
    /// The point of the node's box that lands on `x`, `y`. The top left corner without an `anchor`.
    pub anchor: Anchor,
}

#[derive(Debug, Clone)]
pub enum CanvasPrimitive {
    Rect(CanvasRect),
    Circle(CanvasCircle),
    Ellipse(CanvasEllipse),
    Line(CanvasLine),
    Path(CanvasPath),
    Placed(PlacedNode),
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
    /// Optional clip rectangle `[x, y, w, h]`: what the layer draws outside it is cut off.
    pub clip:      Option<[f32; 4]>,
    /// Draw the layer only when this value is truthy, and only when `data_if_not` is falsy. Bound by `data.rs`.
    pub data_if:     Option<String>,
    pub data_if_not: Option<String>,
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
pub(crate) fn apply_anchor_offset(anchor: &Anchor, x: f32, y: f32, w: f32, h: f32) -> (f32, f32) {
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

/// The `fill` of a shape: a color, or `gradient:name`.
fn opt_canvas_fill(elem: &roxmltree::Node, tokens: &Tokens) -> Result<Option<String>, String> {
    elem.attribute("fill").map(|v| tokens.resolve_fill(v)).transpose()
}

fn opt_canvas_shadow(elem: &roxmltree::Node, tokens: &Tokens) -> Result<Option<Shadow>, String> {
    elem.attribute("shadow").map(|v| parse_shadow(v, tokens)).transpose()
}

fn parse_canvas_rect(elem: &roxmltree::Node, tokens: &Tokens, page_w: f32, page_h: f32) -> Result<CanvasPrimitive, String> {
    let w            = parse_measurement(elem.attribute("width").unwrap_or("0pt"))?;
    let h            = parse_measurement(elem.attribute("height").unwrap_or("0pt"))?;
    let (x, y)       = parse_canvas_box_position(elem, page_w, page_h, w, h)?;
    let fill         = opt_canvas_fill(elem, tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    let radius       = elem.attribute("radius").map(parse_measurement).transpose()?;
    let shadow       = opt_canvas_shadow(elem, tokens)?;
    Ok(CanvasPrimitive::Rect(CanvasRect { x, y, w, h, fill, stroke, stroke_width, stroke_dash, radius, shadow }))
}

fn parse_canvas_circle(elem: &roxmltree::Node, tokens: &Tokens, page_w: f32, page_h: f32) -> Result<CanvasPrimitive, String> {
    let r            = parse_measurement(elem.attribute("r").unwrap_or("0pt"))?;
    let (x, y)       = parse_canvas_box_position(elem, page_w, page_h, 2.0 * r, 2.0 * r)?;
    let (cx, cy)     = (x + r, y + r);
    let fill         = opt_canvas_fill(elem, tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    let shadow       = opt_canvas_shadow(elem, tokens)?;
    Ok(CanvasPrimitive::Circle(CanvasCircle { cx, cy, r, fill, stroke, stroke_width, stroke_dash, shadow }))
}

fn parse_canvas_ellipse(elem: &roxmltree::Node, tokens: &Tokens, page_w: f32, page_h: f32) -> Result<CanvasPrimitive, String> {
    let rx           = parse_measurement(elem.attribute("rx").unwrap_or("0pt"))?;
    let ry           = parse_measurement(elem.attribute("ry").unwrap_or("0pt"))?;
    let (x, y)       = parse_canvas_box_position(elem, page_w, page_h, 2.0 * rx, 2.0 * ry)?;
    let (cx, cy)     = (x + rx, y + ry);
    let fill         = opt_canvas_fill(elem, tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    let shadow       = opt_canvas_shadow(elem, tokens)?;
    Ok(CanvasPrimitive::Ellipse(CanvasEllipse { cx, cy, rx, ry, fill, stroke, stroke_width, stroke_dash, shadow }))
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
    let fill         = opt_canvas_fill(elem, tokens)?;
    let stroke       = opt_canvas_color(elem, "stroke", tokens)?;
    let fill_rule    = elem.attribute("fill-rule").map(str::to_string);
    let stroke_width = elem.attribute("stroke-width").map(parse_measurement).transpose()?;
    let stroke_dash  = elem.attribute("stroke-dash").map(str::to_string);
    let line_cap     = elem.attribute("line-cap").map(str::to_string);
    Ok(CanvasPrimitive::Path(CanvasPath { d, fill, stroke, fill_rule, stroke_width, stroke_dash, line_cap }))
}

/// What a layer's elements are read against: the document's assets and tokens, the defaults its section gives
/// text, and the size of the section's pages, which anchors measure from.
pub(crate) struct LayerContext<'a> {
    pub tokens:       &'a Tokens,
    pub asset_images: &'a HashMap<String, String>,
    pub asset_fonts:  &'a HashMap<String, FontDef>,
    pub text:         &'a TextDefaults,
    pub page_w:       f32,
    pub page_h:       f32,
}

/// A `text-at`, `img-at` or `barcode-at`: the node of its flow version, and where it goes.
fn parse_placed_elem(elem: &roxmltree::Node, ctx: &LayerContext) -> Result<CanvasPrimitive, String> {
    let node = parse_at_node(elem, ctx.tokens, ctx.asset_images, ctx.asset_fonts, ctx.text)?;
    let anchor = elem.attribute("anchor").map(parse_anchor).transpose()?.unwrap_or(Anchor::TopLeft);
    let (x, y) = parse_canvas_position(elem, ctx.page_w, ctx.page_h)?;
    Ok(CanvasPrimitive::Placed(PlacedNode { node, x, y, anchor }))
}

fn parse_canvas_primitive_elem(elem: &roxmltree::Node, ctx: &LayerContext) -> Result<CanvasPrimitive, String> {
    let tag = elem.tag_name().name();
    let located = |e: String| format!("<{tag}>{}: {e}", node_loc(elem));
    let (tokens, page_w, page_h) = (ctx.tokens, ctx.page_w, ctx.page_h);
    match tag {
        "rect"       => parse_canvas_rect(elem, tokens, page_w, page_h),
        "circle"     => parse_canvas_circle(elem, tokens, page_w, page_h),
        "ellipse"    => parse_canvas_ellipse(elem, tokens, page_w, page_h),
        "line"       => parse_canvas_line(elem, tokens),
        "path"       => parse_canvas_path(elem, tokens),
        "text-at" | "img-at" | "barcode-at" => parse_placed_elem(elem, ctx),
        "text" | "img" | "barcode" => Err(format!(
            "in a layer, a {tag} is a <{}>", at_element_for(tag).expect("text, img or barcode")
        )),
        other => Err(format!("unknown element in <layer>: <{other}>")),
    }
    .map_err(located)
}

/// The rectangle of `clip="x y w h"`.
fn parse_clip(s: &str) -> Result<[f32; 4], String> {
    let parts: Vec<&str> = s.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).collect();
    let [x, y, w, h] = parts.as_slice() else {
        return Err(format!("invalid clip '{s}': expected x y w h, like \"0pt 0pt 200pt 100pt\""));
    };
    Ok([parse_signed_measurement(x)?, parse_signed_measurement(y)?, parse_measurement(w)?, parse_measurement(h)?])
}

/// What a layer reads of the data: whether it is drawn, not what it says.
const LAYER_DATA: &str = "'data-value' and 'data-source' do not apply to a layer: it takes data-if and data-if-not, \
    and the elements in it take data-value";

/// Parse a `<layer>` element.
pub(crate) fn parse_layer_elem(elem: &roxmltree::Node, ctx: &LayerContext) -> Result<CanvasLayer, String> {
    let located = |e: String| format!("<layer>{}: {e}", node_loc(elem));
    if elem.attribute("data-value").is_some() || elem.attribute("data-source").is_some() {
        return Err(located(LAYER_DATA.into()));
    }
    let page = elem.attribute("page")
        .map(parse_page_scope)
        .transpose()
        .map_err(located)?;
    let opacity = elem.attribute("opacity")
        .map(|v| v.parse::<f32>().map_err(|_| located(format!("invalid layer opacity '{v}'"))))
        .transpose()?;
    let transform = elem.attribute("transform").and_then(parse_transform);
    let clip = elem.attribute("clip").map(parse_clip).transpose().map_err(located)?;

    // Its text-at take what the layer sets for text over what the section gives it.
    let text = ctx.text
        .overridden_by(ctx.tokens, elem.attribute("font"), elem.attribute("font-size"), elem.attribute("color"))
        .map_err(located)?;
    let ctx = LayerContext { text: &text, ..*ctx };

    let mut children = Vec::new();
    for child in elems(elem) {
        children.push(parse_canvas_primitive_elem(&child, &ctx)?);
    }
    Ok(CanvasLayer {
        children, page, opacity, transform, clip,
        data_if:     elem.attribute("data-if").map(str::to_string),
        data_if_not: elem.attribute("data-if-not").map(str::to_string),
    })
}

// ── JSON parse path ───────────────────────────────────────────────────────────

/// Parse a layer from a JSON tree node.
pub(crate) fn parse_tree_layer(json: &serde_json::Value, ctx: &LayerContext) -> Result<CanvasLayer, String> {
    if jattr(json, "data-value").is_some() || jattr(json, "data-source").is_some() {
        return Err(format!("layer: {LAYER_DATA}"));
    }
    let page = jattr(json, "page").map(parse_page_scope).transpose()?;
    let opacity = jattr(json, "opacity")
        .map(|v| v.parse::<f32>().map_err(|_| format!("invalid layer opacity '{v}'")))
        .transpose()?;
    let transform = jattr(json, "transform").and_then(parse_transform);
    let clip = jattr(json, "clip").map(parse_clip).transpose().map_err(|e| format!("layer: {e}"))?;

    // Its text-at take what the layer sets for text over what the section gives it.
    let text = ctx.text
        .overridden_by(ctx.tokens, jattr(json, "font"), jattr(json, "font-size"), jattr(json, "color"))
        .map_err(|e| format!("layer: {e}"))?;
    let ctx = LayerContext { text: &text, ..*ctx };

    let mut children = Vec::new();
    if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
        for n in arr {
            children.push(parse_tree_canvas_primitive(n, &ctx)?);
        }
    }
    Ok(CanvasLayer {
        children, page, opacity, transform, clip,
        data_if:     jattr(json, "data-if").map(str::to_string),
        data_if_not: jattr(json, "data-if-not").map(str::to_string),
    })
}

fn parse_tree_canvas_primitive(json: &serde_json::Value, ctx: &LayerContext) -> Result<CanvasPrimitive, String> {
    let (tokens, page_w, page_h) = (ctx.tokens, ctx.page_w, ctx.page_h);
    let type_str = json.get("type").and_then(|v| v.as_str())
        .ok_or("canvas primitive missing 'type'")?;

    let get_attr = |k: &str| jattr(json, k);
    let get_color = |k: &str| -> Result<Option<String>, String> {
        get_attr(k).map(|v| tokens.resolve_color(v)).transpose()
    };
    let get_fill = || -> Result<Option<String>, String> {
        get_attr("fill").map(|v| tokens.resolve_fill(v)).transpose()
    };
    let get_shadow = || -> Result<Option<Shadow>, String> {
        get_attr("shadow").map(|v| parse_shadow(v, tokens)).transpose()
    };
    let get_f32 = |k: &str| -> Result<Option<f32>, String> {
        get_attr(k).map(parse_measurement).transpose()
    };
    let get_f32_signed = |k: &str| -> Result<Option<f32>, String> {
        get_attr(k).map(parse_signed_measurement).transpose()
    };
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
                fill:         get_fill()?,
                stroke:       get_color("stroke")?,
                stroke_width: get_f32("stroke-width")?,
                stroke_dash:  get_attr("stroke-dash").map(str::to_string),
                radius:       get_f32("radius")?,
                shadow:       get_shadow()?,
            }))
        }
        "circle" => {
            let r = get_f32("r")?.unwrap_or(0.0);
            let (x, y) = get_box_pos(2.0 * r, 2.0 * r)?;
            Ok(CanvasPrimitive::Circle(CanvasCircle {
                cx: x + r, cy: y + r, r,
                fill:         get_fill()?,
                stroke:       get_color("stroke")?,
                stroke_width: get_f32("stroke-width")?,
                stroke_dash:  get_attr("stroke-dash").map(str::to_string),
                shadow:       get_shadow()?,
            }))
        }
        "ellipse" => {
            let rx = get_f32("rx")?.unwrap_or(0.0);
            let ry = get_f32("ry")?.unwrap_or(0.0);
            let (x, y) = get_box_pos(2.0 * rx, 2.0 * ry)?;
            Ok(CanvasPrimitive::Ellipse(CanvasEllipse {
                cx: x + rx, cy: y + ry, rx, ry,
                fill:         get_fill()?,
                stroke:       get_color("stroke")?,
                stroke_width: get_f32("stroke-width")?,
                stroke_dash:  get_attr("stroke-dash").map(str::to_string),
                shadow:       get_shadow()?,
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
            fill:         get_fill()?,
            stroke:       get_color("stroke")?,
            fill_rule:    get_attr("fill-rule").map(str::to_string),
            stroke_width: get_f32("stroke-width")?,
            stroke_dash:  get_attr("stroke-dash").map(str::to_string),
            line_cap:     get_attr("line-cap").map(str::to_string),
        })),
        "text-at" | "img-at" | "barcode-at" => {
            let node = parse_tree_at_node(json, tokens, ctx.asset_images, ctx.asset_fonts, ctx.text)
                .map_err(|e| format!("canvas primitive '{type_str}': {e}"))?;
            let anchor = get_attr("anchor").map(parse_anchor).transpose()?.unwrap_or(Anchor::TopLeft);
            let (x, y) = get_pos()?;
            Ok(CanvasPrimitive::Placed(PlacedNode { node, x, y, anchor }))
        }
        "text" | "img" | "barcode" => Err(format!(
            "canvas primitive '{type_str}': in a layer, a {type_str} is a '{}'",
            at_element_for(type_str).expect("text, img or barcode")
        )),
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

/// `page` is the number of the page the layer is drawn on, counted from 1 in its section, and `pages` how many
/// pages the section has: what `{page}` and `{pages}` in a `text-at` stand for.
fn primitive_to_render_node(prim: &CanvasPrimitive, page: usize, pages: usize) -> RenderNode {
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
            shadow:        r.shadow.clone(),
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
            shadow:       c.shadow.clone(),
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
            shadow:       e.shadow.clone(),
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
        CanvasPrimitive::Placed(placed) => crate::layout::layout_placed(placed, page, pages),
    }
}

/// What a `<shape>` of the layout draws in its box `x`, `y`, `w`, `h`, which the flow gave it: the rect, circle
/// or ellipse of a layer, fitted to the box. A rect has the corners `radius` rounds; a circle's box is square.
pub(crate) fn draw_flow_shape(shape: &Shape, radius: f32, x: f32, y: f32, w: f32, h: f32) -> RenderNode {
    let Shape { fill, stroke, stroke_width, stroke_dash, .. } = shape.clone();
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let primitive = match shape.kind {
        ShapeKind::Rect => CanvasPrimitive::Rect(CanvasRect {
            x, y, w, h, fill, stroke, stroke_width, stroke_dash, radius: Some(radius), shadow: None,
        }),
        ShapeKind::Circle => CanvasPrimitive::Circle(CanvasCircle {
            cx, cy, r: w / 2.0, fill, stroke, stroke_width, stroke_dash, shadow: None,
        }),
        ShapeKind::Ellipse => CanvasPrimitive::Ellipse(CanvasEllipse {
            cx, cy, rx: w / 2.0, ry: h / 2.0, fill, stroke, stroke_width, stroke_dash, shadow: None,
        }),
        ShapeKind::Checkbox => return draw_checkbox(shape, radius, x, y, w, h),
    };
    primitive_to_render_node(&primitive, 1, 1)
}

/// The three points of a checkbox's tick, as fractions of its box: a short stroke down to the foot, and a long one up
/// to the right.
const TICK: [(f32, f32); 3] = [(0.22, 0.54), (0.43, 0.76), (0.80, 0.26)];

/// The stroke of a tick, as a fraction of the side of the box.
const TICK_STROKE: f32 = 0.14;

/// A checkbox in the box `x`, `y`, `w`, `h`: an outline with rounded corners, and a tick in it when `shape.tick` has
/// a color. The stroke of the outline is centered on its edge, so the outline is drawn inside the box by half of it
/// and ends where the box does.
fn draw_checkbox(shape: &Shape, radius: f32, x: f32, y: f32, w: f32, h: f32) -> RenderNode {
    let stroke_width = shape.stroke_width.unwrap_or(1.0);
    let inset = stroke_width / 2.0;
    let outline = CanvasPrimitive::Rect(CanvasRect {
        x: x + inset, y: y + inset, w: (w - stroke_width).max(0.0), h: (h - stroke_width).max(0.0),
        fill: shape.fill.clone(), stroke: shape.stroke.clone(), stroke_width: Some(stroke_width), stroke_dash: None,
        radius: Some((radius - inset).max(0.0)), shadow: None,
    });
    let mut children = vec![primitive_to_render_node(&outline, 1, 1)];
    if let Some(color) = &shape.tick {
        let [a, b, c] = TICK.map(|(fx, fy)| format!("{} {}", x + fx * w, y + fy * h));
        let tick = CanvasPrimitive::Path(CanvasPath {
            d: format!("M {a} L {b} L {c}"),
            fill: None, stroke: Some(color.clone()), fill_rule: None,
            stroke_width: Some(TICK_STROKE * w), stroke_dash: None, line_cap: Some("round".to_string()),
        });
        children.push(primitive_to_render_node(&tick, 1, 1));
    }
    RenderNode::Box(RenderBox {
        x, y, width: w, height: h,
        fill: None, border_width: 0.0, border_color: None, radius: 0.0,
        opacity: 1.0, shadow: None, debug_self: false,
        children,
    })
}

fn layer_to_render_node(layer: &CanvasLayer, page: usize, pages: usize) -> RenderNode {
    let children = layer.children.iter()
        .map(|p| primitive_to_render_node(p, page, pages))
        .collect();
    RenderNode::CanvasLayer(RenderCanvasLayer {
        transform: layer.transform,
        clip:      layer.clip.map(|[x, y, w, h]| RenderCanvasClip { x, y, w, h, border_radius: 0.0 }),
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
                let node = layer_to_render_node(layer, i + 1, n);
                pages[i].nodes.insert(0, node);
            }
        }
    } else {
        for layer in layers {
            for i in page_scope_indices(&layer.page, n) {
                let node = layer_to_render_node(layer, i + 1, n);
                pages[i].nodes.push(node);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkbox(tick: Option<&str>) -> Shape {
        Shape {
            kind: ShapeKind::Checkbox, fill: None, stroke: Some("#737373".into()), stroke_width: Some(1.0),
            stroke_dash: None, tick: tick.map(str::to_string),
        }
    }

    #[test]
    fn an_open_checkbox_is_an_outline_that_ends_where_its_box_does() {
        let RenderNode::Box(drawn) = draw_flow_shape(&checkbox(None), 2.0, 10.0, 20.0, 10.0, 10.0) else { panic!("a box") };
        assert_eq!(drawn.children.len(), 1);
        let RenderNode::CanvasRect(outline) = &drawn.children[0] else { panic!("a rect") };
        // The stroke is centered on the edge, so a stroke of 1 puts the rect half of it inside on each side.
        assert_eq!((outline.x, outline.y, outline.w, outline.h), (10.5, 20.5, 9.0, 9.0));
        assert_eq!((outline.fill.clone(), outline.stroke.as_deref(), outline.border_radius), (None, Some("#737373"), 1.5));
    }

    #[test]
    fn a_checked_checkbox_has_a_tick_of_three_points_inside_the_box_in_the_color_of_the_tick() {
        let RenderNode::Box(drawn) = draw_flow_shape(&checkbox(Some("#1a1a1a")), 2.0, 0.0, 0.0, 10.0, 10.0) else { panic!("a box") };
        assert_eq!(drawn.children.len(), 2);
        let RenderNode::CanvasPath(tick) = &drawn.children[1] else { panic!("a path") };
        assert_eq!(tick.stroke.as_deref(), Some("#1a1a1a"));
        assert_eq!((tick.fill.clone(), tick.line_cap), (None, 1));
        let numbers: Vec<f32> = tick.d.split_whitespace().filter_map(|word| word.parse().ok()).collect();
        assert_eq!(numbers.len(), 6);
        assert!(numbers.iter().all(|n| (0.0..=10.0).contains(n)), "{}", tick.d);
    }
}
