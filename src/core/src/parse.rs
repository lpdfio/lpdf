use std::collections::{HashMap, HashSet};
use crate::tokens::{parse_factor, parse_opacity, parse_pt, FontDef, FontWidths, Gradient, GradientSpec, Tokens};
use crate::canvas;
use crate::markdown;
use crate::layout::{Held, Placed};

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
    pub background:  Option<String>,
    /// The gradients of the tokens, which a background or a fill names as `gradient:name`.
    pub gradients:   HashMap<String, Gradient>,
    /// The color tokens, which a bound color (`data-color`) is checked against after binding.
    pub colors:      HashMap<String, String>,
    pub debug:       bool,
    /// Where footnote numbers start over, for the sections that don't say (`footnote-counter`).
    pub footnote_counter: Option<FootnoteCounter>,
    /// Sections — always ≥ 1.  Single-section shorthand is normalised here.
    pub sections:    Vec<Section>,
}

impl Document {
    /// Derive a flat `Vec<SectionLayout>` from sections for use with the layout engine.
    /// Each section's layout child is converted to one `SectionLayout`.  Its layers are
    /// collected into `underlays` (before the layout) or `overlays` (after) per document order.
    pub fn section_layouts(&mut self) -> Vec<SectionLayout> {
        // A Markdown that came from data nobody supplied shows the text it stands in with. It was read with the
        // document, so this does not fail.
        let _ = markdown::expand_pending(&mut self.sections, None, markdown::Pass::Rest);
        let mut result = Vec::new();
        let mut document_notes = 0u32;
        for section in std::mem::take(&mut self.sections) {
            let width  = section.options.size.map(|(w, _)| w).unwrap_or(self.page_width);
            let height = section.options.size.map(|(_, h)| h).unwrap_or(self.page_height);
            let bg     = section.options.background
                            .or_else(|| self.background.clone());
            let debug  = section.options.debug.unwrap_or(self.debug);

            // Collect layout nodes including page_scope chrome nodes, and the layers.
            // Layers before the <layout> become underlays; after → overlays.
            let mut children: Vec<Node> = Vec::new();
            let mut underlays: Vec<canvas::CanvasLayer> = Vec::new();
            let mut overlays:  Vec<canvas::CanvasLayer> = Vec::new();
            // Only the flow keeps to a margin, so a section of layers alone has none.
            let mut margin = [0.0_f32; 4];
            let mut gap = 0.0_f32;
            let mut seen_layout = false;
            for sc in section.children {
                match sc {
                    SectionChild::Layout { margin: layout_margin, gap: layout_gap, children: layout_children } => {
                        seen_layout = true;
                        margin = layout_margin;
                        gap = layout_gap;
                        // The layout takes the children in front of the flow as the header and the ones behind it
                        // as the footer, so `pin` is applied here, wherever the region is written.
                        let (mut top, mut bottom, mut middle) = (Vec::new(), Vec::new(), Vec::new());
                        for lc in layout_children {
                            match lc {
                                LayoutChild::Content(node) => middle.push(node),
                                LayoutChild::Region(reg) => match reg.pin {
                                    RegionPin::Top    => top.push(region_to_compat_node(reg)),
                                    RegionPin::Bottom => bottom.push(region_to_compat_node(reg)),
                                    // Not implemented: stays where it is written.
                                    RegionPin::Left | RegionPin::Right => middle.push(region_to_compat_node(reg)),
                                },
                            }
                        }
                        children.append(&mut top);
                        children.append(&mut middle);
                        children.append(&mut bottom);
                    }
                    SectionChild::Layer(layer) => {
                        if seen_layout {
                            overlays.push(layer);
                        } else {
                            underlays.push(layer);
                        }
                    }
                }
            }
            // The items of a list are known once the data is in, so the lists become columns and rows here. A table
            // of contents section is bound after it is laid out, and gets its lists then (`toc.rs`).
            if section.options.toc.is_none() {
                crate::lists::lower_all(&mut children);
            }
            // The numbers of a section that counts across the document go on from the notes the earlier ones that
            // do hold, so they are counted here, once the data is in.
            let footnote_counter = section.options.footnote_counter.or(self.footnote_counter).unwrap_or_default();
            let footnote_start = document_notes;
            if footnote_counter == FootnoteCounter::Document {
                document_notes += children.iter().map(count_footnotes).sum::<u32>();
            }
            result.push(SectionLayout {
                width, height, margin, gap, background: bg, debug, children, underlays, overlays,
                bookmark: section.bookmark,
                toc: section.options.toc,
                footnote_counter, footnote_start,
            });
        }
        result
    }
}

/// Convert a `LayoutRegion` to a `Node` that the layout engine can handle.
/// The `page_scope` is passed through directly so all PageScope variants work correctly.
fn region_to_compat_node(reg: LayoutRegion) -> Node {
    Node {
        kind:       NodeKind::Column,
        page_scope: Some(reg.page.unwrap_or(PageScope::Each)),
        debug:      reg.debug,
        children:   reg.children,
        ..Node::layout_default()
    }
}

// ── Section types ─────────────────────────────────────────────────────────────

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
    /// Top and bottom decide where `section_layouts` puts the region; left and right are not implemented.
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
    /// The flow, the margin it keeps from the page edges (top, right, bottom, left) and the space between the
    /// nodes that stand directly in it, which only a `<markdown>` sets.
    Layout { margin: [f32; 4], gap: f32, children: Vec<LayoutChild> },
    Layer(canvas::CanvasLayer),
}

/// The margin of a `<layout>` that sets none: half an inch on every side.
const DEFAULT_LAYOUT_MARGIN: f32 = 36.0;

/// What to write instead of `margin` on a document or a section, which no longer have one.
const MARGIN_MOVED: &str = "margin moved to <layout>; set it there, in each section: only the flowing content keeps to a margin";

/// Per-section overrides; `None` fields inherit document-level defaults.
#[derive(Debug, Clone, Default)]
pub struct SectionOptions {
    pub size:       Option<(f32, f32)>,
    pub background: Option<String>,
    pub debug:      Option<bool>,
    /// Parsed but not yet surfaced to the render pipeline.
    #[allow(dead_code)]
    pub title:      Option<String>,
    /// Set on the one `<section type="toc">` of a document.
    pub toc:        Option<TocOptions>,
    pub footnote_counter: Option<FootnoteCounter>,
}

/// What a table of contents section needs once the other sections are laid out.
#[derive(Debug, Clone)]
pub struct TocOptions {
    /// Left padding added to an entry per level below the first (`toc-indent-step`).
    pub indent_step: f32,
    /// The caller's data. The TOC section is bound after layout, with `$toc` added to it.
    pub data:        Option<serde_json::Value>,
}

/// The key the bookmark list takes in the data a TOC section is bound with. It replaces a `$toc` key of the
/// caller's own.
pub const TOC_DATA_KEY: &str = "$toc";

/// Default `toc-indent-step`.
const DEFAULT_TOC_INDENT_STEP: f32 = 12.0;

/// A content boundary with its own auto-pagination.
#[derive(Debug, Clone)]
pub struct Section {
    /// In document order: first child is painted first (bottom).
    pub children: Vec<SectionChild>,
    pub options:  SectionOptions,
    /// A `<bookmark>` directly inside `<section>`, which points at the section's first page.
    pub bookmark: Option<Bookmark>,
}

/// An entry of the PDF outline and a named destination for `href="#id"` links.
///
/// A `<bookmark>` in a layout is carried by the element that follows it, so it lands on the page where
/// that element starts.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub id:          String,
    pub label:       String,
    /// Outline depth, 1 to 6.
    pub level:       u8,
    /// `true` when the id came from `id=`, `false` when it was made from the label and may take a suffix.
    pub explicit_id: bool,
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
    pub margin: [f32; 4], // the layout's: top, right, bottom, left
    /// The space between the nodes that stand directly in the flow.
    pub gap: f32,
    pub background: Option<String>,
    pub debug: bool,
    pub children: Vec<Node>,
    pub underlays: Vec<canvas::CanvasLayer>,
    pub overlays: Vec<canvas::CanvasLayer>,
    pub bookmark: Option<Bookmark>,
    pub toc: Option<TocOptions>,
    pub footnote_counter: FootnoteCounter,
    /// How many notes the sections before this one that count across the document hold: its own numbering
    /// goes on from there when it counts across the document too.
    pub footnote_start: u32,
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
    /// Set on a `<span bold>` while parsing. `resolve_node` folds it into `font` and the field is `false`
    /// from then on.
    pub bold:         bool,
    /// The tint drawn behind the run: the color of a `<code-span>`. `None` draws nothing.
    pub background:   Option<String>,
    /// Set on the run an `<fn>` stands for: the anchor, a raised number, with the note it points at. The run has
    /// no text of its own.
    pub footnote:     Option<Box<Footnote>>,
}

/// The note of an `<fn>`, which the page where its anchor lands shows at the bottom.
#[derive(Debug, Clone)]
pub struct Footnote {
    /// The words of the note: plain runs and spans.
    pub runs:   Vec<TextRun>,
    /// The number the anchor and the note show. Layout sets it, from the counter and the page.
    pub number: u32,
}

/// Where the numbers of the footnotes start over (`footnote-counter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FootnoteCounter {
    Page,
    #[default]
    Section,
    Document,
}

impl FootnoteCounter {
    fn parse(v: &str) -> Result<Self, String> {
        match v {
            "page"     => Ok(Self::Page),
            "section"  => Ok(Self::Section),
            "document" => Ok(Self::Document),
            other      => Err(format!("invalid footnote-counter '{other}': expected page, section or document")),
        }
    }
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
    /// The attributes that take their value from the data, with the path each reads (`data-href="url"`).
    pub bound:       Vec<(BoundAttr, String)>,
}

/// An attribute that can take its value from the data. Each is written as `data-` and its name, and sets the
/// field that the attribute of that name sets on the element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundAttr {
    /// `data-href` on a `link`.
    Href,
    /// `data-name` on an `img` or `img-at`: one of the images declared in `<assets>`.
    Name,
    /// `data-color`: the text color of a text or box, the line of a divider, the bars of a barcode.
    Color,
    /// `data-background`: the fill behind a box, or behind a barcode.
    Background,
    /// `data-fill` on a `shape`.
    Fill,
}

impl BoundAttr {
    /// Every bindable attribute, in the order they are read and bound.
    pub const ALL: [BoundAttr; 5] = [Self::Href, Self::Name, Self::Color, Self::Background, Self::Fill];

    /// The attribute as written in a document.
    pub fn data_attribute(self) -> &'static str {
        match self {
            Self::Href       => "data-href",
            Self::Name       => "data-name",
            Self::Color      => "data-color",
            Self::Background => "data-background",
            Self::Fill       => "data-fill",
        }
    }
}

impl BoundAttr {
    /// Whether an element of this kind has the attribute to bind: `data-color` on a `shape` has no `color` to set.
    pub fn applies_to(self, kind: &NodeKind) -> bool {
        match self {
            Self::Href       => *kind == NodeKind::Link,
            Self::Name       => *kind == NodeKind::Img,
            Self::Color      => matches!(
                kind,
                NodeKind::Text | NodeKind::Column | NodeKind::Row | NodeKind::Grid | NodeKind::Frame
                    | NodeKind::TableCell | NodeKind::List | NodeKind::ListItem | NodeKind::Divider
                    | NodeKind::Barcode
            ),
            Self::Background => matches!(
                kind,
                NodeKind::Column | NodeKind::Row | NodeKind::Grid | NodeKind::Frame | NodeKind::TableCell
                    | NodeKind::List | NodeKind::ListItem | NodeKind::Barcode
            ),
            Self::Fill       => *kind == NodeKind::Shape,
        }
    }
}

/// Refuse a binding on an element that has no such attribute, when the document is read, not on the first row
/// of data that would have used it.
fn check_bound_attrs(kind: &NodeKind, bound: &[(BoundAttr, String)]) -> Result<(), String> {
    match bound.iter().find(|(attr, _)| !attr.applies_to(kind)) {
        Some((attr, _)) => Err(format!("'{}' does not apply to this element", attr.data_attribute())),
        None            => Ok(()),
    }
}

/// The `data-<attr>` bindings an element carries, in `BoundAttr::ALL` order.
fn read_bound_attrs(a: &impl Attrs) -> Vec<(BoundAttr, String)> {
    BoundAttr::ALL
        .iter()
        .filter_map(|&attr| a.get(attr.data_attribute()).map(|path| (attr, path.to_owned())))
        .collect()
}

/// Which of the texts captured under a name on a page a `capture-ref` text shows.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CaptureMode {
    First,
    #[default]
    Last,
}

/// A text that shows what was captured under `key`, as of the page it is drawn on: the `mode`'s pick of what the
/// page captured, else the latest value of an earlier page, else `fallback`.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureRef {
    pub key:      String,
    pub mode:     CaptureMode,
    pub fallback: String,
}

/// What a text records or shows for a running header: `capture` or `capture-ref`. Few texts have one, so a node
/// holds it boxed (see `Node`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capture {
    /// The name the text records its words under, as the pages are filled, for a `capture-ref` text to show.
    pub key: Option<String>,
    /// What the text says, set by the layout before it paginates; a text that splits across pages keeps it whole.
    pub words: Option<String>,
    /// A text that shows what was captured, not words of its own.
    pub sink: Option<CaptureRef>,
    /// The lines a `capture-ref` text always takes, whatever it shows, so a header's height is known before the
    /// pages are filled. 0 for every other text.
    pub reserve_lines: usize,
}

/// What a `<shape>` draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShapeKind {
    Rect,
    Circle,
    Ellipse,
    /// The marker of a checked or an open `<li>`: a rounded outline, and a tick in it when `Shape::tick` has a color.
    /// Only a list makes one, so a `<shape>` can't ask for it.
    Checkbox,
}

/// What a `<shape>` draws in its box, which the node's `width_constraint` and `img_height_constraint` size (a
/// circle's is the square of side 2r) and its `radius` and `opacity` shape. Few nodes are shapes, so a node holds
/// it boxed (see `Node`).
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub kind:         ShapeKind,
    pub fill:         Option<String>,
    pub stroke:       Option<String>,
    pub stroke_width: Option<f32>,
    pub stroke_dash:  Option<String>,
    /// The color of the tick of a checkbox that is checked.
    pub tick:         Option<String>,
}

/// How a `<list>` numbers its items.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ListFormat {
    #[default]
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

/// The shape that marks the items of a bullet `<list>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListBullet {
    Disc,
    Circle,
    Square,
    Dash,
}

/// The width of a list's marker column when `indent` is not set.
pub const DEFAULT_LIST_INDENT: f32 = 24.0;

/// The `gap` of an `<li>` that sets none: below any gap a document can write, so `lists::lower` can tell it from
/// `gap="0"` and give the content of the item the gap of its list.
pub const LIST_ITEM_GAP_UNSET: f32 = -1.0;

/// How a `<list>` marks its items. `lists::lower` reads it, once the data is in and the items are known, and turns
/// the list into the columns and rows the layout knows. Few nodes are lists, so a node holds it boxed (see `Node`).
#[derive(Debug, Clone, PartialEq)]
pub struct ListSpec {
    /// A number list, not a bullet list.
    pub numbered: bool,
    pub format:   ListFormat,
    /// The number of the first item of a number list.
    pub start:    i64,
    pub prefix:   String,
    pub suffix:   String,
    /// The shape of a bullet list's marker; `None` takes one by how deep the list is.
    pub bullet:   Option<ListBullet>,
    /// The width of the marker column.
    pub indent:   f32,
}

/// What an `<li checked>` asks of its marker: a checkbox, ticked or open, in place of the bullet or the number. The
/// outline is the `line` color, which `lists::lower` can't look up, so the parser keeps it here.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub checked: bool,
    pub line:    String,
}

/// What only a shape, a list or a list item adds to a node. They share one box, so the pointer costs a node 8 bytes and not 16:
/// `a_node_stays_small` holds the size.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    Shape(Shape),
    List(ListSpec),
    Check(Check),
    /// A `<markdown>` until `markdown::expand` turns it into the nodes of the layout.
    Markdown(Box<crate::markdown::MarkdownSpec>),
}

/// What sets a table's rows apart: the background of every other row, and a line, as a thickness and a color, under
/// the header row and under each of the other rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableStyle {
    pub stripe: Option<String>,
    pub head_border: Option<(f32, String)>,
    pub row_border: Option<(f32, String)>,
}

/// One element of the layout. Every node has every field, whatever its kind, and parse and data binding copy
/// nodes whole, so a field costs its size in every node of a document, set or not: a larger node is a slower
/// parse. A new field that is a number, a flag or a small enum goes inline. A string, a list or a struct that few
/// nodes set goes into a boxed group, `Option<Box<…>>`, one for each set of fields used together, as `shadow`,
/// `capture` and `data_attrs` do. `a_node_stays_small` in the tests holds the size.
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    // shared box attrs
    pub gap: f32,
    pub padding: [f32; 4],
    pub background: Option<String>,
    pub border: Option<(f32, String)>,
    pub radius: f32,
    /// How opaque the box and everything in it is, from 0 to 1.
    pub opacity: f32,
    /// A drop shadow under the box, boxed: few boxes have one.
    pub shadow: Option<Box<Shadow>>,
    pub height_mode: HeightMode,
    pub width_constraint: Option<f32>,
    pub width_fill: bool,
    pub page_scope: Option<PageScope>,
    pub paginate: Paginate,
    /// Whether a table row stays with the row after it across a page break.
    pub keep: RowKeep,
    pub debug: bool,
    // layout-specific
    pub align: Align,
    pub justify: Justify,
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
    /// The color of a text, its own or the one it took from the box around it, and of a list, which gives it to
    /// its markers. `None` on every other node.
    pub text_color: Option<String>,
    pub text_align: TextAlign,
    /// The fewest lines of a split text that may stand alone on the next page.
    pub widows: u8,
    /// The fewest lines of a split text that may stand alone on the page it starts on.
    pub orphans: u8,
    /// The distance between lines of text, as a factor of the font size.
    pub line_height: f32,
    /// The most lines of a text that are drawn; `None` draws them all.
    pub max_lines: Option<usize>,
    /// A text cut short by `max_lines`, or a `text-at` cut short by its width, ends in an ellipsis.
    pub ellipsis: bool,
    /// 0 for a text that collapses its white space. Any other value is the text of a `<code>`, which keeps its
    /// spaces and lines, and a tab in it fills to the next multiple of that many columns.
    pub pre_tab: u8,
    /// Whether a styled run of several words that does not fit what is left of a line goes on in its next words, so
    /// the line is filled and the run is not moved down whole. A text of the document moves it whole, as it always
    /// has; a text written for a `<markdown>` fills the line, since a link in a paragraph or in a cell of a table is a
    /// run of any length. A word or a run wider than a whole line is broken at the last character that fits in every
    /// text, so nothing runs past the box.
    pub flow_runs: bool,
    /// What the text records or shows for a running header, boxed: few texts have one.
    pub capture: Option<Box<Capture>>,
    // link
    pub url: Option<String>,
    // img (NodeKind::Img only)
    pub image_name: Option<String>,
    pub img_height_constraint: Option<f32>,
    /// The width over the height of a picture whose width is `fill`, set where the sizes of the images are filled
    /// in. The width comes from the slot the picture is laid out in, and the height follows from this.
    pub img_aspect: Option<f32>,
    /// What a shape or a list adds to its node, boxed together because few nodes are either: read a shape's with
    /// `shape()`. `lists::lower` takes a list's, which it turns into columns and rows.
    pub detail: Option<Box<Detail>>,
    // barcode (NodeKind::Barcode only)
    pub barcode_type: Option<BarcodeType>,
    pub barcode_data: Option<String>,
    pub barcode_ec: BarcodeEcLevel,
    pub barcode_hrt: bool,
    pub barcode_color: Option<String>,
    pub barcode_bg: Option<String>,
    // table (NodeKind::Table only)
    pub table_cols: String,
    /// How a table is striped and lined, when it is either.
    pub table_style: Option<Box<TableStyle>>,
    // table cell (NodeKind::TableCell only)
    /// How many of the table's columns the cell spans, and how many of its rows, from 1.
    pub col_span: u16,
    pub row_span: u16,
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
    /// The `<bookmark>` elements that come right before this node in the source.
    pub bookmarks: Vec<Bookmark>,
    pub children: Vec<Node>,
}

/// The lines of a split text kept together on each side of a page break when `widows` and `orphans` are not set.
pub const DEFAULT_WIDOWS: u8 = 2;
pub const DEFAULT_ORPHANS: u8 = 2;
/// The distance between lines of text, as a factor of the font size, when a text sets none.
pub const DEFAULT_LINE_HEIGHT: f32 = 1.2;

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

/// Page-break hint set via `keep="…"` on a table row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RowKeep {
    /// The row may end a page (default).
    None,
    /// The row stays on the same page as the row after it, so a chain of these rows and the row that ends it break
    /// only together.
    WithNext,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Column,
    Table,
    TableHead,
    TableRow,
    TableCell,
    Row,
    Grid,
    Frame,
    Divider,
    Text,
    Link,
    Img,
    Shape,
    Barcode,
    Field,
    /// A `<list>`. It becomes a column once the data is in (`lists::lower`), so the layout never sees it.
    List,
    /// An `<li>`. It becomes a row of its marker and its content, with the list.
    ListItem,
    /// A `<markdown>`. It becomes the nodes of its Markdown once the text is known (`markdown::expand`), so the
    /// layout never sees it.
    Markdown,
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
pub(crate) struct ParsedNode {
    kind: NodeKind,
    gap: f32,
    padding: [f32; 4],
    background: Option<String>,
    border: Option<(f32, String)>,
    radius: f32,
    opacity: f32,
    shadow: Option<Box<Shadow>>,
    height_mode: HeightMode,
    width_constraint: Option<f32>,
    width_fill: bool,
    page_scope: Option<PageScope>,
    paginate: Paginate,
    keep: RowKeep,
    debug: bool,
    align: Align,
    justify: Justify,
    cols: u32,
    col_width: Option<f32>,
    direction: Direction,
    color: Option<String>,
    thickness: f32,
    text_runs: Vec<TextRun>,
    bold: bool,                 // `<text bold>`, folded into `font` by `resolve_node`
    font:      Option<String>,  // None = inherit
    font_size: Option<f32>,     // None = inherit
    text_color: Option<String>, // None = inherit; a box sets it for the text inside
    text_align: TextAlign,
    widows: u8,
    orphans: u8,
    line_height: f32,
    max_lines: Option<usize>,
    ellipsis: bool,
    pre_tab: u8,
    capture: Option<Box<Capture>>,
    url: Option<String>,
    image_name: Option<String>,
    img_height_constraint: Option<f32>,
    detail: Option<Box<Detail>>,
    barcode_type: Option<BarcodeType>,
    barcode_data: Option<String>,
    barcode_ec: BarcodeEcLevel,
    barcode_hrt: bool,
    barcode_color: Option<String>,
    barcode_bg: Option<String>,
    table_cols: String,
    table_style: Option<Box<TableStyle>>,
    col_span: u16,
    row_span: u16,
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
    bookmarks: Vec<Bookmark>,
    children: Vec<ParsedNode>,
}


