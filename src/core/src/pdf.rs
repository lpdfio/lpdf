// ── pdf.rs ────────────────────────────────────────────────────────────────────
//
// Native PDF rendering module.
//
// Converts a layout tree (slices of `RenderPage`) directly into binary PDF
// bytes using the `pdf-writer` crate.  This module replaces the TypeScript
// pdf-lib drawing layer that previously lived in the Node adapter.
//
// # High-level flow
//
// 1. **Font preparation** – For every font name appearing in the layout tree:
//    - If the font is a PDF built-in Type 1 name (Helvetica, Times-Roman, …),
//      a lightweight font dictionary referencing the resident font is written.
//    - If the font has associated TrueType bytes (loaded via `FontRegistry`),
//      the font is embedded as a CIDFont Type2 (TrueType) object wrapped in a
//      Type0 composite font, using Identity-H encoding for full Unicode support.
//
// 2. **Glyph collection** – For each embedded TrueType font, all Unicode code
//    points actually used in the document are collected so that the widths table
//    and ToUnicode CMap are built from real glyph metrics.
//
// 3. **Content streams** – One per page.  Coordinates are flipped from the
//    layout's top-down convention (y=0 at the top) to PDF's bottom-up
//    convention (y=0 at the bottom): `pdf_y = page_height − layout_y − node_h`.
//
// 4. **Annotations** – Collected during content building; written as separate
//    indirect objects and referenced from each page's `/Annots` array.
//
// 5. **Assembly** – All objects are written into a `pdf_writer::Pdf` buffer and
//    returned as `Vec<u8>`.

use std::collections::{HashMap, HashSet};

use pdf_writer::{Content, Name, Pdf, Rect, Ref, Str, TextStr};
use pdf_writer::types::{ActionType, AnnotationType, CidFontType, FontFlags};
use ttf_parser::Face;

use crate::render::{RenderNode, RenderPage};
use crate::parse::Meta;
use crate::tokens::FontDef;

// ── Public API ────────────────────────────────────────────────────────────────

/// Stores raw font bytes (TTF/OTF) for custom fonts referenced by a document
/// via its `<fonts src="…">` declarations.  Populate before calling
/// `render_pdf` so that each `Src` font definition has its bytes available.
pub struct FontRegistry {
    bytes: HashMap<String, Vec<u8>>,
}

impl FontRegistry {
    pub fn new() -> Self {
        Self { bytes: HashMap::new() }
    }

    /// Associate `name` (the font name as it appears in `<font name="…">`)
    /// with its raw font file bytes.
    pub fn register(&mut self, name: &str, data: Vec<u8>) {
        self.bytes.insert(name.to_string(), data);
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.bytes.get(name).map(|b| b.as_slice())
    }
}

// ── Built-in font table ───────────────────────────────────────────────────────

/// Map a logical font name to a PDF built-in PostScript font name, or `None`
/// if the name is not one of the 14 standard PDF resident fonts.  Every PDF
/// viewer is required to have these fonts; no bytes need to be embedded.
fn pdf_builtin_name(name: &str) -> Option<&'static str> {
    match name {
        "Helvetica"             => Some("Helvetica"),
        "Helvetica-Bold"        => Some("Helvetica-Bold"),
        "Helvetica-Oblique"     => Some("Helvetica-Oblique"),
        "Helvetica-BoldOblique" => Some("Helvetica-BoldOblique"),
        "Times-Roman"           => Some("Times-Roman"),
        "Times-Bold"            => Some("Times-Bold"),
        "Times-Italic"          => Some("Times-Italic"),
        "Times-BoldItalic"      => Some("Times-BoldItalic"),
        "Courier"               => Some("Courier"),
        "Courier-Bold"          => Some("Courier-Bold"),
        "Courier-Oblique"       => Some("Courier-Oblique"),
        "Courier-BoldOblique"   => Some("Courier-BoldOblique"),
        "Symbol"                => Some("Symbol"),
        "ZapfDingbats"          => Some("ZapfDingbats"),
        _                       => None,
    }
}

// ── Internal font representation ──────────────────────────────────────────────

/// A resolved font ready for embedding into the PDF output.
struct PreparedFont {
    /// Resource key used in the page's `/Font` dictionary, e.g. `"F0"`.
    resource_name: String,
    kind: PreparedFontKind,
}

