use std::collections::HashMap;
use crate::tokens::{parse_pt, FontDef, Tokens};

// ── Document model ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Document {
    pub meta: Meta,
    pub fonts: HashMap<String, FontDef>,
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, Default)]
pub struct Meta {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub width: f32,
    pub height: f32,
    pub margin: [f32; 4], // top, right, bottom, left
    pub background: Option<String>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct TextRun {
    pub text:         String,
    pub leading_space: bool,        // whitespace present before this run in source
    pub font:         Option<String>, // None = inherit from parent <text>
    pub color:        Option<String>, // None = inherit from parent <text>
    pub href:         Option<String>,
    pub underline:    bool,
    pub strike:       bool,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    // shared box attrs
    pub gap: f32,
    pub padding: [f32; 4],
    pub background: Option<String>,
    pub border: Option<(f32, String)>,
    pub radius: f32,
    pub height_mode: HeightMode,
    pub width_constraint: Option<f32>,
    pub repeat: Repeat,
    // layout-specific
    pub align: Align,
    pub justify: Justify,
    pub end: bool,
    pub equal: bool,
    pub wrap: bool,
    pub cols: u32,
    pub col_width: Option<f32>,
    // divider
    pub direction: Direction,
    pub color: Option<String>,
    pub thickness: f32,
    // text
    pub text_runs: Vec<TextRun>,
    pub font: String,
    pub font_size: f32,
    pub text_color: Option<String>,
    pub text_align: TextAlign,
    // link
    pub url: Option<String>,
    pub children: Vec<Node>,
}

/// How a node relates to page pagination.
/// Only meaningful on direct children of `<page>`.
#[derive(Debug, Clone, PartialEq)]
pub enum Repeat {
    /// Ordinary flow node — paginated normally.
    None,
    /// Page chrome — rendered on every generated page at the same position.
    Page,
    /// First-page chrome — rendered only on page 1; its space is reclaimed on later pages.
    First,
}

// ── Font-token resolution ────────────────────────────────────────────────────

/// Walk every node in the document and replace font token aliases (e.g.
/// `"heading"`, `"body"`) with their resolved builtin font names (e.g.
/// `"Helvetica-Bold"`, `"Helvetica"`).  This lets `measure_natural_w` in
/// the layout engine call `text_width` with a real font name and get correct
/// per-character AFM metrics instead of the generic 0.44 fallback.
fn resolve_font_tokens(doc: &mut Document) {
    // Clone to satisfy the borrow checker (fonts is small — typically 2–5 entries).
    let fonts = doc.fonts.clone();
    for page in &mut doc.pages {
        for node in &mut page.children {
            resolve_node_font(node, &fonts);
        }
    }
}

fn resolve_node_font(node: &mut Node, fonts: &std::collections::HashMap<String, crate::tokens::FontDef>) {
    // Resolve the node's own font field (used by Text nodes for their base font).
    if let Some(crate::tokens::FontDef::Builtin(builtin)) = fonts.get(&node.font) {
        node.font = builtin.clone();
    }
    // Resolve per-run font overrides inside Text nodes.
    for run in &mut node.text_runs {
        if let Some(ref alias) = run.font.clone() {
            if let Some(crate::tokens::FontDef::Builtin(builtin)) = fonts.get(alias) {
                run.font = Some(builtin.clone());
            }
        }
    }
    // Recurse.
    for child in &mut node.children {
        resolve_node_font(child, fonts);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Stack,
    Flank,
    Split,
    Cluster,
    Grid,
    Frame,
    Divider,
    Text,
    Link,
}

/// How a node determines its height
#[derive(Debug, Clone, PartialEq)]
pub enum HeightMode {
    /// Size to content
    Auto,
    /// Fills parent's full available height (height="full")
    Full,
    /// Takes remaining space after siblings are sized (height="fill")
    Fill,
    /// Explicit pt value (height="28pt")
    Fixed(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Justify {
    Start,
    Center,
    End,
    Between,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Node {
    fn default_for(kind: NodeKind) -> Self {
        Node {
            kind,
            gap: 0.0,
            padding: [0.0; 4],
            background: None,
            border: None,
            radius: 0.0,
            height_mode: HeightMode::Auto,
            width_constraint: None,
            repeat: Repeat::None,
            align: Align::Start,
            justify: Justify::Start,
            end: false,
            equal: false,
            wrap: true,
            cols: 1,
            col_width: None,
            direction: Direction::Horizontal,
            color: None,
            thickness: 1.0,
            text_runs: Vec::new(),
            font: "Helvetica".to_string(),
            font_size: 11.0,
            text_color: None,
            text_align: TextAlign::Left,
            url: None,
            children: Vec::new(),
        }
    }
}

// ── Main parse entry point ────────────────────────────────────────────────────

pub fn parse(xml: &str) -> Result<Document, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("XML parse error: {e}"))?;

    let root = doc.root_element();
    if root.tag_name().name() != "lpdf" {
        return Err("root element must be <lpdf>".into());
    }

    // ── Pass 1: collect <tokens> regardless of document order ────────────────
    let mut tokens = Tokens::default();
    for child in elems(&root) {
        if child.tag_name().name() == "tokens" {
            parse_tokens_elem(&child, &mut tokens)?;
        }
    }

    let mut meta = Meta::default();
    let mut pages: Vec<Page> = Vec::new();

    // Document-level defaults (overridable per page)
    let mut doc_size = (595.28_f32, 841.89_f32); // a4
    let mut doc_margin = [0.0_f32; 4];
    let mut doc_background: Option<String> = None;

    // ── Pass 2: parse <document> using the resolved tokens ───────────────────
    for child in elems(&root) {
        match child.tag_name().name() {
            "tokens" => { /* already handled in pass 1 */ }
            "document" => {
                if let Some(v) = child.attribute("size") {
                    doc_size = parse_page_size(v)?;
                }
                if let Some("landscape") = child.attribute("orientation") {
                    doc_size = (doc_size.1, doc_size.0);
                }
                if let Some(v) = child.attribute("margin") {
                    doc_margin = tokens.resolve_spacing(v)?;
                }
                if let Some(v) = child.attribute("background") {
                    doc_background = Some(tokens.resolve_color(v)?);
                }

                let mut found_pages_elem = false;
                for doc_child in elems(&child) {
                    match doc_child.tag_name().name() {
                        "meta" => meta = parse_meta(&doc_child),
                        "pages" => {
                            found_pages_elem = true;
                            for page_elem in elems(&doc_child) {
                                match page_elem.tag_name().name() {
                                    "page" => pages.push(parse_page(
                                        &page_elem,
                                        doc_size,
                                        doc_margin,
                                        doc_background.clone(),
                                        &tokens,
                                    )?),
                                    other => {
                                        return Err(format!(
                                            "unexpected element in <pages>: <{other}>"
                                        ))
                                    }
                                }
                            }
                            if pages.is_empty() {
                                return Err("<pages> must contain at least one <page>".into());
                            }
                        }
                        other => {
                            return Err(format!(
                                "unexpected element in <document>: <{other}>"
                            ))
                        }
                    }
                }
                if !found_pages_elem {
                    return Err("<document> is missing a <pages> element".into());
                }
            }
            other => return Err(format!("unexpected element in <lpdf>: <{other}>")),
        }
    }

    let mut doc = Document { meta, fonts: tokens.fonts, pages };
    resolve_font_tokens(&mut doc);
    Ok(doc)
}

// ── Tokens ────────────────────────────────────────────────────────────────────

fn parse_tokens_elem(elem: &roxmltree::Node, tokens: &mut Tokens) -> Result<(), String> {
    for child in elems(elem) {
        match child.tag_name().name() {
            "space" => tokens.space = parse_scale_row(&child)?,
            "border" => tokens.border = parse_scale_row(&child)?,
            "radius" => tokens.radius = parse_scale_row(&child)?,
            "width" => tokens.width = parse_scale_row(&child)?,
            "text" => tokens.text = parse_scale_row(&child)?,
            "grid" => tokens.grid_col = parse_scale_row(&child)?,
            "fonts" => {
                for font_elem in elems(&child) {
                    if font_elem.tag_name().name() == "font" {
                        let name = req_attr(&font_elem, "name")?;
                        let def = if let Some(b) = font_elem.attribute("builtin") {
                            FontDef::Builtin(b.to_string())
                        } else if let Some(s) = font_elem.attribute("src") {
                            if is_url(s) {
                                return Err(format!(
                                    "<font name=\"{name}\"> src must be a file path, not a URL"
                                ));
                            }
                            FontDef::Src(s.to_string())
                        } else {
                            return Err(format!(
                                "<font name=\"{name}\"> needs 'src' or 'builtin'"
                            ));
                        };
                        tokens.fonts.insert(name, def);
                    }
                }
            }
            "colors" => {
                for color_elem in elems(&child) {
                    if color_elem.tag_name().name() == "color" {
                        let name = req_attr(&color_elem, "name")?;
                        let value = req_attr(&color_elem, "value")?;
                        let hex = crate::tokens::normalize_hex(&value)?;
                        tokens.colors.insert(name, hex);
                    }
                }
            }
            other => return Err(format!("unknown element in <tokens>: <{other}>")),
        }
    }
    Ok(())
}

/// Parse xs/s/m/l/xl/xxl pt attributes from a token scale element.
fn parse_scale_row(elem: &roxmltree::Node) -> Result<[f32; 6], String> {
    let tag = elem.tag_name().name();
    let get = |attr: &str| -> Result<f32, String> {
        let v = elem
            .attribute(attr)
            .ok_or_else(|| format!("<{tag}> missing '{attr}' attribute"))?;
        parse_pt(v).ok_or_else(|| format!("<{tag}> '{attr}': invalid pt value '{v}'"))
    };
    Ok([get("xs")?, get("s")?, get("m")?, get("l")?, get("xl")?, get("xxl")?])
}

// ── Page / meta ───────────────────────────────────────────────────────────────

fn parse_meta(elem: &roxmltree::Node) -> Meta {
    Meta {
        title: opt_attr(elem, "title"),
        author: opt_attr(elem, "author"),
        subject: opt_attr(elem, "subject"),
        keywords: opt_attr(elem, "keywords"),
        creator: opt_attr(elem, "creator"),
    }
}

fn parse_page(
    elem: &roxmltree::Node,
    doc_size: (f32, f32),
    doc_margin: [f32; 4],
    doc_background: Option<String>,
    tokens: &Tokens,
) -> Result<Page, String> {
    let mut size = doc_size;
    let mut margin = doc_margin;
    let mut background = doc_background;

    if let Some(v) = elem.attribute("size") {
        size = parse_page_size(v)?;
    }
    if let Some("landscape") = elem.attribute("orientation") {
        size = (size.1, size.0);
    }
    if let Some(v) = elem.attribute("margin") {
        margin = tokens.resolve_spacing(v)?;
    }
    if let Some(v) = elem.attribute("background") {
        background = Some(tokens.resolve_color(v)?);
    }

    let mut children = Vec::new();
    for child in elems(elem) {
        children.push(parse_node(&child, tokens)?);
    }

    Ok(Page {
        width: size.0,
        height: size.1,
        margin,
        background,
        children,
    })
}

fn parse_page_size(val: &str) -> Result<(f32, f32), String> {
    match val {
        "a4" => Ok((595.28, 841.89)),
        "a3" => Ok((841.89, 1190.55)),
        "a5" => Ok((419.53, 595.28)),
        "letter" => Ok((612.0, 792.0)),
        "legal" => Ok((612.0, 1008.0)),
        custom => {
            let parts: Vec<&str> = custom.split_whitespace().collect();
            if parts.len() == 2 {
                let w = parse_pt(parts[0])
                    .ok_or_else(|| format!("invalid page width: '{}'", parts[0]))?;
                let h = parse_pt(parts[1])
                    .ok_or_else(|| format!("invalid page height: '{}'", parts[1]))?;
                Ok((w, h))
            } else {
                Err(format!("invalid page size: '{val}'"))
            }
        }
    }
}

// ── Node parsing ──────────────────────────────────────────────────────────────

fn parse_node(elem: &roxmltree::Node, tokens: &Tokens) -> Result<Node, String> {
    let tag = elem.tag_name().name();
    let kind = match tag {
        "stack" => NodeKind::Stack,
        "flank" => NodeKind::Flank,
        "split" => NodeKind::Split,
        "cluster" => NodeKind::Cluster,
        "grid" => NodeKind::Grid,
        "frame" => NodeKind::Frame,
        "divider" => NodeKind::Divider,
        "text" => NodeKind::Text,
        "link" => NodeKind::Link,
        other => return Err(format!("unknown layout element: <{other}>")),
    };

    let mut node = Node::default_for(kind.clone());

    // ── Shared box attrs ──────────────────────────────────────────────────────
    if let Some(v) = elem.attribute("gap") {
        node.gap = tokens.resolve_space(v)?;
    }
    if let Some(v) = elem.attribute("padding") {
        node.padding = tokens.resolve_spacing(v)?;
    }
    if let Some(v) = elem.attribute("background") {
        node.background = Some(tokens.resolve_color(v)?);
    }
    if let Some(v) = elem.attribute("border") {
        node.border = Some(tokens.resolve_border(v)?);
    }
    if let Some(v) = elem.attribute("radius") {
        node.radius = tokens.resolve_radius(v)?;
    }
    if let Some(v) = elem.attribute("height") {
        node.height_mode = match v {
            "full" => HeightMode::Full,
            "fill" => HeightMode::Fill,
            other => {
                if let Some(h) = parse_pt(other) {
                    HeightMode::Fixed(h)
                } else {
                    return Err(format!("invalid height value: '{other}'"));
                }
            }
        };
    }
    if let Some(v) = elem.attribute("width") {
        node.width_constraint = Some(tokens.resolve_width(v)?);
    }
    if let Some(v) = elem.attribute("repeat") {
        node.repeat = match v {
            "page"  => Repeat::Page,
            "first" => Repeat::First,
            other   => return Err(format!(
                "invalid repeat value: '{other}' (expected 'page' or 'first')"
            )),
        };
    }

    // ── Kind-specific attrs ───────────────────────────────────────────────────
    match kind {
        NodeKind::Stack => {
            node.align = parse_align(elem.attribute("align").unwrap_or("start"))?;
            node.justify = parse_justify(elem.attribute("justify").unwrap_or("start"))?;
        }
        NodeKind::Flank => {
            node.align = parse_align(elem.attribute("align").unwrap_or("start"))?;
            node.end = elem.attribute("end").map(|v| v == "true").unwrap_or(false);
        }
        NodeKind::Split => {
            node.align = parse_align(elem.attribute("align").unwrap_or("start"))?;
            node.equal = elem.attribute("equal").map(|v| v == "true").unwrap_or(false);
        }
        NodeKind::Cluster => {
            node.align = parse_align(elem.attribute("align").unwrap_or("start"))?;
            node.justify = parse_justify(elem.attribute("justify").unwrap_or("start"))?;
            node.wrap = elem.attribute("wrap").map(|v| v != "false").unwrap_or(true);
        }
        NodeKind::Frame => {
            node.align   = parse_align(elem.attribute("align").unwrap_or("center"))?;
            node.justify = parse_justify(elem.attribute("justify").unwrap_or("center"))?;
        }
        NodeKind::Grid => {
            node.cols = elem
                .attribute("cols")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            if let Some(v) = elem.attribute("col-width") {
                node.col_width = Some(tokens.resolve_grid_col(v)?);
            }
        }
        NodeKind::Divider => {
            node.direction = match elem.attribute("direction").unwrap_or("horizontal") {
                "vertical" => Direction::Vertical,
                _ => Direction::Horizontal,
            };
            node.color = if let Some(v) = elem.attribute("color") {
                Some(tokens.resolve_color(v)?)
            } else {
                Some("#000000".into())
            };
            node.thickness = if let Some(v) = elem.attribute("thickness") {
                tokens.resolve_border_thickness(v)?
            } else {
                1.0
            };
        }
        NodeKind::Text => {
            node.font = elem.attribute("font").unwrap_or("Helvetica").to_string();
            node.font_size = if let Some(v) = elem.attribute("size") {
                tokens.resolve_text_size(v)?
            } else {
                11.0
            };
            node.text_color = Some(if let Some(v) = elem.attribute("color") {
                tokens.resolve_color(v)?
            } else {
                tokens.resolve_color("text").unwrap_or_else(|_| "#1a1a1a".into())
            });
            node.text_align = match elem.attribute("align").unwrap_or("left") {
                "center" => TextAlign::Center,
                "right" => TextAlign::Right,
                _ => TextAlign::Left,
            };
            // Mixed content: text nodes become plain runs; <span> elements become styled runs.
            let mut prev_ended_space = false;
            for child in elem.children() {
                if child.is_text() {
                    let raw = child.text().unwrap_or("");
                    let leading = raw.starts_with(char::is_whitespace) || prev_ended_space;
                    let words: Vec<&str> = raw.split_whitespace().collect();
                    if !words.is_empty() {
                        node.text_runs.push(TextRun {
                            text: words.join(" "),
                            leading_space: leading,
                            font: None,
                            color: None,
                            href: None,
                            underline: false,
                            strike: false,
                        });
                    }
                    prev_ended_space = raw.ends_with(char::is_whitespace);
                } else if child.is_element() && child.tag_name().name() == "span" {
                    let raw_span: String = child
                        .children()
                        .filter(|n| n.is_text())
                        .filter_map(|n| n.text())
                        .collect();
                    let span_text = raw_span
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !span_text.is_empty() {
                        node.text_runs.push(TextRun {
                            text: span_text,
                            leading_space: prev_ended_space,
                            font: child.attribute("font").map(|s| s.to_string()),
                            color: if let Some(v) = child.attribute("color") {
                                Some(tokens.resolve_color(v)?)
                            } else {
                                None
                            },
                            href: child.attribute("href").map(|s| s.to_string()),
                            underline: child
                                .attribute("underline")
                                .map(|v| v == "true")
                                .unwrap_or(false),
                            strike: child
                                .attribute("strike")
                                .map(|v| v == "true")
                                .unwrap_or(false),
                        });
                    }
                    prev_ended_space = raw_span.ends_with(char::is_whitespace);
                }
            }
        }
        NodeKind::Link => {
            node.url = Some(
                elem.attribute("href")
                    .ok_or_else(|| "<link> requires an href attribute".to_string())?
                    .to_string(),
            );
        }
    }

    // ── Children (not for leaf nodes) ─────────────────────────────────────────
    if kind != NodeKind::Divider && kind != NodeKind::Text {
        for child in elems(elem) {
            node.children.push(parse_node(&child, tokens)?);
        }
    }

    Ok(node)
}

fn parse_align(val: &str) -> Result<Align, String> {
    match val {
        "start" => Ok(Align::Start),
        "center" => Ok(Align::Center),
        "end" => Ok(Align::End),
        "stretch" => Ok(Align::Stretch),
        other => Err(format!("invalid align value: '{other}'")),
    }
}

fn parse_justify(val: &str) -> Result<Justify, String> {
    match val {
        "start" => Ok(Justify::Start),
        "center" => Ok(Justify::Center),
        "end" => Ok(Justify::End),
        "between" => Ok(Justify::Between),
        other => Err(format!("invalid justify value: '{other}'")),
    }
}

// ── Asset path helpers ────────────────────────────────────────────────────────

/// Returns `true` if the string looks like a URL (starts with a known scheme).
/// Used to reject URLs where only file paths are accepted (fonts, images).
fn is_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("ftp://")
        || lower.starts_with("file://")
        || lower.starts_with("data:")
}

// ── XML helpers ───────────────────────────────────────────────────────────────

fn elems<'a>(
    node: &'a roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    node.children().filter(|n| n.is_element())
}

fn req_attr(elem: &roxmltree::Node, name: &str) -> Result<String, String> {
    elem.attribute(name)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            format!(
                "<{}> missing required attribute '{name}'",
                elem.tag_name().name()
            )
        })
}

