use std::collections::HashMap;
use crate::tokens::{parse_pt, FontDef, FontWidths, Tokens};
use crate::canvas::{self, Canvas};

// ── Resolved document model (layout / render operate on these) ────────────────

#[derive(Debug, Clone)]
pub struct Document {
    pub meta:        Meta,
    pub fonts:       HashMap<String, FontDef>,
    pub images:      HashMap<String, String>,
    /// Caller-supplied glyph width tables.
    pub font_widths: HashMap<String, FontWidths>,
    // Document-level page defaults (inherited by each section unless overridden)
    pub page_width:  f32,
    pub page_height: f32,
    pub margin:      [f32; 4],
    pub background:  Option<String>,
    pub debug:       bool,
    /// Sections — always ≥ 1.  Single-section shorthand is normalised here.
    pub sections:    Vec<Section>,
}

impl Document {
    /// Derive a flat `Vec<SectionLayout>` from sections for use with the layout engine.
    /// Each section's layout child is converted to one `SectionLayout`.  Canvas children are
    /// collected into `underlays` (before first layout) or `overlays` (after) per document order.
    pub fn section_layouts(&mut self) -> Vec<SectionLayout> {
        let mut result = Vec::new();
        for section in std::mem::take(&mut self.sections) {
            let width  = section.options.size.map(|(w, _)| w).unwrap_or(self.page_width);
            let height = section.options.size.map(|(_, h)| h).unwrap_or(self.page_height);
            let margin = section.options.margin.unwrap_or(self.margin);
            let bg     = section.options.background
                            .or_else(|| self.background.clone());
            let debug  = section.options.debug.unwrap_or(self.debug);

            // Collect layout nodes including page_scope chrome nodes and canvas layers.
            // Canvas layers before the first <layout> become underlays; after → overlays.
            let mut children: Vec<Node> = Vec::new();
            let mut underlays: Vec<canvas::CanvasLayer> = Vec::new();
            let mut overlays:  Vec<canvas::CanvasLayer> = Vec::new();
            let mut seen_layout = false;
            for sc in section.children {
                match sc {
                    SectionChild::Layout(layout_children) => {
                        seen_layout = true;
                        for lc in layout_children {
                            match lc {
                                LayoutChild::Content(node) => children.push(node),
                                LayoutChild::Region(reg)   => children.push(region_to_compat_node(reg)),
                            }
                        }
                    }
                    SectionChild::Canvas(cv) => {
                        if seen_layout {
                            overlays.extend(cv.layers);
                        } else {
                            underlays.extend(cv.layers);
                        }
                    }
                }
            }
            result.push(SectionLayout { width, height, margin, background: bg, debug, children, underlays, overlays });
        }
        result
    }
}

/// Convert a `LayoutRegion` to a `Node` that the layout engine can handle.
/// The `page_scope` is passed through directly so all PageScope variants work correctly.
fn region_to_compat_node(reg: LayoutRegion) -> Node {
    Node {
        kind:       NodeKind::Stack,
        page_scope: Some(reg.page.unwrap_or(PageScope::Each)),
        debug:      reg.debug,
        children:   reg.children,
        ..Node::layout_default()
    }
}

// ── New section / canvas types ────────────────────────────────────────────────