enum PreparedFontKind {
    /// One of the 14 PDF resident Type 1 fonts.  No bytes to embed.
    Builtin { base_name: &'static str },
    /// Custom TrueType/OpenType font, to be embedded as CIDFont Type2 with
    /// Identity-H encoding, giving full Unicode support.
    Truetype { bytes: Vec<u8> },
}

impl PreparedFont {
    /// Width of `text` at `size` pt.
    ///
    /// Built-in fonts use a rough per-character average (0.5 × size) because
    /// we do not have AFM metrics at runtime.  TrueType fonts use real
    /// per-glyph advances from `ttf-parser`.
    fn text_width(&self, text: &str, size: f32) -> f32 {
        match &self.kind {
            PreparedFontKind::Builtin { base_name } => {
                crate::layout::text_width(base_name, text, size)
            }
            PreparedFontKind::Truetype { bytes } => text_width_ttf(bytes, text, size),
        }
    }

    /// Encode `text` into the raw byte string required by the content stream.
    ///
    /// - Builtin (Type 1 + WinAnsiEncoding): one Latin-1 byte per character.
    /// - Truetype (CIDFont + Identity-H): two big-endian glyph-ID bytes per
    ///   character, resolved via `ttf-parser`.
    fn encode_text(&self, text: &str) -> Vec<u8> {
        match &self.kind {
            PreparedFontKind::Builtin { .. }      => encode_latin1(text),
            PreparedFontKind::Truetype { bytes }  => encode_glyph_ids(bytes, text),
        }
    }
}

// ── Colour helper ─────────────────────────────────────────────────────────────

/// Parse `#rrggbb` or `#rgb` into three `0.0–1.0` floats.
/// Returns `(0.0, 0.0, 0.0)` (black) for malformed input.
fn parse_hex(hex: &str) -> (f32, f32, f32) {
    let h = hex.trim_start_matches('#');
    let expanded: String = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_string()
    };
    if expanded.len() != 6 {
        return (0.0, 0.0, 0.0);
    }
    let r = u8::from_str_radix(&expanded[0..2], 16).unwrap_or(0) as f32 / 255.0;
    let g = u8::from_str_radix(&expanded[2..4], 16).unwrap_or(0) as f32 / 255.0;
    let b = u8::from_str_radix(&expanded[4..6], 16).unwrap_or(0) as f32 / 255.0;
    (r, g, b)
}

// ── Text encoding ─────────────────────────────────────────────────────────────

/// Convert UTF-8 text to WinAnsiEncoding bytes (used by PDF built-in Type 1 fonts).
///
/// Coverage:
/// - `0x00–0x7F`  ASCII, direct.
/// - `0x80–0x9F`  Windows-1252 extension block (em dash, en dash, curly quotes, …).
/// - `0xA0–0xFF`  ISO-8859-1 upper half, direct (same in both encodings).
/// - Everything else is replaced by `?`.
fn encode_latin1(text: &str) -> Vec<u8> {
    text.chars().map(|c| win1252_byte(c).unwrap_or(b'?')).collect()
}

/// Map a Unicode scalar to its Windows-1252 / WinAnsiEncoding byte value.
fn win1252_byte(c: char) -> Option<u8> {
    let cp = c as u32;
    // ASCII and ISO-8859-1 upper half are identical in WinAnsiEncoding.
    if cp < 0x80 || (cp >= 0xA0 && cp <= 0xFF) {
        return Some(cp as u8);
    }
    // Windows-1252 extension block mapped to Unicode.
    Some(match c {
        '\u{20AC}' => 0x80, // €
        '\u{201A}' => 0x82, // ‚
        '\u{0192}' => 0x83, // ƒ
        '\u{201E}' => 0x84, // „
        '\u{2026}' => 0x85, // …
        '\u{2020}' => 0x86, // †
        '\u{2021}' => 0x87, // ‡
        '\u{02C6}' => 0x88, // ˆ
        '\u{2030}' => 0x89, // ‰
        '\u{0160}' => 0x8A, // Š
        '\u{2039}' => 0x8B, // ‹
        '\u{0152}' => 0x8C, // Œ
        '\u{017D}' => 0x8E, // Ž
        '\u{2018}' => 0x91, // '
        '\u{2019}' => 0x92, // '
        '\u{201C}' => 0x93, // "
        '\u{201D}' => 0x94, // "
        '\u{2022}' => 0x95, // •
        '\u{2013}' => 0x96, // – (en dash)
        '\u{2014}' => 0x97, // — (em dash)
        '\u{02DC}' => 0x98, // ˜
        '\u{2122}' => 0x99, // ™
        '\u{0161}' => 0x9A, // š
        '\u{203A}' => 0x9B, // ›
        '\u{0153}' => 0x9C, // œ
        '\u{017E}' => 0x9E, // ž
        '\u{0178}' => 0x9F, // Ÿ
        _          => return None,
    })
}

