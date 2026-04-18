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

use pdf_writer::{Content, Filter, Name, Pdf, Rect, Ref, Str, TextStr};
use pdf_writer::types::{ActionType, AnnotationType, CidFontType, FontFlags, Predictor};
use ttf_parser::Face;
use miniz_oxide::deflate::compress_to_vec_zlib;
use miniz_oxide::inflate::decompress_to_vec_zlib;
use subsetter::GlyphRemapper;

use crate::render::{RenderNode, RenderPage, RenderedBarcodeKind};
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
            RenderNode::Image(img) => { out.insert(img.name.clone()); }
            RenderNode::Box(b)     => collect_used_images(&b.children, out),
            RenderNode::Link(l)    => collect_used_images(&l.children, out),
            _                      => {}
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
    nodes:         &[RenderNode],
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    page_h:        f32,
) {
    for node in nodes {
        draw_node(content, annots, node, fonts, image_res_map, page_h);
    }
}

fn draw_node(
    content:       &mut Content,
    annots:        &mut Vec<AnnotData>,
    node:          &RenderNode,
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    page_h:        f32,
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

            draw_nodes(content, annots, &b.children, fonts, image_res_map, page_h);
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
            draw_nodes(content, annots, &lk.children, fonts, image_res_map, page_h);

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
struct AllocatedIds {
    catalog_id:      Ref,
    info_id:         Ref,
    pages_id:        Ref,
    page_ids:        Vec<Ref>,
    content_ids:     Vec<Ref>,
    annot_ids:       Vec<Vec<Ref>>,
    font_id_map:     HashMap<String, FontIds>,
    image_id_map:    HashMap<String, Ref>,
    image_smask_ids: Vec<Option<Ref>>,
}

// ── Step 1a: Font preparation ─────────────────────────────────────────────────

/// Collect every font name referenced in `pages`, prepare each for embedding,
/// and return a stable (sorted) name list together with the prepared-font map.
///
/// Helvetica is always included so the optional watermark always has a font.
fn prepare_fonts(
    pages:     &[RenderPage],
    font_defs: &HashMap<String, FontDef>,
    registry:  &FontRegistry,
    watermark: Option<(&str, Option<&str>)>,
) -> (Vec<String>, HashMap<String, PreparedFont>) {
    let mut used_font_names: HashSet<String> = HashSet::new();
    for page in pages {
        collect_used_fonts(&page.nodes, &mut used_font_names);
    }
    used_font_names.insert("Helvetica".to_string());

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
        if name == "Helvetica" {
            if let Some((wtext, _)) = watermark {
                used.extend(wtext.chars());
            }
        }
        chars_per_font.insert(name.clone(), used);
    }

    let empty_chars: HashSet<char> = HashSet::new();
    let mut fonts: HashMap<String, PreparedFont> = HashMap::new();
    for (idx, name) in sorted_font_names.iter().enumerate() {
        let resource_name = format!("F{idx}");
        let used_chars    = chars_per_font.get(name).unwrap_or(&empty_chars);
        let kind          = resolve_font_kind(name, font_defs, registry, used_chars);
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

/// Render every page to a content-stream byte buffer and collect link
/// annotations.  Returns one `(content_bytes, annotations)` tuple per page.
fn build_content_streams(
    pages:         &[RenderPage],
    fonts:         &HashMap<String, PreparedFont>,
    image_res_map: &HashMap<String, String>,
    watermark:     Option<(&str, Option<&str>)>,
) -> Vec<(Vec<u8>, Vec<AnnotData>)> {
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
        draw_nodes(&mut content, &mut annots, &page.nodes, fonts, image_res_map, page.height);

        // Draw optional watermark — top-right corner, 8 pt Helvetica, grey.
        if let Some((wtext, wurl)) = watermark {
            let wfont  = fonts.get("Helvetica")
                .expect("Helvetica always present after prepare_fonts");
            let wsize  = 8.0_f32;
            let wpad   = 4.0_f32;  // distance from page edge
            let tw     = wfont.text_width(wtext, wsize);
            let wx     = page.width - wpad - tw;
            // In PDF bottom-up coords: baseline = height - pad - size
            let pdf_wy = page.height - wpad - wsize;

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

    rendered_pages
}

// ── Step 3: ID allocation ─────────────────────────────────────────────────────

/// Pre-allocate all PDF indirect-object IDs so that forward references (e.g.
/// a page dict referencing its content stream) are known before writing begins.
fn allocate_ids(
    rendered_pages:     &[(Vec<u8>, Vec<AnnotData>)],
    fonts:              &HashMap<String, PreparedFont>,
    sorted_font_names:  &[String],
    sorted_image_names: &[String],
    image_registry:     &ImageRegistry,
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
        .map(|(_, annots)| (0..annots.len()).map(|_| alloc.next()).collect())
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
    rendered_pages:     Vec<(Vec<u8>, Vec<AnnotData>)>,
    ids:                AllocatedIds,
    meta:               &Meta,
    created_on:         Option<&str>,
    licensed:           bool,
) -> Result<Vec<u8>, String> {
    let AllocatedIds {
        catalog_id, info_id, pages_id,
        page_ids, content_ids, annot_ids,
        font_id_map, image_id_map, image_smask_ids,
    } = ids;

    let n = pages.len();
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
        info.producer(TextStr(if licensed { "lpdf.io" } else { "lpdf.io (free)" }));

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
            {
                let mut font_res = resources.fonts();
                for name in sorted_font_names {
                    let font = &fonts[name];
                    let fid  = font_id_map[name].font_dict_id;
                    font_res.pair(Name(font.resource_name.as_bytes()), fid);
                }
            }
            // Add image XObjects to the resource dictionary.
            if !sorted_image_names.is_empty() {
                let mut xobj_res = resources.x_objects();
                for name in sorted_image_names {
                    let res_name = &image_res_map[name];
                    let img_id   = image_id_map[name];
                    xobj_res.pair(Name(res_name.as_bytes()), img_id);
                }
            }
        }

        // Add the content stream reference.
        pw.contents(content_id);

        // Add the per-page annotation array if there are any link annotations.
        let page_annot_ids = &annot_ids[i];
        if !page_annot_ids.is_empty() {
            pw.annotations(page_annot_ids.iter().copied());
        }
    }

    // -- Content streams -------------------------------------------------------
    for (i, (content_bytes, _)) in rendered_pages.iter().enumerate() {
        pdf.stream(content_ids[i], content_bytes);
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
    // (0-width) so only the URI action fires; no visual box is drawn.
    for (i, (_, page_annots)) in rendered_pages.iter().enumerate() {
        for (j, ann) in page_annots.iter().enumerate() {
            let aid       = annot_ids[i][j];
            let url_bytes = ann.url.as_bytes().to_vec();
            let mut aw    = pdf.annotation(aid);
            aw.subtype(AnnotationType::Link)
              .rect(Rect::new(ann.x1, ann.y1, ann.x2, ann.y2))
              .border(0.0, 0.0, 0.0, None);
            aw.action()
              .action_type(ActionType::Uri)
              .uri(Str(&url_bytes));
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
                pdf.stream(cmap_id, &cmap_bytes);

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
/// - `watermark`  – Optional `(text, url)`.  Drawn top-right at 8 pt Helvetica,
///                  light grey (`#aaaaaa`), 4 pt from the page edge.
/// - `created_on` – Optional ISO 8601 date string written to `/CreationDate`.
/// - `licensed`   – `true` when a valid commercial license token was supplied.
///                  Controls the `/Producer` field (`lpdf.io` vs `lpdf.io (free)`).
pub fn render_pdf(
    pages:          &[RenderPage],
    font_defs:      &HashMap<String, FontDef>,
    registry:       &FontRegistry,
    image_registry: &ImageRegistry,
    meta:           &Meta,
    watermark:      Option<(&str, Option<&str>)>,
    created_on:     Option<&str>,
    licensed:       bool,
) -> Result<Vec<u8>, String> {
    let (sorted_font_names, fonts) =
        prepare_fonts(pages, font_defs, registry, watermark);
    let (sorted_image_names, image_res_map) =
        prepare_images(pages, image_registry);
    let rendered_pages =
        build_content_streams(pages, &fonts, &image_res_map, watermark);
    let ids =
        allocate_ids(&rendered_pages, &fonts, &sorted_font_names, &sorted_image_names, image_registry);
    assemble_pdf(
        pages, &fonts, &sorted_font_names, &sorted_image_names,
        image_registry, &image_res_map, rendered_pages, ids,
        meta, created_on, licensed,
    )
}
