/// Canvas — coordinate-based rendering mode.
///
/// Canvas trees and XML/kit trees are separate render modes.  `render_tree_pdf`
/// accepts one or the other per call; they are not mixed.
///
/// This module handles:
///   1. JSON parsing of canvas documents → `CanvasDocument`
///   2. Layout (coordinate pass-through + overflow pagination) → `Vec<RenderPage>`
///
/// Canvas uses top-left origin, y-down coordinates.  The PDF render layer
/// (`pdf.rs`) flips to bottom-up when emitting content streams.

use std::collections::HashMap;
use crate::parse::Meta;
use crate::tokens::FontDef;
use crate::render::{
    RenderCanvasClip, RenderCanvasEllipse, RenderCanvasImage, RenderCanvasLayer,
    RenderCanvasLine, RenderCanvasPath, RenderCanvasRect, RenderCanvasRun, RenderCanvasText,
    RenderNode, RenderPage,
};
use crate::tokens::{FontWidths, Tokens};

// ── Canvas document model ─────────────────────────────────────────────────────

pub struct CanvasDocument {
    pub meta:        Meta,
    pub fonts:       HashMap<String, FontDef>,
    pub font_widths: HashMap<String, FontWidths>,
    pub images:      HashMap<String, String>,
    pub pages:       Vec<CanvasPage>,
}

pub struct CanvasPage {
    pub width:      f32,
    pub height:     f32,
    pub margin:     [f32; 4],
    pub background: Option<String>,
    pub nodes:      Vec<CanvasNode>,
}

pub enum CanvasNode {
    Text(CanvasText),
    Rect(CanvasRect),
    Line(CanvasLine),
    Ellipse(CanvasEllipse),
    Path(CanvasPath),
    Image(CanvasImage),
    Layer(CanvasLayer),
}

pub struct CanvasText {
    pub x:           f32,
    pub y:           f32,
    pub font:        String,
    pub size:        f32,
    pub color:       String,
    pub align:       String,
    pub line_height: f32,
    pub width:       Option<f32>,
    pub content:     String,
    pub runs:        Vec<CanvasRun>,
}

pub struct CanvasRun {
    pub text:  String,
    pub font:  Option<String>,
    pub size:  Option<f32>,
    pub color: Option<String>,
}

pub struct CanvasRect {
    pub x:             f32,
    pub y:             f32,
    pub w:             f32,
    pub h:             f32,
    pub fill:          Option<String>,
    pub stroke:        Option<String>,
    pub stroke_width:  f32,
    pub stroke_dash:   Option<Vec<f32>>,
    pub border_radius: f32,
}

pub struct CanvasLine {
    pub x1:           f32,
    pub y1:           f32,
    pub x2:           f32,
    pub y2:           f32,
    pub stroke:       String,
    pub stroke_width: f32,
    pub stroke_dash:  Option<Vec<f32>>,
    pub line_cap:     u8,
    pub line_join:    u8,
}

pub struct CanvasEllipse {
    pub cx:           f32,
    pub cy:           f32,
    pub rx:           f32,
    pub ry:           f32,
    pub fill:         Option<String>,
    pub stroke:       Option<String>,
    pub stroke_width: f32,
    pub stroke_dash:  Option<Vec<f32>>,
}

pub struct CanvasPath {
    pub d:                  String,
    pub fill:               Option<String>,
    pub stroke:             Option<String>,
    pub stroke_width:       f32,
    pub stroke_dash:        Option<Vec<f32>>,
    pub fill_rule_evenodd:  bool,
    pub line_cap:           u8,
    pub line_join:          u8,
}

pub struct CanvasImage {
    pub x:   f32,
    pub y:   f32,
    pub w:   f32,
    pub h:   f32,
    pub src: String,
}

pub struct CanvasLayer {
    pub transform: Option<[f32; 6]>,
    pub clip:      Option<CanvasClip>,
    pub opacity:   f32,
    /// 1-based page index target (None = place on the current page).
    pub page:      Option<usize>,
    pub children:  Vec<CanvasNode>,
}

pub struct CanvasClip {
    pub x:             f32,
    pub y:             f32,
    pub w:             f32,
    pub h:             f32,
    pub border_radius: f32,
}

// ── Peek helper ───────────────────────────────────────────────────────────────

/// Return `true` if the JSON document uses canvas nodes (not kit nodes).
/// Detects by checking the root "type" field for "canvas-document".
pub fn is_canvas_tree(json: &str) -> bool {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v)  => v,
        Err(_) => return false,
    };
    v.get("type")
     .and_then(|t| t.as_str())
     .map(|t| t == "canvas-document")
     .unwrap_or(false)
}

