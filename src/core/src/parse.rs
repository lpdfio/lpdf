use std::collections::{HashMap, HashSet};
use crate::tokens::{parse_pt, FontDef, FontWidths, Tokens};

// ── Resolved document model (layout / render operate on these) ────────────────

#[derive(Debug, Clone)]
pub struct Document {
    pub meta:        Meta,
    pub fonts:       HashMap<String, FontDef>,
    pub images:      HashSet<String>,
    pub pages:       Vec<Page>,
    /// Caller-supplied glyph width tables for custom fonts (tree path or
    /// injected via `set_font_metrics` before the WASM call).
    pub font_widths: HashMap<String, FontWidths>,
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
    pub debug: bool,
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
    pub debug: bool,
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
    // img (NodeKind::Img only)
    pub image_name: Option<String>,
    pub img_height_constraint: Option<f32>,
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
    Img,
}

/// How a node determines its height.
#[derive(Debug, Clone, PartialEq)]
pub enum HeightMode {
    /// Size to content.
    Auto,
    /// Fills parent's full available height (`height="full"`).
    Full,
    /// Takes remaining space after siblings are sized (`height="fill"`).
    Fill,
    /// Explicit pt value (`height="28pt"`).
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

// ── Pre-trickle-down (parsed) model ──────────────────────────────────────────

/// A node as produced by the XML/JSON parser, before font inheritance is
/// resolved.  `font` and `font_size` may be `None` (meaning "inherit from
/// parent").  The `resolve_doc` pass converts this into a `Document` of
/// concrete `Node` values.
struct ParsedNode {
    kind: NodeKind,
    gap: f32,
    padding: [f32; 4],
    background: Option<String>,
    border: Option<(f32, String)>,
    radius: f32,
    height_mode: HeightMode,
    width_constraint: Option<f32>,
    repeat: Repeat,
    debug: bool,
    align: Align,
    justify: Justify,
    end: bool,
    equal: bool,
    wrap: bool,
    cols: u32,
    col_width: Option<f32>,
    direction: Direction,
    color: Option<String>,
    thickness: f32,
    text_runs: Vec<TextRun>,
    font:      Option<String>,  // None = inherit
    font_size: Option<f32>,     // None = inherit
    text_color: Option<String>,
    text_align: TextAlign,
    url: Option<String>,
    image_name: Option<String>,
    img_height_constraint: Option<f32>,
    children: Vec<ParsedNode>,
}

struct ParsedPage {
    width: f32,
    height: f32,
    margin: [f32; 4],
    background: Option<String>,
    debug: bool,
    children: Vec<ParsedNode>,
}

struct ParsedDocument {
    meta:        Meta,
    fonts:       HashMap<String, FontDef>,
    font_widths: HashMap<String, FontWidths>,
    images:      HashSet<String>,
    pages:       Vec<ParsedPage>,
    doc_font:    Option<String>, // from <document font="...">
}

impl ParsedNode {
    fn default_for(kind: NodeKind) -> Self {
        ParsedNode {
            kind,
            gap: 0.0,
            padding: [0.0; 4],
            background: None,
            border: None,
            radius: 0.0,
            height_mode: HeightMode::Auto,
            width_constraint: None,
            repeat: Repeat::None,
            debug: false,
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
            font: None,
            font_size: None,
            text_color: None,
            text_align: TextAlign::Left,
            url: None,
            image_name: None,
            img_height_constraint: None,
            children: Vec::new(),
        }
    }
}

// ── Trickle-down (font inheritance) pass ─────────────────────────────────────

/// Resolve a font alias (e.g. `"body"`) through the assets font map to a
/// concrete name:
///
/// - `Core("Helvetica-Bold")` → `"Helvetica-Bold"`
/// - `Ref("montserrat")`      → `"montserrat"` (the registry key)
/// - Unknown name             → the name itself (may be a bare builtin like
///   `"Helvetica"` used directly without an assets declaration)
fn resolve_font_alias(name: &str, fonts: &HashMap<String, FontDef>) -> String {
    match fonts.get(name) {
        Some(FontDef::Core(builtin)) => builtin.clone(),
        Some(FontDef::Ref(key))      => key.clone(),
        // Unknown name: pass through. May be a direct builtin name (e.g.
        // "Helvetica-Bold") used without an alias declaration, or an undefined
        // alias. The layout engine handles both via its own fallback logic.
        None                         => name.to_string(),
    }
}

/// Convert a `ParsedDocument` into a resolved `Document` by propagating
/// `font` and `font_size` values down the node tree.
fn resolve_doc(parsed: ParsedDocument) -> Document {
    let root_font = parsed.doc_font
        .as_deref()
        .map(|f| resolve_font_alias(f, &parsed.fonts))
        .unwrap_or_else(|| "Helvetica".to_string());
    let root_size = 11.0_f32;

    // Build resolved_fonts keyed by the concrete name (not the alias).
    let mut resolved_fonts: HashMap<String, FontDef> = HashMap::new();
    for (_alias, def) in &parsed.fonts {
        let key = match def {
            FontDef::Core(name) => name.clone(),
            FontDef::Ref(k)     => k.clone(),
        };
        resolved_fonts.insert(key, def.clone());
    }

    let pages = parsed.pages.into_iter().map(|page| {
        let children = page.children
            .into_iter()
            .map(|n| resolve_node(n, &root_font, root_size, &parsed.fonts))
            .collect();
        Page {
            width:      page.width,
            height:     page.height,
            margin:     page.margin,
            background: page.background,
            debug:      page.debug,
            children,
        }
    }).collect();

    Document {
        meta:        parsed.meta,
        fonts:       resolved_fonts,
        font_widths: parsed.font_widths,
        images:      parsed.images,
        pages,
    }
}

fn resolve_node(
    n:            ParsedNode,
    current_font: &str,
    current_size: f32,
    fonts:        &HashMap<String, FontDef>,
) -> Node {
    let font_raw  = n.font.as_deref().unwrap_or(current_font);
    let font      = resolve_font_alias(font_raw, fonts);
    let font_size = n.font_size.unwrap_or(current_size);

    // Resolve span-level font aliases.
    let text_runs = n.text_runs.into_iter().map(|run| {
        if let Some(ref alias) = run.font {
            let resolved = resolve_font_alias(alias, fonts);
            TextRun { font: Some(resolved), ..run }
        } else {
            run
        }
    }).collect();

    let children = n.children
        .into_iter()
        .map(|c| resolve_node(c, &font, font_size, fonts))
        .collect();

    Node {
        kind:                  n.kind,
        gap:                   n.gap,
        padding:               n.padding,
        background:            n.background,
        border:                n.border,
        radius:                n.radius,
        height_mode:           n.height_mode,
        width_constraint:      n.width_constraint,
        repeat:                n.repeat,
        debug:                 n.debug,
        align:                 n.align,
        justify:               n.justify,
        end:                   n.end,
        equal:                 n.equal,
        wrap:                  n.wrap,
        cols:                  n.cols,
        col_width:             n.col_width,
        direction:             n.direction,
        color:                 n.color,
        thickness:             n.thickness,
        text_runs,
        font,
        font_size,
        text_color:            n.text_color,
        text_align:            n.text_align,
        url:                   n.url,
        image_name:            n.image_name,
        img_height_constraint: n.img_height_constraint,
        children,
    }
}


// ── Main parse entry point (XML) ─────────────────────────────────────────────

pub fn parse(xml: &str) -> Result<Document, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("XML parse error: {e}"))?;

    let root = doc.root_element();
    if root.tag_name().name() != "lpdf" {
        return Err("root element must be <lpdf>".into());
    }

    // ── Pass 0: collect <assets> ─────────────────────────────────────────────
    let mut asset_fonts:  HashMap<String, FontDef> = HashMap::new();
    let mut asset_images: HashSet<String>           = HashSet::new();
    for child in elems(&root) {
        if child.tag_name().name() == "assets" {
            parse_assets_elem(&child, &mut asset_fonts, &mut asset_images)?;
        }
    }

    // ── Pass 1: collect <tokens> ─────────────────────────────────────────────
    let mut tokens = Tokens::default();
    for child in elems(&root) {
        if child.tag_name().name() == "tokens" {
            parse_tokens_elem(&child, &mut tokens)?;
        }
    }

    let mut meta          = Meta::default();
    let mut pages: Vec<ParsedPage> = Vec::new();
    let mut doc_size      = (595.28_f32, 841.89_f32); // a4
    let mut doc_margin    = [0.0_f32; 4];
    let mut doc_background: Option<String> = None;
    let mut doc_debug     = false;
    let mut doc_font: Option<String> = None;

    // ── Pass 2: parse <document> ─────────────────────────────────────────────
    for child in elems(&root) {
        match child.tag_name().name() {
            "assets" | "tokens" => { /* already handled */ }
            "document" => {
                if child.attribute("font-size").is_some() {
                    return Err("<document> does not allow font-size".into());
                }
                if let Some(v) = child.attribute("font") {
                    doc_font = Some(v.to_string());
                }
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
                if let Some(v) = child.attribute("debug") {
                    doc_debug = v == "true";
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
                                        doc_debug,
                                        &tokens,
                                        &asset_images,
                                    )?),
                                    other => return Err(format!(
                                        "unexpected element in <pages>: <{other}>"
                                    )),
                                }
                            }
                            if pages.is_empty() {
                                return Err("<pages> must contain at least one <page>".into());
                            }
                        }
                        other => return Err(format!(
                            "unexpected element in <document>: <{other}>"
                        )),
                    }
                }
                if !found_pages_elem {
                    return Err("<document> is missing a <pages> element".into());
                }
            }
            other => return Err(format!("unexpected element in <lpdf>: <{other}>")),
        }
    }

    let parsed = ParsedDocument {
        meta,
        fonts:       asset_fonts,
        font_widths: HashMap::new(),
        images:      asset_images,
        pages,
        doc_font,
    };
    Ok(resolve_doc(parsed))
}