/// Encode text as two-byte big-endian glyph IDs for a TrueType font using
/// Identity-H encoding.  Unmapped characters produce `[0x00, 0x00]` (.notdef).
fn encode_glyph_ids(font_bytes: &[u8], text: &str) -> Vec<u8> {
    let face = match Face::parse(font_bytes, 0) {
        Ok(f)  => f,
        Err(_) => return vec![0u8; text.chars().count() * 2],
    };
    let mut out = Vec::with_capacity(text.chars().count() * 2);
    for c in text.chars() {
        let gid = face.glyph_index(c).map(|g| g.0).unwrap_or(0);
        out.push((gid >> 8) as u8);
        out.push((gid & 0xFF) as u8);
    }
    out
}

// ── Text width (TrueType) ─────────────────────────────────────────────────────

/// Compute the advance width of `text` at `size` pt using real glyph metrics.
/// Falls back to the 0.5× approximation if the font cannot be parsed.
fn text_width_ttf(font_bytes: &[u8], text: &str, size: f32) -> f32 {
    let face = match Face::parse(font_bytes, 0) {
        Ok(f)  => f,
        Err(_) => return text.chars().count() as f32 * 0.5 * size,
    };
    let upem  = face.units_per_em() as f32;
    let scale = size / upem;
    text.chars()
        .filter_map(|c| face.glyph_index(c))
        .filter_map(|gid| face.glyph_hor_advance(gid))
        .map(|adv| adv as f32 * scale)
        .sum()
}

// ── Rounded rectangle path ────────────────────────────────────────────────────

/// Append a rounded-corner rectangle to the current path using four cubic
/// Bézier arcs (κ ≈ 0.5523 gives the standard 90° arc approximation).
///
/// `x`, `y` are the lower-left corner in PDF (bottom-up) coordinates.
/// `r` is clamped to at most half the shorter side so it never inverts.
fn rounded_rect(content: &mut Content, x: f32, y: f32, w: f32, h: f32, r: f32) {
    // Clamp radius so corners cannot overlap each other.
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    if r == 0.0 {
        content.rect(x, y, w, h);
        return;
    }
    // κ = (4/3) × tan(π/8) ≈ 0.5523 – the control-point offset for a circular arc
    const KAPPA: f32 = 0.5523;
    let k = r * KAPPA;

    // Start at bottom-left corner (after the arc), travel clockwise
    content.move_to(x + r, y);
    // Bottom edge → bottom-right corner
    content.line_to(x + w - r, y);
    content.cubic_to(x + w - r + k, y,      x + w, y + r - k,      x + w, y + r);
    // Right edge → top-right corner
    content.line_to(x + w, y + h - r);
    content.cubic_to(x + w, y + h - r + k,  x + w - r + k, y + h,  x + w - r, y + h);
    // Top edge → top-left corner
    content.line_to(x + r, y + h);
    content.cubic_to(x + r - k, y + h,      x, y + h - r + k,      x, y + h - r);
    // Left edge → bottom-left corner (close)
    content.line_to(x, y + r);
    content.cubic_to(x, y + r - k,          x + r - k, y,           x + r, y);
    content.close_path();
}

// ── Annotation collector ──────────────────────────────────────────────────────

/// A link annotation rect + URL collected while building a page's content
/// stream, so we can pre-assign IDs and reference them from the page dict.
struct AnnotData {
    x1:  f32,
    y1:  f32,
    x2:  f32,
    y2:  f32,
    url: String,
}

// ── Glyph-usage scanner ───────────────────────────────────────────────────────

/// Recursively walk render nodes and record every Unicode character rendered
/// with `font_name`.  Used to build the widths table and ToUnicode CMap for
/// embedded TrueType fonts.
fn collect_chars_for_font(nodes: &[RenderNode], font_name: &str, out: &mut HashSet<char>) {
    for node in nodes {
        match node {
            RenderNode::Text(t) if t.font == font_name => {
                out.extend(t.content.chars());
            }
            RenderNode::Box(b)  => collect_chars_for_font(&b.children, font_name, out),
            RenderNode::Link(l) => collect_chars_for_font(&l.children, font_name, out),
            _                   => {}
        }
    }
}

/// Recursively collect all unique font names referenced by text nodes.
fn collect_used_fonts(nodes: &[RenderNode], out: &mut HashSet<String>) {
    for node in nodes {
        match node {
            RenderNode::Text(t) => { out.insert(t.font.clone()); }
            RenderNode::Box(b)  => collect_used_fonts(&b.children, out),
            RenderNode::Link(l) => collect_used_fonts(&l.children, out),
            _                   => {}
        }
    }
}

// ── Font resolution ───────────────────────────────────────────────────────────