// ── JSON parsing ──────────────────────────────────────────────────────────────

/// Parse a canvas JSON document tree into a `CanvasDocument`.
pub fn parse_canvas_tree(json: &str) -> Result<CanvasDocument, String> {
    use crate::parse::parse_page_size;

    let root: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse error: {e}"))?;

    match root.get("version").and_then(|v| v.as_u64()) {
        Some(1) => {}
        Some(v) => return Err(format!("unsupported tree version: {v}")),
        None    => return Err("tree JSON missing 'version' field".into()),
    }
    if root.get("type").and_then(|v| v.as_str()) != Some("canvas-document") {
        return Err("tree root 'type' must be 'canvas-document'".into());
    }

    let empty_map = serde_json::Map::new();
    let attrs = root.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty_map);

    // ── Tokens (for size / color resolution) ─────────────────────────────────
    let mut tokens = Tokens::default();
    if let Some(tok) = attrs.get("tokens").and_then(|v| v.as_object()) {
        crate::parse::parse_tree_tokens_pub(tok, &mut tokens)?;
    }

    // ── Assets ────────────────────────────────────────────────────────────────
    let mut asset_fonts:       HashMap<String, FontDef>   = HashMap::new();
    let mut asset_images:      HashMap<String, String>     = HashMap::new();
    let mut asset_font_widths: HashMap<String, FontWidths> = HashMap::new();

    if let Some(assets) = attrs.get("assets").and_then(|v| v.as_object()) {
        if let Some(fonts_obj) = assets.get("fonts").and_then(|v| v.as_object()) {
            for (name, def) in fonts_obj {
                let font_def = if let Some(b) = def.get("core").and_then(|v| v.as_str()) {
                    FontDef::Core(b.to_string())
                } else if let Some(r) = def.get("ref").and_then(|v| v.as_str()) {
                    FontDef::Ref(r.to_string())
                } else {
                    return Err(format!("asset font '{name}' needs 'core' or 'ref'"));
                };
                if let Some(w) = def.get("widths").and_then(|v| v.as_object()) {
                    let default = w.get("default").and_then(|v| v.as_u64()).unwrap_or(500) as u16;
                    let ascii: Vec<u16> = w.get("ascii")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().map(|n| n.as_u64().unwrap_or(500) as u16).collect())
                        .unwrap_or_default();
                    asset_font_widths.insert(name.clone(), FontWidths { default, ascii });
                }
                asset_fonts.insert(name.clone(), font_def);
            }
        }
        if let Some(images_obj) = assets.get("images").and_then(|v| v.as_object()) {
            for (name, def) in images_obj {
                let key = def.get("ref").and_then(|v| v.as_str())
                    .unwrap_or(name)
                    .to_string();
                asset_images.insert(name.clone(), key);
            }
        }
    }

    // ── Meta ──────────────────────────────────────────────────────────────────
    let meta = if let Some(m) = attrs.get("meta").and_then(|v| v.as_object()) {
        Meta {
            title:    m.get("title")   .and_then(|v| v.as_str()).unwrap_or("").to_string(),
            author:   m.get("author")  .and_then(|v| v.as_str()).unwrap_or("").to_string(),
            subject:  m.get("subject") .and_then(|v| v.as_str()).unwrap_or("").to_string(),
            keywords: m.get("keywords").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            creator:  m.get("creator") .and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    } else {
        Meta::default()
    };

    // ── Document-level page defaults ──────────────────────────────────────────
    let mut doc_size = if let Some(s) = attrs.get("size").and_then(|v| v.as_str()) {
        parse_page_size(s)?
    } else {
        (595.28_f32, 841.89_f32)
    };
    if attrs.get("orientation").and_then(|v| v.as_str()) == Some("landscape") {
        doc_size = (doc_size.1, doc_size.0);
    }
    let doc_margin = if let Some(s) = attrs.get("margin").and_then(|v| v.as_str()) {
        tokens.resolve_spacing(s)?
    } else {
        [0.0_f32; 4]
    };
    let doc_background = if let Some(s) = attrs.get("background").and_then(|v| v.as_str()) {
        Some(tokens.resolve_color(s)?)
    } else {
        None
    };

    // ── Pages ─────────────────────────────────────────────────────────────────
    let page_arr = root.get("pages").and_then(|v| v.as_array())
        .ok_or("canvas tree JSON 'pages' must be an array")?;

    let mut pages: Vec<CanvasPage> = Vec::new();
    for child in page_arr {
        if child.get("type").and_then(|v| v.as_str()) != Some("canvas-page") {
            return Err("canvas document pages must all be canvas-page nodes".into());
        }
        pages.push(parse_canvas_page(child, doc_size, doc_margin, doc_background.clone(), &tokens, &asset_images)?);
    }
    if pages.is_empty() {
        return Err("canvas document must have at least one page".into());
    }

    Ok(CanvasDocument {
        meta,
        fonts:       asset_fonts,
        font_widths: asset_font_widths,
        images:      asset_images,
        pages,
    })
}