impl ParsedNode {
    fn default_for(kind: NodeKind) -> Self {
        ParsedNode {
            gap: if kind == NodeKind::ListItem { LIST_ITEM_GAP_UNSET } else { 0.0 },
            kind,
            padding: [0.0; 4],
            background: None,
            border: None,
            radius: 0.0,
            opacity: 1.0,
            shadow: None,
            height_mode: HeightMode::Auto,
            width_constraint: None,
            width_fill: false,
            page_scope: None,
            paginate: Paginate::None,
            keep: RowKeep::None,
            debug: false,
            align: Align::Stretch,
            justify: Justify::Start,
            cols: 1,
            col_width: None,
            direction: Direction::Horizontal,
            color: None,
            thickness: 1.0,
            text_runs: Vec::new(),
            bold: false,
            font: None,
            font_size: None,
            text_color: None,
            text_align: TextAlign::Left,
            widows: DEFAULT_WIDOWS,
            orphans: DEFAULT_ORPHANS,
            line_height: DEFAULT_LINE_HEIGHT,
            max_lines: None,
            ellipsis: false,
            pre_tab: 0,
            capture: None,
            url: None,
            image_name: None,
            img_height_constraint: None,
            detail: None,
            barcode_type: None,
            barcode_data: None,
            barcode_ec: BarcodeEcLevel::M,
            barcode_hrt: false,
            barcode_color: None,
            barcode_bg: None,
            table_cols: String::new(),
            table_style: None,
            col_span: 1,
            row_span: 1,
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
            bookmarks: Vec::new(),
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
pub(crate) fn resolve_font_alias(name: &str, fonts: &HashMap<String, FontDef>) -> String {
    match fonts.get(name) {
        Some(FontDef::Core(builtin)) => builtin.clone(),
        Some(FontDef::Ref(key))      => key.clone(),
        // Unknown name: pass through. May be a direct builtin name (e.g.
        // "Helvetica-Bold") used without an alias declaration, or an undefined
        // alias. The layout engine handles both via its own fallback logic.
        None                         => name.to_string(),
    }
}

/// The font the text of a section is set in unless it names another: the document's `font`, or Helvetica.
fn section_root_font(doc_font: Option<&str>, fonts: &HashMap<String, FontDef>) -> String {
    doc_font
        .map(|f| resolve_font_alias(f, fonts))
        .unwrap_or_else(|| "Helvetica".to_string())
}

/// The size of text when nothing sets one.
const DEFAULT_FONT_SIZE: f32 = 11.0;
/// The color of text when nothing sets one and the tokens have no `text` color.
const DEFAULT_TEXT_COLOR: &str = "#1a1a1a";

/// What a text is set in unless it, a box around it or the layer it is in says otherwise.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextDefaults {
    pub font:  String,
    pub size:  f32,
    pub color: String,
}

impl TextDefaults {
    /// The defaults of a section: the document's font, 11pt and the `text` color.
    pub(crate) fn of_section(doc_font: Option<&str>, tokens: &Tokens, fonts: &HashMap<String, FontDef>) -> Self {
        TextDefaults {
            font:  section_root_font(doc_font, fonts),
            size:  DEFAULT_FONT_SIZE,
            color: tokens.resolve_color("text").unwrap_or_else(|_| DEFAULT_TEXT_COLOR.into()),
        }
    }

    /// These defaults with the font, size and color a layer sets over them.
    pub(crate) fn overridden_by(
        &self,
        tokens: &Tokens,
        font:   Option<&str>,
        size:   Option<&str>,
        color:  Option<&str>,
    ) -> Result<Self, String> {
        Ok(TextDefaults {
            font:  font.map_or_else(|| self.font.clone(), str::to_string),
            size:  size.map(|v| tokens.resolve_text_size(v)).transpose()?.unwrap_or(self.size),
            color: color.map(|v| tokens.resolve_color(v)).transpose()?.unwrap_or_else(|| self.color.clone()),
        })
    }
}

/// `Node::layout_default()` — used by region_to_compat_node.
impl Node {
    pub fn layout_default() -> Node {
        Node {
            kind: NodeKind::Column,
            gap: 0.0, padding: [0.0; 4], background: None, border: None, radius: 0.0, opacity: 1.0, shadow: None,
            height_mode: HeightMode::Auto, width_constraint: None, width_fill: false,
            page_scope: None, paginate: Paginate::None, keep: RowKeep::None, debug: false,
            align: Align::Stretch, justify: Justify::Start,
            cols: 1, col_width: None, direction: Direction::Horizontal,
            color: None, thickness: 1.0, text_runs: Vec::new(),
            font: "Helvetica".to_string(), font_size: 11.0, text_color: None,
            text_align: TextAlign::Left, widows: DEFAULT_WIDOWS, orphans: DEFAULT_ORPHANS,
            line_height: DEFAULT_LINE_HEIGHT, max_lines: None, ellipsis: false, pre_tab: 0,
            flow_runs: false, capture: None,
            url: None, image_name: None,
            img_height_constraint: None, img_aspect: None, detail: None, barcode_type: None, barcode_data: None,
            barcode_ec: BarcodeEcLevel::M, barcode_hrt: false, barcode_color: None,
            barcode_bg: None, table_cols: String::new(), table_style: None, col_span: 1, row_span: 1,
            field_kind: None, field_name: None, field_value: None, field_label: None,
            field_options: Vec::new(), field_required: false, field_readonly: false,
            field_checked: false, field_max_len: None, field_group: None,
            field_action_url: None, data_attrs: None, bookmarks: Vec::new(), children: Vec::new(),
        }
    }

    /// What a `<shape>` draws.
    pub fn shape(&self) -> Option<&Shape> {
        match self.detail.as_deref() {
            Some(Detail::Shape(shape)) => Some(shape),
            _                          => None,
        }
    }

    pub fn shape_mut(&mut self) -> Option<&mut Shape> {
        match self.detail.as_deref_mut() {
            Some(Detail::Shape(shape)) => Some(shape),
            _                          => None,
        }
    }

    /// The `capture-ref` of a text that shows what was captured.
    pub fn capture_ref(&self) -> Option<&CaptureRef> {
        self.capture.as_ref()?.sink.as_ref()
    }

    /// The lines a `capture-ref` text always takes; 0 for every other node.
    pub fn reserve_lines(&self) -> usize {
        self.capture.as_ref().map_or(0, |c| c.reserve_lines)
    }
}

/// The bold face of a built-in font. A font with no bold face, a custom font or one that is already
/// bold, comes back unchanged.
pub(crate) fn bold_variant(font: &str) -> String {
    match font {
        "Helvetica"         => "Helvetica-Bold",
        "Helvetica-Oblique" => "Helvetica-BoldOblique",
        "Times-Roman"       => "Times-Bold",
        "Times-Italic"      => "Times-BoldItalic",
        "Courier"           => "Courier-Bold",
        "Courier-Oblique"   => "Courier-BoldOblique",
        other               => other,
    }
    .to_string()
}

/// Resolve the font of a run: a span's font alias, and `bold` folded into the font. A span with no font of its
/// own keeps inheriting the text's font, which is already bold when the text is. The words of a footnote are
/// resolved the same way.
fn resolve_run(run: TextRun, inherited: &str, node_bold: bool, fonts: &HashMap<String, FontDef>) -> TextRun {
    let own = run.font.as_deref().map(|alias| resolve_font_alias(alias, fonts));
    let font = match (own, run.bold) {
        (Some(f), true)  => Some(bold_variant(&f)),
        (Some(f), false) => Some(if node_bold { bold_variant(&f) } else { f }),
        (None, true)     => Some(bold_variant(inherited)),
        (None, false)    => None,
    };
    let footnote = run.footnote.map(|mut note| {
        note.runs = std::mem::take(&mut note.runs).into_iter()
            .map(|r| resolve_run(r, inherited, node_bold, fonts))
            .collect();
        note
    });
    TextRun { font, bold: false, footnote, ..run }
}

fn resolve_node(
    n:             ParsedNode,
    current_font:  &str,
    current_size:  f32,
    current_color: &str,
    fonts:         &HashMap<String, FontDef>,
) -> Node {
    let font_raw   = n.font.as_deref().unwrap_or(current_font);
    let inherited  = resolve_font_alias(font_raw, fonts);
    let font       = if n.bold { bold_variant(&inherited) } else { inherited.clone() };
    let font_size  = n.font_size.unwrap_or(current_size);
    let node_bold  = n.bold;
    let is_text    = n.kind == NodeKind::Text;
    let own_color  = n.text_color;
    let color      = own_color.as_deref().unwrap_or(current_color);

    // Resolve span-level font aliases and fold bold into the font. A span with no font of its own keeps
    // inheriting the text's font, which is already bold when the text is.
    let text_runs = n.text_runs.into_iter().map(|run| resolve_run(run, &inherited, node_bold, fonts)).collect();

    let children = n.children
        .into_iter()
        .map(|c| resolve_node(c, &font, font_size, color, fonts))
        .collect();
    let text_color = (is_text || n.kind == NodeKind::List).then(|| color.to_string());

    Node {
        kind:                  n.kind,
        gap:                   n.gap,
        padding:               n.padding,
        background:            n.background,
        border:                n.border,
        radius:                n.radius,
        opacity:               n.opacity,
        shadow:                n.shadow,
        height_mode:           n.height_mode,
        width_constraint:      n.width_constraint,
        width_fill:            n.width_fill,
        page_scope:            n.page_scope,
        paginate:              n.paginate,
        keep:                  n.keep,
        debug:                 n.debug,
        align:                 n.align,
        justify:               n.justify,
        cols:                  n.cols,
        col_width:             n.col_width,
        direction:             n.direction,
        color:                 n.color,
        thickness:             n.thickness,
        text_runs,
        font,
        font_size,
        text_color,
        text_align:            n.text_align,
        widows:                n.widows,
        orphans:               n.orphans,
        line_height:           n.line_height,
        max_lines:             n.max_lines,
        ellipsis:              n.ellipsis,
        pre_tab:               n.pre_tab,
        flow_runs:             false,
        capture:               n.capture,
        url:                   n.url,
        image_name:            n.image_name,
        img_height_constraint: n.img_height_constraint,
        img_aspect:            None,
        detail:                n.detail,
        barcode_type:          n.barcode_type,
        barcode_data:          n.barcode_data,
        barcode_ec:            n.barcode_ec,
        barcode_hrt:           n.barcode_hrt,
        barcode_color:         n.barcode_color,
        barcode_bg:            n.barcode_bg,
        table_cols:            n.table_cols,
        table_style:           n.table_style,
        col_span:              n.col_span,
        row_span:              n.row_span,
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
        bookmarks:             n.bookmarks,
        children,
    }
}


// ── Main parse entry point (XML) ─────────────────────────────────────────────

// The WASI binary reads every document with its resources, so this is for the engine and the tests.
#[allow(dead_code)]
pub fn parse(xml: &str) -> Result<Document, String> {
    parse_with(xml, &Resources::default())
}

/// What whoever renders a document loads for it, besides the fonts and images: the files that a
/// `<markdown-styles src>` names, by the `src` they were loaded for.
#[derive(Debug, Clone, Default)]
pub struct Resources {
    pub markdown_styles: HashMap<String, String>,
}

/// Parse `xml` with the files its `src` attributes name loaded in `resources`.
pub fn parse_with(xml: &str, resources: &Resources) -> Result<Document, String> {
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
    let mut image_srcs:   HashMap<String, String>     = HashMap::new();
    for child in elems(&root) {
        if child.tag_name().name() == "assets" {
            parse_assets_elem(&child, &mut asset_fonts, &mut asset_images, &mut image_srcs)?;
        }
    }

    // ── Pass 1: collect <tokens> ─────────────────────────────────────────────
    let mut tokens = Tokens::default();
    for child in elems(&root) {
        if child.tag_name().name() == "tokens" {
            parse_tokens_elem(&child, &mut tokens)?;
        }
    }

    let markdown_assets = markdown::Assets { image_srcs, styles_files: resources.markdown_styles.clone() };
    let mut meta          = Meta::default();
    let mut doc_size      = (595.28_f32, 841.89_f32); // a4
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
                if child.attribute("margin").is_some() {
                    return Err(format!("<document>{}: {MARGIN_MOVED}", node_loc(&child)));
                }
                if let Some(v) = child.attribute("background") {
                    doc_background = Some(tokens.resolve_fill(v)?);
                }
                if let Some(v) = child.attribute("debug") {
                    doc_debug = v == "true";
                }
                let doc_footnote_counter = child.attribute("footnote-counter")
                    .map(FootnoteCounter::parse)
                    .transpose()
                    .map_err(|e| format!("<document>{}: {e}", node_loc(&child)))?;

                let mut sections_xml: Vec<Section> = Vec::new();
                for doc_child in elems(&child) {
                    match doc_child.tag_name().name() {
                        "meta" => meta = parse_meta(&doc_child),
                        "section" => {
                            sections_xml.push(parse_section_elem(
                                &doc_child,
                                doc_size,
                                doc_background.clone(),
                                doc_debug,
                                doc_font.as_deref(),
                                &tokens,
                                &asset_fonts,
                                &asset_images,
                                &markdown_assets,
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
                markdown::expand_pending(&mut sections_xml, None, markdown::Pass::Read)?;
                check_sections(&mut sections_xml)?;
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
                    background:  doc_background,
                    gradients:   tokens.gradients.clone(),
                    colors:      tokens.colors.clone(),
                    debug:       doc_debug,
                    footnote_counter: doc_footnote_counter,
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

/// The definition of a `<font>` asset. `core` names a built-in font; otherwise the font is a registry
/// key, `ref` or, with no `ref`, the asset's own name. `location` says where the asset was written.
fn font_asset_def(
    core:     Option<&str>,
    ref_key:  Option<&str>,
    name:     &str,
    location: &str,
) -> Result<FontDef, String> {
    if let Some(core) = core {
        return Ok(FontDef::Core(core.to_string()));
    }
    match ref_key {
        Some(key) if is_url(key) => Err(format!(
            "<font>{location}: ref must be a registry key, not a URL"
        )),
        Some(key) => Ok(FontDef::Ref(key.to_string())),
        None => Ok(FontDef::Ref(name.to_string())),
    }
}

/// The objects in `assets.<kind>` of a kit tree. They carry the attributes of the `<font>` and `<image>`
/// elements, so an asset is declared the same way in XML and in a tree.
fn tree_asset_entries<'a>(
    assets: &'a serde_json::Map<String, serde_json::Value>,
    kind:   &str,
) -> Result<Vec<&'a serde_json::Map<String, serde_json::Value>>, String> {
    match assets.get(kind) {
        None => Ok(Vec::new()),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_object()
                    .ok_or_else(|| format!("assets.{kind}: every entry must be an object"))
            })
            .collect(),
        Some(_) => Err(format!("assets.{kind} must be a list")),
    }
}

/// Read the `assets` of a kit tree into the same maps the XML parser fills from `<assets>`. A font
/// may also carry `widths`, glyph advance widths for layout, which XML has no attribute for.
fn parse_tree_assets(
    assets:     &serde_json::Map<String, serde_json::Value>,
    fonts:      &mut HashMap<String, FontDef>,
    images:     &mut HashMap<String, String>,
    image_srcs: &mut HashMap<String, String>,
    widths:     &mut HashMap<String, FontWidths>,
) -> Result<(), String> {
    for font in tree_asset_entries(assets, "fonts")? {
        let name = tree_asset_name(font, "fonts")?;
        let text = |key: &str| font.get(key).and_then(|v| v.as_str());
        let def = font_asset_def(text("core"), text("ref"), &name, &format!(" '{name}'"))?;
        if let Some(w) = font.get("widths").and_then(|v| v.as_object()) {
            let default = w.get("default").and_then(|v| v.as_u64()).unwrap_or(500) as u16;
            let ascii: Vec<u16> = w.get("ascii")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().map(|n| n.as_u64().unwrap_or(500) as u16).collect())
                .unwrap_or_default();
            widths.insert(name.clone(), FontWidths { default, ascii, ..FontWidths::default() });
        }
        fonts.insert(name, def);
    }
    for image in tree_asset_entries(assets, "images")? {
        let name = tree_asset_name(image, "images")?;
        let key = image.get("ref").and_then(|v| v.as_str()).unwrap_or(&name).to_string();
        if let Some(src) = image.get("src").and_then(|v| v.as_str()) { image_srcs.insert(src.to_string(), name.clone()); }
        images.insert(name, key);
    }
    Ok(())
}

fn tree_asset_name(
    entry: &serde_json::Map<String, serde_json::Value>,
    kind:  &str,
) -> Result<String, String> {
    let name = entry
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("assets.{kind}: every entry needs a 'name'"))?;
    validate_asset_name(name)?;
    Ok(name.to_string())
}

fn parse_assets_elem(
    elem:         &roxmltree::Node,
    fonts:        &mut HashMap<String, FontDef>,
    images:       &mut HashMap<String, String>,
    image_srcs:   &mut HashMap<String, String>,
) -> Result<(), String> {
    for child in elems(elem) {
        match child.tag_name().name() {
            "font" => {
                let name = req_attr(&child, "name")?;
                validate_asset_name(&name)?;
                let def = font_asset_def(
                    child.attribute("core"),
                    child.attribute("ref"),
                    &name,
                    &node_loc(&child),
                )?;
                // src is an adapter-level hint only — ignored by the Rust parser.
                fonts.insert(name, def);
            }
            "image" => {
                let name = req_attr(&child, "name")?;
                validate_asset_name(&name)?;
                let key = child.attribute("ref").unwrap_or(&name).to_string();
                // src is an adapter-level hint, which loads the file. The parser keeps it only to say which asset a
                // Markdown image is, when the Markdown names the file.
                if let Some(src) = child.attribute("src") { image_srcs.insert(src.to_string(), name.clone()); }
                images.insert(name, key);
            }
            other => return Err(format!("<{}>{}: unknown element in <assets>", other, node_loc(&child))),
        }
    }
    Ok(())
}

// ── Tokens ────────────────────────────────────────────────────────────────────

fn parse_tokens_elem(elem: &roxmltree::Node, tokens: &mut Tokens) -> Result<(), String> {
    // A gradient names colors of the `colors` row, which may come after it, so gradients are read last.
    let mut gradients_elem = None;
    for child in elems(elem) {
        match child.tag_name().name() {
            "space"  => tokens.space    = parse_scale_row(&child)?,
            "border" => tokens.border   = parse_scale_row(&child)?,
            "radius" => tokens.radius   = parse_scale_row(&child)?,
            "width"  => tokens.width    = parse_scale_row(&child)?,
            "height" => tokens.height   = parse_scale_row(&child)?,
            "text-size" => tokens.text_size = parse_scale_row(&child)?,
            "line-height" => tokens.line_height = parse_factor_row(&child)?,
            "grid"   => tokens.grid_col = parse_scale_row(&child)?,
            "aliases" => parse_aliases_elem(&child, tokens)?,
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
            "gradients" => gradients_elem = Some(child),
            other => return Err(format!("<{}>{}: unknown element in <tokens>", other, node_loc(&child))),
        }
    }
    if let Some(gradients) = gradients_elem {
        parse_gradients_elem(&gradients, tokens)?;
    }
    Ok(())
}

/// Read the `<alias>` elements of `<aliases>`, each naming a slot of the scale.
fn parse_aliases_elem(elem: &roxmltree::Node, tokens: &mut Tokens) -> Result<(), String> {
    for alias in elems(elem) {
        if alias.tag_name().name() != "alias" {
            return Err(format!(
                "<{}>{}: unknown element in <aliases>",
                alias.tag_name().name(),
                node_loc(&alias)
            ));
        }
        let name  = req_attr(&alias, "name")?;
        let value = req_attr(&alias, "value")?;
        tokens.add_alias(&name, &value).map_err(|e| format!("{e}{}", node_loc(&alias)))?;
    }
    Ok(())
}

/// Read the `<gradient>` elements of `<gradients>`, each with its `<stop>` children.
fn parse_gradients_elem(elem: &roxmltree::Node, tokens: &mut Tokens) -> Result<(), String> {
    for gradient in elems(elem) {
        if gradient.tag_name().name() != "gradient" {
            return Err(format!(
                "<{}>{}: unknown element in <gradients>",
                gradient.tag_name().name(),
                node_loc(&gradient)
            ));
        }
        let name = req_attr(&gradient, "name")?;
        let mut stops: Vec<(&str, &str)> = Vec::new();
        for stop in elems(&gradient) {
            if stop.tag_name().name() != "stop" {
                return Err(format!(
                    "<{}>{}: unknown element in <gradient>",
                    stop.tag_name().name(),
                    node_loc(&stop)
                ));
            }
            let offset = stop.attribute("offset").ok_or_else(|| format!("<stop>{}: missing offset", node_loc(&stop)))?;
            let color  = stop.attribute("color").ok_or_else(|| format!("<stop>{}: missing color", node_loc(&stop)))?;
            stops.push((offset, color));
        }
        let kind = req_attr(&gradient, "type")?;
        let spec = GradientSpec {
            kind:  &kind,
            angle: gradient.attribute("angle"),
            from:  gradient.attribute("from"),
            to:    gradient.attribute("to"),
            stops,
        };
        let resolved = tokens.resolve_gradient(&name, &spec)?;
        tokens.gradients.insert(name, resolved);
    }
    Ok(())
}


/// Parse xs/s/m/l/xl/xxl pt attributes from a token scale element.
fn parse_scale_row(elem: &roxmltree::Node) -> Result<[f32; 6], String> {
    parse_row(elem, parse_pt, "pt value")
}

/// Parse xs/s/m/l/xl/xxl factors from a token row of factors, such as `line-height`.
fn parse_factor_row(elem: &roxmltree::Node) -> Result<[f32; 6], String> {
    parse_row(elem, parse_factor, "factor, a number above 0")
}

/// The six slots of a token row, each read with `parse`. `expected` says what a slot holds, for the error.
fn parse_row(elem: &roxmltree::Node, parse: fn(&str) -> Option<f32>, expected: &str) -> Result<[f32; 6], String> {
    let tag = elem.tag_name().name();
    let get = |attr: &str| -> Result<f32, String> {
        let v = elem
            .attribute(attr)
            .ok_or_else(|| format!("<{tag}> missing '{attr}' attribute"))?;
        parse(v).ok_or_else(|| format!("<{tag}> '{attr}': invalid {expected} '{v}'"))
    };
    Ok([get("xs")?, get("s")?, get("m")?, get("l")?, get("xl")?, get("xxl")?])
}

// ── Page / meta ───────────────────────────────────────────────────────────────

/// Apply size/orientation/background/debug overrides from an attribute
/// source onto the running section-defaults.  Works for both XML `<section>` elements
/// and JSON section nodes via the `Attrs` trait.  Returns the resolved `debug` flag.
fn apply_page_overrides(
    size:       &mut (f32, f32),
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
    if let Some(v) = a.get("background") {
        *background = Some(tokens.resolve_fill(v)?);
    }
    Ok(a.get("debug").map(|v| v == "true").unwrap_or(doc_debug))
}

/// The margin a `<layout>` keeps from the page edges: its `margin`, or the default on every side.
pub(crate) fn layout_margin(a: &impl Attrs, tokens: &Tokens) -> Result<[f32; 4], String> {
    match a.get("margin") {
        Some(v) => tokens.resolve_spacing(v),
        None    => Ok([DEFAULT_LAYOUT_MARGIN; 4]),
    }
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

/// A drop shadow: a copy of a shape, moved by `dx` and `dy` and filled with `color`, drawn under the shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    /// How far right of the shape the copy is, in pt. Negative is left.
    pub dx: f32,
    /// How far below the shape the copy is, in pt. Negative is above.
    pub dy: f32,
    pub color: String,
    /// How opaque the shadow alone is, from 0 to 1.
    pub opacity: f32,
}

/// Parse a `shadow`: "x y color", and after the color an opacity that fades the shadow alone, as in
/// "2pt 3pt #000000 0.2".
pub fn parse_shadow(val: &str, tokens: &Tokens) -> Result<Shadow, String> {
    let parts: Vec<&str> = val.split_whitespace().collect();
    if !(3..=4).contains(&parts.len()) {
        return Err(format!(
            "invalid shadow '{val}': expected 'x y color' and optionally an opacity, like '2pt 3pt #000000 0.2'"
        ));
    }
    let offset = |part: &str, axis: &str| {
        parse_signed_measurement(part)
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("invalid shadow '{val}': {axis} '{part}' is not a length, like 2pt or -3pt"))
    };
    let dx = offset(parts[0], "x")?;
    let dy = offset(parts[1], "y")?;
    let color = tokens.resolve_color(parts[2]).map_err(|e| format!("invalid shadow '{val}': {e}"))?;
    let opacity = match parts.get(3) {
        Some(part) => parse_opacity(part)
            .map_err(|_| format!("invalid shadow '{val}': opacity '{part}' is not a number from 0 to 1"))?,
        None => 1.0,
    };
    Ok(Shadow { dx, dy, color, opacity })
}

/// Thin wrapper: resolve a ParsedNode in isolation (used during XML parse of sections
/// where we haven't yet done the full resolve_doc pass).  The caller should use
/// `resolve_doc` for proper font inheritance; this is only used to convert
/// `ParsedNode` → `Node` when the context font is already known to be the default.
pub(crate) fn resolve_parsed_node(
    n:     ParsedNode,
    text:  &TextDefaults,
    fonts: &HashMap<String, FontDef>,
) -> Node {
    resolve_node(n, &text.font, text.size, &text.color, fonts)
}

fn parse_region_elem(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    asset_fonts:  &HashMap<String, FontDef>,
    text:         &TextDefaults,
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
    let mut pending = PendingBookmarks::default();
    for child in elems(elem) {
        if child.tag_name().name() == "bookmark" {
            pending.push(xml_bookmark(&child)?);
            continue;
        }
        let mut node = parse_node(&child, tokens, asset_images)?;
        pending.attach(&mut node);
        children.push(resolve_parsed_node(node, text, asset_fonts));
    }
    pending.finish("region").map_err(|e| format!("<region>{}: {e}", node_loc(elem)))?;
    if children.iter().any(|c| count_footnotes(c) > 0) {
        return Err(format!("<region>{}: {}", node_loc(elem), footnote_not_here("a region")));
    }
    Ok(LayoutRegion { pin, page, w, children, debug })
}

/// What to write instead of `<canvas>`, which the format no longer has.
const CANVAS_REMOVED: &str =
    "removed; put its <layer> elements directly in the <section>: before the <layout> to paint under it, after it to paint over it";

/// What to write instead of a second `<layout>` in a section.
const SECOND_LAYOUT: &str = "a <section> has at most one <layout>; put all of its flowing content in the first one";

/// What to write instead of a `<layout>` and a `<markdown>` in one section.
const ONE_FLOW: &str = "a <section> has one <layout> or one <markdown>, not both; put the Markdown in a section of its own";

/// Why a table of contents section can't hold a `<markdown>`.
const NO_MARKDOWN_IN_TOC: &str = "a table of contents section is laid out from its <layout>; put the Markdown in a section of its own";

#[allow(clippy::too_many_arguments)]
fn parse_section_elem(
    elem:           &roxmltree::Node,
    doc_size:       (f32, f32),
    doc_background: Option<String>,
    doc_debug:      bool,
    doc_font:       Option<&str>,
    tokens:         &Tokens,
    asset_fonts:    &HashMap<String, FontDef>,
    asset_images:   &HashMap<String, String>,
    markdown:       &markdown::Assets,
) -> Result<Section, String> {
    if elem.attribute("margin").is_some() {
        return Err(format!("<section>{}: {MARGIN_MOVED}", node_loc(elem)));
    }
    // Inherit doc-level defaults, then apply section overrides.
    let mut size       = doc_size;
    let mut background = doc_background;
    let debug = apply_page_overrides(&mut size, &mut background, elem, tokens, doc_debug)?;
    let title = elem.attribute("title").map(str::to_string);
    let toc = parse_section_toc(elem, tokens).map_err(|e| format!("<section>{}: {e}", node_loc(elem)))?;
    let footnote_counter = elem.attribute("footnote-counter")
        .map(FootnoteCounter::parse)
        .transpose()
        .map_err(|e| format!("<section>{}: {e}", node_loc(elem)))?;

    let text = TextDefaults::of_section(doc_font, tokens, asset_fonts);

    let mut children: Vec<SectionChild> = Vec::new();
    let mut bookmark: Option<Bookmark> = None;
    let mut has_markdown = false;
    for child in elems(elem) {
        match child.tag_name().name() {
            "markdown" => {
                if children.iter().any(|c| matches!(c, SectionChild::Layout { .. })) {
                    return Err(format!("<markdown>{}: {ONE_FLOW}", node_loc(&child)));
                }
                if toc.is_some() {
                    return Err(format!("<markdown>{}: {NO_MARKDOWN_IN_TOC}", node_loc(&child)));
                }
                has_markdown = true;
                let cx = markdown::SectionContext {
                    tokens, fonts: asset_fonts, images: asset_images, image_srcs: &markdown.image_srcs,
                    defaults: &text, styles_files: &markdown.styles_files,
                };
                children.push(markdown::parse_elem(&child, &cx)?);
            }
            "layout" => {
                if children.iter().any(|c| matches!(c, SectionChild::Layout { .. })) {
                    return Err(format!("<layout>{}: {}", node_loc(&child), if has_markdown { ONE_FLOW } else { SECOND_LAYOUT }));
                }
                let margin = layout_margin(&child, tokens).map_err(|e| format!("<layout>{}: {e}", node_loc(&child)))?;
                // Inline layout: build a Layout from its children (may include <region>).
                let mut lc: Vec<LayoutChild> = Vec::new();
                let mut pending = PendingBookmarks::default();
                for layout_child in elems(&child) {
                    match layout_child.tag_name().name() {
                        "region" => lc.push(LayoutChild::Region(parse_region_elem(&layout_child, tokens, asset_images, asset_fonts, &text)?)),
                        "bookmark" => pending.push(xml_bookmark(&layout_child)?),
                        _ => {
                            let mut node = parse_node(&layout_child, tokens, asset_images)?;
                            pending.attach(&mut node);
                            lc.push(LayoutChild::Content(resolve_parsed_node(node, &text, asset_fonts)));
                        }
                    }
                }
                pending.finish("layout").map_err(|e| format!("<layout>{}: {e}", node_loc(&child)))?;
                children.push(SectionChild::Layout { margin, gap: 0.0, children: lc });
            }
            "layer" => {
                let ctx = canvas::LayerContext {
                    tokens, asset_images, asset_fonts,
                    text: &text, page_w: size.0, page_h: size.1,
                };
                children.push(SectionChild::Layer(canvas::parse_layer_elem(&child, &ctx)?));
            }
            "canvas" => return Err(format!("<canvas>{}: {CANVAS_REMOVED}", node_loc(&child))),
            "bookmark" => {
                if bookmark.is_some() {
                    return Err(format!(
                        "<bookmark>{}: a <section> has one <bookmark> of its own; put the others inside its <layout>",
                        node_loc(&child)
                    ));
                }
                bookmark = Some(xml_bookmark(&child)?);
            }
            other => return Err(format!(
                "<{}>{}: unexpected child in <section> — expected <bookmark>, <layer>, <layout> or <markdown>",
                other, node_loc(&child)
            )),
        }
    }

    if children.is_empty() {
        return Err(format!(
            "<section>{}: must have at least one <layout> or <layer> child",
            node_loc(elem)
        ));
    }

    Ok(Section {
        children,
        options: SectionOptions {
            size:       if size != doc_size { Some(size) } else { None },
            background,
            debug:      Some(debug),
            title,
            toc,
            footnote_counter,
        },
        bookmark,
    })
}

pub(crate) fn xml_bookmark(elem: &roxmltree::Node) -> Result<Bookmark, String> {
    parse_bookmark(elem).map_err(|e| format!("<bookmark>{}: {e}", node_loc(elem)))
}

// ── Node parsing ──────────────────────────────────────────────────────────────

/// What to write instead of a layout element the format no longer has, for the
/// error a document using it gets.
fn replaced_element_hint(tag: &str) -> Option<&'static str> {
    match tag {
        "stack"   => Some("renamed to <column>"),
        "cluster" => Some("renamed to <row>"),
        "flank"   => Some("removed; use <row> with width=\"fill\" on the child that fills the rest"),
        "split"   => Some("removed; use <row justify=\"between\">, or <row> with width=\"fill\" on both children for equal halves"),
        _         => None,
    }
}