// ── Assets ────────────────────────────────────────────────────────────────────

fn validate_asset_name(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.chars().next().map_or(false, |c| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(format!(
            "asset name '{name}' is invalid: use only lowercase letters, digits, and '-', starting with a letter"
        ));
    }
    Ok(())
}

fn parse_assets_elem(
    elem:         &roxmltree::Node,
    fonts:        &mut HashMap<String, FontDef>,
    images:       &mut HashSet<String>,
) -> Result<(), String> {
    for child in elems(elem) {
        match child.tag_name().name() {
            "fonts" => {
                for font_elem in elems(&child) {
                    if font_elem.tag_name().name() == "font" {
                        let name = req_attr(&font_elem, "name")?;
                        validate_asset_name(&name)?;
                        let def = if let Some(b) = font_elem.attribute("core") {
                            FontDef::Core(b.to_string())
                        } else if let Some(r) = font_elem.attribute("ref") {
                            if is_url(r) {
                                return Err(format!(
                                    "<font name=\"{name}\"> ref must be a registry key, not a URL"
                                ));
                            }
                            FontDef::Ref(r.to_string())
                        } else {
                            return Err(format!(
                                "<font name=\"{name}\"> needs 'core' or 'ref'"
                            ));
                        };
                        fonts.insert(name, def);
                    }
                }
            }
            "images" => {
                for img_elem in elems(&child) {
                    if img_elem.tag_name().name() == "image" {
                        let name = req_attr(&img_elem, "name")?;
                        validate_asset_name(&name)?;
                        images.insert(name);
                    }
                }
            }
            other => return Err(format!("unknown element in <assets>: <{other}>")),
        }
    }
    Ok(())
}