/// Determine the `PreparedFontKind` for `name` by consulting the document's
/// font definitions and the caller-supplied byte registry.
///
/// Resolution order:
/// 1. Document defines the font as `Builtin(name)` → use the builtin.
/// 2. Document defines the font as `Src(_)` and the registry has bytes → embed.
/// 3. Document defines the font as `Src(_)` but no bytes → fall back to Helvetica.
/// 4. No document definition → try the name directly as a builtin, else Helvetica.
fn resolve_font_kind(
    name:      &str,
    font_defs: &HashMap<String, FontDef>,
    registry:  &FontRegistry,
) -> PreparedFontKind {
    if let Some(def) = font_defs.get(name) {
        match def {
            FontDef::Builtin(b) => {
                let ps = pdf_builtin_name(b).unwrap_or("Helvetica");
                PreparedFontKind::Builtin { base_name: ps }
            }
            FontDef::Src(_) => {
                if let Some(bytes) = registry.get(name) {
                    return PreparedFontKind::Truetype { bytes: bytes.to_vec() };
                }
                // Bytes not provided at render time → degrade gracefully.
                PreparedFontKind::Builtin { base_name: "Helvetica" }
            }
        }
    } else {
        let ps = pdf_builtin_name(name).unwrap_or("Helvetica");
        PreparedFontKind::Builtin { base_name: ps }
    }
}

// ── Content-stream builder ────────────────────────────────────────────────────

/// Write all render nodes for one page into a `Content` stream and collect
/// any link annotations encountered along the way.
///
/// `page_h` is required for the top-down → bottom-up coordinate flip:
///   `pdf_y = page_h − layout_y − node_height`
fn draw_nodes(
    content: &mut Content,
    annots:  &mut Vec<AnnotData>,
    nodes:   &[RenderNode],
    fonts:   &HashMap<String, PreparedFont>,
    page_h:  f32,
) {
    for node in nodes {
        draw_node(content, annots, node, fonts, page_h);
    }
}

fn draw_node(
    content: &mut Content,
    annots:  &mut Vec<AnnotData>,
    node:    &RenderNode,
    fonts:   &HashMap<String, PreparedFont>,
    page_h:  f32,
) {
    match node {
        // ── Box ──────────────────────────────────────────────────────────────
        RenderNode::Box(b) => {
            let has_fill   = b.fill.is_some();
            let has_border = b.border_width > 0.0 && b.border_color.is_some();

            // Flip the box y from top-down to bottom-up.
            let pdf_y = page_h - b.y - b.height;

            if has_fill || has_border {
                content.save_state();

                if has_fill {
                    let (r, g, bl) = parse_hex(b.fill.as_deref().unwrap_or("#000000"));
                    content.set_fill_rgb(r, g, bl);
                }
                if has_border {
                    let (r, g, bl) = parse_hex(
                        b.border_color.as_deref().unwrap_or("#000000"),
                    );
                    content.set_stroke_rgb(r, g, bl);
                    content.set_line_width(b.border_width);
                }

                if b.radius > 0.0 {
                    rounded_rect(content, b.x, pdf_y, b.width, b.height, b.radius);
                } else {
                    content.rect(b.x, pdf_y, b.width, b.height);
                }

                match (has_fill, has_border) {
                    (true,  true)  => { content.fill_nonzero_and_stroke(); }
                    (true,  false) => { content.fill_nonzero(); }
                    (false, true)  => { content.stroke(); }
                    (false, false) => { content.end_path(); }
                }

                content.restore_state();
            }

            draw_nodes(content, annots, &b.children, fonts, page_h);
        }

        // ── Line ─────────────────────────────────────────────────────────────
        RenderNode::Line(l) => {
            let (r, g, b) = parse_hex(&l.color);
            content.save_state();
            content.set_stroke_rgb(r, g, b);
            content.set_line_width(l.thickness);

            // Optional dash pattern for dashed/dotted lines.
            if let Some(dash) = &l.dash {
                content.set_dash_pattern(dash.iter().copied(), 0.0);
            }

            // Lines use single points; flip y directly (no height offset).
            content.move_to(l.x1, page_h - l.y1);
            content.line_to(l.x2, page_h - l.y2);
            content.stroke();
            content.restore_state();
        }

        // ── Text ─────────────────────────────────────────────────────────────
        RenderNode::Text(t) => {
            // Fall back to Helvetica if the font is not in the prepared map.
            let font = fonts.get(&t.font)
                .or_else(|| fonts.get("Helvetica"))
                .expect("Helvetica must always be present as the fallback font");

            let (r, g, b) = parse_hex(&t.color);

            // node.x is an alignment anchor; compute the true left edge.
            let text_w  = font.text_width(&t.content, t.size);
            let draw_x  = match t.text_align.as_str() {
                "right"  => t.x - text_w,
                "center" => t.x - text_w / 2.0,
                _        => t.x,   // "left" — the anchor already is the left edge
            };

            // Layout y = top of the text block; PDF baseline = top − font-size.
            let pdf_y   = page_h - t.y - t.size;
            let encoded = font.encode_text(&t.content);
            let rname   = font.resource_name.as_bytes().to_vec();

            content.begin_text();
            content.set_fill_rgb(r, g, b);
            content.set_font(Name(&rname), t.size);
            // set_text_matrix positions the text origin precisely.
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, draw_x, pdf_y]);
            content.show(Str(&encoded));
            content.end_text();
        }

        // ── Link ─────────────────────────────────────────────────────────────
        RenderNode::Link(lk) => {
            // Draw child nodes first (they appear inside the link area).
            draw_nodes(content, annots, &lk.children, fonts, page_h);

            // Collect the annotation rect in bottom-up PDF coordinates.
            // The annotation PDF object will be written during assembly.
            let y_bottom = page_h - lk.y - lk.height;
            let y_top    = page_h - lk.y;
            annots.push(AnnotData {
                x1:  lk.x,
                y1:  y_bottom,
                x2:  lk.x + lk.width,
                y2:  y_top,
                url: lk.url.clone(),
            });
        }
    }
}

