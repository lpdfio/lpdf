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
use pdf_writer::types::{ActionType, AnnotationFlags, AnnotationType, CheckBoxState, CidFontType, FieldFlags, FieldType, FontFlags, FunctionShadingType, HighlightEffect, LineCapStyle, LineJoinStyle, OutputIntentSubtype, PageMode, Predictor, TabOrder, TableHeaderScope, TextRenderingMode};
use md5::{Digest, Md5};
use ttf_parser::{Face, Permissions};
use miniz_oxide::deflate::compress_to_vec_zlib;
use miniz_oxide::inflate::decompress_to_vec_zlib;
use subsetter::GlyphRemapper;

use crate::attachments::{self, EmbeddedFile};
use crate::render::{placed_anchors, PlacedAnchor, RenderBox, RenderCanvasLayer, RenderField, RenderNode, RenderPage, RenderedBarcodeKind};
use crate::parse::{FieldKind, Meta, Output, Shadow};
use crate::pdfa::{self, FileDate, PdfA, Standards};
use crate::pdfua::PdfUa;
use crate::tagging::{Kid, Kind, LinkMark, Marks};
use crate::tokens::{FontDef, Gradient, GradientKind, GRADIENT_PREFIX, is_cmyk};

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
    Builtin {
        base_name: &'static str,
        /// Whether the document draws a comparison sign in this font (see `SIGNS`), which then has codes for them.
        signs: bool,
    },
    /// Custom TrueType/OpenType font, subsetted and embedded as a CIDFont with Identity-H encoding, giving full
    /// Unicode support: a Type 2 CIDFont for TrueType outlines, a Type 0 one for CFF outlines (`FontProgram`).
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
        /// Exact advances of the characters drawn, so measuring does not parse the face again.
        /// `None` when the face could not be parsed.
        advances: Option<FaceAdvances>,
    },
}

/// The advances of the characters a document draws in an embedded face, in font units.
struct FaceAdvances {
    units_per_em: f32,
    /// Advance of glyph 0 (.notdef), which stands in for a character the face does not map,
    /// as the encoder writes it.
    notdef: u16,
    advances: HashMap<char, u16>,
}

impl FaceAdvances {
    fn new(face: &Face, used_chars: &HashSet<char>) -> Self {
        let advance = |gid| face.glyph_hor_advance(gid).unwrap_or(0);
        let advances = used_chars.iter()
            .filter_map(|&c| face.glyph_index(c).map(|gid| (c, advance(gid))))
            .collect();
        FaceAdvances {
            units_per_em: face.units_per_em() as f32,
            notdef: advance(ttf_parser::GlyphId(0)),
            advances,
        }
    }

    fn text_width(&self, text: &str, size: f32) -> f32 {
        let scale = size / self.units_per_em;
        text.chars()
            .map(|c| self.advances.get(&c).copied().unwrap_or(self.notdef))
            .map(|advance| advance as f32 * scale)
            .sum()
    }
}

/// The font program an embedded face is written as, which sets the CIDFont it goes under (ISO 32000-1, 9.9).
#[derive(Clone, Copy, PartialEq, Eq)]
enum FontProgram {
    /// TrueType outlines: `/FontFile2` under a `/CIDFontType2`.
    TrueType,
    /// CFF outlines in an OpenType font: `/FontFile3` with `/Subtype /OpenType` under a `/CIDFontType0`.
    OpenTypeCff,
}

impl FontProgram {
    /// The kind of `program`, read from its OpenType header: `OTTO` for CFF outlines. The subsetter writes a CFF2
    /// face's outlines as TrueType ones, under TrueType's header.
    fn of(program: &[u8]) -> Self {
        if program.starts_with(b"OTTO") { FontProgram::OpenTypeCff } else { FontProgram::TrueType }
    }
}

impl PreparedFont {
    /// Width of `text` at `size` pt.
    ///
    /// Built-in fonts use a rough per-character average (0.5 × size) because
    /// we do not have AFM metrics at runtime.  TrueType fonts use real
    /// per-glyph advances from `ttf-parser`.
    fn text_width(&self, text: &str, size: f32) -> f32 {
        match &self.kind {
            PreparedFontKind::Builtin { base_name, .. } => {
                crate::layout::text_width(base_name, text, size)
            }
            PreparedFontKind::Truetype { advances: Some(advances), .. } => advances.text_width(text, size),
            // The face could not be parsed: the same 0.5 × size per character as before.
            PreparedFontKind::Truetype { advances: None, .. } => text.chars().count() as f32 * 0.5 * size,
        }
    }

    /// Encode `text` into the raw byte string required by the content stream.
    ///
    /// - Builtin (Type 1 + WinAnsiEncoding): one Latin-1 byte per character, and
    ///   a code of its own for each comparison sign if the font has them.
    /// - Truetype (CIDFont + Identity-H): two big-endian glyph-ID bytes per
    ///   character, resolved via `ttf-parser`.
    fn encode_text(&self, text: &str) -> Vec<u8> {
        match &self.kind {
            PreparedFontKind::Builtin { signs, .. } => encode_latin1(text, *signs),
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

    /// Whether a space is the one-byte code 32 in this font, which `Tw` adds its gap to. It is in a built-in font; an
    /// embedded font has two-byte codes, which `Tw` doesn't touch.
    fn space_is_code_32(&self) -> bool {
        matches!(self.kind, PreparedFontKind::Builtin { .. })
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

/// A color as the PDF paints it, each component from 0 to 1. The operator names the color space: `rg` and `RG`
/// DeviceRGB, `k` and `K` DeviceCMYK, so the two mix on a page with nothing else to set.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Color {
    Rgb(f32, f32, f32),
    Cmyk(f32, f32, f32, f32),
}

impl Color {
    /// The components, in the order of the color space: what a shading's functions take.
    fn components(self) -> Vec<f32> {
        match self {
            Color::Rgb(r, g, b)     => vec![r, g, b],
            Color::Cmyk(c, m, y, k) => vec![c, m, y, k],
        }
    }
}

/// Parse a normalised color (`tokens::normalize_color`): `#rrggbb` or `#rgb`, or `cmyk(C, M, Y, K)` with values from
/// 0 to 100. Colors are checked when the document is read, so a malformed one here is black, as in `parse_hex`.
fn parse_color(color: &str) -> Color {
    let Some(values) = color.strip_prefix("cmyk(").and_then(|rest| rest.strip_suffix(')')) else {
        let (r, g, b) = parse_hex(color);
        return Color::Rgb(r, g, b);
    };
    let inks: Vec<f32> = values.split(',').map(|value| value.trim().parse::<f32>().unwrap_or(0.0) / 100.0).collect();
    match inks[..] {
        [c, m, y, k] => Color::Cmyk(c, m, y, k),
        _            => Color::Cmyk(0.0, 0.0, 0.0, 1.0),
    }
}

/// Set the color that fills paint with, in its own color space.
fn set_fill_color(content: &mut Content, color: &str) {
    match parse_color(color) {
        Color::Rgb(r, g, b)     => content.set_fill_rgb(r, g, b),
        Color::Cmyk(c, m, y, k) => content.set_fill_cmyk(c, m, y, k),
    };
}

/// Set the color that strokes paint with, in its own color space.
fn set_stroke_color(content: &mut Content, color: &str) {
    match parse_color(color) {
        Color::Rgb(r, g, b)     => content.set_stroke_rgb(r, g, b),
        Color::Cmyk(c, m, y, k) => content.set_stroke_cmyk(c, m, y, k),
    };
}

// ── Text encoding ─────────────────────────────────────────────────────────────

/// Convert UTF-8 text to WinAnsiEncoding bytes (used by PDF built-in Type 1 fonts).
///
/// Coverage:
/// - `0x00–0x7F`  ASCII, direct.
/// - `0x80–0x9F`  Windows-1252 extension block (em dash, en dash, curly quotes, …).
/// - `0xA0–0xFF`  ISO-8859-1 upper half, direct (same in both encodings).
/// - A comparison sign has the code of `SIGNS` when the font has `signs`: its dictionary names the glyphs.
/// - A character with none of these is drawn as its `look_alike`, if it has one, and is replaced by `?` if not.
fn encode_latin1(text: &str, signs: bool) -> Vec<u8> {
    text.chars()
        .map(|c| {
            win1252_byte(c)
                .or_else(|| sign_code(c).filter(|_| signs))
                .or_else(|| look_alike(c).and_then(win1252_byte))
                .unwrap_or(b'?')
        })
        .collect()
}

/// The comparison signs a built-in font draws although Windows-1252 has no code for them: the glyphs are in the
/// fonts (the Adobe metrics of the Helvetica, Times and Courier families list them), only the encoding lacks
/// them. Each has a code that Windows-1252 leaves empty, and the `/Differences` of a font that draws signs
/// names its glyph. `approxequal` is not in those metrics: a viewer draws it from another font, with no
/// width, so the dictionary has to give one (`write_sign_encoding`).
const SIGNS: [(char, u8, &str); 4] = [
    ('\u{2248}', 0x81, "approxequal"),
    ('\u{2260}', 0x8D, "notequal"),
    ('\u{2264}', 0x8F, "lessequal"),
    ('\u{2265}', 0x90, "greaterequal"),
];

/// How far a sign advances in the Helvetica and Times families, in thousandths of the size, as their metrics
/// have it for the three of them that list one; Courier's characters are all 600.
const SIGN_ADVANCE: u16 = 549;

/// The code a comparison sign has in a font that draws them, if `c` is one.
fn sign_code(c: char) -> Option<u8> {
    SIGNS.iter().find(|(sign, ..)| *sign == c).map(|&(_, code, _)| code)
}

/// How far a comparison sign advances in a font that is not monospace, if `c` is one.
pub(crate) fn sign_advance(c: char) -> Option<u16> {
    sign_code(c).map(|_| SIGN_ADVANCE)
}

/// Whether a built-in font has an encoding that the signs' codes can go in: Symbol and ZapfDingbats have their own.
fn takes_signs(base_name: &str) -> bool {
    !matches!(base_name, "Symbol" | "ZapfDingbats")
}

/// Writes the encoding and the widths of a built-in font that draws the comparison signs: Windows-1252 with
/// the glyph of each sign at its code, and the width of every code, which is what layout measured. A viewer
/// advances by these, so the line comes out as layout placed it, `approxequal` included.
fn write_sign_encoding(font: &mut pdf_writer::writers::Type1Font, base_name: &str) {
    {
        let mut encoding = font.encoding_custom();
        encoding.base_encoding(Name(b"WinAnsiEncoding"));
        let mut differences = encoding.differences();
        for (_, code, glyph) in SIGNS {
            differences.consecutive(code, [Name(glyph.as_bytes())]);
        }
    }
    font.first_char(WIN_ANSI_FIRST)
        .last_char(u8::MAX)
        .widths((WIN_ANSI_FIRST..=u8::MAX).map(|code| sign_encoding_width(base_name, code)));
}

/// The first code of a font's `/Widths`: the space.
const WIN_ANSI_FIRST: u8 = 32;

/// The width of a code, in thousandths of the size, in a font that draws the signs: what a viewer advanced by
/// before (the Adobe metrics), and the sign's own for the four signs.
fn sign_encoding_width(base_name: &str, code: u8) -> f32 {
    let is_sign = SIGNS.iter().any(|&(_, sign, _)| sign == code);
    let advance = if is_sign && !base_name.contains("Courier") {
        Some(SIGN_ADVANCE)
    } else {
        crate::layout::afm_code_advance(base_name, code)
    };
    f32::from(advance.unwrap_or(0))
}

/// The ASCII character a built-in font draws for a character it has no glyph for, where one looks much like it: the
/// lines and corners of a drawn box, arrows, primes, the minus sign and the wide spaces. An embedded font draws
/// the characters as they are.
pub(crate) fn look_alike(c: char) -> Option<char> {
    Some(match c {
        // Box drawing: a straight line is - or |, and a corner, a tee or a cross is +.
        '\u{2500}' | '\u{2501}' | '\u{2504}' | '\u{2505}' | '\u{2508}' | '\u{2509}' | '\u{254C}' | '\u{254D}'
        | '\u{2550}' | '\u{2574}' | '\u{2576}' | '\u{2578}' | '\u{257A}' | '\u{257C}' | '\u{257E}' => '-',
        '\u{2502}' | '\u{2503}' | '\u{2506}' | '\u{2507}' | '\u{250A}' | '\u{250B}' | '\u{254E}' | '\u{254F}'
        | '\u{2551}' | '\u{2575}' | '\u{2577}' | '\u{2579}' | '\u{257B}' | '\u{257D}' | '\u{257F}' => '|',
        '\u{250C}'..='\u{254B}' | '\u{2552}'..='\u{2570}' => '+',
        '\u{2571}' => '/',
        '\u{2572}' => '\\',
        '\u{2573}' => 'X',
        // Arrows and pointers.
        '\u{2192}' | '\u{21D2}' | '\u{2794}' | '\u{279C}' | '\u{27A1}' | '\u{27F6}' | '\u{27F9}'
        | '\u{25B6}' | '\u{25B8}' | '\u{25BA}' => '>',
        '\u{2190}' | '\u{21D0}' | '\u{27F5}' | '\u{27F8}' | '\u{25C0}' | '\u{25C2}' | '\u{25C4}' => '<',
        '\u{2191}' | '\u{21D1}' | '\u{25B2}' | '\u{25B4}' => '^',
        '\u{2193}' | '\u{21D3}' | '\u{25BC}' | '\u{25BE}' => 'v',
        // Primes, hyphens and the minus sign, and the spaces of other widths.
        '\u{2032}' => '\'',
        '\u{2033}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2212}' => '-',
        '\u{2002}'..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}' => ' ',
        _ => return None,
    })
}

/// Map a Unicode scalar to its Windows-1252 / WinAnsiEncoding byte value.
pub(crate) fn win1252_byte(c: char) -> Option<u8> {
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
    /// The link element of a tagged PDF that holds the annotation, and the number it lists it by.
    mark: Option<LinkMark>,
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

/// Recursively collect every font the nodes draw in: a text's own font, and Helvetica for the caption of a
/// barcode and for a form field, whose appearance and `/DA` name it as `Helv`.
fn collect_used_fonts(nodes: &[RenderNode], out: &mut HashSet<String>) {
    for node in nodes {
        match node {
            RenderNode::Text(t) => { out.insert(t.font.clone()); }
            RenderNode::Barcode(bc) if has_caption(&bc.kind) => { out.insert("Helvetica".to_string()); }
            RenderNode::Field(_) => { out.insert("Helvetica".to_string()); }
            RenderNode::Box(b)  => collect_used_fonts(&b.children, out),
            RenderNode::Link(l) => collect_used_fonts(&l.children, out),
            RenderNode::CanvasLayer(l) => collect_used_fonts(&l.children, out),
            _ => {}
        }
    }
}

/// Whether a barcode draws its human-readable text under the bars.
fn has_caption(kind: &RenderedBarcodeKind) -> bool {
    matches!(kind, RenderedBarcodeKind::Code128 { hrt: Some(_), .. } | RenderedBarcodeKind::Ean13 { hrt: true, .. })
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
                builtin_font(ps, used_chars)
            }
            FontDef::Ref(_) => {
                if let Some(bytes) = registry.get(name) {
                    return prepare_truetype_font(bytes, used_chars);
                }
                // Bytes not provided at render time → degrade gracefully.
                builtin_font("Helvetica", used_chars)
            }
        }
    } else {
        let ps = pdf_builtin_name(name).unwrap_or("Helvetica");
        builtin_font(ps, used_chars)
    }
}