fn opt_attr(elem: &roxmltree::Node, name: &str) -> String {
    elem.attribute(name).unwrap_or("").to_string()
}

// ── Tree (JSON) parser ────────────────────────────────────────────────────────

/// Inline helper: read a string attribute from a JSON node's "attrs" object.
fn jattr<'a>(json: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    json.get("attrs")?.get(key)?.as_str()
}

/// Parse a JSON document tree (produced by `LpdfKit`) into a `Document`.
pub fn parse_tree(json: &str) -> Result<Document, String> {
    let root: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("JSON parse error: {e}"))?;

    match root.get("version").and_then(|v| v.as_u64()) {
        Some(1) => {}
        Some(v) => return Err(format!("unsupported tree version: {v}")),
        None    => return Err("tree JSON missing 'version' field".into()),
    }
    if root.get("type").and_then(|v| v.as_str()) != Some("document") {
        return Err("tree root 'type' must be 'document'".into());
    }

    let empty_map = serde_json::Map::new();
    let attrs = root.get("attrs").and_then(|v| v.as_object()).unwrap_or(&empty_map);

    // ── Tokens ────────────────────────────────────────────────────────────────
    let mut tokens = Tokens::default();
    if let Some(tok) = attrs.get("tokens").and_then(|v| v.as_object()) {
        parse_tree_tokens(tok, &mut tokens)?;
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
    let doc_size_str  = attrs.get("size")       .and_then(|v| v.as_str());
    let doc_orient    = attrs.get("orientation").and_then(|v| v.as_str());
    let doc_margin_s  = attrs.get("margin")     .and_then(|v| v.as_str());
    let doc_bg_s      = attrs.get("background") .and_then(|v| v.as_str());

    let mut doc_size = if let Some(s) = doc_size_str {
        parse_page_size(s)?
    } else {
        (595.28_f32, 841.89_f32) // a4
    };
    if doc_orient == Some("landscape") {
        doc_size = (doc_size.1, doc_size.0);
    }
    let doc_margin = if let Some(s) = doc_margin_s {
        tokens.resolve_spacing(s)?
    } else {
        [0.0_f32; 4]
    };
    let doc_background = if let Some(s) = doc_bg_s {
        Some(tokens.resolve_color(s)?)
    } else {
        None
    };

    // ── Pages ─────────────────────────────────────────────────────────────────
    let page_arr = root.get("children").and_then(|v| v.as_array())
        .ok_or("tree JSON 'children' must be an array")?;

    let mut pages: Vec<Page> = Vec::new();
    for child in page_arr {
        if child.get("type").and_then(|v| v.as_str()) != Some("page") {
            return Err("document children must all be page nodes".into());
        }
        pages.push(parse_tree_page(child, doc_size, doc_margin, doc_background.clone(), &tokens)?);
    }
    if pages.is_empty() {
        return Err("document must have at least one page".into());
    }

    let mut doc = Document { meta, fonts: tokens.fonts, pages };
    resolve_font_tokens(&mut doc);
    Ok(doc)
}