// ── Tokens ────────────────────────────────────────────────────────────────────

fn parse_tokens_elem(elem: &roxmltree::Node, tokens: &mut Tokens) -> Result<(), String> {
    for child in elems(elem) {
        match child.tag_name().name() {
            "space"  => tokens.space    = parse_scale_row(&child)?,
            "border" => tokens.border   = parse_scale_row(&child)?,
            "radius" => tokens.radius   = parse_scale_row(&child)?,
            "width"  => tokens.width    = parse_scale_row(&child)?,
            "text"   => tokens.text     = parse_scale_row(&child)?,
            "grid"   => tokens.grid_col = parse_scale_row(&child)?,
            "colors" => {
                for color_elem in elems(&child) {
                    if color_elem.tag_name().name() == "color" {
                        let name  = req_attr(&color_elem, "name")?;
                        let value = req_attr(&color_elem, "value")?;
                        let hex   = crate::tokens::normalize_hex(&value)?;
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

/// Apply size/orientation/margin/background/debug overrides from an attribute
/// source onto the running page-defaults.  Works for both XML `<page>` elements
/// and JSON page nodes via the `Attrs` trait.  Returns the resolved `debug` flag.
fn apply_page_overrides(
    size:       &mut (f32, f32),
    margin:     &mut [f32; 4],
    background: &mut Option<String>,
    a:          &impl Attrs,
    tokens:     &Tokens,
    doc_debug:  bool,
) -> Result<bool, String> {
    if let Some(v) = a.get("size") {
        *size = parse_page_size(v)?;
    }
    if a.get("orientation") == Some("landscape") {
        *size = (size.1, size.0);
    }
    if let Some(v) = a.get("margin") {
        *margin = tokens.resolve_spacing(v)?;
    }
    if let Some(v) = a.get("background") {
        *background = Some(tokens.resolve_color(v)?);
    }
    Ok(a.get("debug").map(|v| v == "true").unwrap_or(doc_debug))
}

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
    elem:           &roxmltree::Node,
    doc_size:       (f32, f32),
    doc_margin:     [f32; 4],
    doc_background: Option<String>,
    doc_debug:      bool,
    tokens:         &Tokens,
    asset_images:   &HashSet<String>,
) -> Result<ParsedPage, String> {
    if elem.attribute("font-size").is_some() {
        return Err("<page> does not allow font-size".into());
    }
    let mut size       = doc_size;
    let mut margin     = doc_margin;
    let mut background = doc_background;
    let debug = apply_page_overrides(&mut size, &mut margin, &mut background, elem, tokens, doc_debug)?;

    let mut children = Vec::new();
    for child in elems(elem) {
        children.push(parse_node(&child, tokens, asset_images)?);
    }

    Ok(ParsedPage { width: size.0, height: size.1, margin, background, debug, children })
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

fn parse_node(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashSet<String>,
) -> Result<ParsedNode, String> {
    let tag = elem.tag_name().name();
    let kind = match tag {
        "stack"   => NodeKind::Stack,
        "flank"   => NodeKind::Flank,
        "split"   => NodeKind::Split,
        "cluster" => NodeKind::Cluster,
        "grid"    => NodeKind::Grid,
        "frame"   => NodeKind::Frame,
        "divider" => NodeKind::Divider,
        "text"    => NodeKind::Text,
        "link"    => NodeKind::Link,
        "img"     => NodeKind::Img,
        other     => return Err(format!("unknown layout element: <{other}>")),
    };

    let mut node = ParsedNode::default_for(kind.clone());
    apply_box_attrs(&mut node, elem, tokens)?;
    apply_layout_kind_attrs(&mut node, elem, tokens)?;

    match kind {
        NodeKind::Text => {
            node.text_color = Some(if let Some(v) = elem.attribute("color") {
                tokens.resolve_color(v)?
            } else {
                tokens.resolve_color("text").unwrap_or_else(|_| "#1a1a1a".into())
            });
            node.text_align = match elem.attribute("align").unwrap_or("left") {
                "center" => TextAlign::Center,
                "right"  => TextAlign::Right,
                _        => TextAlign::Left,
            };
            // Mixed content: text nodes → plain runs; <span> elements → styled runs.
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
                            font: None, color: None, href: None,
                            underline: false, strike: false,
                        });
                    }
                    prev_ended_space = raw.ends_with(char::is_whitespace);
                } else if child.is_element() && child.tag_name().name() == "span" {
                    let raw_span: String = child
                        .children()
                        .filter(|n| n.is_text())
                        .filter_map(|n| n.text())
                        .collect();
                    let span_text = raw_span.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !span_text.is_empty() {
                        node.text_runs.push(TextRun {
                            text: span_text,
                            leading_space: prev_ended_space,
                            font:  child.attribute("font").map(|s| s.to_string()),
                            color: if let Some(v) = child.attribute("color") {
                                Some(tokens.resolve_color(v)?)
                            } else {
                                None
                            },
                            href:      child.attribute("href").map(|s| s.to_string()),
                            underline: child.attribute("underline").map(|v| v == "true").unwrap_or(false),
                            strike:    child.attribute("strike").map(|v| v == "true").unwrap_or(false),
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
        NodeKind::Img => {
            apply_img_attrs(&mut node, elem, asset_images, "<assets><images>")?;
        }
        _ => {}
    }

    // ── Children (not for leaf nodes) ────────────────────────────────────────
    if !matches!(kind, NodeKind::Divider | NodeKind::Text | NodeKind::Img) {
        for child in elems(elem) {
            node.children.push(parse_node(&child, tokens, asset_images)?);
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

// ── Shared node construction helpers ─────────────────────────────────────────

/// Read a string attribute from a JSON node's "attrs" object.
fn jattr<'a>(json: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    json.get("attrs")?.get(key)?.as_str()
}

/// Uniform attribute lookup over both XML element and JSON object nodes,
/// so shared node-construction helpers can work with either input format.
trait Attrs {
    fn get(&self, key: &str) -> Option<&str>;
}

impl<'a, 'b> Attrs for roxmltree::Node<'a, 'b> {
    fn get(&self, key: &str) -> Option<&str> { self.attribute(key) }
}

struct JsonAttrs<'a>(&'a serde_json::Value);

impl<'a> Attrs for JsonAttrs<'a> {
    fn get(&self, key: &str) -> Option<&str> { jattr(self.0, key) }
}

fn parse_height_mode(v: &str) -> Result<HeightMode, String> {
    match v {
        "full" => Ok(HeightMode::Full),
        "fill" => Ok(HeightMode::Fill),
        other  => parse_pt(other)
            .map(HeightMode::Fixed)
            .ok_or_else(|| format!("invalid height value: '{other}'")),
    }
}

fn parse_repeat_attr(v: &str) -> Result<Repeat, String> {
    match v {
        "page"  => Ok(Repeat::Page),
        "first" => Ok(Repeat::First),
        other   => Err(format!(
            "invalid repeat value: '{other}' (expected 'page' or 'first')"
        )),
    }
}

/// Apply shared box attributes (font, spacing, visual, sizing) to a parsed node.
/// Works with both XML elements and JSON attr objects via the `Attrs` trait.
fn apply_box_attrs(
    node:   &mut ParsedNode,
    a:      &impl Attrs,
    tokens: &Tokens,
) -> Result<(), String> {
    if let Some(v) = a.get("font")       { node.font      = Some(v.to_string()); }
    if let Some(v) = a.get("font-size")  { node.font_size = Some(tokens.resolve_text_size(v)?); }
    if let Some(v) = a.get("gap")        { node.gap       = tokens.resolve_space(v)?; }
    if let Some(v) = a.get("padding")    { node.padding   = tokens.resolve_spacing(v)?; }
    if let Some(v) = a.get("background") { node.background = Some(tokens.resolve_color(v)?); }
    if let Some(v) = a.get("border")     { node.border    = Some(tokens.resolve_border(v)?); }
    if let Some(v) = a.get("radius")     { node.radius    = tokens.resolve_radius(v)?; }
    if node.kind != NodeKind::Img {
        if let Some(v) = a.get("height") { node.height_mode = parse_height_mode(v)?; }
    }
    if let Some(v) = a.get("width")  { node.width_constraint = Some(tokens.resolve_width(v)?); }
    if let Some(v) = a.get("repeat") { node.repeat = parse_repeat_attr(v)?; }
    node.debug = a.get("debug").map(|v| v == "true").unwrap_or(false);
    Ok(())
}

/// Apply kind-specific layout attributes shared between the XML and JSON parsers.
/// Text, Link, and Img are omitted — their content is format-specific.
fn apply_layout_kind_attrs(
    node:   &mut ParsedNode,
    a:      &impl Attrs,
    tokens: &Tokens,
) -> Result<(), String> {
    match node.kind {
        NodeKind::Stack => {
            node.align   = parse_align(a.get("align").unwrap_or("start"))?;
            node.justify = parse_justify(a.get("justify").unwrap_or("start"))?;
        }
        NodeKind::Flank => {
            node.align   = parse_align(a.get("align").unwrap_or("start"))?;
            node.justify = parse_justify(a.get("justify").unwrap_or("start"))?;
            node.end     = a.get("end").map(|v| v == "true").unwrap_or(false);
        }
        NodeKind::Split => {
            node.align = parse_align(a.get("align").unwrap_or("start"))?;
            node.equal = a.get("equal").map(|v| v == "true").unwrap_or(false);
        }
        NodeKind::Cluster => {
            node.align   = parse_align(a.get("align").unwrap_or("start"))?;
            node.justify = parse_justify(a.get("justify").unwrap_or("start"))?;
            node.wrap    = a.get("wrap").map(|v| v != "false").unwrap_or(true);
        }
        NodeKind::Frame => {
            node.align   = parse_align(a.get("align").unwrap_or("center"))?;
            node.justify = parse_justify(a.get("justify").unwrap_or("center"))?;
        }
        NodeKind::Grid => {
            node.cols = a.get("cols").and_then(|v| v.parse().ok()).unwrap_or(1);
            if let Some(v) = a.get("col-width") {
                node.col_width = Some(tokens.resolve_grid_col(v)?);
            }
        }
        NodeKind::Divider => {
            node.direction = match a.get("direction").unwrap_or("horizontal") {
                "vertical" => Direction::Vertical,
                _          => Direction::Horizontal,
            };
            node.color = Some(match a.get("color") {
                Some(v) => tokens.resolve_color(v)?,
                None    => "#000000".into(),
            });
            node.thickness = match a.get("thickness") {
                Some(v) => tokens.resolve_border_thickness(v)?,
                None    => 1.0,
            };
        }
        NodeKind::Text | NodeKind::Link | NodeKind::Img => {}
    }
    Ok(())
}

/// Apply `name` and `height` attributes for an `<img>` / `"img"` node.
/// Works with both XML elements and JSON attr objects via the `Attrs` trait.
fn apply_img_attrs(
    node:         &mut ParsedNode,
    a:            &impl Attrs,
    asset_images: &HashSet<String>,
    hint:         &str,
) -> Result<(), String> {
    let name = a.get("name")
        .ok_or_else(|| "<img> missing required attribute 'name'".to_string())?
        .to_string();
    validate_img_asset(&name, asset_images, hint)?;
    node.image_name = Some(name);
    // `height` on <img> sets the display height constraint (not HeightMode).
    if let Some(v) = a.get("height") {
        node.img_height_constraint = Some(
            parse_pt(v).ok_or_else(|| format!("img height: invalid pt value '{v}'"))?
        );
    }
    Ok(())
}

fn validate_img_asset(
    name:         &str,
    asset_images: &HashSet<String>,
    declare_hint: &str,
) -> Result<(), String> {
    if !asset_images.contains(name) {
        return Err(format!(
            "<img name=\"{name}\"> references an unknown asset image; \
             declare it in {declare_hint}"
        ));
    }
    Ok(())
}

// ── Tree (JSON) parser ────────────────────────────────────────────────────────

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

    // ── Assets ────────────────────────────────────────────────────────────────
    let mut asset_fonts:       HashMap<String, FontDef>   = HashMap::new();
    let mut asset_images:      HashSet<String>             = HashSet::new();
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
                // Optional caller-supplied glyph widths for accurate layout.
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
            for (name, _) in images_obj {
                asset_images.insert(name.clone());
            }
        }
    }

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
    let doc_font   = attrs.get("font")       .and_then(|v| v.as_str()).map(str::to_string);
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
    let doc_debug = attrs.get("debug").and_then(|v| v.as_str())
        .map(|v| v == "true").unwrap_or(false);

    // ── Pages ─────────────────────────────────────────────────────────────────
    let page_arr = root.get("children").and_then(|v| v.as_array())
        .ok_or("tree JSON 'children' must be an array")?;

    let mut pages: Vec<ParsedPage> = Vec::new();
    for child in page_arr {
        if child.get("type").and_then(|v| v.as_str()) != Some("page") {
            return Err("document children must all be page nodes".into());
        }
        pages.push(parse_tree_page(
            child, doc_size, doc_margin, doc_background.clone(),
            doc_debug, &tokens, &asset_images,
        )?);
    }
    if pages.is_empty() {
        return Err("document must have at least one page".into());
    }

    let parsed = ParsedDocument { meta, fonts: asset_fonts, font_widths: asset_font_widths, images: asset_images, pages, doc_font };
    Ok(resolve_doc(parsed))
}

fn parse_tree_tokens(
    obj: &serde_json::Map<String, serde_json::Value>,
    tokens: &mut Tokens,
) -> Result<(), String> {
    use crate::tokens::scale_idx;

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

    Ok(())
}

fn parse_tree_page(
    json:           &serde_json::Value,
    doc_size:       (f32, f32),
    doc_margin:     [f32; 4],
    doc_background: Option<String>,
    doc_debug:      bool,
    tokens:         &Tokens,
    asset_images:   &HashSet<String>,
) -> Result<ParsedPage, String> {
    let mut size       = doc_size;
    let mut margin     = doc_margin;
    let mut background = doc_background;
    let a = JsonAttrs(json);
    let debug = apply_page_overrides(&mut size, &mut margin, &mut background, &a, tokens, doc_debug)?;

    let children = if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
        arr.iter()
           .map(|c| parse_tree_node(c, tokens, asset_images))
           .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };

    Ok(ParsedPage { width: size.0, height: size.1, margin, background, debug, children })
}

fn parse_tree_node(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashSet<String>,
) -> Result<ParsedNode, String> {
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
        "img"     => NodeKind::Img,
        other => return Err(format!("unknown node type: '{other}'")),
    };

    let mut node = ParsedNode::default_for(kind.clone());
    let a = JsonAttrs(json);
    apply_box_attrs(&mut node, &a, tokens)?;
    apply_layout_kind_attrs(&mut node, &a, tokens)?;

    match kind {
        NodeKind::Text => {
            node.text_color = Some(match jattr(json, "color") {
                Some(v) => tokens.resolve_color(v)?,
                None    => tokens.resolve_color("text").unwrap_or_else(|_| "#1a1a1a".into()),
            });
            node.text_align = match jattr(json, "text-align").or_else(|| jattr(json, "align")).unwrap_or("left") {
                "center" => TextAlign::Center,
                "right"  => TextAlign::Right,
                _        => TextAlign::Left,
            };

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
        NodeKind::Img => {
            apply_img_attrs(&mut node, &a, asset_images, "assets.images")?;
        }
        _ => {}
    }

    // ── Layout children ───────────────────────────────────────────────────────
    if !matches!(kind, NodeKind::Divider | NodeKind::Img) {
        if let Some(arr) = json.get("children").and_then(|v| v.as_array()) {
            for child in arr {
                node.children.push(parse_tree_node(child, tokens, asset_images)?);
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
        let doc = parse(&minimal(r#"<text font-size="m" color="text">Hello world</text>"#)).unwrap();
        let t = &doc.pages[0].children[0];
        assert_eq!(t.kind, NodeKind::Text);
        assert_eq!(t.font_size, 11.0);
        assert_eq!(t.text_runs.len(), 1);
        assert_eq!(t.text_runs[0].text, "Hello world");
    }

    #[test]
    fn font_inheritance_from_document() {
        let xml = r#"<lpdf version="1">
            <assets>
                <fonts>
                    <font name="body" core="Helvetica-Oblique"/>
                </fonts>
            </assets>
            <document size="a4" font="body"><pages><page>
                <text>Hello</text>
            </page></pages></document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        let t = &doc.pages[0].children[0];
        assert_eq!(t.font, "Helvetica-Oblique");
    }

    #[test]
    fn font_size_inheritance() {
        let xml = r#"<lpdf version="1">
            <document size="a4"><pages><page>
                <stack font-size="14pt"><text>Hello</text></stack>
            </page></pages></document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        let stack = &doc.pages[0].children[0];
        let text  = &stack.children[0];
        assert_eq!(text.font_size, 14.0);
    }

    #[test]
    fn img_node_registered() {
        let xml = r#"<lpdf version="1">
            <assets>
                <images>
                    <image name="logo"/>
                </images>
            </assets>
            <document size="a4"><pages><page>
                <img name="logo" width="100pt"/>
            </page></pages></document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        let img = &doc.pages[0].children[0];
        assert_eq!(img.kind, NodeKind::Img);
        assert_eq!(img.image_name.as_deref(), Some("logo"));
    }

    #[test]
    fn img_unregistered_errors() {
        let result = parse(&minimal(r#"<img name="ghost" />"#));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown asset image"));
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