fn parse_canvas_page(
    json:           &serde_json::Value,
    doc_size:       (f32, f32),
    doc_margin:     [f32; 4],
    doc_background: Option<String>,
    tokens:         &Tokens,
    asset_images:   &HashMap<String, String>,
) -> Result<CanvasPage, String> {
    use crate::parse::parse_page_size;

    let mut size   = doc_size;
    let mut margin = doc_margin;
    let mut bg     = doc_background;

    let empty = serde_json::Map::new();
    let attrs  = json.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty);

    if let Some(s) = attrs.get("size").and_then(|v| v.as_str()) {
        size = parse_page_size(s)?;
    } else {
        if let Some(w) = attrs.get("width").and_then(|v| v.as_f64()) { size.0 = w as f32; }
        if let Some(h) = attrs.get("height").and_then(|v| v.as_f64()) { size.1 = h as f32; }
    }
    if attrs.get("orientation").and_then(|v| v.as_str()) == Some("landscape") {
        size = (size.1, size.0);
    }
    if let Some(s) = attrs.get("margin").and_then(|v| v.as_str()) {
        margin = tokens.resolve_spacing(s)?;
    }
    if let Some(s) = attrs.get("background").and_then(|v| v.as_str()) {
        bg = Some(tokens.resolve_color(s)?);
    }

    let mut nodes: Vec<CanvasNode> = Vec::new();
    if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
        for child in arr {
            nodes.push(parse_canvas_node(child, tokens, asset_images)?);
        }
    }

    Ok(CanvasPage { width: size.0, height: size.1, margin, background: bg, nodes })
}

fn jf(json: &serde_json::Value, key: &str) -> Option<f32> {
    let v = json.get("attrs")?.get(key)?;
    v.as_f64().map(|n| n as f32)
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn js<'a>(json: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    json.get("attrs")?.get(key)?.as_str()
}

fn parse_dash(json: &serde_json::Value, key: &str) -> Option<Vec<f32>> {
    json.get("attrs")?.get(key)?.as_array().map(|arr| {
        arr.iter().filter_map(|v| v.as_f64().map(|n| n as f32)).collect()
    })
}

fn parse_line_cap(s: &str) -> u8 {
    match s { "round" => 1, "square" => 2, _ => 0 }
}

fn parse_line_join(s: &str) -> u8 {
    match s { "round" => 1, "bevel" => 2, _ => 0 }
}