// ── ToUnicode CMap builder ────────────────────────────────────────────────────

/// Build the raw bytes of a `/ToUnicode` CMap stream for a TrueType font that
/// uses Identity-H encoding.  `glyph_unicode` maps glyph ID (u16) → Unicode
/// code point (u32), sorted by glyph ID.
///
/// The stream allows PDF viewers to extract text from the document (copy/paste,
/// accessibility) even though the content bytes are glyph IDs rather than
/// Unicode codepoints.
fn build_to_unicode_cmap(
    font_name:     &str,
    glyph_unicode: &[(u16, u32)],
) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("/CIDInit /ProcSet findresource begin\n");
    out.push_str("12 dict begin\n");
    out.push_str("begincmap\n");
    out.push_str("/CIDSystemInfo\n");
    out.push_str("<< /Registry (Adobe)\n");
    out.push_str("   /Ordering (UCS)\n");
    out.push_str("   /Supplement 0 >> def\n");
    out.push_str(&format!("/CMapName /{font_name}-UCS def\n"));
    out.push_str("/CMapType 2 def\n");
    out.push_str("1 begincodespacerange\n");
    out.push_str("<0000> <FFFF>\n");
    out.push_str("endcodespacerange\n");

    // Emit in chunks of 100 (PDF spec recommendation).
    for chunk in glyph_unicode.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, cp) in chunk {
            if *cp <= 0xFFFF {
                out.push_str(&format!("<{gid:04X}> <{cp:04X}>\n"));
            } else {
                // Encode supplementary planes as UTF-16BE surrogate pairs.
                let offset = cp - 0x10000;
                let high = 0xD800 + (offset >> 10) as u32;
                let low  = 0xDC00 + (offset & 0x3FF) as u32;
                out.push_str(&format!("<{gid:04X}> <{high:04X}{low:04X}>\n"));
            }
        }
        out.push_str("endbfchar\n");
    }

    out.push_str("endcmap\n");
    out.push_str("CMapName currentdict /CMap defineresource pop\n");
    out.push_str("end\nend\n");
    out.into_bytes()
}

// ── ID allocator ──────────────────────────────────────────────────────────────

/// Simple monotonically increasing PDF indirect-object ID counter.
struct Alloc(i32);

impl Alloc {
    fn next(&mut self) -> Ref {
        self.0 += 1;
        Ref::new(self.0)
    }
}

// ── Main render function ──────────────────────────────────────────────────────