pub(crate) fn parse_node(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
) -> Result<ParsedNode, String> {
    let tag = elem.tag_name().name();
    if tag == "code" {
        return parse_code(elem, tokens);
    }
    let kind = match tag {
        "column"  => NodeKind::Column,
        "row"     => NodeKind::Row,
        "grid"    => NodeKind::Grid,
        "frame"   => NodeKind::Frame,
        "divider" => NodeKind::Divider,
        "text"    => NodeKind::Text,
        "link"    => NodeKind::Link,
        "img"     => NodeKind::Img,
        "shape"   => NodeKind::Shape,
        "barcode" => NodeKind::Barcode,
        "field"   => NodeKind::Field,
        "table"   => NodeKind::Table,
        "thead"   => NodeKind::TableHead,
        "tr"      => NodeKind::TableRow,
        "td"      => NodeKind::TableCell,
        "list"    => NodeKind::List,
        "li"      => NodeKind::ListItem,
        other     => return Err(match replaced_element_hint(other) {
            Some(hint) => format!("<{}>{}: {hint}", other, node_loc(elem)),
            None       => format!("<{}>{}: unknown element", other, node_loc(elem)),
        }),
    };

    let mut node = ParsedNode::default_for(kind.clone());
    apply_box_attrs(&mut node, elem, tokens)
        .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;
    apply_layout_kind_attrs(&mut node, elem, tokens)
        .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;

    // data-binding attributes (valid on any element; boxed to keep Node lean)
    {
        let dv    = elem.attribute("data-value").map(str::to_owned);
        let ds    = elem.attribute("data-source").map(str::to_owned);
        let di    = elem.attribute("data-if").map(str::to_owned);
        let din   = elem.attribute("data-if-not").map(str::to_owned);
        let bound = read_bound_attrs(elem);
        check_bound_attrs(&kind, &bound).map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;
        if dv.is_some() || ds.is_some() || di.is_some() || din.is_some() || !bound.is_empty() {
            node.data_attrs = Some(Box::new(DataAttrs {
                data_value:  dv,
                data_source: ds,
                data_if:     di,
                data_if_not: din,
                bound,
            }));
        }
    }

    match kind {
        NodeKind::Text => {
            node.text_align = match elem.attribute("align").unwrap_or("left") {
                "center"  => TextAlign::Center,
                "right"   => TextAlign::Right,
                "justify" => TextAlign::Justify,
                _         => TextAlign::Left,
            };
            node.bold = elem.attribute("bold").map(|v| v == "true").unwrap_or(false);
            read_text_runs(&mut node, elem, tokens)?;
        }
        NodeKind::Link => {
            node.url = Some(
                elem.attribute("href")
                    .ok_or_else(|| format!("<link>{}: missing required attribute 'href'", node_loc(elem)))?
                    .to_string(),
            );
        }
        NodeKind::Img => {
            apply_img_attrs(&mut node, elem, Some(tokens), asset_images, "<assets>")
                .map_err(|e| format!("<{}>{}: {e}", tag, node_loc(elem)))?;
        }
        NodeKind::Shape => {
            apply_shape_attrs(&mut node, elem, tokens)
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
    if !matches!(kind, NodeKind::Divider | NodeKind::Text | NodeKind::Img | NodeKind::Shape | NodeKind::Barcode | NodeKind::Field) {
        let mut pending = PendingBookmarks::default();
        let mut table_rows = (kind == NodeKind::Table).then(|| TableRows::new(&node.table_cols));
        for child in elems(elem) {
            if child.tag_name().name() == "bookmark" {
                if matches!(kind, NodeKind::Table | NodeKind::TableHead | NodeKind::TableRow) {
                    return Err(format!(
                        "<bookmark>{}: not allowed in <{tag}> — put it inside a <td>",
                        node_loc(&child)
                    ));
                }
                pending.push(xml_bookmark(&child)?);
                continue;
            }
            let mut child_node = parse_node(&child, tokens, asset_images)?;
            pending.attach(&mut child_node);
            match kind {
                NodeKind::Table => {
                    if !matches!(child_node.kind, NodeKind::TableHead | NodeKind::TableRow) {
                        return Err(format!(
                            "<table>{}: child <{}> is not allowed here — expected <thead> or <tr>",
                            node_loc(elem), child.tag_name().name()
                        ));
                    }
                    if let Some(rows) = table_rows.as_mut() {
                        rows.check(&child_node)
                            .map_err(|e| format!("<{}>{}: {e}", child.tag_name().name(), node_loc(&child)))?;
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
                NodeKind::List => {
                    if child_node.kind != NodeKind::ListItem {
                        return Err(format!(
                            "<list>{}: child <{}> is not allowed here — expected <li>",
                            node_loc(elem), child.tag_name().name()
                        ));
                    }
                }
                _ => {
                    if child_node.kind == NodeKind::ListItem {
                        return Err(format!("<{tag}>{}: <li> belongs inside a <list>", node_loc(elem)));
                    }
                }
            }
            node.children.push(child_node);
        }
        if let Some(rows) = &table_rows {
            rows.finish().map_err(|e| format!("<table>{}: {e}", node_loc(elem)))?;
        }
        pending.finish(tag).map_err(|e| format!("<{tag}>{}: {e}", node_loc(elem)))?;
        if kind == NodeKind::Frame && node.children.len() > 1 {
            return Err(format!(
                "<frame>{}: accepts at most one child; got {}",
                node_loc(elem), node.children.len()
            ));
        }
        if kind == NodeKind::Table && node.children.iter().any(parsed_has_footnote) {
            return Err(format!("<table>{}: {}", node_loc(elem), footnote_not_here("a table")));
        }
    }

    Ok(node)
}

/// The plain run for a stretch of text between spans. A space leads it where the stretch starts with whitespace
/// or the text before it ended with some, and `prev_ended_space` is left for the run after it.
fn push_plain_run(runs: &mut Vec<TextRun>, raw: &str, prev_ended_space: &mut bool) {
    let words: Vec<&str> = raw.split_whitespace().collect();
    if !words.is_empty() {
        runs.push(TextRun {
            text: words.join(" "),
            leading_space: raw.starts_with(char::is_whitespace) || *prev_ended_space,
            font: None, color: None, href: None,
            underline: false, strike: false, bold: false, background: None, footnote: None,
        });
    }
    *prev_ended_space = raw.ends_with(char::is_whitespace);
}

/// The anchor of a footnote: a run without words of its own that carries the note. It is numbered when the page
/// it lands on is known.
fn footnote_run(words: Vec<TextRun>, leading_space: bool) -> TextRun {
    TextRun {
        text: String::new(), leading_space,
        font: None, color: None, href: None,
        underline: false, strike: false, bold: false, background: None,
        footnote: Some(Box::new(Footnote { runs: words, number: 1 })),
    }
}

/// What a footnote with no words, or inside another, is told.
const EMPTY_FOOTNOTE: &str = "a footnote needs words";
const NESTED_FOOTNOTE: &str = "a footnote cannot hold another footnote";

/// The runs of a `<text>` or `<text-at>`: its plain words, a styled run for each `<span>`, and an anchor for each
/// `<fn>`.
fn read_text_runs(node: &mut ParsedNode, elem: &roxmltree::Node, tokens: &Tokens) -> Result<(), String> {
    read_runs(&mut node.text_runs, elem, tokens, true)
}

/// Read the mixed content of `elem` into `runs`. `notes` says whether an `<fn>` may stand in it: the words of a
/// footnote may not hold one.
fn read_runs(runs: &mut Vec<TextRun>, elem: &roxmltree::Node, tokens: &Tokens, notes: bool) -> Result<(), String> {
    // Mixed content: text nodes → plain runs; <span> elements → styled runs.
    let mut prev_ended_space = false;
    for child in elem.children() {
        if child.is_text() {
            push_plain_run(runs, child.text().unwrap_or(""), &mut prev_ended_space);
        } else if child.is_element() && child.tag_name().name() == "span" {
            let raw_span: String = child
                .children()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            let span_text = raw_span.split_whitespace().collect::<Vec<_>>().join(" ");
            if !span_text.is_empty() {
                runs.push(TextRun {
                    text: span_text,
                    leading_space: raw_span.starts_with(char::is_whitespace) || prev_ended_space,
                    font:  child.attribute("font").map(|s| s.to_string()),
                    color: if let Some(v) = child.attribute("color") {
                        Some(tokens.resolve_color(v)?)
                    } else {
                        None
                    },
                    href:      child.attribute("href").map(|s| s.to_string()),
                    underline: child.attribute("underline").map(|v| v == "true").unwrap_or(false),
                    strike:    child.attribute("strike").map(|v| v == "true").unwrap_or(false),
                    bold:      child.attribute("bold").map(|v| v == "true").unwrap_or(false),
                    background: None,
                    footnote:  None,
                });
            }
            prev_ended_space = raw_span.ends_with(char::is_whitespace);
        } else if child.is_element() && child.tag_name().name() == "code-span" {
            let raw_span: String = child
                .children()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            let span_text = raw_span.split_whitespace().collect::<Vec<_>>().join(" ");
            if !span_text.is_empty() {
                let leading_space = raw_span.starts_with(char::is_whitespace) || prev_ended_space;
                runs.push(code_span_run(&child, span_text, leading_space, tokens)?);
            }
            prev_ended_space = raw_span.ends_with(char::is_whitespace);
        } else if child.is_element() && child.tag_name().name() == "fn" {
            if !notes {
                return Err(format!("<fn>{}: {NESTED_FOOTNOTE}", node_loc(&child)));
            }
            let mut words = Vec::new();
            read_runs(&mut words, &child, tokens, false)?;
            if words.is_empty() {
                return Err(format!("<fn>{}: {EMPTY_FOOTNOTE}", node_loc(&child)));
            }
            runs.push(footnote_run(words, prev_ended_space));
            prev_ended_space = false;
        }
    }
    Ok(())
}

/// The runs of a text node of a kit tree: its strings, a styled run for each `span` child, and an anchor for each
/// `fn`. The strings are read as the text of the XML is, so a space falls between two runs only where the text has
/// whitespace there: `["$", span("5")]` is `$5`, and `["Total: ", span("5")]` is `Total: 5`. Strings with no span
/// between them are one text, as they are once written as XML.
fn read_tree_text_runs(node: &mut ParsedNode, json: &serde_json::Value, tokens: &Tokens) -> Result<(), String> {
    read_tree_runs(&mut node.text_runs, json, tokens, true)
}

fn read_tree_runs(runs: &mut Vec<TextRun>, json: &serde_json::Value, tokens: &Tokens, notes: bool) -> Result<(), String> {
    let Some(children) = json.get("nodes").and_then(|v| v.as_array()) else {
        return Ok(());
    };
    let mut prev_ended_space = false;
    let mut text = String::new();
    for child in children {
        if let Some(s) = child.as_str() {
            text.push_str(s);
            continue;
        }
        let kind = child.get("type").and_then(|v| v.as_str());
        if !matches!(kind, Some("span" | "fn" | "code-span")) {
            continue;
        }
        if !text.is_empty() {
            push_plain_run(runs, &text, &mut prev_ended_space);
            text.clear();
        }
        if kind == Some("fn") {
            if !notes {
                return Err(format!("fn: {NESTED_FOOTNOTE}"));
            }
            let mut words = Vec::new();
            read_tree_runs(&mut words, child, tokens, false)?;
            if words.is_empty() {
                return Err(format!("fn: {EMPTY_FOOTNOTE}"));
            }
            runs.push(footnote_run(words, prev_ended_space));
            prev_ended_space = false;
            continue;
        }
        let raw_span: String = child
            .get("nodes").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let span_text = raw_span.split_whitespace().collect::<Vec<_>>().join(" ");
        let leading_space = raw_span.starts_with(char::is_whitespace) || prev_ended_space;
        if kind == Some("code-span") {
            if !span_text.is_empty() {
                runs.push(code_span_run(&JsonAttrs(child), span_text, leading_space, tokens)?);
            }
            prev_ended_space = raw_span.ends_with(char::is_whitespace);
            continue;
        }
        if !span_text.is_empty() {
            let color = match jattr(child, "color") {
                Some(v) => Some(tokens.resolve_color(v)?),
                None    => None,
            };
            runs.push(TextRun {
                text: span_text,
                leading_space,
                font:      jattr(child, "font").map(str::to_string),
                color,
                href:      jattr(child, "href").map(str::to_string),
                underline: jattr(child, "underline").map(|v| v == "true").unwrap_or(false),
                strike:    jattr(child, "strike")   .map(|v| v == "true").unwrap_or(false),
                bold:      jattr(child, "bold")     .map(|v| v == "true").unwrap_or(false),
                background: None,
                footnote:  None,
            });
        }
        prev_ended_space = raw_span.ends_with(char::is_whitespace);
    }
    if !text.is_empty() {
        push_plain_run(runs, &text, &mut prev_ended_space);
    }
    Ok(())
}

// ── Code ──────────────────────────────────────────────────────────────────────

/// The font a `<code>` and a `<code-span>` are set in unless they name another.
const CODE_FONT: &str = "Courier";
/// The tint behind a `<code>` and a `<code-span>` unless they name another.
const CODE_BACKGROUND: &str = "#f3f4f6";
/// The space token between the edge of a `<code>` and its text unless it has a `padding`.
const CODE_PADDING: &str = "m";
/// How many columns a tab fills to in a `<code>` unless it has a `tab-size`.
const DEFAULT_TAB_SIZE: u8 = 4;

/// The part of a `<code>` that is not a box: its text, and the spans in it.
enum CodePart<'a> {
    Text(String),
    Span(Box<dyn Attrs + 'a>, String),
}

/// A `<code>` starts on the line after its tag and ends on the line before its end tag, as a `<pre>` does, so
/// that the tags can stand on lines of their own.
pub(crate) fn without_first_newline(text: &str) -> &str {
    text.strip_prefix("\r\n").or_else(|| text.strip_prefix('\n')).unwrap_or(text)
}

/// The indent of the line that holds the end tag is not code: it is dropped, and the newline before it stays.
pub(crate) fn without_last_indent(text: &str) -> &str {
    match text.rfind('\n') {
        Some(i) if text[i + 1..].chars().all(|c| c == ' ' || c == '\t') => &text[..=i],
        _ => text,
    }
}

/// A run of code in the text's own style.
fn plain_code_run(text: &str) -> TextRun {
    TextRun {
        text: text.to_string(), leading_space: false, font: None, color: None, href: None,
        underline: false, strike: false, bold: false, background: None, footnote: None,
    }
}

/// The run of a `<span>` in a `<code>`: its text as it is written, with the span's style.
fn code_span_in_code(a: &dyn Attrs, text: String, tokens: &Tokens) -> Result<TextRun, String> {
    Ok(TextRun {
        text,
        leading_space: false,
        font:       a.get("font").map(str::to_string),
        color:      a.get("color").map(|v| tokens.resolve_color(v)).transpose()?,
        href:       a.get("href").map(str::to_string),
        underline:  a.get("underline").map(|v| v == "true").unwrap_or(false),
        strike:     a.get("strike").map(|v| v == "true").unwrap_or(false),
        bold:       a.get("bold").map(|v| v == "true").unwrap_or(false),
        background: None,
        footnote:   None,
    })
}

/// The runs of a `<code>`: its text as it is written, and a run for each `<span>`.
fn code_runs(parts: Vec<CodePart>, tokens: &Tokens) -> Result<Vec<TextRun>, String> {
    let last = parts.len().saturating_sub(1);
    let mut runs = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        match part {
            CodePart::Text(text) => {
                let mut text = text.as_str();
                if i == 0    { text = without_first_newline(text); }
                if i == last { text = without_last_indent(text); }
                if !text.is_empty() { runs.push(plain_code_run(text)); }
            }
            CodePart::Span(attrs, text) => {
                if !text.is_empty() { runs.push(code_span_in_code(attrs.as_ref(), text, tokens)?); }
            }
        }
    }
    Ok(runs)
}

/// What a `<code>` holds besides text and spans is told.
fn not_in_code(tag: &str, place: &str) -> String {
    format!("<{tag}>{place}: not allowed in <code>, which holds text and <span>s")
}

/// The `tab-size` of a `<code>`: a whole number of columns from 1 to 16.
fn parse_tab_size(v: &str) -> Result<u8, String> {
    match v.parse::<u8>() {
        Ok(columns) if (1..=16).contains(&columns) => Ok(columns),
        _ => Err(format!("invalid tab-size value: '{v}' — expected a whole number from 1 to 16")),
    }
}

/// A `<code>`: a column, which splits across pages and has the box attributes, around one text that keeps its
/// white space. XML and kit trees are read the same way. The box is tinted and padded unless it says otherwise,
/// and its text is set in the code font.
fn code_node(a: &impl Attrs, runs: Vec<TextRun>, tokens: &Tokens) -> Result<ParsedNode, String> {
    let mut boxed = ParsedNode::default_for(NodeKind::Column);
    apply_box_attrs(&mut boxed, a, tokens)?;
    if boxed.font.is_none() { boxed.font = Some(CODE_FONT.to_string()); }
    if a.get("padding").is_none() { boxed.padding = tokens.resolve_spacing(CODE_PADDING)?; }
    if a.get("background").is_none() { boxed.background = Some(tokens.resolve_fill(CODE_BACKGROUND)?); }

    let mut text = ParsedNode::default_for(NodeKind::Text);
    text.pre_tab = a.get("tab-size").map(parse_tab_size).transpose()?.unwrap_or(DEFAULT_TAB_SIZE);
    if let Some(v) = a.get("widows")      { text.widows      = parse_line_limit("widows", v)?; }
    if let Some(v) = a.get("orphans")     { text.orphans     = parse_line_limit("orphans", v)?; }
    if let Some(v) = a.get("line-height") { text.line_height = tokens.resolve_line_height(v)?; }
    text.text_runs = runs;
    boxed.children.push(text);
    Ok(boxed)
}

/// A `<code>` of an XML document. The binding of its text (`data-value`) goes to the text inside, and the rest to
/// the box, which a `data-source` repeats and a `data-if` leaves out.
fn parse_code(elem: &roxmltree::Node, tokens: &Tokens) -> Result<ParsedNode, String> {
    let at = || format!("<code>{}", node_loc(elem));
    let mut parts = Vec::new();
    for child in elem.children().filter(|c| c.is_text() || c.is_element()) {
        if child.is_text() {
            parts.push(CodePart::Text(child.text().unwrap_or("").to_string()));
        } else if child.tag_name().name() == "span" {
            let text: String = child.children().filter_map(|n| n.text().filter(|_| n.is_text())).collect();
            parts.push(CodePart::Span(Box::new(child), text));
        } else {
            return Err(not_in_code(child.tag_name().name(), &node_loc(&child)));
        }
    }
    // Adjacent text is one text, as it is in a tree; only the first and the last are trimmed.
    let runs = code_runs(merge_code_text(parts), tokens)?;
    let mut node = code_node(elem, runs, tokens).map_err(|e| format!("{}: {e}", at()))?;

    let bound = read_bound_attrs(elem);
    check_bound_attrs(&NodeKind::Column, &bound).map_err(|e| format!("{}: {e}", at()))?;
    let [value, source, only_if, unless] =
        ["data-value", "data-source", "data-if", "data-if-not"].map(|name| elem.attribute(name).map(str::to_owned));
    if source.is_some() || only_if.is_some() || unless.is_some() || !bound.is_empty() {
        node.data_attrs = Some(Box::new(DataAttrs {
            data_value: None, data_source: source, data_if: only_if, data_if_not: unless, bound,
        }));
    }
    if value.is_some() {
        node.children[0].data_attrs = Some(Box::new(DataAttrs { data_value: value, ..DataAttrs::default() }));
    }
    Ok(node)
}

/// Join the text pieces that stand next to each other: an XML parser gives them as one, and a tree may not.
fn merge_code_text(parts: Vec<CodePart>) -> Vec<CodePart> {
    let mut merged: Vec<CodePart> = Vec::new();
    for part in parts {
        match (merged.last_mut(), part) {
            (Some(CodePart::Text(last)), CodePart::Text(text)) => last.push_str(&text),
            (_, part) => merged.push(part),
        }
    }
    merged
}

/// A `code` of a kit tree: its strings and its `span` children.
fn parse_tree_code(json: &serde_json::Value, tokens: &Tokens) -> Result<ParsedNode, String> {
    let empty = Vec::new();
    let children = json.get("nodes").and_then(|v| v.as_array()).unwrap_or(&empty);
    let mut parts = Vec::new();
    for child in children {
        if let Some(s) = child.as_str() {
            parts.push(CodePart::Text(s.to_string()));
            continue;
        }
        match child.get("type").and_then(|v| v.as_str()) {
            Some("span") => {
                let text: String = child.get("nodes").and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                parts.push(CodePart::Span(Box::new(JsonAttrs(child)), text));
            }
            other => return Err(not_in_code(other.unwrap_or("?"), "")),
        }
    }
    let runs = code_runs(merge_code_text(parts), tokens)?;
    code_node(&JsonAttrs(json), runs, tokens).map_err(|e| format!("code: {e}"))
}

/// The run of a `<code-span>`: its words in the code font on a tint, unless it names others.
fn code_span_run(a: &impl Attrs, text: String, leading_space: bool, tokens: &Tokens) -> Result<TextRun, String> {
    Ok(TextRun {
        text,
        leading_space,
        font:       Some(a.get("font").unwrap_or(CODE_FONT).to_string()),
        color:      a.get("color").map(|v| tokens.resolve_color(v)).transpose()?,
        href:       None,
        underline:  false,
        strike:     false,
        bold:       false,
        background: Some(tokens.resolve_color(a.get("background").unwrap_or(CODE_BACKGROUND))?),
        footnote:   None,
    })
}

/// The footnotes in a node and below it: how many numbers a section takes.
pub(crate) fn count_footnotes(node: &Node) -> u32 {
    let own = node.text_runs.iter().filter(|r| r.footnote.is_some()).count() as u32;
    own + node.children.iter().map(count_footnotes).sum::<u32>()
}

fn parsed_has_footnote(node: &ParsedNode) -> bool {
    node.text_runs.iter().any(|r| r.footnote.is_some()) || node.children.iter().any(parsed_has_footnote)
}

/// What a footnote in a place that has no page of its own to show it on is told.
fn footnote_not_here(place: &str) -> String {
    format!("<fn> is not supported in {place}: put footnotes in the texts of the layout, outside tables and regions")
}

// ── Text, images and barcodes at fixed places ───────────────────────────────

/// The node kind a layer's `text-at`, `img-at` or `barcode-at` becomes: the one its flow version has.
pub(crate) fn at_node_kind(tag: &str) -> Option<NodeKind> {
    match tag {
        "text-at"    => Some(NodeKind::Text),
        "img-at"     => Some(NodeKind::Img),
        "barcode-at" => Some(NodeKind::Barcode),
        _            => None,
    }
}

/// What to write in a layer in place of a text, an image or a barcode of the layout.
pub(crate) fn at_element_for(tag: &str) -> Option<&'static str> {
    match tag {
        "text"    => Some("text-at"),
        "img"     => Some("img-at"),
        "barcode" => Some("barcode-at"),
        _         => None,
    }
}

/// The `width` of a `text-at` or `img-at`: a length, since there is no line for it to take its width from.
fn at_width(a: &impl Attrs) -> Result<Option<f32>, String> {
    a.get("width")
        .map(|v| parse_pt(v).ok_or_else(|| format!("invalid width '{v}': expected a pt value like 80pt")))
        .transpose()
}

/// Read the attributes of a `text-at`, `img-at` or `barcode-at` that are not about where it goes: the ones its
/// flow version has too, and the few of its own. XML and kit trees are read the same way. A layer element has
/// none of the flow's box attributes, so a `padding` or a `background` on it is not read.
fn at_node_attrs(
    kind:         NodeKind,
    a:            &impl Attrs,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    hint:         &str,
) -> Result<ParsedNode, String> {
    if a.get("data-source").is_some() {
        return Err("'data-source' is not allowed here: every copy would sit in the same place, so a list belongs in the layout".into());
    }
    let mut node = ParsedNode::default_for(kind.clone());
    let (value, only_if, unless) = (a.get("data-value"), a.get("data-if"), a.get("data-if-not"));
    let bound = read_bound_attrs(a);
    check_bound_attrs(&kind, &bound)?;
    if value.is_some() || only_if.is_some() || unless.is_some() || !bound.is_empty() {
        node.data_attrs = Some(Box::new(DataAttrs {
            data_value:  value.map(str::to_owned),
            data_source: None,
            data_if:     only_if.map(str::to_owned),
            data_if_not: unless.map(str::to_owned),
            bound,
        }));
    }
    match kind {
        NodeKind::Text => {
            if let Some(v) = a.get("font")      { node.font      = Some(v.to_string()); }
            if let Some(v) = a.get("font-size") { node.font_size = Some(tokens.resolve_text_size(v)?); }
            node.width_constraint = at_width(a)?;
            node.text_color = a.get("color").map(|v| tokens.resolve_color(v)).transpose()?;
            node.text_align = match a.get("align").unwrap_or("left") {
                "center"  => TextAlign::Center,
                "right"   => TextAlign::Right,
                "justify" => TextAlign::Justify,
                _         => TextAlign::Left,
            };
            node.bold = a.get("bold").map(|v| v == "true").unwrap_or(false);
            if let Some(v) = a.get("line-height") { node.line_height = tokens.resolve_line_height(v)?; }
            let max_lines = a.get("max-lines").map(parse_max_lines).transpose()?.unwrap_or(1);
            if max_lines > 1 && node.width_constraint.is_none() {
                return Err(format!("'max-lines' of {max_lines} needs a 'width' to wrap the text at"));
            }
            node.max_lines = Some(max_lines);
            node.ellipsis = a.get("ellipsis").map(|v| v == "true").unwrap_or(false);
        }
        NodeKind::Img => {
            node.width_constraint = at_width(a)?;
            apply_img_attrs(&mut node, a, None, asset_images, hint).map_err(|e| e.replace("<img>", "<img-at>"))?;
            if node.width_constraint.is_none() && node.img_height_constraint.is_none() {
                return Err("needs a 'width' or a 'height': on a fixed page its size must not depend on the image file's resolution".into());
            }
        }
        NodeKind::Barcode => {
            apply_barcode_attrs(&mut node, a, tokens).map_err(|e| e.replace("<barcode", "<barcode-at"))?;
            if node.width_constraint.is_none() && node.img_height_constraint.is_none() {
                return Err("needs a 'width' or a 'height': a barcode in a layer has no line to take its size from".into());
            }
        }
        _ => unreachable!("only text, image and barcode have an -at element"),
    }
    Ok(node)
}

/// Read a `<text-at>`, `<img-at>` or `<barcode-at>` as the node of its flow version. `text` is what the
/// document and the layer give its text unless it says otherwise.
pub(crate) fn parse_at_node(
    elem:         &roxmltree::Node,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    asset_fonts:  &HashMap<String, FontDef>,
    text:         &TextDefaults,
) -> Result<Node, String> {
    let kind = at_node_kind(elem.tag_name().name()).expect("a text-at, img-at or barcode-at");
    let mut node = at_node_attrs(kind.clone(), elem, tokens, asset_images, "<assets>")?;
    if kind == NodeKind::Text {
        read_text_runs(&mut node, elem, tokens)?;
        if parsed_has_footnote(&node) {
            return Err(format!("<text-at>{}: {}", node_loc(elem), footnote_not_here("a <text-at>")));
        }
    }
    Ok(resolve_parsed_node(node, text, asset_fonts))
}

/// Read a `text-at`, `img-at` or `barcode-at` of a kit tree as the node of its flow version.
pub(crate) fn parse_tree_at_node(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    asset_fonts:  &HashMap<String, FontDef>,
    text:         &TextDefaults,
) -> Result<Node, String> {
    let type_str = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let kind = at_node_kind(type_str).expect("a text-at, img-at or barcode-at");
    let mut node = at_node_attrs(kind.clone(), &JsonAttrs(json), tokens, asset_images, "assets.images")?;
    if kind == NodeKind::Text {
        read_tree_text_runs(&mut node, json, tokens)?;
        if parsed_has_footnote(&node) {
            return Err(format!("text-at: {}", footnote_not_here("a text-at")));
        }
    }
    Ok(resolve_parsed_node(node, text, asset_fonts))
}

// ── Bookmarks and the table of contents ──────────────────────────────────────

/// Read a `<bookmark>`. Its id is made from the label when `id=` is left out; `assign_bookmark_ids` settles
/// repeats once the whole document is read.
fn parse_bookmark(a: &impl Attrs) -> Result<Bookmark, String> {
    let label = a.get("label").ok_or("missing required attribute 'label'")?.to_string();
    let level = match a.get("level") {
        None    => 1,
        Some(v) => v.parse::<u8>().ok()
            .filter(|level| (1..=6).contains(level))
            .ok_or_else(|| format!("invalid level '{v}' — expected 1 to 6"))?,
    };
    let (id, explicit_id) = match a.get("id") {
        // An empty id would be the target of href="#", which inside a $toc item means that item's entry.
        Some("") => return Err("id must not be empty".into()),
        Some(id) => (id.to_string(), true),
        None     => (slug(&label), false),
    };
    Ok(Bookmark { id, label, level, explicit_id })
}

/// The id a bookmark gets from its label: lowercase, with every run of characters other than letters and
/// digits turned into one hyphen. "§ 3.1 — Methods" becomes "3-1-methods".
fn slug(label: &str) -> String {
    let words: Vec<String> = label
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() { "bookmark".to_string() } else { words.join("-") }
}

/// The bookmarks read in a list of children, waiting for the element they mark.
#[derive(Default)]
pub(crate) struct PendingBookmarks(Vec<Bookmark>);

impl PendingBookmarks {
    pub(crate) fn push(&mut self, bookmark: Bookmark) {
        self.0.push(bookmark);
    }

    /// Hand the waiting bookmarks to `node`, the element that follows them.
    pub(crate) fn attach(&mut self, node: &mut ParsedNode) {
        node.bookmarks.append(&mut self.0);
    }

    /// Fails when bookmarks are left at the end of `container`, with no element after them to mark.
    pub(crate) fn finish(self, container: &str) -> Result<(), String> {
        match self.0.first() {
            None           => Ok(()),
            Some(bookmark) => Err(format!(
                "<bookmark label=\"{}\"> is the last thing in <{container}>: a bookmark marks the element that follows it",
                bookmark.label
            )),
        }
    }
}

/// Read `type="toc"` and `toc-indent-step` on a section.
fn parse_section_toc(a: &impl Attrs, tokens: &Tokens) -> Result<Option<TocOptions>, String> {
    match a.get("type") {
        None        => Ok(None),
        Some("toc") => {
            let indent_step = match a.get("toc-indent-step") {
                Some(v) => tokens.resolve_space(v)?,
                None    => DEFAULT_TOC_INDENT_STEP,
            };
            Ok(Some(TocOptions { indent_step, data: None }))
        }
        Some(other) => Err(format!("invalid type '{other}' — expected toc")),
    }
}

/// Checks what spans sections once they are all read: one table of contents at most, and bookmark ids.
fn check_sections(sections: &mut [Section]) -> Result<(), String> {
    if sections.iter().filter(|s| s.options.toc.is_some()).count() > 1 {
        return Err("only one <section type=\"toc\"> is allowed per document".into());
    }
    assign_bookmark_ids(sections)
}

/// Settle bookmark ids in source order. Two explicit ids that are the same are an error. An id made from a
/// label takes a `-2`, `-3`… suffix when an explicit id or an earlier bookmark has it.
pub(crate) fn assign_bookmark_ids(sections: &mut [Section]) -> Result<(), String> {
    let mut bookmarks: Vec<&mut Bookmark> = Vec::new();
    for section in sections.iter_mut() {
        bookmarks.extend(section.bookmark.as_mut());
        for child in &mut section.children {
            if let SectionChild::Layout { children: layout_children, .. } = child {
                for layout_child in layout_children {
                    match layout_child {
                        LayoutChild::Content(node)  => collect_bookmarks(std::slice::from_mut(node), &mut bookmarks),
                        LayoutChild::Region(region) => collect_bookmarks(&mut region.children, &mut bookmarks),
                    }
                }
            }
        }
    }

    let mut taken: HashSet<String> = HashSet::new();
    for bookmark in bookmarks.iter().filter(|b| b.explicit_id) {
        if !taken.insert(bookmark.id.clone()) {
            return Err(format!("two bookmarks have the id '{}'", bookmark.id));
        }
    }
    for bookmark in bookmarks.iter_mut().filter(|b| !b.explicit_id) {
        let mut id = bookmark.id.clone();
        let mut suffix = 2;
        while taken.contains(&id) {
            id = format!("{}-{suffix}", bookmark.id);
            suffix += 1;
        }
        taken.insert(id.clone());
        bookmark.id = id;
    }
    Ok(())
}

fn collect_bookmarks<'a>(nodes: &'a mut [Node], out: &mut Vec<&'a mut Bookmark>) {
    for node in nodes {
        let Node { bookmarks, children, .. } = node;
        out.extend(bookmarks.iter_mut());
        collect_bookmarks(children, out);
    }
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
pub(crate) fn node_loc(elem: &roxmltree::Node) -> String {
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
pub(crate) trait Attrs {
    fn get(&self, key: &str) -> Option<&str>;
}

impl<'a, 'b> Attrs for roxmltree::Node<'a, 'b> {
    fn get(&self, key: &str) -> Option<&str> { self.attribute(key) }
}

pub(crate) struct JsonAttrs<'a>(pub &'a serde_json::Value);

impl<'a> Attrs for JsonAttrs<'a> {
    fn get(&self, key: &str) -> Option<&str> { jattr(self.0, key) }
}

fn parse_height_mode(v: &str, tokens: &Tokens) -> Result<HeightMode, String> {
    match v {
        "full" => Ok(HeightMode::Full),
        "fill" => Ok(HeightMode::Fill),
        other  => tokens.resolve_height(other)
            .map(HeightMode::Fixed)
            .map_err(|_| format!(
                "invalid height value: '{other}' — expected full, fill, {} or a pt value like 28pt",
                tokens.slot_names()
            )),
    }
}

fn parse_row_keep(v: &str) -> Result<RowKeep, String> {
    match v {
        "with-next" => Ok(RowKeep::WithNext),
        other       => Err(format!("invalid keep value: '{other}' (expected 'with-next')")),
    }
}

/// Read a `colspan` value: a whole number of columns from 1.
fn parse_col_span(v: &str) -> Result<u16, String> {
    v.parse::<u16>().ok().filter(|n| *n >= 1)
        .ok_or_else(|| format!("invalid colspan value: '{v}' — expected a whole number from 1"))
}

/// Read a `rowspan` value: a whole number of rows from 1.
fn parse_row_span(v: &str) -> Result<u16, String> {
    v.parse::<u16>().ok().filter(|n| *n >= 1)
        .ok_or_else(|| format!("invalid rowspan value: '{v}' — expected a whole number from 1"))
}

/// Checks the rows of a `<table>` as they are parsed: that the cells of each row fit the columns that the cells
/// above it that span rows leave free, and that no cell spans past the last row. `check` takes each `<thead>` or
/// `<tr>` in order, and `finish` the end of the table.
struct TableRows {
    held:     Held,
    cols:     String,
    next_row: usize,
    cells:    Vec<(u16, u16)>,
    placed:   Vec<Placed>,
}

impl TableRows {
    fn new(table_cols: &str) -> TableRows {
        TableRows {
            held: Held::new(crate::layout::table_col_count(table_cols)),
            cols: table_cols.to_string(),
            next_row: 0,
            cells: Vec::new(),
            placed: Vec::new(),
        }
    }

    fn check(&mut self, row: &ParsedNode) -> Result<(), String> {
        if row.kind == NodeKind::TableHead && row.children.iter().any(|cell| cell.row_span > 1) {
            return Err("a cell of the header cannot have a rowspan: the header is a single row".to_string());
        }
        self.cells.clear();
        self.cells.extend(row.children.iter().map(|cell| (cell.col_span, cell.row_span)));
        self.placed.clear();
        let problem = self.held.place_row(self.next_row, &self.cells, &mut self.placed);
        self.next_row += 1;
        problem.map_or(Ok(()), |problem| Err(format!("{problem} (cols=\"{}\")", self.cols)))
    }

    fn finish(&self) -> Result<(), String> {
        match self.held.unfinished() {
            Some((column, rows)) => Err(format!(
                "the cell in column {} has a rowspan that runs {rows} {} past the last row of the table",
                column + 1,
                if rows == 1 { "row" } else { "rows" }
            )),
            None => Ok(()),
        }
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
    if let Some(v) = a.get("background") { node.background = Some(tokens.resolve_fill(v)?); }
    if let Some(v) = a.get("border")     { node.border    = Some(tokens.resolve_border(v)?); }
    if let Some(v) = a.get("radius")     { node.radius    = tokens.resolve_radius(v)?; }
    // A box passes its color to the text inside it and fades itself with everything in it. A divider and a
    // barcode have a color of their own, which is not the text's.
    let is_box = matches!(
        node.kind,
        NodeKind::Column | NodeKind::Row | NodeKind::Grid | NodeKind::Frame | NodeKind::TableCell
            | NodeKind::List | NodeKind::ListItem
    );
    if is_box || node.kind == NodeKind::Text {
        if let Some(v) = a.get("color") { node.text_color = Some(tokens.resolve_color(v)?); }
    }
    if is_box || node.kind == NodeKind::Shape {
        if let Some(v) = a.get("opacity") { node.opacity = parse_opacity(v)?; }
    }
    if is_box {
        if let Some(v) = a.get("shadow")  { node.shadow  = Some(Box::new(parse_shadow(v, tokens)?)); }
    }
    if !matches!(node.kind, NodeKind::Img | NodeKind::Shape | NodeKind::Barcode | NodeKind::Field) {
        if let Some(v) = a.get("height") { node.height_mode = parse_height_mode(v, tokens)?; }
    }
    match a.get("width") {
        Some("fill") => node.width_fill = true,
        Some(v)      => node.width_constraint = Some(tokens.resolve_width(v)?),
        None         => {}
    }
    if let Some(v) = a.get("paginate") { node.paginate = parse_paginate_attr(v)?; }
    node.debug = a.get("debug").map(|v| v == "true").unwrap_or(false);
    Ok(())
}

/// Apply kind-specific layout attributes shared between the XML and JSON parsers.
/// Link and Img are omitted, and Text keeps its runs to the parsers — their content is format-specific.
fn apply_layout_kind_attrs(
    node:   &mut ParsedNode,
    a:      &impl Attrs,
    tokens: &Tokens,
) -> Result<(), String> {
    match node.kind {
        NodeKind::Column => {
            node.align   = parse_align(a.get("align").unwrap_or("stretch"))?;
            node.justify = parse_justify(a.get("justify").unwrap_or("start"))?;
        }
        NodeKind::Row => {
            node.align   = parse_align(a.get("align").unwrap_or("start"))?;
            node.justify = parse_justify(a.get("justify").unwrap_or("start"))?;
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
                None    => tokens.resolve_color("line")?,
            });
            node.thickness = tokens.resolve_border_thickness(a.get("thickness").unwrap_or("xs"))?;
        }
        NodeKind::Table => {
            if let Some(v) = a.get("cols") {
                crate::layout::check_cols(v)?;
                node.table_cols = v.to_string();
            }
            // `none` turns a stripe or a line off, which a rule of Markdown styles needs to do to a default.
            let mut style = TableStyle::default();
            if let Some(v) = a.get("stripe").filter(|v| *v != "none") {
                style.stripe = Some(tokens.resolve_color(v)?);
            }
            if let Some(v) = a.get("head-border").filter(|v| *v != "none") {
                style.head_border = Some(tokens.resolve_border(v)?);
            }
            if let Some(v) = a.get("row-border").filter(|v| *v != "none") {
                style.row_border = Some(tokens.resolve_border(v)?);
            }
            if style != TableStyle::default() {
                node.table_style = Some(Box::new(style));
            }
        }
        NodeKind::TableHead => {}
        NodeKind::TableRow => {
            if let Some(v) = a.get("keep") { node.keep = parse_row_keep(v)?; }
        }
        NodeKind::TableCell => {
            if let Some(v) = a.get("colspan") { node.col_span = parse_col_span(v)?; }
            if let Some(v) = a.get("rowspan") { node.row_span = parse_row_span(v)?; }
            node.align   = parse_align(a.get("align").unwrap_or("stretch"))?;
            let valign_str = a.get("valign").unwrap_or("top");
            node.justify = parse_justify(match valign_str {
                "top"    => "start",
                "middle" => "center",
                "bottom" => "end",
                other    => other,
            })?;
        }
        NodeKind::Text => {
            if let Some(v) = a.get("widows")  { node.widows  = parse_line_limit("widows", v)?; }
            if let Some(v) = a.get("orphans") { node.orphans = parse_line_limit("orphans", v)?; }
            if let Some(v) = a.get("line-height") { node.line_height = tokens.resolve_line_height(v)?; }
            node.max_lines = a.get("max-lines").map(parse_max_lines).transpose()?;
            node.ellipsis  = a.get("ellipsis").map(|v| v == "true").unwrap_or(false);
            apply_capture_attrs(node, a)?;
        }
        NodeKind::List => apply_list_attrs(node, a, tokens)?,
        NodeKind::ListItem => apply_item_attrs(node, a, tokens)?,
        NodeKind::Markdown => {}
        NodeKind::Link | NodeKind::Img | NodeKind::Shape | NodeKind::Barcode | NodeKind::Field => {}
    }
    Ok(())
}

/// Apply the attributes of an `<li>` / `"li"` node that `apply_box_attrs` has not read: `checked`, which makes
/// its marker a checkbox, ticked when it is true.
fn apply_item_attrs(node: &mut ParsedNode, a: &impl Attrs, tokens: &Tokens) -> Result<(), String> {
    let Some(value) = a.get("checked") else { return Ok(()) };
    let checked = match value {
        "true"  => true,
        "false" => false,
        other   => return Err(format!("invalid checked '{other}': use 'true' or 'false'")),
    };
    node.detail = Some(Box::new(Detail::Check(Check { checked, line: tokens.resolve_color("line")? })));
    Ok(())
}

/// Apply the attributes of a `<list>` / `"list"` node that `apply_box_attrs` has not read: how its items are
/// marked and how wide the marker column is. An attribute of a number list on a bullet list, and the other way
/// round, is an error, so that nothing is written that has no effect.
fn apply_list_attrs(node: &mut ParsedNode, a: &impl Attrs, tokens: &Tokens) -> Result<(), String> {
    let numbered = match a.get("type").unwrap_or("bullet") {
        "bullet" => false,
        "number" => true,
        other    => return Err(format!("invalid type '{other}': expected bullet or number")),
    };
    let only_numbered = ["format", "start", "prefix", "suffix"];
    if !numbered {
        if let Some(name) = only_numbered.iter().find(|name| a.get(name).is_some()) {
            return Err(format!("'{name}' numbers the items of a list with type=\"number\""));
        }
    } else if a.get("bullet").is_some() {
        return Err("'bullet' is the shape of a list with type=\"bullet\"".to_string());
    }
    let format = match a.get("format").unwrap_or("decimal") {
        "decimal"     => ListFormat::Decimal,
        "lower-alpha" => ListFormat::LowerAlpha,
        "upper-alpha" => ListFormat::UpperAlpha,
        "lower-roman" => ListFormat::LowerRoman,
        "upper-roman" => ListFormat::UpperRoman,
        other => return Err(format!(
            "invalid format '{other}': expected decimal, lower-alpha, upper-alpha, lower-roman or upper-roman"
        )),
    };
    let start = match a.get("start") {
        Some(v) => v.trim().parse::<i64>().map_err(|_| format!("invalid start '{v}': expected a whole number"))?,
        None    => 1,
    };
    let bullet = match a.get("bullet") {
        None           => None,
        Some("disc")   => Some(ListBullet::Disc),
        Some("circle") => Some(ListBullet::Circle),
        Some("square") => Some(ListBullet::Square),
        Some("dash")   => Some(ListBullet::Dash),
        Some(other)    => return Err(format!("invalid bullet '{other}': expected disc, circle, square or dash")),
    };
    let indent = match a.get("indent") {
        Some(v) => tokens.resolve_space(v)?,
        None    => DEFAULT_LIST_INDENT,
    };
    node.detail = Some(Box::new(Detail::List(ListSpec {
        numbered,
        format,
        start,
        prefix: a.get("prefix").unwrap_or("").to_string(),
        suffix: a.get("suffix").unwrap_or(".").to_string(),
        bullet,
        indent,
    })));
    Ok(())
}

/// Apply the attributes of a `<shape>` / `"shape"` node, which `apply_box_attrs` has already read the `width`,
/// `height`, `radius` and `opacity` of. A rect and an ellipse take a `width` and a `height`, and a circle takes
/// `r`, whose box is the square of side 2r: the node holds the size of every shape as a width and a height, as an
/// image does.
fn apply_shape_attrs(node: &mut ParsedNode, a: &impl Attrs, tokens: &Tokens) -> Result<(), String> {
    let type_str = a.get("type").ok_or("missing required attribute 'type'")?;
    let kind = match type_str {
        "rect"    => ShapeKind::Rect,
        "circle"  => ShapeKind::Circle,
        "ellipse" => ShapeKind::Ellipse,
        other     => return Err(format!("unknown type '{other}'; use 'rect', 'circle', or 'ellipse'")),
    };
    if node.width_fill {
        return Err("'width' is a length or a name of the width row, not fill".to_string());
    }
    match kind {
        ShapeKind::Circle => {
            if a.get("width").is_some() || a.get("height").is_some() {
                return Err("a circle takes 'r', not 'width' or 'height': its box is the square of side 2r".to_string());
            }
            if a.get("radius").is_some() {
                return Err("'radius' rounds the corners of a rect; a circle is round already".to_string());
            }
            let r = a.get("r").ok_or("a circle needs 'r'")?;
            let side = 2.0 * parse_measurement(r).map_err(|e| format!("invalid r '{r}': {e}"))?;
            node.width_constraint      = Some(side);
            node.img_height_constraint = Some(side);
        }
        ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::Checkbox => {
            if a.get("r").is_some() {
                return Err(format!("'r' is the radius of a circle; a {type_str} takes 'width' and 'height'"));
            }
            if kind == ShapeKind::Ellipse && a.get("radius").is_some() {
                return Err("'radius' rounds the corners of a rect; an ellipse is round already".to_string());
            }
            if node.width_constraint.is_none() || a.get("height").is_none() {
                return Err(format!("a {type_str} needs both 'width' and 'height'"));
            }
            let height = a.get("height").expect("checked above");
            node.img_height_constraint = Some(tokens.resolve_height(height).map_err(|e| format!("shape height: {e}"))?);
        }
    }
    node.detail = Some(Box::new(Detail::Shape(Shape {
        kind,
        fill:         a.get("fill").map(|v| tokens.resolve_fill(v)).transpose()?,
        stroke:       a.get("stroke").map(|v| tokens.resolve_color(v)).transpose()?,
        stroke_width: a.get("stroke-width").map(parse_measurement).transpose()?,
        stroke_dash:  a.get("stroke-dash").map(str::to_string),
        tick:         None,
    })));
    Ok(())
}

/// Read `capture`, `capture-ref`, `capture-mode` and `capture-fallback` of a text of the layout.
fn apply_capture_attrs(node: &mut ParsedNode, a: &impl Attrs) -> Result<(), String> {
    let name = |attr: &str, v: &str| -> Result<String, String> {
        let key = v.trim();
        if key.is_empty() { Err(format!("'{attr}' needs a name")) } else { Ok(key.to_string()) }
    };
    let capture = a.get("capture").map(|v| name("capture", v)).transpose()?;
    let key = a.get("capture-ref").map(|v| name("capture-ref", v)).transpose()?;
    let mode = match a.get("capture-mode") {
        None         => None,
        Some("first") => Some(CaptureMode::First),
        Some("last")  => Some(CaptureMode::Last),
        Some(other)   => return Err(format!("invalid capture-mode '{other}': expected first or last")),
    };
    let fallback = a.get("capture-fallback");
    match key {
        Some(key) => {
            if capture.is_some() {
                return Err("a text cannot have both 'capture' and 'capture-ref': one records its words, the other shows what was recorded".to_string());
            }
            let sink = CaptureRef { key, mode: mode.unwrap_or_default(), fallback: fallback.unwrap_or("").to_string() };
            node.capture = Some(Box::new(Capture { sink: Some(sink), ..Capture::default() }));
        }
        None if mode.is_some() || fallback.is_some() => {
            return Err("'capture-mode' and 'capture-fallback' belong to a text with 'capture-ref'".to_string());
        }
        None => {}
    }
    if capture.is_some() {
        node.capture = Some(Box::new(Capture { key: capture, ..Capture::default() }));
    }
    Ok(())
}

/// Read a `max-lines` value: a whole number of lines from 1.
fn parse_max_lines(v: &str) -> Result<usize, String> {
    v.parse::<usize>().ok().filter(|n| *n >= 1)
        .ok_or_else(|| format!("invalid max-lines '{v}': expected a whole number from 1"))
}

/// Read a `widows` or `orphans` value: a whole number of lines from 1 to 10.
fn parse_line_limit(attr: &str, v: &str) -> Result<u8, String> {
    match v.parse::<u8>() {
        Ok(lines) if (1..=10).contains(&lines) => Ok(lines),
        _ => Err(format!("invalid {attr} value: '{v}' — expected a whole number from 1 to 10")),
    }
}

/// Apply `name` and `height` attributes for an `<img>` / `"img"` node.
/// Works with both XML elements and JSON attr objects via the `Attrs` trait.
/// `tokens` is what a `height` may name: the flow's image takes the height row, and `None` is an `img-at` in a
/// layer, whose height is a length.
fn apply_img_attrs(
    node:         &mut ParsedNode,
    a:            &impl Attrs,
    tokens:       Option<&Tokens>,
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
        node.img_height_constraint = Some(match tokens {
            Some(tokens) => tokens.resolve_height(v).map_err(|e| format!("img height: {e}"))?,
            None         => parse_pt(v).ok_or_else(|| format!("img height: invalid pt value '{v}'"))?,
        });
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
    if a.get("size").is_some() {
        return Err("'size' removed; use 'width' or 'height', which a QR code takes either of".to_string());
    }
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

    // Dimensions: `width` and `height`, for every type.  A QR code is square, so it takes
    // either one, and the smaller when it has both.  `width="fill"` sizes the slot of a QR code
    // in a row, not the code itself, so then only `height` sets its size.
    let width = a.get("width").filter(|w| *w != "fill")
        .map(|w| tokens.resolve_width(w).map_err(|_| format!("<barcode> invalid width '{w}'")))
        .transpose()?;
    let height = a.get("height")
        .map(|h| tokens.resolve_height(h).map_err(|_| format!("<barcode> invalid height '{h}'")))
        .transpose()?;
    match node.barcode_type.as_ref().unwrap() {
        BarcodeType::Qr => {
            let pt = match (width, height) {
                (Some(w), Some(h)) => w.min(h),
                (Some(side), None) | (None, Some(side)) => side,
                (None, None) => return Err(
                    "<barcode type=\"qr\"> needs a 'height', or a 'width' that is a length".to_string(),
                ),
            };
            node.width_constraint      = Some(pt);
            node.img_height_constraint = Some(pt);
        }
        BarcodeType::Code128 | BarcodeType::Ean13 => {
            if width.is_some() { node.width_constraint = width; }
            node.img_height_constraint = height;
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
            tokens.resolve_height(v)
                  .map_err(|_| format!("<field> invalid height '{v}'"))?
        );
    }

    Ok(())
}

/// Parse a JSON document tree (produced by `LpdfKit`) into a `Document`.
// The WASI binary reads every document with its resources, so this is for the engine and the tests.
#[allow(dead_code)]
pub fn parse_tree(json: &str) -> Result<Document, String> {
    parse_tree_with(json, &Resources::default())
}

/// Parse a kit tree with the files its `src` attributes name loaded in `resources`.
pub fn parse_tree_with(json: &str, resources: &Resources) -> Result<Document, String> {
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
    let mut image_srcs:        HashMap<String, String>     = HashMap::new();
    if let Some(assets) = attrs.get("assets").and_then(|v| v.as_object()) {
        parse_tree_assets(assets, &mut asset_fonts, &mut asset_images, &mut image_srcs, &mut asset_font_widths)?;
    }
    let markdown_assets = markdown::Assets { image_srcs, styles_files: resources.markdown_styles.clone() };

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
    let doc_bg_s      = attrs.get("background") .and_then(|v| v.as_str());

    if attrs.contains_key("margin") {
        return Err(format!("document: {MARGIN_MOVED}"));
    }
    let mut doc_size = if let Some(s) = doc_size_str {
        parse_page_size(s)?
    } else {
        (595.28_f32, 841.89_f32) // a4
    };
    if doc_orient == Some("landscape") {
        doc_size = (doc_size.1, doc_size.0);
    }
    let doc_background = if let Some(s) = doc_bg_s {
        Some(tokens.resolve_fill(s)?)
    } else {
        None
    };
    let doc_debug = attrs.get("debug").and_then(|v| v.as_str())
        .map(|v| v == "true").unwrap_or(false);
    let doc_font = attrs.get("font").and_then(|v| v.as_str());
    let doc_footnote_counter = attrs.get("footnote-counter").and_then(|v| v.as_str())
        .map(FootnoteCounter::parse)
        .transpose()
        .map_err(|e| format!("document: {e}"))?;

    // ── Sections ──────────────────────────────────────────────────────────────
    let nodes_arr = root.get("nodes")
        .and_then(|v| v.as_array())
        .ok_or("tree JSON must have a 'nodes' array")?;

    let mut sections: Vec<Section> = Vec::new();
    for sec_json in nodes_arr {
        sections.push(parse_tree_section(
            sec_json, doc_size, doc_background.clone(),
            doc_debug, doc_font, &tokens, &asset_fonts, &asset_images, &markdown_assets,
        )?);
    }
    if sections.is_empty() {
        return Err("document must have at least one section".into());
    }
    markdown::expand_pending(&mut sections, None, markdown::Pass::Read)?;
    check_sections(&mut sections)?;
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
        background:  doc_background,
        gradients:   tokens.gradients.clone(),
        colors:      tokens.colors.clone(),
        debug:       doc_debug,
        footnote_counter: doc_footnote_counter,
        sections,
    })
}