fn parse_canvas_node(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
) -> Result<CanvasNode, String> {
    let type_str = json.get("type").and_then(|v| v.as_str())
        .ok_or("canvas node missing 'type' field")?;

    match type_str {
        "canvas-text" => {
            let x    = jf(json, "x").unwrap_or(0.0);
            let y    = jf(json, "y").unwrap_or(0.0);
            let font = js(json, "font").unwrap_or("Helvetica").to_string();
            let size = jf(json, "size").unwrap_or(12.0);
            let color_raw = js(json, "color").unwrap_or("#000000");
            let color = tokens.resolve_color(color_raw).unwrap_or_else(|_| color_raw.to_string());
            let align = js(json, "align").unwrap_or("left").to_string();
            let line_height = jf(json, "line-height").unwrap_or(1.2);
            let width = jf(json, "width");

            // Plain content attr takes priority; children = mixed runs fallback.
            let (content, runs) = if let Some(c) = js(json, "content") {
                (c.to_string(), vec![])
            } else if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
                let runs = arr.iter().filter_map(|child| {
                    if child.get("type").and_then(|v| v.as_str()) == Some("canvas-run") {
                        let text = child.get("children").and_then(|v| v.as_array())
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let run_font  = child.get("attrs").and_then(|v| v.get("font")).and_then(|v| v.as_str()).map(str::to_string);
                        let run_size  = child.get("attrs").and_then(|v| v.get("size")).and_then(|v| v.as_f64()).map(|n| n as f32);
                        let run_color = child.get("attrs").and_then(|v| v.get("color")).and_then(|v| v.as_str()).map(|s| {
                            tokens.resolve_color(s).unwrap_or_else(|_| s.to_string())
                        });
                        Some(CanvasRun { text, font: run_font, size: run_size, color: run_color })
                    } else {
                        None
                    }
                }).collect();
                (String::new(), runs)
            } else {
                (String::new(), vec![])
            };

            Ok(CanvasNode::Text(CanvasText { x, y, font, size, color, align, line_height, width, content, runs }))
        }

        "canvas-rect" => {
            let x = jf(json, "x").unwrap_or(0.0);
            let y = jf(json, "y").unwrap_or(0.0);
            let w = jf(json, "w").unwrap_or(0.0);
            let h = jf(json, "h").unwrap_or(0.0);
            let fill   = js(json, "fill")
                .filter(|&s| s != "none")
                .map(|s| tokens.resolve_color(s).unwrap_or_else(|_| s.to_string()));
            let stroke = js(json, "stroke")
                .filter(|&s| s != "none")
                .map(|s| tokens.resolve_color(s).unwrap_or_else(|_| s.to_string()));
            let stroke_width  = jf(json, "stroke-width").unwrap_or(1.0);
            let stroke_dash   = parse_dash(json, "stroke-dash");
            let border_radius = jf(json, "border-radius").unwrap_or(0.0);
            Ok(CanvasNode::Rect(CanvasRect { x, y, w, h, fill, stroke, stroke_width, stroke_dash, border_radius }))
        }

        "canvas-line" => {
            let x1 = jf(json, "x1").unwrap_or(0.0);
            let y1 = jf(json, "y1").unwrap_or(0.0);
            let x2 = jf(json, "x2").unwrap_or(0.0);
            let y2 = jf(json, "y2").unwrap_or(0.0);
            let stroke_raw   = js(json, "stroke").unwrap_or("#000000");
            let stroke       = tokens.resolve_color(stroke_raw).unwrap_or_else(|_| stroke_raw.to_string());
            let stroke_width = jf(json, "stroke-width").unwrap_or(1.0);
            let stroke_dash  = parse_dash(json, "stroke-dash");
            let line_cap     = parse_line_cap(js(json, "line-cap").unwrap_or("butt"));
            let line_join    = parse_line_join(js(json, "line-join").unwrap_or("miter"));
            Ok(CanvasNode::Line(CanvasLine { x1, y1, x2, y2, stroke, stroke_width, stroke_dash, line_cap, line_join }))
        }

        "canvas-ellipse" | "canvas-circle" => {
            let cx = jf(json, "cx").unwrap_or(0.0);
            let cy = jf(json, "cy").unwrap_or(0.0);
            let (rx, ry) = if type_str == "canvas-circle" {
                let r = jf(json, "r").unwrap_or(0.0);
                (r, r)
            } else {
                (jf(json, "rx").unwrap_or(0.0), jf(json, "ry").unwrap_or(0.0))
            };
            let fill   = js(json, "fill")
                .filter(|&s| s != "none")
                .map(|s| tokens.resolve_color(s).unwrap_or_else(|_| s.to_string()));
            let stroke = js(json, "stroke")
                .filter(|&s| s != "none")
                .map(|s| tokens.resolve_color(s).unwrap_or_else(|_| s.to_string()));
            let stroke_width = jf(json, "stroke-width").unwrap_or(1.0);
            let stroke_dash  = parse_dash(json, "stroke-dash");
            Ok(CanvasNode::Ellipse(CanvasEllipse { cx, cy, rx, ry, fill, stroke, stroke_width, stroke_dash }))
        }

        "canvas-path" => {
            let d = js(json, "d").unwrap_or("").to_string();
            let fill  = js(json, "fill")
                .filter(|&s| s != "none")
                .map(|s| tokens.resolve_color(s).unwrap_or_else(|_| s.to_string()));
            let stroke = js(json, "stroke")
                .filter(|&s| s != "none")
                .map(|s| tokens.resolve_color(s).unwrap_or_else(|_| s.to_string()));
            let stroke_width      = jf(json, "stroke-width").unwrap_or(1.0);
            let stroke_dash       = parse_dash(json, "stroke-dash");
            let fill_rule_evenodd = js(json, "fill-rule").map(|s| s == "evenodd").unwrap_or(false);
            let line_cap          = parse_line_cap(js(json, "line-cap").unwrap_or("butt"));
            let line_join         = parse_line_join(js(json, "line-join").unwrap_or("miter"));
            Ok(CanvasNode::Path(CanvasPath { d, fill, stroke, stroke_width, stroke_dash, fill_rule_evenodd, line_cap, line_join }))
        }

        "canvas-image" => {
            let x   = jf(json, "x").unwrap_or(0.0);
            let y   = jf(json, "y").unwrap_or(0.0);
            let w   = jf(json, "w").unwrap_or(0.0);
            let h   = jf(json, "h").unwrap_or(0.0);
            let src = js(json, "src").unwrap_or("").to_string();
            // Map through asset image aliases, same as kit path.
            let resolved_src = asset_images.get(&src).cloned().unwrap_or(src);
            Ok(CanvasNode::Image(CanvasImage { x, y, w, h, src: resolved_src }))
        }

        "canvas-layer" => {
            let opacity = jf(json, "opacity").unwrap_or(1.0).clamp(0.0, 1.0);
            let page    = json.get("attrs")
                .and_then(|a| a.get("page"))
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);

            // transform: { translate: [tx, ty], rotate: deg, scale: [sx, sy] }
            // Convert to a 6-element PDF CTM [a b c d e f].
            // For day-one we support translate + scale; rotate deferred.
            let transform = json.get("attrs")
                .and_then(|a| a.get("transform"))
                .and_then(|t| {
                    // Build CTM from sub-fields.
                    let tx = t.get("translate").and_then(|v| v.as_array())
                        .and_then(|a| a.first()).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let ty = t.get("translate").and_then(|v| v.as_array())
                        .and_then(|a| a.get(1)).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let sx = t.get("scale").and_then(|v| v.as_array())
                        .and_then(|a| a.first()).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    let sy = t.get("scale").and_then(|v| v.as_array())
                        .and_then(|a| a.get(1)).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                    if tx == 0.0 && ty == 0.0 && sx == 1.0 && sy == 1.0 { None }
                    else { Some([sx, 0.0, 0.0, sy, tx, ty]) }
                });

            // clip: { x, y, w, h, borderRadius? }
            let clip = json.get("attrs")
                .and_then(|a| a.get("clip"))
                .and_then(|c| c.as_object())
                .map(|c| CanvasClip {
                    x:             c.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    y:             c.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    w:             c.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    h:             c.get("h").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    border_radius: c.get("borderRadius").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                });

            let mut children: Vec<CanvasNode> = Vec::new();
            if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
                for child in arr {
                    children.push(parse_canvas_node(child, tokens, asset_images)?);
                }
            }

            Ok(CanvasNode::Layer(CanvasLayer { transform, clip, opacity, page, children }))
        }

        other => Err(format!("unknown canvas node type: '{other}'")),
    }
}