/// Convert a fully laid-out document into a binary PDF.
///
/// # Parameters
/// - `pages`      – Layout output: one `RenderPage` per document page.
/// - `font_defs`  – Font name → definition from the document's `<fonts>` section.
/// - `registry`   – Raw font bytes for custom fonts (populated via `load_font`).
/// - `meta`       – Document metadata (title, author, subject, etc.).
/// - `watermark`  – Optional `(text, url)`.  Drawn top-right at 8 pt Helvetica,
///                  light grey (`#aaaaaa`), 4 pt from the page edge.
/// - `created_on` – Optional ISO 8601 date string written to `/CreationDate`.
pub fn render_pdf(
    pages:      &[RenderPage],
    font_defs:  &HashMap<String, FontDef>,
    registry:   &FontRegistry,
    meta:       &Meta,
    watermark:  Option<(&str, Option<&str>)>,
    created_on: Option<&str>,
) -> Result<Vec<u8>, String> {

    // ── Step 1: Resolve all font definitions ─────────────────────────────────
    //
    // Walk the entire document to find every font name referenced by text
    // nodes.  Helvetica is always included so the watermark always has a font.

    let mut used_font_names: HashSet<String> = HashSet::new();
    for page in pages {
        collect_used_fonts(&page.nodes, &mut used_font_names);
    }
    used_font_names.insert("Helvetica".to_string());

    let mut fonts: HashMap<String, PreparedFont> = HashMap::new();
    for (idx, name) in used_font_names.iter().enumerate() {
        let resource_name = format!("F{idx}");
        let kind          = resolve_font_kind(name, font_defs, registry);
        fonts.insert(name.clone(), PreparedFont { resource_name, kind });
    }

    // ── Step 2: Build page content streams + collect annotations ─────────────
    //
    // Each page is processed independently.  The resulting content stream
    // bytes and any link annotations are stored for later object writing.

    let mut rendered_pages: Vec<(Vec<u8>, Vec<AnnotData>)> = Vec::new();

    for page in pages {
        let mut content = Content::new();
        let mut annots: Vec<AnnotData> = Vec::new();

        // Draw optional page background before any nodes.
        if let Some(bg) = &page.background {
            let (r, g, b) = parse_hex(bg);
            content.set_fill_rgb(r, g, b);
            content.rect(0.0, 0.0, page.width, page.height);
            content.fill_nonzero();
        }

        // Draw all render-tree nodes.
        draw_nodes(&mut content, &mut annots, &page.nodes, &fonts, page.height);

        // Draw optional watermark — top-right corner, 8 pt Helvetica, grey.
        if let Some((wtext, wurl)) = watermark {
            let wfont   = fonts.get("Helvetica")
                .expect("Helvetica always present after Step 1");
            let wsize   = 8.0_f32;
            let wpad    = 4.0_f32;  // distance from page edge (independent of margin)
            let tw      = wfont.text_width(wtext, wsize);
            let wx      = page.width - wpad - tw;
            // In PDF bottom-up coords: baseline = height - 4 (pad) - 8 (size)
            let pdf_wy  = page.height - wpad - wsize;

            let encoded = wfont.encode_text(wtext);
            let rname   = wfont.resource_name.as_bytes().to_vec();
            let (wr, wg, wb) = parse_hex("#aaaaaa");

            content.begin_text();
            content.set_fill_rgb(wr, wg, wb);
            content.set_font(Name(&rname), wsize);
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, wx, pdf_wy]);
            content.show(Str(&encoded));
            content.end_text();

            if let Some(url) = wurl {
                annots.push(AnnotData {
                    x1:  wx,
                    y1:  pdf_wy,
                    x2:  wx + tw,
                    y2:  pdf_wy + wsize,
                    url: url.to_string(),
                });
            }
        }

        rendered_pages.push((content.finish(), annots));
    }

    // ── Step 3: Pre-allocate all PDF object IDs ───────────────────────────────
    //
    // Every indirect object in the file needs a unique integer ID (starting at
    // 1).  We allocate them all upfront so that forward references (e.g. a
    // page dict referencing its content stream) are known before any writing.

    let n = pages.len();
    let mut alloc = Alloc(0);

    let catalog_id   = alloc.next();  // 1  — document catalog
    let info_id      = alloc.next();  // 2  — document information dictionary
    let pages_id     = alloc.next();  // 3  — page tree root

    // One page dict + one content stream per page.
    let page_ids:    Vec<Ref> = (0..n).map(|_| alloc.next()).collect();
    let content_ids: Vec<Ref> = (0..n).map(|_| alloc.next()).collect();

    // One annotation object per link annotation, grouped by page.
    let annot_ids: Vec<Vec<Ref>> = rendered_pages
        .iter()
        .map(|(_, annots)| (0..annots.len()).map(|_| alloc.next()).collect())
        .collect();

    // Font object IDs:
    //   Builtin  → 1 object  (Type1 font dict)
    //   Truetype → 5 objects (font-program stream, font descriptor, CID font,
    //                         ToUnicode CMap stream, Type0 wrapper dict)
    struct FontIds {
        /// The ref placed in page /Font resource dictionaries.
        font_dict_id: Ref,
        /// Additional objects: [program, descriptor, cid_font, cmap] for TTF.
        extra_ids:    Vec<Ref>,
    }

    let mut font_id_map: HashMap<String, FontIds> = HashMap::new();
    for (name, font) in &fonts {
        let font_dict_id = alloc.next();
        let extra_ids = match &font.kind {
            PreparedFontKind::Builtin { .. }  => vec![],
            PreparedFontKind::Truetype { .. } => (0..4).map(|_| alloc.next()).collect(),
        };
        font_id_map.insert(name.clone(), FontIds { font_dict_id, extra_ids });
    }

    // ── Step 4: Write all PDF objects ─────────────────────────────────────────

    let mut pdf = Pdf::new();

    // -- Catalog ---------------------------------------------------------------
    pdf.catalog(catalog_id).pages(pages_id);

    // -- Document Information --------------------------------------------------
    // Metadata visible in PDF reader "Properties" dialogs.
    {
        let mut info = pdf.document_info(info_id);
        if !meta.title.is_empty()    { info.title(TextStr(&meta.title)); }
        if !meta.author.is_empty()   { info.author(TextStr(&meta.author)); }
        if !meta.subject.is_empty()  { info.subject(TextStr(&meta.subject)); }
        if !meta.creator.is_empty()  { info.creator(TextStr(&meta.creator)); }
        if !meta.keywords.is_empty() { info.keywords(TextStr(&meta.keywords)); }
        info.producer(TextStr("lpdf.io"));

        // created_on: write as a raw PDF date string if provided.
        // Format expected: ISO 8601 "YYYY-MM-DDTHH:mm:ss" → "D:YYYYMMDDHHmmss"
        if let Some(dt) = created_on {
            let clean: String = dt.chars().filter(|c| c.is_ascii_digit()).collect();
            if clean.len() >= 8 {
                let pdf_date = format!("D:{clean}");
                // DocumentInfo derefs to Dict; use raw pair insertion for
                // keys not covered by the typed API (like CreationDate).
                info.pair(Name(b"CreationDate"), Str(pdf_date.as_bytes()));
            }
        }
    }

    // -- Pages tree ------------------------------------------------------------
    // A flat page tree with all pages as direct children.
    {
        let mut tree = pdf.pages(pages_id);
        tree.kids(page_ids.iter().copied()).count(n as i32);
    }

    // -- Per-page dicts --------------------------------------------------------
    // We write the page dictionary before its content stream so that content
    // stream IDs can be referenced in the page's /Contents entry.
    for (i, page) in pages.iter().enumerate() {
        let page_id    = page_ids[i];
        let content_id = content_ids[i];

        let mut pw = pdf.page(page_id);
        pw.parent(pages_id)
          .media_box(Rect::new(0.0, 0.0, page.width, page.height));

        // List all fonts used anywhere in this document in every page's
        // resource dict.  (Over-inclusion is harmless and avoids a second
        // per-page scan.)
        {
            let mut resources = pw.resources();
            let mut font_res  = resources.fonts();
            for (name, font) in &fonts {
                let fid = font_id_map[name].font_dict_id;
                font_res.pair(Name(font.resource_name.as_bytes()), fid);
            }
        }

        // Add the content stream reference.
        pw.contents(content_id);

        // Add the per-page annotation array if there are any link annotations.
        let page_annot_ids = &annot_ids[i];
        if !page_annot_ids.is_empty() {
            // `annotations` takes the iterator of Refs directly.
            pw.annotations(page_annot_ids.iter().copied());
        }
    }

    // -- Content streams -------------------------------------------------------
    for (i, (content_bytes, _)) in rendered_pages.iter().enumerate() {
        pdf.stream(content_ids[i], content_bytes);
    }

    // -- Link annotation objects -----------------------------------------------
    // Each link annotation is a separate indirect object.  Invisible border
    // (0-width) so only the URI action fires; no visual box is drawn.
    for (i, (_, page_annots)) in rendered_pages.iter().enumerate() {
        for (j, ann) in page_annots.iter().enumerate() {
            let aid      = annot_ids[i][j];
            let url_bytes = ann.url.as_bytes().to_vec();
            let mut aw   = pdf.annotation(aid);
            aw.subtype(AnnotationType::Link)
              .rect(Rect::new(ann.x1, ann.y1, ann.x2, ann.y2))
              .border(0.0, 0.0, 0.0, None);
            aw.action()
              .action_type(ActionType::Uri)
              .uri(Str(&url_bytes));
        }
    }

    // -- Font objects ----------------------------------------------------------
    for (name, font) in &fonts {
        let ids = &font_id_map[name];

        match &font.kind {
            PreparedFontKind::Builtin { base_name } => {
                // Type 1 (built-in resident) font.  Just a font dictionary with
                // a /BaseFont entry; no font program stream is needed.
                pdf.type1_font(ids.font_dict_id)
                   .base_font(Name(base_name.as_bytes()))
                   .encoding_predefined(Name(b"WinAnsiEncoding"));
            }

            PreparedFontKind::Truetype { bytes } => {
                // Five objects are needed for a proper TrueType composite font:
                //   [0] Font program stream (raw TrueType bytes)
                //   [1] Font descriptor
                //   [2] CID font dictionary (the descendant font)
                //   [3] ToUnicode CMap stream
                //   [font_dict_id] Type0 composite font wrapper

                let prog_id  = ids.extra_ids[0];
                let desc_id  = ids.extra_ids[1];
                let cid_id   = ids.extra_ids[2];
                let cmap_id  = ids.extra_ids[3];

                // Collect every character used with this font across all pages.
                let mut used_chars: HashSet<char> = HashSet::new();
                for page in pages {
                    collect_chars_for_font(&page.nodes, name, &mut used_chars);
                }
                if name == "Helvetica" {
                    if let Some((wtext, _)) = watermark {
                        used_chars.extend(wtext.chars());
                    }
                }

                // Parse the font to extract metrics.
                let face = Face::parse(bytes, 0)
                    .map_err(|e| format!("Failed to parse font '{name}': {e:?}"))?;
                let upem = face.units_per_em() as f32;

                // Build glyph ID → Unicode mapping (sorted by glyph ID).
                let mut glyph_unicode: Vec<(u16, u32)> = used_chars.iter()
                    .filter_map(|&c| face.glyph_index(c).map(|g| (g.0, c as u32)))
                    .collect();
                glyph_unicode.sort_by_key(|(gid, _)| *gid);

                // Build glyph ID → width in PDF per-mille units (1000 = 1 em).
                let glyph_widths: Vec<(u16, f32)> = glyph_unicode.iter()
                    .filter_map(|(gid, _)| {
                        let ttf_gid = ttf_parser::GlyphId(*gid);
                        face.glyph_hor_advance(ttf_gid)
                            .map(|adv| (*gid, adv as f32 / upem * 1000.0))
                    })
                    .collect();

                // [0] Font program (raw TTF bytes, uncompressed for simplicity).
                pdf.stream(prog_id, bytes);

                // [1] Font descriptor — describes the font's metrics and links
                //     to the embedded font program.
                let bbox   = face.global_bounding_box();
                let ascent = face.ascender()  as f32 / upem * 1000.0;
                let desc   = face.descender() as f32 / upem * 1000.0;
                let cap_h  = face.capital_height()
                    .map(|h| h as f32 / upem * 1000.0)
                    .unwrap_or(ascent * 0.7);
                let fname  = name.replace(' ', "-");

                pdf.font_descriptor(desc_id)
                   .name(Name(fname.as_bytes()))
                   .flags(FontFlags::NON_SYMBOLIC)
                   .bbox(Rect::new(
                       bbox.x_min as f32 / upem * 1000.0,
                       bbox.y_min as f32 / upem * 1000.0,
                       bbox.x_max as f32 / upem * 1000.0,
                       bbox.y_max as f32 / upem * 1000.0,
                   ))
                   .italic_angle(0.0)
                   .ascent(ascent)
                   .descent(desc)
                   .cap_height(cap_h)
                   .stem_v(80.0)
                   .font_file2(prog_id);  // /FontFile2 = TrueType

                // [2] CID font dictionary (TrueType descendant).
                {
                    // Adobe-Identity characterises CID fonts using Identity-H.
                    let sysinfo = pdf_writer::types::SystemInfo {
                        registry:   Str(b"Adobe"),
                        ordering:   Str(b"Identity"),
                        supplement: 0,
                    };
                    let mut cid = pdf.cid_font(cid_id);
                    cid.subtype(CidFontType::Type2)
                       .base_font(Name(fname.as_bytes()))
                       .system_info(sysinfo)
                       .font_descriptor(desc_id)
                       .default_width(1000.0);

                    // Per-glyph widths so the PDF renderer places characters
                    // with correct spacing.  Written as individual consecutive
                    // ranges of length 1 for simplicity.
                    if !glyph_widths.is_empty() {
                        let mut w = cid.widths();
                        for (gid, width) in &glyph_widths {
                            w.consecutive(*gid, [*width]);
                        }
                    }
                }

                // [3] ToUnicode CMap stream — enables text extraction.
                let cmap_bytes = build_to_unicode_cmap(&fname, &glyph_unicode);
                pdf.stream(cmap_id, &cmap_bytes);

                // [font_dict_id] Type0 composite font wrapper.
                pdf.type0_font(ids.font_dict_id)
                   .base_font(Name(fname.as_bytes()))
                   .encoding_predefined(Name(b"Identity-H"))
                   .descendant_font(cid_id)
                   .to_unicode(cmap_id);
            }
        }
    }

    Ok(pdf.finish())
}