pub fn parse_tree_tokens_pub(
    obj: &serde_json::Map<String, serde_json::Value>,
    tokens: &mut Tokens,
) -> Result<(), String> {
    use crate::tokens::scale_idx;

    let apply_row = |json_key: &str, scale: &mut [f32; 6], parse: fn(&str) -> Option<f32>| -> Result<(), String> {
        if let Some(map) = obj.get(json_key).and_then(|v| v.as_object()) {
            for (k, v) in map {
                if let (Some(idx), Some(s)) = (scale_idx(k), v.as_str()) {
                    scale[idx] = parse(s)
                        .ok_or_else(|| format!("invalid {json_key} token '{k}': '{s}'"))?;
                }
            }
        }
        Ok(())
    };

    apply_row("space",  &mut tokens.space, parse_pt)?;
    apply_row("border", &mut tokens.border, parse_pt)?;
    apply_row("radius", &mut tokens.radius, parse_pt)?;
    apply_row("width",  &mut tokens.width, parse_pt)?;
    apply_row("height", &mut tokens.height, parse_pt)?;
    apply_row("grid",  &mut tokens.grid_col, parse_pt)?;
    apply_row("text-size", &mut tokens.text_size, parse_pt)?;
    apply_row("line-height", &mut tokens.line_height, parse_factor)?;

    if let Some(aliases) = obj.get("aliases").and_then(|v| v.as_object()) {
        for (name, value) in aliases {
            let slot = value
                .as_str()
                .ok_or_else(|| format!("alias '{name}': expected a slot from xs to xxl as text"))?;
            tokens.add_alias(name, slot)?;
        }
    }

    if let Some(colors) = obj.get("colors").and_then(|v| v.as_object()) {
        for (k, v) in colors {
            if let Some(s) = v.as_str() {
                tokens.colors.insert(k.clone(), crate::tokens::normalize_hex(s)?);
            }
        }
    }

    // Gradients name colors of the row above, so they are read after it.
    if let Some(gradients) = obj.get("gradients").and_then(|v| v.as_object()) {
        for (name, def) in gradients {
            let resolved = parse_tree_gradient(name, def, tokens)?;
            tokens.gradients.insert(name.clone(), resolved);
        }
    }

    Ok(())
}

/// Read one gradient of a Kit tree's tokens: `{ type, from, to, angle, stops: [{ offset, color }] }`. The angle
/// may be a number or a string.
fn parse_tree_gradient(name: &str, def: &serde_json::Value, tokens: &Tokens) -> Result<Gradient, String> {
    let text = |key: &str| {
        def.get(key).and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _                            => None,
        })
    };
    let kind = text("type").ok_or_else(|| format!("gradient '{name}': missing type"))?;
    let (angle, from, to) = (text("angle"), text("from"), text("to"));

    let mut stops: Vec<(String, String)> = Vec::new();
    for stop in def.get("stops").and_then(|v| v.as_array()).into_iter().flatten() {
        let field = |key: &str| {
            stop.get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("gradient '{name}': a stop needs an {key}"))
        };
        stops.push((field("offset")?, field("color")?));
    }

    let spec = GradientSpec {
        kind:  &kind,
        angle: angle.as_deref(),
        from:  from.as_deref(),
        to:    to.as_deref(),
        stops: stops.iter().map(|(offset, color)| (offset.as_str(), color.as_str())).collect(),
    };
    tokens.resolve_gradient(name, &spec)
}


#[allow(clippy::too_many_arguments)]
fn parse_tree_section(
    json:           &serde_json::Value,
    doc_size:       (f32, f32),
    doc_background: Option<String>,
    doc_debug:      bool,
    doc_font:       Option<&str>,
    tokens:         &Tokens,
    asset_fonts:    &HashMap<String, FontDef>,
    asset_images:   &HashMap<String, String>,
    markdown:       &markdown::Assets,
) -> Result<Section, String> {
    if json.get("attrs").is_some_and(|attrs| attrs.get("margin").is_some()) {
        return Err(format!("section: {MARGIN_MOVED}"));
    }
    let mut size       = doc_size;
    let mut background = doc_background;
    let a = JsonAttrs(json);
    let debug = apply_page_overrides(&mut size, &mut background, &a, tokens, doc_debug)?;
    let title = jattr(json, "title").map(str::to_string);
    let toc = parse_section_toc(&a, tokens).map_err(|e| format!("section: {e}"))?;
    let footnote_counter = a.get("footnote-counter")
        .map(FootnoteCounter::parse)
        .transpose()
        .map_err(|e| format!("section: {e}"))?;

    let text = TextDefaults::of_section(doc_font, tokens, asset_fonts);

    let mut children: Vec<SectionChild> = Vec::new();
    let mut bookmark: Option<Bookmark> = None;
    let mut has_markdown = false;
    if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
        for child in arr {
            let kind = child.get("type").and_then(|v| v.as_str());
            match kind {
                Some("markdown") => {
                    if children.iter().any(|c| matches!(c, SectionChild::Layout { .. })) {
                        return Err(format!("node type 'markdown': {ONE_FLOW}"));
                    }
                    if toc.is_some() {
                        return Err(format!("node type 'markdown': {NO_MARKDOWN_IN_TOC}"));
                    }
                    has_markdown = true;
                    let cx = markdown::SectionContext {
                        tokens, fonts: asset_fonts, images: asset_images, image_srcs: &markdown.image_srcs,
                        defaults: &text, styles_files: &markdown.styles_files,
                    };
                    children.push(markdown::parse_tree(child, &cx)?);
                }
                Some("layout") => {
                    if children.iter().any(|c| matches!(c, SectionChild::Layout { .. })) {
                        return Err(format!("node type 'layout': {}", if has_markdown { ONE_FLOW } else { SECOND_LAYOUT }));
                    }
                    let margin = layout_margin(&JsonAttrs(child), tokens).map_err(|e| format!("layout: {e}"))?;
                    let mut lc: Vec<LayoutChild> = Vec::new();
                    let mut pending = PendingBookmarks::default();
                    if let Some(nodes) = child.get("nodes").and_then(|v| v.as_array()) {
                        for n in nodes {
                            if n.get("type").and_then(|v| v.as_str()) == Some("region") {
                                lc.push(LayoutChild::Region(parse_tree_region(n, tokens, asset_images, asset_fonts, &text)?));
                            } else if is_tree_bookmark(n) {
                                pending.push(tree_bookmark(n)?);
                            } else {
                                let mut node = parse_tree_node(n, tokens, asset_images)?;
                                pending.attach(&mut node);
                                lc.push(LayoutChild::Content(resolve_parsed_node(node, &text, asset_fonts)));
                            }
                        }
                    }
                    pending.finish("layout")?;
                    children.push(SectionChild::Layout { margin, gap: 0.0, children: lc });
                }
                Some("bookmark") => {
                    if bookmark.is_some() {
                        return Err("a section has one bookmark of its own; put the others inside its layout".into());
                    }
                    bookmark = Some(tree_bookmark(child)?);
                }
                Some("layer") => {
                    let ctx = canvas::LayerContext {
                        tokens, asset_images, asset_fonts,
                        text: &text, page_w: size.0, page_h: size.1,
                    };
                    children.push(SectionChild::Layer(canvas::parse_tree_layer(child, &ctx)?));
                }
                Some("canvas") => return Err(format!("node type 'canvas': {CANVAS_REMOVED}")),
                other => return Err(format!(
                    "section child 'type' must be 'bookmark', 'layer', 'layout' or 'markdown', got {:?}", other
                )),
            }
        }
    }

    if children.is_empty() {
        return Err("section must have at least one layout or layer child".into());
    }

    Ok(Section {
        children,
        options: SectionOptions {
            size:       if size != doc_size { Some(size) } else { None },
            background,
            debug:      Some(debug),
            title,
            toc,
            footnote_counter,
        },
        bookmark,
    })
}

fn is_tree_bookmark(json: &serde_json::Value) -> bool {
    json.get("type").and_then(|v| v.as_str()) == Some("bookmark")
}

fn tree_bookmark(json: &serde_json::Value) -> Result<Bookmark, String> {
    parse_bookmark(&JsonAttrs(json)).map_err(|e| format!("bookmark: {e}"))
}