/// A built-in font that draws the comparison signs when the document uses one in it, so that a document with
/// none writes the font as it always did.
fn builtin_font(base_name: &'static str, used_chars: &HashSet<char>) -> PreparedFontKind {
    let signs = takes_signs(base_name) && used_chars.iter().any(|&c| sign_code(c).is_some());
    PreparedFontKind::Builtin { base_name, signs }
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
                advances:        None,
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

    // Subset the font at its default instance.  `subset` refuses CFF2 outlines, for which PDF has no font program;
    // this turns them into TrueType ones, and subsets TrueType and CFF outlines as `subset` does.  Fall back to full
    // bytes on error.
    let (subsetted_bytes, use_remap) = match subsetter::subset_with_variations(bytes, 0, &[], &remapper) {
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
        advances: Some(FaceAdvances::new(&face, used_chars)),
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
/// What drawing looks up by name: the fonts, the resource name of each image, and the gradients of the tokens.
#[derive(Clone, Copy)]
struct DrawResources<'a> {
    fonts:         &'a HashMap<String, PreparedFont>,
    image_res_map: &'a HashMap<String, String>,
    gradients:     &'a HashMap<String, Gradient>,
}

fn draw_nodes(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    marks:         &mut Marks,
    nodes:         &[RenderNode],
    res:           &DrawResources,
    page_h:        f32,
) {
    for node in nodes {
        draw_node(content, annots, groups, marks, node, res, page_h);
    }
}

/// Draw `node`, and in a tagged PDF mark what it is (`tagging`): a line of text is content of the element it is in,
/// a figure is marked as a whole, a box with a tag is an element that what it holds goes in, and what is drawn and
/// not read, such as a rule or a shape, is an artifact. In a PDF that is not tagged `marks` is off and adds nothing.
fn draw_node(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    marks:         &mut Marks,
    node:          &RenderNode,
    res:           &DrawResources,
    page_h:        f32,
) {
    if marks.marking() {
        let mut draw = |content: &mut Content, marks: &mut Marks| draw_drawn(content, annots, groups, marks, node, res, page_h);
        match node {
            RenderNode::Text(t) => return marks.content(content, &t.content, draw),
            RenderNode::Line(_) | RenderNode::CanvasRect(_) | RenderNode::CanvasLine(_) | RenderNode::CanvasEllipse(_)
                | RenderNode::CanvasPath(_) => return marks.artifact(content, draw),
            RenderNode::Image(i) => return marks.whole(content, i.tag.as_deref(), draw),
            RenderNode::Barcode(bc) => return marks.whole(content, bc.tag.as_deref(), draw),
            RenderNode::CanvasLayer(l) if l.tag.is_some() => return marks.whole(content, l.tag.as_deref(), draw),
            RenderNode::Box(b) => {
                if let Some(tag) = b.tag.as_deref() {
                    if marks.is_whole(tag) {
                        return marks.whole(content, Some(tag), draw);
                    }
                    let entered = marks.enter(tag);
                    draw(content, marks);
                    return marks.leave(entered);
                }
            }
            _ => {}
        }
    }
    draw_drawn(content, annots, groups, marks, node, res, page_h);
}

/// Draw `node` itself; what it holds goes through `draw_node`.
fn draw_drawn(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    marks:         &mut Marks,
    node:          &RenderNode,
    res:           &DrawResources,
    page_h:        f32,
) {
    let DrawResources { fonts, image_res_map, gradients } = *res;
    match node {
        // ── Box ──────────────────────────────────────────────────────────────
        RenderNode::Box(b) => {
            let percent = opacity_percent(b.opacity);
            if percent >= 100 {
                draw_box(content, annots, groups, marks, b, res, page_h);
            } else {
                draw_faded(content, groups, marks, percent, |content, groups, marks| {
                    draw_box(content, annots, groups, marks, b, res, page_h);
                });
            }
        }

        // ── Line ─────────────────────────────────────────────────────────────
        RenderNode::Line(l) => {
            content.save_state();
            set_stroke_color(content, &l.color);
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
            let font = fonts.get(&t.font).expect("prepare_fonts prepares the font of every text node");

            // node.x is an alignment anchor; compute the true left edge.
            let text_w  = font.text_width(&t.content, t.size);
            let draw_x  = match t.text_align.as_str() {
                "right"  => t.x - text_w,
                "center" => t.x - text_w / 2.0,
                // "left" and "justify" — anchor is already the left edge.
                _        => t.x,
            };

            // Layout y = top of the text block; PDF baseline = top − font-size.
            let pdf_y   = page_h - t.y - t.size;
            let encoded = font.encode_text(&t.content);
            let rname   = font.resource_name.as_bytes().to_vec();

            content.begin_text();
            set_fill_color(content, &t.color);
            content.set_font(Name(&rname), t.size);
            // set_text_matrix positions the text origin precisely.
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, draw_x, pdf_y]);
            if t.word_gap > 0.0 && font.space_is_code_32() {
                // Layout placed the line's words; the gap it worked out goes in at each space. In a built-in
                // font that is the one-byte code 32, so the line stays one string with the gap in `Tw`. Layout
                // splits words at any whitespace, so the spaces of a justified line are the gaps it counted.
                content.set_word_spacing(t.word_gap);
                content.show(Str(&encoded));
                content.set_word_spacing(0.0);
            } else if t.word_gap > 0.0 {
                // `Tw` would not do: it moves only the one-byte code 32, and an embedded font has two-byte codes.
                // The gap goes in before each space instead, and a positive adjustment moves back, in thousandths
                // of the font size.
                let adjustment = -t.word_gap * 1000.0 / t.size;
                let mut pieces = t.content.split(' ');
                let mut positioned = content.show_positioned();
                let mut items = positioned.items();
                items.show(Str(&font.encode_text(pieces.next().unwrap_or(""))));
                for piece in pieces {
                    items.adjust(adjustment);
                    items.show(Str(&font.encode_text(&format!(" {piece}"))));
                }
            } else {
                content.show(Str(&encoded));
            }
            content.end_text();
        }

        // ── Link ─────────────────────────────────────────────────────────────
        RenderNode::Link(lk) => {
            // Draw child nodes first (they appear inside the link area). In a tagged PDF they are drawn in the link's
            // element, which holds the annotation too.
            let link = marks.begin_link();
            draw_nodes(content, annots, groups, marks, &lk.children, res, page_h);
            let mark = marks.end_link(link);

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
                mark,
            });
        }

        // ── Image ────────────────────────────────────────────────────────────
        RenderNode::Image(img) => {
            if let Some(res_name) = image_res_map.get(&img.name) {
                // PDF images are painted with a CTM that scales a 1×1 unit
                // image to the desired width/height, positioned at (x, pdf_y).
                let pdf_y = page_h - img.y - img.height;
                content.save_state();
                // Rounded corners: only what is inside the rounded rectangle is painted.
                if img.radius > 0.0 {
                    rounded_rect(content, img.x, pdf_y, img.width, img.height, img.radius);
                    content.clip_nonzero();
                    content.end_path();
                }
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
                let pdf_y = page_h - bc.y - bc.height;
                set_fill_color(content, bg);
                content.rect(bc.x, pdf_y, bc.width, bc.height);
                content.fill_nonzero();
            }

            set_fill_color(content, &bc.color);

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

        // Anchors become the outline and the named destinations; they draw nothing, and neither does the mark of a
        // footnote's number.
        RenderNode::Anchor(_) | RenderNode::FootnoteMark(_) => {}

        // ── Canvas Rect ───────────────────────────────────────────────────────
        RenderNode::CanvasRect(r) => {
            let has_fill   = r.fill.is_some();
            let has_stroke = r.stroke.is_some() && r.stroke_width > 0.0;
            if let Some(shadow) = &r.shadow {
                let outline = Outline::Rect { x: r.x, y: page_h - r.y - r.h, w: r.w, h: r.h, radius: r.border_radius };
                draw_shadow(content, groups, marks, shadow, &outline, has_fill);
            }
            let pdf_y = page_h - r.y - r.h;
            if let Some((name, gradient)) = r.fill.as_deref().and_then(|fill| gradient_of(fill, gradients)) {
                fill_with_gradient(content, name, gradient, (r.x, pdf_y, r.w, r.h), false, |content| {
                    if r.border_radius > 0.0 {
                        rounded_rect(content, r.x, pdf_y, r.w, r.h, r.border_radius);
                    } else {
                        content.rect(r.x, pdf_y, r.w, r.h);
                    }
                });
            }
            let solid_fill = has_fill && !r.fill.as_deref().is_some_and(is_gradient_fill);
            if solid_fill || has_stroke {
                content.save_state();
                if solid_fill {
                    set_fill_color(content, r.fill.as_deref().unwrap_or("#000000"));
                }
                if has_stroke {
                    set_stroke_color(content, r.stroke.as_deref().unwrap_or("#000000"));
                    content.set_line_width(r.stroke_width);
                    if let Some(dash) = &r.stroke_dash {
                        content.set_dash_pattern(dash.iter().copied(), 0.0);
                    }
                }
                if r.border_radius > 0.0 {
                    rounded_rect(content, r.x, pdf_y, r.w, r.h, r.border_radius);
                } else {
                    content.rect(r.x, pdf_y, r.w, r.h);
                }
                match (solid_fill, has_stroke) {
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
            content.save_state();
            set_stroke_color(content, &l.stroke);
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
            if let Some(shadow) = &e.shadow {
                let outline = Outline::Ellipse { cx: e.cx, cy: page_h - e.cy, rx: e.rx, ry: e.ry };
                draw_shadow(content, groups, marks, shadow, &outline, has_fill);
            }
            if let Some((name, gradient)) = e.fill.as_deref().and_then(|fill| gradient_of(fill, gradients)) {
                let bounds = (e.cx - e.rx, page_h - e.cy - e.ry, 2.0 * e.rx, 2.0 * e.ry);
                fill_with_gradient(content, name, gradient, bounds, false, |content| {
                    ellipse_path(content, e.cx, page_h - e.cy, e.rx, e.ry);
                });
            }
            let solid_fill = has_fill && !e.fill.as_deref().is_some_and(is_gradient_fill);
            if solid_fill || has_stroke {
                content.save_state();
                if solid_fill {
                    set_fill_color(content, e.fill.as_deref().unwrap_or("#000000"));
                }
                if has_stroke {
                    set_stroke_color(content, e.stroke.as_deref().unwrap_or("#000000"));
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
                match (solid_fill, has_stroke) {
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
            if let Some((name, gradient)) = p.fill.as_deref().and_then(|fill| gradient_of(fill, gradients)) {
                let mut bounds = PathBounds::default();
                emit_svg_path(&mut bounds, &p.d, page_h);
                if let Some(bounds) = bounds.rect() {
                    fill_with_gradient(content, name, gradient, bounds, p.fill_rule_evenodd, |content| {
                        emit_svg_path(content, &p.d, page_h);
                    });
                }
            }
            let solid_fill = has_fill && !p.fill.as_deref().is_some_and(is_gradient_fill);
            if solid_fill || has_stroke {
                content.save_state();
                if solid_fill {
                    set_fill_color(content, p.fill.as_deref().unwrap_or("#000000"));
                }
                if has_stroke {
                    set_stroke_color(content, p.stroke.as_deref().unwrap_or("#000000"));
                    content.set_line_width(p.stroke_width);
                    if p.line_cap > 0 { content.set_line_cap(u8_to_line_cap(p.line_cap)); }
                    if p.line_join > 0 { content.set_line_join(u8_to_line_join(p.line_join)); }
                    if let Some(dash) = &p.stroke_dash {
                        content.set_dash_pattern(dash.iter().copied(), 0.0);
                    }
                }
                emit_svg_path(content, &p.d, page_h);
                match (solid_fill, has_stroke, p.fill_rule_evenodd) {
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
                draw_layer(content, annots, groups, marks, layer, res, page_h);
                content.restore_state();
            } else {
                draw_faded(content, groups, marks, percent, |content, groups, marks| {
                    draw_layer(content, annots, groups, marks, layer, res, page_h);
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
    marks:         &mut Marks,
    b:             &RenderBox,
    res:           &DrawResources,
    page_h:        f32,
) {
    let gradients = res.gradients;
    let has_fill   = b.fill.is_some();
    let has_border = b.border_width > 0.0 && b.border_color.is_some();
    // A gradient is painted ahead of the border; only a color is left to the fill operator below.
    let solid_fill = has_fill && !b.fill.as_deref().is_some_and(is_gradient_fill);

    // Flip the box y from top-down to bottom-up.
    let pdf_y = page_h - b.y - b.height;

    // The box's shadow, background and border are drawn, not read: in a tagged PDF each is an artifact.
    if let Some(shadow) = &b.shadow {
        let outline = Outline::Rect { x: b.x, y: pdf_y, w: b.width, h: b.height, radius: b.radius };
        marks.artifact(content, |content, marks| draw_shadow(content, groups, marks, shadow, &outline, has_fill));
    }

    if let Some((name, gradient)) = b.fill.as_deref().and_then(|fill| gradient_of(fill, gradients)) {
        marks.artifact(content, |content, _| {
            fill_with_gradient(content, name, gradient, (b.x, pdf_y, b.width, b.height), false, |content| {
                if b.radius > 0.0 {
                    rounded_rect(content, b.x, pdf_y, b.width, b.height, b.radius);
                } else {
                    content.rect(b.x, pdf_y, b.width, b.height);
                }
            });
        });
    }

    if solid_fill || has_border {
        marks.artifact(content, |content, _| draw_box_paint(content, b, pdf_y, solid_fill, has_border));
    }

    draw_nodes(content, annots, groups, marks, &b.children, res, page_h);
}

/// The fill and the border of a box.
fn draw_box_paint(content: &mut Content, b: &RenderBox, pdf_y: f32, solid_fill: bool, has_border: bool) {
    content.save_state();

    if solid_fill {
        set_fill_color(content, b.fill.as_deref().unwrap_or("#000000"));
    }
    if has_border {
        set_stroke_color(content, b.border_color.as_deref().unwrap_or("#000000"));
        content.set_line_width(b.border_width);
    }

    if b.radius > 0.0 {
        rounded_rect(content, b.x, pdf_y, b.width, b.height, b.radius);
    } else {
        content.rect(b.x, pdf_y, b.width, b.height);
    }

    match (solid_fill, has_border) {
        (true,  true)  => { content.fill_nonzero_and_stroke(); }
        (true,  false) => { content.fill_nonzero(); }
        (false, true)  => { content.stroke(); }
        (false, false) => { content.end_path(); }
    }

    content.restore_state();
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
///
/// In a tagged PDF the group is a content stream of its own, which its marked content is counted in (`Marks::begin_group`).
fn draw_faded(
    content: &mut Content,
    groups:  &mut Vec<PageGroup>,
    marks:   &mut Marks,
    percent: u32,
    draw:    impl FnOnce(&mut Content, &mut Vec<PageGroup>, &mut Marks),
) {
    if percent == 0 {
        return;
    }
    let first_nested = groups.len();
    let mut group = Content::new();
    let back = marks.begin_group();
    draw(&mut group, groups, marks);
    let nested = first_nested..groups.len();
    marks.end_group(back, groups.len());
    let name = group_resource_name(groups.len());
    groups.push(PageGroup { content: group.finish(), nested });

    content.save_state();
    content.set_parameters(Name(opacity_gs_name(percent).as_bytes()));
    content.x_object(Name(name.as_bytes()));
    content.restore_state();
}

/// The outline of a box or a canvas shape in PDF coordinates, which a shadow is a copy of.
enum Outline {
    /// `x`, `y` is the lower-left corner.
    Rect { x: f32, y: f32, w: f32, h: f32, radius: f32 },
    /// `cx`, `cy` is the centre.
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32 },
}

impl Outline {
    /// Append the outline to the current path, moved by `dx` and `dy`.
    fn path(&self, content: &mut Content, dx: f32, dy: f32) {
        match *self {
            Outline::Rect { x, y, w, h, radius } => rounded_rect(content, x + dx, y + dy, w, h, radius),
            Outline::Ellipse { cx, cy, rx, ry }  => ellipse_path(content, cx + dx, cy + dy, rx, ry),
        }
    }

    /// The smallest rectangle around the outline: its lower-left corner, width and height.
    fn bounds(&self) -> (f32, f32, f32, f32) {
        match *self {
            Outline::Rect { x, y, w, h, .. } => (x, y, w, h),
            Outline::Ellipse { cx, cy, rx, ry } => (cx - rx, cy - ry, 2.0 * rx, 2.0 * ry),
        }
    }
}

/// A drop shadow: the outline moved by the shadow's offset and filled with its color, drawn before the shape so the
/// shape covers the part of it that lies behind. `covers` is whether the shape has a fill to do that. Without
/// one the shape is see-through, and the shadow is drawn only outside it, as CSS does.
///
/// A shadow with an opacity below 1 is drawn as a transparency group of its own (`draw_faded`), so it fades alone.
/// Inside a faded box it is part of that box's group, and fades with it.
fn draw_shadow(
    content: &mut Content,
    groups:  &mut Vec<PageGroup>,
    marks:   &mut Marks,
    shadow:  &Shadow,
    outline: &Outline,
    covers:  bool,
) {
    // The offset is down on the page and the outline is bottom-up.
    let (dx, dy) = (shadow.dx, -shadow.dy);
    let draw = |content: &mut Content, _: &mut Vec<PageGroup>, _: &mut Marks| {
        content.save_state();
        if !covers {
            // Keep the shadow out of the shape: clip to a rectangle around both with the shape cut out.
            let (x, y, w, h) = outline.bounds();
            let (left, bottom) = (x.min(x + dx) - 1.0, y.min(y + dy) - 1.0);
            let (right, top)   = ((x + w).max(x + w + dx) + 1.0, (y + h).max(y + h + dy) + 1.0);
            content.rect(left, bottom, right - left, top - bottom);
            outline.path(content, 0.0, 0.0);
            content.clip_even_odd();
            content.end_path();
        }
        set_fill_color(content, &shadow.color);
        outline.path(content, dx, dy);
        content.fill_nonzero();
        content.restore_state();
    };
    match opacity_percent(shadow.opacity) {
        100.. => draw(content, groups, marks),
        percent => draw_faded(content, groups, marks, percent, draw),
    }
}

/// The inside of a canvas layer: its transform, its clip and its children. Its opacity is not applied here.
fn draw_layer(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    groups:        &mut Vec<PageGroup>,
    marks:         &mut Marks,
    layer:         &RenderCanvasLayer,
    res:           &DrawResources,
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

    // Apply optional clip rect. Around a flow cut at the foot of a page, in a tagged PDF, the clipping path is an
    // artifact of its own, and the clip holds on after it.
    if let Some(clip) = &layer.clip {
        let clip_pdf_y = page_h - clip.y - clip.h;
        marks.artifact(content, |content, _| {
            if clip.border_radius > 0.0 {
                rounded_rect(content, clip.x, clip_pdf_y, clip.w, clip.h, clip.border_radius);
            } else {
                content.rect(clip.x, clip_pdf_y, clip.w, clip.h);
            }
            content.clip_nonzero();
            content.end_path();
        });
    }

    draw_nodes(content, annots, groups, marks, &layer.children, res, page_h);
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
/// from canvas layers, boxes and shadows. Opacity 1.0 (fully opaque) is excluded.
fn collect_used_opacities(nodes: &[RenderNode], out: &mut HashSet<u32>) {
    for node in nodes {
        match node {
            RenderNode::CanvasLayer(l) => {
                note_opacity(l.opacity, out);
                collect_used_opacities(&l.children, out);
            }
            RenderNode::Box(b) => {
                note_opacity(b.opacity, out);
                if let Some(s) = &b.shadow { note_opacity(s.opacity, out); }
                collect_used_opacities(&b.children, out);
            }
            RenderNode::CanvasRect(r) => {
                if let Some(s) = &r.shadow { note_opacity(s.opacity, out); }
            }
            RenderNode::CanvasEllipse(e) => {
                if let Some(s) = &e.shadow { note_opacity(s.opacity, out); }
            }
            RenderNode::Link(l) => collect_used_opacities(&l.children, out),
            _                   => {}
        }
    }
}

/// Add an opacity that is drawn through an ExtGState: only a percentage from 1 to 99 is.
fn note_opacity(opacity: f32, out: &mut HashSet<u32>) {
    let pct = opacity_percent(opacity);
    if pct > 0 && pct < 100 { out.insert(pct); }
}

/// The gradients the pages fill with, by name. A page's background counts, as does any box or shape drawn
/// anywhere in the page, in a group or a layer.
fn used_gradients<'a>(pages: &[RenderPage], gradients: &'a HashMap<String, Gradient>) -> Vec<(&'a str, &'a Gradient)> {
    let mut names: HashSet<String> = HashSet::new();
    for page in pages {
        note_gradient(page.background.as_deref(), gradients, &mut names);
        collect_used_gradients(&page.nodes, gradients, &mut names);
    }
    let mut used: Vec<(&str, &Gradient)> = names
        .iter()
        .filter_map(|name| gradients.get_key_value(name.as_str()))
        .map(|(name, gradient)| (name.as_str(), gradient))
        .collect();
    // Sorted, so the objects of a document are numbered the same way every time.
    used.sort_by_key(|(name, _)| *name);
    used
}

fn collect_used_gradients(nodes: &[RenderNode], gradients: &HashMap<String, Gradient>, out: &mut HashSet<String>) {
    for node in nodes {
        match node {
            RenderNode::Box(b) => {
                note_gradient(b.fill.as_deref(), gradients, out);
                collect_used_gradients(&b.children, gradients, out);
            }
            RenderNode::CanvasRect(r)    => note_gradient(r.fill.as_deref(), gradients, out),
            RenderNode::CanvasEllipse(e) => note_gradient(e.fill.as_deref(), gradients, out),
            RenderNode::CanvasPath(p)    => note_gradient(p.fill.as_deref(), gradients, out),
            RenderNode::CanvasLayer(l)   => collect_used_gradients(&l.children, gradients, out),
            RenderNode::Link(l)          => collect_used_gradients(&l.children, gradients, out),
            _                            => {}
        }
    }
}

/// Add the gradient a fill names, if it names one the tokens have.
fn note_gradient(fill: Option<&str>, gradients: &HashMap<String, Gradient>, out: &mut HashSet<String>) {
    if let Some((name, _)) = fill.and_then(|fill| gradient_of(fill, gradients)) {
        out.insert(name.to_string());
    }
}

/// How many function objects a gradient takes: one for two colors, else one per pair of colors and the function
/// that stitches them.
fn gradient_function_count(gradient: &Gradient) -> usize {
    if gradient.stops.len() <= 2 { 1 } else { gradient.stops.len() }
}

/// Write a gradient's functions and its shading. The shading is on the unit axis (0, 0) to (1, 0), or the unit
/// circle, and extends past its ends, so whatever the matrix before `sh` puts over the clip is painted.
fn write_gradient(pdf: &mut Pdf, gradient: &Gradient, ids: &GradientIds) {
    let components = |color: &str| parse_color(color).components();
    let stops = &gradient.stops;
    let pieces = stops.len() - 1;
    if pieces == 1 {
        pdf.exponential_function(ids.function_ids[0])
            .domain([0.0, 1.0])
            .c0(components(&stops[0].color))
            .c1(components(&stops[1].color))
            .n(1.0);
    } else {
        for (piece, pair) in stops.windows(2).enumerate() {
            pdf.exponential_function(ids.function_ids[piece])
                .domain([0.0, 1.0])
                .c0(components(&pair[0].color))
                .c1(components(&pair[1].color))
                .n(1.0);
        }
        pdf.stitching_function(ids.function_ids[pieces])
            .domain([0.0, 1.0])
            .functions(ids.function_ids[..pieces].iter().copied())
            .bounds(stops[1..pieces].iter().map(|stop| stop.offset))
            .encode((0..pieces).flat_map(|_| [0.0, 1.0]));
    }

    let function_id = ids.function_ids[ids.function_ids.len() - 1];
    let mut shading = pdf.function_shading(ids.shading_id);
    // The stops are all CMYK or all RGB (`Tokens::resolve_gradient`).
    if stops.iter().all(|stop| is_cmyk(&stop.color)) {
        shading.color_space().device_cmyk();
    } else {
        shading.color_space().device_rgb();
    }
    match gradient.kind {
        GradientKind::Linear => {
            shading.shading_type(FunctionShadingType::Axial).coords([0.0, 0.0, 1.0, 0.0]);
        }
        GradientKind::Radial => {
            shading.shading_type(FunctionShadingType::Radial).coords([0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        }
    }
    shading.function(function_id).extend([true, true]);
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

// ── Gradients ─────────────────────────────────────────────────────────────────

/// Whether a fill names a gradient of the tokens, as `gradient:name`, and not a color.
fn is_gradient_fill(fill: &str) -> bool {
    fill.starts_with(GRADIENT_PREFIX)
}

/// The gradient a fill names: its name and its definition. `None` for a color, and for a name the tokens lack.
fn gradient_of<'a>(fill: &'a str, gradients: &'a HashMap<String, Gradient>) -> Option<(&'a str, &'a Gradient)> {
    let name = fill.strip_prefix(GRADIENT_PREFIX)?;
    gradients.get(name).map(|gradient| (name, gradient))
}

/// The name a page's resources give the shading of a gradient. The shading is drawn on a unit axis or circle and
/// put onto each shape by a matrix, so a gradient has one shading however many shapes it fills.
fn shading_resource_name(gradient: &str) -> String {
    format!("Sh-{gradient}")
}

/// A value this close to 0 is the rounding of a sine or cosine, and is written as 0.
fn snap_to_zero(value: f32) -> f32 {
    if value.abs() < 1e-6 { 0.0 } else { value }
}

/// The matrix that puts a gradient's unit shading onto the box `(x, y, w, h)`, in PDF coordinates.
///
/// A linear gradient's shading runs from (0, 0) to (1, 0), and a radial gradient's from the center of the unit
/// circle out to its edge.
fn gradient_matrix(gradient: &Gradient, x: f32, y: f32, w: f32, h: f32) -> [f32; 6] {
    let (center_x, center_y) = (x + w / 2.0, y + h / 2.0);
    match gradient.kind {
        GradientKind::Linear => {
            let (sin, cos) = gradient.angle.to_radians().sin_cos();
            let (sin, cos) = (snap_to_zero(sin), snap_to_zero(cos));
            // Angles run from the top of the box, and PDF's y points up.
            let (dx, dy) = (sin, -cos);
            // The line is as long as the box is along it, so the first and last colors reach the corners.
            let length = (w * sin).abs() + (h * cos).abs();
            [
                dx * length, dy * length, -dy * length, dx * length,
                center_x - dx * length / 2.0, center_y - dy * length / 2.0,
            ]
        }
        GradientKind::Radial => {
            // Out to the farthest corner.
            let radius = w.hypot(h) / 2.0;
            [radius, 0.0, 0.0, radius, center_x, center_y]
        }
    }
}

/// Fill a shape with a gradient: clip to the path `outline` appends, then paint the shading over the clip, so the
/// shape keeps its own corners. `bounds` is the box around the shape that the gradient spans, `(x, y, w, h)` in
/// PDF coordinates. A box with no width or height has nothing to paint.
fn fill_with_gradient(
    content:  &mut Content,
    name:     &str,
    gradient: &Gradient,
    bounds:   (f32, f32, f32, f32),
    even_odd: bool,
    outline:  impl FnOnce(&mut Content),
) {
    let (x, y, w, h) = bounds;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    content.save_state();
    outline(content);
    if even_odd {
        content.clip_even_odd();
    } else {
        content.clip_nonzero();
    }
    content.end_path();
    content.transform(gradient_matrix(gradient, x, y, w, h));
    content.shading(Name(shading_resource_name(name).as_bytes()));
    content.restore_state();
}

/// Where `emit_svg_path` sends the segments of a path: a content stream, or a measure of the path.
trait PathSink {
    fn move_to(&mut self, x: f32, y: f32);
    fn line_to(&mut self, x: f32, y: f32);
    fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32);
    fn close_path(&mut self);
}

impl PathSink for Content {
    fn move_to(&mut self, x: f32, y: f32) {
        Content::move_to(self, x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        Content::line_to(self, x, y);
    }

    fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        Content::cubic_to(self, x1, y1, x2, y2, x3, y3);
    }

    fn close_path(&mut self) {
        Content::close_path(self);
    }
}

/// The box around the points of a path, its control points included.
#[derive(Default)]
struct PathBounds {
    /// `(x_min, y_min, x_max, y_max)`, once the path has a point.
    corners: Option<(f32, f32, f32, f32)>,
}

impl PathBounds {
    fn include(&mut self, x: f32, y: f32) {
        self.corners = Some(match self.corners {
            None                          => (x, y, x, y),
            Some((x_min, y_min, x_max, y_max)) => (x_min.min(x), y_min.min(y), x_max.max(x), y_max.max(y)),
        });
    }

    /// The box as `(x, y, width, height)`; `None` for a path with no points.
    fn rect(&self) -> Option<(f32, f32, f32, f32)> {
        self.corners.map(|(x_min, y_min, x_max, y_max)| (x_min, y_min, x_max - x_min, y_max - y_min))
    }
}

impl PathSink for PathBounds {
    fn move_to(&mut self, x: f32, y: f32) {
        self.include(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.include(x, y);
    }

    fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        self.include(x1, y1);
        self.include(x2, y2);
        self.include(x3, y3);
    }

    fn close_path(&mut self) {}
}

/// Parse an SVG-like path string and emit PDF path operators.
/// Supports: M/m, L/l, H/h, V/v, C/c, Q/q, Z/z.
/// Y-coordinates are flipped (canvas top-down → PDF bottom-up).
fn emit_svg_path(content: &mut impl PathSink, d: &str, page_h: f32) {
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
            set_fill_color(content, background.as_deref().unwrap_or("#ffffff"));
        }
        if has_border {
            let (bw, bc) = border.as_ref().unwrap();
            set_stroke_color(content, bc);
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
    /// The counter, where allocation stopped: what the writer adds later takes the ids after these, so a
    /// document without it keeps the ids it had.
    alloc:             Alloc,
}

/// The objects of a gradient: the function that blends its colors, and the shading that applies it.
struct GradientIds {
    /// A two-color gradient is one function. More colors are a function between each pair, and a last one that
    /// stitches them, which the shading uses.
    function_ids: Vec<Ref>,
    shading_id:   Ref,
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
/// Only the fonts something draws in are prepared, so a document that uses no
/// Helvetica has none. The attribution's name face is added only when `attribution`
/// is set, so a licensed document embeds nothing for it.
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
    gradients:     &HashMap<String, Gradient>,
    attribution:   bool,
    marks:         &mut Marks,
) -> Vec<RenderedPage> {
    let mut rendered_pages: Vec<RenderedPage> = Vec::new();
    let res = DrawResources { fonts, image_res_map, gradients };

    for (index, page) in pages.iter().enumerate() {
        let mut content = Content::new();
        let mut annots: Vec<AnnotData> = Vec::new();
        let mut groups: Vec<PageGroup> = Vec::new();
        marks.begin_page(index);

        // Draw optional page background before any nodes.
        if let Some(bg) = &page.background {
            marks.artifact(&mut content, |content, _| draw_page_background(content, page, bg, gradients));
        }

        // Draw all render-tree nodes.
        draw_nodes(&mut content, &mut annots, &mut groups, marks, &page.nodes, &res, page.height);

        // Drawn last so it sits above the page's own content.
        if attribution {
            draw_attribution(&mut content, &mut annots, marks, fonts, page.width, page.height);
        }

        rendered_pages.push(RenderedPage { content: content.finish(), annots, groups });
    }

    rendered_pages
}

/// The background of a page: a color, or a gradient across the whole page.
fn draw_page_background(content: &mut Content, page: &RenderPage, bg: &str, gradients: &HashMap<String, Gradient>) {
    if is_gradient_fill(bg) {
        if let Some((name, gradient)) = gradient_of(bg, gradients) {
            let bounds = (0.0, 0.0, page.width, page.height);
            fill_with_gradient(content, name, gradient, bounds, false, |content| {
                content.rect(0.0, 0.0, page.width, page.height);
            });
        }
    } else {
        set_fill_color(content, bg);
        content.rect(0.0, 0.0, page.width, page.height);
        content.fill_nonzero();
    }
}

// ── Unlicensed-use attribution ────────────────────────────────────────────────
//
// Drawn on every page of an unlicensed render. Deliberately small: the name is a
// quiet grey and only the mark carries the brand orange, so the line reads as a
// credit and never competes with the customer's document. It uses no built-in font,
// only the embedded face of the name.

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

/// Neutral dark grey for the name: about 5.7:1 on white paper, legible without
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

/// Draw the lpdf mark and "Lpdf" right-aligned in the page's top-right corner, and
/// cover the whole line with one link to lpdf.io.
///
/// The line sits 12 pt in from the edges because desktop printers cannot reach the
/// outer 12–18 pt of a sheet; any closer and printed copies lose it.
fn draw_attribution(
    content: &mut Content,
    annots:  &mut Vec<AnnotData>,
    marks:   &mut Marks,
    fonts:   &HashMap<String, PreparedFont>,
    page_w:  f32,
    page_h:  f32,
) {
    const PAD:         f32 = 12.0;
    const NAME_SIZE:   f32 = 9.0;
    const MARK_SIZE:   f32 = 9.0;
    const GAP:         f32 = 3.0;
    // Radley ships no bold. Stroking the outline as well as filling it thickens the
    // glyphs, the way browsers synthesise the 600 weight the lpdf.io wordmark asks for.
    const BOLD_STROKE: f32 = NAME_SIZE * 0.04;

    let Some(name_font) = fonts.get(ATTRIBUTION_FONT_KEY) else {
        return;
    };

    // The stroke grows each glyph by half its width on either side.
    let name_w   = name_font.text_width(ATTRIBUTION_NAME, NAME_SIZE) + BOLD_STROKE;
    let line_w   = MARK_SIZE + GAP + name_w;

    // The mark fills [PAD, PAD + MARK_SIZE] measured down from the top edge, and the
    // name's baseline is placed so its capitals centre on the mark.
    let left     = page_w - PAD - line_w;
    let baseline = page_h - (PAD + (MARK_SIZE + NAME_SIZE * ATTRIBUTION_CAP_HEIGHT) / 2.0);
    let mark_x   = left;
    let name_x   = mark_x + MARK_SIZE + GAP + BOLD_STROKE / 2.0;

    let (tr, tg, tb) = parse_hex(ATTRIBUTION_TEXT_COLOR);
    let (mr, mg, mb) = parse_hex(ATTRIBUTION_MARK_COLOR);

    // A credit, not part of the document: in a tagged PDF it is an artifact, and its link an element of its own.
    marks.artifact(content, |content, _| {
        // Its own graphics state, so the colours, stroke width and text rendering mode set
        // here cannot leak into anything drawn later on the page.
        content.save_state();

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
    });

    // One target for the whole line, with slack below and above so the name's
    // descender and a fingertip both land inside it.
    annots.push(AnnotData {
        x1:  left,
        y1:  page_h - (PAD + MARK_SIZE + 3.0),
        x2:  page_w - PAD,
        y2:  page_h - (PAD - 2.0),
        url: ATTRIBUTION_URL.to_string(),
        mark: marks.attribution_link(ATTRIBUTION_NAME),
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
            RenderNode::CanvasLayer(l) => collect_fields_in_nodes(&l.children, page_idx, out),
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
        alloc,
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
    used_gradients:     &[(&str, &Gradient)],
    meta:               &Meta,
    date:               Option<&str>,
    file:               Option<&StandardFile>,
    files:              &[EmbeddedFile],
    marks:              &Marks,
) -> Result<Vec<u8>, String> {
    // What PDF/A alone asks for: an output intent, a file ID, and links that print.
    let pdfa = file.filter(|file| file.standards.pdfa.is_some());
    let AllocatedIds {
        catalog_id, info_id, pages_id,
        page_ids, content_ids, annot_ids,
        font_id_map, image_id_map, image_smask_ids,
        field_alloc, radio_parent_alloc, opacity_gs_ids, group_ids,
        outline, mut alloc,
    } = ids;

    // The gradients' objects come last, so a document with no gradient keeps the ids it had before gradients existed.
    let gradient_ids: Vec<GradientIds> = used_gradients
        .iter()
        .map(|(_, gradient)| GradientIds {
            function_ids: (0..gradient_function_count(gradient)).map(|_| alloc.next()).collect(),
            shading_id:   alloc.next(),
        })
        .collect();

    // The XMP packet of a PDF/A or PDF/UA file, and the profile of a PDF/A file's output intent, come after them, for
    // the same reason.
    let xmp_id = file.map(|_| alloc.next());
    let profile_id = pdfa.map(|_| alloc.next());

    // The files the document carries come last, so a document without any keeps its ids.
    let file_ids = attachments::allocate(files, || alloc.next());

    // A tagged PDF's structure after them: the root and an object for each element.
    let structure = marks.is_on().then(|| StructureIds {
        root:     alloc.next(),
        elements: marks.elements.iter().map(|_| alloc.next()).collect(),
    });
    let keys = ParentKeys::of(marks, &rendered_pages);

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
        if let Some(xmp_id) = xmp_id {
            cat.metadata(xmp_id);
        }
        if let Some(profile_id) = profile_id {
            cat.output_intents()
                .push()
                .subtype(OutputIntentSubtype::PDFA)
                .output_condition_identifier(TextStr(pdfa::SRGB_CONDITION))
                .registry_name(TextStr("http://www.color.org"))
                .info(TextStr(pdfa::SRGB_CONDITION))
                .dest_output_profile(profile_id);
        }
        attachments::write_catalog(&mut cat, files, &file_ids);
        // A tagged PDF: the structure, the language a screen reader reads it in, and the title shown in place of the
        // file name (ISO 14289-1, 7.1 and 7.2).
        if let Some(structure) = &structure {
            cat.mark_info().marked(true);
            if !meta.lang.is_empty() {
                cat.lang(TextStr(&meta.lang));
            }
            cat.pair(Name(b"StructTreeRoot"), structure.root);
            cat.viewer_preferences().display_doc_title(true);
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
        info.producer(TextStr(pdfa::PRODUCER));

        // A PDF/A file is made and changed at the one date it was given, which its XMP packet repeats.
        // DocumentInfo derefs to Dict; raw pairs write the keys its typed API doesn't cover.
        if let Some(date) = &date {
            info.pair(Name(b"CreationDate"), Str(date.as_bytes()));
            if file.is_some() {
                info.pair(Name(b"ModDate"), Str(date.as_bytes()));
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
        // The shadings of the gradients, which any page or group may paint.
        if !used_gradients.is_empty() {
            let mut shading_res = resources.shadings();
            for ((name, _), gradient) in used_gradients.iter().zip(&gradient_ids) {
                shading_res.pair(Name(shading_resource_name(name).as_bytes()), gradient.shading_id);
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

        // In a tagged PDF the annotations go in the order of the structure, and the marked content of the page is
        // found through the parent tree.
        if structure.is_some() {
            pw.tab_order(TabOrder::StructureOrder);
            if let Some(key) = keys.page(i) {
                pw.struct_parents(key);
            }
        }

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
            if let Some(key) = keys.group(i, j) {
                form.struct_parents(key);
            }
        }
    }

    // -- Opacity ExtGState objects ---------------------------------------------
    for (pct, gs_id) in &opacity_gs_ids {
        let opacity = *pct as f32 / 100.0;
        let mut gs = pdf.ext_graphics(*gs_id);
        gs.non_stroking_alpha(opacity);
        gs.stroking_alpha(opacity);
    }

    // -- Gradient functions and shadings ---------------------------------------
    for ((_, gradient), ids) in used_gradients.iter().zip(&gradient_ids) {
        write_gradient(&mut pdf, gradient, ids);
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
            // PDF/A asks every annotation to print, as it is part of the page.
            if pdfa.is_some() {
                aw.flags(AnnotationFlags::PRINT);
            }
            // In a tagged PDF a link is described by its words, and found from its element (ISO 14289-1, 7.18.5).
            if let Some(mark) = ann.mark {
                let words = marks.elements[mark.element].words.trim();
                aw.contents(TextStr(if words.is_empty() { &ann.url } else { words }));
                if let Some(key) = keys.annotation(mark.number) {
                    aw.struct_parent(key);
                }
            }
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
                    ap_content.save_state();
                    set_fill_color(&mut ap_content, bg);
                    ap_content.rect(0.0, 0.0, w, h);
                    ap_content.fill_nonzero();
                    ap_content.restore_state();
                    if !field.label.is_empty() {
                        let font = fonts.get("Helvetica").expect("a document with fields has Helvetica");
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
            PreparedFontKind::Builtin { base_name, signs } => {
                // Type 1 (built-in resident) font.  Just a font dictionary with
                // a /BaseFont entry; no font program stream is needed.
                let mut font_dict = pdf.type1_font(fids.font_dict_id);
                font_dict.base_font(Name(base_name.as_bytes()));
                if *signs {
                    write_sign_encoding(&mut font_dict, base_name);
                } else {
                    font_dict.encoding_predefined(Name(b"WinAnsiEncoding"));
                }
            }

            PreparedFontKind::Truetype {
                original_bytes, subsetted_bytes, glyph_unicode, glyph_widths, ..
            } => {
                // Five objects are needed for a proper composite font:
                //   [0] Font program stream (the subset, TrueType or OpenType with CFF outlines)
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
                let program = FontProgram::of(subsetted_bytes);
                let compressed_font = compress_to_vec_zlib(subsetted_bytes, 6);
                {
                    let mut stream = pdf.stream(prog_id, &compressed_font);
                    stream.filter(Filter::FlateDecode);
                    match program {
                        // /Length1 must be the *uncompressed* (subsetted) length per the
                        // PDF spec for /FontFile2 TrueType streams.
                        FontProgram::TrueType => stream.pair(Name(b"Length1"), subsetted_bytes.len() as i32),
                        FontProgram::OpenTypeCff => stream.pair(Name(b"Subtype"), Name(b"OpenType")),
                    };
                }

                // [1] Font descriptor — describes the font's metrics and links
                //     to the embedded font program.
                let bbox   = face.global_bounding_box();
                let ascent = face.ascender()  as f32 / upem * 1000.0;
                let desc   = face.descender() as f32 / upem * 1000.0;
                let cap_h  = face.capital_height()
                    .map(|h| h as f32 / upem * 1000.0)
                    .unwrap_or(ascent * 0.7);
                let fname  = name.replace(' ', "-");

                {
                    let mut descriptor = pdf.font_descriptor(desc_id);
                    descriptor.name(Name(fname.as_bytes()))
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
                       .stem_v(80.0);
                    match program {
                        FontProgram::TrueType => descriptor.font_file2(prog_id),
                        FontProgram::OpenTypeCff => descriptor.font_file3(prog_id),
                    };
                }

                // [2] CID font dictionary (the descendant font).
                {
                    // Adobe-Identity characterises CID fonts using Identity-H.
                    let sysinfo = pdf_writer::types::SystemInfo {
                        registry:   Str(b"Adobe"),
                        ordering:   Str(b"Identity"),
                        supplement: 0,
                    };
                    let subtype = match program {
                        FontProgram::TrueType => CidFontType::Type2,
                        FontProgram::OpenTypeCff => CidFontType::Type0,
                    };
                    let mut cid = pdf.cid_font(cid_id);
                    cid.subtype(subtype)
                       .base_font(Name(fname.as_bytes()))
                       .system_info(sysinfo)
                       .font_descriptor(desc_id)
                       .default_width(1000.0);
                    // A Type 2 CIDFont's CIDToGIDMap: Identity is the default, and PDF/A asks for it to be written. A
                    // Type 0 CIDFont has none: the CFF's own charset maps CIDs to glyphs, which the subsetter writes
                    // as the identity.
                    if file.is_some() && program == FontProgram::TrueType {
                        cid.cid_to_gid_map_predefined(Name(b"Identity"));
                    }

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

    // -- The files the document carries, dated as the document is --------------
    attachments::write_objects(&mut pdf, files, &file_ids, date.as_deref());

    // -- A tagged PDF's structure ----------------------------------------------------
    if let Some(structure) = &structure {
        write_structure(&mut pdf, marks, structure, &keys, &page_ids, &group_ids, &annot_ids, &rendered_pages);
    }

    // -- PDF/A and PDF/UA: the XMP packet; PDF/A: the output intent's profile and the file ID -------
    if let (Some(file), Some(xmp_id)) = (file, xmp_id) {
        // Not compressed, so that a tool that knows nothing of PDF can still find the packet.
        let xmp = pdfa::xmp_packet(file.standards, meta, &file.date);
        pdf.metadata(xmp_id, &xmp);
    }
    if let Some(profile_id) = profile_id {
        pdf.icc_profile(profile_id, pdfa::SRGB_PROFILE)
           .n(3)
           .alternate()
           .device_rgb();
        // From the bytes written so far, so the same document gives the same file.
        let id = Md5::digest(pdf.as_bytes()).to_vec();
        pdf.set_file_id((id.clone(), id));
    }

    Ok(pdf.finish())
}

/// The date of the file as a PDF date: a PDF/A file's date, or else `created_on` read as a PDF/A file's is
/// (`pdfa::plain_date`), and an error when it is not a date. `None` leaves the file undated, so the same input gives
/// the same bytes.
fn file_date(created_on: Option<&str>, standard: Option<&StandardFile>) -> Result<Option<String>, String> {
    if let Some(file) = standard {
        return Ok(Some(file.date.pdf()));
    }
    Ok(pdfa::plain_date(created_on)?.map(|date| date.pdf()))
}

// ── Main render function ──────────────────────────────────────────────────────

/// Convert a fully laid-out document into a binary PDF.
///
/// # Parameters
/// - `pages`      – Layout output: one `RenderPage` per document page.
/// - `font_defs`  – Font name → definition from the document's `<fonts>` section.
/// - `registry`   – Raw font bytes for custom fonts (populated via `load_font`).
/// - `meta`       – Document metadata (title, author, subject, etc.).
/// - `gradients`  – The gradients of the document's tokens, which a fill names as `gradient:name`.
/// - `created_on` – Optional ISO 8601 date string written to `/CreationDate`
///                  and each attached file's `/ModDate`, with the precision
///                  and the time zone it was given in; one that is not a date
///                  is an error. Required with `pdfa`, which writes it as both
///                  dates of the Info dictionary and of the XMP packet.
/// - `licensed`   – `true` when a valid commercial license token was supplied.
///                  When `false`, draws the attribution line (the lpdf mark and
///                  "Lpdf") in every page's top-right corner.
/// - `output`     – The document's `<output>`: with `pdfa`, a PDF/A file gets its
///                  XMP packet, the sRGB output intent and a file ID; with `pdfua`,
///                  a PDF/UA file is tagged, with its structure, its marked content
///                  and its XMP packet. What either cannot hold is an error.
/// - `files`      – The files of `<attachments>` with their bytes (`attachments::resolve`), carried in the PDF.
#[allow(clippy::too_many_arguments)]
pub fn render_pdf(
    pages:          &[RenderPage],
    font_defs:      &HashMap<String, FontDef>,
    registry:       &FontRegistry,
    image_registry: &ImageRegistry,
    meta:           &Meta,
    gradients:      &HashMap<String, Gradient>,
    created_on:     Option<&str>,
    licensed:       bool,
    output:         &Output,
    files:          &[EmbeddedFile],
) -> Result<Vec<u8>, String> {
    let standards = Standards::of(output);
    let file = standards.any()
        .then(|| pdfa::file_date(standards, created_on).map(|date| StandardFile { standards, date }))
        .transpose()?;
    let date = file_date(created_on, file.as_ref())?;
    // The attribution is the licence signal itself, so it follows `licensed` rather
    // than a separate flag every caller would have to keep in step.
    let attribution = !licensed;
    let (sorted_font_names, fonts) =
        prepare_fonts(pages, font_defs, registry, attribution);
    let (sorted_image_names, image_res_map) =
        prepare_images(pages, image_registry);
    let anchors = placed_anchors(pages);
    let used_gradients = used_gradients(pages, gradients);
    let fields = collect_render_fields(pages);
    if let Some(level) = standards.pdfa {
        refuse_for_pdfa(level, pages, &fonts, &sorted_font_names, font_defs, &used_gradients, &fields)?;
    }
    if let Some(part) = standards.pdfua {
        refuse_for_pdfua(part, pages, &fonts, &sorted_font_names, font_defs)?;
    }
    let mut marks = if standards.pdfua.is_some() { Marks::tagged() } else { Marks::off() };
    let mut rendered_pages =
        build_content_streams(pages, &fonts, &image_res_map, gradients, attribution, &mut marks);
    drop_dangling_links(&mut rendered_pages, &anchors);
    let ids =
        allocate_ids(pages, &rendered_pages, &fonts, &sorted_font_names, &sorted_image_names, image_registry, &fields);
    assemble_pdf(
        pages, &fonts, &sorted_font_names, &sorted_image_names,
        image_registry, &image_res_map, rendered_pages, ids, &fields, &anchors,
        &used_gradients, meta, date.as_deref(), file.as_ref(), files, &marks,
    )
}

/// The standards and the date of a render of a document that asks for PDF/A or PDF/UA.
struct StandardFile {
    standards: Standards,
    date:      FileDate,
}

// ── PDF/A refusals ────────────────────────────────────────────────────────────

/// Refuse what a PDF/A file cannot hold where only the author can change it, with an error that says what to
/// change: a form field, text in a font that is not embedded, a font whose licence forbids embedding, a character
/// the font has no glyph for, which a reader would draw as an empty box, and a CMYK color, which the sRGB output
/// intent doesn't cover. The engine repairs none of these; it writes the rest of what PDF/A needs itself.
fn refuse_for_pdfa(
    level:             PdfA,
    pages:             &[RenderPage],
    fonts:             &HashMap<String, PreparedFont>,
    sorted_font_names: &[String],
    font_defs:         &HashMap<String, FontDef>,
    used_gradients:    &[(&str, &Gradient)],
    fields:            &[(usize, RenderField)],
) -> Result<(), String> {
    let refuse = |reason: String| Err(format!("<output pdfa=\"{}\">: {reason}", level.attribute()));

    if let Some((_, field)) = fields.first() {
        let name = if field.name.is_empty() { String::new() } else { format!(" '{}'", field.name) };
        return refuse(format!(
            "the form field{name} can't be in a PDF/A file yet: a field draws its text in Helvetica, which is not \
            embedded, and PDF/A embeds every font. Remove the field, or remove pdfa"
        ));
    }

    for name in sorted_font_names {
        if let PreparedFontKind::Truetype { original_bytes, .. } = &fonts[name].kind
            && let Some(reason) = embedding_refused(original_bytes)
        {
            return refuse(format!(
                "the font '{name}' {reason}, and a PDF/A file embeds every font it draws in: use another font"
            ));
        }
    }

    for page in pages {
        if let Some(reason) = first_in(&page.nodes, &mut |node| font_refusal(node, fonts, font_defs, "PDF/A")) {
            return refuse(reason);
        }
    }

    for page in pages {
        let background = page.background.as_deref().filter(|color| is_cmyk(color));
        if let Some(color) = background.map(str::to_string).or_else(|| first_in(&page.nodes, &mut cmyk_color)) {
            return refuse(format!(
                "the color {color} is CMYK, and the sRGB output intent of a PDF/A file covers RGB colors only: give \
                it as an RGB color, such as a hex one, or remove pdfa"
            ));
        }
    }
    if let Some((name, _)) = used_gradients.iter().find(|(_, gradient)| gradient.stops.iter().any(|stop| is_cmyk(&stop.color))) {
        return refuse(format!(
            "the gradient '{name}' has CMYK colors, and the sRGB output intent of a PDF/A file covers RGB colors \
            only: give its colors as RGB ones, such as hex ones, or remove pdfa"
        ));
    }
    Ok(())
}

/// Refuse what a PDF/UA file cannot hold where only the author can change it: text in a font that is not embedded, a
/// font whose licence forbids embedding, and a character the font has no glyph for, which a reader would draw as an
/// empty box (ISO 14289-1, 7.21.4 and 7.21.8). A form field, an image without `alt` and the rest of what the author
/// writes are refused before the layout (`pdfua::check`).
fn refuse_for_pdfua(
    part:              PdfUa,
    pages:             &[RenderPage],
    fonts:             &HashMap<String, PreparedFont>,
    sorted_font_names: &[String],
    font_defs:         &HashMap<String, FontDef>,
) -> Result<(), String> {
    let refuse = |reason: String| Err(format!("<output pdfua=\"{}\">: {reason}", part.attribute()));
    for name in sorted_font_names {
        if let PreparedFontKind::Truetype { original_bytes, .. } = &fonts[name].kind
            && let Some(reason) = embedding_refused(original_bytes)
        {
            return refuse(format!(
                "the font '{name}' {reason}, and a PDF/UA file embeds every font it draws in: use another font"
            ));
        }
    }
    for page in pages {
        if let Some(reason) = first_in(&page.nodes, &mut |node| font_refusal(node, fonts, font_defs, "PDF/UA")) {
            return refuse(reason);
        }
    }
    Ok(())
}

/// The first answer `check` gives for a node of `nodes` or of their children, in drawing order.
fn first_in(nodes: &[RenderNode], check: &mut impl FnMut(&RenderNode) -> Option<String>) -> Option<String> {
    nodes.iter().find_map(|node| check(node).or_else(|| match node {
        RenderNode::Box(b)         => first_in(&b.children, check),
        RenderNode::Link(l)        => first_in(&l.children, check),
        RenderNode::CanvasLayer(l) => first_in(&l.children, check),
        _                          => None,
    }))
}

/// Why `node` can't be drawn in a file of `standard`, PDF/A or PDF/UA, for its font: its text is in a font that is not
/// embedded, or has a character its embedded font has no glyph for, or it is a barcode's digits, which are drawn in
/// Helvetica.
fn font_refusal(
    node:      &RenderNode,
    fonts:     &HashMap<String, PreparedFont>,
    font_defs: &HashMap<String, FontDef>,
    standard:  &str,
) -> Option<String> {
    match node {
        RenderNode::Text(t) => match &fonts.get(&t.font)?.kind {
            PreparedFontKind::Builtin { base_name, .. } => {
                let text = excerpt(&t.content);
                Some(match font_defs.get(&t.font) {
                    Some(FontDef::Ref(_)) => format!(
                        "the text \"{text}\" is in the font '{}', which <assets> declares, and its file was not loaded, \
                        so it would be drawn in Helvetica, which is not embedded: check the font's src",
                        t.font
                    ),
                    _ => format!(
                        "the text \"{text}\" is in {base_name}, a font built into PDF readers, which is not embedded, \
                        and a {standard} file embeds every font: declare a font in <assets> and set it with font on the \
                        document or on the text"
                    ),
                })
            }
            PreparedFontKind::Truetype { advances: None, .. } => Some(format!(
                "the font '{}' could not be read as a TrueType or OpenType font, so its glyphs are unknown: use another \
                font file",
                t.font
            )),
            PreparedFontKind::Truetype { char_to_gid, .. } => {
                let missing = t.content.chars().find(|c| !char_to_gid.contains_key(c))?;
                Some(format!(
                    "the text \"{}\" has '{missing}' (U+{:04X}), which the font '{}' has no glyph for, and a {standard} \
                    file may not draw the empty box a reader shows instead: use a font that has it, or leave it out",
                    excerpt(&t.content), missing as u32, t.font
                ))
            }
        },
        RenderNode::Barcode(bc) if has_caption(&bc.kind) => Some(format!(
            "a barcode's digits (hrt) are drawn in Helvetica, which is not embedded, and a {standard} file embeds every \
            font: set hrt=\"false\" on the barcode, and write the digits as text in a font of <assets> if they are \
            needed"
        )),
        _ => None,
    }
}

/// Why an embedded font's licence forbids embedding it, from its OS/2 `fsType`, or `None` when it allows it: a
/// restricted licence, or one that allows only bitmaps, where the engine embeds outlines.
fn embedding_refused(bytes: &[u8]) -> Option<&'static str> {
    let face = Face::parse(bytes, 0).ok()?;
    match face.permissions()? {
        Permissions::Restricted => Some("does not allow embedding (its licence, in the OS/2 table, is restricted)"),
        _ if !face.is_outline_embedding_allowed() => Some("allows embedding its bitmaps only, not its outlines"),
        _ => None,
    }
}

/// The first CMYK color `node` itself paints with, not its children's.
fn cmyk_color(node: &RenderNode) -> Option<String> {
    fn shadow(shadow: &Option<Shadow>) -> Option<&str> {
        shadow.as_ref().map(|s| s.color.as_str())
    }
    let colors: Vec<Option<&str>> = match node {
        RenderNode::Box(b)           => vec![b.fill.as_deref(), b.border_color.as_deref(), shadow(&b.shadow)],
        RenderNode::Line(l)          => vec![Some(&l.color)],
        RenderNode::Text(t)          => vec![Some(&t.color)],
        RenderNode::Barcode(bc)      => vec![Some(&bc.color), bc.bg.as_deref()],
        RenderNode::Field(f)         => vec![f.background.as_deref(), f.border.as_ref().map(|(_, color)| color.as_str())],
        RenderNode::CanvasRect(r)    => vec![r.fill.as_deref(), r.stroke.as_deref(), shadow(&r.shadow)],
        RenderNode::CanvasLine(l)    => vec![Some(&l.stroke)],
        RenderNode::CanvasEllipse(e) => vec![e.fill.as_deref(), e.stroke.as_deref(), shadow(&e.shadow)],
        RenderNode::CanvasPath(p)    => vec![p.fill.as_deref(), p.stroke.as_deref()],
        RenderNode::Link(_) | RenderNode::Image(_) | RenderNode::CanvasLayer(_)
            | RenderNode::Anchor(_) | RenderNode::FootnoteMark(_) => vec![],
    };
    colors.into_iter().flatten().find(|color| is_cmyk(color)).map(str::to_string)
}

/// The start of a text, for an error that names it.
fn excerpt(text: &str) -> String {
    const MAX: usize = 40;
    let text = text.trim();
    match text.char_indices().nth(MAX) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

// ── Tagged PDF: the structure ─────────────────────────────────────────────────

/// The objects of a tagged PDF's structure: its root, and one for each element of `Marks::elements`.
struct StructureIds {
    root:     Ref,
    elements: Vec<Ref>,
}

/// What a key of the parent tree finds: the elements of the marked content of a stream, by MCID, or the element of
/// a link annotation.
enum ParentEntry {
    Stream(usize),
    Annotation(usize),
}

/// The keys of the parent tree (ISO 32000-1, 14.7.4.4): one for each content stream that holds marked content, a page
/// or a transparency group, then one for each link annotation that is written. A page or a group gives its key in
/// `/StructParents`, an annotation in `/StructParent`.
struct ParentKeys {
    entries:     Vec<(i32, ParentEntry)>,
    pages:       HashMap<usize, i32>,
    groups:      HashMap<(usize, usize), i32>,
    annotations: HashMap<usize, i32>,
}

impl ParentKeys {
    /// The keys of the streams `marks` drew marked content in, and of the annotations of `rendered_pages` that a link
    /// element holds. Empty for a document that is not tagged.
    fn of(marks: &Marks, rendered_pages: &[RenderedPage]) -> Self {
        let mut keys = ParentKeys { entries: Vec::new(), pages: HashMap::new(), groups: HashMap::new(), annotations: HashMap::new() };
        for (index, stream) in marks.streams.iter().enumerate().filter(|(_, stream)| !stream.owners.is_empty()) {
            let key = keys.entries.len() as i32;
            match stream.group {
                Some(group) => keys.groups.insert((stream.page, group), key),
                None => keys.pages.insert(stream.page, key),
            };
            keys.entries.push((key, ParentEntry::Stream(index)));
        }
        for mark in rendered_pages.iter().flat_map(|page| &page.annots).filter_map(|annot| annot.mark) {
            let key = keys.entries.len() as i32;
            keys.annotations.insert(mark.number, key);
            keys.entries.push((key, ParentEntry::Annotation(mark.element)));
        }
        keys
    }

    fn page(&self, page: usize) -> Option<i32> {
        self.pages.get(&page).copied()
    }

    fn group(&self, page: usize, group: usize) -> Option<i32> {
        self.groups.get(&(page, group)).copied()
    }

    fn annotation(&self, number: usize) -> Option<i32> {
        self.annotations.get(&number).copied()
    }
}

/// The id of the `n`th note, from 1: a `Note` element has one (ISO 14289-1, 7.9).
fn note_id(n: usize) -> String {
    format!("note-{n}")
}

/// Write the structure of a tagged PDF: the root with its parent tree and the ids of its notes, and each element,
/// with what it holds in reading order. A link annotation that was dropped (`drop_dangling_links`) is left out of its
/// element.
#[allow(clippy::too_many_arguments)]
fn write_structure(
    pdf:            &mut Pdf,
    marks:          &Marks,
    ids:            &StructureIds,
    keys:           &ParentKeys,
    page_ids:       &[Ref],
    group_ids:      &[Vec<Ref>],
    annot_ids:      &[Vec<Ref>],
    rendered_pages: &[RenderedPage],
) {
    let annotations: HashMap<usize, (Ref, Ref)> = rendered_pages.iter().enumerate()
        .flat_map(|(page, rendered)| rendered.annots.iter().enumerate().map(move |(j, annot)| (page, j, annot)))
        .filter_map(|(page, j, annot)| annot.mark.map(|mark| (mark.number, (annot_ids[page][j], page_ids[page]))))
        .collect();
    let notes: Vec<usize> = (0..marks.elements.len()).filter(|&i| marks.elements[i].kind == Kind::Note).collect();

    {
        let mut root = pdf.indirect(ids.root).start::<pdf_writer::writers::StructTreeRoot>();
        root.child(ids.elements[0]);
        {
            let mut tree = root.insert(Name(b"ParentTree")).dict();
            let mut nums = tree.insert(Name(b"Nums")).array();
            for (key, entry) in &keys.entries {
                nums.item(*key);
                match entry {
                    ParentEntry::Stream(stream) => {
                        nums.push().array().items(marks.streams[*stream].owners.iter().map(|owner| ids.elements[*owner]));
                    }
                    ParentEntry::Annotation(element) => {
                        nums.item(ids.elements[*element]);
                    }
                }
            }
        }
        root.parent_tree_next_key(keys.entries.len() as i32);
        if !notes.is_empty() {
            // A name tree's keys are in the order of their bytes.
            let mut by_id: Vec<(String, Ref)> = notes.iter().enumerate()
                .map(|(n, &element)| (note_id(n + 1), ids.elements[element]))
                .collect();
            by_id.sort();
            let mut tree = root.id_tree();
            let mut names = tree.names();
            for (id, element) in &by_id {
                names.insert(Str(id.as_bytes()), *element);
            }
        }
    }

    for (index, element) in marks.elements.iter().enumerate() {
        let mut written = pdf.struct_element(ids.elements[index]);
        written.custom_kind(Name(element.kind.name()));
        written.parent(if index == 0 { ids.root } else { ids.elements[element.parent] });
        if index != 0 {
            written.page(page_ids[element.page]);
        }
        match &element.kind {
            Kind::Figure(alt) => {
                written.alt(TextStr(alt));
            }
            Kind::Note => {
                let n = notes.iter().position(|&note| note == index).map_or(0, |n| n + 1);
                written.id(Str(note_id(n).as_bytes()));
            }
            // A header cell heads its column, which lets a reader find the headers of each data cell.
            Kind::TH { col_span, row_span } => {
                let mut attributes = written.attributes();
                let mut table = attributes.push().table();
                table.scope(TableHeaderScope::Column);
                if *col_span > 1 { table.col_span(i32::from(*col_span)); }
                if *row_span > 1 { table.row_span(i32::from(*row_span)); }
            }
            Kind::TD { col_span, row_span } if *col_span > 1 || *row_span > 1 => {
                let mut attributes = written.attributes();
                let mut table = attributes.push().table();
                if *col_span > 1 { table.col_span(i32::from(*col_span)); }
                if *row_span > 1 { table.row_span(i32::from(*row_span)); }
            }
            _ => {}
        }
        let mut kids = written.children();
        for kid in &element.kids {
            match *kid {
                Kid::Element(child) => {
                    kids.struct_element(ids.elements[child]);
                }
                Kid::Marked { stream, mcid } => {
                    let stream = &marks.streams[stream];
                    let mut marked = kids.marked_content_ref();
                    marked.page(page_ids[stream.page]);
                    if let Some(group) = stream.group {
                        marked.stream(group_ids[stream.page][group]);
                    }
                    marked.marked_content_id(mcid);
                }
                Kid::Annotation(number) => {
                    if let Some(&(annotation, page)) = annotations.get(&number) {
                        kids.object_ref().object(annotation).page(page);
                    }
                }
            }
        }
    }
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

    /// Fonts of `test/assets/fonts`, as embedded fonts are in a document.
    const FIXTURE_FONTS: [(&str, &[u8]); 5] = [
        ("Radley", include_bytes!("../../../test/assets/fonts/Radley-Regular.ttf")),
        ("Montserrat", include_bytes!("../../../test/assets/fonts/Montserrat-Regular.ttf")),
        ("CrimsonText", include_bytes!("../../../test/assets/fonts/CrimsonText-Regular.ttf")),
        ("CrimsonTextItalic", include_bytes!("../../../test/assets/fonts/CrimsonText-Italic.ttf")),
        ("CrimsonTextSemiBold", include_bytes!("../../../test/assets/fonts/CrimsonText-SemiBold.ttf")),
    ];

    #[test]
    fn a_built_in_font_draws_the_lines_of_a_box_and_the_arrows_as_ascii_and_the_rest_as_a_question_mark() {
        assert_eq!(encode_latin1("┌──┐ │ └──┘ ├─┼─┤ ╔═╗ ║ ╭╮", false), b"+--+ | +--+ +-+-+ +-+ | ++".to_vec());
        assert_eq!(encode_latin1("a → b ← c ↑ d ▼ e", false), b"a > b < c ^ d v e".to_vec());
        assert_eq!(encode_latin1("5′ 10″, −3\u{2009}x", false), b"5' 10\", -3 x".to_vec());
        assert_eq!(encode_latin1("日本 ✓", false), b"?? ?".to_vec(), "a character with no look-alike stays a question mark");
        assert_eq!(encode_latin1("é — €", false), vec![0xE9, b' ', 0x97, b' ', 0x80], "what the encoding has is not replaced");
    }

    #[test]
    fn layout_measures_a_look_alike_as_the_character_that_is_drawn() {
        for (font, drawn) in [("Helvetica", '-'), ("Times-Roman", '|'), ("Helvetica-Bold", '>')] {
            let shown = match drawn { '-' => '─', '|' => '│', _ => '→' };
            assert_eq!(
                crate::layout::text_width(font, &shown.to_string(), 10.0),
                crate::layout::text_width(font, &drawn.to_string(), 10.0),
                "{font}: {shown}"
            );
        }
    }

    #[test]
    fn layout_measures_a_character_drawn_as_a_question_mark_with_the_width_of_one() {
        // The fi and fl ligatures, the lowest and highest C1 controls, and CJK, Greek and Cyrillic letters have no
        // code, sign or look-alike in any built-in font.
        let shown_as_question_mark = ['\u{FB01}', '\u{FB02}', '\u{0080}', '\u{009F}', '日', 'λ', 'ж'];
        for font in BUILT_IN_FONTS {
            let question_mark = crate::layout::text_width(font, "?", 1000.0);
            for shown in shown_as_question_mark {
                let text = shown.to_string();
                assert_eq!(encode_latin1(&text, true), b"?".to_vec(), "{shown:?} is not drawn as a question mark");
                assert_eq!(crate::layout::text_width(font, &text, 1000.0), question_mark, "{font}: {shown:?}");
            }
        }
        assert_eq!(crate::layout::text_width("Helvetica", "日", 1000.0).round(), 556.0);
        assert_eq!(crate::layout::text_width("Times-Roman", "日", 1000.0).round(), 444.0);
    }

    const BUILT_IN_FONTS: [&str; 12] = [
        "Helvetica", "Helvetica-Bold", "Helvetica-Oblique", "Helvetica-BoldOblique",
        "Times-Roman", "Times-Bold", "Times-Italic", "Times-BoldItalic",
        "Courier", "Courier-Bold", "Courier-Oblique", "Courier-BoldOblique",
    ];

    fn built_in(base_name: &'static str, signs: bool) -> PreparedFont {
        PreparedFont { resource_name: "F0".to_string(), kind: PreparedFontKind::Builtin { base_name, signs } }
    }

    #[test]
    fn a_built_in_font_with_signs_has_a_code_for_each_comparison_sign_and_one_without_draws_a_question_mark() {
        assert_eq!(built_in("Helvetica", true).encode_text("a ≥ b ≤ c ≠ d ≈ e"), b"a \x90 b \x8F c \x8D d \x81 e".to_vec());
        assert_eq!(built_in("Helvetica", false).encode_text("a ≥ b ≤ c ≠ d ≈ e"), b"a ? b ? c ? d ? e".to_vec());
    }

    #[test]
    fn the_codes_of_the_signs_differ_and_are_ones_that_no_character_of_windows_1252_has() {
        let codes: HashSet<u8> = SIGNS.iter().map(|&(_, code, _)| code).collect();
        assert_eq!(codes.len(), SIGNS.len(), "two signs share a code");
        for c in (0..=0xFFFF).filter_map(char::from_u32) {
            assert!(win1252_byte(c).is_none_or(|code| !codes.contains(&code)), "{c:?} has the code of a sign");
        }
    }

    #[test]
    fn layout_measures_a_comparison_sign_as_the_viewer_advances_by_the_metrics() {
        for font in BUILT_IN_FONTS {
            let expected = if font.starts_with("Courier") { 600.0 } else { 549.0 };
            for sign in ['≥', '≤', '≠', '≈'] {
                assert_eq!(crate::layout::text_width(font, &sign.to_string(), 1000.0).round(), expected, "{font}: {sign}");
            }
        }
    }

    /// The `/Widths` of the dictionary of the built-in font that a document with a sign draws in, one for each
    /// code from the space.
    fn widths_of(font: &str) -> Vec<f32> {
        let pdf = render(&format!(r#"<text font="{font}">a ≥ b</text>"#));
        let (_, dict) = object_with(&pdf, &format!("/BaseFont /{font}\n"));
        let list = dict.split("/Widths [").nth(1).and_then(|rest| rest.split(']').next()).expect("a /Widths array");
        list.split_whitespace().map(|width| width.parse().expect("a width")).collect()
    }

    #[test]
    fn the_font_of_a_document_with_signs_names_their_glyphs_and_gives_every_code_a_width() {
        let pdf = render(r#"<text>Node ≥ 22, npm ≤ 10, a ≠ b, 0.1 + 0.2 ≈ 0.3</text>"#);
        let (_, helvetica) = object_with(&pdf, "/BaseFont /Helvetica\n");
        assert!(helvetica.contains("/BaseEncoding /WinAnsiEncoding"), "{helvetica}");
        assert!(helvetica.contains("/Differences [129 /approxequal 141 /notequal 143 /lessequal 144 /greaterequal]"), "{helvetica}");
        assert!(helvetica.contains("/FirstChar 32") && helvetica.contains("/LastChar 255"), "{helvetica}");
        assert_eq!(widths_of("Helvetica").len(), 224);
    }

    #[test]
    fn the_widths_are_the_adobe_metrics_so_that_a_viewer_advances_as_it_did_without_them() {
        let width = |font: &str, code: u8| widths_of(font)[usize::from(code) - 32];
        // Printable ASCII, the straight quote and the grave accent among them: 191 and 333, not the curly quotes' 222.
        assert_eq!([b' ', b'\'', b'`', b'A', b'i'].map(|code| width("Helvetica", code)), [278.0, 191.0, 333.0, 667.0, 222.0]);
        // Accented letters, and a bold one that is wider than its regular.
        assert_eq!([0xE9, 0xC1, 0xDF].map(|code| width("Helvetica", code)), [556.0, 667.0, 611.0]);
        assert_eq!(width("Helvetica-Bold", 0xC0), 722.0);
        assert_eq!([b'e', 0xE9].map(|code| width("Times-Italic", code)), [444.0, 444.0]);
        // The bullet that WinAnsiEncoding draws for a code it leaves empty.
        assert_eq!([0x7F, 0x9D].map(|code| width("Helvetica", code)), [350.0; 2]);
        assert!(widths_of("Courier-BoldOblique").iter().all(|&width| width == 600.0));
        assert_eq!(widths_of("Helvetica-Oblique"), widths_of("Helvetica"));
        assert_eq!(widths_of("Helvetica-BoldOblique"), widths_of("Helvetica-Bold"));
    }

    #[test]
    fn the_width_of_a_sign_in_the_dictionary_is_the_width_layout_measures() {
        for font in BUILT_IN_FONTS {
            let widths = widths_of(font);
            for &(sign, code, _) in &SIGNS {
                let layout = crate::layout::text_width(font, &sign.to_string(), 1000.0).round();
                assert_eq!(widths[usize::from(code) - 32], layout, "{font}: {sign}");
            }
        }
    }

    #[test]
    fn layout_measures_every_character_of_windows_1252_with_the_width_the_metrics_give_its_code() {
        let drawn: Vec<(char, u8)> = (0..=0xFFFF)
            .filter_map(char::from_u32)
            .filter_map(|c| win1252_byte(c).filter(|&code| code >= WIN_ANSI_FIRST).map(|code| (c, code)))
            .collect();
        // Every code from the space has a character but the five that WinAnsiEncoding leaves empty.
        let codes: HashSet<u8> = drawn.iter().map(|&(_, code)| code).collect();
        assert_eq!(codes, (WIN_ANSI_FIRST..=u8::MAX).filter(|code| ![0x81, 0x8D, 0x8F, 0x90, 0x9D].contains(code)).collect());
        for font in BUILT_IN_FONTS {
            for &(c, code) in &drawn {
                let metrics = crate::layout::afm_code_advance(font, code).map(f32::from);
                assert_eq!(Some(crate::layout::text_width(font, &c.to_string(), 1000.0).round()), metrics, "{font}: {c:?}");
            }
        }
    }

    #[test]
    fn a_font_of_a_document_with_no_sign_is_written_as_it_was() {
        let pdf = render(r#"<text>Node is at least 22, é — €</text>"#);
        let (_, helvetica) = object_with(&pdf, "/BaseFont /Helvetica\n");
        assert!(helvetica.contains("/Encoding /WinAnsiEncoding"), "{helvetica}");
        assert!(!helvetica.contains("/Differences") && !helvetica.contains("/Widths"), "{helvetica}");
    }

    #[test]
    fn only_the_font_that_draws_a_sign_is_written_with_codes_for_them() {
        let pdf = render(r#"<text font="Times-Roman">a ≥ b</text><text font="Courier">no signs</text>"#);
        assert!(object_with(&pdf, "/BaseFont /Times-Roman\n").1.contains("/Differences"));
        assert!(!object_with(&pdf, "/BaseFont /Courier\n").1.contains("/Differences"));
    }

    #[test]
    fn symbol_and_zapf_dingbats_are_left_with_their_encoding_whatever_they_are_given() {
        let pdf = render(r#"<text font="Symbol">≥</text><text font="ZapfDingbats">≤</text>"#);
        for name in ["Symbol", "ZapfDingbats"] {
            let (_, dict) = object_with(&pdf, &format!("/BaseFont /{name}\n"));
            assert!(!dict.contains("/Differences") && !dict.contains("/Widths"), "{name}: {dict}");
        }
    }

    #[test]
    fn an_embedded_font_draws_the_comparison_signs_as_it_has_them() {
        let (name, bytes) = FIXTURE_FONTS.iter()
            .find(|(_, bytes)| Face::parse(bytes, 0).is_ok_and(|face| ['≥', '≤', '≠', '≈'].iter().all(|&c| face.glyph_index(c).is_some())))
            .expect("a fixture font with the four signs");
        let used: HashSet<char> = "a ≥ b ≤ c ≠ d ≈ e".chars().collect();
        let font = PreparedFont { resource_name: "F0".to_string(), kind: prepare_truetype_font(bytes, &used) };
        let encoded = font.encode_text("≥≈");
        let glyphs: Vec<u16> = encoded.chunks(2).map(|pair| u16::from(pair[0]) << 8 | u16::from(pair[1])).collect();
        assert!(glyphs.iter().all(|&glyph| glyph != 0), "{name} draws a sign as .notdef");
    }

    #[test]
    fn layout_and_renderer_measure_text_beyond_ascii_alike() {
        let samples = [
            "Crème brûlée, señor Åström: “quoted” – 5 € — ¿qué?",
            "Привет, мир: «цена» — 5 ₽",
            "plain ascii text, 0123456789",
        ];
        for (name, bytes) in FIXTURE_FONTS {
            let face = Face::parse(bytes, 0).expect("fixture face parses");
            let widths = crate::shared::extract_font_widths(bytes).expect("fixture face has widths");
            crate::layout::set_font_widths(HashMap::from([(name.to_string(), widths)]));
            for text in samples {
                let used: HashSet<char> = text.chars().collect();
                let font = PreparedFont {
                    resource_name: "F0".to_string(),
                    kind: prepare_truetype_font(bytes, &used),
                };
                let (size, upem) = (12.0_f32, face.units_per_em() as f32);
                let laid_out = crate::layout::text_width(name, text, size);
                let drawn = font.text_width(text, size);
                // Glyph 0 stands in for a character the face does not map, as the encoder writes it.
                let real: f32 = text.chars()
                    .map(|c| face.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0)))
                    .map(|gid| face.glyph_hor_advance(gid).unwrap_or(0) as f32 * size / upem)
                    .sum();
                // Layout rounds each advance to 1/1000 em, at most 0.0005 em off.
                let rounding = text.chars().count() as f32 * 0.0005 * size;
                assert!((drawn - real).abs() < 0.001, "{name}: the renderer measures {text:?} as {drawn}, the face says {real}");
                assert!((laid_out - drawn).abs() <= rounding, "{name}: layout {laid_out} against renderer {drawn} for {text:?}");
            }
        }
    }

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

    /// A document of `layout` written as a PDF, licensed or with the attribution line.
    fn render_as(layout: &str, licensed: bool) -> Vec<u8> {
        let xml = format!(r#"<lpdf version="1"><document><section><layout>{layout}</layout></section></document></lpdf>"#);
        let mut doc = crate::parse::parse(&xml).expect("parses");
        let pages = crate::layout::layout_document(&doc.section_layouts());
        render_pdf(&pages, &doc.fonts, &FontRegistry::new(), &ImageRegistry::new(), &doc.meta, &doc.gradients, None, licensed, &doc.output, &[])
            .expect("renders")
    }

    /// The numbers of the array that follows `key` in `dict`.
    fn array_after(dict: &str, key: &str) -> Vec<f32> {
        let list = dict.split(key).nth(1).and_then(|rest| rest.split(']').next()).unwrap_or_else(|| panic!("no {key} in {dict}"));
        list.split_whitespace().map(|number| number.parse().expect("a number")).collect()
    }

    #[test]
    fn the_attribution_line_is_the_mark_and_the_name_in_the_top_right_corner_under_one_link() {
        let pdf = render_as("", false);
        let (page, _) = page_and_groups(&pdf);
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(page.matches(" Tf").count(), 1, "the name is the only text of an empty page: {page}");
        assert!(!page.contains("Made with"), "{page}");
        assert_eq!(text.matches("/URI (https://lpdf.io)").count(), 1);

        let (_, link) = object_with(&text, "/URI (https://lpdf.io)");
        let (_, page_dict) = object_with(&text, "/Type /Page\n");
        let [x1, y1, x2, y2] = array_after(&link, "/Rect [")[..] else { panic!("a rectangle: {link}") };
        let [.., width, height] = array_after(&page_dict, "/MediaBox [")[..] else { panic!("a media box: {page_dict}") };
        assert!((x2 - (width - 12.0)).abs() < 0.01, "the line ends 12 pt in from the right edge: {x2} of {width}");
        assert!(x1 > width - 60.0 && y1 > height - 30.0 && y2 <= height, "the line is in the top-right corner: {link}");
    }

    #[test]
    fn a_document_that_draws_nothing_in_helvetica_has_no_helvetica_licensed_or_not() {
        for licensed in [true, false] {
            let pdf = render_as(r#"<text font="Times-Roman">Body</text>"#, licensed);
            let text = String::from_utf8_lossy(&pdf);
            assert!(text.contains("/BaseFont /Times-Roman"), "licensed: {licensed}");
            assert!(!text.contains("/BaseFont /Helvetica"), "licensed: {licensed}");
        }
    }

    #[test]
    fn a_barcode_caption_and_a_form_field_draw_in_helvetica() {
        for layout in [
            r#"<barcode type="code128" data="LP-1" hrt="true"/>"#,
            r#"<barcode type="ean13" data="590123412345" hrt="true"/>"#,
            r#"<field type="text" name="email"/>"#,
            r#"<field type="checkbox" name="terms"/>"#,
        ] {
            let pdf = render_as(layout, true);
            assert!(String::from_utf8_lossy(&pdf).contains("/BaseFont /Helvetica\n"), "{layout}");
        }
        let pdf = render_as(r#"<barcode type="code128" data="LP-1"/>"#, true);
        assert!(!String::from_utf8_lossy(&pdf).contains("/BaseFont /Helvetica"), "a barcode without a caption draws no text");
    }

    #[test]
    fn a_form_names_helvetica_helv_in_its_default_resources() {
        let pdf = render_as(r#"<field type="text" name="email"/>"#, true);
        let text = String::from_utf8_lossy(&pdf);
        let (helvetica, _) = object_with(&text, "/BaseFont /Helvetica\n");
        let (_, catalog) = object_with(&text, "/Type /Catalog");
        assert!(catalog.contains(&format!("/Helv {helvetica} 0 R")), "{catalog}");
        assert!(text.contains("/DA (/Helv 0 Tf 0 g)"));
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

    // ── An image clipped to rounded corners ───────────────────────────────────

    /// A page with one `img` of the given attributes, from `RED_PIXEL`, laid out as the engine does.
    fn render_image_page(attrs: &str) -> Vec<u8> {
        let xml = format!(
            r#"<lpdf version="1"><assets><image name="photo" src="photo.png"/></assets><document><section><layout margin="20pt"><img name="photo" {attrs}/></layout></section></document></lpdf>"#
        );
        let mut doc = crate::parse::parse(&xml).expect("parses");
        let mut images = ImageRegistry::new();
        images.load("photo", RED_PIXEL.to_vec());
        let meta = build_image_meta(&images);
        let mut layouts = doc.section_layouts();
        for section in &mut layouts {
            crate::layout::prefill_section_image_sizes(section, &meta);
        }
        let pages = crate::layout::layout_document(&layouts);
        render_pdf(&pages, &doc.fonts, &FontRegistry::new(), &images, &doc.meta, &doc.gradients, None, false, &doc.output, &[]).expect("renders")
    }

    #[test]
    fn an_image_with_a_radius_is_painted_inside_a_clip_and_one_without_is_not() {
        let (rounded, _) = page_and_groups(&render_image_page(r#"width="60pt" height="60pt" radius="12pt""#));
        let (square, _) = page_and_groups(&render_image_page(r#"width="60pt" height="60pt""#));
        let clip = rounded.find("W\nn").or_else(|| rounded.find("W n")).expect("a clip before the image");
        let draw = rounded.find("/I0 Do").expect("the image is drawn");
        assert!(clip < draw, "the clip comes before the picture: {rounded}");
        assert!(rounded.contains(" c\n") || rounded.contains(" c "), "the clip has curved corners: {rounded}");
        assert!(!square.contains("W\nn") && !square.contains("W n"), "a plain image has no clip: {square}");
    }

    #[test]
    fn a_box_around_an_image_fills_and_strokes_before_the_picture_is_drawn() {
        let (page, _) = page_and_groups(&render_image_page(r##"width="60pt" height="60pt" padding="8pt" background="#eeeeee" border="1pt #112233" radius="10pt""##));
        let fill = page.find("0.93333334 0.93333334 0.93333334 rg").unwrap_or_else(|| panic!("the background is filled: {page}"));
        let draw = page.find("/I0 Do").expect("the image is drawn");
        assert!(fill < draw, "the box is painted before the picture: {page}");
    }

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
        let pdf = render_pdf(&pages, &doc.fonts, &FontRegistry::new(), &images, &doc.meta, &doc.gradients, None, false, &doc.output, &[]).expect("renders");
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

    // ── Justified lines ───────────────────────────────────────────────────────

    /// Enough words for a column of 200pt to take several lines.
    const PARAGRAPH: &str = "Contracts layout layout as line with be on so each footers pages totals measures on be breaks \
        with to width footers straight contracts straight word breaks layout layout exactly which";

    /// The operators of the page content, one for each line.
    fn operators(content: &str) -> Vec<&str> {
        content.lines().collect()
    }

    /// The indexes of the lines that set a word spacing above zero.
    fn word_spacings(operators: &[&str]) -> Vec<usize> {
        (0..operators.len())
            .filter(|&at| operators[at].strip_suffix(" Tw").is_some_and(|gap| gap.parse::<f32>().is_ok_and(|gap| gap > 0.0)))
            .collect()
    }

    fn arrays(operators: &[&str]) -> usize {
        operators.iter().filter(|line| line.ends_with(" TJ")).count()
    }

    #[test]
    fn a_justified_line_in_a_built_in_font_is_one_string_with_a_word_spacing() {
        let pdf = render_layout(&format!(r#"<column width="200pt"><text align="justify">{PARAGRAPH}</text></column>"#));
        let page = page_and_groups(&pdf).0;
        let operators = operators(&page);
        let spaced = word_spacings(&operators);
        assert!(spaced.len() > 1, "the paragraph has justified lines: {page}");
        for at in spaced {
            assert!(operators[at + 1].ends_with(" Tj"), "the line is shown as one string: {page}");
            assert_eq!(operators[at + 2], "0 Tw", "the spacing is put back: {page}");
        }
        assert_eq!(arrays(&operators), 0, "{page}");
    }

    #[test]
    fn a_wide_space_in_a_justified_line_in_a_built_in_font_is_a_gap_like_the_others() {
        // A built-in font draws a wide space as a space. Layout splits words at it, so `Tw` widens it with the rest.
        let body = PARAGRAPH.replacen("with to", "with\u{2003}to", 1);
        let pdf = render_layout(&format!(r#"<column width="200pt"><text align="justify">{body}</text></column>"#));
        let page = page_and_groups(&pdf).0;
        assert!(page.contains("(on be breaks with to width footers) Tj"), "{page}");
        assert_eq!(arrays(&operators(&page)), 0, "{page}");
    }

    #[test]
    fn a_justified_line_in_an_embedded_font_is_an_array_with_no_word_spacing() {
        let bytes = FIXTURE_FONTS[1].1;
        let xml = format!(
            r#"<lpdf version="1"><assets><font name="brand" src="brand.ttf"/></assets><document font="brand"><section><layout><column width="200pt"><text align="justify">{PARAGRAPH}</text></column></layout></section></document></lpdf>"#
        );
        let mut doc = crate::parse::parse(&xml).expect("parses");
        crate::layout::set_font_widths(HashMap::from([("brand".to_string(), crate::shared::extract_font_widths(bytes).expect("widths"))]));
        let pages = crate::layout::layout_document(&doc.section_layouts());
        let mut registry = FontRegistry::new();
        registry.register("brand", bytes.to_vec());
        let pdf = render_pdf(&pages, &doc.fonts, &registry, &ImageRegistry::new(), &doc.meta, &doc.gradients, None, false, &doc.output, &[]).expect("renders");
        let page = page_and_groups(&pdf).0;
        let operators = operators(&page);
        assert!(arrays(&operators) > 1, "the justified lines are arrays: {page}");
        assert!(word_spacings(&operators).is_empty(), "{page}");
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
        let pdf = render_pdf(&pages, &doc.fonts, &FontRegistry::new(), &images, &doc.meta, &doc.gradients, None, false, &doc.output, &[]).expect("renders");
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].1.contains("/I0 Do"), "the image is drawn in the group: {}", groups[0].1);
        assert!(!page.contains("/I0 Do"), "{page}");
    }

    // ── Drop shadow ───────────────────────────────────────────────────────────

    /// The `x y w h` of each rectangle a content stream draws, in order.
    fn rectangle_boxes(content: &str) -> Vec<[f32; 4]> {
        content
            .lines()
            .filter_map(|line| line.strip_suffix(" re"))
            .map(|line| {
                let n: Vec<f32> = line.split(' ').map(|v| v.parse().expect("a number")).collect();
                [n[0], n[1], n[2], n[3]]
            })
            .collect()
    }

    fn curves(content: &str) -> usize {
        content.lines().filter(|line| line.ends_with(" c")).count()
    }

    fn position(content: &str, line: &str) -> usize {
        content.lines().position(|l| l == line).unwrap_or_else(|| panic!("no '{line}' in {content}"))
    }

    const BOX_SHADOW: &str = r##"<column background="#ff0000" shadow="3pt 4pt #0000ff"><text>A</text></column>"##;

    #[test]
    fn a_shadow_is_drawn_under_its_box_moved_right_and_down() {
        let (page, groups) = page_and_groups(&render_layout(BOX_SHADOW));
        assert!(groups.is_empty());
        let boxes = rectangle_boxes(&page);
        assert_eq!(boxes.len(), 2, "the shadow and the fill: {page}");
        let (shadow, fill) = (boxes[0], boxes[1]);
        assert!(position(&page, "0 0 1 rg") < position(&page, "1 0 0 rg"), "the shadow comes first: {page}");
        assert_eq!((shadow[0] - fill[0], fill[1] - shadow[1]), (3.0, 4.0), "right 3 and down 4: {page}");
        assert_eq!((shadow[2], shadow[3]), (fill[2], fill[3]), "the same size");
    }

    #[test]
    fn a_negative_offset_moves_the_shadow_left_and_up() {
        let (page, _) = page_and_groups(&render_layout(
            r##"<column background="#ff0000" shadow="-3pt -4pt #0000ff"><text>A</text></column>"##,
        ));
        let boxes = rectangle_boxes(&page);
        assert_eq!((boxes[1][0] - boxes[0][0], boxes[0][1] - boxes[1][1]), (3.0, 4.0), "{page}");
    }

    #[test]
    fn a_shadow_takes_the_radius_of_its_box() {
        let (page, _) = page_and_groups(&render_layout(
            r##"<column background="#ff0000" radius="8pt" shadow="3pt 4pt #0000ff"><text>A</text></column>"##,
        ));
        assert_eq!((rectangles(&page), curves(&page)), (0, 8), "a rounded fill and a rounded shadow: {page}");
    }

    #[test]
    fn a_shadow_takes_a_color_token() {
        let xml = r##"<lpdf version="1"><tokens><colors><color name="ink" value="#00ff00"/></colors></tokens><document><section><layout>
            <column background="#ff0000" shadow="3pt 4pt ink"><text>A</text></column></layout></section></document></lpdf>"##;
        let bytes = crate::LpdfEngine::render_xml_to_pdf_bytes(xml).expect("renders");
        let (page, _) = page_and_groups(&bytes);
        assert!(position(&page, "0 1 0 rg") < position(&page, "1 0 0 rg"), "{page}");
    }

    #[test]
    fn a_box_with_no_background_shows_its_shadow_only_outside_it() {
        let (page, _) = page_and_groups(&render_layout(r##"<column shadow="3pt 4pt #0000ff"><text>A</text></column>"##));
        assert!(page.contains("W*\nn"), "the shadow is clipped to the outside of the box: {page}");
        let (page, _) = page_and_groups(&render_layout(BOX_SHADOW));
        assert!(!page.contains("W*"), "a fill hides the part behind it, so nothing is clipped: {page}");
    }

    #[test]
    fn a_shadow_below_full_opacity_is_a_transparency_group_of_its_own() {
        let (page, groups) = page_and_groups(&render_layout(
            r##"<column background="#ff0000" shadow="3pt 4pt #0000ff 0.5"><text>A</text></column>"##,
        ));
        assert_eq!(groups.len(), 1);
        let (dict, group) = &groups[0];
        assert!(dict.contains("/Subtype /Form") && dict.contains("/I true"), "{dict}");
        assert_eq!(rectangles(group), 1, "only the shadow is in the group: {group}");
        assert!(page.contains("/GS50 gs") && page.contains("/G0 Do"), "the page fades the group: {page}");
        assert_eq!(rectangles(&page), 1, "the fill is on the page, not faded: {page}");
        assert!(position(&page, "/G0 Do") < position(&page, "1 0 0 rg"), "the shadow is under the fill: {page}");
    }

    #[test]
    fn a_shadow_at_full_opacity_is_drawn_on_the_page() {
        for shadow in ["3pt 4pt #0000ff", "3pt 4pt #0000ff 1", "3pt 4pt #0000ff 1.0", "3pt 4pt #0000ff 0.999"] {
            let (page, groups) = page_and_groups(&render_layout(&format!(
                r##"<column background="#ff0000" shadow="{shadow}"><text>A</text></column>"##
            )));
            assert!(groups.is_empty() && rectangles(&page) == 2, "{shadow}: {page}");
        }
    }

    #[test]
    fn a_shadow_at_opacity_zero_draws_nothing() {
        let (page, groups) = page_and_groups(&render_layout(
            r##"<column background="#ff0000" shadow="3pt 4pt #0000ff 0"><text>A</text></column>"##,
        ));
        assert!(groups.is_empty());
        assert_eq!(rectangles(&page), 1, "only the fill: {page}");
    }

    #[test]
    fn a_shadow_in_a_faded_box_fades_with_the_box() {
        let (page, groups) = page_and_groups(&render_layout(
            r##"<column opacity="0.5" background="#ff0000" shadow="3pt 4pt #0000ff"><text>A</text></column>"##,
        ));
        assert_eq!(groups.len(), 1);
        assert_eq!(rectangles(&groups[0].1), 2, "the shadow and the fill are in the box's group: {}", groups[0].1);
        assert_eq!(rectangles(&page), 0, "{page}");
    }

    #[test]
    fn a_faded_shadow_in_a_faded_box_is_a_group_in_its_group() {
        let (page, groups) = page_and_groups(&render_layout(
            r##"<column opacity="0.5" background="#ff0000" shadow="3pt 4pt #0000ff 0.4"><text>A</text></column>"##,
        ));
        assert_eq!(groups.len(), 2);
        let (outer_dict, outer) = &groups[1];
        assert!(outer.contains("/GS40 gs") && outer.contains("/G0 Do"), "the box's group fades the shadow's: {outer}");
        assert_eq!(rectangles(outer), 1, "the fill is in the box's group: {outer}");
        assert!(outer_dict.contains("/G0 "), "{outer_dict}");
        assert!(page.contains("/GS50 gs") && page.contains("/G1 Do") && !page.contains("/G0 Do"), "{page}");
    }

    #[test]
    fn every_opacity_a_shadow_sets_is_defined_in_the_resources_of_what_sets_it() {
        let pdf = render_layout(
            r##"<column opacity="0.5" shadow="3pt 4pt #0000ff 0.4"><text>A</text></column><column shadow="3pt 4pt #0000ff 0.7"><text>B</text></column>"##,
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
    fn a_box_that_splits_across_pages_has_its_shadow_on_each() {
        let lines: String = (1..=120).map(|n| format!("<text>Line {n}</text>")).collect();
        let pdf = render_layout(&format!(r##"<column background="#ff0000" shadow="3pt 4pt #0000ff">{lines}</column>"##));
        let pages = String::from_utf8_lossy(&pdf).matches("/Type /Page\n").count();
        let shadows = streams(&pdf).iter().map(|s| s.2.matches("0 0 1 rg").count()).sum::<usize>();
        assert!(pages > 1);
        assert_eq!(shadows, pages, "a shadow for each page");
    }

    #[test]
    fn a_layer_rect_has_a_shadow_moved_right_and_down() {
        let pdf = render_layers(
            r##"<layer><rect x="50pt" y="50pt" width="100pt" height="60pt" fill="#ff0000" shadow="5pt 6pt #0000ff"/></layer>"##,
        );
        let (page, _) = page_and_groups(&pdf);
        let boxes = rectangle_boxes(&page);
        assert!(position(&page, "0 0 1 rg") < position(&page, "1 0 0 rg"), "{page}");
        assert_eq!((boxes[1][0] - boxes[0][0], boxes[1][1] - boxes[0][1]), (-5.0, 6.0), "{page}");
        assert!(!page.contains("W*"), "{page}");
    }

    #[test]
    fn a_layer_rect_shadow_takes_the_radius_of_its_rect() {
        let pdf = render_layers(
            r##"<layer><rect x="50pt" y="50pt" width="100pt" height="60pt" radius="8pt" fill="#ff0000" shadow="5pt 6pt #0000ff"/></layer>"##,
        );
        let (page, _) = page_and_groups(&pdf);
        assert_eq!((rectangles(&page), curves(&page)), (0, 8), "{page}");
    }

    #[test]
    fn a_layer_circle_and_ellipse_have_shadows_of_their_own_shape() {
        for shape in [
            r##"<circle x="110pt" y="110pt" r="40pt" fill="#ff0000" shadow="5pt 6pt #0000ff"/>"##,
            r##"<ellipse x="90pt" y="120pt" rx="60pt" ry="30pt" fill="#ff0000" shadow="5pt 6pt #0000ff"/>"##,
        ] {
            let (page, _) = page_and_groups(&render_layers(&format!("<layer>{shape}</layer>")));
            assert_eq!((rectangles(&page), curves(&page)), (0, 8), "{shape}: {page}");
            assert!(position(&page, "0 0 1 rg") < position(&page, "1 0 0 rg"), "{shape}: {page}");
        }
    }

    #[test]
    fn a_layer_shape_with_no_fill_shows_its_shadow_only_outside_it() {
        let (page, _) = page_and_groups(&render_layers(
            r##"<layer><rect x="50pt" y="50pt" width="100pt" height="60pt" stroke="#ff0000" shadow="5pt 6pt #0000ff"/></layer>"##,
        ));
        assert!(page.contains("W*\nn"), "{page}");
    }

    #[test]
    fn a_layer_shape_shadow_below_full_opacity_is_a_group_of_its_own() {
        let (page, groups) = page_and_groups(&render_layers(
            r##"<layer><rect x="50pt" y="50pt" width="100pt" height="60pt" fill="#ff0000" shadow="5pt 6pt #0000ff 0.25"/></layer>"##,
        ));
        assert_eq!(groups.len(), 1);
        assert_eq!(rectangles(&groups[0].1), 1, "{}", groups[0].1);
        assert!(page.contains("/GS25 gs") && page.contains("/G0 Do") && rectangles(&page) == 1, "{page}");
    }

    #[test]
    fn a_layer_shape_shadow_fades_with_its_layer() {
        let (page, groups) = page_and_groups(&render_layers(
            r##"<layer opacity="0.5"><rect x="50pt" y="50pt" width="100pt" height="60pt" fill="#ff0000" shadow="5pt 6pt #0000ff"/></layer>"##,
        ));
        assert_eq!(groups.len(), 1);
        assert_eq!(rectangles(&groups[0].1), 2, "{}", groups[0].1);
        assert_eq!(rectangles(&page), 0, "{page}");
    }

    // ── Gradients ─────────────────────────────────────────────────────────────

    const FADE: &str = r##"<gradient name="fade" type="linear" from="#ff0000" to="#0000ff"/>"##;
    const RIGHT: &str = r##"<gradient name="right" type="linear" from="#ff0000" to="#0000ff" angle="90"/>"##;

    /// The PDF of a document whose tokens hold `gradients`, around the given sections.
    fn render_with_gradients(gradients: &str, sections: &str) -> Vec<u8> {
        let xml = format!(
            r#"<lpdf version="1"><tokens><gradients>{gradients}</gradients></tokens><document>{sections}</document></lpdf>"#
        );
        crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).expect("renders")
    }

    /// The same, with `body` as the layout of one section.
    fn render_gradient_layout(gradients: &str, body: &str) -> Vec<u8> {
        render_with_gradients(gradients, &format!("<section><layout>{body}</layout></section>"))
    }

    /// The same, with `shapes` in the layer of one section.
    fn render_gradient_layer(gradients: &str, shapes: &str) -> Vec<u8> {
        render_with_gradients(gradients, &format!("<section><layer>{shapes}</layer></section>"))
    }

    /// The matrix of each `cm` of a content stream, in order.
    fn matrices(content: &str) -> Vec<[f32; 6]> {
        content
            .lines()
            .filter_map(|line| line.strip_suffix(" cm"))
            .map(|line| {
                let n: Vec<f32> = line.split(' ').map(|v| v.parse().expect("a number")).collect();
                [n[0], n[1], n[2], n[3], n[4], n[5]]
            })
            .collect()
    }

    fn assert_matrix(actual: [f32; 6], expected: [f32; 6]) {
        assert!(actual.iter().zip(&expected).all(|(a, e)| (a - e).abs() < 0.01), "{actual:?} is not {expected:?}");
    }

    fn linear(angle: f32) -> Gradient {
        Gradient { kind: GradientKind::Linear, angle, stops: Vec::new() }
    }

    #[test]
    fn a_linear_gradient_runs_down_a_box_at_zero_degrees() {
        // The box is 100 wide and 50 high at (10, 20): the line starts at its top edge and ends at its bottom edge.
        assert_matrix(gradient_matrix(&linear(0.0), 10.0, 20.0, 100.0, 50.0), [0.0, -50.0, 50.0, 0.0, 60.0, 70.0]);
    }

    #[test]
    fn a_linear_gradient_runs_across_a_box_at_ninety_degrees() {
        assert_matrix(gradient_matrix(&linear(90.0), 10.0, 20.0, 100.0, 50.0), [100.0, 0.0, 0.0, 100.0, 10.0, 45.0]);
    }

    #[test]
    fn a_linear_gradient_runs_up_a_box_at_a_hundred_and_eighty_degrees() {
        assert_matrix(gradient_matrix(&linear(180.0), 10.0, 20.0, 100.0, 50.0), [0.0, 50.0, -50.0, 0.0, 60.0, 20.0]);
    }

    #[test]
    fn a_diagonal_gradient_reaches_its_colors_at_the_corners() {
        // At 45 degrees the line runs from the top left to the bottom right, and is as long as the box is along it:
        // the first color is at the top left corner, (10, 70), and the last at the bottom right, (110, 20).
        let matrix = gradient_matrix(&linear(45.0), 10.0, 20.0, 100.0, 50.0);
        let along = |x: f32, y: f32| ((x - matrix[4]) * matrix[0] + (y - matrix[5]) * matrix[1]) / (matrix[0].powi(2) + matrix[1].powi(2));
        assert!(along(10.0, 70.0).abs() < 0.001, "the top left corner is at the start: {}", along(10.0, 70.0));
        assert!((along(110.0, 20.0) - 1.0).abs() < 0.001, "the bottom right corner is at the end: {}", along(110.0, 20.0));
    }

    #[test]
    fn a_radial_gradient_is_a_circle_out_to_the_farthest_corner() {
        let radial = Gradient { kind: GradientKind::Radial, angle: 0.0, stops: Vec::new() };
        let radius = 100.0_f32.hypot(50.0) / 2.0;
        assert_matrix(gradient_matrix(&radial, 10.0, 20.0, 100.0, 50.0), [radius, 0.0, 0.0, radius, 60.0, 45.0]);
    }

    #[test]
    fn a_box_with_a_gradient_background_is_clipped_to_its_box_and_painted_with_the_shading() {
        let pdf = render_gradient_layout(FADE, r##"<column background="gradient:fade" width="100pt" height="50pt"/>"##);
        let (page, groups) = page_and_groups(&pdf);
        assert!(groups.is_empty());
        let lines: Vec<&str> = page.lines().collect();
        let clip = lines.iter().position(|line| *line == "W").unwrap_or_else(|| panic!("no clip in {page}"));
        assert!(lines[clip - 1].ends_with(" re"), "the clip is the box: {page}");
        assert_eq!(&lines[clip + 1..clip + 3], ["n", lines[clip + 2]], "{page}");
        assert!(lines[clip + 2].ends_with(" cm"), "{page}");
        assert_eq!(&lines[clip + 3..clip + 5], ["/Sh-fade sh", "Q"], "{page}");
        assert!(!page.contains("1 0 0 rg") && !page.contains("0 0 1 rg"), "no flat fill: {page}");
    }

    #[test]
    fn the_shading_is_in_the_resources_of_the_page() {
        let pdf = render_gradient_layout(FADE, r##"<column background="gradient:fade" height="50pt"/>"##);
        let text = String::from_utf8_lossy(&pdf);
        let (_, page) = object_with(&text, "/Type /Page\n");
        assert!(page.contains("/Shading") && page.contains("/Sh-fade "), "{page}");
        let (shading, dict) = object_with(&text, "/ShadingType 2");
        assert!(page.contains(&format!("/Sh-fade {shading} 0 R")), "{page}");
        assert!(dict.contains("/ColorSpace /DeviceRGB") && dict.contains("/Coords [0 0 1 0]") && dict.contains("/Extend [true true]"), "{dict}");
        let (function, function_dict) = object_with(&text, "/FunctionType 2");
        assert!(dict.contains(&format!("/Function {function} 0 R")), "{dict}");
        assert!(function_dict.contains("/C0 [1 0 0]") && function_dict.contains("/C1 [0 0 1]"), "{function_dict}");
    }

    #[test]
    fn a_document_without_a_gradient_has_no_shading() {
        let pdf = render_layout(r##"<column background="#ff0000" height="50pt"/>"##);
        let text = String::from_utf8_lossy(&pdf);
        assert!(!text.contains("/Shading") && !text.contains("/FunctionType"));
    }

    #[test]
    fn a_gradient_that_nothing_uses_is_not_written() {
        let pdf = render_gradient_layout(FADE, r##"<column background="#ff0000" height="50pt"/>"##);
        let text = String::from_utf8_lossy(&pdf);
        assert!(!text.contains("/Shading") && !text.contains("/FunctionType"));
    }

    #[test]
    fn a_gradient_used_twice_is_one_shading_painted_twice() {
        let pdf = render_gradient_layout(
            FADE,
            r##"<column background="gradient:fade" height="50pt"/><column background="gradient:fade" height="30pt"/>"##,
        );
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(text.matches("/ShadingType").count(), 1);
        let (page, _) = page_and_groups(&pdf);
        assert_eq!(page.matches("/Sh-fade sh").count(), 2, "{page}");
        let (first, second) = (matrices(&page)[0], matrices(&page)[1]);
        assert_ne!(first, second, "each box has a matrix of its own");
    }

    #[test]
    fn two_gradients_are_two_shadings_in_the_resources() {
        let pdf = render_gradient_layout(
            &format!("{FADE}{RIGHT}"),
            r##"<column background="gradient:fade" height="50pt"/><column background="gradient:right" height="30pt"/>"##,
        );
        let text = String::from_utf8_lossy(&pdf);
        let (_, page) = object_with(&text, "/Type /Page\n");
        assert!(page.contains("/Sh-fade ") && page.contains("/Sh-right "), "{page}");
        assert_eq!(text.matches("/ShadingType").count(), 2);
    }

    #[test]
    fn a_gradient_of_several_colors_is_stitched_from_a_function_for_each_pair() {
        let sunset = r##"<gradient name="sunset" type="linear"><stop offset="0%" color="#f00"/><stop offset="50%" color="#0f0"/><stop offset="100%" color="#00f"/></gradient>"##;
        let pdf = render_gradient_layout(sunset, r##"<column background="gradient:sunset" height="50pt"/>"##);
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(text.matches("/FunctionType 2").count(), 2);
        let (stitching, dict) = object_with(&text, "/FunctionType 3");
        assert!(dict.contains("/Bounds [0.5]") && dict.contains("/Encode [0 1 0 1]") && dict.contains("/Domain [0 1]"), "{dict}");
        let (_, shading) = object_with(&text, "/ShadingType 2");
        assert!(shading.contains(&format!("/Function {stitching} 0 R")), "the shading uses the stitching function: {shading}");
    }

    #[test]
    fn a_radial_gradient_is_a_radial_shading() {
        let spotlight = r##"<gradient name="spot" type="radial" from="#ffffff" to="#0000ff"/>"##;
        let pdf = render_gradient_layout(spotlight, r##"<column background="gradient:spot" height="50pt"/>"##);
        let text = String::from_utf8_lossy(&pdf);
        let (_, shading) = object_with(&text, "/ShadingType 3");
        assert!(shading.contains("/Coords [0 0 0 0 0 1]") && shading.contains("/Extend [true true]"), "{shading}");
    }

    #[test]
    fn the_matrix_of_a_box_gradient_follows_its_box() {
        let pdf = render_gradient_layout(RIGHT, r##"<column background="gradient:right" width="100pt" height="50pt"/>"##);
        let (page, _) = page_and_groups(&pdf);
        let [x, y, w, h] = rectangle_boxes(&page)[0];
        assert_matrix(matrices(&page)[0], [w, 0.0, 0.0, w, x, y + h / 2.0]);
    }

    #[test]
    fn a_box_with_a_radius_clips_the_gradient_to_its_rounded_corners() {
        let pdf = render_gradient_layout(FADE, r##"<column background="gradient:fade" radius="8pt" height="50pt"/>"##);
        let (page, _) = page_and_groups(&pdf);
        assert_eq!((rectangles(&page), curves(&page)), (0, 4), "{page}");
        assert!(position(&page, "W") < position(&page, "/Sh-fade sh"), "{page}");
    }

    #[test]
    fn a_border_is_stroked_over_the_gradient() {
        let pdf = render_gradient_layout(FADE, r##"<column background="gradient:fade" border="2pt #00ff00" height="50pt"/>"##);
        let (page, _) = page_and_groups(&pdf);
        assert!(position(&page, "/Sh-fade sh") < position(&page, "S"), "{page}");
        assert!(page.contains("0 1 0 RG"), "{page}");
        assert!(!page.contains("1 0 0 rg") && !page.contains("0 0 1 rg"), "no flat fill: {page}");
    }

    #[test]
    fn a_box_with_a_gradient_covers_its_shadow_like_one_with_a_color() {
        let pdf = render_gradient_layout(FADE, r##"<column background="gradient:fade" shadow="3pt 4pt #00ff00" height="50pt"/>"##);
        let (page, _) = page_and_groups(&pdf);
        assert!(position(&page, "0 1 0 rg") < position(&page, "/Sh-fade sh"), "the shadow comes first: {page}");
        assert!(!page.contains("W*"), "the gradient fills the box, so the shadow is not cut out of it: {page}");
    }

    #[test]
    fn a_faded_box_draws_its_gradient_in_its_group_and_the_group_has_the_shading_in_its_resources() {
        let pdf = render_gradient_layout(FADE, r##"<column opacity="0.5" background="gradient:fade" height="50pt"/>"##);
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 1);
        let (dict, group) = &groups[0];
        assert!(group.contains("/Sh-fade sh") && !page.contains("/Sh-fade sh"), "{group}");
        assert!(dict.contains("/Shading") && dict.contains("/Sh-fade "), "{dict}");
    }

    #[test]
    fn a_box_split_over_pages_has_a_gradient_on_each_page() {
        let lines: String = (1..=120).map(|n| format!("<text>Line {n}</text>")).collect();
        let pdf = render_gradient_layout(FADE, &format!(r##"<column background="gradient:fade">{lines}</column>"##));
        let text = String::from_utf8_lossy(&pdf);
        let pages = text.matches("/Type /Page\n").count();
        let painted = streams(&pdf).into_iter().filter(|(_, _, content)| content.contains("/Sh-fade sh")).count();
        assert!(pages >= 2, "the column does not fit on a page");
        assert_eq!(painted, pages, "a gradient on every page");
        assert_eq!(text.matches("/ShadingType").count(), 1);
    }

    #[test]
    fn a_page_background_gradient_spans_the_page() {
        let pdf = render_with_gradients(
            RIGHT,
            r##"<section background="gradient:right"><layout><text>Page</text></layout></section>"##,
        );
        let (page, _) = page_and_groups(&pdf);
        let (width, height) = (595.28_f32, 841.89_f32);
        assert!(position(&page, "/Sh-right sh") < position(&page, "BT"), "{page}");
        assert_matrix(matrices(&page)[0], [width, 0.0, 0.0, width, 0.0, height / 2.0]);
    }

    #[test]
    fn a_document_background_gradient_spans_each_page() {
        let xml = r##"<lpdf version="1"><tokens><gradients><gradient name="night" type="linear" from="#000000" to="#0000ff"/></gradients></tokens>
            <document background="gradient:night"><section><layout><text>One</text></layout></section></document></lpdf>"##;
        let pdf = crate::LpdfEngine::render_xml_to_pdf_bytes(xml).expect("renders");
        let (page, _) = page_and_groups(&pdf);
        assert!(page.contains("/Sh-night sh"), "{page}");
    }

    #[test]
    fn a_layer_rect_circle_and_ellipse_are_filled_with_a_gradient() {
        for (shape, curves_drawn, rectangles_drawn) in [
            (r##"<rect x="50pt" y="50pt" width="100pt" height="60pt" fill="gradient:fade"/>"##, 0, 1),
            (r##"<rect x="50pt" y="50pt" width="100pt" height="60pt" radius="8pt" fill="gradient:fade"/>"##, 4, 0),
            (r##"<circle x="110pt" y="110pt" r="40pt" fill="gradient:fade"/>"##, 4, 0),
            (r##"<ellipse x="90pt" y="120pt" rx="60pt" ry="30pt" fill="gradient:fade"/>"##, 4, 0),
        ] {
            let (page, _) = page_and_groups(&render_gradient_layer(FADE, shape));
            assert_eq!((curves(&page), rectangles(&page)), (curves_drawn, rectangles_drawn), "{shape}: {page}");
            assert!(position(&page, "W") < position(&page, "/Sh-fade sh"), "{shape}: {page}");
            assert!(!page.contains("1 0 0 rg") && !page.contains("0 0 1 rg"), "no flat fill, {shape}: {page}");
        }
    }

    #[test]
    fn a_layer_shape_gradient_spans_its_own_box() {
        let (page, _) = page_and_groups(&render_gradient_layer(
            RIGHT,
            r##"<rect x="50pt" y="50pt" width="100pt" height="60pt" fill="gradient:right"/>"##,
        ));
        let [x, y, w, h] = rectangle_boxes(&page)[0];
        assert_matrix(matrices(&page)[0], [w, 0.0, 0.0, w, x, y + h / 2.0]);
    }

    #[test]
    fn a_layer_circle_gradient_spans_the_box_around_the_circle() {
        let radial = r##"<gradient name="spot" type="radial" from="#ffffff" to="#0000ff"/>"##;
        let (page, _) = page_and_groups(&render_gradient_layer(
            radial,
            r##"<circle x="110pt" y="110pt" r="40pt" fill="gradient:spot"/>"##,
        ));
        // 80 by 80 around the circle's center, so out to the corner is 40 times the square root of 2.
        let radius = 80.0_f32.hypot(80.0) / 2.0;
        assert_matrix(matrices(&page)[0], [radius, 0.0, 0.0, radius, 150.0, 841.89 - 150.0]);
    }

    #[test]
    fn a_layer_path_is_filled_with_a_gradient_over_the_box_around_it() {
        let (page, _) = page_and_groups(&render_gradient_layer(
            RIGHT,
            r##"<path d="M 50 50 L 150 50 L 100 100 Z" fill="gradient:right"/>"##,
        ));
        assert!(position(&page, "W") < position(&page, "/Sh-right sh"), "{page}");
        // The path spans 100 by 50 from (50, 50): the gradient line runs across it.
        assert_matrix(matrices(&page)[0], [100.0, 0.0, 0.0, 100.0, 50.0, 841.89 - 75.0]);
    }

    #[test]
    fn a_layer_path_with_the_even_odd_rule_is_clipped_with_it() {
        let hollow = r##"<path d="M 50 50 L 250 50 L 250 250 L 50 250 Z M 100 100 L 200 100 L 200 200 L 100 200 Z" fill="gradient:fade" fill-rule="evenodd"/>"##;
        let (page, _) = page_and_groups(&render_gradient_layer(FADE, hollow));
        assert!(position(&page, "W*") < position(&page, "/Sh-fade sh"), "{page}");
        let solid = hollow.replace(r#" fill-rule="evenodd""#, "");
        let (page, _) = page_and_groups(&render_gradient_layer(FADE, &solid));
        assert!(!page.contains("W*") && page.contains("W\n"), "{page}");
    }

    #[test]
    fn a_layer_shape_keeps_its_stroke_over_a_gradient_fill() {
        let (page, _) = page_and_groups(&render_gradient_layer(
            FADE,
            r##"<rect x="50pt" y="50pt" width="100pt" height="60pt" fill="gradient:fade" stroke="#00ff00" stroke-width="2pt"/>"##,
        ));
        assert!(position(&page, "/Sh-fade sh") < position(&page, "S"), "{page}");
        assert!(page.contains("0 1 0 RG"), "{page}");
    }

    #[test]
    fn a_gradient_in_a_faded_layer_is_painted_in_the_layer_group() {
        let pdf = render_with_gradients(
            FADE,
            r##"<section><layer opacity="0.5"><rect x="50pt" y="50pt" width="100pt" height="60pt" fill="gradient:fade"/></layer></section>"##,
        );
        let (page, groups) = page_and_groups(&pdf);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].1.contains("/Sh-fade sh") && !page.contains("/Sh-fade sh"), "{}", groups[0].1);
        assert!(groups[0].0.contains("/Sh-fade "), "{}", groups[0].0);
    }

    // ── Colors ────────────────────────────────────────────────────────────────

    /// The licensed page of `layout`: no attribution line, which is drawn in RGB.
    fn licensed_page(layout: &str) -> String {
        page_and_groups(&render_as(layout, true)).0
    }

    #[test]
    fn cmyk_colors_fill_and_stroke_with_the_cmyk_operators() {
        let page = licensed_page(
            r##"<column background="cmyk(0, 100, 100, 0)" border="1pt cmyk(100 0 0 0)" height="40pt">
                <text color="cmyk(0, 0, 0, 100)">A</text><divider color="cmyk(0 0 100 0)"/></column>"##,
        );
        assert!(page.contains("0 1 1 0 k"), "the background: {page}");
        assert!(page.contains("1 0 0 0 K"), "the border: {page}");
        assert!(page.contains("0 0 0 1 k"), "the text: {page}");
        assert!(page.contains("0 0 1 0 K"), "the divider: {page}");
        assert!(!page.contains(" rg") && !page.contains(" RG"), "nothing is in RGB: {page}");
    }

    #[test]
    fn cmyk_values_are_written_from_0_to_1() {
        assert!(licensed_page(r##"<text color="cmyk(91, 61, 0, 19)">A</text>"##).contains("0.91 0.61 0 0.19 k"));
    }

    #[test]
    fn layer_shapes_fill_and_stroke_in_cmyk() {
        let pdf = render_layers(
            r##"<layer><rect x="10pt" y="10pt" width="40pt" height="40pt" fill="cmyk(0, 100, 100, 0)" stroke="cmyk(100 0 0 0)"/>
                <line x1="10pt" y1="80pt" x2="90pt" y2="80pt" stroke="cmyk(0, 0, 0, 100)"/></layer>"##,
        );
        let (page, _) = page_and_groups(&pdf);
        assert!(page.contains("0 1 1 0 k") && page.contains("1 0 0 0 K") && page.contains("0 0 0 1 K"), "{page}");
    }

    #[test]
    fn a_cmyk_color_token_paints_in_cmyk() {
        let xml = r##"<lpdf version="1"><tokens><colors><color name="ink" value="cmyk(0 0 0 100)"/></colors></tokens>
            <document><section><layout><text color="ink">A</text></layout></section></document></lpdf>"##;
        let (page, _) = page_and_groups(&crate::LpdfEngine::render_xml_to_pdf_bytes(xml).expect("renders"));
        assert!(page.contains("0 0 0 1 k"), "{page}");
    }

    #[test]
    fn rgb_and_hsl_render_the_same_pdf_as_their_hex() {
        let document = |color: &str| {
            render_as(
                &format!(r##"<column background="{color}" border="1pt {color}" shadow="2pt 3pt {color} 0.5"><text color="{color}">A</text></column>"##),
                false,
            )
        };
        let hex = document("#1763cf");
        for color in ["rgb(23, 99, 207)", "rgb(23 99 207)", "hsl(215, 80%, 45%)", "hsl(215 80% 45%)"] {
            assert!(document(color) == hex, "{color} renders differently from #1763cf");
        }
    }

    #[test]
    fn hex_and_cmyk_colors_mix_on_a_page() {
        let page = licensed_page(r##"<text color="#ff0000">Screen</text><text color="cmyk(91, 61, 0, 19)">Print</text>"##);
        assert!(page.contains("1 0 0 rg") && page.contains("0.91 0.61 0 0.19 k"), "{page}");
        assert!(position(&page, "1 0 0 rg") < position(&page, "0.91 0.61 0 0.19 k"), "{page}");
    }

    #[test]
    fn hex_and_cmyk_sections_mix_in_a_document() {
        let xml = r##"<lpdf version="1"><document>
            <section><layout><text color="#ff0000">Screen</text></layout></section>
            <section><layout><text color="cmyk(91 61 0 19)">Print</text></layout></section></document></lpdf>"##;
        let pdf = crate::LpdfEngine::render_xml_to_pdf_bytes(xml).expect("renders");
        let contents: Vec<String> = streams(&pdf).into_iter().map(|(.., content)| content).collect();
        assert!(contents.iter().any(|content| content.contains("1 0 0 rg") && content.contains("(Screen) Tj")), "{contents:?}");
        assert!(contents.iter().any(|content| content.contains("0.91 0.61 0 0.19 k") && content.contains("(Print) Tj")), "{contents:?}");
    }

    #[test]
    fn a_shadow_takes_a_cmyk_color_in_either_form() {
        let page = licensed_page(r##"<column background="#ff0000" shadow="3pt 4pt cmyk(0 0 0 100)"><text>A</text></column>"##);
        assert!(position(&page, "0 0 0 1 k") < position(&page, "1 0 0 rg"), "the shadow comes first: {page}");

        let pdf = render_as(r##"<column background="#ff0000" shadow="3pt 4pt cmyk(0, 0, 0, 100) 0.5"><text>A</text></column>"##, true);
        let (_, groups) = page_and_groups(&pdf);
        assert!(groups.iter().any(|(_, group)| group.contains("0 0 0 1 k")), "the faded shadow is a group: {groups:?}");
    }

    #[test]
    fn a_gradient_of_cmyk_stops_is_a_cmyk_shading() {
        let gradients = r##"<gradient name="ink" type="linear" from="cmyk(0, 100, 100, 0)" to="cmyk(100 0 0 0)"/>
            <gradient name="bands" type="radial"><stop offset="0%" color="cmyk(0 0 0 0)"/><stop offset="50%" color="cmyk(0, 0, 100, 0)"/>
            <stop offset="100%" color="cmyk(0, 0, 0, 100)"/></gradient>"##;
        let pdf = render_gradient_layout(
            gradients,
            r##"<column background="gradient:ink" height="50pt"/><column background="gradient:bands" height="50pt"/>"##,
        );
        let text = String::from_utf8_lossy(&pdf);
        let (_, axial) = object_with(&text, "/ShadingType 2");
        let (_, radial) = object_with(&text, "/ShadingType 3");
        assert!(axial.contains("/ColorSpace /DeviceCMYK") && radial.contains("/ColorSpace /DeviceCMYK"), "{axial}\n{radial}");
        assert!(text.contains("/C0 [0 1 1 0]") && text.contains("/C1 [1 0 0 0]"), "the two-color one: {text}");
        assert!(text.contains("/C0 [0 0 0 0]") && text.contains("/C1 [0 0 1 0]"), "the first band: {text}");
        assert!(text.contains("/C0 [0 0 1 0]") && text.contains("/C1 [0 0 0 1]"), "the second band: {text}");
    }

    #[test]
    fn a_gradient_that_mixes_cmyk_and_rgb_stops_is_an_error_naming_it() {
        let xml = r##"<lpdf version="1"><tokens><gradients><gradient name="mixed" type="linear" from="cmyk(0, 0, 0, 100)" to="#ffffff"/>
            </gradients></tokens><document><section><layout/></section></document></lpdf>"##;
        let err = crate::LpdfEngine::render_xml_to_pdf_bytes(xml).unwrap_err();
        assert!(err.contains("gradient 'mixed': its stops mix CMYK and RGB colors"), "{err}");
    }

    #[test]
    fn a_color_that_is_not_one_stops_the_render_and_names_it() {
        for (color, says) in [
            ("rgb(23, 99 207)", "it mixes commas and spaces"),
            ("rgb(9%, 39%, 81%)", "not percentages"),
            ("hsl(215, 0.8, 0.45)", "saturation '0.8' needs a %"),
            ("hsl(2150, 80%, 45%)", "hue '2150' is not a number from 0 to 360"),
            ("rgba(23, 99, 207, 0.5)", "fade a box or a layer with opacity"),
            ("rgb(23 99 207 / 50%)", "fade a box or a layer with opacity"),
        ] {
            let xml = format!(r#"<lpdf version="1"><document><section><layout><text color="{color}">A</text></layout></section></document></lpdf>"#);
            let err = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml).unwrap_err();
            assert!(err.contains(&format!("invalid color '{color}'")) && err.contains(says), "{color}: {err}");
        }
    }

    #[test]
    fn a_box_with_nothing_to_span_has_no_gradient_painted() {
        let mut content = Content::new();
        let gradient = linear(0.0);
        fill_with_gradient(&mut content, "fade", &gradient, (10.0, 10.0, 0.0, 50.0), false, |content| {
            content.rect(10.0, 10.0, 0.0, 50.0);
        });
        fill_with_gradient(&mut content, "fade", &gradient, (10.0, 10.0, 50.0, 0.0), false, |content| {
            content.rect(10.0, 10.0, 50.0, 0.0);
        });
        assert!(content.finish().is_empty());
    }
}