fn parse_tree_tokens(
    obj: &serde_json::Map<String, serde_json::Value>,
    tokens: &mut Tokens,
) -> Result<(), String> {
    use crate::tokens::scale_idx;

    // Helper: overlay a [f32; 6] array from a JSON string map.
    let apply_scale = |json_key: &str, scale: &mut [f32; 6]| -> Result<(), String> {
        if let Some(map) = obj.get(json_key).and_then(|v| v.as_object()) {
            for (k, v) in map {
                if let (Some(idx), Some(s)) = (scale_idx(k), v.as_str()) {
                    scale[idx] = parse_pt(s)
                        .ok_or_else(|| format!("invalid {json_key} token '{k}': '{s}'"))?;
                }
            }
        }
        Ok(())
    };

    apply_scale("space",  &mut tokens.space)?;
    apply_scale("border", &mut tokens.border)?;
    apply_scale("radius", &mut tokens.radius)?;
    apply_scale("width",  &mut tokens.width)?;
    apply_scale("grid",   &mut tokens.grid_col)?;
    apply_scale("text",   &mut tokens.text)?;

    if let Some(colors) = obj.get("colors").and_then(|v| v.as_object()) {
        for (k, v) in colors {
            if let Some(s) = v.as_str() {
                tokens.colors.insert(k.clone(), crate::tokens::normalize_hex(s)?);
            }
        }
    }

    if let Some(fonts) = obj.get("fonts").and_then(|v| v.as_object()) {
        for (name, def) in fonts {
            let font_def = if let Some(b) = def.get("builtin").and_then(|v| v.as_str()) {
                FontDef::Builtin(b.to_string())
            } else if let Some(s) = def.get("src").and_then(|v| v.as_str()) {
                if is_url(s) {
                    return Err(format!(
                        "font '{name}' src must be a file path, not a URL"
                    ));
                }
                FontDef::Src(s.to_string())
            } else {
                return Err(format!("font '{name}' needs 'src' or 'builtin'"));
            };
            tokens.fonts.insert(name.clone(), font_def);
        }
    }

    Ok(())
}