fn parse_tree_region(
    json:         &serde_json::Value,
    tokens:       &Tokens,
    asset_images: &HashMap<String, String>,
    asset_fonts:  &HashMap<String, FontDef>,
    text:         &TextDefaults,
) -> Result<LayoutRegion, String> {
    let pin_str = jattr(json, "pin")
        .ok_or("region missing 'pin' attribute")?;
    let pin = match pin_str {
        "top"    => RegionPin::Top,
        "bottom" => RegionPin::Bottom,
        "left"   => RegionPin::Left,
        "right"  => RegionPin::Right,
        other => return Err(format!("region invalid pin '{other}'")),
    };
    let page  = jattr(json, "page").map(parse_page_scope).transpose()?;
    let w     = jattr(json, "w").map(parse_measurement).transpose()?;
    let debug = jattr(json, "debug").map(|v| v == "true").unwrap_or(false);

    let mut children = Vec::new();
    if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
        let mut pending = PendingBookmarks::default();
        for n in arr {
            if is_tree_bookmark(n) {
                pending.push(tree_bookmark(n)?);
                continue;
            }
            let mut node = parse_tree_node(n, tokens, asset_images)?;
            pending.attach(&mut node);
            children.push(resolve_parsed_node(node, text, asset_fonts));
        }
        pending.finish("region")?;
    }
    if children.iter().any(|c| count_footnotes(c) > 0) {
        return Err(format!("region: {}", footnote_not_here("a region")));
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
    if type_str == "code" {
        return parse_tree_code(json, tokens);
    }

    let kind = match type_str {
        "column"  => NodeKind::Column,
        "row"     => NodeKind::Row,
        "grid"    => NodeKind::Grid,
        "frame"   => NodeKind::Frame,
        "divider" => NodeKind::Divider,
        "text"    => NodeKind::Text,
        "link"    => NodeKind::Link,
        "img"     => NodeKind::Img,
        "shape"   => NodeKind::Shape,
        "barcode" => NodeKind::Barcode,
        "field"   => NodeKind::Field,
        "table"   => NodeKind::Table,
        "thead"   => NodeKind::TableHead,
        "tr"      => NodeKind::TableRow,
        "td"      => NodeKind::TableCell,
        "list"    => NodeKind::List,
        "li"      => NodeKind::ListItem,
        other => return Err(match replaced_element_hint(other) {
            Some(hint) => format!("node type '{other}': {hint}"),
            None       => format!("unknown node type: '{other}'"),
        }),
    };

    let mut node = ParsedNode::default_for(kind.clone());
    let a = JsonAttrs(json);
    apply_box_attrs(&mut node, &a, tokens)?;
    apply_layout_kind_attrs(&mut node, &a, tokens)?;

    match kind {
        NodeKind::Text => {
            node.text_align = match jattr(json, "align").unwrap_or("left") {
                "center"  => TextAlign::Center,
                "right"   => TextAlign::Right,
                "justify" => TextAlign::Justify,
                _         => TextAlign::Left,
            };
            node.bold = jattr(json, "bold").map(|v| v == "true").unwrap_or(false);

            read_tree_text_runs(&mut node, json, tokens)?;
            return Ok(node); // text nodes have no layout children
        }
        NodeKind::Link => {
            node.url = Some(
                jattr(json, "href")
                    .ok_or("<link> missing required attribute 'href'")?
                    .to_string(),
            );
        }
        NodeKind::Img => {
            apply_img_attrs(&mut node, &a, Some(tokens), asset_images, "assets.images")?;
        }
        NodeKind::Shape => {
            apply_shape_attrs(&mut node, &a, tokens).map_err(|e| format!("shape: {e}"))?;
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
    if !matches!(kind, NodeKind::Divider | NodeKind::Img | NodeKind::Shape | NodeKind::Barcode | NodeKind::Field) {
        if let Some(arr) = json.get("nodes").and_then(|v| v.as_array()) {
            let mut pending = PendingBookmarks::default();
            let mut table_rows = (kind == NodeKind::Table).then(|| TableRows::new(&node.table_cols));
            for child in arr {
                if is_tree_bookmark(child) {
                    if matches!(kind, NodeKind::Table | NodeKind::TableHead | NodeKind::TableRow) {
                        return Err(format!("bookmark not allowed in '{type_str}' — put it inside a 'td'"));
                    }
                    pending.push(tree_bookmark(child)?);
                    continue;
                }
                let mut child_node = parse_tree_node(child, tokens, asset_images)?;
                pending.attach(&mut child_node);
                match kind {
                    NodeKind::Table => {
                        if !matches!(child_node.kind, NodeKind::TableHead | NodeKind::TableRow) {
                            return Err(format!(
                                "table children must be 'thead' or 'tr', got '{}'",
                                child.get("type").and_then(|v| v.as_str()).unwrap_or("?")
                            ));
                        }
                        if let Some(rows) = table_rows.as_mut() {
                            rows.check(&child_node)
                                .map_err(|e| format!("{}: {e}", child.get("type").and_then(|v| v.as_str()).unwrap_or("?")))?;
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
                    NodeKind::List => {
                        if child_node.kind != NodeKind::ListItem {
                            return Err(format!(
                                "list children must be 'li', got '{}'",
                                child.get("type").and_then(|v| v.as_str()).unwrap_or("?")
                            ));
                        }
                    }
                    _ => {
                        if child_node.kind == NodeKind::ListItem {
                            return Err(format!("'li' belongs inside a 'list', not in '{type_str}'"));
                        }
                    }
                }
                node.children.push(child_node);
            }
            if let Some(rows) = &table_rows {
                rows.finish().map_err(|e| format!("table: {e}"))?;
            }
            pending.finish(type_str)?;
        }
        if kind == NodeKind::Frame && node.children.len() > 1 {
            return Err(format!(
                "frame accepts at most one child; got {}",
                node.children.len()
            ));
        }
        if kind == NodeKind::Table && node.children.iter().any(parsed_has_footnote) {
            return Err(format!("table: {}", footnote_not_here("a table")));
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
            r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt"/></section></document></lpdf>"#.to_string()
        } else {
            format!(
                r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
            )
        }
    }

    // ── code ─────────────────────────────────────────────────────────────────

    /// The first layout node of a document that holds `body`.
    fn first_node(body: &str) -> Node {
        let mut doc = parse(&minimal(body)).unwrap();
        doc.section_layouts().remove(0).children.remove(0)
    }

    // ── lists ────────────────────────────────────────────────────────────────

    fn list_error(body: &str) -> String {
        parse(&minimal(body)).unwrap_err()
    }

    #[test]
    fn a_list_and_its_items_are_in_the_tree_until_the_data_is_in() {
        let mut doc = parse(&minimal("<list><li><text>A</text></li><li><text>B</text></li></list>")).unwrap();
        let section = &mut doc.sections[0];
        let SectionChild::Layout { children, .. } = &mut section.children[0] else { panic!("no layout") };
        let LayoutChild::Content(list) = &children[0] else { panic!("no content") };
        assert_eq!(list.kind, NodeKind::List);
        assert!(matches!(list.detail.as_deref(), Some(Detail::List(spec)) if !spec.numbered && spec.indent == DEFAULT_LIST_INDENT));
        assert!(list.children.len() == 2 && list.children.iter().all(|item| item.kind == NodeKind::ListItem));
    }

    #[test]
    fn a_list_takes_its_numbers_from_type_format_start_prefix_suffix_and_indent() {
        let mut doc = parse(&minimal(
            r#"<list type="number" format="upper-roman" start="4" prefix="(" suffix=")" indent="30pt"><li><text>A</text></li></list>"#,
        ))
        .unwrap();
        let SectionChild::Layout { children, .. } = &mut doc.sections[0].children[0] else { panic!("no layout") };
        let LayoutChild::Content(list) = &children[0] else { panic!("no content") };
        let Some(Detail::List(spec)) = list.detail.as_deref() else { panic!("no list spec") };
        assert_eq!(
            (spec.numbered, spec.format, spec.start, spec.prefix.as_str(), spec.suffix.as_str(), spec.indent),
            (true, ListFormat::UpperRoman, 4, "(", ")", 30.0)
        );
    }

    #[test]
    fn an_item_belongs_in_a_list_and_a_list_holds_items_only() {
        let err = list_error("<column><li><text>A</text></li></column>");
        assert!(err.contains("<li>") && err.contains("belongs inside a <list>"), "{err}");
        let err = list_error("<list><text>A</text></list>");
        assert!(err.contains("<list>") && err.contains("expected <li>"), "{err}");
    }

    #[test]
    fn what_numbers_a_list_is_refused_on_a_bullet_list_and_the_bullet_on_a_number_list() {
        for attr in [r#"format="lower-alpha""#, r#"start="2""#, r#"prefix="(""#, r#"suffix=")""#] {
            let err = list_error(&format!("<list {attr}><li><text>A</text></li></list>"));
            assert!(err.contains("numbers the items of a list with type=\"number\""), "{attr}: {err}");
        }
        let err = list_error(r#"<list type="number" bullet="dash"><li><text>A</text></li></list>"#);
        assert!(err.contains("'bullet' is the shape of a list with type=\"bullet\""), "{err}");
    }

    #[test]
    fn a_list_names_what_it_expected_for_a_value_it_cannot_read() {
        for (attrs, expected) in [
            (r#"type="roman""#, "expected bullet or number"),
            (r#"type="number" format="greek""#, "expected decimal, lower-alpha"),
            (r#"type="number" start="one""#, "expected a whole number"),
            (r#"bullet="star""#, "expected disc, circle, square or dash"),
        ] {
            let err = list_error(&format!("<list {attrs}><li><text>A</text></li></list>"));
            assert!(err.contains(expected), "{attrs}: {err}");
        }
    }

    #[test]
    fn a_code_is_a_padded_tinted_column_around_a_text_that_keeps_its_white_space() {
        let code = first_node("<code>\n  a  b\n</code>");
        assert_eq!(code.kind, NodeKind::Column);
        assert_eq!(code.padding, [8.0; 4]);
        assert_eq!(code.background.as_deref(), Some("#f3f4f6"));
        assert_eq!(code.children.len(), 1);
        let text = &code.children[0];
        assert_eq!(text.kind, NodeKind::Text);
        assert_eq!(text.font, "Courier");
        assert_eq!(text.pre_tab, 4);
        assert_eq!(text.text_runs.len(), 1);
        assert_eq!(text.text_runs[0].text, "  a  b\n");
    }

    #[test]
    fn a_code_drops_the_line_break_after_its_start_tag_and_the_indent_before_its_end_tag() {
        let text = first_node("<column>\n    <code>\n        a\n          b\n    </code>\n</column>").children.remove(0).children.remove(0);
        assert_eq!(text.text_runs[0].text, "        a\n          b\n");
    }

    #[test]
    fn a_code_on_one_line_is_its_text_as_it_is() {
        let text = first_node("<code>  a  b  </code>").children.remove(0);
        assert_eq!(text.text_runs[0].text, "  a  b  ");
    }

    #[test]
    fn a_code_takes_the_box_attributes_and_the_font_a_code_is_set_in() {
        let code = first_node(r##"<code font="Times-Roman" font-size="s" color="#336699" padding="2pt" background="#ffeeee" border="1pt #000000" radius="4pt">x</code>"##);
        assert_eq!(code.padding, [2.0; 4]);
        assert_eq!(code.background.as_deref(), Some("#ffeeee"));
        assert_eq!(code.radius, 4.0);
        let text = &code.children[0];
        assert_eq!(text.font, "Times-Roman");
        assert_eq!(text.font_size, 9.0);
        assert_eq!(text.text_color.as_deref(), Some("#336699"));
    }

    #[test]
    fn a_code_has_a_tab_size_widows_orphans_and_line_height_for_its_text() {
        let text = first_node(r#"<code tab-size="8" widows="3" orphans="4" line-height="1.5">x</code>"#).children.remove(0);
        assert_eq!((text.pre_tab, text.widows, text.orphans, text.line_height), (8, 3, 4, 1.5));
    }

    #[test]
    fn a_span_in_a_code_keeps_its_white_space_and_its_style() {
        let text = first_node("<code>let <span color=\"#c0392b\" bold=\"true\">  x  </span> = 1;</code>").children.remove(0);
        let runs: Vec<(&str, Option<&str>)> = text.text_runs.iter().map(|r| (r.text.as_str(), r.color.as_deref())).collect();
        assert_eq!(runs, vec![("let ", None), ("  x  ", Some("#c0392b")), (" = 1;", None)]);
        assert_eq!(text.text_runs[1].font.as_deref(), Some("Courier-Bold"));
    }

    #[test]
    fn a_cdata_section_holds_the_code_of_a_code_as_it_is() {
        let text = first_node("<code><![CDATA[\nif a < b && c {\n\tx();\n}\n]]></code>").children.remove(0);
        let runs: Vec<&str> = text.text_runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(runs, vec!["if a < b && c {\n\tx();\n}\n"]);
    }

    #[test]
    fn a_code_holds_text_and_spans_only() {
        let err = parse(&minimal("<code>a<frame/></code>")).unwrap_err();
        assert!(err.contains("<frame>") && err.contains("not allowed in <code>"), "{err}");
        let err = parse(&minimal("<code>a<code-span>b</code-span></code>")).unwrap_err();
        assert!(err.contains("<code-span>") && err.contains("not allowed in <code>"), "{err}");
        let err = parse(&minimal("<code>a<fn>b</fn></code>")).unwrap_err();
        assert!(err.contains("<fn>") && err.contains("not allowed in <code>"), "{err}");
    }

    #[test]
    fn a_tab_size_is_a_whole_number_of_columns_from_1_to_16() {
        for bad in ["0", "17", "two", "-1", "2.5"] {
            let err = parse(&minimal(&format!(r#"<code tab-size="{bad}">x</code>"#))).unwrap_err();
            assert!(err.contains("invalid tab-size value") && err.contains(bad), "{err}");
        }
        for good in ["1", "16"] {
            assert!(parse(&minimal(&format!(r#"<code tab-size="{good}">x</code>"#))).is_ok());
        }
    }

    #[test]
    fn data_attributes_of_a_code_go_to_the_text_inside_or_the_box() {
        let code = first_node(r#"<code data-value="snippet" data-if="show" data-source="rows">x</code>"#);
        let on_box = code.data_attrs.as_ref().unwrap();
        assert_eq!((on_box.data_source.as_deref(), on_box.data_if.as_deref(), on_box.data_value.as_deref()), (Some("rows"), Some("show"), None));
        let on_text = code.children[0].data_attrs.as_ref().unwrap();
        assert_eq!(on_text.data_value.as_deref(), Some("snippet"));
    }

    #[test]
    fn a_code_span_is_a_run_in_the_code_font_on_a_tint() {
        let text = first_node("<text>Run <code-span>cargo test</code-span> now</text>");
        let runs: Vec<(&str, bool)> = text.text_runs.iter().map(|r| (r.text.as_str(), r.leading_space)).collect();
        assert_eq!(runs, vec![("Run", false), ("cargo test", true), ("now", true)]);
        assert_eq!(text.text_runs[1].font.as_deref(), Some("Courier"));
        assert_eq!(text.text_runs[1].background.as_deref(), Some("#f3f4f6"));
    }

    #[test]
    fn a_code_span_takes_a_font_a_color_and_a_tint_of_its_own() {
        let text = first_node(r##"<text><code-span font="Times-Roman" color="#336699" background="#ffff00">x</code-span></text>"##);
        let run = &text.text_runs[0];
        assert_eq!(run.font.as_deref(), Some("Times-Roman"));
        assert_eq!(run.color.as_deref(), Some("#336699"));
        assert_eq!(run.background.as_deref(), Some("#ffff00"));
    }

    /// The words of each run of a text, and whether a space leads it.
    fn spaced_runs(text: &Node) -> Vec<(&str, bool)> {
        text.text_runs.iter().map(|r| (r.text.as_str(), r.leading_space)).collect()
    }

    /// The first node of a kit tree whose layout holds `child`.
    fn first_tree_node(child: &str) -> Node {
        let tree = format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layout","attrs":{{}},"nodes":[{child}]}}]}}]}}"#
        );
        let mut doc = parse_tree(&tree).unwrap();
        doc.section_layouts().remove(0).children.remove(0)
    }

    #[test]
    fn a_span_or_a_code_span_that_starts_with_a_space_keeps_it_after_another() {
        let xml = first_node(concat!(
            r#"<text><span bold="true">per </span><code-span>x</code-span><span bold="true"> call</span>"#,
            r#"<span> a</span><code-span> b</code-span><code-span> c</code-span></text>"#,
        ));
        let tree = first_tree_node(concat!(
            r#"{"type":"text","attrs":{},"nodes":[{"type":"span","attrs":{"bold":"true"},"nodes":["per "]},"#,
            r#"{"type":"code-span","attrs":{},"nodes":["x"]},{"type":"span","attrs":{"bold":"true"},"nodes":[" call"]},"#,
            r#"{"type":"span","attrs":{},"nodes":[" a"]},{"type":"code-span","attrs":{},"nodes":[" b"]},"#,
            r#"{"type":"code-span","attrs":{},"nodes":[" c"]}]}"#,
        ));
        let spaced = [("per", false), ("x", true), ("call", true), ("a", true), ("b", true), ("c", true)];
        assert_eq!(spaced_runs(&xml), spaced, "XML");
        assert_eq!(spaced_runs(&tree), spaced, "kit tree");
    }

    #[test]
    fn no_space_is_added_after_a_code_span_where_the_source_has_none() {
        let xml = first_node(r#"<text>the <code-span>x</code-span>'s and <code-span>y</code-span><span bold="true">'s</span></text>"#);
        let tree = first_tree_node(concat!(
            r#"{"type":"text","attrs":{},"nodes":["the ",{"type":"code-span","attrs":{},"nodes":["x"]},"'s and ","#,
            r#"{"type":"code-span","attrs":{},"nodes":["y"]},{"type":"span","attrs":{"bold":"true"},"nodes":["'s"]}]}"#,
        ));
        let attached = [("the", false), ("x", true), ("'s and", false), ("y", true), ("'s", false)];
        assert_eq!(spaced_runs(&xml), attached, "XML");
        assert_eq!(spaced_runs(&tree), attached, "kit tree");
    }

    #[test]
    fn a_code_of_a_tree_is_read_like_the_code_of_its_xml() {
        let tree = r##"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","nodes":[
            {"type":"code","attrs":{"tab-size":"2"},"nodes":["\nlet ",{"type":"span","attrs":{"color":"#c0392b"},"nodes":["x"]},
            " = 1;\n"]}]}]}]}"##;
        let mut doc = parse_tree(tree).unwrap();
        let code = doc.section_layouts().remove(0).children.remove(0);
        assert_eq!(code.padding, [8.0; 4]);
        let text = &code.children[0];
        assert_eq!(text.pre_tab, 2);
        let runs: Vec<&str> = text.text_runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(runs, vec!["let ", "x", " = 1;\n"]);
    }

    #[test]
    fn a_node_stays_small() {
        // Parse and data binding copy nodes whole, so a larger node is a slower parse: see the comment on `Node`.
        // Raise a limit only with a benchmark of parse that shows the room is worth it.
        assert!(std::mem::size_of::<Node>() <= 704, "Node is {} bytes", std::mem::size_of::<Node>());
        assert!(std::mem::size_of::<ParsedNode>() <= 704, "ParsedNode is {} bytes", std::mem::size_of::<ParsedNode>());
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
    fn parse_column_gap_and_children() {
        let mut doc = parse(&minimal(
            r#"<column gap="m"><frame /><frame /></column>"#,
        ))
        .unwrap();
        let pages = doc.section_layouts();
        let column = &pages[0].children[0];
        assert_eq!(column.gap, 8.0);
        assert_eq!(column.children.len(), 2);
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
    fn a_divider_is_a_thin_line_in_the_line_color_unless_it_says_otherwise() {
        let mut doc = parse(&minimal(r#"<divider/>"#)).unwrap();
        let d = &doc.section_layouts()[0].children[0];
        assert_eq!((d.color.as_deref(), d.thickness), (Some("#737373"), 0.5));

        let mut doc = parse(
            r##"<lpdf version="1"><tokens><colors><color name="line" value="#336699"/></colors></tokens>
            <document size="a4"><section><layout margin="28pt"><divider/></layout></section></document></lpdf>"##,
        )
        .unwrap();
        let d = &doc.section_layouts()[0].children[0];
        assert_eq!(d.color.as_deref(), Some("#336699"));
    }

    #[test]
    fn a_table_is_striped_and_lined_by_its_attributes_and_none_turns_each_off() {
        let style = |attrs: &str| {
            let mut doc = parse(&minimal(&format!(r#"<table cols="1fr" {attrs}><tr><td/></tr></table>"#))).unwrap();
            doc.section_layouts()[0].children[0].table_style.clone()
        };
        assert_eq!(style(""), None);
        assert_eq!(style(r#"stripe="none" head-border="none" row-border="none""#), None);
        let style = style(r##"stripe="#eeeeee" head-border="xs line" row-border="xs line-muted""##).unwrap();
        assert_eq!(style.stripe.as_deref(), Some("#eeeeee"));
        assert_eq!(style.head_border, Some((0.5, "#737373".to_string())));
        assert_eq!(style.row_border, Some((0.5, "#d4d4d4".to_string())));
    }

    #[test]
    fn a_line_of_a_table_that_does_not_read_is_an_error() {
        let err = parse(&minimal(r#"<table cols="1fr" row-border="thin"><tr><td/></tr></table>"#)).unwrap_err();
        assert!(err.contains("row-border") || err.contains("border requires"), "{err}");
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
    fn text_reads_its_line_limits_and_has_no_cap_by_default() {
        let text = |attrs: &str| {
            let mut doc = parse(&minimal(&format!("<text {attrs}>Hello world</text>"))).unwrap();
            let t = doc.section_layouts().remove(0).children.remove(0);
            (t.max_lines, t.ellipsis)
        };
        // A text of the flow wraps at the width it has, so a cap above 1 needs no width of its own.
        assert_eq!(text(r#"max-lines="3" ellipsis="true""#), (Some(3), true));
        assert_eq!(text(""), (None, false));
        for value in ["0", "-1", "two", "1.5"] {
            let err = parse(&minimal(&format!(r#"<text max-lines="{value}">Hi</text>"#))).unwrap_err();
            assert!(err.contains(&format!("invalid max-lines '{value}'")) && err.contains("at line"), "{err}");
        }
    }

    #[test]
    fn text_reads_line_height_as_a_factor_or_a_token() {
        let line_height = |attrs: &str| {
            let mut doc = parse(&minimal(&format!("<text {attrs}>Hello world</text>"))).unwrap();
            doc.section_layouts().remove(0).children.remove(0).line_height
        };
        assert_eq!(line_height(""), 1.2);
        assert_eq!(line_height(r#"line-height="1.5""#), 1.5);
        assert_eq!(line_height(r#"line-height="2""#), 2.0);
        assert_eq!(line_height(r#"line-height="xs""#), 1.0);
        assert_eq!(line_height(r#"line-height="m""#), 1.2);
        assert_eq!(line_height(r#"line-height="xxl""#), 2.0);
        for value in ["0", "-1", "tall", "1.5pt", ""] {
            let err = parse(&minimal(&format!(r#"<text line-height="{value}">Hi</text>"#))).unwrap_err();
            assert!(err.contains(&format!("invalid line-height '{value}'")) && err.contains("at line"), "{err}");
        }
    }

    #[test]
    fn a_text_and_a_text_at_read_a_line_height_token_from_the_documents_row() {
        let head = r#"<tokens><line-height xs="1" s="1.25" m="1.5" l="1.75" xl="2" xxl="2.5"/></tokens>"#;
        let xml = format!(
            r#"<lpdf version="1">{head}<document><section>
                <layer><text-at x="0pt" y="0pt" line-height="l">Hi</text-at></layer>
                <layout><text line-height="l">Hi</text></layout></section></document></lpdf>"#
        );
        let mut doc = parse(&xml).unwrap();
        assert_eq!(first_placed(&doc).node.line_height, 1.75);
        assert_eq!(doc.section_layouts().remove(0).children.remove(0).line_height, 1.75);
    }

    #[test]
    fn a_line_height_row_needs_all_six_slots_each_a_factor_above_zero() {
        let tokens = |row: &str| parse(&format!(r#"<lpdf version="1"><tokens>{row}</tokens><document><section><layout/></section></document></lpdf>"#));
        assert!(tokens(r#"<line-height xs="1" s="1.1" m="1.2" l="1.4" xl="1.6" xxl="2"/>"#).is_ok());
        let err = tokens(r#"<line-height xs="1" s="1.1" m="1.2" l="1.4" xl="1.6"/>"#).unwrap_err();
        assert!(err.contains("<line-height> missing 'xxl' attribute"), "{err}");
        for bad in ["0", "-1", "1.5pt", "tall"] {
            let err = tokens(&format!(r#"<line-height xs="{bad}" s="1.1" m="1.2" l="1.4" xl="1.6" xxl="2"/>"#)).unwrap_err();
            assert!(err.contains("<line-height> 'xs': invalid factor") && err.contains(&format!("'{bad}'")), "{err}");
        }
    }

    // ── Aliases ───────────────────────────────────────────────────────────────

    const ALIASES: &str = r#"<tokens><aliases>
        <alias name="caption" value="xs"/><alias name="body" value="m"/><alias name="heading" value="xl"/>
    </aliases></tokens>"#;

    /// A document whose tokens define the aliases `caption` (xs), `body` (m) and `heading` (xl), around `body` in a layout.
    fn with_aliases(body: &str) -> String {
        format!(
            r#"<lpdf version="1">{ALIASES}<document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
        )
    }

    #[test]
    fn a_scale_attribute_takes_an_alias_for_a_slot() {
        let mut doc = parse(&with_aliases(
            r##"<column gap="caption" padding="body" radius="heading" border="caption #000" width="caption">
                <text font-size="heading" line-height="caption">Hi</text>
                <divider thickness="body"/>
            </column>
            <grid col-width="body"/>"##,
        ))
        .unwrap();
        let layout = doc.section_layouts().remove(0);
        let column = &layout.children[0];
        assert_eq!(column.gap, 2.0, "space xs");
        assert_eq!(column.padding, [8.0; 4], "space m");
        assert_eq!(column.radius, 16.0, "radius xl");
        assert_eq!(column.border, Some((0.5, "#000000".to_string())), "border xs");
        assert_eq!(column.width_constraint, Some(120.0), "width xs");
        assert_eq!(column.children[0].font_size, 20.0, "text-size xl");
        assert_eq!(column.children[0].line_height, 1.0, "line-height xs");
        assert_eq!(column.children[1].thickness, 1.5, "border m");
        assert_eq!(layout.children[1].col_width, Some(140.0), "grid m");
    }

    #[test]
    fn an_alias_is_a_slot_of_the_row_the_document_redefines() {
        let xml = format!(
            r#"<lpdf version="1"><tokens><text-size xs="8pt" s="10pt" m="12pt" l="16pt" xl="22pt" xxl="32pt"/>
            <aliases><alias name="body" value="m"/></aliases></tokens>
            <document><section><layout><text font-size="body">Hi</text></layout></section></document></lpdf>"#
        );
        let mut doc = parse(&xml).unwrap();
        assert_eq!(doc.section_layouts().remove(0).children[0].font_size, 12.0);
    }

    #[test]
    fn an_alias_is_shared_by_every_row() {
        let mut doc = parse(&with_aliases(r#"<column gap="caption"><text font-size="caption">Hi</text></column>"#)).unwrap();
        let column = doc.section_layouts().remove(0).children.remove(0);
        assert_eq!((column.gap, column.children[0].font_size), (2.0, 7.0), "space xs, then text-size xs");
    }

    #[test]
    fn a_name_that_is_not_an_alias_is_an_error_that_lists_the_aliases() {
        let err = parse(&with_aliases(r#"<text font-size="medum">Hi</text>"#)).unwrap_err();
        assert!(
            err.contains("invalid text size: 'medum': expected a slot from xs to xxl, an alias (body, caption, heading) or a pt value")
                && err.contains("at line"),
            "{err}"
        );
        let err = parse(&with_aliases(r#"<column gap="medum"/>"#)).unwrap_err();
        assert!(err.contains("invalid space value: 'medum': expected a slot from xs to xxl, an alias (body, caption, heading)"), "{err}");
    }

    #[test]
    fn a_document_without_aliases_does_not_know_the_name() {
        let err = parse(&minimal(r#"<text font-size="caption">Hi</text>"#)).unwrap_err();
        assert!(err.contains("invalid text size: 'caption': expected a slot from xs to xxl or a pt value"), "{err}");
    }

    #[test]
    fn an_alias_that_is_not_one_says_so() {
        for (alias, says) in [
            (r#"<alias name="body" value="huge"/>"#, "alias 'body': invalid value 'huge': expected a slot from xs to xxl"),
            (r#"<alias name="body" value="caption"/><alias name="caption" value="xs"/>"#, "alias 'body': invalid value 'caption'"),
            (r#"<alias name="xs" value="m"/>"#, "alias 'xs': xs, s, m, l, xl and xxl are the slots themselves"),
            (r#"<alias name="Body" value="m"/>"#, "alias 'Body': the name must be lowercase letters"),
            (r#"<alias name="fill" value="m"/>"#, "alias 'fill': 'fill' is a keyword"),
            (r#"<alias name="body" value="m"/><alias name="body" value="l"/>"#, "alias 'body': it is defined twice"),
            (r#"<alias value="m"/>"#, "missing required attribute 'name'"),
            (r#"<alias name="body"/>"#, "missing required attribute 'value'"),
            (r#"<nickname name="body" value="m"/>"#, "<nickname>"),
        ] {
            let xml = format!(
                r#"<lpdf version="1"><tokens><aliases>{alias}</aliases></tokens><document><section><layout/></section></document></lpdf>"#
            );
            let err = parse(&xml).unwrap_err();
            assert!(err.contains(says) && err.contains("at line"), "{alias}: {err}");
        }
    }

    const ALIASED_TREE: &str = r#"{"version":1,"type":"document","attrs":{"tokens":{"aliases":ALIASES}},
        "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[
            {"type":"text","attrs":{"font-size":"caption"},"nodes":["Hi"]}]}]}]}"#;

    #[test]
    fn a_kit_tree_defines_aliases() {
        let mut doc = parse_tree(&ALIASED_TREE.replace("ALIASES", r#"{"caption":"xs","body":"m"}"#)).unwrap();
        assert_eq!(doc.section_layouts().remove(0).children[0].font_size, 7.0);
    }

    #[test]
    fn a_kit_tree_alias_that_is_not_one_says_so() {
        for (aliases, says) in [
            (r#"{"caption":"huge"}"#, "alias 'caption': invalid value 'huge'"),
            (r#"{"caption":7}"#, "alias 'caption': expected a slot from xs to xxl as text"),
            (r#"{"Caption":"xs"}"#, "alias 'Caption': the name must be lowercase letters"),
        ] {
            let err = parse_tree(&ALIASED_TREE.replace("ALIASES", aliases)).unwrap_err();
            assert!(err.contains(says), "{aliases}: {err}");
        }
    }

    // ── Height tokens ─────────────────────────────────────────────────────────

    /// A document with the image `logo` and the given `tokens`, around `body` in a layout.
    fn with_height_tokens(tokens: &str, body: &str) -> String {
        format!(
            r#"<lpdf version="1"><assets><image name="logo"/></assets><tokens>{tokens}</tokens>
            <document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"#
        )
    }

    #[test]
    fn a_box_height_is_a_slot_an_alias_a_length_fill_or_full() {
        let mut doc = parse(&with_aliases(
            r#"<column height="xs"/><frame height="m"/><row height="xxl"/><grid height="body"/>
            <column height="40pt"/><column height="fill"/><column height="full"/>
            <table cols="1fr" height="s"><tr><td height="l"/></tr></table>"#,
        ))
        .unwrap();
        let children = doc.section_layouts().remove(0).children;
        let modes: Vec<HeightMode> = children[..7].iter().map(|c| c.height_mode.clone()).collect();
        assert_eq!(
            modes,
            [
                HeightMode::Fixed(16.0),
                HeightMode::Fixed(32.0),
                HeightMode::Fixed(96.0),
                HeightMode::Fixed(32.0),
                HeightMode::Fixed(40.0),
                HeightMode::Fill,
                HeightMode::Full,
            ]
        );
        assert_eq!(children[7].height_mode, HeightMode::Fixed(24.0), "table s");
        assert_eq!(children[7].children[0].children[0].height_mode, HeightMode::Fixed(48.0), "td l");
    }

    #[test]
    fn a_height_follows_the_height_row_the_document_redefines() {
        let row = r#"<height xs="10pt" s="20pt" m="30pt" l="40pt" xl="50pt" xxl="60pt"/>"#;
        let mut doc = parse(&with_height_tokens(row, r#"<column height="m"/><column gap="m"/><column width="m"/>"#)).unwrap();
        let children = doc.section_layouts().remove(0).children;
        assert_eq!(children[0].height_mode, HeightMode::Fixed(30.0));
        assert_eq!(children[1].gap, 8.0, "the space row is not the height row");
        assert_eq!(children[2].width_constraint, Some(280.0), "nor is the width row");
    }

    #[test]
    fn a_height_row_needs_all_six_slots_each_a_length() {
        let tokens = |row: &str| parse(&with_height_tokens(row, ""));
        assert!(tokens(r#"<height xs="16pt" s="24pt" m="32pt" l="48pt" xl="64pt" xxl="96pt"/>"#).is_ok());
        let err = tokens(r#"<height xs="16pt" s="24pt" m="32pt" l="48pt" xl="64pt"/>"#).unwrap_err();
        assert!(err.contains("<height> missing 'xxl' attribute"), "{err}");
        let err = tokens(r#"<height xs="tall" s="24pt" m="32pt" l="48pt" xl="64pt" xxl="96pt"/>"#).unwrap_err();
        assert!(err.contains("<height> 'xs': invalid pt value 'tall'"), "{err}");
    }

    #[test]
    fn a_height_that_is_none_of_these_says_what_is_expected() {
        let err = parse(&minimal(r#"<column height="tall"/>"#)).unwrap_err();
        assert!(
            err.contains("invalid height value: 'tall' — expected full, fill, a slot from xs to xxl or a pt value like 28pt")
                && err.contains("at line"),
            "{err}"
        );
        let err = parse(&with_aliases(r#"<frame height="medum"/>"#)).unwrap_err();
        assert!(err.contains("expected full, fill, a slot from xs to xxl, an alias (body, caption, heading) or a pt value"), "{err}");
    }

    #[test]
    fn an_image_a_barcode_and_a_field_take_a_height_from_the_row() {
        let mut doc = parse(&with_height_tokens(
            "",
            r#"<img name="logo" height="m"/>
            <barcode type="code128" data="x" height="s"/>
            <barcode type="qr" data="x" height="l"/>
            <field type="text" name="f" height="xs"/>"#,
        ))
        .unwrap();
        let heights: Vec<Option<f32>> = doc.section_layouts().remove(0).children.iter().map(|c| c.img_height_constraint).collect();
        assert_eq!(heights, [Some(32.0), Some(24.0), Some(48.0), Some(16.0)]);
    }

    #[test]
    fn a_height_a_layer_image_takes_is_a_length() {
        let layer = |height: &str| {
            parse(&format!(
                r#"<lpdf version="1"><assets><image name="logo"/></assets><document size="a4"><section>
                <layer><img-at name="logo" x="0pt" y="0pt" height="{height}"/></layer><layout/></section></document></lpdf>"#
            ))
        };
        assert!(layer("40pt").is_ok());
        let err = layer("m").unwrap_err();
        assert!(err.contains("img height: invalid pt value 'm'"), "{err}");
    }

    #[test]
    fn a_kit_tree_defines_the_height_row_and_uses_it() {
        let tree = r#"{"version":1,"type":"document","attrs":{"tokens":{"height":{"m":"50pt"}}},
            "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[
                {"type":"column","attrs":{"height":"m"},"nodes":[]},
                {"type":"column","attrs":{"height":"xs"},"nodes":[]}]}]}]}"#;
        let children = parse_tree(tree).unwrap().section_layouts().remove(0).children;
        assert_eq!(children[0].height_mode, HeightMode::Fixed(50.0));
        assert_eq!(children[1].height_mode, HeightMode::Fixed(16.0), "a slot the tree leaves out keeps its default");

        let err = parse_tree(&tree.replace(r#""m":"50pt""#, r#""m":"tall""#)).unwrap_err();
        assert!(err.contains("invalid height token 'm': 'tall'"), "{err}");
    }

    #[test]
    fn text_reads_its_capture_attributes() {
        let text = |attrs: &str| {
            let mut doc = parse(&minimal(&format!("<text {attrs}>Hello</text>"))).unwrap();
            doc.section_layouts().remove(0).children.remove(0)
        };
        let source = text(r#"capture=" chapter ""#);
        assert_eq!((source.capture.as_ref().and_then(|c| c.key.as_deref()), source.capture_ref()), (Some("chapter"), None));
        let sink = text(r#"capture-ref="chapter""#);
        assert_eq!(sink.capture.as_ref().and_then(|c| c.key.as_deref()), None);
        assert_eq!(
            sink.capture_ref().cloned(),
            Some(CaptureRef { key: "chapter".into(), mode: CaptureMode::Last, fallback: String::new() })
        );
        let sink = text(r#"capture-ref="entry" capture-mode="first" capture-fallback="A""#);
        assert_eq!(
            sink.capture_ref().cloned(),
            Some(CaptureRef { key: "entry".into(), mode: CaptureMode::First, fallback: "A".into() })
        );
        let plain = text("");
        assert!(plain.capture.is_none());
    }

    #[test]
    fn a_text_cannot_capture_and_show_a_capture() {
        for (attrs, want) in [
            (r#"capture="a" capture-ref="a""#, "cannot have both"),
            (r#"capture-mode="first""#, "belong to a text with 'capture-ref'"),
            (r#"capture-fallback="x""#, "belong to a text with 'capture-ref'"),
            (r#"capture-ref="a" capture-mode="middle""#, "invalid capture-mode 'middle'"),
            (r#"capture="  ""#, "'capture' needs a name"),
            (r#"capture-ref="""#, "'capture-ref' needs a name"),
        ] {
            let err = parse(&minimal(&format!("<text {attrs}>Hi</text>"))).unwrap_err();
            assert!(err.contains(want) && err.contains("at line"), "{attrs}: {err}");
        }
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
                <column font-size="14pt"><text>Hello</text></column>
            </layout></section></document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let pages = doc.section_layouts();
        let column = &pages[0].children[0];
        let text  = &column.children[0];
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
    fn a_shape_is_a_box_of_the_size_its_type_names() {
        let xml = minimal(
            r##"<shape type="rect" width="40pt" height="20pt" radius="4pt" fill="#112233" stroke="#445566" stroke-width="2pt" stroke-dash="3 2" opacity="0.5"/>
            <shape type="circle" r="10pt"/>
            <shape type="ellipse" width="30pt" height="12pt"/>"##,
        );
        let mut doc = parse(&xml).unwrap();
        let nodes = &doc.section_layouts()[0].children;
        assert!(nodes.iter().all(|n| n.kind == NodeKind::Shape));

        let rect = &nodes[0];
        assert_eq!((rect.width_constraint, rect.img_height_constraint), (Some(40.0), Some(20.0)));
        assert_eq!((rect.radius, rect.opacity), (4.0, 0.5));
        let shape = rect.shape().unwrap();
        assert_eq!(shape.kind, ShapeKind::Rect);
        assert_eq!((shape.fill.as_deref(), shape.stroke.as_deref()), (Some("#112233"), Some("#445566")));
        assert_eq!((shape.stroke_width, shape.stroke_dash.as_deref()), (Some(2.0), Some("3 2")));

        let circle = &nodes[1];
        assert_eq!(circle.shape().unwrap().kind, ShapeKind::Circle);
        assert_eq!((circle.width_constraint, circle.img_height_constraint), (Some(20.0), Some(20.0)), "the square of side 2r");

        let ellipse = &nodes[2];
        assert_eq!(ellipse.shape().unwrap().kind, ShapeKind::Ellipse);
        assert_eq!((ellipse.width_constraint, ellipse.img_height_constraint), (Some(30.0), Some(12.0)));
    }

    #[test]
    fn a_shape_takes_its_size_and_radius_from_the_tokens() {
        let xml = r#"<lpdf version="1"><document size="a4"><section><layout>
            <shape type="rect" width="m" height="m" radius="s"/>
        </layout></section></document></lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let shape = &doc.section_layouts()[0].children[0];
        let tokens = Tokens::default();
        assert_eq!(shape.width_constraint, Some(tokens.resolve_width("m").unwrap()));
        assert_eq!(shape.img_height_constraint, Some(tokens.resolve_height("m").unwrap()));
        assert_eq!(shape.radius, tokens.resolve_radius("s").unwrap());
    }

    #[test]
    fn a_shape_that_lacks_what_its_type_needs_says_what() {
        for (shape, says) in [
            (r#"<shape width="10pt" height="10pt"/>"#, "missing required attribute 'type'"),
            (r#"<shape type="star"/>"#, "unknown type 'star'"),
            (r#"<shape type="circle"/>"#, "a circle needs 'r'"),
            (r#"<shape type="circle" r="big"/>"#, "invalid r 'big'"),
            (r#"<shape type="circle" r="5pt" width="10pt"/>"#, "a circle takes 'r', not 'width' or 'height'"),
            (r#"<shape type="circle" r="5pt" radius="2pt"/>"#, "a circle is round already"),
            (r#"<shape type="rect" height="10pt"/>"#, "a rect needs both 'width' and 'height'"),
            (r#"<shape type="rect" width="10pt"/>"#, "a rect needs both 'width' and 'height'"),
            (r#"<shape type="ellipse" r="2pt"/>"#, "'r' is the radius of a circle"),
            (r#"<shape type="ellipse" width="10pt" height="5pt" radius="2pt"/>"#, "an ellipse is round already"),
            (r#"<shape type="rect" width="fill" height="10pt"/>"#, "not fill"),
        ] {
            let err = parse(&minimal(shape)).unwrap_err();
            assert!(err.starts_with("<shape> at line") && err.contains(says), "{shape}: {err}");
        }
    }

    #[test]
    fn a_shape_in_a_kit_tree_is_read_as_in_xml() {
        let tree = |shape: &str| format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layout","attrs":{{}},"nodes":[{shape}]}}]}}]}}"#
        );
        let mut doc = parse_tree(&tree(r##"{"type":"shape","attrs":{"type":"circle","r":"10pt","fill":"#ff0000","opacity":"0.5"}}"##)).unwrap();
        let shape = &doc.section_layouts()[0].children[0];
        assert_eq!(shape.kind, NodeKind::Shape);
        assert_eq!((shape.width_constraint, shape.img_height_constraint, shape.opacity), (Some(20.0), Some(20.0), 0.5));
        assert_eq!(shape.shape().unwrap().fill.as_deref(), Some("#ff0000"));

        let err = parse_tree(&tree(r#"{"type":"shape","attrs":{"type":"circle"}}"#)).unwrap_err();
        assert!(err.contains("shape: a circle needs 'r'"), "{err}");
    }

    #[test]
    fn an_fn_in_a_text_is_an_anchor_run_that_holds_its_note() {
        let mut doc = parse(&minimal(
            r#"<text>The model<fn>First <span bold="true">proposed</span> in 1960.</fn> describes <fn>Another.</fn>forces.</text>"#,
        )).unwrap();
        let runs = &doc.section_layouts()[0].children[0].text_runs;
        let texts: Vec<_> = runs.iter().map(|r| (r.text.as_str(), r.footnote.is_some(), r.leading_space)).collect();
        assert_eq!(texts, [("The model", false, false), ("", true, false), ("describes", false, true), ("", true, true), ("forces.", false, false)]);
        let note = runs[1].footnote.as_deref().unwrap();
        let words: Vec<_> = note.runs.iter().map(|r| (r.text.as_str(), r.font.as_deref())).collect();
        assert_eq!(words, [("First", None), ("proposed", Some("Helvetica-Bold")), ("in 1960.", None)], "bold in a note is resolved");
        assert_eq!(note.number, 1);
    }

    #[test]
    fn an_fn_with_no_words_or_inside_another_is_an_error() {
        for (body, says) in [
            (r#"<text>A<fn/></text>"#, "a footnote needs words"),
            (r#"<text>A<fn>   </fn></text>"#, "a footnote needs words"),
            (r#"<text>A<fn>B<fn>C</fn></fn></text>"#, "cannot hold another footnote"),
        ] {
            let err = parse(&minimal(body)).unwrap_err();
            assert!(err.starts_with("<fn>") && err.contains(says), "{body}: {err}");
        }
    }

    #[test]
    fn an_fn_is_an_error_in_a_table_a_region_and_a_text_at() {
        let table = r#"<table cols="1fr"><tr><td><text>A<fn>B</fn></text></td></tr></table>"#;
        let err = parse(&minimal(table)).unwrap_err();
        assert!(err.starts_with("<table>") && err.contains("<fn> is not supported in a table"), "{err}");

        let region = r#"<region pin="bottom"><text>A<fn>B</fn></text></region>"#;
        let err = parse(&minimal(region)).unwrap_err();
        assert!(err.starts_with("<region>") && err.contains("<fn> is not supported in a region"), "{err}");

        let layer = r#"<lpdf version="1"><document size="a4"><section><layer><text-at x="10pt" y="10pt">A<fn>B</fn></text-at></layer><layout/></section></document></lpdf>"#;
        let err = parse(layer).unwrap_err();
        assert!(err.contains("<fn> is not supported in a <text-at>"), "{err}");
    }

    #[test]
    fn an_fn_in_a_kit_tree_is_read_as_in_xml() {
        let tree = |text: &str| format!(
            r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
                {{"type":"layout","attrs":{{}},"nodes":[{text}]}}]}}]}}"#
        );
        let mut doc = parse_tree(&tree(
            r#"{"type":"text","attrs":{},"nodes":["The model",{"type":"fn","attrs":{},"nodes":["First ",{"type":"span","attrs":{"bold":"true"},"nodes":["proposed"]}]}," describes."]}"#,
        )).unwrap();
        let runs = &doc.section_layouts()[0].children[0].text_runs;
        let texts: Vec<_> = runs.iter().map(|r| (r.text.as_str(), r.footnote.is_some())).collect();
        assert_eq!(texts, [("The model", false), ("", true), ("describes.", false)]);
        assert!(runs[2].leading_space);
        let words: Vec<_> = runs[1].footnote.as_deref().unwrap().runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(words, ["First", "proposed"]);

        for (text, says) in [
            (r#"{"type":"text","attrs":{},"nodes":["A",{"type":"fn","attrs":{},"nodes":[]}]}"#, "a footnote needs words"),
            (r#"{"type":"text","attrs":{},"nodes":["A",{"type":"fn","attrs":{},"nodes":["B",{"type":"fn","attrs":{},"nodes":["C"]}]}]}"#, "cannot hold another footnote"),
            (r#"{"type":"table","attrs":{"cols":"1fr"},"nodes":[{"type":"tr","attrs":{},"nodes":[{"type":"td","attrs":{},"nodes":[{"type":"text","attrs":{},"nodes":["A",{"type":"fn","attrs":{},"nodes":["B"]}]}]}]}]}"#, "not supported in a table"),
        ] {
            let err = parse_tree(&tree(text)).unwrap_err();
            assert!(err.contains(says), "{text}: {err}");
        }
    }

    #[test]
    fn footnote_counter_is_read_on_the_document_and_the_section_and_a_section_overrides() {
        let xml = r#"<lpdf version="1"><document size="a4" footnote-counter="document">
            <section><layout/></section>
            <section footnote-counter="page"><layout/></section>
            <section footnote-counter="section"><layout/></section>
        </document></lpdf>"#;
        let counters: Vec<_> = parse(xml).unwrap().section_layouts().iter().map(|s| s.footnote_counter).collect();
        assert_eq!(counters, [FootnoteCounter::Document, FootnoteCounter::Page, FootnoteCounter::Section]);

        let default: Vec<_> = parse(&minimal("")).unwrap().section_layouts().iter().map(|s| s.footnote_counter).collect();
        assert_eq!(default, [FootnoteCounter::Section], "a section counts its own notes unless told otherwise");

        let err = parse(&xml.replace(r#"footnote-counter="page""#, r#"footnote-counter="book""#)).unwrap_err();
        assert!(err.starts_with("<section>") && err.contains("invalid footnote-counter 'book'"), "{err}");
        let err = parse(&xml.replace(r#"footnote-counter="document""#, r#"footnote-counter="book""#)).unwrap_err();
        assert!(err.starts_with("<document>") && err.contains("invalid footnote-counter 'book'"), "{err}");
    }

    #[test]
    fn footnote_counter_is_read_in_a_kit_tree() {
        let tree = r#"{"version":1,"type":"document","attrs":{"footnote-counter":"document"},"nodes":[
            {"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[
                {"type":"text","attrs":{},"nodes":["A",{"type":"fn","attrs":{},"nodes":["B"]}]}]}]},
            {"type":"section","attrs":{"footnote-counter":"page"},"nodes":[{"type":"layout","attrs":{},"nodes":[]}]},
            {"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[
                {"type":"text","attrs":{},"nodes":["C",{"type":"fn","attrs":{},"nodes":["D"]}]}]}]}]}"#;
        let layouts = parse_tree(tree).unwrap().section_layouts();
        let seen: Vec<_> = layouts.iter().map(|s| (s.footnote_counter, s.footnote_start)).collect();
        assert_eq!(seen, [(FootnoteCounter::Document, 0), (FootnoteCounter::Page, 1), (FootnoteCounter::Document, 1)]);

        let err = parse_tree(&tree.replace(r#""footnote-counter":"page""#, r#""footnote-counter":"book""#)).unwrap_err();
        assert!(err.contains("section: invalid footnote-counter 'book'"), "{err}");
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
            <document size="a4">
                <section>
                    <layout margin="0pt"><column gap="m"><text>Hello</text></column></layout>
                </section>
            </document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.sections.len(), 1);
        let sc = &doc.sections[0];
        assert_eq!(sc.children.len(), 1);
        assert!(matches!(sc.children[0], SectionChild::Layout { .. }));
    }

    /// The layer a section child is, or a panic.
    fn as_layer(child: &SectionChild) -> &canvas::CanvasLayer {
        match child {
            SectionChild::Layer(layer) => layer,
            SectionChild::Layout { .. } => panic!("expected a layer"),
        }
    }

    #[test]
    fn parse_section_layers_only() {
        let xml = r##"<lpdf version="1">
            <document size="a4">
                <section>
                    <layer><rect width="100pt" height="50pt" fill="#ff0000"/></layer>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.sections.len(), 1);
        assert_eq!(doc.sections[0].children.len(), 1);
        assert_eq!(as_layer(&doc.sections[0].children[0]).children.len(), 1);
    }

    #[test]
    fn parse_section_layout_plus_layer() {
        let xml = r##"<lpdf version="1">
            <document size="a4">
                <section>
                    <layout margin="0pt"><text>Body</text></layout>
                    <layer page="each"><rect width="100pt" height="5pt" fill="#000000"/></layer>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.sections[0].children.len(), 2);
    }

    #[test]
    fn layers_before_the_layout_go_under_it_and_layers_after_it_over_it() {
        let xml = r##"<lpdf version="1">
            <document size="a4">
                <section>
                    <layer page="first"><rect width="10pt" height="10pt" fill="#000"/></layer>
                    <layer page="last"><rect width="10pt" height="10pt" fill="#000"/></layer>
                    <layout margin="0pt"><text>Body</text></layout>
                    <layer page="odd"><rect width="10pt" height="10pt" fill="#000"/></layer>
                    <layer page="even"><rect width="10pt" height="10pt" fill="#000"/></layer>
                </section>
            </document>
        </lpdf>"##;
        let mut doc = parse(xml).unwrap();
        let layouts = doc.section_layouts();
        let pages = |layers: &[canvas::CanvasLayer]| layers.iter().map(|l| l.page.clone()).collect::<Vec<_>>();
        assert_eq!(pages(&layouts[0].underlays), vec![Some(PageScope::First), Some(PageScope::Last)]);
        assert_eq!(pages(&layouts[0].overlays), vec![Some(PageScope::Odd), Some(PageScope::Even)]);
    }

    #[test]
    fn canvas_is_an_error_that_says_where_its_layers_go() {
        let err = parse(&document(r#"<section><canvas><layer/></canvas></section>"#)).unwrap_err();
        assert!(err.contains("<canvas>") && err.contains("at line"), "{err}");
        assert!(err.contains("<layer> elements directly in the <section>"), "{err}");
    }

    #[test]
    fn second_layout_is_an_error() {
        let err = parse(&document(
            r#"<section><layout><text>a</text></layout><layer/><layout><text>b</text></layout></section>"#,
        )).unwrap_err();
        assert!(err.contains("<layout>") && err.contains("at line"), "{err}");
        assert!(err.contains("at most one <layout>"), "{err}");
    }

    #[test]
    fn tree_layers_parse_as_the_xml_does() {
        let json = r##"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{},"nodes":[
                {"type":"layer","attrs":{"page":"first"},"nodes":[{"type":"rect","attrs":{"width":"10pt","height":"10pt"}}]},
                {"type":"layout","nodes":[{"type":"text","attrs":{},"nodes":["Body"]}]},
                {"type":"layer","attrs":{"page":"last"},"nodes":[]}]}]}"##;
        let mut doc = parse_tree(json).unwrap();
        let layouts = doc.section_layouts();
        assert_eq!(layouts[0].underlays.len(), 1);
        assert_eq!(layouts[0].underlays[0].page, Some(PageScope::First));
        assert_eq!(layouts[0].underlays[0].children.len(), 1);
        assert_eq!(layouts[0].overlays.len(), 1);
        assert_eq!(layouts[0].overlays[0].page, Some(PageScope::Last));
    }

    #[test]
    fn tree_canvas_and_second_layout_are_errors() {
        let canvas = r#"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{},"nodes":[{"type":"canvas","nodes":[{"type":"layer","attrs":{},"nodes":[]}]}]}]}"#;
        let err = parse_tree(canvas).unwrap_err();
        assert!(err.contains("'canvas'") && err.contains("<layer> elements directly in the <section>"), "{err}");

        let layouts = r#"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{},"nodes":[
                {"type":"layout","nodes":[{"type":"text","attrs":{},"nodes":["a"]}]},
                {"type":"layout","nodes":[{"type":"text","attrs":{},"nodes":["b"]}]}]}]}"#;
        let err = parse_tree(layouts).unwrap_err();
        assert!(err.contains("at most one <layout>"), "{err}");
    }

    #[test]
    fn a_layout_without_a_margin_keeps_36pt_from_the_page_edges() {
        let mut doc = parse(&document("<section><layout><text>Body</text></layout></section>")).unwrap();
        assert_eq!(doc.section_layouts()[0].margin, [36.0; 4]);

        let json = r#"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{},"nodes":[{"type":"layout","nodes":[{"type":"text","attrs":{},"nodes":["Body"]}]}]}]}"#;
        assert_eq!(parse_tree(json).unwrap().section_layouts()[0].margin, [36.0; 4]);
    }

    #[test]
    fn a_layout_reads_its_margin_in_xml_and_in_a_tree() {
        let mut doc = parse(&document(r#"<section><layout margin="10pt 20pt"><text>Body</text></layout></section>"#)).unwrap();
        assert_eq!(doc.section_layouts()[0].margin, [10.0, 20.0, 10.0, 20.0]);

        let json = r#"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{},"nodes":[
                {"type":"layout","attrs":{"margin":"10pt 20pt"},"nodes":[{"type":"text","attrs":{},"nodes":["Body"]}]}]}]}"#;
        assert_eq!(parse_tree(json).unwrap().section_layouts()[0].margin, [10.0, 20.0, 10.0, 20.0]);
    }

    #[test]
    fn a_section_of_layers_alone_has_no_margin() {
        let mut doc = parse(&document(r##"<section><layer><rect width="10pt" height="10pt" fill="#000"/></layer></section>"##)).unwrap();
        assert_eq!(doc.section_layouts()[0].margin, [0.0; 4]);
    }

    #[test]
    fn margin_on_a_document_or_a_section_is_an_error_that_names_the_layout() {
        let on_document = r#"<lpdf version="1"><document margin="28pt"><section><layout/></section></document></lpdf>"#;
        let on_section = document(r#"<section margin="28pt"><layout/></section>"#);
        for (element, xml) in [("<document>", on_document.to_string()), ("<section>", on_section)] {
            let err = parse(&xml).unwrap_err();
            assert!(err.starts_with(element) && err.contains("at line"), "{err}");
            assert!(err.contains("margin moved to <layout>"), "{err}");
        }
    }

    #[test]
    fn tree_margin_on_a_document_or_a_section_is_an_error_that_names_the_layout() {
        let on_document = r#"{"version":1,"type":"document","attrs":{"margin":"28pt"},"nodes":[
            {"type":"section","attrs":{},"nodes":[{"type":"layout","nodes":[]}]}]}"#;
        let on_section = r#"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{"margin":"28pt"},"nodes":[{"type":"layout","nodes":[]}]}]}"#;
        for (element, json) in [("document", on_document), ("section", on_section)] {
            let err = parse_tree(json).unwrap_err();
            assert!(err.starts_with(element) && err.contains("margin moved to <layout>"), "{err}");
        }
    }

    #[test]
    fn parse_multi_section() {
        let xml = r##"<lpdf version="1">
            <document size="a4">
                <section title="Cover"><layer><rect width="100pt" height="100pt" fill="#000"/></layer></section>
                <section title="Body"><layout margin="0pt"><text>Hello</text></layout></section>
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
            <document size="a4">
                <section>
                    <layout margin="0pt">
                        <region pin="top" page="each"><text>Header</text></region>
                        <text>Body</text>
                    </layout>
                </section>
            </document>
        </lpdf>"#;
        let doc = parse(xml).unwrap();
        let layout = match &doc.sections[0].children[0] {
            SectionChild::Layout { children, .. } => children,
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
            <document size="a4">
                <section>
                    <layout margin="28pt"><text>Hello</text></layout>
                </section>
            </document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let pages = doc.section_layouts();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].children.len(), 1);
        assert_eq!(pages[0].children[0].kind, NodeKind::Text);
    }

    /// The first words of what a layout child holds: its own text, or the text of the region it stands for.
    fn label(node: &Node) -> String {
        let text = if node.kind == NodeKind::Text { node } else { &node.children[0] };
        text.text_runs[0].text.clone()
    }

    #[test]
    fn regions_go_by_pin_wherever_they_are_written() {
        let xml = r#"<lpdf version="1">
            <document size="a4">
                <section>
                    <layout>
                        <region pin="bottom"><text>Foot</text></region>
                        <text>Body</text>
                        <region pin="top" page="first"><text>Head</text></region>
                        <region pin="bottom" page="last"><text>Foot 2</text></region>
                        <region pin="top"><text>Head 2</text></region>
                    </layout>
                </section>
            </document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let layout = doc.section_layouts();
        let order: Vec<String> = layout[0].children.iter().map(label).collect();
        assert_eq!(order, ["Head", "Head 2", "Body", "Foot", "Foot 2"]);
    }

    #[test]
    fn regions_written_at_the_edges_keep_their_order() {
        let xml = r#"<lpdf version="1">
            <document size="a4">
                <section>
                    <layout>
                        <region pin="top"><text>Head</text></region>
                        <text>Body</text>
                        <region pin="bottom"><text>Foot</text></region>
                    </layout>
                </section>
            </document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let layout = doc.section_layouts();
        let order: Vec<String> = layout[0].children.iter().map(label).collect();
        assert_eq!(order, ["Head", "Body", "Foot"]);
    }

    #[test]
    fn a_left_region_stays_where_it_is_written() {
        let xml = r#"<lpdf version="1">
            <document size="a4">
                <section>
                    <layout>
                        <text>A</text>
                        <region pin="left" w="40pt"><text>Side</text></region>
                        <text>B</text>
                        <region pin="top"><text>Head</text></region>
                    </layout>
                </section>
            </document>
        </lpdf>"#;
        let mut doc = parse(xml).unwrap();
        let layout = doc.section_layouts();
        let order: Vec<String> = layout[0].children.iter().map(label).collect();
        assert_eq!(order, ["Head", "A", "Side", "B"]);
    }

    // §5.2.8 layer page scope
    #[test]
    fn layer_page_scope() {
        let xml = r##"<lpdf version="1">
            <document size="a4">
                <section>
                    <layer page="odd"><rect width="10pt" height="10pt" fill="#000"/></layer>
                    <layer page="2-4"><rect width="10pt" height="10pt" fill="#000"/></layer>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        assert_eq!(as_layer(&doc.sections[0].children[0]).page, Some(PageScope::Odd));
        assert_eq!(
            as_layer(&doc.sections[0].children[1]).page,
            Some(PageScope::Pages(vec![PageRange { start: 2, end: Some(4) }])),
        );
    }

    // §5.2.9 layer primitives
    #[test]
    fn layer_rect_absolute_pos() {
        use crate::canvas::CanvasPrimitive;
        let xml = r##"<lpdf version="1">
            <document size="a4">
                <section>
                    <layer>
                        <rect x="10pt" y="20pt" width="100pt" height="50pt" fill="#ff0000" radius="4pt"/>
                    </layer>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        if let CanvasPrimitive::Rect(ref r) = as_layer(&doc.sections[0].children[0]).children[0] {
            assert!((r.x - 10.0).abs() < 0.01);
            assert!((r.y - 20.0).abs() < 0.01);
            assert!((r.w - 100.0).abs() < 0.01);
            assert_eq!(r.fill.as_deref(), Some("#ff0000"));
            assert!((r.radius.unwrap() - 4.0).abs() < 0.01);
        } else { panic!("expected rect"); }
    }

    #[test]
    fn layer_rect_anchored_pos() {
        use crate::canvas::CanvasPrimitive;
        let xml = r##"<lpdf version="1">
            <document size="a4">
                <section>
                    <layer>
                        <rect anchor="center" x="10pt" y="-5pt" width="50pt" height="20pt" fill="#000"/>
                    </layer>
                </section>
            </document>
        </lpdf>"##;
        let doc = parse(xml).unwrap();
        if let CanvasPrimitive::Rect(ref r) = as_layer(&doc.sections[0].children[0]).children[0] {
            // A4 center = (297.64, 420.945), dx=10, dy=-5 → anchor point (307.64, 415.945)
            // rect is 50×20pt, so top-left = anchor - (w/2, h/2) = (282.64, 405.945)
            assert!((r.x - 282.64).abs() < 0.5);
            assert!((r.y - 405.94).abs() < 0.5);
        } else { panic!("expected rect"); }
    }

    #[test]
    fn a_circle_is_placed_by_its_box() {
        use crate::canvas::CanvasPrimitive;
        let xml = r##"<lpdf version="1"><document size="a4"><section><layer>
            <circle x="10pt" y="20pt" r="5pt" fill="#000"/>
            <circle r="5pt" fill="#000"/>
        </layer></section></document></lpdf>"##;
        let doc = parse(xml).unwrap();
        let children = &as_layer(&doc.sections[0].children[0]).children;
        let centres: Vec<(f32, f32)> = children.iter().map(|c| match c {
            CanvasPrimitive::Circle(c) => (c.cx, c.cy),
            _ => panic!("expected a circle"),
        }).collect();
        // The box is 10pt square with its top-left corner at x, y; the centre is r further in.
        assert_eq!(centres, vec![(15.0, 25.0), (5.0, 5.0)]);
    }

    #[test]
    fn an_ellipse_is_placed_by_its_box() {
        use crate::canvas::CanvasPrimitive;
        let xml = r##"<lpdf version="1"><document size="a4"><section><layer>
            <ellipse x="10pt" y="20pt" rx="8pt" ry="3pt" fill="#000"/>
        </layer></section></document></lpdf>"##;
        let doc = parse(xml).unwrap();
        if let CanvasPrimitive::Ellipse(ref e) = as_layer(&doc.sections[0].children[0]).children[0] {
            assert_eq!((e.cx, e.cy, e.rx, e.ry), (18.0, 23.0, 8.0, 3.0));
        } else { panic!("expected an ellipse"); }
    }

    #[test]
    fn an_anchored_circle_has_its_box_anchored_not_its_centre() {
        use crate::canvas::CanvasPrimitive;
        // A4 is 595.28 x 841.89pt. Each box is 20pt square, its anchor point offset by (-5, 7) from the page's.
        let cases = [
            ("top-left",     (-5.0 + 10.0,            7.0 + 10.0)),
            ("top-right",    (595.28 - 5.0 - 10.0,    7.0 + 10.0)),
            ("bottom-right", (595.28 - 5.0 - 10.0,    841.89 + 7.0 - 10.0)),
            ("center",       (595.28 / 2.0 - 5.0,     841.89 / 2.0 + 7.0)),
        ];
        for (anchor, (cx, cy)) in cases {
            let xml = format!(r##"<lpdf version="1"><document size="a4"><section><layer>
                <circle anchor="{anchor}" x="-5pt" y="7pt" r="10pt" fill="#000"/>
            </layer></section></document></lpdf>"##);
            let doc = parse(&xml).unwrap();
            if let CanvasPrimitive::Circle(ref c) = as_layer(&doc.sections[0].children[0]).children[0] {
                assert!((c.cx - cx).abs() < 0.01 && (c.cy - cy).abs() < 0.01, "{anchor}: ({}, {})", c.cx, c.cy);
            } else { panic!("expected a circle"); }
        }
    }

    #[test]
    fn a_layer_element_with_an_attribute_it_lost_says_what_replaces_it() {
        for (body, attr, replacement) in [
            (r#"<rect w="10pt" height="10pt"/>"#, "w", "'width'"),
            (r#"<rect width="10pt" h="10pt"/>"#, "h", "'height'"),
            (r#"<circle cx="10pt" cy="10pt" r="5pt"/>"#, "cx", "'x' and 'y'"),
            (r#"<circle cy="10pt" r="5pt"/>"#, "cy", "'x' and 'y'"),
            (r#"<ellipse cx="10pt" rx="5pt" ry="3pt"/>"#, "cx", "'x' and 'y'"),
            (r#"<text-at w="100pt">Hi</text-at>"#, "w", "'width'"),
        ] {
            let tag = &body[1..body[1..].find([' ', '>']).unwrap() + 1];
            let xml = format!(r##"<lpdf version="1"><document size="a4"><section><layer>{body}</layer></section></document></lpdf>"##);
            let err = parse(&xml).unwrap_err();
            assert!(err.contains(&format!("<{tag}>")) && err.contains("at line"), "{err}");
            assert!(err.contains(&format!("'{attr}'")) && err.contains(replacement), "{err}");
        }
    }

    /// The `PlacedNode` that is the first child of the first layer of the first section.
    fn first_placed(doc: &Document) -> &canvas::PlacedNode {
        match &as_layer(&doc.sections[0].children[0]).children[0] {
            canvas::CanvasPrimitive::Placed(placed) => placed,
            _ => panic!("expected a text-at, img-at or barcode-at"),
        }
    }

    fn layer_document(head: &str, layer: &str) -> String {
        format!(r##"<lpdf version="1">{head}<document size="a4"><section><layer>{layer}</layer></section></document></lpdf>"##)
    }

    #[test]
    fn layer_text_at_is_the_text_of_the_layout_with_a_position() {
        let xml = layer_document("", r#"<text-at anchor="center" font-size="12pt">Hello <span font="Helvetica-Bold">world</span></text-at>"#);
        let doc = parse(&xml).unwrap();
        let placed = first_placed(&doc);
        assert_eq!(placed.node.kind, NodeKind::Text);
        // A4 center = (297.64, 420.945)
        assert!((placed.x - 297.64).abs() < 0.5 && (placed.y - 420.94).abs() < 0.5);
        assert_eq!(placed.anchor, canvas::Anchor::Center);
        assert_eq!(placed.node.font_size, 12.0);
        assert_eq!(placed.node.text_runs.len(), 2);
        assert_eq!(placed.node.text_runs[1].font.as_deref(), Some("Helvetica-Bold"));
    }

    #[test]
    fn layer_text_takes_the_defaults_of_the_text_of_the_layout() {
        let layout_xml = r##"<lpdf version="1">
            <assets><font name="body" core="Times-Roman"/></assets>
            <tokens><colors><color name="text" value="#223344"/></colors></tokens>
            <document size="a4" font="body"><section>
                <layer><text-at x="0pt" y="0pt">Layer</text-at></layer>
                <layout><text>Layout</text></layout>
            </section></document></lpdf>"##;
        let mut doc = parse(layout_xml).unwrap();
        let placed = first_placed(&doc).node.clone();
        let layout = doc.section_layouts().remove(0).children.remove(0);
        for node in [&placed, &layout] {
            assert_eq!(node.font, "Times-Roman");
            assert_eq!(node.font_size, 11.0);
            assert_eq!(node.text_color.as_deref(), Some("#223344"));
        }
        // Without a document font or a text colour of its own, a layer's text is Helvetica, 11pt and #1a1a1a.
        let doc = parse(&layer_document("", r#"<text-at x="0pt" y="0pt">Hi</text-at>"#)).unwrap();
        let node = &first_placed(&doc).node;
        assert_eq!((node.font.as_str(), node.font_size, node.text_color.as_deref()), ("Helvetica", 11.0, Some("#1a1a1a")));
    }

    // ── Color and opacity of a box, font and color of a layer ────────────────

    /// The colors of the texts under `nodes`, in document order, at any depth.
    fn text_colors(nodes: &[Node]) -> Vec<String> {
        nodes.iter().flat_map(|n| {
            let own = (n.kind == NodeKind::Text).then(|| n.text_color.clone().expect("a text has a color"));
            own.into_iter().chain(text_colors(&n.children))
        }).collect()
    }

    const NESTED_COLORS: &str = r##"
        <column color="#111111">
            <text>a</text>
            <row color="#222222"><text>b</text><text color="#333333">c</text></row>
            <grid cols="2"><text>d</text></grid>
            <frame color="primary"><text>e</text></frame>
        </column>
        <table cols="1fr"><tr><td color="#444444"><text>f</text></td></tr></table>
        <text>g</text>"##;

    const NESTED_COLORS_EXPECTED: [&str; 7] =
        ["#111111", "#222222", "#333333", "#111111", "#1763cf", "#444444", "#1a1a1a"];

    #[test]
    fn a_text_takes_the_color_of_the_nearest_box() {
        let mut doc = parse(&minimal(NESTED_COLORS)).unwrap();
        assert_eq!(text_colors(&doc.section_layouts()[0].children), NESTED_COLORS_EXPECTED);
    }

    #[test]
    fn a_text_in_a_kit_tree_takes_the_color_of_the_nearest_box() {
        let text = |attrs: &str, word: &str| format!(r#"{{"type":"text","attrs":{{{attrs}}},"nodes":["{word}"]}}"#);
        let boxed = |tag: &str, attrs: &str, inner: &[String]| {
            format!(r#"{{"type":"{tag}","attrs":{{{attrs}}},"nodes":[{}]}}"#, inner.join(","))
        };
        let nodes = [
            boxed("column", r##""color":"#111111""##, &[
                text("", "a"),
                boxed("row", r##""color":"#222222""##, &[text("", "b"), text(r##""color":"#333333""##, "c")]),
                boxed("grid", r#""cols":"2""#, &[text("", "d")]),
                boxed("frame", r#""color":"primary""#, &[text("", "e")]),
            ]),
            boxed("table", r#""cols":"1fr""#, &[boxed("tr", "", &[boxed("td", r##""color":"#444444""##, &[text("", "f")])])]),
            text("", "g"),
        ];
        let json = format!(r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
            {{"type":"layout","attrs":{{}},"nodes":[{}]}}]}}]}}"#, nodes.join(","));
        let mut doc = parse_tree(&json).unwrap();
        assert_eq!(text_colors(&doc.section_layouts()[0].children), NESTED_COLORS_EXPECTED);
    }

    #[test]
    fn the_color_of_a_box_reaches_a_text_through_a_table_and_a_link() {
        let mut doc = parse(&minimal(
            r##"<column color="#555555"><link href="https://a.example"><text>a</text></link>
                <table cols="1fr"><thead><td><text>b</text></td></thead><tr><td><text>c</text></td></tr></table></column>"##,
        )).unwrap();
        assert_eq!(text_colors(&doc.section_layouts()[0].children), ["#555555"; 3]);
    }

    #[test]
    fn a_text_outside_every_box_takes_the_text_color_of_the_tokens() {
        let xml = r##"<lpdf version="1"><tokens><colors><color name="text" value="#223344"/></colors></tokens>
            <document size="a4"><section><layout><text>a</text><column><text>b</text></column></layout></section></document></lpdf>"##;
        let mut doc = parse(xml).unwrap();
        assert_eq!(text_colors(&doc.section_layouts()[0].children), ["#223344", "#223344"]);
    }

    #[test]
    fn the_color_of_a_box_is_not_the_color_of_a_divider_or_a_barcode() {
        let mut doc = parse(&minimal(r##"<column color="#111111"><divider/><barcode type="qr" data="x" width="50pt"/></column>"##)).unwrap();
        let column = &doc.section_layouts()[0].children[0];
        assert_eq!(column.children[0].color.as_deref(), Some("#737373"));
        assert_eq!(column.children[1].barcode_color, None);
        assert_eq!(column.children.iter().filter_map(|n| n.text_color.as_deref()).count(), 0, "only a text carries a color");
    }

    #[test]
    fn a_color_that_is_not_one_says_so() {
        for body in [r#"<column color="nope"><text>a</text></column>"#, r##"<column color="#12"><text>a</text></column>"##] {
            let err = parse(&minimal(body)).unwrap_err();
            assert!(err.contains("<column>") && err.contains("color"), "{err}");
        }
    }

    #[test]
    fn a_box_reads_its_opacity() {
        let mut doc = parse(&minimal(
            r#"<column opacity="0.5"><row opacity="0"/><grid opacity="1"/><frame/><table cols="1fr"><tr><td opacity=".25"/></tr></table></column>"#,
        )).unwrap();
        let column = &doc.section_layouts()[0].children[0];
        let opacities: Vec<f32> = column.children.iter().map(|n| n.opacity).collect();
        assert_eq!((column.opacity, opacities[..3].to_vec()), (0.5, vec![0.0, 1.0, 1.0]));
        assert_eq!(column.children[3].children[0].children[0].opacity, 0.25);
    }

    #[test]
    fn a_box_in_a_kit_tree_reads_its_opacity() {
        let json = r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[
            {"type":"layout","attrs":{},"nodes":[{"type":"column","attrs":{"opacity":"0.5"},"nodes":[]}]}]}]}"#;
        let mut doc = parse_tree(json).unwrap();
        assert_eq!(doc.section_layouts()[0].children[0].opacity, 0.5);
    }

    #[test]
    fn an_opacity_that_is_not_a_number_from_0_to_1_says_so() {
        for opacity in ["1.5", "-0.1", "half", "", "NaN"] {
            let err = parse(&minimal(&format!(r#"<column opacity="{opacity}"/>"#))).unwrap_err();
            assert!(err.contains("<column>") && err.contains(&format!("invalid opacity '{opacity}'")), "{err}");
        }
    }

    fn shadow(dx: f32, dy: f32, color: &str, opacity: f32) -> Option<Shadow> {
        Some(Shadow { dx, dy, color: color.to_string(), opacity })
    }

    #[test]
    fn a_box_reads_its_shadow() {
        let mut doc = parse(&minimal(
            r##"<column shadow="2pt 3pt #112233"><row shadow="-2pt -3pt #fff 0.2"/><grid shadow="0pt 0pt #000 0"/><frame shadow="1pt 1pt #000 1"/><table cols="1fr"><tr><td shadow=".5pt 1.5pt #abc .25"/></tr></table><column/></column>"##,
        )).unwrap();
        let column = &doc.section_layouts()[0].children[0];
        let shadows: Vec<Option<Shadow>> = column.children.iter().map(|n| n.shadow.as_deref().cloned()).collect();
        assert_eq!(column.shadow.as_deref().cloned(), shadow(2.0, 3.0, "#112233", 1.0));
        assert_eq!(shadows[0], shadow(-2.0, -3.0, "#ffffff", 0.2));
        assert_eq!(shadows[1], shadow(0.0, 0.0, "#000000", 0.0));
        assert_eq!(shadows[2], shadow(1.0, 1.0, "#000000", 1.0));
        assert_eq!(column.children[3].children[0].children[0].shadow.as_deref().cloned(), shadow(0.5, 1.5, "#aabbcc", 0.25));
        assert_eq!(shadows[4], None, "a box without a shadow has none");
    }

    #[test]
    fn a_shadow_takes_a_length_in_any_unit_and_a_color_token() {
        let xml = r##"<lpdf version="1"><tokens><colors><color name="ink" value="#123456"/></colors></tokens>
            <document><section><layout><column shadow="1in 25.4mm ink"/></layout></section></document></lpdf>"##;
        let mut doc = parse(xml).unwrap();
        let shadow = doc.section_layouts()[0].children[0].shadow.clone().unwrap();
        assert_eq!((shadow.dx, shadow.dy, shadow.color.as_str()), (72.0, 72.0, "#123456"));
    }

    #[test]
    fn a_box_in_a_kit_tree_reads_its_shadow() {
        let json = r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[
            {"type":"layout","attrs":{},"nodes":[{"type":"column","attrs":{"shadow":"2pt 3pt #112233 0.5"},"nodes":[]}]}]}]}"#;
        let mut doc = parse_tree(json).unwrap();
        assert_eq!(doc.section_layouts()[0].children[0].shadow.as_deref().cloned(), shadow(2.0, 3.0, "#112233", 0.5));
    }

    #[test]
    fn a_shadow_that_is_not_x_y_color_and_an_opacity_says_so() {
        for (value, says) in [
            ("", "expected 'x y color'"),
            ("2pt", "expected 'x y color'"),
            ("2pt 3pt", "expected 'x y color'"),
            ("2pt 3pt #000 0.5 4pt", "expected 'x y color'"),
            ("two 3pt #000", "x 'two' is not a length"),
            ("2pt 3px #000", "y '3px' is not a length"),
            ("2pt NaNpt #000", "y 'NaNpt' is not a length"),
            ("2pt 3pt #12", "invalid hex color"),
            ("2pt 3pt nope", "unknown color token: 'nope'"),
            ("2pt 3pt #000 1.5", "opacity '1.5' is not a number from 0 to 1"),
            ("2pt 3pt #000 half", "opacity 'half' is not a number from 0 to 1"),
            ("2pt 3pt #000 -0.1", "opacity '-0.1' is not a number from 0 to 1"),
        ] {
            let err = parse(&minimal(&format!(r#"<column shadow="{value}"/>"#))).unwrap_err();
            assert!(err.contains("<column>") && err.contains(&format!("invalid shadow '{value}'")) && err.contains(says), "{value}: {err}");
        }
    }

    #[test]
    fn layer_shapes_read_their_shadow() {
        use crate::canvas::CanvasPrimitive;
        let xml = r##"<lpdf version="1"><document size="a4"><section><layer>
            <rect x="10pt" y="20pt" width="100pt" height="50pt" shadow="2pt 3pt #112233"/>
            <circle x="10pt" y="20pt" r="5pt" shadow="-2pt 3pt #fff 0.5"/>
            <ellipse x="10pt" y="20pt" rx="8pt" ry="3pt" shadow="2pt -3pt #000 0"/>
            <rect x="10pt" y="20pt" width="100pt" height="50pt"/>
        </layer></section></document></lpdf>"##;
        let doc = parse(xml).unwrap();
        let children = &as_layer(&doc.sections[0].children[0]).children;
        let shadows: Vec<Option<Shadow>> = children.iter().map(|c| match c {
            CanvasPrimitive::Rect(r) => r.shadow.clone(),
            CanvasPrimitive::Circle(c) => c.shadow.clone(),
            CanvasPrimitive::Ellipse(e) => e.shadow.clone(),
            _ => panic!("expected a shape"),
        }).collect();
        assert_eq!(shadows, vec![
            shadow(2.0, 3.0, "#112233", 1.0),
            shadow(-2.0, 3.0, "#ffffff", 0.5),
            shadow(2.0, -3.0, "#000000", 0.0),
            None,
        ]);
    }

    #[test]
    fn layer_shapes_in_a_kit_tree_read_their_shadow() {
        use crate::canvas::CanvasPrimitive;
        let json = r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[
            {"type":"layer","attrs":{},"nodes":[
                {"type":"rect","attrs":{"x":"10pt","y":"20pt","width":"100pt","height":"50pt","shadow":"2pt 3pt #112233"}},
                {"type":"circle","attrs":{"x":"10pt","y":"20pt","r":"5pt","shadow":"-2pt 3pt #fff 0.5"}},
                {"type":"ellipse","attrs":{"x":"10pt","y":"20pt","rx":"8pt","ry":"3pt","shadow":"2pt -3pt #000 0"}}]}]}]}"#;
        let doc = parse_tree(json).unwrap();
        let shadows: Vec<Option<Shadow>> = as_layer(&doc.sections[0].children[0]).children.iter().map(|c| match c {
            CanvasPrimitive::Rect(r) => r.shadow.clone(),
            CanvasPrimitive::Circle(c) => c.shadow.clone(),
            CanvasPrimitive::Ellipse(e) => e.shadow.clone(),
            _ => panic!("expected a shape"),
        }).collect();
        assert_eq!(shadows, vec![
            shadow(2.0, 3.0, "#112233", 1.0),
            shadow(-2.0, 3.0, "#ffffff", 0.5),
            shadow(2.0, -3.0, "#000000", 0.0),
        ]);
    }

    #[test]
    fn a_layer_shape_shadow_that_is_not_valid_says_so() {
        let xml = r##"<lpdf version="1"><document size="a4"><section><layer>
            <rect x="10pt" y="20pt" width="100pt" height="50pt" shadow="2pt 3pt"/>
        </layer></section></document></lpdf>"##;
        let err = parse(xml).unwrap_err();
        assert!(err.contains("<rect>") && err.contains("invalid shadow '2pt 3pt'"), "{err}");
    }

    /// The nodes of the `text-at` that are the children of the layer at `index` in the first section.
    fn layer_texts(doc: &Document, index: usize) -> Vec<&Node> {
        as_layer(&doc.sections[0].children[index]).children.iter().map(|c| match c {
            canvas::CanvasPrimitive::Placed(placed) => &placed.node,
            _ => panic!("expected a text-at"),
        }).collect()
    }

    #[test]
    fn a_layer_gives_its_text_at_a_font_a_size_and_a_color() {
        let xml = r##"<lpdf version="1">
            <assets><font name="body" core="Times-Roman"/><font name="mono" core="Courier"/></assets>
            <tokens><colors><color name="text" value="#223344"/></colors></tokens>
            <document size="a4" font="body"><section>
                <layer font="mono" font-size="s" color="danger">
                    <text-at x="0pt" y="0pt">own</text-at>
                    <text-at x="0pt" y="9pt" font="body" font-size="20pt" color="#445566">theirs</text-at>
                </layer>
                <layer font-size="l"><text-at x="0pt" y="0pt">size only</text-at></layer>
                <layer><text-at x="0pt" y="0pt">none</text-at></layer>
            </section></document></lpdf>"##;
        let doc = parse(xml).unwrap();
        let summary = |n: &Node| (n.font.clone(), n.font_size, n.text_color.clone().unwrap());
        let danger = "#d92626".to_string();
        assert_eq!(summary(layer_texts(&doc, 0)[0]), ("Courier".into(), 9.0, danger));
        assert_eq!(summary(layer_texts(&doc, 0)[1]), ("Times-Roman".into(), 20.0, "#445566".into()));
        assert_eq!(summary(layer_texts(&doc, 1)[0]), ("Times-Roman".into(), 14.0, "#223344".into()));
        assert_eq!(summary(layer_texts(&doc, 2)[0]), ("Times-Roman".into(), 11.0, "#223344".into()));
    }

    #[test]
    fn a_layer_in_a_kit_tree_gives_its_text_at_a_font_a_size_and_a_color() {
        let json = r##"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[
            {"type":"layer","attrs":{"font":"Courier","font-size":"s","color":"danger"},"nodes":[
                {"type":"text-at","attrs":{"x":"0pt","y":"0pt"},"nodes":["own"]},
                {"type":"text-at","attrs":{"x":"0pt","y":"9pt","color":"#445566"},"nodes":["theirs"]}]}]}]}"##;
        let doc = parse_tree(json).unwrap();
        let texts = layer_texts(&doc, 0);
        assert_eq!((texts[0].font.as_str(), texts[0].font_size, texts[0].text_color.as_deref()), ("Courier", 9.0, Some("#d92626")));
        assert_eq!((texts[1].font.as_str(), texts[1].font_size, texts[1].text_color.as_deref()), ("Courier", 9.0, Some("#445566")));
    }

    #[test]
    fn a_layer_with_a_size_or_color_that_is_not_one_says_so() {
        for (attrs, expected) in [(r#"font-size="huge""#, "text size"), (r#"color="nope""#, "color token")] {
            let err = parse(&format!(
                r#"<lpdf version="1"><document size="a4"><section><layer {attrs}><text-at x="0pt" y="0pt">a</text-at></layer></section></document></lpdf>"#
            )).unwrap_err();
            assert!(err.contains("<layer>") && err.contains("at line") && err.contains(expected), "{err}");
        }
    }

    #[test]
    fn text_at_reads_its_line_limits() {
        let xml = layer_document("", r#"<text-at x="0pt" y="0pt" width="90pt" max-lines="3" ellipsis="true" line-height="1.5">Hi</text-at>"#);
        let doc = parse(&xml).unwrap();
        let node = &first_placed(&doc).node;
        assert_eq!((node.width_constraint, node.max_lines, node.ellipsis, node.line_height), (Some(90.0), Some(3), true, 1.5));
        // One line, cut off at nothing, unless told otherwise.
        let doc = parse(&layer_document("", r#"<text-at x="0pt" y="0pt">Hi</text-at>"#)).unwrap();
        let node = &first_placed(&doc).node;
        assert_eq!((node.width_constraint, node.max_lines, node.ellipsis, node.line_height), (None, Some(1), false, 1.2));
    }

    #[test]
    fn img_at_and_barcode_at_are_the_image_and_barcode_of_the_layout() {
        let head = r#"<assets><image name="logo"/></assets>"#;
        let doc = parse(&layer_document(head, r#"<img-at name="logo" x="10pt" y="20pt" width="60pt"/>"#)).unwrap();
        let placed = first_placed(&doc);
        assert_eq!(placed.node.kind, NodeKind::Img);
        assert_eq!((placed.node.width_constraint, placed.node.img_height_constraint), (Some(60.0), None));

        let doc = parse(&layer_document("", r#"<barcode-at type="qr" data="x" x="10pt" y="20pt" height="50pt"/>"#)).unwrap();
        let node = &first_placed(&doc).node;
        assert_eq!(node.kind, NodeKind::Barcode);
        assert_eq!((node.width_constraint, node.img_height_constraint), (Some(50.0), Some(50.0)));
    }

    #[test]
    fn a_text_img_or_barcode_in_a_layer_says_to_use_the_at_element() {
        let head = r#"<assets><image name="logo"/></assets>"#;
        for (body, hint) in [
            (r#"<text x="0pt" y="0pt">Hi</text>"#, "<text-at>"),
            (r#"<img name="logo" x="0pt" y="0pt" width="9pt" height="9pt"/>"#, "<img-at>"),
            (r#"<barcode type="qr" data="x" x="0pt" y="0pt" width="9pt"/>"#, "<barcode-at>"),
        ] {
            let err = parse(&layer_document(head, body)).unwrap_err();
            assert!(err.contains("at line") && err.contains(hint), "{err}");
        }
        let tree = |node: &str| format!(r#"{{"version":1,"type":"document","attrs":{{}},"nodes":[{{"type":"section","attrs":{{}},"nodes":[
            {{"type":"layer","attrs":{{}},"nodes":[{node}]}}]}}]}}"#);
        let err = parse_tree(&tree(r#"{"type":"text","attrs":{"x":"0pt","y":"0pt"},"nodes":["Hi"]}"#)).unwrap_err();
        assert!(err.contains("'text-at'"), "{err}");
    }

    #[test]
    fn errors_for_text_at_img_at_and_barcode_at() {
        let head = r#"<assets><image name="logo"/></assets>"#;
        for (body, expected) in [
            (r#"<text-at x="0pt" y="0pt" max-lines="2">Hi</text-at>"#, "'max-lines' of 2 needs a 'width'"),
            (r#"<text-at x="0pt" y="0pt" max-lines="0">Hi</text-at>"#, "invalid max-lines"),
            (r#"<text-at x="0pt" y="0pt" line-height="0">Hi</text-at>"#, "invalid line-height"),
            (r#"<text-at x="0pt" y="0pt" line-height="tall">Hi</text-at>"#, "invalid line-height 'tall'"),
            (r#"<text-at x="0pt" y="0pt" width="fill">Hi</text-at>"#, "invalid width"),
            (r#"<text-at x="0pt" y="0pt" data-source="items">Hi</text-at>"#, "a list belongs in the layout"),
            (r#"<img-at name="logo" x="0pt" y="0pt"/>"#, "needs a 'width' or a 'height'"),
            (r#"<barcode-at type="code128" data="x" x="0pt" y="0pt"/>"#, "needs a 'width' or a 'height'"),
            (r#"<barcode-at type="qr" data="x" x="0pt" y="0pt"/>"#, "needs a 'height', or a 'width'"),
            (r#"<rect x="0pt" y="0pt" width="9pt" height="9pt" opacity="0.5"/>"#, "put the element in a <layer> with opacity"),
            (r#"<text-at x="0pt" y="0pt" opacity="0.5">Hi</text-at>"#, "put the element in a <layer> with opacity"),
        ] {
            let err = parse(&layer_document(head, body)).unwrap_err();
            assert!(err.contains(expected), "{body}: {err}");
        }
    }

    #[test]
    fn a_layer_reads_its_clip_and_its_conditions() {
        let xml = r#"<lpdf version="1"><document size="a4"><section>
            <layer clip="10pt 20pt 100pt 50pt" data-if="draft" data-if-not="final"><rect width="9pt" height="9pt"/></layer>
        </section></document></lpdf>"#;
        let doc = parse(xml).unwrap();
        let layer = as_layer(&doc.sections[0].children[0]);
        assert_eq!(layer.clip, Some([10.0, 20.0, 100.0, 50.0]));
        assert_eq!((layer.data_if.as_deref(), layer.data_if_not.as_deref()), (Some("draft"), Some("final")));

        for (attrs, expected) in [
            (r#"clip="10pt 20pt""#, "expected x y w h"),
            (r#"data-value="title""#, "do not apply to a layer"),
            (r#"data-source="items""#, "do not apply to a layer"),
        ] {
            let xml = format!(r#"<lpdf version="1"><document size="a4"><section><layer {attrs}/></section></document></lpdf>"#);
            let err = parse(&xml).unwrap_err();
            assert!(err.contains(expected), "{attrs}: {err}");
        }
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
        let err = parse(&minimal(r#"<column align="middle" />"#)).unwrap_err();
        assert!(err.contains("<column>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("middle"),   "{err}");
        assert!(err.contains("start"),    "{err}"); // allowed values present
    }

    #[test]
    fn err_invalid_justify() {
        let err = parse(&minimal(r#"<column justify="around" />"#)).unwrap_err();
        assert!(err.contains("<column>"),  "{err}");
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
        let err = parse(&minimal(r#"<column paginate="always" />"#)).unwrap_err();
        assert!(err.contains("<column>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("always"),   "{err}");
        assert!(err.contains("break-before"), "{err}"); // allowed values present
    }

    #[test]
    fn err_invalid_widows_and_orphans() {
        for (attr, value) in [("widows", "0"), ("widows", "11"), ("orphans", "two"), ("orphans", "2.5")] {
            let err = parse(&minimal(&format!(r#"<text {attr}="{value}">Words</text>"#))).unwrap_err();
            assert!(err.contains("<text>"), "{err}");
            assert!(err.contains("at line"), "{err}");
            assert!(err.contains(&format!("invalid {attr} value: '{value}'")), "{err}");
            assert!(err.contains("1 to 10"), "{err}"); // allowed values present
        }
    }

    #[test]
    fn a_table_row_takes_keep_with_next() {
        let mut doc = parse(&minimal(
            r#"<table cols="1fr"><tr keep="with-next"><td><text>A</text></td></tr><tr><td><text>B</text></td></tr></table>"#,
        )).unwrap();
        let layouts = doc.section_layouts();
        let rows = &layouts[0].children[0].children;
        assert_eq!((rows[0].keep, rows[1].keep), (RowKeep::WithNext, RowKeep::None));
    }

    #[test]
    fn a_table_cell_takes_colspan_and_spans_one_column_without_it() {
        let mut doc = parse(&minimal(
            r#"<table cols="1fr 1fr 1fr"><tr><td colspan="2"><text>A</text></td><td><text>B</text></td></tr></table>"#,
        )).unwrap();
        let layouts = doc.section_layouts();
        let cells = &layouts[0].children[0].children[0].children;
        assert_eq!((cells[0].col_span, cells[1].col_span), (2, 1));
    }

    #[test]
    fn err_invalid_colspan() {
        for value in ["0", "-1", "two", "1.5", "", "70000"] {
            let err = parse(&minimal(&format!(
                r#"<table cols="1fr"><tr><td colspan="{value}"><text>A</text></td></tr></table>"#
            ))).unwrap_err();
            assert!(err.contains("<td>"), "{err}");
            assert!(err.contains("at line"), "{err}");
            assert!(err.contains(&format!("invalid colspan value: '{value}'")), "{err}");
        }
    }

    #[test]
    fn err_a_row_spans_more_columns_than_the_table_has() {
        let table = |row: &str| minimal(&format!(r#"<table cols="1fr 1fr">{row}</table>"#));
        for (row, spanned) in [
            (r#"<tr><td colspan="2"><text>A</text></td><td><text>B</text></td></tr>"#, 3),
            (r#"<tr><td><text>A</text></td><td colspan="2"><text>B</text></td></tr>"#, 3),
            (r#"<tr><td colspan="3"><text>A</text></td></tr>"#, 3),
            // Cells without a colspan count too: a third cell has no column to go in.
            (r#"<tr><td><text>A</text></td><td><text>B</text></td><td><text>C</text></td></tr>"#, 3),
        ] {
            let err = parse(&table(row)).unwrap_err();
            assert!(err.contains("<tr>"), "{err}");
            assert!(err.contains("at line"), "{err}");
            assert!(err.contains(&format!("span {spanned} columns, but the table has 2")), "{err}");
        }
        let err = parse(&table(r#"<thead><td colspan="3"><text>A</text></td></thead>"#)).unwrap_err();
        assert!(err.contains("<thead>"), "{err}");
        assert!(err.contains("span 3 columns, but the table has 2"), "{err}");
    }

    #[test]
    fn a_table_cell_takes_rowspan_and_spans_one_row_without_it() {
        let mut doc = parse(&minimal(
            r#"<table cols="1fr 1fr"><tr><td rowspan="2"><text>A</text></td><td><text>B</text></td></tr><tr><td><text>C</text></td></tr></table>"#,
        )).unwrap();
        let layouts = doc.section_layouts();
        let rows = &layouts[0].children[0].children;
        assert_eq!((rows[0].children[0].row_span, rows[0].children[1].row_span, rows[1].children[0].row_span), (2, 1, 1));
    }

    #[test]
    fn err_invalid_rowspan() {
        for value in ["0", "-1", "two", "1.5", "", "70000"] {
            let err = parse(&minimal(&format!(
                r#"<table cols="1fr"><tr><td rowspan="{value}"><text>A</text></td></tr></table>"#
            ))).unwrap_err();
            assert!(err.contains("<td>"), "{err}");
            assert!(err.contains("at line"), "{err}");
            assert!(err.contains(&format!("invalid rowspan value: '{value}'")), "{err}");
        }
    }

    #[test]
    fn err_a_row_has_a_cell_too_many_under_a_rowspan() {
        // The span holds the first column of the second row, so it has two columns for its three cells.
        let err = parse(&minimal(
            r#"<table cols="1fr 1fr 1fr"><tr><td rowspan="2"><text>A</text></td><td><text>B</text></td><td><text>C</text></td></tr>
                <tr><td><text>D</text></td><td><text>E</text></td><td><text>F</text></td></tr></table>"#,
        )).unwrap_err();
        assert!(err.contains("<tr>"), "{err}");
        assert!(err.contains("at line 2"), "the second row: {err}");
        assert!(err.contains("span 3 columns, but only 2 of the table's 3 are free"), "{err}");
    }

    #[test]
    fn err_a_colspan_does_not_fit_between_the_columns_that_a_rowspan_holds() {
        let err = parse(&minimal(
            r#"<table cols="1fr 1fr 1fr"><tr><td><text>A</text></td><td rowspan="2"><text>B</text></td><td><text>C</text></td></tr>
                <tr><td colspan="2"><text>D</text></td></tr></table>"#,
        )).unwrap_err();
        assert!(err.contains("<tr>"), "{err}");
        assert!(err.contains(r#"colspan="2" does not fit before column 2"#), "{err}");
    }

    #[test]
    fn err_a_rowspan_runs_past_the_last_row() {
        for (rows, past) in [
            (r#"<tr><td rowspan="2"><text>A</text></td><td><text>B</text></td></tr>"#, "1 row"),
            (r#"<tr><td><text>A</text></td></tr><tr><td><text>B</text></td><td rowspan="4"><text>C</text></td></tr>"#, "3 rows"),
        ] {
            let err = parse(&minimal(&format!(r#"<table cols="1fr 1fr">{rows}</table>"#))).unwrap_err();
            assert!(err.contains("<table>"), "{err}");
            assert!(err.contains("at line"), "{err}");
            assert!(err.contains(&format!("a rowspan that runs {past} past the last row of the table")), "{err}");
        }
    }

    #[test]
    fn err_a_header_cell_has_a_rowspan() {
        let err = parse(&minimal(
            r#"<table cols="1fr 1fr"><thead><td rowspan="2"><text>A</text></td><td><text>B</text></td></thead>
                <tr><td><text>C</text></td></tr></table>"#,
        )).unwrap_err();
        assert!(err.contains("<thead>"), "{err}");
        assert!(err.contains("the header is a single row"), "{err}");
    }

    #[test]
    fn a_table_whose_rowspans_line_up_with_its_columns_is_accepted() {
        for rows in [
            // The span ends with the last row.
            r#"<tr><td rowspan="2"><text>A</text></td><td><text>B</text></td></tr><tr><td><text>C</text></td></tr>"#,
            // The rows under a span may have fewer cells than the free columns.
            r#"<tr><td rowspan="3"><text>A</text></td><td><text>B</text></td></tr><tr></tr><tr><td><text>C</text></td></tr>"#,
            // Spans side by side, and one that starts under another's end.
            r#"<tr><td rowspan="2"><text>A</text></td><td rowspan="3"><text>B</text></td></tr><tr></tr><tr><td><text>C</text></td></tr>"#,
        ] {
            assert!(parse(&minimal(&format!(r#"<table cols="1fr 1fr">{rows}</table>"#))).is_ok(), "{rows}");
        }
    }

    #[test]
    fn a_row_that_spans_as_many_columns_as_the_table_has_or_fewer_is_accepted() {
        for row in [
            r#"<tr><td colspan="2"><text>A</text></td></tr>"#,
            r#"<tr><td><text>A</text></td><td><text>B</text></td></tr>"#,
            r#"<tr><td><text>A</text></td></tr>"#,
        ] {
            assert!(parse(&minimal(&format!(r#"<table cols="1fr 1fr">{row}</table>"#))).is_ok(), "{row}");
        }
    }

    #[test]
    fn err_invalid_row_keep() {
        for value in ["together", "with-body", "yes", ""] {
            let err = parse(&minimal(&format!(
                r#"<table cols="1fr"><tr keep="{value}"><td><text>A</text></td></tr></table>"#
            ))).unwrap_err();
            assert!(err.contains("<tr>"), "{err}");
            assert!(err.contains("at line"), "{err}");
            assert!(err.contains(&format!("invalid keep value: '{value}'")), "{err}");
            assert!(err.contains("with-next"), "{err}"); // allowed value present
        }
    }

    #[test]
    fn row_takes_justify_between_and_children_width_fill() {
        let mut doc = parse(&minimal(r#"<row justify="between"><text width="fill">A</text><text width="40pt">B</text></row>"#)).unwrap();
        let layouts = doc.section_layouts();
        let row = &layouts[0].children[0];
        assert_eq!(row.justify, Justify::Between);
        assert!(row.children[0].width_fill && row.children[0].width_constraint.is_none());
        assert!(!row.children[1].width_fill && row.children[1].width_constraint == Some(40.0));
    }

    #[test]
    fn err_renamed_layout_element_names_the_new_one() {
        for (old, new) in [("stack", "<column>"), ("cluster", "<row>"), ("flank", "width=\"fill\""), ("split", "justify=\"between\"")] {
            let err = parse(&minimal(&format!("<{old} />"))).unwrap_err();
            assert!(err.contains(&format!("<{old}>")), "{err}");
            assert!(err.contains("at line"), "{err}");
            assert!(err.contains(new), "{err}");
        }
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
        let err = parse(&minimal(r#"<barcode type="qr" data="x" width="100pt" ec="Z" />"#)).unwrap_err();
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
        let xml = r#"<lpdf version="1"><document size="a4">
  <section><layout margin="28pt">
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
    fn err_qr_barcode_without_width_or_height() {
        for body in [r#"<barcode type="qr" data="x" />"#, r#"<barcode type="qr" data="x" width="fill" />"#] {
            let err = parse(&minimal(body)).unwrap_err();
            assert!(err.contains("<barcode>"), "{err}");
            assert!(err.contains("at line"),  "{err}");
            assert!(err.contains("'height'") && err.contains("'width'"), "{err}");
        }
    }

    #[test]
    fn err_barcode_size_names_width_and_height() {
        let err = parse(&minimal(r#"<barcode type="qr" data="x" size="100pt" />"#)).unwrap_err();
        assert!(err.contains("<barcode>"), "{err}");
        assert!(err.contains("at line"),  "{err}");
        assert!(err.contains("'size'") && err.contains("'width'") && err.contains("'height'"), "{err}");

        let tree = r#"{"version":1,"type":"document","attrs":{},"nodes":[{"type":"section","attrs":{},"nodes":[
            {"type":"layout","nodes":[{"type":"barcode","attrs":{"type":"qr","data":"x","size":"100pt"}}]}]}]}"#;
        let err = parse_tree(tree).unwrap_err();
        assert!(err.contains("'size'") && err.contains("'width'") && err.contains("'height'"), "{err}");
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
        let xml = r#"<lpdf version="1"><document size="a4">
  <section><layout margin="28pt">
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
        let err = parse(&minimal("<frame><column /><column /></frame>")).unwrap_err();
        assert!(err.contains("<frame>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
    }

    #[test]
    fn err_table_bad_child() {
        let err = parse(&minimal("<table><column /></table>")).unwrap_err();
        assert!(err.contains("<table>"),  "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("column"),    "{err}");
    }

    #[test]
    fn err_tr_bad_child() {
        let err = parse(&minimal("<table><tr><column /></tr></table>")).unwrap_err();
        assert!(err.contains("<tr>"),     "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("column"),    "{err}");
    }

    // Category 5 — Asset reference

    #[test]
    fn err_img_unknown_asset() {
        let err = parse(&minimal(r#"<img name="ghost" />"#)).unwrap_err();
        assert!(err.contains("<img>"),    "{err}");
        assert!(err.contains("at line"), "{err}");
        assert!(err.contains("ghost"),    "{err}");
    }

    // ── Bookmarks and the table of contents ──────────────────────────────────

    fn document(sections: &str) -> String {
        format!(r#"<lpdf version="1"><document>{sections}</document></lpdf>"#)
    }

    /// The bookmarks of a one-section document, in source order.
    fn bookmarks_of(body: &str) -> Vec<Bookmark> {
        let mut doc = parse(&minimal(body)).unwrap();
        let mut found = Vec::new();
        fn walk(nodes: &[Node], found: &mut Vec<Bookmark>) {
            for node in nodes {
                found.extend(node.bookmarks.iter().cloned());
                walk(&node.children, found);
            }
        }
        walk(&doc.section_layouts()[0].children, &mut found);
        found
    }

    #[test]
    fn bookmark_is_carried_by_the_element_after_it() {
        let mut doc = parse(&minimal(
            r#"<text>Before</text><bookmark label="Methods" level="2"/><text>Methods</text>"#,
        )).unwrap();
        let children = &doc.section_layouts()[0].children;
        assert_eq!(children.len(), 2, "a bookmark is not a node of its own");
        assert!(children[0].bookmarks.is_empty());
        assert_eq!(
            children[1].bookmarks,
            vec![Bookmark { id: "methods".into(), label: "Methods".into(), level: 2, explicit_id: false }]
        );
    }

    #[test]
    fn bookmark_inside_a_container_is_carried_by_the_child_after_it() {
        let bookmarks = bookmarks_of(r#"<column><text>A</text><bookmark label="Inner" id="inner"/><text>B</text></column>"#);
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].id, "inner");
        assert_eq!(bookmarks[0].level, 1);
    }

    #[test]
    fn bookmark_id_is_made_from_the_label() {
        for (label, id) in [
            ("Introduction", "introduction"),
            ("Data Collection", "data-collection"),
            ("§ 3.1 — Methods", "3-1-methods"),
            ("Über  uns!", "über-uns"),
            ("§ —", "bookmark"),
        ] {
            let bookmarks = bookmarks_of(&format!(r#"<bookmark label="{label}"/><text>x</text>"#));
            assert_eq!(bookmarks[0].id, id, "label {label:?}");
        }
    }

    #[test]
    fn repeated_ids_made_from_labels_take_a_suffix() {
        let bookmarks = bookmarks_of(
            r#"<bookmark label="Introduction"/><text>a</text>
               <bookmark label="Introduction"/><text>b</text>
               <bookmark label="Notes" id="introduction-2"/><text>c</text>"#,
        );
        let ids: Vec<&str> = bookmarks.iter().map(|b| b.id.as_str()).collect();
        // The explicit id is kept, so the second label-made id moves on to -3.
        assert_eq!(ids, ["introduction", "introduction-3", "introduction-2"]);
    }

    #[test]
    fn two_explicit_ids_that_are_the_same_are_an_error() {
        let err = parse(&minimal(
            r#"<bookmark label="A" id="same"/><text>a</text><bookmark label="B" id="same"/><text>b</text>"#,
        )).unwrap_err();
        assert!(err.contains("'same'"), "{err}");
    }

    #[test]
    fn bookmark_rejects_a_level_outside_1_to_6_and_an_empty_id() {
        for (attrs, expected) in [(r#"level="7""#, "level"), (r#"level="0""#, "level"), (r#"id="""#, "id")] {
            let err = parse(&minimal(&format!(r#"<bookmark label="A" {attrs}/><text>a</text>"#))).unwrap_err();
            assert!(err.contains("<bookmark>") && err.contains(expected), "{attrs}: {err}");
        }
    }

    #[test]
    fn bookmark_with_nothing_after_it_is_an_error() {
        let err = parse(&minimal(r#"<column><text>a</text><bookmark label="End"/></column>"#)).unwrap_err();
        assert!(err.contains("<column>") && err.contains("End"), "{err}");
    }

    #[test]
    fn bookmark_is_not_allowed_in_a_layer_or_between_table_rows() {
        let layer = document(r#"<section><layer><bookmark label="A"/></layer></section>"#);
        assert!(parse(&layer).is_err());
        let table = minimal(r#"<table cols="1fr"><bookmark label="A"/><tr><td><text>a</text></td></tr></table>"#);
        let err = parse(&table).unwrap_err();
        assert!(err.contains("<td>"), "{err}");
    }

    #[test]
    fn section_has_one_bookmark_of_its_own() {
        let mut doc = parse(&document(
            r#"<section><bookmark label="Cover"/><layer/></section>"#,
        )).unwrap();
        let layouts = doc.section_layouts();
        assert_eq!(layouts[0].bookmark.as_ref().map(|b| b.id.as_str()), Some("cover"));

        let err = parse(&document(
            r#"<section><bookmark label="A"/><bookmark label="B"/><layout><text>a</text></layout></section>"#,
        )).unwrap_err();
        assert!(err.contains("one <bookmark>"), "{err}");
    }

    #[test]
    fn toc_section_reads_its_indent_step() {
        for (attrs, step) in [(r#"toc-indent-step="20pt""#, 20.0), ("", DEFAULT_TOC_INDENT_STEP)] {
            let mut doc = parse(&document(&format!(r#"<section type="toc" {attrs}><layout><text>a</text></layout></section>"#))).unwrap();
            assert_eq!(doc.section_layouts()[0].toc.as_ref().map(|t| t.indent_step), Some(step), "{attrs}");
        }
        let err = parse(&document(r#"<section type="index"><layout><text>a</text></layout></section>"#)).unwrap_err();
        assert!(err.contains("index"), "{err}");
    }

    #[test]
    fn second_toc_section_is_an_error() {
        let err = parse(&document(
            r#"<section type="toc"><layout><text>a</text></layout></section>
               <section type="toc"><layout><text>b</text></layout></section>"#,
        )).unwrap_err();
        assert!(err.contains("only one <section type=\"toc\">"), "{err}");
    }

    #[test]
    fn tree_bookmarks_parse_as_the_xml_does() {
        let json = r#"{"version":1,"type":"document","attrs":{},"nodes":[
            {"type":"section","attrs":{"type":"toc"},"nodes":[
                {"type":"bookmark","attrs":{"label":"Contents"}},
                {"type":"layout","nodes":[{"type":"text","attrs":{},"nodes":["TOC"]}]}]},
            {"type":"section","attrs":{},"nodes":[
                {"type":"layout","nodes":[
                    {"type":"bookmark","attrs":{"label":"Methods","level":"2"}},
                    {"type":"text","attrs":{},"nodes":["Methods"]}]}]}]}"#;
        let mut doc = parse_tree(json).unwrap();
        let layouts = doc.section_layouts();
        assert!(layouts[0].toc.is_some());
        assert_eq!(layouts[0].bookmark.as_ref().map(|b| b.id.as_str()), Some("contents"));
        assert_eq!(layouts[1].children[0].bookmarks[0].id, "methods");
        assert_eq!(layouts[1].children[0].bookmarks[0].level, 2);
    }

    // ── Gradients ─────────────────────────────────────────────────────────────

    const FADE: &str = r##"<gradient name="fade" type="linear" from="brand" to="#fff" angle="90"/>"##;

    /// A document whose tokens define a color `brand` and the given gradients, around `body` in a layout.
    fn with_gradients(gradients: &str, body: &str) -> String {
        format!(
            r##"<lpdf version="1"><tokens><colors><color name="brand" value="#112233"/></colors><gradients>{gradients}</gradients></tokens>
            <document size="a4"><section><layout margin="28pt">{body}</layout></section></document></lpdf>"##
        )
    }

    fn stop_at(offset: f32, color: &str) -> crate::tokens::GradientStop {
        crate::tokens::GradientStop { offset, color: color.to_string() }
    }

    #[test]
    fn a_document_reads_the_gradients_of_its_tokens() {
        let doc = parse(&with_gradients(
            &format!(
                r##"{FADE}<gradient name="sunset" type="radial"><stop offset="0%" color="#f00"/><stop offset="40%" color="brand"/></gradient>"##
            ),
            "",
        ))
        .unwrap();
        let fade = &doc.gradients["fade"];
        assert_eq!((fade.kind, fade.angle), (crate::tokens::GradientKind::Linear, 90.0));
        assert_eq!(fade.stops, vec![stop_at(0.0, "#112233"), stop_at(1.0, "#ffffff")]);
        let sunset = &doc.gradients["sunset"];
        assert_eq!(sunset.kind, crate::tokens::GradientKind::Radial);
        assert_eq!(sunset.stops, vec![stop_at(0.0, "#ff0000"), stop_at(0.4, "#112233"), stop_at(1.0, "#112233")]);
    }

    #[test]
    fn a_gradient_may_come_before_the_colors_it_names() {
        let xml = format!(
            r##"<lpdf version="1"><tokens><gradients>{FADE}</gradients><colors><color name="brand" value="#445566"/></colors></tokens>
            <document><section><layout/></section></document></lpdf>"##
        );
        assert_eq!(parse(&xml).unwrap().gradients["fade"].stops[0].color, "#445566");
    }

    #[test]
    fn a_document_without_gradients_has_none() {
        assert!(parse(&minimal("")).unwrap().gradients.is_empty());
    }

    #[test]
    fn a_box_and_a_table_row_read_a_gradient_as_their_background() {
        let mut doc = parse(&with_gradients(
            FADE,
            r##"<column background="gradient:fade"><table cols="1fr"><thead background="gradient:fade"><td/></thead><tr background="#abc"><td background="gradient:fade"/></tr></table></column>"##,
        ))
        .unwrap();
        let column = &doc.section_layouts()[0].children[0];
        let table = &column.children[0];
        assert_eq!(column.background.as_deref(), Some("gradient:fade"));
        assert_eq!(table.children[0].background.as_deref(), Some("gradient:fade"), "the head");
        assert_eq!(table.children[1].background.as_deref(), Some("#aabbcc"), "a row with a color keeps it");
        assert_eq!(table.children[1].children[0].background.as_deref(), Some("gradient:fade"), "the cell");
    }

    #[test]
    fn a_section_and_a_document_read_a_gradient_as_their_background() {
        let xml = format!(
            r##"<lpdf version="1"><tokens><colors><color name="brand" value="#112233"/></colors><gradients>{FADE}</gradients></tokens>
            <document background="gradient:fade"><section><layout/></section><section background="#fff"><layout/></section></document></lpdf>"##
        );
        let mut doc = parse(&xml).unwrap();
        let layouts = doc.section_layouts();
        assert_eq!(layouts[0].background.as_deref(), Some("gradient:fade"));
        assert_eq!(layouts[1].background.as_deref(), Some("#ffffff"));
    }

    #[test]
    fn a_kit_tree_defines_gradients_with_a_number_or_a_string_for_the_angle() {
        let json = r##"{"version":1,"type":"document","attrs":{"tokens":{"colors":{"brand":"#112233"},"gradients":{
            "fade":{"type":"linear","from":"brand","to":"#fff","angle":90},
            "same":{"type":"linear","from":"brand","to":"#fff","angle":"90"},
            "multi":{"type":"radial","stops":[{"offset":"0%","color":"#f00"},{"offset":"100%","color":"brand"}]}}}},
            "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[
                {"type":"column","attrs":{"background":"gradient:fade"},"nodes":[]}]}]}]}"##;
        let mut doc = parse_tree(json).unwrap();
        assert_eq!(doc.gradients["fade"], doc.gradients["same"]);
        assert_eq!(doc.gradients["fade"].angle, 90.0);
        assert_eq!(doc.gradients["multi"].stops, vec![stop_at(0.0, "#ff0000"), stop_at(1.0, "#112233")]);
        assert_eq!(doc.section_layouts()[0].children[0].background.as_deref(), Some("gradient:fade"));
    }

    #[test]
    fn a_kit_tree_gradient_that_is_not_one_says_so() {
        let tree = |gradient: &str| {
            format!(
                r##"{{"version":1,"type":"document","attrs":{{"tokens":{{"gradients":{{"fade":{gradient}}}}}}},"nodes":[]}}"##
            )
        };
        for (gradient, says) in [
            (r##"{"from":"#f00","to":"#00f"}"##, "gradient 'fade': missing type"),
            (r##"{"type":"linear","from":"#f00"}"##, "give from and to, or at least two stops"),
            (r##"{"type":"linear","stops":[{"offset":"0%"}]}"##, "a stop needs an color"),
            (r##"{"type":"linear","stops":[{"color":"#f00"}]}"##, "a stop needs an offset"),
        ] {
            let err = parse_tree(&tree(gradient)).unwrap_err();
            assert!(err.contains(says), "{gradient}: {err}");
        }
    }

    #[test]
    fn a_gradient_that_is_not_one_says_so() {
        for (gradient, says) in [
            (r##"<gradient name="fade" type="linear" from="#f00"/>"##, "gradient 'fade': give from and to"),
            (r##"<gradient name="fade" type="conic" from="#f00" to="#00f"/>"##, "invalid type 'conic'"),
            (r##"<gradient name="fade" type="linear" from="#f00" to="#00f" angle="up"/>"##, "invalid angle 'up'"),
            (r##"<gradient type="linear" from="#f00" to="#00f"/>"##, "missing required attribute 'name'"),
            (r##"<gradient name="fade" from="#f00" to="#00f"/>"##, "missing required attribute 'type'"),
            (r##"<gradient name="fade" type="linear"><stop color="#f00"/></gradient>"##, "<stop>"),
            (r##"<gradient name="fade" type="linear"><step/></gradient>"##, "<step>"),
            (r##"<shade/>"##, "<shade>"),
        ] {
            let err = parse(&with_gradients(gradient, "")).unwrap_err();
            assert!(err.contains(says), "{gradient}: {err}");
        }
    }

    #[test]
    fn a_background_that_names_no_gradient_says_so() {
        for body in [
            r#"<column background="gradient:missing"/>"#,
            r#"<table cols="1fr"><tr background="gradient:missing"><td/></tr></table>"#,
        ] {
            let err = parse(&with_gradients(FADE, body)).unwrap_err();
            assert!(err.contains("unknown gradient token: 'missing'"), "{body}: {err}");
        }
    }

    #[test]
    fn a_gradient_is_refused_where_only_a_color_goes() {
        for body in [
            r#"<column color="gradient:fade"><text>a</text></column>"#,
            r#"<text color="gradient:fade">a</text>"#,
            r#"<text>a <span color="gradient:fade">b</span></text>"#,
            r#"<column border="s gradient:fade"/>"#,
            r#"<column shadow="1pt 1pt gradient:fade"/>"#,
            r#"<divider color="gradient:fade"/>"#,
            r#"<table cols="1fr" stripe="gradient:fade"><tr><td/></tr></table>"#,
            r#"<barcode type="qr" data="x" background="gradient:fade"/>"#,
        ] {
            let err = parse(&with_gradients(FADE, body)).unwrap_err();
            assert!(err.contains("is a gradient, which can fill a background or a shape"), "{body}: {err}");
        }
    }
}