// ── Layout (canvas → RenderPage) ─────────────────────────────────────────────

/// Convert a `CanvasDocument` into `Vec<RenderPage>` ready for PDF rendering.
///
/// Page targeting (`canvas-layer` with `page: N`) is handled here.  Layers
/// that target a specific page are lifted out and appended to that page's
/// node list.  Pages are auto-created (as empty pages with the document
/// defaults) if `N` exceeds the current page count.
pub fn layout_canvas_pages(doc: &CanvasDocument) -> Vec<RenderPage> {
    // Phase 1: build base pages from the document's page list.
    let mut pages: Vec<RenderPage> = doc.pages.iter().map(|p| RenderPage {
        width:      p.width,
        height:     p.height,
        margin:     p.margin,
        background: p.background.clone(),
        nodes:      Vec::new(),
    }).collect();

    // Phase 2: walk pages, convert canvas nodes to render nodes, handle page targeting.
    for (page_idx, canvas_page) in doc.pages.iter().enumerate() {
        let mut deferred: Vec<(usize, RenderNode)> = Vec::new();
        for node in &canvas_page.nodes {
            let render_nodes = layout_canvas_node(node, canvas_page, &mut deferred);
            pages[page_idx].nodes.extend(render_nodes);
        }
        // Apply deferred page-targeted layers.
        for (target_page, render_node) in deferred {
            let target_idx = target_page.saturating_sub(1); // convert 1-based to 0-based
            // Auto-create pages up to target_idx if needed.
            while pages.len() <= target_idx {
                let last = pages.last().map(|p| (p.width, p.height, p.margin, p.background.clone()))
                    .unwrap_or((595.28, 841.89, [0.0; 4], None));
                pages.push(RenderPage {
                    width:      last.0,
                    height:     last.1,
                    margin:     last.2,
                    background: last.3,
                    nodes:      Vec::new(),
                });
            }
            pages[target_idx].nodes.push(render_node);
        }
    }

    pages
}

