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
use std::ops::Range;

use pdf_writer::{Content, Filter, Name, Pdf, Rect, Ref, Str, TextStr};
use pdf_writer::writers::Resources;
use pdf_writer::types::{ActionType, AnnotationType, CheckBoxState, CidFontType, FieldFlags, FieldType, FontFlags, HighlightEffect, LineCapStyle, LineJoinStyle, PageMode, Predictor, TextRenderingMode};
use ttf_parser::Face;
use miniz_oxide::deflate::compress_to_vec_zlib;
use miniz_oxide::inflate::decompress_to_vec_zlib;
use subsetter::GlyphRemapper;

use crate::render::{placed_anchors, PlacedAnchor, RenderBox, RenderCanvasLayer, RenderField, RenderNode, RenderPage, RenderedBarcodeKind};
use crate::parse::{FieldKind, Meta};
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

    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.bytes.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }
}

/// Stores raw image bytes (JPEG/PNG) for images referenced by a document via
/// its `<assets><images>` declarations.  Populate before calling `render_pdf`.
pub struct ImageRegistry {
    bytes: HashMap<String, Vec<u8>>,
}

impl ImageRegistry {
    pub fn new() -> Self {
        Self { bytes: HashMap::new() }
    }

    /// Associate `name` with raw image file bytes.
    pub fn load(&mut self, name: &str, data: Vec<u8>) {
        self.bytes.insert(name.to_string(), data);
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.bytes.get(name).map(|b| b.as_slice())
    }

    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.bytes.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }
}

// ── Image helpers ─────────────────────────────────────────────────────────────

/// Read the pixel dimensions of a JPEG or PNG image.
/// Returns `(width_px, height_px)` or `None` if unrecognised format.
pub fn image_natural_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        jpeg_dims(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        png_dims(bytes)
    } else {
        None
    }
}

/// Build an `ImageMeta` map (name → (w_px, h_px)) from a registry.
pub fn build_image_meta(registry: &ImageRegistry) -> crate::layout::ImageMeta {
    let mut meta = crate::layout::ImageMeta::new();
    for (name, bytes) in &registry.bytes {
        if let Some(dims) = image_natural_size(bytes) {
            meta.insert(name.clone(), dims);
        }
    }
    meta
}

fn jpeg_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2usize;
    while i + 3 < bytes.len() {
        if bytes[i] != 0xFF { break; }
        let marker = bytes[i + 1];
        if i + 3 >= bytes.len() { break; }
        let seg_len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        // SOF0=0xC0, SOF1=0xC1, SOF2=0xC2
        if matches!(marker, 0xC0 | 0xC1 | 0xC2) && i + 8 < bytes.len() {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + seg_len;
    }
    None
}

/// PNG bit depth + color type → (components, is_supported)
fn png_info(bytes: &[u8]) -> Option<(u32, u32, u8, u8)> {
    // PNG IHDR: 8-byte sig + 4-byte length + 4-byte "IHDR" + 4-byte W + 4-byte H
    //           + 1-byte bit-depth + 1-byte color-type
    if bytes.len() < 26 { return None; }
    let w  = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h  = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    let bd = bytes[24]; // bit depth
    let ct = bytes[25]; // color type: 0=gray, 2=RGB, 3=palette, 4=gray+A, 6=RGBA
    Some((w, h, bd, ct))
}

fn png_dims(bytes: &[u8]) -> Option<(u32, u32)> {
    let (w, h, _, _) = png_info(bytes)?;
    Some((w, h))
}

/// Collect IDAT payload bytes from a PNG file (all chunks concatenated).
fn png_idat_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pos = 8usize; // skip 8-byte PNG signature
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([
            bytes[pos], bytes[pos+1], bytes[pos+2], bytes[pos+3],
        ]) as usize;
        let tag = &bytes[pos+4..pos+8];
        if tag == b"IDAT" && pos + 8 + len <= bytes.len() {
            out.extend_from_slice(&bytes[pos+8..pos+8+len]);
        }
        pos += 12 + len; // length(4) + tag(4) + data(len) + crc(4)
    }
    out
}

// ── PNG RGBA decoding ─────────────────────────────────────────────────────────

/// Paeth predictor function used by PNG filter type 4.
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i32;
    let b = b as i32;
    let c = c as i32;
    let p  = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc { a as u8 }
    else if pb <= pc { b as u8 }
    else { c as u8 }
}

/// Reconstruct (unfilter) raw PNG scanline data.
/// `channels` is bytes per pixel (e.g. 4 for RGBA, 3 for RGB).
/// Returns the flat defiltered pixel buffer, or `None` on malformed input.
fn unfilter_png(width: u32, height: u32, channels: usize, raw: &[u8]) -> Option<Vec<u8>> {
    let stride  = width as usize * channels;
    let row_len = stride + 1; // +1 for the leading filter byte
    if raw.len() < row_len * height as usize {
        return None;
    }
    let mut out  = vec![0u8; stride * height as usize];
    let mut prev = vec![0u8; stride];
    for y in 0..height as usize {
        let filter = raw[y * row_len];
        let src    = &raw[y * row_len + 1..y * row_len + 1 + stride];
        let dst    = &mut out[y * stride..(y + 1) * stride];
        match filter {
            0 => dst.copy_from_slice(src),
            1 => {
                for i in 0..stride {
                    let a = if i >= channels { dst[i - channels] } else { 0 };
                    dst[i] = src[i].wrapping_add(a);
                }
            }
            2 => {
                for i in 0..stride { dst[i] = src[i].wrapping_add(prev[i]); }
            }
            3 => {
                for i in 0..stride {
                    let a = if i >= channels { dst[i - channels] as u16 } else { 0 };
                    let b = prev[i] as u16;
                    dst[i] = src[i].wrapping_add(((a + b) / 2) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let a = if i >= channels { dst[i - channels] } else { 0 };
                    let b = prev[i];
                    let c = if i >= channels { prev[i - channels] } else { 0 };
                    dst[i] = src[i].wrapping_add(paeth_predictor(a, b, c));
                }
            }
            _ => return None,
        }
        prev.copy_from_slice(dst);
    }
    Some(out)
}

/// Decode an RGBA PNG (8-bit or 16-bit) into separate zlib-compressed RGB and
/// alpha buffers suitable for embedding as a PDF image XObject + SMask.
///
/// 16-bit channels are downsampled to 8-bit by taking the high byte.
///
/// Returns `(width, height, rgb_zlib, alpha_zlib)` or `None` on failure.
fn decode_rgba_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>, Vec<u8>)> {
    let (w, h, bd, ct) = png_info(bytes)?;
    if ct != 6 { return None; }
    if bd != 8 && bd != 16 { return None; }
    let bpc     = (bd / 8) as usize; // bytes per channel (1 or 2)
    let idat    = png_idat_bytes(bytes);
    let raw     = decompress_to_vec_zlib(&idat).ok()?;
    let pixels  = unfilter_png(w, h, 4 * bpc, &raw)?;
    let n       = (w * h) as usize;
    let mut rgb   = Vec::with_capacity(n * 3);
    let mut alpha = Vec::with_capacity(n);
    if bpc == 1 {
        for px in pixels.chunks_exact(4) {
            rgb.push(px[0]);
            rgb.push(px[1]);
            rgb.push(px[2]);
            alpha.push(px[3]);
        }
    } else {
        // 16-bit big-endian per channel; take the high byte (downsample to 8-bit)
        for px in pixels.chunks_exact(8) {
            rgb.push(px[0]);
            rgb.push(px[2]);
            rgb.push(px[4]);
            alpha.push(px[6]);
        }
    }
    let level   = 6u8;
    let rgb_z   = compress_to_vec_zlib(&rgb,   level);
    let alpha_z = compress_to_vec_zlib(&alpha, level);
    Some((w, h, rgb_z, alpha_z))
}

/// Return the data bytes of the first PNG chunk with the given 4-byte tag,
/// or `None` if the chunk is absent or the file is truncated.
fn png_chunk<'a>(bytes: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let mut pos = 8usize; // skip 8-byte PNG signature
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([
            bytes[pos], bytes[pos+1], bytes[pos+2], bytes[pos+3],
        ]) as usize;
        if &bytes[pos+4..pos+8] == tag {
            if pos + 8 + len <= bytes.len() {
                return Some(&bytes[pos+8..pos+8+len]);
            }
            return None; // truncated chunk
        }
        pos += 12 + len;
    }
    None
}

/// Decode an 8-bit indexed-color PNG to raw RGB pixels with an optional alpha
/// channel.  Returns `(width, height, rgb_pixels, alpha_pixels_or_none)`.
///
/// `alpha_pixels` is `Some(…)` only when the image has a `tRNS` chunk that
/// contains at least one non-opaque (< 255) entry.
fn decode_indexed_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>, Option<Vec<u8>>)> {
    let (w, h, bd, ct) = png_info(bytes)?;
    if ct != 3 || bd != 8 { return None; }

    // Read palette (PLTE chunk: N × RGB triplets, N ≤ 256)
    let plte_data = png_chunk(bytes, b"PLTE")?;
    if plte_data.len() % 3 != 0 { return None; }
    let palette: Vec<[u8; 3]> = plte_data.chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    // Optional per-index alpha values (tRNS chunk)
    let trns: &[u8] = png_chunk(bytes, b"tRNS").unwrap_or(&[]);
    let has_alpha   = trns.iter().any(|&a| a != 255);

    // Decode and unfilter the IDAT stream (1 byte per pixel for 8-bit indexed)
    let idat    = png_idat_bytes(bytes);
    let raw     = decompress_to_vec_zlib(&idat).ok()?;
    let indices = unfilter_png(w, h, 1, &raw)?;

    let n = (w * h) as usize;
    let mut rgb   = Vec::with_capacity(n * 3);
    let mut alpha = if has_alpha { Some(Vec::with_capacity(n)) } else { None };

    for &idx in &indices {
        let i     = idx as usize;
        let color = if i < palette.len() { palette[i] } else { [0, 0, 0] };
        rgb.push(color[0]);
        rgb.push(color[1]);
        rgb.push(color[2]);
        if let Some(ref mut a_buf) = alpha {
            a_buf.push(if i < trns.len() { trns[i] } else { 255 });
        }
    }

    Some((w, h, rgb, alpha))
}

/// Collect all unique image names referenced by the render tree.
fn collect_used_images(nodes: &[RenderNode], out: &mut HashSet<String>) {
    for node in nodes {
        match node {
            RenderNode::Image(img)    => { out.insert(img.name.clone()); }
            RenderNode::Box(b)        => collect_used_images(&b.children, out),
            RenderNode::Link(l)       => collect_used_images(&l.children, out),
            RenderNode::CanvasLayer(l) => collect_used_images(&l.children, out),
            _                          => {}
        }
    }
}

/// Returns `true` if the image format and subtype can be embedded by
/// `embed_image_xobject`.  Images that fail this check are silently excluded
/// from the PDF resource dictionary so no dangling XObject references occur.
pub fn is_image_embeddable(bytes: &[u8]) -> bool {
    image_format_error(bytes).is_none()
}

/// Returns a human-readable error string if `bytes` cannot be embedded,
/// or `None` if the format is supported.
pub fn image_format_error(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return if jpeg_dims(bytes).is_some() {
            None
        } else {
            Some("JPEG file could not be parsed (no valid SOF marker found)".to_string())
        };
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        return match png_info(bytes) {
            None => Some("PNG file could not be parsed (IHDR chunk missing or truncated)".to_string()),
            Some((_, _, bd, ct)) => match (ct, bd) {
                (0 | 2, 8 | 16) => None, // grayscale / RGB — 8-bit and 16-bit
                (6, 8 | 16)     => None, // RGBA — 8-bit and 16-bit
                (3, 8)          => None, // indexed-color 8-bit
                (0 | 2 | 6, bd) => Some(format!(
                    "PNG with {bd}-bit depth is not supported"
                )),
                (3, bd) => Some(format!(
                    "PNG indexed-color with {bd}-bit depth is not supported; \
                     use 8-bit depth or convert to RGB"
                )),
                (4, _) => Some(
                    "PNG grayscale+alpha images are not supported; \
                     convert to RGBA".to_string()
                ),
                (ct, _) => Some(format!("PNG color type {ct} is not supported")),
            },
        };
    }
    // Detect common unsupported formats to give a better message.
    let hint = if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "WebP"
    } else if bytes.starts_with(b"GIF8") {
        "GIF"
    } else if bytes.starts_with(b"\x00\x00\x00") && bytes.get(4..8) == Some(b"ftyp") {
        "AVIF/HEIC"
    } else if bytes.starts_with(b"BM") {
        "BMP"
    } else if bytes.starts_with(b"II\x2A\x00") || bytes.starts_with(b"MM\x00\x2A") {
        "TIFF"
    } else {
        "unknown"
    };
    Some(format!(
        "{hint} images are not supported; use JPEG or PNG"
    ))
}