fn parse_tree_page(
    json: &serde_json::Value,
    doc_size: (f32, f32),
    doc_margin: [f32; 4],
    doc_background: Option<String>,
    tokens: &Tokens,
) -> Result<Page, String> {
    let mut size = doc_size;
    let mut margin = doc_margin;
    let mut background = doc_background;

    if let Some(s) = jattr(json, "size") {
        size = parse_page_size(s)?;
    }
    if jattr(json, "orientation") == Some("landscape") {
        size = (size.1, size.0);
    }
    if let Some(s) = jattr(json, "margin") {
        margin = tokens.resolve_spacing(s)?;
    }
    if let Some(s) = jattr(json, "background") {
        background = Some(tokens.resolve_color(s)?);
    }

    let children = if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
        arr.iter().map(|c| parse_tree_node(c, tokens)).collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };

    Ok(Page { width: size.0, height: size.1, margin, background, children })
}

fn parse_tree_node(json: &serde_json::Value, tokens: &Tokens) -> Result<Node, String> {
    let type_str = json.get("type").and_then(|v| v.as_str())
        .ok_or_else(|| "node missing 'type' field".to_string())?;

    let kind = match type_str {
        "stack"   => NodeKind::Stack,
        "flank"   => NodeKind::Flank,
        "split"   => NodeKind::Split,
        "cluster" => NodeKind::Cluster,
        "grid"    => NodeKind::Grid,
        "frame"   => NodeKind::Frame,
        "divider" => NodeKind::Divider,
        "text"    => NodeKind::Text,
        "link"    => NodeKind::Link,
        other => return Err(format!("unknown node type: '{other}'")),
    };

    let mut node = Node::default_for(kind.clone());

    // ── Shared box attrs ──────────────────────────────────────────────────────
    if let Some(v) = jattr(json, "gap") {
        node.gap = tokens.resolve_space(v)?;
    }
    if let Some(v) = jattr(json, "padding") {
        node.padding = tokens.resolve_spacing(v)?;
    }
    if let Some(v) = jattr(json, "background") {
        node.background = Some(tokens.resolve_color(v)?);
    }
    if let Some(v) = jattr(json, "border") {
        node.border = Some(tokens.resolve_border(v)?);
    }
    if let Some(v) = jattr(json, "radius") {
        node.radius = tokens.resolve_radius(v)?;
    }
    if let Some(v) = jattr(json, "height") {
        node.height_mode = match v {
            "full" => HeightMode::Full,
            "fill" => HeightMode::Fill,
            other  => {
                parse_pt(other)
                    .map(HeightMode::Fixed)
                    .ok_or_else(|| format!("invalid height value: '{other}'"))?
            }
        };
    }
    if let Some(v) = jattr(json, "width") {
        node.width_constraint = Some(tokens.resolve_width(v)?);
    }
    if let Some(v) = jattr(json, "repeat") {
        node.repeat = match v {
            "page"  => Repeat::Page,
            "first" => Repeat::First,
            other   => return Err(format!("invalid repeat value: '{other}'")),
        };
    }

    // ── Kind-specific attrs ───────────────────────────────────────────────────
    match kind {
        NodeKind::Stack => {
            node.align   = parse_align(jattr(json, "align").unwrap_or("start"))?;
            node.justify = parse_justify(jattr(json, "justify").unwrap_or("start"))?;
        }
        NodeKind::Flank => {
            node.align   = parse_align(jattr(json, "align").unwrap_or("start"))?;
            node.justify = parse_justify(jattr(json, "justify").unwrap_or("start"))?;
            node.end     = jattr(json, "end").map(|v| v == "true").unwrap_or(false);
        }
        NodeKind::Split => {
            node.align = parse_align(jattr(json, "align").unwrap_or("start"))?;
            node.equal = jattr(json, "equal").map(|v| v == "true").unwrap_or(false);
        }
        NodeKind::Cluster => {
            node.align   = parse_align(jattr(json, "align").unwrap_or("start"))?;
            node.justify = parse_justify(jattr(json, "justify").unwrap_or("start"))?;
            node.wrap    = jattr(json, "wrap").map(|v| v != "false").unwrap_or(true);
        }
        NodeKind::Frame => {
            node.align   = parse_align(jattr(json, "align").unwrap_or("center"))?;
            node.justify = parse_justify(jattr(json, "justify").unwrap_or("center"))?;
        }
        NodeKind::Grid => {
            node.cols = jattr(json, "cols").and_then(|v| v.parse().ok()).unwrap_or(1);
            if let Some(v) = jattr(json, "col-width") {
                node.col_width = Some(tokens.resolve_grid_col(v)?);
            }
        }
        NodeKind::Divider => {
            node.direction = match jattr(json, "direction").unwrap_or("horizontal") {
                "vertical" => Direction::Vertical,
                _          => Direction::Horizontal,
            };
            node.color = Some(if let Some(v) = jattr(json, "color") {
                tokens.resolve_color(v)?
            } else {
                "#000000".into()
            });
            node.thickness = if let Some(v) = jattr(json, "thickness") {
                tokens.resolve_border_thickness(v)?
            } else {
                1.0
            };
        }
        NodeKind::Text => {
            node.font = jattr(json, "font").unwrap_or("Helvetica").to_string();
            // tree uses "font-size"; XML uses "size" — try both
            node.font_size = match jattr(json, "font-size").or_else(|| jattr(json, "size")) {
                Some(v) => tokens.resolve_text_size(v)?,
                None    => 11.0,
            };
            node.text_color = Some(match jattr(json, "color") {
                Some(v) => tokens.resolve_color(v)?,
                None    => tokens.resolve_color("text").unwrap_or_else(|_| "#1a1a1a".into()),
            });
            // tree uses "text-align"; XML uses "align" — try both
            node.text_align = match jattr(json, "text-align").or_else(|| jattr(json, "align")).unwrap_or("left") {
                "center" => TextAlign::Center,
                "right"  => TextAlign::Right,
                _        => TextAlign::Left,
            };

            // Children: array of string | span node
            if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
                for (i, child) in arr.iter().enumerate() {
                    let leading = i > 0;
                    if let Some(s) = child.as_str() {
                        let words: Vec<&str> = s.split_whitespace().collect();
                        if !words.is_empty() {
                            node.text_runs.push(TextRun {
                                text: words.join(" "),
                                leading_space: leading,
                                font: None, color: None, href: None,
                                underline: false, strike: false,
                            });
                        }
                    } else if child.get("type").and_then(|v| v.as_str()) == Some("span") {
                        let span_text: String = child
                            .get("children").and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                   .filter_map(|v| v.as_str())
                                   .collect::<Vec<_>>()
                                   .join(" ")
                            })
                            .unwrap_or_default()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        if !span_text.is_empty() {
                            let color = match jattr(child, "color") {
                                Some(v) => Some(tokens.resolve_color(v)?),
                                None    => None,
                            };
                            // tree uses "url"; XML uses "href"
                            let href = jattr(child, "url")
                                .or_else(|| jattr(child, "href"))
                                .map(str::to_string);
                            node.text_runs.push(TextRun {
                                text: span_text,
                                leading_space: leading,
                                font:      jattr(child, "font").map(str::to_string),
                                color,
                                href,
                                underline: jattr(child, "underline").map(|v| v == "true").unwrap_or(false),
                                strike:    jattr(child, "strike")   .map(|v| v == "true").unwrap_or(false),
                            });
                        }
                    }
                }
            }
            return Ok(node); // text nodes have no layout children
        }
        NodeKind::Link => {
            node.url = Some(
                jattr(json, "url")
                    .ok_or("<link> node requires a 'url' attribute")?
                    .to_string(),
            );
        }
    }

    // ── Layout children ───────────────────────────────────────────────────────
    if kind != NodeKind::Divider {
        if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
            for child in arr {
                node.children.push(parse_tree_node(child, tokens)?);
            }
        }
    }

    Ok(node)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal(body: &str) -> String {
        format!(
            r#"<lpdf version="1"><document size="a4" margin="28pt"><pages><page>{body}</page></pages></document></lpdf>"#
        )
    }

    #[test]
    fn parse_empty_page() {
        let doc = parse(&minimal("")).unwrap();
        assert_eq!(doc.pages.len(), 1);
        assert_eq!(doc.pages[0].width, 595.28);
        assert_eq!(doc.pages[0].height, 841.89);
        assert_eq!(doc.pages[0].margin, [28.0; 4]);
    }

    #[test]
    fn parse_frame_with_background() {
        let doc = parse(&minimal(r#"<frame background="primary" />"#)).unwrap();
        let node = &doc.pages[0].children[0];
        assert_eq!(node.background.as_deref(), Some("#1763cf"));
    }

    #[test]
    fn parse_stack_gap_and_children() {
        let doc = parse(&minimal(
            r#"<stack gap="m"><frame /><frame /></stack>"#,
        ))
        .unwrap();
        let stack = &doc.pages[0].children[0];
        assert_eq!(stack.gap, 8.0);
        assert_eq!(stack.children.len(), 2);
    }

    #[test]
    fn parse_divider() {
        let doc = parse(&minimal(r##"<divider color="#e0e0e0" thickness="xs" />"##)).unwrap();
        let d = &doc.pages[0].children[0];
        assert_eq!(d.kind, NodeKind::Divider);
        assert_eq!(d.thickness, 0.5);
        assert_eq!(d.color.as_deref(), Some("#e0e0e0"));
    }

    #[test]
    fn parse_grid_cols() {
        let doc = parse(&minimal(r#"<grid cols="3" gap="m" />"#)).unwrap();
        let g = &doc.pages[0].children[0];
        assert_eq!(g.cols, 3);
    }

    #[test]
    fn parse_text_node() {
        let doc = parse(&minimal(r#"<text size="m" color="text">Hello world</text>"#)).unwrap();
        let t = &doc.pages[0].children[0];
        assert_eq!(t.kind, NodeKind::Text);
        assert_eq!(t.font_size, 11.0);
        assert_eq!(t.text_runs.len(), 1);
        assert_eq!(t.text_runs[0].text, "Hello world");
    }

    #[test]
    fn invalid_root_element() {
        let result = parse("<doc/>");
        assert!(result.is_err());
    }

    #[test]
    fn unknown_element_errors() {
        let result = parse(&minimal("<unknown />"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown layout element"));
    }

    #[test]
    fn custom_tokens_override_defaults() {
        let xml = r##"<lpdf version="1">
            <tokens>
                <colors>
                    <color name="primary" value="#ff0000" />
                </colors>
            </tokens>
            <document size="a4"><pages><page><frame background="primary" /></page></pages></document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        assert_eq!(
            doc.pages[0].children[0].background.as_deref(),
            Some("#ff0000")
        );
    }

    #[test]
    fn landscape_swaps_dimensions() {
        let xml = r#"<lpdf version="1"><document size="a4" orientation="landscape"><pages><page /></pages></document></lpdf>"#;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.pages[0].width, 841.89);
        assert_eq!(doc.pages[0].height, 595.28);
    }

    #[test]
    fn height_fixed_pt() {
        let doc = parse(&minimal(r#"<frame height="28pt" />"#)).unwrap();
        assert_eq!(doc.pages[0].children[0].height_mode, HeightMode::Fixed(28.0));
    }

    #[test]
    fn height_fill_mode() {
        let doc = parse(&minimal(r#"<frame height="fill" />"#)).unwrap();
        assert_eq!(doc.pages[0].children[0].height_mode, HeightMode::Fill);
    }
}