/// A page-range within a `PageScope`.  `end = None` means "last".
#[derive(Debug, Clone, PartialEq)]
pub struct PageRange {
    pub start: u32,
    pub end:   Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageScope {
    Each,
    First,
    Last,
    Odd,
    Even,
    Pages(Vec<PageRange>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RegionPin { Top, Bottom, Left, Right }

/// A pinned chrome slot inside a `<layout>`.
/// Only valid as a direct child of `<layout>`, not inside containers.
#[derive(Debug, Clone)]
pub struct LayoutRegion {
    /// Not yet used by `region_to_compat_node`; reserved for left/right pins.
    #[allow(dead_code)]
    pub pin:      RegionPin,
    pub page:     Option<PageScope>,
    /// `w` is required for `Left`/`Right`; ignored for `Top`/`Bottom`.
    /// Not yet used by `region_to_compat_node`.
    #[allow(dead_code)]
    pub w:        Option<f32>,
    pub children: Vec<Node>,
    pub debug:    bool,
}

#[derive(Debug, Clone)]
pub enum LayoutChild {
    Content(Node),
    Region(LayoutRegion),
}

#[derive(Debug, Clone)]
pub enum SectionChild {
    Layout(Vec<LayoutChild>),
    Canvas(Canvas),
}

/// Per-section overrides; `None` fields inherit document-level defaults.
#[derive(Debug, Clone, Default)]
pub struct SectionOptions {
    pub size:       Option<(f32, f32)>,
    pub margin:     Option<[f32; 4]>,
    pub background: Option<String>,
    pub debug:      Option<bool>,
    /// Parsed but not yet surfaced to the render pipeline.
    #[allow(dead_code)]
    pub title:      Option<String>,
}

/// A content boundary with its own auto-pagination.
#[derive(Debug, Clone)]
pub struct Section {
    /// In document order: first child is painted first (bottom).
    pub children: Vec<SectionChild>,
    pub options:  SectionOptions,
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
pub struct SectionLayout {
    pub width: f32,
    pub height: f32,
    pub margin: [f32; 4], // top, right, bottom, left
    pub background: Option<String>,
    pub debug: bool,
    pub children: Vec<Node>,
    pub underlays: Vec<canvas::CanvasLayer>,
    pub overlays: Vec<canvas::CanvasLayer>,
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

/// Data-binding attributes.  Boxed on `Node` so that the common case (no
/// data binding) pays only a pointer's worth of overhead rather than 4 × 24
/// bytes inline on every node in the tree.
#[derive(Debug, Clone, Default)]
pub struct DataAttrs {
    pub data_value:  Option<String>,
    pub data_source: Option<String>,
    pub data_if:     Option<String>,
    pub data_if_not: Option<String>,
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
    pub page_scope: Option<PageScope>,
    pub paginate: Paginate,
    pub debug: bool,
    // layout-specific
    pub align: Align,
    pub justify: Justify,
    pub end: bool,
    pub equal: bool,
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
    // barcode (NodeKind::Barcode only)
    pub barcode_type: Option<BarcodeType>,
    pub barcode_data: Option<String>,
    pub barcode_ec: BarcodeEcLevel,
    pub barcode_hrt: bool,
    pub barcode_color: Option<String>,
    pub barcode_bg: Option<String>,
    // table (NodeKind::Table only)
    pub table_cols: String,
    pub stripe: Option<String>,
    // form field (NodeKind::Field only)
    pub field_kind:       Option<FieldKind>,
    pub field_name:       Option<String>,
    pub field_value:      Option<String>,
    pub field_label:      Option<String>,
    pub field_options:    Vec<String>,
    pub field_required:   bool,
    pub field_readonly:   bool,
    pub field_checked:    bool,
    pub field_max_len:    Option<u32>,
    pub field_group:      Option<String>,
    pub field_action_url: Option<String>,
    /// Data-binding attributes; `None` for the vast majority of nodes that
    /// carry no `data-*` attributes, saving 96 bytes per node.
    pub data_attrs: Option<Box<DataAttrs>>,
    pub children: Vec<Node>,
}

/// Pagination hint set via `paginate="…"` on any node.
#[derive(Debug, Clone, PartialEq)]
pub enum Paginate {
    /// Normal flow — split according to node type (default).
    None,
    /// Never split this node across pages; treat as atomic regardless of type.
    No,
    /// Always start this node on a new page.
    BreakBefore,
    /// Start a new page after this node (and all its continuations) have been placed.
    BreakAfter,
    /// If the next sibling would land on a different page, bump this node to that page too.
    KeepNext,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Stack,
    Flank,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Split,
    Cluster,
    Grid,
    Frame,
    Divider,
    Text,
    Link,
    Img,
    Barcode,
    Field,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldKind {
    Text,
    Checkbox,
    Dropdown,
    Radio,
    Button,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BarcodeType {
    Qr,
    Code128,
    Ean13,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BarcodeEcLevel {
    L,
    M,
    Q,
    H,
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
    Justify,
}

// ── Pre-trickle-down (parsed) model ──────────────────────────────────────────

/// A node as produced by the XML/JSON parser, before font inheritance is
/// resolved.  `font` and `font_size` may be `None` (meaning "inherit from
/// parent").  Font resolution happens in `resolve_node` during section parsing.
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
    page_scope: Option<PageScope>,
    paginate: Paginate,
    debug: bool,
    align: Align,
    justify: Justify,
    end: bool,
    equal: bool,
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
    barcode_type: Option<BarcodeType>,
    barcode_data: Option<String>,
    barcode_ec: BarcodeEcLevel,
    barcode_hrt: bool,
    barcode_color: Option<String>,
    barcode_bg: Option<String>,
    table_cols: String,
    stripe: Option<String>,
    field_kind:       Option<FieldKind>,
    field_name:       Option<String>,
    field_value:      Option<String>,
    field_label:      Option<String>,
    field_options:    Vec<String>,
    field_required:   bool,
    field_readonly:   bool,
    field_checked:    bool,
    field_max_len:    Option<u32>,
    field_group:      Option<String>,
    field_action_url: Option<String>,
    data_attrs: Option<Box<DataAttrs>>,
    children: Vec<ParsedNode>,
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
            page_scope: None,
            paginate: Paginate::None,
            debug: false,
            align: Align::Stretch,
            justify: Justify::Start,
            end: false,
            equal: false,
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
            barcode_type: None,
            barcode_data: None,
            barcode_ec: BarcodeEcLevel::M,
            barcode_hrt: false,
            barcode_color: None,
            barcode_bg: None,
            table_cols: String::new(),
            stripe: None,
            field_kind:       None,
            field_name:       None,
            field_value:      None,
            field_label:      None,
            field_options:    Vec::new(),
            field_required:   false,
            field_readonly:   false,
            field_checked:    false,
            field_max_len:    None,
            field_group:      None,
            field_action_url: None,
            data_attrs: None,
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

/// `Node::layout_default()` — used by region_to_compat_node.
impl Node {
    pub fn layout_default() -> Node {
        Node {
            kind: NodeKind::Stack,
            gap: 0.0, padding: [0.0; 4], background: None, border: None, radius: 0.0,
            height_mode: HeightMode::Auto, width_constraint: None,
            page_scope: None, paginate: Paginate::None, debug: false,
            align: Align::Stretch, justify: Justify::Start, end: false, equal: false,
            cols: 1, col_width: None, direction: Direction::Horizontal,
            color: None, thickness: 1.0, text_runs: Vec::new(),
            font: "Helvetica".to_string(), font_size: 11.0, text_color: None,
            text_align: TextAlign::Left, url: None, image_name: None,
            img_height_constraint: None, barcode_type: None, barcode_data: None,
            barcode_ec: BarcodeEcLevel::M, barcode_hrt: false, barcode_color: None,
            barcode_bg: None, table_cols: String::new(), stripe: None,
            field_kind: None, field_name: None, field_value: None, field_label: None,
            field_options: Vec::new(), field_required: false, field_readonly: false,
            field_checked: false, field_max_len: None, field_group: None,
            field_action_url: None, data_attrs: None, children: Vec::new(),
        }
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
        page_scope:            n.page_scope,
        paginate:              n.paginate,
        debug:                 n.debug,
        align:                 n.align,
        justify:               n.justify,
        end:                   n.end,
        equal:                 n.equal,
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
        barcode_type:          n.barcode_type,
        barcode_data:          n.barcode_data,
        barcode_ec:            n.barcode_ec,
        barcode_hrt:           n.barcode_hrt,
        barcode_color:         n.barcode_color,
        barcode_bg:            n.barcode_bg,
        table_cols:            n.table_cols,
        stripe:                n.stripe,
        field_kind:            n.field_kind,
        field_name:            n.field_name,
        field_value:           n.field_value,
        field_label:           n.field_label,
        field_options:         n.field_options,
        field_required:        n.field_required,
        field_readonly:        n.field_readonly,
        field_checked:         n.field_checked,
        field_max_len:         n.field_max_len,
        field_group:           n.field_group,
        field_action_url:      n.field_action_url,
        data_attrs:           n.data_attrs,
        children,
    }
}


// ── Main parse entry point (XML) ─────────────────────────────────────────────

pub fn parse(xml: &str) -> Result<Document, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("XML parse error: {e}"))?;

    let root = doc.root_element();
    if root.tag_name().name() != "lpdf" {
        return Err(format!(
            "<{}>{}: root element must be <lpdf>",
            root.tag_name().name(), node_loc(&root)
        ));
    }

    // ── Pass 0: collect <assets> ─────────────────────────────────────────────
    let mut asset_fonts:  HashMap<String, FontDef>   = HashMap::new();
    let mut asset_images: HashMap<String, String>     = HashMap::new();
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

                let mut sections_xml: Vec<Section> = Vec::new();
                for doc_child in elems(&child) {
                    match doc_child.tag_name().name() {
                        "meta" => meta = parse_meta(&doc_child),
                        "section" => {
                            sections_xml.push(parse_section_elem(
                                &doc_child,
                                doc_size,
                                doc_margin,
                                doc_background.clone(),
                                doc_debug,
                                doc_font.as_deref(),
                                &tokens,
                                &asset_fonts,
                                &asset_images,
                            )?);
                        }
                        other => return Err(format!(
                            "<{}>{}: unexpected element in <document>",
                            other, node_loc(&doc_child)
                        )),
                    }
                }
                if sections_xml.is_empty() {
                    return Err(format!(
                        "<document>{}: must contain at least one <section>",
                        node_loc(&child)
                    ));
                }
                let mut resolved_fonts: HashMap<String, FontDef> = HashMap::new();
                for (_alias, def) in &asset_fonts {
                    let key = match def {
                        FontDef::Core(n) => n.clone(),
                        FontDef::Ref(k)  => k.clone(),
                    };
                    resolved_fonts.insert(key, def.clone());
                }
                return Ok(Document {
                    meta,
                    fonts:       resolved_fonts,
                    font_widths: HashMap::new(),
                    images:      asset_images,
                    page_width:  doc_size.0,
                    page_height: doc_size.1,
                    margin:      doc_margin,
                    background:  doc_background,
                    debug:       doc_debug,
                    sections:    sections_xml,
                });
            }
            other => return Err(format!("<{}>{}: unexpected element in <lpdf>", other, node_loc(&child))),
        }
    }

    Err(format!("<lpdf>{}: must contain a <document> element", node_loc(&root)))
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
    images:       &mut HashMap<String, String>,
) -> Result<(), String> {
    for child in elems(elem) {
        match child.tag_name().name() {
            "font" => {
                let name = req_attr(&child, "name")?;
                validate_asset_name(&name)?;
                let def = if let Some(b) = child.attribute("core") {
                    FontDef::Core(b.to_string())
                } else if let Some(r) = child.attribute("ref") {
                    if is_url(r) {
                        return Err(format!(
                            "<font>{}: ref must be a registry key, not a URL",
                            node_loc(&child)
                        ));
                    }
                    FontDef::Ref(r.to_string())
                } else {
                    // No ref: use name as the registry key.
                    FontDef::Ref(name.clone())
                };
                // src is an adapter-level hint only — ignored by the Rust parser.
                fonts.insert(name, def);
            }
            "image" => {
                let name = req_attr(&child, "name")?;
                validate_asset_name(&name)?;
                let key = child.attribute("ref").unwrap_or(&name).to_string();
                // src is an adapter-level hint only — ignored by the Rust parser.
                images.insert(name, key);
            }
            other => return Err(format!("<{}>{}: unknown element in <assets>", other, node_loc(&child))),
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
            "text-size" => tokens.text_size = parse_scale_row(&child)?,
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
            other => return Err(format!("<{}>{}: unknown element in <tokens>", other, node_loc(&child))),
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
/// source onto the running section-defaults.  Works for both XML `<section>` elements
/// and JSON section nodes via the `Attrs` trait.  Returns the resolved `debug` flag.
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


pub fn parse_page_size(val: &str) -> Result<(f32, f32), String> {
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

/// Convert a measurement string with a unit suffix to points.
/// Supported: `pt`, `mm`, `in`.  Bare numbers are treated as `pt`.
pub fn parse_measurement(s: &str) -> Result<f32, String> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("mm") {
        let n: f32 = v.trim().parse().map_err(|_| format!("invalid measurement: '{s}'"))?;
        Ok(n * 72.0 / 25.4)
    } else if let Some(v) = s.strip_suffix("in") {
        let n: f32 = v.trim().parse().map_err(|_| format!("invalid measurement: '{s}'"))?;
        Ok(n * 72.0)
    } else if let Some(v) = s.strip_suffix("pt") {
        v.trim().parse().map_err(|_| format!("invalid measurement: '{s}'"))
    } else {
        s.parse().map_err(|_| format!("invalid measurement: '{s}'"))
    }
}

/// Parse a `PageScope` from its string representation.
/// Keywords: `each`, `first`, `last`, `odd`, `even`.
/// Numeric: comma-separated ranges where each is `N`, `N-M`, or `N-last`.
pub fn parse_page_scope(s: &str) -> Result<PageScope, String> {
    let s = s.trim();
    match s {
        "each"  => return Ok(PageScope::Each),
        "first" => return Ok(PageScope::First),
        "last"  => return Ok(PageScope::Last),
        "odd"   => return Ok(PageScope::Odd),
        "even"  => return Ok(PageScope::Even),
        _       => {}
    }
    // Numeric range(s): "1", "2-4", "1,3-5", "2-last"
    let mut ranges = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() { continue; }
        if let Some((a, b)) = part.split_once('-') {
            let start: u32 = a.trim().parse()
                .map_err(|_| format!("invalid page scope '{s}': '{a}' is not a number"))?;
            let end = if b.trim() == "last" {
                None
            } else {
                let n: u32 = b.trim().parse()
                    .map_err(|_| format!("invalid page scope '{s}': '{b}' is not a number or 'last'"))?;
                Some(n)
            };
            ranges.push(PageRange { start, end });
        } else {
            let n: u32 = part.parse()
                .map_err(|_| format!("invalid page scope '{s}': '{part}' is not a number"))?;
            ranges.push(PageRange { start: n, end: Some(n) });
        }
    }
    if ranges.is_empty() {
        return Err(format!("invalid page scope: '{s}'"));
    }
    Ok(PageScope::Pages(ranges))
}

/// Parse a signed measurement (allows negative values) to points.
pub fn parse_signed_measurement(s: &str) -> Result<f32, String> {
    let s = s.trim();
    let neg = s.starts_with('-');
    let inner = if neg { &s[1..] } else { s };
    let val = parse_measurement(inner)?;
    Ok(if neg { -val } else { val })
}

/// Thin wrapper: resolve a ParsedNode in isolation (used during XML parse of sections
/// where we haven't yet done the full resolve_doc pass).  The caller should use
/// `resolve_doc` for proper font inheritance; this is only used to convert
/// `ParsedNode` → `Node` when the context font is already known to be the default.
fn resolve_parsed_node(
    n:     ParsedNode,
    font:  &str,
    size:  f32,
    fonts: &HashMap<String, FontDef>,
) -> Node {
    resolve_node(n, font, size, fonts)
}

fn parse_region_elem(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    asset_fonts:  &HashMap<String, FontDef>,
    root_font:    &str,
    root_size:    f32,
) -> Result<LayoutRegion, String> {
    let pin_str = req_attr(elem, "pin")?;
    let pin = match pin_str.as_str() {
        "top"    => RegionPin::Top,
        "bottom" => RegionPin::Bottom,
        "left"   => RegionPin::Left,
        "right"  => RegionPin::Right,
        other => return Err(format!("<region>{}: invalid pin '{other}' — use top, bottom, left, or right", node_loc(elem))),
    };
    let page = elem.attribute("page").map(parse_page_scope).transpose()?;
    let w    = elem.attribute("w").map(parse_measurement).transpose()?;
    let debug = elem.attribute("debug").map(|v| v == "true").unwrap_or(false);

    let mut children = Vec::new();
    for child in elems(elem) {
        children.push(resolve_parsed_node(
            parse_node(&child, tokens, asset_images)?,
            root_font, root_size, asset_fonts,
        ));
    }
    Ok(LayoutRegion { pin, page, w, children, debug })
}

#[allow(clippy::too_many_arguments)]
fn parse_section_elem(
    elem:           &roxmltree::Node,
    doc_size:       (f32, f32),
    doc_margin:     [f32; 4],
    doc_background: Option<String>,
    doc_debug:      bool,
    doc_font:       Option<&str>,
    tokens:         &Tokens,
    asset_fonts:    &HashMap<String, FontDef>,
    asset_images:   &HashMap<String, String>,
) -> Result<Section, String> {
    // Inherit doc-level defaults, then apply section overrides.
    let mut size       = doc_size;
    let mut margin     = doc_margin;
    let mut background = doc_background;
    let debug = apply_page_overrides(&mut size, &mut margin, &mut background, elem, tokens, doc_debug)?;
    let title = elem.attribute("title").map(str::to_string);

    let root_font = doc_font
        .map(|f| resolve_font_alias(f, asset_fonts))
        .unwrap_or_else(|| "Helvetica".to_string());
    let root_size = 11.0_f32;

    let mut children: Vec<SectionChild> = Vec::new();
    for child in elems(elem) {
        match child.tag_name().name() {
            "layout" => {
                // Inline layout: build a Layout from its children (may include <region>).
                let mut lc: Vec<LayoutChild> = Vec::new();
                for layout_child in elems(&child) {
                    if layout_child.tag_name().name() == "region" {
                        lc.push(LayoutChild::Region(parse_region_elem(&layout_child, tokens, asset_images, asset_fonts, &root_font, root_size)?));
                    } else {
                        lc.push(LayoutChild::Content(resolve_parsed_node(
                            parse_node(&layout_child, tokens, asset_images)?,
                            &root_font, root_size, asset_fonts,
                        )));
                    }
                }
                children.push(SectionChild::Layout(lc));
            }
            "canvas" => {
                children.push(SectionChild::Canvas(
                    canvas::parse_canvas_elem(&child, tokens, asset_images, size.0, size.1)?
                ));
            }
            other => return Err(format!(
                "<{}>{}: unexpected child in <section> — expected <layout> or <canvas>",
                other, node_loc(&child)
            )),
        }
    }

    if children.is_empty() {
        return Err(format!(
            "<section>{}: must have at least one <layout> or <canvas> child",
            node_loc(elem)
        ));
    }

    Ok(Section {
        children,
        options: SectionOptions {
            size:       if size != doc_size { Some(size) } else { None },
            margin:     Some(margin),
            background,
            debug:      Some(debug),
            title,
        },
    })
}

// ── Node parsing ──────────────────────────────────────────────────────────────

fn parse_node(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
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
        "barcode" => NodeKind::Barcode,
        "field"   => NodeKind::Field,
        "table"   => NodeKind::Table,
        "thead"   => NodeKind::TableHead,
        "tr"      => NodeKind::TableRow,
        "td"      => NodeKind::TableCell,
        other     => return Err(format!("<{}>{}: unknown element", other, node_loc(elem))),
    };

    let mut node = ParsedNode::default_for(kind.clone());
    apply_box_attrs(&mut node, elem, tokens)
        .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;
    apply_layout_kind_attrs(&mut node, elem, tokens)
        .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;

    // data-binding attributes (valid on any element; boxed to keep Node lean)
    {
        let dv  = elem.attribute("data-value").map(str::to_owned);
        let ds  = elem.attribute("data-source").map(str::to_owned);
        let di  = elem.attribute("data-if").map(str::to_owned);
        let din = elem.attribute("data-if-not").map(str::to_owned);
        if dv.is_some() || ds.is_some() || di.is_some() || din.is_some() {
            node.data_attrs = Some(Box::new(DataAttrs {
                data_value:  dv,
                data_source: ds,
                data_if:     di,
                data_if_not: din,
            }));
        }
    }

    match kind {
        NodeKind::Text => {
            node.text_color = Some(if let Some(v) = elem.attribute("color") {
                tokens.resolve_color(v)?
            } else {
                tokens.resolve_color("text").unwrap_or_else(|_| "#1a1a1a".into())
            });
            node.text_align = match elem.attribute("align").unwrap_or("left") {
                "center"  => TextAlign::Center,
                "right"   => TextAlign::Right,
                "justify" => TextAlign::Justify,
                _         => TextAlign::Left,
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
                    .ok_or_else(|| format!("<link>{}: missing required attribute 'href'", node_loc(elem)))?
                    .to_string(),
            );
        }
        NodeKind::Img => {
            apply_img_attrs(&mut node, elem, asset_images, "<assets>")
                .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;
        }
        NodeKind::Barcode => {
            apply_barcode_attrs(&mut node, elem, tokens)
                .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;
        }
        NodeKind::Field => {
            apply_field_attrs(&mut node, elem, tokens)
                .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;
        }
        _ => {}
    }

    // ── Children (not for leaf nodes) ────────────────────────────────────────
    if !matches!(kind, NodeKind::Divider | NodeKind::Text | NodeKind::Img | NodeKind::Barcode | NodeKind::Field) {
        for child in elems(elem) {
            let child_node = parse_node(&child, tokens, asset_images)?;
            match kind {
                NodeKind::Table => {
                    if !matches!(child_node.kind, NodeKind::TableHead | NodeKind::TableRow) {
                        return Err(format!(
                            "<table>{}: child <{}> is not allowed here — expected <thead> or <tr>",
                            node_loc(elem), child.tag_name().name()
                        ));
                    }
                }
                NodeKind::TableHead | NodeKind::TableRow => {
                    if child_node.kind != NodeKind::TableCell {
                        return Err(format!(
                            "<{}>{}: child <{}> is not allowed here — expected <td>",
                            tag, node_loc(elem), child.tag_name().name()
                        ));
                    }
                }
                _ => {}
            }
            node.children.push(child_node);
        }
        if kind == NodeKind::Frame && node.children.len() > 1 {
            return Err(format!(
                "<frame>{}: accepts at most one child; got {}",
                node_loc(elem), node.children.len()
            ));
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
        other => Err(format!("invalid align value: '{other}' — expected start, center, end, or stretch")),
    }
}

fn parse_justify(val: &str) -> Result<Justify, String> {
    match val {
        "start" => Ok(Justify::Start),
        "center" => Ok(Justify::Center),
        "end" => Ok(Justify::End),
        "between" => Ok(Justify::Between),
        other => Err(format!("invalid justify value: '{other}' — expected start, center, end, or between")),
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

pub(crate) fn elems<'a>(
    node: &'a roxmltree::Node<'a, 'a>,
) -> impl Iterator<Item = roxmltree::Node<'a, 'a>> {
    node.children().filter(|n| n.is_element())
}

/// Format the source position of an XML element as " at line N, col M".
fn node_loc(elem: &roxmltree::Node) -> String {
    let pos = elem.document().text_pos_at(elem.range().start);
    format!(" at line {}, col {}", pos.row, pos.col)
}

fn req_attr(elem: &roxmltree::Node, name: &str) -> Result<String, String> {
    elem.attribute(name)
        .map(|s| s.to_string())
        .ok_or_else(|| {
            format!(
                "<{}>{}: missing required attribute '{name}'",
                elem.tag_name().name(), node_loc(elem)
            )
        })
}

fn opt_attr(elem: &roxmltree::Node, name: &str) -> String {
    elem.attribute(name).unwrap_or("").to_string()
}

// ── Shared node construction helpers ─────────────────────────────────────────

/// Read a string attribute from a JSON node's "attrs" object.
pub(crate) fn jattr<'a>(json: &'a serde_json::Value, key: &str) -> Option<&'a str> {
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
            .ok_or_else(|| format!("invalid height value: '{other}' — expected full, fill, or a pt value like 28pt")),
    }
}

fn parse_paginate_attr(v: &str) -> Result<Paginate, String> {
    match v {
        "no"           => Ok(Paginate::No),
        "break-before" => Ok(Paginate::BreakBefore),
        "break-after"  => Ok(Paginate::BreakAfter),
        "keep-next"    => Ok(Paginate::KeepNext),
        other          => Err(format!(
            "invalid paginate value: '{other}' (expected 'no', 'break-before', 'break-after', or 'keep-next')"
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
    if !matches!(node.kind, NodeKind::Img | NodeKind::Barcode | NodeKind::Field) {
        if let Some(v) = a.get("height") { node.height_mode = parse_height_mode(v)?; }
    }
    if let Some(v) = a.get("width")  { node.width_constraint = Some(tokens.resolve_width(v)?); }
    if let Some(v) = a.get("paginate") { node.paginate = parse_paginate_attr(v)?; }
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
            node.align   = parse_align(a.get("align").unwrap_or("stretch"))?;
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
            let j_str = a.get("justify").unwrap_or("start");
            if j_str == "between" {
                return Err("cluster does not support justify=\"between\"; use start, center, or end".into());
            }
            node.justify = parse_justify(j_str)?;
        }
        NodeKind::Frame => {
            // Frame always centers its single child; these attrs are not supported.
            if a.get("gap").is_some() {
                return Err("frame does not support gap; it always centers its child".into());
            }
            if a.get("align").is_some() {
                return Err("frame does not support align; it always centers its child".into());
            }
            if a.get("justify").is_some() {
                return Err("frame does not support justify; it always centers its child".into());
            }
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
        NodeKind::Table => {
            if let Some(v) = a.get("cols") {
                node.table_cols = v.to_string();
            }
            if let Some(v) = a.get("stripe") {
                node.stripe = Some(tokens.resolve_color(v)?);
            }
        }
        NodeKind::TableHead | NodeKind::TableRow => {}
        NodeKind::TableCell => {
            node.align   = parse_align(a.get("align").unwrap_or("stretch"))?;
            let valign_str = a.get("valign").unwrap_or("top");
            node.justify = parse_justify(match valign_str {
                "top"    => "start",
                "middle" => "center",
                "bottom" => "end",
                other    => other,
            })?;
        }
        NodeKind::Text | NodeKind::Link | NodeKind::Img | NodeKind::Barcode | NodeKind::Field => {}
    }
    Ok(())
}

/// Apply `name` and `height` attributes for an `<img>` / `"img"` node.
/// Works with both XML elements and JSON attr objects via the `Attrs` trait.
fn apply_img_attrs(
    node:         &mut ParsedNode,
    a:            &impl Attrs,
    asset_images: &HashMap<String, String>,
    hint:         &str,
) -> Result<(), String> {
    let name = a.get("name")
        .ok_or_else(|| "<img> missing required attribute 'name'".to_string())?
        .to_string();
    validate_img_asset(&name, asset_images, hint)?;
    // Store the registry key (ref ?? name) so the render engine can look up bytes.
    let registry_key = asset_images.get(&name).unwrap().clone();
    node.image_name = Some(registry_key);
    // `height` on <img> sets the display height constraint (not HeightMode).
    if let Some(v) = a.get("height") {
        node.img_height_constraint = Some(
            parse_pt(v).ok_or_else(|| format!("img height: invalid pt value '{v}'"))?
        );
    }
    Ok(())
}

/// Apply attributes for a `<barcode>` / `"barcode"` node.
/// Works with both XML elements and JSON attr objects via the `Attrs` trait.
fn apply_barcode_attrs(
    node: &mut ParsedNode,
    a:    &impl Attrs,
    tokens: &Tokens,
) -> Result<(), String> {
    let type_str = a.get("type")
        .ok_or_else(|| "<barcode> missing required attribute 'type'".to_string())?;
    node.barcode_type = Some(match type_str {
        "qr"      => BarcodeType::Qr,
        "code128" => BarcodeType::Code128,
        "ean13"   => BarcodeType::Ean13,
        other     => return Err(format!("<barcode> unknown type '{other}'; use 'qr', 'code128', or 'ean13'")),
    });

    node.barcode_data = Some(
        a.get("data")
         .ok_or_else(|| "<barcode> missing required attribute 'data'".to_string())?
         .to_string(),
    );

    // Error correction level (QR only, ignored for 1D barcodes)
    node.barcode_ec = match a.get("ec").unwrap_or("M") {
        "L" => BarcodeEcLevel::L,
        "M" => BarcodeEcLevel::M,
        "Q" => BarcodeEcLevel::Q,
        "H" => BarcodeEcLevel::H,
        other => return Err(format!("<barcode> invalid ec '{other}'; use L, M, Q, or H")),
    };

    // Human-readable text below 1D barcodes
    node.barcode_hrt = a.get("hrt").map(|v| v == "true").unwrap_or(false);

    // Bar color
    node.barcode_color = if let Some(v) = a.get("color") {
        Some(tokens.resolve_color(v)?)
    } else {
        None
    };

    // Background fill
    node.barcode_bg = if let Some(v) = a.get("background") {
        Some(tokens.resolve_color(v)?)
    } else {
        None
    };

    // Dimensions:
    // QR uses `size` for both width and height (square).
    // Code128 / EAN-13 use `width` and `height` independently.
    match node.barcode_type.as_ref().unwrap() {
        BarcodeType::Qr => {
            let size = a.get("size")
                .ok_or_else(|| "<barcode type=\"qr\"> missing required attribute 'size'".to_string())?;
            let pt = tokens.resolve_width(size)
                .map_err(|_| format!("<barcode> invalid size '{size}'"))?;
            node.width_constraint      = Some(pt);
            node.img_height_constraint = Some(pt);
        }
        BarcodeType::Code128 | BarcodeType::Ean13 => {
            if let Some(w) = a.get("width") {
                node.width_constraint = Some(
                    tokens.resolve_width(w)
                          .map_err(|_| format!("<barcode> invalid width '{w}'"))?,
                );
            }
            if let Some(h) = a.get("height") {
                node.img_height_constraint = Some(
                    tokens.resolve_width(h)
                          .map_err(|_| format!("<barcode> invalid height '{h}'"))?,
                );
            }
        }
    }

    Ok(())
}

pub(crate) fn validate_img_asset(
    name:         &str,
    asset_images: &HashMap<String, String>,
    declare_hint: &str,
) -> Result<(), String> {
    if !asset_images.contains_key(name) {
        return Err(format!(
            "<img name=\"{name}\"> references an unknown asset image; \
             declare it in {declare_hint}"
        ));
    }
    Ok(())
}

// ── Tree (JSON) parser ────────────────────────────────────────────────────────

/// Apply attributes for a `<field>` / `"field"` node.
fn apply_field_attrs(
    node:   &mut ParsedNode,
    a:      &impl Attrs,
    tokens: &Tokens,
) -> Result<(), String> {
    let type_str = a.get("type")
        .ok_or_else(|| "<field> missing required attribute 'type'".to_string())?;
    node.field_kind = Some(match type_str {
        "text"     => FieldKind::Text,
        "checkbox" => FieldKind::Checkbox,
        "dropdown" => FieldKind::Dropdown,
        "radio"    => FieldKind::Radio,
        "button"   => FieldKind::Button,
        other => return Err(format!(
            "<field> unknown type '{other}'; use text, checkbox, dropdown, radio, or button"
        )),
    });

    node.field_name = Some(
        a.get("name")
            .ok_or_else(|| "<field> missing required attribute 'name'".to_string())?
            .to_string(),
    );

    if let Some(v) = a.get("value")      { node.field_value      = Some(v.to_string()); }
    if let Some(v) = a.get("label")      { node.field_label      = Some(v.to_string()); }
    if let Some(v) = a.get("group")      { node.field_group      = Some(v.to_string()); }
    if let Some(v) = a.get("action-url") { node.field_action_url = Some(v.to_string()); }
    if let Some(v) = a.get("options")    {
        node.field_options = v.split(',').map(|s| s.trim().to_string()).collect();
    }
    node.field_required = a.get("required").map(|v| v == "true").unwrap_or(false);
    node.field_readonly = a.get("readonly").map(|v| v == "true").unwrap_or(false);
    node.field_checked  = a.get("checked").map(|v| v == "true").unwrap_or(false);
    if let Some(v) = a.get("max-len") {
        node.field_max_len = Some(
            v.parse::<u32>().map_err(|_| format!("<field> invalid max-len '{v}'"))?
        );
    }
    // height stored as img_height_constraint (atomic leaf, like <img>)
    if let Some(v) = a.get("height") {
        node.img_height_constraint = Some(
            tokens.resolve_width(v)
                  .map_err(|_| format!("<field> invalid height '{v}'"))?
        );
    }

    Ok(())
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
            for (name, def) in images_obj {
                let key = def.get("ref").and_then(|v| v.as_str())
                    .unwrap_or(name)
                    .to_string();
                asset_images.insert(name.clone(), key);
            }
        }
    }

    // ── Tokens ────────────────────────────────────────────────────────────────
    let mut tokens = Tokens::default();
    if let Some(tok) = attrs.get("tokens").and_then(|v| v.as_object()) {
        parse_tree_tokens_pub(tok, &mut tokens)?;
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
    let doc_debug = attrs.get("debug").and_then(|v| v.as_str())
        .map(|v| v == "true").unwrap_or(false);

    // ── Sections ──────────────────────────────────────────────────────────────
    let nodes_arr = root.get("nodes")
        .and_then(|v| v.as_array())
        .ok_or("tree JSON must have a 'nodes' array")?;

    let mut sections: Vec<Section> = Vec::new();
    for sec_json in nodes_arr {
        sections.push(parse_tree_section(
            sec_json, doc_size, doc_margin, doc_background.clone(),
            doc_debug, &tokens, &asset_fonts, &asset_images,
        )?);
    }
    if sections.is_empty() {
        return Err("document must have at least one section".into());
    }
    let mut resolved_fonts: HashMap<String, FontDef> = HashMap::new();
    for (_alias, def) in &asset_fonts {
        let key = match def {
            FontDef::Core(n) => n.clone(),
            FontDef::Ref(k)  => k.clone(),
        };
        resolved_fonts.insert(key, def.clone());
    }
    Ok(Document {
        meta,
        fonts:       resolved_fonts,
        font_widths: asset_font_widths,
        images:      asset_images,
        page_width:  doc_size.0,
        page_height: doc_size.1,
        margin:      doc_margin,
        background:  doc_background,
        debug:       doc_debug,
        sections,
    })
}

pub fn parse_tree_tokens_pub(
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
    apply_scale("text-size", &mut tokens.text_size)?;

    if let Some(colors) = obj.get("colors").and_then(|v| v.as_object()) {
        for (k, v) in colors {
            if let Some(s) = v.as_str() {
                tokens.colors.insert(k.clone(), crate::tokens::normalize_hex(s)?);
            }
        }
    }

    Ok(())
}


#[allow(clippy::too_many_arguments)]
fn parse_tree_section(
    json:           &serde_json::Value,
    doc_size:       (f32, f32),
    doc_margin:     [f32; 4],
    doc_background: Option<String>,
    doc_debug:      bool,
    tokens:         &Tokens,
    asset_fonts:    &HashMap<String, FontDef>,
    asset_images:   &HashMap<String, String>,
) -> Result<Section, String> {
    let mut size       = doc_size;
    let mut margin     = doc_margin;
    let mut background = doc_background;
    let a = JsonAttrs(json);
    let debug = apply_page_overrides(&mut size, &mut margin, &mut background, &a, tokens, doc_debug)?;
    let title = jattr(json, "title").map(str::to_string);

    let root_font = "Helvetica";
    let root_size = 11.0_f32;

    let mut children: Vec<SectionChild> = Vec::new();
    if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
        for child in arr {
            let kind = child.get("type").and_then(|v| v.as_str());
            match kind {
                Some("layout") => {
                    let mut lc: Vec<LayoutChild> = Vec::new();
                    if let Some(nodes) = child.get("nodes").and_then(|v| v.as_array()) {
                        for n in nodes {
                            if n.get("type").and_then(|v| v.as_str()) == Some("layout-region") {
                                lc.push(LayoutChild::Region(parse_tree_region(n, tokens, asset_images, asset_fonts)?));
                            } else {
                                lc.push(LayoutChild::Content(
                                    resolve_parsed_node(
                                        parse_tree_node(n, tokens, asset_images)?,
                                        root_font, root_size, asset_fonts,
                                    )
                                ));
                            }
                        }
                    }
                    children.push(SectionChild::Layout(lc));
                }
                Some("canvas") => {
                    let mut layers = Vec::new();
                    if let Some(nodes) = child.get("nodes").and_then(|v| v.as_array()) {
                        for layer_json in nodes {
                            layers.push(canvas::parse_tree_canvas_layer(layer_json, tokens, asset_images, size.0, size.1)?);
                        }
                    }
                    children.push(SectionChild::Canvas(Canvas { layers }));
                }
                other => return Err(format!(
                    "section child 'type' must be 'layout' or 'canvas', got {:?}", other
                )),
            }
        }
    }

    if children.is_empty() {
        return Err("section must have at least one layout or canvas child".into());
    }

    Ok(Section {
        children,
        options: SectionOptions {
            size:       if size != doc_size { Some(size) } else { None },
            margin:     Some(margin),
            background,
            debug:      Some(debug),
            title,
        },
    })
}

fn parse_tree_region(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    asset_fonts:  &HashMap<String, FontDef>,
) -> Result<LayoutRegion, String> {
    let pin_str = jattr(json, "pin")
        .ok_or("layout-region missing 'pin' attribute")?;
    let pin = match pin_str {
        "top"    => RegionPin::Top,
        "bottom" => RegionPin::Bottom,
        "left"   => RegionPin::Left,
        "right"  => RegionPin::Right,
        other => return Err(format!("layout-region invalid pin '{other}'")),
    };
    let page  = jattr(json, "page").map(parse_page_scope).transpose()?;
    let w     = jattr(json, "w").map(parse_measurement).transpose()?;
    let debug = jattr(json, "debug").map(|v| v == "true").unwrap_or(false);

    let mut children = Vec::new();
    if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
        for n in arr {
            children.push(resolve_parsed_node(
                parse_tree_node(n, tokens, asset_images)?,
                "Helvetica", 11.0, asset_fonts,
            ));
        }
    }
    Ok(LayoutRegion { pin, page, w, children, debug })
}

fn parse_tree_node(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
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
        "barcode" => NodeKind::Barcode,
        "field"   => NodeKind::Field,
        "table"   => NodeKind::Table,
        "thead"   => NodeKind::TableHead,
        "tr"      => NodeKind::TableRow,
        "td"      => NodeKind::TableCell,
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
                "center"  => TextAlign::Center,
                "right"   => TextAlign::Right,
                "justify" => TextAlign::Justify,
                _         => TextAlign::Left,
            };

            if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
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
                            .get("nodes").and_then(|v| v.as_array())
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
        NodeKind::Barcode => {
            apply_barcode_attrs(&mut node, &a, tokens)?;
        }
        NodeKind::Field => {
            apply_field_attrs(&mut node, &a, tokens)?;
        }
        _ => {}
    }