/// Write one image XObject into the PDF.  Supports:
///   - JPEG                      → `/Filter /DCTDecode`
///   - PNG grayscale/RGB 8 or 16-bit (ct 0, 2)  → IDAT passthrough with FlateDecode + PNG predictor
///   - PNG RGBA 8 or 16-bit (ct 6)               → decoded, split into RGB XObject + alpha SMask
///   - PNG indexed 8-bit (ct 3)                  → palette-expanded to RGB, or RGBA+SMask if tRNS present
fn embed_image_xobject(pdf: &mut Pdf, id: Ref, smask_id: Option<Ref>, bytes: &[u8]) {
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        // JPEG
        if let Some((w, h)) = jpeg_dims(bytes) {
            let mut img = pdf.image_xobject(id, bytes);
            img.width(w as i32)
               .height(h as i32)
               .bits_per_component(8);
            img.color_space().device_rgb();
            img.filter(Filter::DctDecode);
        }
    } else if bytes.starts_with(b"\x89PNG\r\n\x1A\n") {
        // PNG passthrough: embed raw IDAT (zlib stream) with FlateDecode + PNG predictor
        if let Some((w, h, bd, ct)) = png_info(bytes) {
            match ct {
                0 | 2 => {
                    // Grayscale or RGB — raw IDAT passthrough (no decompression needed).
                    let (colors, supported) = match ct {
                        0 => (1i32, true),
                        2 => (3i32, true),
                        _ => (3i32, false),
                    };
                    if supported {
                        let idat = png_idat_bytes(bytes);
                        let mut img = pdf.image_xobject(id, &idat);
                        img.width(w as i32)
                           .height(h as i32)
                           .bits_per_component(bd as i32);
                        match ct {
                            0 => { img.color_space().device_gray(); }
                            _ => { img.color_space().device_rgb(); }
                        }
                        img.filter(Filter::FlateDecode);
                        img.decode_parms()
                           .predictor(Predictor::PngOptimum)
                           .colors(colors)
                           .bits_per_component(bd as i32)
                           .columns(w as i32);
                    }
                }
                3 => {
                    // Indexed/palette — expand palette, embed as RGB or RGBA+SMask.
                    if let Some((dw, dh, rgb, alpha_opt)) = decode_indexed_png(bytes) {
                        let level = 6u8;
                        let rgb_z = compress_to_vec_zlib(&rgb, level);
                        if let Some(alpha) = alpha_opt {
                            let alpha_z = compress_to_vec_zlib(&alpha, level);
                            if let Some(smask_ref) = smask_id {
                                let mut mask = pdf.image_xobject(smask_ref, &alpha_z);
                                mask.width(dw as i32)
                                    .height(dh as i32)
                                    .bits_per_component(8);
                                mask.color_space().device_gray();
                                mask.filter(Filter::FlateDecode);
                            }
                            let mut img = pdf.image_xobject(id, &rgb_z);
                            img.width(dw as i32)
                               .height(dh as i32)
                               .bits_per_component(8);
                            img.color_space().device_rgb();
                            img.filter(Filter::FlateDecode);
                            if let Some(smask_ref) = smask_id {
                                img.s_mask(smask_ref);
                            }
                        } else {
                            let mut img = pdf.image_xobject(id, &rgb_z);
                            img.width(dw as i32)
                               .height(dh as i32)
                               .bits_per_component(8);
                            img.color_space().device_rgb();
                            img.filter(Filter::FlateDecode);
                        }
                    }
                }
                6 => {
                    // RGBA — decompress, split into RGB + alpha, embed with SMask.
                    if let Some((dw, dh, rgb_z, alpha_z)) = decode_rgba_png(bytes) {
                        // Write the alpha channel as a grayscale SMask XObject.
                        if let Some(smask_ref) = smask_id {
                            let mut mask = pdf.image_xobject(smask_ref, &alpha_z);
                            mask.width(dw as i32)
                                .height(dh as i32)
                                .bits_per_component(8);
                            mask.color_space().device_gray();
                            mask.filter(Filter::FlateDecode);
                        }
                        // Write the RGB image XObject, linking to the SMask.
                        let mut img = pdf.image_xobject(id, &rgb_z);
                        img.width(dw as i32)
                           .height(dh as i32)
                           .bits_per_component(8);
                        img.color_space().device_rgb();
                        img.filter(Filter::FlateDecode);
                        if let Some(smask_ref) = smask_id {
                            img.s_mask(smask_ref);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // Unsupported format: no object written for this ID (PDF ref unused).
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
    /// Custom TrueType/OpenType font, subsetted and embedded as CIDFont Type2
    /// with Identity-H encoding, giving full Unicode support.
    Truetype {
        /// Original full font bytes — used for descriptor metrics (ascender, etc.).
        original_bytes: Vec<u8>,
        /// Subsetted font bytes — what is actually written to the PDF stream.
        subsetted_bytes: Vec<u8>,
        /// Character → remapped glyph ID for content stream encoding.
        char_to_gid: HashMap<char, u16>,
        /// (remapped_gid, unicode_codepoint) sorted by GID — for ToUnicode CMap.
        glyph_unicode: Vec<(u16, u32)>,
        /// (remapped_gid, width_per_mille) sorted by GID — for /W array.
        glyph_widths: Vec<(u16, f32)>,
    },
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
            PreparedFontKind::Truetype { original_bytes, .. } => text_width_ttf(original_bytes, text, size),
        }
    }

    /// Encode `text` into the raw byte string required by the content stream.
    ///
    /// - Builtin (Type 1 + WinAnsiEncoding): one Latin-1 byte per character.
    /// - Truetype (CIDFont + Identity-H): two big-endian glyph-ID bytes per
    ///   character, resolved via `ttf-parser`.
    fn encode_text(&self, text: &str) -> Vec<u8> {
        match &self.kind {
            PreparedFontKind::Builtin { .. } => encode_latin1(text),
            PreparedFontKind::Truetype { char_to_gid, .. } => {
                let mut out = Vec::with_capacity(text.chars().count() * 2);
                for c in text.chars() {
                    let gid = char_to_gid.get(&c).copied().unwrap_or(0);
                    out.push((gid >> 8) as u8);
                    out.push((gid & 0xFF) as u8);
                }
                out
            }
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
            RenderNode::CanvasLayer(l) => collect_chars_for_font(&l.children, font_name, out),
            _                      => {}
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
            RenderNode::CanvasLayer(l) => collect_used_fonts(&l.children, out),
            RenderNode::Field(_) | _ => {}
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
    name:       &str,
    font_defs:  &HashMap<String, FontDef>,
    registry:   &FontRegistry,
    used_chars: &HashSet<char>,
) -> PreparedFontKind {
    if let Some(def) = font_defs.get(name) {
        match def {
            FontDef::Core(b) => {
                let ps = pdf_builtin_name(b).unwrap_or("Helvetica");
                PreparedFontKind::Builtin { base_name: ps }
            }
            FontDef::Ref(_) => {
                if let Some(bytes) = registry.get(name) {
                    return prepare_truetype_font(bytes, used_chars);
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

/// Subset a TrueType/OpenType font to only the glyphs used in the document,
/// and pre-build all per-glyph tables needed by the PDF writer.
///
/// On subsetting failure the full font is embedded as a fallback.
fn prepare_truetype_font(bytes: &[u8], used_chars: &HashSet<char>) -> PreparedFontKind {
    // Parse the font to map characters → original glyph IDs and extract metrics.
    let face = match Face::parse(bytes, 0) {
        Ok(f)  => f,
        Err(_) => {
            return PreparedFontKind::Truetype {
                original_bytes:  bytes.to_vec(),
                subsetted_bytes: bytes.to_vec(),
                char_to_gid:     HashMap::new(),
                glyph_unicode:   Vec::new(),
                glyph_widths:    Vec::new(),
            };
        }
    };
    let upem = face.units_per_em() as f32;

    // Map every used character to its original glyph ID.
    let mut char_gid_orig: Vec<(char, u16)> = used_chars.iter()
        .filter_map(|&c| face.glyph_index(c).map(|g| (c, g.0)))
        .collect();
    // Sort by codepoint so GlyphRemapper assigns new GIDs deterministically
    // regardless of HashSet iteration order (which is randomised per process).
    char_gid_orig.sort_by_key(|(c, _)| *c as u32);

    // Build a glyph remapper — registers each original GID and assigns new
    // consecutive IDs starting at 1 (.notdef stays at 0).
    let mut remapper = GlyphRemapper::new();
    for (_, orig_gid) in &char_gid_orig {
        remapper.remap(*orig_gid);
    }

    // Subset the font.  Fall back to full bytes on error.
    let (subsetted_bytes, use_remap) = match subsetter::subset(bytes, 0, &remapper) {
        Ok(sub) => (sub, true),
        Err(_)  => (bytes.to_vec(), false),
    };

    // Build char → (new) glyph ID map for content stream encoding.
    let char_to_gid: HashMap<char, u16> = char_gid_orig.iter()
        .filter_map(|(c, orig_gid)| {
            let new_gid = if use_remap {
                remapper.get(*orig_gid)?
            } else {
                *orig_gid
            };
            Some((*c, new_gid))
        })
        .collect();

    // Build glyph_unicode: (new_gid, unicode_codepoint), one entry per new GID.
    // Use a HashMap to deduplicate (multiple chars can share a glyph; keep first).
    let mut gid_to_unicode: HashMap<u16, u32> = HashMap::new();
    for (c, new_gid) in &char_to_gid {
        gid_to_unicode.entry(*new_gid).or_insert(*c as u32);
    }
    let mut glyph_unicode: Vec<(u16, u32)> = gid_to_unicode.into_iter().collect();
    glyph_unicode.sort_by_key(|(gid, _)| *gid);

    // Build glyph_widths: (new_gid, advance_per_mille), one entry per new GID.
    // Widths come from the original face (advances are the same after subsetting).
    let mut gid_to_width: HashMap<u16, f32> = HashMap::new();
    for (_, orig_gid) in &char_gid_orig {
        let new_gid = if use_remap {
            match remapper.get(*orig_gid) { Some(g) => g, None => continue }
        } else {
            *orig_gid
        };
        if !gid_to_width.contains_key(&new_gid) {
            if let Some(adv) = face.glyph_hor_advance(ttf_parser::GlyphId(*orig_gid)) {
                gid_to_width.insert(new_gid, adv as f32 / upem * 1000.0);
            }
        }
    }
    let mut glyph_widths: Vec<(u16, f32)> = gid_to_width.into_iter().collect();
    glyph_widths.sort_by_key(|(gid, _)| *gid);

    PreparedFontKind::Truetype {
        original_bytes: bytes.to_vec(),
        subsetted_bytes,
        char_to_gid,
        glyph_unicode,
        glyph_widths,
    }
}

// ── Content-stream builder ────────────────────────────────────────────────────

/// Draw HRT (human-readable text) for a barcode, centered below the bars.
fn draw_barcode_hrt(
    content:  &mut Content,
    fonts:    &HashMap<String, PreparedFont>,
    text:     &str,
    bar_x:    f32,
    bar_bottom_y: f32,  // top-down coordinate of the baseline zone top
    bar_w:    f32,
    page_h:   f32,
) {
    let font = match fonts.get("Helvetica") {
        Some(f) => f,
        None    => return,
    };
    let size   = 8.0_f32;
    let text_w = font.text_width(text, size);
    let draw_x = bar_x + (bar_w - text_w) / 2.0;
    // Place text 1pt below the bars.
    let pdf_y  = page_h - bar_bottom_y - size - 1.0;
    let encoded = font.encode_text(text);
    let rname   = font.resource_name.as_bytes().to_vec();
    content.begin_text();
    content.set_fill_rgb(0.0, 0.0, 0.0);
    content.set_font(Name(&rname), size);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, draw_x, pdf_y]);
    content.show(Str(&encoded));
    content.end_text();
}

/// Write all render nodes for one page into a `Content` stream and collect
/// any link annotations encountered along the way.
///
/// `page_h` is required for the top-down → bottom-up coordinate flip:
///   `pdf_y = page_h − layout_y − node_height`
fn draw_nodes(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    nodes:         &[RenderNode],
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    page_h:        f32,
) {
    for node in nodes {
        draw_node(content, annots, groups, node, fonts, image_res_map, page_h);
    }
}

fn draw_node(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    node:          &RenderNode,
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    page_h:        f32,
) {
    match node {
        // ── Box ──────────────────────────────────────────────────────────────
        RenderNode::Box(b) => {
            let percent = opacity_percent(b.opacity);
            if percent >= 100 {
                draw_box(content, annots, groups, b, fonts, image_res_map, page_h);
            } else {
                draw_faded(content, groups, percent, |content, groups| {
                    draw_box(content, annots, groups, b, fonts, image_res_map, page_h);
                });
            }
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
                // "left" and "justify" — anchor is already the left edge.
                _        => t.x,
            };

            // Compute word-spacing for justified lines.
            let word_spacing = if t.text_align == "justify" && t.line_width > 0.0 {
                let word_count = t.content.split_whitespace().count();
                if word_count > 1 {
                    let extra = t.line_width - text_w;
                    (extra / (word_count - 1) as f32).max(0.0)
                } else {
                    0.0
                }
            } else {
                0.0
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
            if word_spacing > 0.0 { content.set_word_spacing(word_spacing); }
            content.show(Str(&encoded));
            if word_spacing > 0.0 { content.set_word_spacing(0.0); }
            content.end_text();
        }

        // ── Link ─────────────────────────────────────────────────────────────
        RenderNode::Link(lk) => {
            // Draw child nodes first (they appear inside the link area).
            draw_nodes(content, annots, groups, &lk.children, fonts, image_res_map, page_h);

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

        // ── Image ────────────────────────────────────────────────────────────
        RenderNode::Image(img) => {
            if let Some(res_name) = image_res_map.get(&img.name) {
                // PDF images are painted with a CTM that scales a 1×1 unit
                // image to the desired width/height, positioned at (x, pdf_y).
                let pdf_y = page_h - img.y - img.height;
                content.save_state();
                content.transform([img.width, 0.0, 0.0, img.height, img.x, pdf_y]);
                let rname: Vec<u8> = res_name.bytes().collect();
                content.x_object(Name(&rname));
                content.restore_state();
            }
        }

        // ── Barcode ──────────────────────────────────────────────────────────
        RenderNode::Barcode(bc) => {
            content.save_state();

            // Optional background fill.
            if let Some(bg) = &bc.bg {
                let (r, g, b) = parse_hex(bg);
                let pdf_y = page_h - bc.y - bc.height;
                content.set_fill_rgb(r, g, b);
                content.rect(bc.x, pdf_y, bc.width, bc.height);
                content.fill_nonzero();
            }

            let (fr, fg, fb) = parse_hex(&bc.color);
            content.set_fill_rgb(fr, fg, fb);

            match &bc.kind {
                RenderedBarcodeKind::Qr { modules, size } => {
                    let sz = *size as f32;
                    let module_pt = bc.width / sz;
                    for row in 0..*size {
                        for col in 0..*size {
                            let idx = (row * size + col) as usize;
                            if modules[idx] {
                                // PDF y-axis is bottom-up; row 0 is top.
                                let mx = bc.x + col as f32 * module_pt;
                                let my = page_h - bc.y - (row + 1) as f32 * module_pt;
                                content.rect(mx, my, module_pt, module_pt);
                                content.fill_nonzero();
                            }
                        }
                    }
                }

                RenderedBarcodeKind::Code128 { bars, hrt } => {
                    let total_units: u32 = bars.iter().map(|&w| w as u32).sum();
                    if total_units > 0 {
                        let unit_w = bc.width / total_units as f32;
                        let bar_h  = if hrt.is_some() { bc.height - 12.0 } else { bc.height };
                        let pdf_y  = page_h - bc.y - bar_h;
                        let mut cur_x = bc.x;
                        for (i, &run) in bars.iter().enumerate() {
                            let run_w = run as f32 * unit_w;
                            if i % 2 == 0 {
                                // Even indices are bars.
                                content.rect(cur_x, pdf_y, run_w, bar_h);
                                content.fill_nonzero();
                            }
                            cur_x += run_w;
                        }
                        // HRT text is drawn as a simple centered annotation
                        // using the draw_barcode_hrt helper.
                        if let Some(text) = hrt {
                            draw_barcode_hrt(content, fonts, text, bc.x, bc.y + bar_h, bc.width, page_h);
                        }
                    }
                }

                RenderedBarcodeKind::Ean13 { bars, digits, hrt } => {
                    let total_units: u32 = bars.iter().map(|&w| w as u32).sum();
                    if total_units > 0 {
                        let unit_w = bc.width / total_units as f32;
                        // EAN-13 standard: guard bars extend 5 units below digit bars.
                        // For simplicity we use a uniform bar height here.
                        let hrt_h  = if *hrt { 12.0_f32 } else { 0.0 };
                        let bar_h  = bc.height - hrt_h;
                        let pdf_y  = page_h - bc.y - bar_h;
                        let mut cur_x = bc.x;
                        for (i, &run) in bars.iter().enumerate() {
                            let run_w = run as f32 * unit_w;
                            if i % 2 == 0 {
                                content.rect(cur_x, pdf_y, run_w, bar_h);
                                content.fill_nonzero();
                            }
                            cur_x += run_w;
                        }
                        if *hrt {
                            draw_barcode_hrt(content, fonts, digits, bc.x, bc.y + bar_h, bc.width, page_h);
                        }
                    }
                }
            }

            content.restore_state();
        }

        // ── Field ─────────────────────────────────────────────────────────────
        // Form fields are written as annotation objects during assembly,
        // not as content stream operations.  Nothing to draw here.
        RenderNode::Field(_) => {}

        // Anchors become the outline and the named destinations; they draw nothing.
        RenderNode::Anchor(_) => {}

        // ── Canvas Rect ───────────────────────────────────────────────────────
        RenderNode::CanvasRect(r) => {
            let has_fill   = r.fill.is_some();
            let has_stroke = r.stroke.is_some() && r.stroke_width > 0.0;
            if has_fill || has_stroke {
                content.save_state();
                if has_fill {
                    let (rf, gf, bf) = parse_hex(r.fill.as_deref().unwrap_or("#000000"));
                    content.set_fill_rgb(rf, gf, bf);
                }
                if has_stroke {
                    let (rs, gs, bs) = parse_hex(r.stroke.as_deref().unwrap_or("#000000"));
                    content.set_stroke_rgb(rs, gs, bs);
                    content.set_line_width(r.stroke_width);
                    if let Some(dash) = &r.stroke_dash {
                        content.set_dash_pattern(dash.iter().copied(), 0.0);
                    }
                }
                let pdf_y = page_h - r.y - r.h;
                if r.border_radius > 0.0 {
                    rounded_rect(content, r.x, pdf_y, r.w, r.h, r.border_radius);
                } else {
                    content.rect(r.x, pdf_y, r.w, r.h);
                }
                match (has_fill, has_stroke) {
                    (true,  true)  => { content.fill_nonzero_and_stroke(); }
                    (true,  false) => { content.fill_nonzero(); }
                    (false, true)  => { content.stroke(); }
                    (false, false) => { content.end_path(); }
                }
                content.restore_state();
            }
        }

        // ── Canvas Line ───────────────────────────────────────────────────────
        RenderNode::CanvasLine(l) => {
            let (r, g, b) = parse_hex(&l.stroke);
            content.save_state();
            content.set_stroke_rgb(r, g, b);
            content.set_line_width(l.stroke_width);
            if l.line_cap > 0 { content.set_line_cap(u8_to_line_cap(l.line_cap)); }
            if l.line_join > 0 { content.set_line_join(u8_to_line_join(l.line_join)); }
            if let Some(dash) = &l.stroke_dash {
                content.set_dash_pattern(dash.iter().copied(), 0.0);
            }
            content.move_to(l.x1, page_h - l.y1);
            content.line_to(l.x2, page_h - l.y2);
            content.stroke();
            content.restore_state();
        }

        // ── Canvas Ellipse ────────────────────────────────────────────────────
        RenderNode::CanvasEllipse(e) => {
            let has_fill   = e.fill.is_some();
            let has_stroke = e.stroke.is_some() && e.stroke_width > 0.0;
            if has_fill || has_stroke {
                content.save_state();
                if has_fill {
                    let (rf, gf, bf) = parse_hex(e.fill.as_deref().unwrap_or("#000000"));
                    content.set_fill_rgb(rf, gf, bf);
                }
                if has_stroke {
                    let (rs, gs, bs) = parse_hex(e.stroke.as_deref().unwrap_or("#000000"));
                    content.set_stroke_rgb(rs, gs, bs);
                    content.set_line_width(e.stroke_width);
                    if let Some(dash) = &e.stroke_dash {
                        content.set_dash_pattern(dash.iter().copied(), 0.0);
                    }
                }
                // Emit ellipse via 4 cubic-Bézier arcs (Kappa approximation).
                // PDF y is flipped.
                let cx = e.cx;
                let cy = page_h - e.cy;
                ellipse_path(content, cx, cy, e.rx, e.ry);
                match (has_fill, has_stroke) {
                    (true,  true)  => { content.fill_nonzero_and_stroke(); }
                    (true,  false) => { content.fill_nonzero(); }
                    (false, true)  => { content.stroke(); }
                    (false, false) => { content.end_path(); }
                }
                content.restore_state();
            }
        }

        // ── Canvas Path ───────────────────────────────────────────────────────
        RenderNode::CanvasPath(p) => {
            let has_fill   = p.fill.is_some();
            let has_stroke = p.stroke.is_some() && p.stroke_width > 0.0;
            if has_fill || has_stroke {
                content.save_state();
                if has_fill {
                    let (rf, gf, bf) = parse_hex(p.fill.as_deref().unwrap_or("#000000"));
                    content.set_fill_rgb(rf, gf, bf);
                }
                if has_stroke {
                    let (rs, gs, bs) = parse_hex(p.stroke.as_deref().unwrap_or("#000000"));
                    content.set_stroke_rgb(rs, gs, bs);
                    content.set_line_width(p.stroke_width);
                    if p.line_cap > 0 { content.set_line_cap(u8_to_line_cap(p.line_cap)); }
                    if p.line_join > 0 { content.set_line_join(u8_to_line_join(p.line_join)); }
                    if let Some(dash) = &p.stroke_dash {
                        content.set_dash_pattern(dash.iter().copied(), 0.0);
                    }
                }
                emit_svg_path(content, &p.d, page_h);
                match (has_fill, has_stroke, p.fill_rule_evenodd) {
                    (true,  true,  false) => { content.fill_nonzero_and_stroke(); }
                    (true,  true,  true)  => { content.fill_even_odd_and_stroke(); }
                    (true,  false, false) => { content.fill_nonzero(); }
                    (true,  false, true)  => { content.fill_even_odd(); }
                    (false, true,  _)     => { content.stroke(); }
                    (false, false, _)     => { content.end_path(); }
                }
                content.restore_state();
            }
        }

        // ── Canvas Layer ──────────────────────────────────────────────────────
        RenderNode::CanvasLayer(layer) => {
            let percent = opacity_percent(layer.opacity);
            if percent >= 100 {
                content.save_state();
                draw_layer(content, annots, groups, layer, fonts, image_res_map, page_h);
                content.restore_state();
            } else {
                draw_faded(content, groups, percent, |content, groups| {
                    draw_layer(content, annots, groups, layer, fonts, image_res_map, page_h);
                });
            }
        }
    }
}

/// A box: its fill and border, then its children.
fn draw_box(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    b:             &RenderBox,
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    page_h:        f32,
) {
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

    draw_nodes(content, annots, groups, &b.children, fonts, image_res_map, page_h);
}

/// What `draw` writes, faded to `percent` of its strength, which is below 100.
///
/// It is drawn into a transparency group of its own. The group is composited first and faded as one, so shapes
/// that overlap inside it do not show through each other. The group is a form XObject, written with the page
/// (`groups`); the page only draws it, under the opacity's ExtGState, which is pre-written during assembly and
/// referenced here by name. What `draw` fades can hold faded things in turn: their groups come first in
/// `groups`, and this group's resources name them (`PageGroup::nested`).
///
/// At 0% nothing shows. It can't be faded to nothing: no ExtGState is written for 0%, and a viewer ignores a
/// `gs` that names a missing one, which would draw the group at full strength.
fn draw_faded(
    content: &mut Content,
    groups:  &mut Vec<PageGroup>,
    percent: u32,
    draw:    impl FnOnce(&mut Content, &mut Vec<PageGroup>),
) {
    if percent == 0 {
        return;
    }
    let first_nested = groups.len();
    let mut group = Content::new();
    draw(&mut group, groups);
    let nested = first_nested..groups.len();
    let name = group_resource_name(groups.len());
    groups.push(PageGroup { content: group.finish(), nested });

    content.save_state();
    content.set_parameters(Name(opacity_gs_name(percent).as_bytes()));
    content.x_object(Name(name.as_bytes()));
    content.restore_state();
}

/// The inside of a canvas layer: its transform, its clip and its children. Its opacity is not applied here.
fn draw_layer(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    layer:         &RenderCanvasLayer,
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    page_h:        f32,
) {
    // Apply optional CTM transform.
    // The matrix [a b c d e f] is written for canvas space (y down, origin at the top
    // left), but the shapes inside the layer are already in PDF space (y up, origin at
    // the bottom left). A point is (X, Y) there and (X, H - Y) on the canvas, so
    // conjugating the canvas matrix with that flip gives the matrix the page needs:
    //   X' = a X - c Y + (c H + e)
    //   Y' = -b X + d Y + (H - d H - f)
    if let Some(ctm) = layer.transform {
        let [a, b, c, d, e, f] = ctm;
        content.transform([a, -b, -c, d, c * page_h + e, page_h - d * page_h - f]);
    }

    // Apply optional clip rect.
    if let Some(clip) = &layer.clip {
        let clip_pdf_y = page_h - clip.y - clip.h;
        if clip.border_radius > 0.0 {
            rounded_rect(content, clip.x, clip_pdf_y, clip.w, clip.h, clip.border_radius);
        } else {
            content.rect(clip.x, clip_pdf_y, clip.w, clip.h);
        }
        content.clip_nonzero();
        content.end_path();
    }

    draw_nodes(content, annots, groups, &layer.children, fonts, image_res_map, page_h);
}

/// The name a page's resources give its `index`th transparency group.
fn group_resource_name(index: usize) -> String {
    format!("G{index}")
}

// ── Form appearance helpers ───────────────────────────────────────────────────

/// A layer's opacity in whole percent, which is the precision the document keeps it to. 0 is invisible and 100 is
/// full strength, and only 1 to 99 are faded by an ExtGState.
fn opacity_percent(opacity: f32) -> u32 {
    (opacity * 100.0).round() as u32
}

/// Build an ExtGState resource name for an opacity in whole percent.
/// E.g. 50 → "GS50", 30 → "GS30".
fn opacity_gs_name(percent: u32) -> String {
    format!("GS{percent:02}")
}

/// Convert a canvas line-cap code (0=butt, 1=round, 2=square) to pdf_writer's enum.
fn u8_to_line_cap(v: u8) -> LineCapStyle {
    match v {
        1 => LineCapStyle::RoundCap,
        2 => LineCapStyle::ProjectingSquareCap,
        _ => LineCapStyle::ButtCap,
    }
}

/// Convert a canvas line-join code (0=miter, 1=round, 2=bevel) to pdf_writer's enum.
fn u8_to_line_join(v: u8) -> LineJoinStyle {
    match v {
        1 => LineJoinStyle::RoundJoin,
        2 => LineJoinStyle::BevelJoin,
        _ => LineJoinStyle::MiterJoin,
    }
}

/// Recursively collect all unique opacity values (as integer percentages 1..99)
/// from canvas layers and boxes. Opacity 1.0 (fully opaque) is excluded.
fn collect_used_opacities(nodes: &[RenderNode], out: &mut HashSet<u32>) {
    for node in nodes {
        match node {
            RenderNode::CanvasLayer(l) => {
                let pct = opacity_percent(l.opacity);
                if pct > 0 && pct < 100 { out.insert(pct); }
                collect_used_opacities(&l.children, out);
            }
            RenderNode::Box(b) => {
                let pct = opacity_percent(b.opacity);
                if pct > 0 && pct < 100 { out.insert(pct); }
                collect_used_opacities(&b.children, out);
            }
            RenderNode::Link(l) => collect_used_opacities(&l.children, out),
            _                   => {}
        }
    }
}

/// Append an ellipse path (4 cubic-Bézier arcs) in PDF bottom-up coords.
/// `cx`, `cy` are already in PDF (bottom-up) coordinates.
fn ellipse_path(content: &mut Content, cx: f32, cy: f32, rx: f32, ry: f32) {
    const KAPPA: f32 = 0.5523;
    let kx = rx * KAPPA;
    let ky = ry * KAPPA;
    content.move_to(cx + rx, cy);
    content.cubic_to(cx + rx, cy + ky,   cx + kx, cy + ry,   cx,      cy + ry);
    content.cubic_to(cx - kx, cy + ry,   cx - rx, cy + ky,   cx - rx, cy);
    content.cubic_to(cx - rx, cy - ky,   cx - kx, cy - ry,   cx,      cy - ry);
    content.cubic_to(cx + kx, cy - ry,   cx + rx, cy - ky,   cx + rx, cy);
    content.close_path();
}

/// Parse an SVG-like path string and emit PDF path operators.
/// Supports: M/m, L/l, H/h, V/v, C/c, Q/q, Z/z.
/// Y-coordinates are flipped (canvas top-down → PDF bottom-up).
fn emit_svg_path(content: &mut Content, d: &str, page_h: f32) {
    let flip = |y: f32| page_h - y;

    // Tokenise: split on whitespace and commas, preserve command letters.
    let tokens: Vec<&str> = d.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();

    let mut i     = 0;
    let mut cur_x = 0.0_f32;
    let mut cur_y = 0.0_f32;

    while i < tokens.len() {
        let cmd = tokens[i];
        i += 1;

        // Helper closures to consume the next f32 token.
        let next_f = |i: &mut usize| -> f32 {
            if *i < tokens.len() {
                let v = tokens[*i].parse::<f32>().unwrap_or(0.0);
                *i += 1;
                v
            } else {
                0.0
            }
        };

        match cmd {
            "M" => {
                let x = next_f(&mut i); let y = next_f(&mut i);
                content.move_to(x, flip(y));
                cur_x = x; cur_y = y;
                // Implicit subsequent L commands.
                while i < tokens.len() && tokens[i].parse::<f32>().is_ok() {
                    let lx = next_f(&mut i); let ly = next_f(&mut i);
                    content.line_to(lx, flip(ly));
                    cur_x = lx; cur_y = ly;
                }
            }
            "m" => {
                let dx = next_f(&mut i); let dy = next_f(&mut i);
                cur_x += dx; cur_y += dy;
                content.move_to(cur_x, flip(cur_y));
                while i < tokens.len() && tokens[i].parse::<f32>().is_ok() {
                    let dx2 = next_f(&mut i); let dy2 = next_f(&mut i);
                    cur_x += dx2; cur_y += dy2;
                    content.line_to(cur_x, flip(cur_y));
                }
            }
            "L" => {
                let x = next_f(&mut i); let y = next_f(&mut i);
                content.line_to(x, flip(y));
                cur_x = x; cur_y = y;
                while i < tokens.len() && tokens[i].parse::<f32>().is_ok() {
                    let lx = next_f(&mut i); let ly = next_f(&mut i);
                    content.line_to(lx, flip(ly));
                    cur_x = lx; cur_y = ly;
                }
            }
            "l" => {
                let dx = next_f(&mut i); let dy = next_f(&mut i);
                cur_x += dx; cur_y += dy;
                content.line_to(cur_x, flip(cur_y));
                while i < tokens.len() && tokens[i].parse::<f32>().is_ok() {
                    let dx2 = next_f(&mut i); let dy2 = next_f(&mut i);
                    cur_x += dx2; cur_y += dy2;
                    content.line_to(cur_x, flip(cur_y));
                }
            }
            "H" => {
                let x = next_f(&mut i);
                content.line_to(x, flip(cur_y));
                cur_x = x;
            }
            "h" => {
                cur_x += next_f(&mut i);
                content.line_to(cur_x, flip(cur_y));
            }
            "V" => {
                let y = next_f(&mut i);
                content.line_to(cur_x, flip(y));
                cur_y = y;
            }
            "v" => {
                cur_y += next_f(&mut i);
                content.line_to(cur_x, flip(cur_y));
            }
            "C" => {
                while i < tokens.len() && (tokens[i].parse::<f32>().is_ok() || tokens[i].starts_with('-')) {
                    let x1 = next_f(&mut i); let y1 = next_f(&mut i);
                    let x2 = next_f(&mut i); let y2 = next_f(&mut i);
                    let x  = next_f(&mut i); let y  = next_f(&mut i);
                    content.cubic_to(x1, flip(y1), x2, flip(y2), x, flip(y));
                    cur_x = x; cur_y = y;
                }
            }
            "c" => {
                while i < tokens.len() && (tokens[i].parse::<f32>().is_ok() || tokens[i].starts_with('-')) {
                    let dx1 = next_f(&mut i); let dy1 = next_f(&mut i);
                    let dx2 = next_f(&mut i); let dy2 = next_f(&mut i);
                    let dx  = next_f(&mut i); let dy  = next_f(&mut i);
                    content.cubic_to(cur_x + dx1, flip(cur_y + dy1), cur_x + dx2, flip(cur_y + dy2), cur_x + dx, flip(cur_y + dy));
                    cur_x += dx; cur_y += dy;
                }
            }
            "Q" => {
                // Quadratic Bézier → convert to cubic.
                while i < tokens.len() && (tokens[i].parse::<f32>().is_ok() || tokens[i].starts_with('-')) {
                    let qx1 = next_f(&mut i); let qy1 = next_f(&mut i);
                    let x   = next_f(&mut i); let y   = next_f(&mut i);
                    // Cubic control points: cp1 = start + 2/3*(qp1-start), cp2 = end + 2/3*(qp1-end)
                    let cp1x = cur_x + (qx1 - cur_x) * 2.0 / 3.0;
                    let cp1y = cur_y + (qy1 - cur_y) * 2.0 / 3.0;
                    let cp2x = x     + (qx1 - x)     * 2.0 / 3.0;
                    let cp2y = y     + (qy1 - y)      * 2.0 / 3.0;
                    content.cubic_to(cp1x, flip(cp1y), cp2x, flip(cp2y), x, flip(y));
                    cur_x = x; cur_y = y;
                }
            }
            "q" => {
                while i < tokens.len() && (tokens[i].parse::<f32>().is_ok() || tokens[i].starts_with('-')) {
                    let dqx1 = next_f(&mut i); let dqy1 = next_f(&mut i);
                    let dx   = next_f(&mut i); let dy   = next_f(&mut i);
                    let qx1 = cur_x + dqx1; let qy1 = cur_y + dqy1;
                    let ex  = cur_x + dx;   let ey  = cur_y + dy;
                    let cp1x = cur_x + (qx1 - cur_x) * 2.0 / 3.0;
                    let cp1y = cur_y + (qy1 - cur_y) * 2.0 / 3.0;
                    let cp2x = ex    + (qx1 - ex)    * 2.0 / 3.0;
                    let cp2y = ey    + (qy1 - ey)     * 2.0 / 3.0;
                    content.cubic_to(cp1x, flip(cp1y), cp2x, flip(cp2y), ex, flip(ey));
                    cur_x = ex; cur_y = ey;
                }
            }
            "Z" | "z" => {
                content.close_path();
            }
            _ => {}
        }
    }
}

/// Draw background fill and/or border for a form field appearance XObject.
/// Coordinates are local (0,0 → w,h).
fn write_field_background_border(
    content: &mut Content,
    background: &Option<String>,
    border: &Option<(f32, String)>,
    w: f32,
    h: f32,
) {
    let has_bg = background.is_some();
    let has_border = border.as_ref().map_or(false, |(bw, _)| *bw > 0.0);
    if has_bg || has_border {
        content.save_state();
        if has_bg {
            let (r, g, b) = parse_hex(background.as_deref().unwrap_or("#ffffff"));
            content.set_fill_rgb(r, g, b);
        }
        if has_border {
            let (bw, bc) = border.as_ref().unwrap();
            let (r, g, b) = parse_hex(bc);
            content.set_stroke_rgb(r, g, b);
            content.set_line_width(*bw);
        }
        content.rect(0.0, 0.0, w, h);
        match (has_bg, has_border) {
            (true,  true)  => { content.fill_nonzero_and_stroke(); }
            (true,  false) => { content.fill_nonzero(); }
            (false, true)  => { content.stroke(); }
            (false, false) => {}
        }
        content.restore_state();
    }
}

/// Draw a circle using 4 Bézier curves.  If `filled` is true, fills with
/// current fill colour; otherwise strokes with a 1pt black stroke.
fn write_circle(content: &mut Content, cx: f32, cy: f32, r: f32, filled: bool) {
    // Bézier constant for circles: k ≈ 0.5523
    let k = r * 0.5523;
    content.save_state();
    if filled {
        content.set_fill_rgb(0.0, 0.0, 0.0);
    } else {
        content.set_stroke_rgb(0.0, 0.0, 0.0);
        content.set_line_width(1.0);
    }
    content.move_to(cx, cy + r);
    content.cubic_to(cx + k, cy + r,   cx + r, cy + k,   cx + r, cy);
    content.cubic_to(cx + r, cy - k,   cx + k, cy - r,   cx,     cy - r);
    content.cubic_to(cx - k, cy - r,   cx - r, cy - k,   cx - r, cy);
    content.cubic_to(cx - r, cy + k,   cx - k, cy + r,   cx,     cy + r);
    content.close_path();
    if filled { content.fill_nonzero(); } else { content.stroke(); }
    content.restore_state();
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

// ── Sub-function data structures ──────────────────────────────────────────────

/// Font object ID bundle for one font.
///   Builtin  → only `font_dict_id` used (`extra_ids` is empty).
///   Truetype → `extra_ids` = [program, descriptor, cid_font, cmap].
struct FontIds {
    /// The ref placed in page /Font resource dictionaries.
    font_dict_id: Ref,
    /// Additional objects for TrueType fonts.
    extra_ids:    Vec<Ref>,
}

/// All pre-allocated indirect-object IDs for a document, produced by
/// `allocate_ids` and consumed by `assemble_pdf`.
/// Per-field allocated PDF object IDs.
struct FieldAllocIds {
    /// The merged field+widget annotation ref (or standalone widget for radio children).
    widget_id: Ref,
    /// Appearance XObject refs.
    /// Text/Button/Dropdown: [normal_ap]
    /// Checkbox: [yes_ap, off_ap]
    /// Radio: [on_ap, off_ap]
    ap_ids: Vec<Ref>,
}

struct AllocatedIds {
    catalog_id:        Ref,
    info_id:           Ref,
    pages_id:          Ref,
    page_ids:          Vec<Ref>,
    content_ids:       Vec<Ref>,
    annot_ids:         Vec<Vec<Ref>>,
    font_id_map:       HashMap<String, FontIds>,
    image_id_map:      HashMap<String, Ref>,
    image_smask_ids:   Vec<Option<Ref>>,
    field_alloc:       Vec<FieldAllocIds>,        // one per field in collect order
    radio_parent_alloc: Vec<(String, Ref)>,        // (group_name, parent_dict_id)
    /// ExtGState objects for canvas layer opacity: (opacity_pct 1..99) → Ref.
    opacity_gs_ids:    Vec<(u32, Ref)>,
    /// The form XObjects of the transparency groups, per page, in the order of the page's groups.
    group_ids:         Vec<Vec<Ref>>,
    /// `None` when the document has no bookmarks.
    outline:           Option<OutlineIds>,
}

/// The objects of the outline and the named destinations, allocated last so that a document without bookmarks
/// keeps the ids it had before bookmarks existed.
struct OutlineIds {
    dests_id:   Ref,
    outline_id: Ref,
    /// One outline item per anchor, in the order of the anchors.
    item_ids:   Vec<Ref>,
}

// ── Step 1a: Font preparation ─────────────────────────────────────────────────

/// Collect every font name referenced in `pages`, prepare each for embedding,
/// and return a stable (sorted) name list together with the prepared-font map.
///
/// Helvetica is always included so the attribution's leading words always have a
/// font. The attribution's name face is added only when `attribution` is set, so a
/// licensed document embeds nothing for it.
fn prepare_fonts(
    pages:       &[RenderPage],
    font_defs:   &HashMap<String, FontDef>,
    registry:    &FontRegistry,
    attribution: bool,
) -> (Vec<String>, HashMap<String, PreparedFont>) {
    let mut used_font_names: HashSet<String> = HashSet::new();
    for page in pages {
        collect_used_fonts(&page.nodes, &mut used_font_names);
    }
    used_font_names.insert("Helvetica".to_string());
    if attribution {
        used_font_names.insert(ATTRIBUTION_FONT_KEY.to_string());
    }

    // Sort names so resource IDs (F0, F1, …) are assigned deterministically.
    let mut sorted_font_names: Vec<String> = used_font_names.into_iter().collect();
    sorted_font_names.sort();

    // Pre-collect every character used per font across all pages.  This must
    // happen before font preparation so that subsetting knows which glyphs to
    // keep and can build the remapped-GID tables used by the content streams.
    let mut chars_per_font: HashMap<String, HashSet<char>> = HashMap::new();
    for name in &sorted_font_names {
        let mut used: HashSet<char> = HashSet::new();
        for page in pages {
            collect_chars_for_font(&page.nodes, name, &mut used);
        }
        if attribution && name == "Helvetica" {
            used.extend(ATTRIBUTION_PREFIX.chars());
        }
        if name == ATTRIBUTION_FONT_KEY {
            used.extend(ATTRIBUTION_NAME.chars());
        }
        chars_per_font.insert(name.clone(), used);
    }

    let empty_chars: HashSet<char> = HashSet::new();
    let mut fonts: HashMap<String, PreparedFont> = HashMap::new();
    for (idx, name) in sorted_font_names.iter().enumerate() {
        let resource_name = format!("F{idx}");
        let used_chars    = chars_per_font.get(name).unwrap_or(&empty_chars);
        // The attribution face ships inside the engine, so it never depends on the
        // document's font declarations or on bytes the caller registered.
        let kind = if name == ATTRIBUTION_FONT_KEY {
            prepare_truetype_font(ATTRIBUTION_FONT, used_chars)
        } else {
            resolve_font_kind(name, font_defs, registry, used_chars)
        };
        fonts.insert(name.clone(), PreparedFont { resource_name, kind });
    }

    (sorted_font_names, fonts)
}

// ── Step 1b: Image preparation ────────────────────────────────────────────────

/// Collect every image name referenced in `pages`, filter to embeddable
/// formats, and return a stable (sorted) name list and the XObject resource
/// name map (`"logo.png"` → `"I0"`, etc.).
fn prepare_images(
    pages:          &[RenderPage],
    image_registry: &ImageRegistry,
) -> (Vec<String>, HashMap<String, String>) {
    let mut used_image_names: HashSet<String> = HashSet::new();
    for page in pages {
        collect_used_images(&page.nodes, &mut used_image_names);
    }
    // Filter to only formats that can actually be embedded to avoid dangling
    // XObject references in the resource dictionary.
    let mut sorted_image_names: Vec<String> = used_image_names.into_iter()
        .filter(|name| image_registry.get(name).map_or(false, is_image_embeddable))
        .collect();
    sorted_image_names.sort();
    let image_res_map: HashMap<String, String> = sorted_image_names.iter().enumerate()
        .map(|(i, n)| (n.clone(), format!("I{i}")))
        .collect();
    (sorted_image_names, image_res_map)
}

// ── Step 2: Content stream building ──────────────────────────────────────────

/// A page drawn: its content stream, its link annotations, and the transparency groups its content draws. A group
/// is a form XObject written with the page, and the content invokes it by the name `group_resource_name` gives
/// its position here.
struct RenderedPage {
    content: Vec<u8>,
    annots:  Vec<AnnotData>,
    groups:  Vec<PageGroup>,
}

/// A transparency group of a page: its content stream, and which of the page's groups it draws. A group is
/// pushed after the groups it draws, so those are the ones just before it, and its resources name them.
struct PageGroup {
    content: Vec<u8>,
    nested:  Range<usize>,
}

/// Render every page to a content-stream byte buffer and collect link
/// annotations and transparency groups.  Returns one `RenderedPage` per page.
fn build_content_streams(
    pages:         &[RenderPage],
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    attribution:   bool,
) -> Vec<RenderedPage> {
    let mut rendered_pages: Vec<RenderedPage> = Vec::new();

    for page in pages {
        let mut content = Content::new();
        let mut annots: Vec<AnnotData> = Vec::new();
        let mut groups: Vec<PageGroup> = Vec::new();

        // Draw optional page background before any nodes.
        if let Some(bg) = &page.background {
            let (r, g, b) = parse_hex(bg);
            content.set_fill_rgb(r, g, b);
            content.rect(0.0, 0.0, page.width, page.height);
            content.fill_nonzero();
        }

        // Draw all render-tree nodes.
        draw_nodes(&mut content, &mut annots, &mut groups, &page.nodes, fonts, image_res_map, page.height);

        // Drawn last so it sits above the page's own content.
        if attribution {
            draw_attribution(&mut content, &mut annots, fonts, page.width, page.height);
        }

        rendered_pages.push(RenderedPage { content: content.finish(), annots, groups });
    }

    rendered_pages
}

// ── Unlicensed-use attribution ────────────────────────────────────────────────
//
// Drawn on every page of an unlicensed render. Deliberately small: the words are a
// quiet grey and only the mark carries the brand orange, so the line reads as a
// credit and never competes with the customer's document.

/// Words set before the mark, in Helvetica so they need no embedded font.
const ATTRIBUTION_PREFIX: &str = "Made with";

/// Product name set after the mark, in the lpdf.io wordmark face.
pub(crate) const ATTRIBUTION_NAME: &str = "Lpdf";

/// Target of the single link annotation covering the whole line.
const ATTRIBUTION_URL: &str = "https://lpdf.io";

/// Font-map key for the wordmark face. Specific enough that no document's own font
/// declarations are expected to collide with it.
pub(crate) const ATTRIBUTION_FONT_KEY: &str = "Radley-LpdfAttribution";

/// Radley Regular cut down to the glyphs of `ATTRIBUTION_NAME`: about 2 KB inside the
/// engine rather than the full 93 KB face. See `assets/fonts/README.md`.
pub(crate) const ATTRIBUTION_FONT: &[u8] = include_bytes!("../assets/fonts/radley-attribution.ttf");

/// Radley's cap height as a share of the em (1302 of 2048 units, OS/2 `sCapHeight`),
/// used to centre the name's capitals on the mark.
const ATTRIBUTION_CAP_HEIGHT: f32 = 1302.0 / 2048.0;

/// Neutral dark grey for the words: about 5.7:1 on white paper, legible without
/// drawing the eye.
const ATTRIBUTION_TEXT_COLOR: &str = "#666666";

/// lpdf brand orange, as in `lpdf-mark.svg`.
const ATTRIBUTION_MARK_COLOR: &str = "#d76f04";

/// The lpdf mark from `lpdf-mark.svg`: two parallelograms in a 100 × 100 box with y
/// pointing down, drawn as vector paths so it needs no image object.
const LPDF_MARK: [[(f32, f32); 4]; 2] = [
    [(0.0, 33.03), (33.03, 0.0), (33.03, 62.452), (0.0, 95.482)],
    [(4.518, 100.0), (37.548, 66.97), (100.0, 66.97), (66.97, 100.0)],
];

/// Draw "Made with", the lpdf mark and "Lpdf" right-aligned in the page's top-right
/// corner, and cover the whole line with one link to lpdf.io.
///
/// The line sits 12 pt in from the edges because desktop printers cannot reach the
/// outer 12–18 pt of a sheet; any closer and printed copies lose it.
fn draw_attribution(
    content: &mut Content,
    annots:  &mut Vec<AnnotData>,
    fonts:   &HashMap<String, PreparedFont>,
    page_w:  f32,
    page_h:  f32,
) {
    const PAD:         f32 = 12.0;
    const PREFIX_SIZE: f32 = 8.0;
    // A point larger than the words, because a serif reads smaller than Helvetica
    // at the same size.
    const NAME_SIZE:   f32 = 9.0;
    const MARK_SIZE:   f32 = 9.0;
    const GAP:         f32 = 3.0;
    // Radley ships no bold. Stroking the outline as well as filling it thickens the
    // glyphs, the way browsers synthesise the 600 weight the lpdf.io wordmark asks for.
    const BOLD_STROKE: f32 = NAME_SIZE * 0.04;

    let (Some(prefix_font), Some(name_font)) =
        (fonts.get("Helvetica"), fonts.get(ATTRIBUTION_FONT_KEY))
    else {
        return;
    };

    let prefix_w = prefix_font.text_width(ATTRIBUTION_PREFIX, PREFIX_SIZE);
    // The stroke grows each glyph by half its width on either side.
    let name_w   = name_font.text_width(ATTRIBUTION_NAME, NAME_SIZE) + BOLD_STROKE;
    let line_w   = prefix_w + GAP + MARK_SIZE + GAP + name_w;

    // The mark fills [PAD, PAD + MARK_SIZE] measured down from the top edge, and both
    // runs of text share one baseline placed so the name's capitals centre on the mark.
    let left     = page_w - PAD - line_w;
    let baseline = page_h - (PAD + (MARK_SIZE + NAME_SIZE * ATTRIBUTION_CAP_HEIGHT) / 2.0);
    let mark_x   = left + prefix_w + GAP;
    let name_x   = mark_x + MARK_SIZE + GAP + BOLD_STROKE / 2.0;

    let (tr, tg, tb) = parse_hex(ATTRIBUTION_TEXT_COLOR);
    let (mr, mg, mb) = parse_hex(ATTRIBUTION_MARK_COLOR);

    // Its own graphics state, so the colours, stroke width and text rendering mode set
    // here cannot leak into anything drawn later on the page.
    content.save_state();

    content.set_fill_rgb(tr, tg, tb);
    content.begin_text();
    content.set_font(Name(prefix_font.resource_name.as_bytes()), PREFIX_SIZE);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, left, baseline]);
    content.show(Str(&prefix_font.encode_text(ATTRIBUTION_PREFIX)));
    content.end_text();

    let scale = MARK_SIZE / 100.0;
    content.set_fill_rgb(mr, mg, mb);
    for shape in &LPDF_MARK {
        let (x0, y0) = shape[0];
        content.move_to(mark_x + x0 * scale, page_h - (PAD + y0 * scale));
        for &(x, y) in &shape[1..] {
            content.line_to(mark_x + x * scale, page_h - (PAD + y * scale));
        }
        content.close_path();
    }
    content.fill_nonzero();

    content.set_fill_rgb(tr, tg, tb);
    content.set_stroke_rgb(tr, tg, tb);
    content.set_line_width(BOLD_STROKE);
    content.begin_text();
    content.set_text_rendering_mode(TextRenderingMode::FillStroke);
    content.set_font(Name(name_font.resource_name.as_bytes()), NAME_SIZE);
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, name_x, baseline]);
    content.show(Str(&name_font.encode_text(ATTRIBUTION_NAME)));
    content.end_text();

    content.restore_state();

    // One target for the whole line, with slack below and above so the name's
    // descender and a fingertip both land inside it.
    annots.push(AnnotData {
        x1:  left,
        y1:  page_h - (PAD + MARK_SIZE + 3.0),
        x2:  page_w - PAD,
        y2:  page_h - (PAD - 2.0),
        url: ATTRIBUTION_URL.to_string(),
    });
}

// ── Step 3: ID allocation ─────────────────────────────────────────────────────

/// Pre-allocate all PDF indirect-object IDs so that forward references (e.g.
/// a page dict referencing its content stream) are known before writing begins.
// ── Form field collection ─────────────────────────────────────────────────────

/// Walk the render tree and collect all `RenderField` nodes with their page index.
fn collect_render_fields(pages: &[RenderPage]) -> Vec<(usize, RenderField)> {
    let mut out = Vec::new();
    for (page_idx, page) in pages.iter().enumerate() {
        collect_fields_in_nodes(&page.nodes, page_idx, &mut out);
    }
    out
}

fn collect_fields_in_nodes(nodes: &[RenderNode], page_idx: usize, out: &mut Vec<(usize, RenderField)>) {
    for node in nodes {
        match node {
            RenderNode::Field(f)  => out.push((page_idx, f.clone())),
            RenderNode::Box(b)    => collect_fields_in_nodes(&b.children, page_idx, out),
            RenderNode::Link(l)   => collect_fields_in_nodes(&l.children, page_idx, out),
            _                     => {}
        }
    }
}

// ── Step 3b: ID allocation ─────────────────────────────────────────────────────

fn allocate_ids(
    pages:              &[RenderPage],
    rendered_pages:     &[RenderedPage],
    fonts:              &HashMap<String, PreparedFont>,
    sorted_font_names:  &[String],
    sorted_image_names: &[String],
    image_registry:     &ImageRegistry,
    fields:             &[(usize, RenderField)],
) -> AllocatedIds {
    let n = rendered_pages.len();
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
        .map(|page| (0..page.annots.len()).map(|_| alloc.next()).collect())
        .collect();

    // Font object IDs:
    //   Builtin  → 1 object  (Type1 font dict)
    //   Truetype → 5 objects (font-program stream, font descriptor, CID font,
    //                         ToUnicode CMap stream, Type0 wrapper dict)
    let mut font_id_map: HashMap<String, FontIds> = HashMap::new();
    for name in sorted_font_names {
        let font = &fonts[name];
        let font_dict_id = alloc.next();
        let extra_ids = match &font.kind {
            PreparedFontKind::Builtin { .. }  => vec![],
            PreparedFontKind::Truetype { .. } => (0..4).map(|_| alloc.next()).collect(),
        };
        font_id_map.insert(name.clone(), FontIds { font_dict_id, extra_ids });
    }

    // One image XObject per unique image name.
    let image_xobj_ids: Vec<Ref> = (0..sorted_image_names.len()).map(|_| alloc.next()).collect();
    let image_id_map: HashMap<String, Ref> = sorted_image_names.iter().enumerate()
        .map(|(i, n)| (n.clone(), image_xobj_ids[i]))
        .collect();

    // For RGBA PNGs, allocate an extra Ref for the alpha SMask XObject.
    let image_smask_ids: Vec<Option<Ref>> = sorted_image_names.iter().map(|name| {
        let is_rgba = image_registry.get(name)
            .and_then(|b| png_info(b))
            .map_or(false, |(_, _, _, ct)| ct == 6);
        if is_rgba { Some(alloc.next()) } else { None }
    }).collect();

    // Per-field allocation.
    // For each field we allocate:
    //   - widget_id (merged field+annotation, or radio child widget)
    //   - appearance XObject refs:
    //       Text/Dropdown/Button: 1 (normal AP stream)
    //       Checkbox: 2 (/Yes + /Off)
    //       Radio:    2 (/On  + /Off)
    let mut field_alloc: Vec<FieldAllocIds> = Vec::new();
    // Radio parent dicts: deduplicated by group name.
    let mut radio_parent_alloc: Vec<(String, Ref)> = Vec::new();
    let mut seen_radio_groups: HashMap<String, ()> = HashMap::new();

    for (_, field) in fields {
        let widget_id = alloc.next();
        let ap_count = match field.kind {
            FieldKind::Checkbox | FieldKind::Radio => 2,
            _                                       => 1,
        };
        let ap_ids: Vec<Ref> = (0..ap_count).map(|_| alloc.next()).collect();
        field_alloc.push(FieldAllocIds { widget_id, ap_ids });

        if field.kind == FieldKind::Radio {
            let group = field.group.clone().unwrap_or_default();
            if !group.is_empty() && !seen_radio_groups.contains_key(&group) {
                seen_radio_groups.insert(group.clone(), ());
                radio_parent_alloc.push((group, alloc.next()));
            }
        }
    }

    let acroform_id = if !fields.is_empty() { Some(alloc.next()) } else { None };
    let _ = acroform_id; // AcroForm is written inline in catalog, no separate object needed

    // One ExtGState object per unique opacity percentage (1..99) used in canvas layers.
    let mut used_opacities: HashSet<u32> = HashSet::new();
    for page in pages {
        collect_used_opacities(&page.nodes, &mut used_opacities);
    }
    let mut sorted_opacities: Vec<u32> = used_opacities.into_iter().collect();
    sorted_opacities.sort();
    let opacity_gs_ids: Vec<(u32, Ref)> =
        sorted_opacities.iter().map(|&pct| (pct, alloc.next())).collect();

    // One form XObject per transparency group, grouped by page.
    let group_ids: Vec<Vec<Ref>> = rendered_pages
        .iter()
        .map(|page| (0..page.groups.len()).map(|_| alloc.next()).collect())
        .collect();

    let anchor_count = placed_anchors(pages).len();
    let outline = (anchor_count > 0).then(|| OutlineIds {
        dests_id:   alloc.next(),
        outline_id: alloc.next(),
        item_ids:   (0..anchor_count).map(|_| alloc.next()).collect(),
    });

    AllocatedIds {
        catalog_id,
        info_id,
        pages_id,
        page_ids,
        content_ids,
        annot_ids,
        font_id_map,
        image_id_map,
        image_smask_ids,
        field_alloc,
        radio_parent_alloc,
        opacity_gs_ids,
        group_ids,
        outline,
    }
}

// ── Step 4: PDF assembly ──────────────────────────────────────────────────────

/// Write all pre-built objects into a `pdf_writer::Pdf` buffer and return the
/// final binary bytes.
fn assemble_pdf(
    pages:              &[RenderPage],
    fonts:              &HashMap<String, PreparedFont>,
    sorted_font_names:  &[String],
    sorted_image_names: &[String],
    image_registry:     &ImageRegistry,
    image_res_map:      &HashMap<String, String>,
    rendered_pages:     Vec<RenderedPage>,
    ids:                AllocatedIds,
    fields:             &[(usize, RenderField)],
    anchors:            &[PlacedAnchor],
    meta:               &Meta,
    created_on:         Option<&str>,
) -> Result<Vec<u8>, String> {
    let AllocatedIds {
        catalog_id, info_id, pages_id,
        page_ids, content_ids, annot_ids,
        font_id_map, image_id_map, image_smask_ids,
        field_alloc, radio_parent_alloc, opacity_gs_ids, group_ids,
        outline,
    } = ids;

    let n = pages.len();
    let mut pdf = Pdf::new();

    // -- Catalog ---------------------------------------------------------------
    {
        let mut cat = pdf.catalog(catalog_id);
        cat.pages(pages_id);
        if let Some(outline) = &outline {
            cat.destinations(outline.dests_id);
            cat.outlines(outline.outline_id);
            // Open with the outline panel showing, as the bookmarks are there to be used.
            cat.page_mode(PageMode::UseOutlines);
        }
        if !fields.is_empty() {
            // Build root field refs list.
            let mut root_fields: Vec<Ref> = Vec::new();
            for ((_, field), fa) in fields.iter().zip(field_alloc.iter()) {
                if field.kind != FieldKind::Radio {
                    root_fields.push(fa.widget_id);
                }
            }
            for (_, parent_id) in &radio_parent_alloc {
                root_fields.push(*parent_id);
            }
            let helv_font_id = font_id_map.get("Helvetica").map(|f| f.font_dict_id);
            let mut aform = cat.form();
            aform.fields(root_fields.iter().copied());
            if let Some(helv_id) = helv_font_id {
                let mut dr = aform.default_resources();
                let mut dfr = dr.fonts();
                dfr.pair(Name(b"Helv"), helv_id);
            }
        }
    }

    // -- Document Information --------------------------------------------------
    // Metadata visible in PDF reader "Properties" dialogs.
    {
        let mut info = pdf.document_info(info_id);
        if !meta.title.is_empty()    { info.title(TextStr(&meta.title)); }
        if !meta.author.is_empty()   { info.author(TextStr(&meta.author)); }
        if !meta.subject.is_empty()  { info.subject(TextStr(&meta.subject)); }
        if !meta.creator.is_empty()  { info.creator(TextStr(&meta.creator)); }
        if !meta.keywords.is_empty() { info.keywords(TextStr(&meta.keywords)); }
        // The same for every render: an unlicensed one is already marked by its attribution line.
        info.producer(TextStr("Lpdf (lpdf.io)"));

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

    // -- Resources -------------------------------------------------------------
    // What a page, or a transparency group, draws with: all fonts and images used anywhere in this document
    // (over-inclusion is harmless and avoids a second per-page scan), the ExtGStates for opacity, and the
    // groups it draws: the `drawn` positions in `groups`, the form XObjects of the page.
    let write_resources = |resources: &mut Resources<'_>, groups: &[Ref], drawn: Range<usize>| {
        {
            let mut font_res = resources.fonts();
            for name in sorted_font_names {
                let font = &fonts[name];
                let fid  = font_id_map[name].font_dict_id;
                font_res.pair(Name(font.resource_name.as_bytes()), fid);
            }
        }
        // Add image and group XObjects to the resource dictionary.
        if !sorted_image_names.is_empty() || !drawn.is_empty() {
            let mut xobj_res = resources.x_objects();
            for name in sorted_image_names {
                let res_name = &image_res_map[name];
                let img_id   = image_id_map[name];
                xobj_res.pair(Name(res_name.as_bytes()), img_id);
            }
            for index in drawn {
                xobj_res.pair(Name(group_resource_name(index).as_bytes()), groups[index]);
            }
        }
        // Add ExtGState entries for opacity.
        if !opacity_gs_ids.is_empty() {
            let mut gs_res = resources.ext_g_states();
            for (pct, gs_id) in &opacity_gs_ids {
                gs_res.pair(Name(opacity_gs_name(*pct).as_bytes()), *gs_id);
            }
        }
    };

    // -- Per-page dicts --------------------------------------------------------
    // We write the page dictionary before its content stream so that content
    // stream IDs can be referenced in the page's /Contents entry.
    for (i, page) in pages.iter().enumerate() {
        let page_id    = page_ids[i];
        let content_id = content_ids[i];

        let mut pw = pdf.page(page_id);
        pw.parent(pages_id)
          .media_box(Rect::new(0.0, 0.0, page.width, page.height));

        write_resources(&mut pw.resources(), &group_ids[i], 0..group_ids[i].len());

        // Add the content stream reference.
        pw.contents(content_id);

        // Add the per-page annotation array if there are any link annotations
        // or form field annotations.
        let page_annot_ids = &annot_ids[i];
        let field_widget_ids_on_page: Vec<Ref> = fields.iter().zip(field_alloc.iter())
            .filter(|((pg, _), _)| *pg == i)
            .map(|(_, fa)| fa.widget_id)
            .collect();
        let mut all_annot_ids: Vec<Ref> = page_annot_ids.to_vec();
        all_annot_ids.extend_from_slice(&field_widget_ids_on_page);
        if !all_annot_ids.is_empty() {
            pw.annotations(all_annot_ids.iter().copied());
        }
    }

    // -- Content streams -------------------------------------------------------
    for (i, page) in rendered_pages.iter().enumerate() {
        let compressed = compress_to_vec_zlib(&page.content, 6);
        pdf.stream(content_ids[i], &compressed)
           .filter(Filter::FlateDecode);
    }

    // -- Transparency groups ---------------------------------------------------
    // A faded layer or box is a form XObject of its own, so that the page composites it first and fades it as
    // one. It is isolated, so nothing under it on the page reaches into it, and it covers the page. A faded box
    // can hold faded boxes, so a group's resources name the groups drawn inside it.
    for (i, page) in rendered_pages.iter().enumerate() {
        for (j, group) in page.groups.iter().enumerate() {
            let compressed = compress_to_vec_zlib(&group.content, 6);
            let mut form = pdf.form_xobject(group_ids[i][j], &compressed);
            form.filter(Filter::FlateDecode);
            form.bbox(Rect::new(0.0, 0.0, pages[i].width, pages[i].height));
            form.group().transparency().isolated(true).color_space().device_rgb();
            write_resources(&mut form.resources(), &group_ids[i], group.nested.clone());
        }
    }

    // -- Opacity ExtGState objects ---------------------------------------------
    for (pct, gs_id) in &opacity_gs_ids {
        let opacity = *pct as f32 / 100.0;
        let mut gs = pdf.ext_graphics(*gs_id);
        gs.non_stroking_alpha(opacity);
        gs.stroking_alpha(opacity);
    }

    // -- Image XObject streams -------------------------------------------------
    for (i, name) in sorted_image_names.iter().enumerate() {
        let img_id   = image_id_map[name];
        let smask_id = image_smask_ids[i];
        if let Some(bytes) = image_registry.get(name) {
            embed_image_xobject(&mut pdf, img_id, smask_id, bytes);
        }
    }

    // -- Link annotation objects -----------------------------------------------
    // Each link annotation is a separate indirect object.  Invisible border
    // (0-width) so only the action fires; no visual box is drawn.  An href of
    // "#id" goes to the named destination of the bookmark with that id.
    for (i, page) in rendered_pages.iter().enumerate() {
        for (j, ann) in page.annots.iter().enumerate() {
            let aid       = annot_ids[i][j];
            let mut aw    = pdf.annotation(aid);
            aw.subtype(AnnotationType::Link)
              .rect(Rect::new(ann.x1, ann.y1, ann.x2, ann.y2))
              .border(0.0, 0.0, 0.0, None);
            match ann.url.strip_prefix('#') {
                Some(id) => {
                    aw.action()
                      .action_type(ActionType::GoTo)
                      .destination_named(Name(id.as_bytes()));
                }
                None => {
                    aw.action()
                      .action_type(ActionType::Uri)
                      .uri(Str(ann.url.as_bytes()));
                }
            }
        }
    }

    // -- Outline and named destinations ------------------------------------------
    if let Some(outline) = &outline {
        write_outline(&mut pdf, outline, anchors, &page_ids, pages);
    }

    // -- AcroForm and field annotations ----------------------------------------
    if !fields.is_empty() {
        let helv_font_id = font_id_map.get("Helvetica").map(|f| f.font_dict_id);

        // Write appearance XObjects for each field.
        for ((page_idx, field), fa) in fields.iter().zip(field_alloc.iter()) {
            let page_h = pages[*page_idx].height;
            let w = field.width;
            let h = field.height;
            match &field.kind {
                FieldKind::Text | FieldKind::Dropdown => {
                    // Normal AP: draw field background and border.
                    let ap_id = fa.ap_ids[0];
                    let mut ap_content = Content::new();
                    write_field_background_border(&mut ap_content, &field.background, &field.border, w, h);
                    let ap_bytes = ap_content.finish();
                    let mut xobj = pdf.form_xobject(ap_id, &ap_bytes);
                    xobj.bbox(Rect::new(0.0, 0.0, w, h));
                }
                FieldKind::Button => {
                    let ap_id = fa.ap_ids[0];
                    let mut ap_content = Content::new();
                    // Button: filled rectangle with label.
                    let bg = field.background.as_deref().unwrap_or("#1763cf");
                    let (r, g, b) = parse_hex(bg);
                    ap_content.save_state();
                    ap_content.set_fill_rgb(r, g, b);
                    ap_content.rect(0.0, 0.0, w, h);
                    ap_content.fill_nonzero();
                    ap_content.restore_state();
                    if !field.label.is_empty() {
                        let font = fonts.get("Helvetica").expect("Helvetica always present");
                        let font_size = (h * 0.55).max(6.0);
                        let text_w = font.text_width(&field.label, font_size);
                        let text_x = (w - text_w) / 2.0;
                        let text_y = (h - font_size) / 2.0;
                        let encoded = font.encode_text(&field.label);
                        ap_content.begin_text();
                        ap_content.set_fill_rgb(1.0, 1.0, 1.0);
                        ap_content.set_font(Name(b"Helv"), font_size);
                        ap_content.set_text_matrix([1.0, 0.0, 0.0, 1.0, text_x, text_y]);
                        ap_content.show(Str(&encoded));
                        ap_content.end_text();
                    }
                    let ap_bytes = ap_content.finish();
                    let mut xobj = pdf.form_xobject(ap_id, &ap_bytes);
                    xobj.bbox(Rect::new(0.0, 0.0, w, h));
                    let mut res = xobj.resources();
                    let mut fr = res.fonts();
                    if let Some(helv_id) = helv_font_id {
                        fr.pair(Name(b"Helv"), helv_id);
                    }
                }
                FieldKind::Checkbox => {
                    // /Yes appearance: border + checkmark.
                    {
                        let ap_id = fa.ap_ids[0];
                        let mut ap_content = Content::new();
                        write_field_background_border(&mut ap_content, &field.background, &field.border, w, h);
                        // Draw a simple checkmark path.
                        let p  = (h * 0.15).max(1.5);
                        let x0 = p * 2.0;        let y0 = h * 0.45;
                        let x1 = w * 0.4;        let y1 = p;
                        let x2 = w - p * 2.0;    let y2 = h - p;
                        ap_content.save_state();
                        ap_content.set_stroke_rgb(0.0, 0.0, 0.0);
                        ap_content.set_line_width(h * 0.1);
                        ap_content.move_to(x0, y0);
                        ap_content.line_to(x1, y1);
                        ap_content.line_to(x2, y2);
                        ap_content.stroke();
                        ap_content.restore_state();
                        let ap_bytes = ap_content.finish();
                        let mut xobj = pdf.form_xobject(ap_id, &ap_bytes);
                        xobj.bbox(Rect::new(0.0, 0.0, w, h));
                    }
                    // /Off appearance: border only.
                    {
                        let ap_id = fa.ap_ids[1];
                        let mut ap_content = Content::new();
                        write_field_background_border(&mut ap_content, &field.background, &field.border, w, h);
                        let ap_bytes = ap_content.finish();
                        let mut xobj = pdf.form_xobject(ap_id, &ap_bytes);
                        xobj.bbox(Rect::new(0.0, 0.0, w, h));
                    }
                }
                FieldKind::Radio => {
                    // /On appearance: filled circle.
                    {
                        let ap_id = fa.ap_ids[0];
                        let r_outer = (h / 2.0).min(w / 2.0) - 1.0;
                        let cx = w / 2.0;  let cy = h / 2.0;
                        let mut ap_content = Content::new();
                        write_circle(&mut ap_content, cx, cy, r_outer, false);
                        let r_inner = r_outer * 0.45;
                        write_circle(&mut ap_content, cx, cy, r_inner, true);
                        let ap_bytes = ap_content.finish();
                        let mut xobj = pdf.form_xobject(ap_id, &ap_bytes);
                        xobj.bbox(Rect::new(0.0, 0.0, w, h));
                    }
                    // /Off appearance: circle border only.
                    {
                        let ap_id = fa.ap_ids[1];
                        let r_outer = (h / 2.0).min(w / 2.0) - 1.0;
                        let cx = w / 2.0;  let cy = h / 2.0;
                        let mut ap_content = Content::new();
                        write_circle(&mut ap_content, cx, cy, r_outer, false);
                        let ap_bytes = ap_content.finish();
                        let mut xobj = pdf.form_xobject(ap_id, &ap_bytes);
                        xobj.bbox(Rect::new(0.0, 0.0, w, h));
                    }
                    let _ = page_h; // suppress unused warning for radio
                }
            }
        }

        // Write field widget annotations.
        for ((page_idx, field), fa) in fields.iter().zip(field_alloc.iter()) {
            let page_h  = pages[*page_idx].height;
            let pdf_y   = page_h - field.y - field.height;
            let rect    = Rect::new(field.x, pdf_y, field.x + field.width, pdf_y + field.height);
            let page_id = page_ids[*page_idx];

            match &field.kind {
                FieldKind::Text => {
                    let mut fw = pdf.form_field(fa.widget_id);
                    fw.field_type(FieldType::Text);
                    let mut flags = FieldFlags::empty();
                    if field.readonly  { flags |= FieldFlags::READ_ONLY; }
                    if field.required  { flags |= FieldFlags::REQUIRED; }
                    fw.field_flags(flags);
                    if !field.name.is_empty() { fw.partial_name(TextStr(&field.name)); }
                    if !field.value.is_empty() { fw.text_value(TextStr(&field.value)); }
                    if let Some(ml) = field.max_len { fw.text_max_len(ml as i32); }
                    fw.vartext_default_appearance(Str(b"/Helv 0 Tf 0 g"));
                    let mut aw = fw.into_annotation();
                    aw.subtype(AnnotationType::Widget)
                      .rect(rect)
                      .page(page_id)
                      .highlight(HighlightEffect::None)
                      .border(0.0, 0.0, 0.0, None);
                    aw.appearance()
                      .normal()
                      .stream(fa.ap_ids[0]);
                }
                FieldKind::Dropdown => {
                    let mut fw = pdf.form_field(fa.widget_id);
                    fw.field_type(FieldType::Choice);
                    let mut flags = FieldFlags::COMBO;
                    if field.readonly { flags |= FieldFlags::READ_ONLY; }
                    if field.required { flags |= FieldFlags::REQUIRED; }
                    fw.field_flags(flags);
                    if !field.name.is_empty() { fw.partial_name(TextStr(&field.name)); }
                    if !field.value.is_empty() { fw.choice_value(Some(TextStr(&field.value))); }
                    if !field.options.is_empty() {
                        fw.choice_options().options(field.options.iter().map(|o| TextStr(o.as_str())));
                    }
                    fw.vartext_default_appearance(Str(b"/Helv 0 Tf 0 g"));
                    let mut aw = fw.into_annotation();
                    aw.subtype(AnnotationType::Widget)
                      .rect(rect)
                      .page(page_id)
                      .highlight(HighlightEffect::None)
                      .border(0.0, 0.0, 0.0, None);
                    aw.appearance()
                      .normal()
                      .stream(fa.ap_ids[0]);
                }
                FieldKind::Checkbox => {
                    let checked_state = if field.checked {
                        CheckBoxState::Yes
                    } else {
                        CheckBoxState::Off
                    };
                    let mut fw = pdf.form_field(fa.widget_id);
                    fw.field_type(FieldType::Button);
                    let mut flags = FieldFlags::empty();
                    if field.readonly { flags |= FieldFlags::READ_ONLY; }
                    if field.required { flags |= FieldFlags::REQUIRED; }
                    fw.field_flags(flags);
                    if !field.name.is_empty() { fw.partial_name(TextStr(&field.name)); }
                    fw.checkbox_value(checked_state);
                    let mut aw = fw.into_annotation();
                    aw.subtype(AnnotationType::Widget)
                      .rect(rect)
                      .page(page_id)
                      .highlight(HighlightEffect::None)
                      .border(0.0, 0.0, 0.0, None);
                    aw.appearance_state(Name(if field.checked { b"Yes" } else { b"Off" }));
                    {
                        let mut ap = aw.appearance();
                        ap.normal().streams().pairs([
                            (Name(b"Yes"), fa.ap_ids[0]),
                            (Name(b"Off"), fa.ap_ids[1]),
                        ]);
                    }
                }
                FieldKind::Radio => {
                    // Radio children: find the parent's ref
                    let group = field.group.as_deref().unwrap_or("");
                    let parent_id_opt = radio_parent_alloc.iter()
                        .find(|(g, _)| g == group)
                        .map(|(_, id)| *id);
                    if let Some(parent_id) = parent_id_opt {
                        let on_name_bytes = field.value.as_bytes().to_vec();
                        let mut fw = pdf.form_field(fa.widget_id);
                        fw.parent(parent_id);
                        let mut aw = fw.into_annotation();
                        aw.subtype(AnnotationType::Widget)
                          .rect(rect)
                          .page(page_id)
                          .highlight(HighlightEffect::None)
                          .border(0.0, 0.0, 0.0, None);
                        aw.appearance_state(Name(if field.checked { &on_name_bytes } else { b"Off" }));
                        {
                            let mut ap = aw.appearance();
                            ap.normal().streams().pairs([
                                (Name(&on_name_bytes), fa.ap_ids[0]),
                                (Name(b"Off"), fa.ap_ids[1]),
                            ]);
                        }
                    }
                }
                FieldKind::Button => {
                    let mut fw = pdf.form_field(fa.widget_id);
                    fw.field_type(FieldType::Button);
                    fw.field_flags(FieldFlags::PUSHBUTTON);
                    if !field.name.is_empty() { fw.partial_name(TextStr(&field.name)); }
                    let mut aw = fw.into_annotation();
                    aw.subtype(AnnotationType::Widget)
                      .rect(rect)
                      .page(page_id)
                      .highlight(HighlightEffect::None)
                      .border(0.0, 0.0, 0.0, None);
                    aw.appearance()
                      .normal()
                      .stream(fa.ap_ids[0]);
                    // Submit action: if action_url is set, add a submit-form action.
                    if let Some(url) = &field.action_url {
                        let url_bytes = url.as_bytes().to_vec();
                        aw.action()
                          .action_type(ActionType::SubmitForm)
                          .uri(Str(&url_bytes));
                    }
                }
            }
        }

        // Write radio parent field dicts (one per group).
        for (group_name, parent_id) in &radio_parent_alloc {
            // Collect child widget IDs for this group.
            let child_ids: Vec<Ref> = fields.iter().zip(field_alloc.iter())
                .filter(|((_, f), _)| f.kind == FieldKind::Radio && f.group.as_deref() == Some(group_name.as_str()))
                .map(|(_, fa)| fa.widget_id)
                .collect();

            // Determine the selected value: whichever radio child has checked=true.
            let selected_val: String = fields.iter()
                .find(|(_, f)| f.kind == FieldKind::Radio && f.group.as_deref() == Some(group_name.as_str()) && f.checked)
                .map(|(_, f)| f.value.clone())
                .unwrap_or_default();

            let mut fw = pdf.form_field(*parent_id);
            fw.field_type(FieldType::Button);
            fw.field_flags(FieldFlags::RADIO);
            fw.partial_name(TextStr(group_name.as_str()));
            if !selected_val.is_empty() {
                fw.radio_value(Name(selected_val.as_bytes()));
            }
            fw.children(child_ids.iter().copied());
        }
    }

    // -- Font objects ----------------------------------------------------------
    for name in sorted_font_names {
        let font = &fonts[name];
        let fids = &font_id_map[name];

        match &font.kind {
            PreparedFontKind::Builtin { base_name } => {
                // Type 1 (built-in resident) font.  Just a font dictionary with
                // a /BaseFont entry; no font program stream is needed.
                pdf.type1_font(fids.font_dict_id)
                   .base_font(Name(base_name.as_bytes()))
                   .encoding_predefined(Name(b"WinAnsiEncoding"));
            }

            PreparedFontKind::Truetype {
                original_bytes, subsetted_bytes, glyph_unicode, glyph_widths, ..
            } => {
                // Five objects are needed for a proper TrueType composite font:
                //   [0] Font program stream (subsetted TrueType bytes)
                //   [1] Font descriptor
                //   [2] CID font dictionary (the descendant font)
                //   [3] ToUnicode CMap stream
                //   [font_dict_id] Type0 composite font wrapper

                let prog_id = fids.extra_ids[0];
                let desc_id = fids.extra_ids[1];
                let cid_id  = fids.extra_ids[2];
                let cmap_id = fids.extra_ids[3];

                // Parse the original font for descriptor metrics (ascender,
                // descender, bounding box).  These values are identical in the
                // subsetted font but parsing the full face is simpler.
                let face = Face::parse(original_bytes, 0)
                    .map_err(|e| format!("Failed to parse font '{name}': {e:?}"))?;
                let upem = face.units_per_em() as f32;

                // [0] Font program — subsetted + FlateDecode-compressed.
                // /Length1 must be the *uncompressed* (subsetted) length per the
                // PDF spec for /FontFile2 TrueType streams.
                let compressed_font = compress_to_vec_zlib(subsetted_bytes, 6);
                pdf.stream(prog_id, &compressed_font)
                   .filter(Filter::FlateDecode)
                   .pair(Name(b"Length1"), subsetted_bytes.len() as i32);

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
                        for (gid, width) in glyph_widths {
                            w.consecutive(*gid, [*width]);
                        }
                    }
                }

                // [3] ToUnicode CMap stream — enables text extraction.
                let cmap_bytes = build_to_unicode_cmap(&fname, glyph_unicode);
                let cmap_compressed = compress_to_vec_zlib(&cmap_bytes, 6);
                pdf.stream(cmap_id, &cmap_compressed)
                   .filter(Filter::FlateDecode);

                // [font_dict_id] Type0 composite font wrapper.
                pdf.type0_font(fids.font_dict_id)
                   .base_font(Name(fname.as_bytes()))
                   .encoding_predefined(Name(b"Identity-H"))
                   .descendant_font(cid_id)
                   .to_unicode(cmap_id);
            }
        }
    }

    Ok(pdf.finish())
}

// ── Main render function ──────────────────────────────────────────────────────

/// Convert a fully laid-out document into a binary PDF.
///
/// # Parameters
/// - `pages`      – Layout output: one `RenderPage` per document page.
/// - `font_defs`  – Font name → definition from the document's `<fonts>` section.
/// - `registry`   – Raw font bytes for custom fonts (populated via `load_font`).
/// - `meta`       – Document metadata (title, author, subject, etc.).
/// - `created_on` – Optional ISO 8601 date string written to `/CreationDate`.
/// - `licensed`   – `true` when a valid commercial license token was supplied.
///                  When `false`, draws the attribution line ("Made with", the lpdf
///                  mark, "Lpdf") in every page's top-right corner.
pub fn render_pdf(
    pages:          &[RenderPage],
    font_defs:      &HashMap<String, FontDef>,
    registry:       &FontRegistry,
    image_registry: &ImageRegistry,
    meta:           &Meta,
    created_on:     Option<&str>,
    licensed:       bool,
) -> Result<Vec<u8>, String> {
    // The attribution is the licence signal itself, so it follows `licensed` rather
    // than a separate flag every caller would have to keep in step.
    let attribution = !licensed;
    let (sorted_font_names, fonts) =
        prepare_fonts(pages, font_defs, registry, attribution);
    let (sorted_image_names, image_res_map) =
        prepare_images(pages, image_registry);
    let anchors = placed_anchors(pages);
    let mut rendered_pages =
        build_content_streams(pages, &fonts, &image_res_map, attribution);
    drop_dangling_links(&mut rendered_pages, &anchors);
    let fields = collect_render_fields(pages);
    let ids =
        allocate_ids(pages, &rendered_pages, &fonts, &sorted_font_names, &sorted_image_names, image_registry, &fields);
    assemble_pdf(
        pages, &fonts, &sorted_font_names, &sorted_image_names,
        image_registry, &image_res_map, rendered_pages, ids, &fields, &anchors,
        meta, created_on,
    )
}

/// Remove the links to an id no bookmark has, such as an `href="#"` outside a table of contents.
fn drop_dangling_links(rendered_pages: &mut [RenderedPage], anchors: &[PlacedAnchor]) {
    let ids: HashSet<&str> = anchors.iter().map(|placed| placed.anchor.id.as_str()).collect();
    for page in rendered_pages.iter_mut() {
        page.annots.retain(|ann| ann.url.strip_prefix('#').is_none_or(|id| ids.contains(id)));
    }
}

/// Write the `/Dests` dictionary, a named destination per anchor, and the outline. The outline nests the
/// anchors by level: an anchor's parent is the closest anchor before it with a lower level.
fn write_outline(pdf: &mut Pdf, ids: &OutlineIds, anchors: &[PlacedAnchor], page_ids: &[Ref], pages: &[RenderPage]) {
    {
        let mut dests = pdf.destinations(ids.dests_id);
        for placed in anchors {
            let top = pages[placed.page].height - placed.anchor.y;
            dests.insert(Name(placed.anchor.id.as_bytes()))
                .page(page_ids[placed.page])
                .xyz(0.0, top, None);
        }
    }

    let n = anchors.len();
    let mut parents: Vec<Option<usize>> = vec![None; n];
    let mut ancestors: Vec<usize> = Vec::new();
    for (i, placed) in anchors.iter().enumerate() {
        while ancestors.last().is_some_and(|&a| anchors[a].anchor.level >= placed.anchor.level) {
            ancestors.pop();
        }
        parents[i] = ancestors.last().copied();
        ancestors.push(i);
    }

    let mut top_level: Vec<usize> = Vec::new();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, parent) in parents.iter().enumerate() {
        match parent {
            Some(p) => children[*p].push(i),
            None    => top_level.push(i),
        }
    }
    let mut prev: Vec<Option<usize>> = vec![None; n];
    let mut next: Vec<Option<usize>> = vec![None; n];
    for siblings in children.iter().chain(std::iter::once(&top_level)) {
        for pair in siblings.windows(2) {
            next[pair[0]] = Some(pair[1]);
            prev[pair[1]] = Some(pair[0]);
        }
    }
    // Every item is open, so an item's count is all of its descendants. A child comes after its parent, so a
    // walk from the end has each count complete before it is added to the parent's.
    let mut descendants: Vec<i32> = vec![0; n];
    for i in (0..n).rev() {
        if let Some(p) = parents[i] {
            descendants[p] += 1 + descendants[i];
        }
    }

    {
        let mut root = pdf.outline(ids.outline_id);
        if let (Some(&first), Some(&last)) = (top_level.first(), top_level.last()) {
            root.first(ids.item_ids[first]).last(ids.item_ids[last]);
        }
        root.count(n as i32);
    }
    for (i, placed) in anchors.iter().enumerate() {
        let mut item = pdf.outline_item(ids.item_ids[i]);
        item.title(TextStr(&placed.anchor.label));
        item.parent(parents[i].map_or(ids.outline_id, |p| ids.item_ids[p]));
        if let Some(p) = prev[i] { item.prev(ids.item_ids[p]); }
        if let Some(nx) = next[i] { item.next(ids.item_ids[nx]); }
        if let (Some(&first), Some(&last)) = (children[i].first(), children[i].last()) {
            item.first(ids.item_ids[first]).last(ids.item_ids[last]).count(descendants[i]);
        }
        item.dest_name(Name(placed.anchor.id.as_bytes()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_attribution_face_has_every_glyph_of_the_name() {
        // The face is a cut-down copy: changing the name without regenerating it would
        // draw empty .notdef boxes instead of letters.
        let face = Face::parse(ATTRIBUTION_FONT, 0).expect("bundled attribution face parses");
        for c in ATTRIBUTION_NAME.chars() {
            assert!(face.glyph_index(c).is_some(), "attribution face has no glyph for {c:?}");
        }
    }

    #[test]
    fn attribution_cap_height_matches_the_bundled_face() {
        // The constant positions the baseline; a regenerated face with other metrics
        // would otherwise misalign the name against the mark without any error.
        let face = Face::parse(ATTRIBUTION_FONT, 0).expect("bundled attribution face parses");
        let cap_height = face.capital_height().expect("face declares a cap height") as f32
            / face.units_per_em() as f32;
        assert_eq!(cap_height, ATTRIBUTION_CAP_HEIGHT);
    }

    // ── Bookmarks ─────────────────────────────────────────────────────────────

    fn render(layout: &str) -> String {
        let xml = format!(r#"<lpdf version="1"><document><section><layout>{layout}</layout></section></document></lpdf>"#);
        let bytes = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).expect("renders");
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// The number and the dictionary of the object whose text contains `needle`.
    fn object_with(pdf: &str, needle: &str) -> (u32, String) {
        let at = pdf.find(needle).unwrap_or_else(|| panic!("no {needle} in the PDF"));
        let start = pdf[..at].rfind(" 0 obj").expect("inside an object");
        let number_start = pdf[..start].rfind('\n').map_or(0, |i| i + 1);
        let number = pdf[number_start..start].trim().parse().expect("an object number");
        let end = at + pdf[at..].find("endobj").expect("an object end");
        (number, pdf[start..end].to_string())
    }

    const THREE: &str = r#"<bookmark label="Methods"/><text>Methods</text>
        <bookmark label="Data" level="2"/><text>Data</text>
        <bookmark label="Results"/><text>Results</text>"#;

    #[test]
    fn bookmarks_make_an_outline_nested_by_level() {
        let pdf = render(THREE);
        let (methods, methods_dict) = object_with(&pdf, "/Title (Methods)");
        let (data, data_dict) = object_with(&pdf, "/Title (Data)");
        let (results, results_dict) = object_with(&pdf, "/Title (Results)");
        let (_, root) = object_with(&pdf, "/Type /Outlines");

        assert!(root.contains(&format!("/First {methods} 0 R")) && root.contains(&format!("/Last {results} 0 R")), "{root}");
        assert!(root.contains("/Count 3"), "{root}");
        assert!(methods_dict.contains(&format!("/First {data} 0 R")) && methods_dict.contains("/Count 1"), "{methods_dict}");
        assert!(methods_dict.contains(&format!("/Next {results} 0 R")), "{methods_dict}");
        assert!(data_dict.contains(&format!("/Parent {methods} 0 R")) && !data_dict.contains("/Next"), "{data_dict}");
        assert!(results_dict.contains(&format!("/Prev {methods} 0 R")), "{results_dict}");
        assert!(methods_dict.contains("/Dest /methods"), "{methods_dict}");
    }

    #[test]
    fn catalog_names_the_outline_and_its_destinations() {
        let pdf = render(THREE);
        let (_, catalog) = object_with(&pdf, "/Type /Catalog");
        assert!(catalog.contains("/Outlines") && catalog.contains("/Dests"), "{catalog}");
        assert!(catalog.contains("/PageMode /UseOutlines"), "{catalog}");
        let (_, dests) = object_with(&pdf, "/methods [");
        assert!(dests.contains("/data [") && dests.contains("/results ["), "{dests}");
        assert!(dests.contains("/XYZ"), "{dests}");
    }

    #[test]
    fn link_to_a_bookmark_id_goes_to_its_destination() {
        let pdf = render(r##"<link href="#results"><text>See results</text></link>
            <text>Or <span href="#methods">the methods</span></text>
            <bookmark label="Methods"/><text>Methods</text>
            <bookmark label="Results"/><text>Results</text>"##);
        let (_, link) = object_with(&pdf, "/D /results");
        assert!(link.contains("/S /GoTo"), "{link}");
        let (_, span) = object_with(&pdf, "/D /methods");
        assert!(span.contains("/S /GoTo"), "{span}");
    }

    #[test]
    fn link_to_an_id_no_bookmark_has_is_left_out() {
        let pdf = render(r##"<link href="#nowhere"><text>Nowhere</text></link><link href="#"><text>Here</text></link>"##);
        assert!(!pdf.contains("/GoTo"));
        assert!(!pdf.contains("/Outlines"));
    }

    #[test]
    fn document_without_bookmarks_has_no_outline() {
        let pdf = render("<text>Plain</text>");
        assert!(!pdf.contains("/Outlines") && !pdf.contains("/Dests") && !pdf.contains("/PageMode"));
    }

    // ── Layer opacity ─────────────────────────────────────────────────────────

    /// Two shapes that overlap: a red square, and a blue one over its corner.
    const OVERLAPPING: &str = r##"<rect x="40pt" y="40pt" width="100pt" height="100pt" fill="#ff0000"/>
        <rect x="90pt" y="90pt" width="100pt" height="100pt" fill="#0000ff"/>"##;

    /// The PDF of a section that holds only the given layers.
    fn render_layers(layers: &str) -> Vec<u8> {
        let xml = format!(r#"<lpdf version="1"><document><section>{layers}</section></document></lpdf>"#);
        crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).expect("renders")
    }

    /// Each stream of a PDF with its object number, the text of its object up to the stream, and its bytes once inflated.
    fn streams(pdf: &[u8]) -> Vec<(u32, String, String)> {
        const START: &[u8] = b">>\nstream\n";
        const END: &[u8] = b"\nendstream";
        let find = |from: usize, needle: &[u8]| pdf[from..].windows(needle.len()).position(|w| w == needle).map(|i| from + i);
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(at) = find(from, START) {
            let start = at + START.len();
            let end = find(start, END).expect("a stream end");
            let obj = pdf[..at].windows(6).rposition(|w| w == b" 0 obj").expect("inside an object");
            let line = pdf[..obj].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            let number = String::from_utf8_lossy(&pdf[line..obj]).trim().parse().expect("an object number");
            let dict = String::from_utf8_lossy(&pdf[obj..start]).into_owned();
            if let Ok(inflated) = decompress_to_vec_zlib(&pdf[start..end]) {
                found.push((number, dict, String::from_utf8_lossy(&inflated).into_owned()));
            }
            from = end;
        }
        found
    }

    /// The page's content stream, and the dictionary and content of each transparency group.
    fn page_and_groups(pdf: &[u8]) -> (String, Vec<(String, String)>) {
        let text = String::from_utf8_lossy(pdf);
        let (_, page) = object_with(&text, "/Type /Page\n");
        let contents: u32 = page.split("/Contents ").nth(1).and_then(|rest| rest.split_whitespace().next())
            .and_then(|number| number.parse().ok()).expect("a /Contents entry");
        let streams = streams(pdf);
        let content = streams.iter().find(|(number, ..)| *number == contents).expect("the content stream").2.clone();
        let groups = streams.into_iter()
            .filter(|(_, dict, _)| dict.contains("/S /Transparency"))
            .map(|(_, dict, group)| (dict, group))
            .collect();
        (content, groups)
    }

    fn rectangles(content: &str) -> usize {
        content.lines().filter(|line| line.ends_with(" re")).count()
    }

    #[test]
    fn a_faded_layer_is_drawn_as_one_transparency_group() {
        let pdf = render_layers(&format!(r#"<layer opacity="0.5">{OVERLAPPING}</layer>"#));
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 1);
        let (dict, group) = &groups[0];
        assert!(dict.contains("/Subtype /Form") && dict.contains("/I true"), "{dict}");
        assert_eq!(rectangles(group), 2, "both shapes are drawn into the group");
        assert!(page.contains("/GS50 gs") && page.contains("/G0 Do"), "the page fades the group: {page}");
        assert_eq!(rectangles(&page), 0, "the shapes are not drawn on the page");
    }

    #[test]
    fn two_faded_layers_are_two_groups() {
        let pdf = render_layers(
            r##"<layer opacity="0.5"><rect x="40pt" y="40pt" width="100pt" height="100pt" fill="#ff0000"/></layer>
                <layer opacity="0.3"><rect x="90pt" y="90pt" width="100pt" height="100pt" fill="#0000ff"/></layer>"##,
        );
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 2);
        assert!(page.contains("/GS50 gs") && page.contains("/G0 Do"), "{page}");
        assert!(page.contains("/GS30 gs") && page.contains("/G1 Do"), "{page}");
    }

    /// A PNG of one red pixel.
    const RED_PIXEL: [u8; 69] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn a_faded_layer_draws_an_image_from_the_resources_of_its_group() {
        let xml = r#"<lpdf version="1"><assets><image name="photo" src="photo.png"/></assets><document><section>
            <layer opacity="0.5"><img-at name="photo" x="40pt" y="40pt" width="100pt" height="100pt"/></layer>
            </section></document></lpdf>"#;
        // Built from the modules both crates share: the core library's `bench_render_xml_with_image` is not in the WASI crate.
        let mut doc = crate::parse::parse(xml).expect("parses");
        let pages = crate::layout::layout_document(&doc.section_layouts());
        let mut images = ImageRegistry::new();
        images.load("photo", RED_PIXEL.to_vec());
        let pdf = render_pdf(&pages, &doc.fonts, &FontRegistry::new(), &images, &doc.meta, None, false).expect("renders");
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 1);
        let (dict, group) = &groups[0];
        assert!(group.contains("/I0 Do"), "the image is drawn in the group: {group}");
        assert!(dict.contains("/XObject") && dict.contains("/I0 "), "the group lists the image: {dict}");
        assert!(!page.contains("/I0 Do"), "{page}");
    }

    #[test]
    fn a_layer_at_full_opacity_is_drawn_on_the_page() {
        let pdf = render_layers(&format!(r#"<layer>{OVERLAPPING}</layer>"#));
        let (page, groups) = page_and_groups(&pdf);
        assert!(groups.is_empty());
        assert_eq!(rectangles(&page), 2);
    }

    #[test]
    fn a_layer_whose_opacity_rounds_to_zero_draws_nothing() {
        let shown = r##"<layer><rect x="10pt" y="10pt" width="50pt" height="50pt" fill="#00ff00"/></layer>"##;
        let without = render_layers(shown);
        for opacity in ["0", "0.004"] {
            let hidden = format!(r#"<layer opacity="{opacity}">{OVERLAPPING}</layer>"#);
            assert!(render_layers(&format!("{shown}{hidden}")) == without, "opacity {opacity} shows something");
        }
    }

    #[test]
    fn every_opacity_a_page_sets_is_defined_in_its_resources() {
        for opacity in ["0", "0.004", "0.5", "0.996", "1"] {
            let pdf = render_layers(&format!(r#"<layer opacity="{opacity}">{OVERLAPPING}</layer>"#));
            let (page, _) = page_and_groups(&pdf);
            let resources = object_with(&String::from_utf8_lossy(&pdf), "/Type /Page\n").1;
            for state in page.lines().filter_map(|line| line.strip_suffix(" gs")) {
                assert!(resources.contains(&format!("{state} ")), "opacity {opacity}: the page sets {state}, which its resources do not define: {resources}");
            }
        }
    }

    #[test]
    fn a_layer_at_almost_full_opacity_is_drawn_on_the_page() {
        let (page, groups) = page_and_groups(&render_layers(&format!(r#"<layer opacity="0.996">{OVERLAPPING}</layer>"#)));
        assert!(groups.is_empty());
        assert_eq!(rectangles(&page), 2);
    }

    #[test]
    fn a_faded_layer_keeps_its_transform_and_clip_inside_the_group() {
        let pdf = render_layers(&format!(r#"<layer opacity="0.5" transform="translate(10 20)">{OVERLAPPING}</layer>"#));
        let (page, groups) = page_and_groups(&pdf);
        assert!(groups[0].1.contains(" cm"), "the transform is applied to the group's content: {}", groups[0].1);
        assert!(!page.contains(" cm"), "{page}");
    }

    // ── Box opacity ───────────────────────────────────────────────────────────

    /// The PDF of a section whose layout holds `body`.
    fn render_layout(body: &str) -> Vec<u8> {
        let xml = format!(r#"<lpdf version="1"><document><section><layout>{body}</layout></section></document></lpdf>"#);
        crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).expect("renders")
    }

    fn texts(content: &str) -> usize {
        content.lines().filter(|line| *line == "BT").count()
    }

    #[test]
    fn a_faded_box_is_drawn_as_one_transparency_group() {
        let pdf = render_layout(r##"<column opacity="0.5" background="#ff0000"><text>One</text><text>Two</text></column>"##);
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 1);
        let (dict, group) = &groups[0];
        assert!(dict.contains("/Subtype /Form") && dict.contains("/I true"), "{dict}");
        assert_eq!((rectangles(group), texts(group)), (1, 2), "the fill and both texts are drawn into the group: {group}");
        assert!(page.contains("/GS50 gs") && page.contains("/G0 Do"), "the page fades the group: {page}");
        assert!(rectangles(&page) == 0 && !page.contains("(One)") && !page.contains("(Two)"), "nothing of the box is drawn on the page: {page}");
    }

    #[test]
    fn a_box_at_full_opacity_is_drawn_on_the_page() {
        for opacity in [r#"opacity="1""#, ""] {
            let pdf = render_layout(&format!(r##"<column {opacity} background="#ff0000"><text>One</text></column>"##));
            let (page, groups) = page_and_groups(&pdf);
            assert!(groups.is_empty());
            assert!(rectangles(&page) == 1 && page.contains("(One)"), "{page}");
        }
    }

    #[test]
    fn a_box_at_zero_opacity_draws_nothing() {
        let pdf = render_layout(r##"<column opacity="0" background="#ff0000"><text>Hidden</text></column>"##);
        let (page, groups) = page_and_groups(&pdf);
        assert!(groups.is_empty());
        assert!(rectangles(&page) == 0 && !page.contains("(Hidden)"), "{page}");
    }

    #[test]
    fn a_faded_box_in_a_faded_box_is_a_group_in_its_group() {
        let pdf = render_layout(
            r##"<column opacity="0.5" background="#ff0000"><column opacity="0.3" background="#0000ff"><text>In</text></column></column>"##,
        );
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 2);
        let (inner_dict, inner) = &groups[0];
        let (outer_dict, outer) = &groups[1];
        assert_eq!((rectangles(inner), texts(inner)), (1, 1), "the inner box is drawn into the inner group: {inner}");
        assert_eq!((rectangles(outer), texts(outer)), (1, 0), "the outer group has its own fill: {outer}");
        assert!(outer.contains("/GS30 gs") && outer.contains("/G0 Do"), "the outer group fades the inner one: {outer}");
        assert!(page.contains("/GS50 gs") && page.contains("/G1 Do") && !page.contains("/G0 Do"), "the page draws the outer group only: {page}");
        assert!(outer_dict.contains("/G0 "), "the outer group names the group it draws: {outer_dict}");
        assert!(!inner_dict.contains("/G0 ") && !inner_dict.contains("/G1 "), "the inner group draws no group: {inner_dict}");
    }

    #[test]
    fn two_boxes_side_by_side_do_not_name_each_others_group() {
        let pdf = render_layout(r#"<column opacity="0.5"><text>A</text></column><column opacity="0.5"><text>B</text></column>"#);
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 2);
        assert!(page.contains("/G0 Do") && page.contains("/G1 Do"), "{page}");
        for (dict, _) in &groups {
            assert!(!dict.contains("/G0 ") && !dict.contains("/G1 "), "a group names none: {dict}");
        }
    }

    #[test]
    fn every_opacity_a_box_sets_is_defined_in_the_resources_of_what_sets_it() {
        let pdf = render_layout(
            r#"<column opacity="0.5"><column opacity="0.3"><text>In</text></column></column><column opacity="0.7"><text>Beside</text></column>"#,
        );
        let (page, groups) = page_and_groups(&pdf);
        let text = String::from_utf8_lossy(&pdf);
        let page_resources = object_with(&text, "/Type /Page\n").1;
        for state in page.lines().filter_map(|line| line.strip_suffix(" gs")) {
            assert!(page_resources.contains(&format!("{state} ")), "the page sets {state}: {page_resources}");
        }
        for (dict, content) in &groups {
            for state in content.lines().filter_map(|line| line.strip_suffix(" gs")) {
                assert!(dict.contains(&format!("{state} ")), "a group sets {state}, which its resources do not define: {dict}");
            }
        }
    }

    #[test]
    fn a_faded_box_that_splits_across_pages_is_a_group_on_each() {
        let lines: String = (1..=120).map(|n| format!("<text>Line {n}</text>")).collect();
        let pdf = render_layout(&format!(r#"<column opacity="0.5">{lines}</column>"#));
        let pages = String::from_utf8_lossy(&pdf).matches("/Type /Page\n").count();
        let groups: Vec<String> = streams(&pdf).into_iter().filter(|(_, dict, _)| dict.contains("/S /Transparency")).map(|s| s.2).collect();
        assert!(pages > 1);
        assert_eq!(groups.len(), pages, "a group for each page");
        assert!(groups.iter().all(|group| texts(group) > 0));
    }

    #[test]
    fn a_link_in_a_faded_box_stays_a_link() {
        let pdf = render_layout(r#"<column opacity="0.5"><link href="https://a.example"><text>Go</text></link></column>"#);
        assert!(String::from_utf8_lossy(&pdf).contains("/Subtype /Link"));
    }

    #[test]
    fn a_faded_box_draws_an_image_from_the_resources_of_its_group() {
        let xml = r#"<lpdf version="1"><assets><image name="photo" src="photo.png"/></assets><document><section><layout>
            <column opacity="0.5"><img name="photo" width="100pt" height="100pt"/></column>
            </layout></section></document></lpdf>"#;
        let mut doc = crate::parse::parse(xml).expect("parses");
        let pages = crate::layout::layout_document(&doc.section_layouts());
        let mut images = ImageRegistry::new();
        images.load("photo", RED_PIXEL.to_vec());
        let pdf = render_pdf(&pages, &doc.fonts, &FontRegistry::new(), &images, &doc.meta, None, false).expect("renders");
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].1.contains("/I0 Do"), "the image is drawn in the group: {}", groups[0].1);
        assert!(!page.contains("/I0 Do"), "{page}");
    }
}