/// Convert one `CanvasNode` to `RenderNode`(s).
/// Page-targeted layers are pushed to `deferred` instead of returned inline.
fn layout_canvas_node(
    node:     &CanvasNode,
    page:     &CanvasPage,
    deferred: &mut Vec<(usize, RenderNode)>,
) -> Vec<RenderNode> {
    match node {
        CanvasNode::Text(t) => vec![RenderNode::CanvasText(RenderCanvasText {
            x:           t.x,
            y:           t.y,
            font:        t.font.clone(),
            size:        t.size,
            color:       t.color.clone(),
            align:       t.align.clone(),
            line_height: t.line_height,
            width:       t.width.or_else(|| Some(page.width - page.margin[1] - page.margin[3] - t.x)),
            content:     t.content.clone(),
            runs:        t.runs.iter().map(|r| RenderCanvasRun {
                text:  r.text.clone(),
                font:  r.font.clone(),
                size:  r.size,
                color: r.color.clone(),
            }).collect(),
        })],

        CanvasNode::Rect(r) => vec![RenderNode::CanvasRect(RenderCanvasRect {
            x:             r.x,
            y:             r.y,
            w:             r.w,
            h:             r.h,
            fill:          r.fill.clone(),
            stroke:        r.stroke.clone(),
            stroke_width:  r.stroke_width,
            stroke_dash:   r.stroke_dash.clone(),
            border_radius: r.border_radius,
        })],

        CanvasNode::Line(l) => vec![RenderNode::CanvasLine(RenderCanvasLine {
            x1:           l.x1,
            y1:           l.y1,
            x2:           l.x2,
            y2:           l.y2,
            stroke:       l.stroke.clone(),
            stroke_width: l.stroke_width,
            stroke_dash:  l.stroke_dash.clone(),
            line_cap:     l.line_cap,
            line_join:    l.line_join,
        })],

        CanvasNode::Ellipse(e) => vec![RenderNode::CanvasEllipse(RenderCanvasEllipse {
            cx:           e.cx,
            cy:           e.cy,
            rx:           e.rx,
            ry:           e.ry,
            fill:         e.fill.clone(),
            stroke:       e.stroke.clone(),
            stroke_width: e.stroke_width,
            stroke_dash:  e.stroke_dash.clone(),
        })],

        CanvasNode::Path(p) => vec![RenderNode::CanvasPath(RenderCanvasPath {
            d:                  p.d.clone(),
            fill:               p.fill.clone(),
            stroke:             p.stroke.clone(),
            stroke_width:       p.stroke_width,
            stroke_dash:        p.stroke_dash.clone(),
            fill_rule_evenodd:  p.fill_rule_evenodd,
            line_cap:           p.line_cap,
            line_join:          p.line_join,
        })],

        CanvasNode::Image(i) => vec![RenderNode::CanvasImage(RenderCanvasImage {
            x:   i.x,
            y:   i.y,
            w:   i.w,
            h:   i.h,
            src: i.src.clone(),
        })],

        CanvasNode::Layer(layer) => {
            let mut layer_deferred: Vec<(usize, RenderNode)> = Vec::new();
            let mut children: Vec<RenderNode> = Vec::new();
            for child in &layer.children {
                children.extend(layout_canvas_node(child, page, &mut layer_deferred));
            }
            // Nested deferred layers: propagate up (they'll be placed by the
            // outer layout_canvas_pages loop).
            deferred.extend(layer_deferred);

            let render_layer = RenderNode::CanvasLayer(RenderCanvasLayer {
                transform: layer.transform,
                clip: layer.clip.as_ref().map(|c| RenderCanvasClip {
                    x:             c.x,
                    y:             c.y,
                    w:             c.w,
                    h:             c.h,
                    border_radius: c.border_radius,
                }),
                opacity:  layer.opacity,
                children,
            });

            // If the layer has a page target, defer it; otherwise return inline.
            if let Some(target_page) = layer.page {
                deferred.push((target_page, render_layer));
                vec![]
            } else {
                vec![render_layer]
            }
        }
    }
}