    // ── Layout children ───────────────────────────────────────────────────────
    if !matches!(kind, NodeKind::Divider | NodeKind::Img | NodeKind::Barcode | NodeKind::Field) {
        if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
            for child in arr {
                let child_node = parse_tree_node(child, tokens, asset_images)?;
                match kind {
                    NodeKind::Table => {
                        if !matches!(child_node.kind, NodeKind::TableHead | NodeKind::TableRow) {
                            return Err(format!(
                                "table children must be 'thead' or 'tr', got '{}'",
                                child.get("type").and_then(|v| v.as_str()).unwrap_or("?")
                            ));
                        }
                    }
                    NodeKind::TableHead | NodeKind::TableRow => {
                        if child_node.kind != NodeKind::TableCell {
                            return Err(format!(
                                "thead/tr children must be 'td', got '{}'",
                                child.get("type").and_then(|v| v.as_str()).unwrap_or("?")
                            ));
                        }
                    }
                    _ => {}
                }
                node.children.push(child_node);
            }
        }
        if kind == NodeKind::Frame && node.children.len() > 1 {
            return Err(format!(
                "frame accepts at most one child; got {}",
                node.children.len()
            ));
        }
    }

    Ok(node)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal(body: &str) -> String {
        if body.is_empty() {
            r#"<lpdf version="1"><document size="a4" margin="28pt"><section><layout/></section></document></lpdf>"#.to_string()
        } else {
            format!(
                r#"<lpdf version="1"><document size="a4" margin="28pt"><section><layout>{body}</layout></section></document></lpdf>"#
            )
        }
    }

    #[test]
    fn parse_empty_page() {
        let mut doc = parse(&minimal("")).unwrap();
        let pages = doc.section_layouts();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].width, 595.28);
        assert_eq!(pages[0].height, 841.89);
        assert_eq!(pages[0].margin, [28.0; 4]);
    }

    #[test]
    fn parse_frame_with_background() {
        let mut doc = parse(&minimal(r#"<frame background="primary" />"#)).unwrap();
        let pages = doc.section_layouts();
        let node = &pages[0].children[0];
        assert_eq!(node.background.as_deref(), Some("#1763cf"));
    }

    #[test]
    fn parse_stack_gap_and_children() {
        let mut doc = parse(&minimal(
            r#"<stack gap="m"><frame /><frame /></stack>"#,
        ))
        .unwrap();
        let pages = doc.section_layouts();
        let stack = &pages[0].children[0];
        assert_eq!(stack.gap, 8.0);
        assert_eq!(stack.children.len(), 2);
    }

    #[test]
    fn parse_divider() {
        let mut doc = parse(&minimal(r##"<divider color="#e0e0e0" thickness="xs" />"##)).unwrap();
        let pages = doc.section_layouts();
        let d = &pages[0].children[0];
        assert_eq!(d.kind, NodeKind::Divider);
        assert_eq!(d.thickness, 0.5);
        assert_eq!(d.color.as_deref(), Some("#e0e0e0"));
    }

    #[test]
    fn parse_grid_cols() {
        let mut doc = parse(&minimal(r#"<grid cols="3" gap="m" />"#)).unwrap();
        let pages = doc.section_layouts();
        let g = &pages[0].children[0];
        assert_eq!(g.cols, 3);
    }

    #[test]
    fn parse_text_node() {
        let mut doc = parse(&minimal(r#"<text font-size="m" color="text">Hello world</text>"#)).unwrap();
        let pages = doc.section_layouts();
        let t = &pages[0].children[0];
        assert_eq!(t.kind, NodeKind::Text);
        assert_eq!(t.font_size, 11.0);
        assert_eq!(t.text_runs.len(), 1);
        assert_eq!(t.text_runs[0].text, "Hello world");
    }

    #[test]
    fn font_inheritance_from_document() {
        let xml = r#"<lpdf version="1">
            <assets>
                <font name="body" core="Helvetica-Oblique"/>
            </assets>
            <document size="a4" font="body"><section><layout>
                <text>Hello</text>
            </layout></section></document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let pages = doc.section_layouts();
        let t = &pages[0].children[0];
        assert_eq!(t.font, "Helvetica-Oblique");
    }

    #[test]
    fn font_size_inheritance() {
        let xml = r#"<lpdf version="1">
            <document size="a4"><section><layout>
                <stack font-size="14pt"><text>Hello</text></stack>
            </layout></section></document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let pages = doc.section_layouts();
        let stack = &pages[0].children[0];
        let text  = &stack.children[0];
        assert_eq!(text.font_size, 14.0);
    }

    #[test]
    fn img_node_registered() {
        let xml = r#"<lpdf version="1">
            <assets>
                <image name="logo"/>
            </assets>
            <document size="a4"><section><layout>
                <img name="logo" width="100pt"/>
            </layout></section></document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let pages = doc.section_layouts();
        let img = &pages[0].children[0];
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
        assert!(result.unwrap_err().contains("unknown"));
    }

    #[test]
    fn custom_tokens_override_defaults() {
        let xml = r##"<lpdf version="1">
            <tokens>
                <colors>
                    <color name="primary" value="#ff0000" />
                </colors>
            </tokens>
            <document size="a4"><section><layout><frame background="primary" /></layout></section></document>
        </lpdf>"##;
        let mut doc = parse(xml).unwrap();
        assert_eq!(
            doc.section_layouts()[0].children[0].background.as_deref(),
            Some("#ff0000")
        );
    }

    #[test]
    fn landscape_swaps_dimensions() {
        let xml = r#"<lpdf version="1"><document size="a4" orientation="landscape"><section><layout/></section></document></lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let pages = doc.section_layouts();
        assert_eq!(pages[0].width, 841.89);
        assert_eq!(pages[0].height, 595.28);
    }

    #[test]
    fn height_fixed_pt() {
        let mut doc = parse(&minimal(r#"<frame height="28pt" />"#)).unwrap();
        assert_eq!(doc.section_layouts()[0].children[0].height_mode, HeightMode::Fixed(28.0));
    }

    #[test]
    fn height_fill_mode() {
        let mut doc = parse(&minimal(r#"<frame height="fill" />"#)).unwrap();
        assert_eq!(doc.section_layouts()[0].children[0].height_mode, HeightMode::Fill);
    }

    // ── New Phase-2 unit tests ──────────────────────────────────────────────

    // §5.2.1 parse_measurement
    #[test]
    fn measurement_pt() {
        assert!((parse_measurement("72pt").unwrap() - 72.0).abs() < 0.01);
    }
    #[test]
    fn measurement_mm() {
        let v = parse_measurement("25.4mm").unwrap();
        assert!((v - 72.0).abs() < 0.01, "25.4mm should be ~72pt, got {v}");
    }
    #[test]
    fn measurement_in() {
        let v = parse_measurement("1in").unwrap();
        assert!((v - 72.0).abs() < 0.01, "1in should be 72pt, got {v}");
    }
    #[test]
    fn measurement_invalid() {
        assert!(parse_measurement("abc").is_err());
    }

    // §5.2.2 parse_page_scope keywords
    #[test]
    fn page_scope_keyword_each()  { assert_eq!(parse_page_scope("each").unwrap(),  PageScope::Each); }
    #[test]
    fn page_scope_keyword_first() { assert_eq!(parse_page_scope("first").unwrap(), PageScope::First); }
    #[test]
    fn page_scope_keyword_last()  { assert_eq!(parse_page_scope("last").unwrap(),  PageScope::Last); }
    #[test]
    fn page_scope_keyword_odd()   { assert_eq!(parse_page_scope("odd").unwrap(),   PageScope::Odd); }
    #[test]
    fn page_scope_keyword_even()  { assert_eq!(parse_page_scope("even").unwrap(),  PageScope::Even); }

    // §5.2.3 parse_page_scope numeric
    #[test]
    fn page_scope_single_number() {
        assert_eq!(
            parse_page_scope("1").unwrap(),
            PageScope::Pages(vec![PageRange { start: 1, end: Some(1) }])
        );
    }
    #[test]
    fn page_scope_range() {
        assert_eq!(
            parse_page_scope("2-4").unwrap(),
            PageScope::Pages(vec![PageRange { start: 2, end: Some(4) }])
        );
    }
    #[test]
    fn page_scope_last_range() {
        assert_eq!(
            parse_page_scope("3-last").unwrap(),
            PageScope::Pages(vec![PageRange { start: 3, end: None }])
        );
    }
    #[test]
    fn page_scope_comma_list() {
        let ps = parse_page_scope("1,3-5").unwrap();
        assert_eq!(ps, PageScope::Pages(vec![
            PageRange { start: 1, end: Some(1) },
            PageRange { start: 3, end: Some(5) },
        ]));
    }

    // §5.2.4 parse_anchor
    #[test]
    fn anchor_all_values() {
        use crate::canvas::{Anchor, parse_anchor};
        let pairs = [
            ("top-left",      Anchor::TopLeft),
            ("top-center",    Anchor::TopCenter),
            ("top-right",     Anchor::TopRight),
            ("center-left",   Anchor::CenterLeft),
            ("center",        Anchor::Center),
            ("center-right",  Anchor::CenterRight),
            ("bottom-left",   Anchor::BottomLeft),
            ("bottom-center", Anchor::BottomCenter),
            ("bottom-right",  Anchor::BottomRight),
        ];
        for (s, expected) in pairs {
            assert_eq!(parse_anchor(s).unwrap(), expected, "anchor '{s}'");
        }
    }
    #[test]
    fn anchor_invalid() {
        use crate::canvas::parse_anchor;
        assert!(parse_anchor("middle").is_err());
    }

    // §5.2.5 XML section parsing
    #[test]
    fn parse_section_layout_only() {
        let xml = r#"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <layout><stack gap="m"><text>Hello</text></stack></layout>
                </section>
            </document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.sections.len(), 1);
        let sc = &doc.sections[0];
        assert_eq!(sc.children.len(), 1);
        assert!(matches!(sc.children[0], SectionChild::Layout(_)));
    }

    #[test]
    fn parse_section_canvas_only() {
        let xml = r##"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <canvas>
                        <layer><rect w="100pt" h="50pt" fill="#ff0000"/></layer>
                    </canvas>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.sections.len(), 1);
        assert!(matches!(doc.sections[0].children[0], SectionChild::Canvas(_)));
        if let SectionChild::Canvas(ref cv) = doc.sections[0].children[0] {
            assert_eq!(cv.layers.len(), 1);
            assert_eq!(cv.layers[0].children.len(), 1);
        }
    }

    #[test]
    fn parse_section_layout_plus_canvas() {
        let xml = r##"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <layout><text>Body</text></layout>
                    <canvas><layer page="each"><rect w="100pt" h="5pt" fill="#000000"/></layer></canvas>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.sections[0].children.len(), 2);
    }

    #[test]
    fn parse_multi_section() {
        let xml = r##"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section title="Cover"><canvas><layer><rect w="100pt" h="100pt" fill="#000"/></layer></canvas></section>
                <section title="Body"><layout><text>Hello</text></layout></section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.sections.len(), 2);
        assert_eq!(doc.sections[0].options.title.as_deref(), Some("Cover"));
        assert_eq!(doc.sections[1].options.title.as_deref(), Some("Body"));
    }

    // §5.2.6 region parsing
    #[test]
    fn parse_layout_region_top() {
        let xml = r#"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <layout>
                        <region pin="top" page="each"><text>Header</text></region>
                        <text>Body</text>
                    </layout>
                </section>
            </document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        let layout = match &doc.sections[0].children[0] {
            SectionChild::Layout(l) => l,
            _ => panic!("expected layout"),
        };
        assert_eq!(layout.len(), 2);
        assert!(matches!(layout[0], LayoutChild::Region(_)));
        if let LayoutChild::Region(ref r) = layout[0] {
            assert_eq!(r.pin, RegionPin::Top);
            assert_eq!(r.page, Some(PageScope::Each));
        }
    }

    #[test]
    fn section_layouts_from_section() {
        let xml = r#"<lpdf version="1">
            <document size="a4" margin="28pt">
                <section>
                    <layout><text>Hello</text></layout>
                </section>
            </document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let pages = doc.section_layouts();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].children.len(), 1);
        assert_eq!(pages[0].children[0].kind, NodeKind::Text);
    }

    // §5.2.8 canvas layer page scope
    #[test]
    fn canvas_layer_page_scope() {
        let xml = r##"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <canvas>
                        <layer page="odd"><rect w="10pt" h="10pt" fill="#000"/></layer>
                        <layer page="2-4"><rect w="10pt" h="10pt" fill="#000"/></layer>
                    </canvas>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        if let SectionChild::Canvas(ref cv) = doc.sections[0].children[0] {
            assert_eq!(cv.layers[0].page, Some(PageScope::Odd));
            assert_eq!(cv.layers[1].page, Some(PageScope::Pages(vec![PageRange { start: 2, end: Some(4) }])));
        } else {
            panic!("expected canvas");
        }
    }

    // §5.2.9 canvas primitives
    #[test]
    fn canvas_rect_absolute_pos() {
        use crate::canvas::CanvasPrimitive;
        let xml = r##"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <canvas>
                        <layer>
                            <rect x="10pt" y="20pt" w="100pt" h="50pt" fill="#ff0000" radius="4pt" opacity="0.5"/>
                        </layer>
                    </canvas>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        if let SectionChild::Canvas(ref cv) = doc.sections[0].children[0] {
            if let CanvasPrimitive::Rect(ref r) = cv.layers[0].children[0] {
                assert!((r.x - 10.0).abs() < 0.01);
                assert!((r.y - 20.0).abs() < 0.01);
                assert!((r.w - 100.0).abs() < 0.01);
                assert_eq!(r.fill.as_deref(), Some("#ff0000"));
                assert!((r.radius.unwrap() - 4.0).abs() < 0.01);
            } else { panic!("expected rect"); }
        } else { panic!("expected canvas"); }
    }

    #[test]
    fn canvas_rect_anchored_pos() {
        use crate::canvas::CanvasPrimitive;
        let xml = r##"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <canvas>
                        <layer>
                            <rect anchor="center" x="10pt" y="-5pt" w="50pt" h="20pt" fill="#000"/>
                        </layer>
                    </canvas>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        if let SectionChild::Canvas(ref cv) = doc.sections[0].children[0] {
            if let CanvasPrimitive::Rect(ref r) = cv.layers[0].children[0] {
                // A4 center = (297.64, 420.945), dx=10, dy=-5 → anchor point (307.64, 415.945)
                // rect is 50×20pt, so top-left = anchor - (w/2, h/2) = (282.64, 405.945)
                assert!((r.x - 282.64).abs() < 0.5);
                assert!((r.y - 405.94).abs() < 0.5);
            } else { panic!("expected rect"); }
        } else { panic!("expected canvas"); }
    }

    #[test]
    fn canvas_text_with_spans() {
        use crate::canvas::CanvasPrimitive;
        let xml = r#"<lpdf version="1">
            <document size="a4" margin="0pt">
                <section>
                    <canvas>
                        <layer>
                            <text anchor="center" font-size="12pt">Hello <span font="Helvetica-Bold">world</span></text>
                        </layer>
                    </canvas>
                </section>
            </document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        if let SectionChild::Canvas(ref cv) = doc.sections[0].children[0] {
            if let CanvasPrimitive::Text(ref t) = cv.layers[0].children[0] {
                // A4 center = (297.64, 420.945)
                assert!((t.x - 297.64).abs() < 0.5);
                assert!((t.y - 420.94).abs() < 0.5);
                assert!((t.font_size.unwrap() - 12.0).abs() < 0.01);
                assert_eq!(t.runs.len(), 1);
                assert_eq!(t.runs[0].font.as_deref(), Some("Helvetica-Bold"));
            } else { panic!("expected text"); }
        } else { panic!("expected canvas"); }
    }

    // ── Error quality tests ───────────────────────────────────────────────────
    // Each test asserts: element name, line/col position, and the specific
    // problem are all present — proving the message is self-sufficient.

    // Category 1 — Unknown element

    #[test]
    fn err_unknown_element() {
        let err = parse(&minimal("<box />")).unwrap_err();
        assert!(err.contains("<box>"),    "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("unknown"), "{err}");
    }

    // Category 2 — Invalid attribute value

    #[test]
    fn err_invalid_align() {
        let err = parse(&minimal(r#"<stack align="middle" />"#)).unwrap_err();
        assert!(err.contains("<stack>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("middle"),   "{err}");
        assert!(err.contains("start"),    "{err}"); // allowed values present
    }

    #[test]
    fn err_invalid_justify() {
        let err = parse(&minimal(r#"<stack justify="around" />"#)).unwrap_err();
        assert!(err.contains("<stack>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("around"),   "{err}");
        assert!(err.contains("start"),    "{err}"); // allowed values present
    }

    #[test]
    fn err_invalid_height() {
        let err = parse(&minimal(r#"<frame height="big" />"#)).unwrap_err();
        assert!(err.contains("<frame>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("big"),      "{err}");
        assert!(err.contains("fill"),     "{err}"); // allowed values present
    }

    #[test]
    fn err_invalid_paginate() {
        let err = parse(&minimal(r#"<stack paginate="always" />"#)).unwrap_err();
        assert!(err.contains("<stack>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("always"),   "{err}");
        assert!(err.contains("break-before"), "{err}"); // allowed values present
    }

    #[test]
    fn err_cluster_rejects_justify_between() {
        let err = parse(&minimal(r#"<cluster justify="between" />"#)).unwrap_err();
        assert!(err.contains("<cluster>"), "{err}");
        assert!(err.contains("at line"),  "{err}");
        assert!(err.contains("between"),   "{err}");
    }

    #[test]
    fn err_frame_rejects_gap() {
        let err = parse(&minimal(r#"<frame gap="m" />"#)).unwrap_err();
        assert!(err.contains("<frame>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("gap"),      "{err}");
    }

    #[test]
    fn err_barcode_invalid_type() {
        let err = parse(&minimal(r#"<barcode type="pdf417" data="x" />"#)).unwrap_err();
        assert!(err.contains("<barcode>"), "{err}");
        assert!(err.contains("at line"),  "{err}");
        assert!(err.contains("pdf417"),    "{err}");
        assert!(err.contains("qr"),        "{err}"); // allowed values present
    }

    #[test]
    fn err_barcode_invalid_ec() {
        let err = parse(&minimal(r#"<barcode type="qr" data="x" size="100pt" ec="Z" />"#)).unwrap_err();
        assert!(err.contains("<barcode>"), "{err}");
        assert!(err.contains("at line"),  "{err}");
        assert!(err.contains("Z"),         "{err}");
        assert!(err.contains("L"),         "{err}"); // allowed values present
    }

    #[test]
    fn err_field_invalid_type() {
        let err = parse(&minimal(r#"<field type="slider" name="x" />"#)).unwrap_err();
        assert!(err.contains("<field>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("slider"),   "{err}");
        assert!(err.contains("text"),     "{err}"); // allowed values present
    }

    #[test]
    fn err_region_invalid_pin() {
        let xml = r#"<lpdf version="1"><document size="a4" margin="28pt">
  <section><layout>
    <region pin="center" />
  </layout></section></document></lpdf>"#;
        let err = parse(xml).unwrap_err();
        assert!(err.contains("<region>"), "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("center"),   "{err}");
        assert!(err.contains("top"),      "{err}"); // allowed values present
    }

    // Category 3 — Missing required attribute

    #[test]
    fn err_missing_attr_img_name() {
        let err = parse(&minimal("<img />")).unwrap_err();
        assert!(err.contains("<img>"),    "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("name"),     "{err}");
    }

    #[test]
    fn err_missing_attr_barcode_type() {
        let err = parse(&minimal("<barcode />")).unwrap_err();
        assert!(err.contains("<barcode>"), "{err}");
        assert!(err.contains("at line"),  "{err}");
        assert!(err.contains("type"),      "{err}");
    }

    #[test]
    fn err_missing_attr_barcode_size() {
        let err = parse(&minimal(r#"<barcode type="qr" data="x" />"#)).unwrap_err();
        assert!(err.contains("<barcode>"), "{err}");
        assert!(err.contains("at line"),  "{err}");
        assert!(err.contains("size"),      "{err}");
    }

    #[test]
    fn err_missing_attr_field_type() {
        let err = parse(&minimal("<field />")).unwrap_err();
        assert!(err.contains("<field>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("type"),     "{err}");
    }

    #[test]
    fn err_missing_attr_field_name() {
        let err = parse(&minimal(r#"<field type="text" />"#)).unwrap_err();
        assert!(err.contains("<field>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("name"),     "{err}");
    }

    #[test]
    fn err_missing_attr_link_href() {
        let err = parse(&minimal("<link />")).unwrap_err();
        assert!(err.contains("<link>"),   "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("href"),     "{err}");
    }

    #[test]
    fn err_missing_attr_region_pin() {
        let xml = r#"<lpdf version="1"><document size="a4" margin="28pt">
  <section><layout>
    <region />
  </layout></section></document></lpdf>"#;
        let err = parse(xml).unwrap_err();
        assert!(err.contains("<region>"), "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("pin"),      "{err}");
    }

    #[test]
    fn err_missing_attr_asset_font_name() {
        let xml = r#"<lpdf version="1">
  <assets><font core="Helvetica" /></assets>
  <document size="a4"><section><layout /></section></document>
</lpdf>"#;
        let err = parse(xml).unwrap_err();
        assert!(err.contains("<font>"),   "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("name"),     "{err}");
    }

    // Category 4 — Wrong nesting / structural

    #[test]
    fn err_frame_too_many_children() {
        let err = parse(&minimal("<frame><stack /><stack /></frame>")).unwrap_err();
        assert!(err.contains("<frame>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
    }

    #[test]
    fn err_table_bad_child() {
        let err = parse(&minimal("<table><stack /></table>")).unwrap_err();
        assert!(err.contains("<table>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("stack"),    "{err}");
    }

    #[test]
    fn err_tr_bad_child() {
        let err = parse(&minimal("<table><tr><stack /></tr></table>")).unwrap_err();
        assert!(err.contains("<tr>"),     "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("stack"),    "{err}");
    }

    // Category 5 — Asset reference

    #[test]
    fn err_img_unknown_asset() {
        let err = parse(&minimal(r#"<img name="ghost" />"#)).unwrap_err();
        assert!(err.contains("<img>"),    "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("ghost"),    "{err}");
    }
}
