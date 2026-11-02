//! The PNGs a stamp draws (`stamp.rs`): decoded to 8-bit pixels, gray or RGB, with their alpha apart, and the
//! resolution the file records. A render embeds its images in `pdf.rs`, which passes a gray or RGB PNG through as it
//! is; a stamp decodes every kind, since a PDF/A file needs its pixels in RGB, and scans are often 1-bit gray.

use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::pdf::{png_chunk, png_idat_bytes, unfilter_png_rows};

const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1A\n";

/// A decoded PNG.
pub(crate) struct Png {
    pub width:  u32,
    pub height: u32,
    /// The channels of `pixels`: 1 for gray, 3 for RGB.
    pub colors: u8,
    /// The pixels, row after row, 8 bits to a channel.
    pub pixels: Vec<u8>,
    /// The alpha of each pixel, 8 bits, when one of them is not opaque.
    pub alpha:  Option<Vec<u8>>,
    /// The pixels to an inch across and down, when the file records them (`pHYs`).
    pub dpi:    Option<(f32, f32)>,
}

impl Png {
    /// The pixels in RGB: gray ones repeated in each channel.
    pub fn rgb(&self) -> Vec<u8> {
        if self.colors == 3 {
            return self.pixels.clone();
        }
        self.pixels.iter().flat_map(|&v| [v, v, v]).collect()
    }
}

/// What the image is, when it is not a PNG, for the error that says so.
pub(crate) fn not_png(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(SIGNATURE) {
        return None;
    }
    Some(if bytes.starts_with(b"\xFF\xD8\xFF") {
        "a JPEG"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "a WebP"
    } else if bytes.starts_with(b"GIF8") {
        "a GIF"
    } else if bytes.starts_with(b"\x00\x00\x00") && bytes.get(4..8) == Some(b"ftyp") {
        "an AVIF or HEIC"
    } else if bytes.starts_with(b"BM") {
        "a BMP"
    } else if bytes.starts_with(b"II\x2A\x00") || bytes.starts_with(b"MM\x00\x2A") {
        "a TIFF"
    } else if bytes.starts_with(b"%PDF") {
        "a PDF"
    } else if bytes.is_empty() {
        "empty"
    } else {
        "not an image Lpdf knows"
    })
}

/// Decode a PNG of any color type and bit depth the format allows. An interlaced PNG is an error.
pub(crate) fn decode(bytes: &[u8]) -> Result<Png, String> {
    if let Some(kind) = not_png(bytes) {
        return Err(format!("the image is {kind}, and a stamp takes a PNG: convert it to PNG"));
    }
    let header = png_chunk(bytes, b"IHDR").filter(|h| h.len() >= 13)
        .ok_or("the PNG could not be read: its header is missing or cut short")?;
    let width  = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let height = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
    let depth  = header[8];
    let kind   = header[9];
    if width == 0 || height == 0 {
        return Err("the PNG has no pixels".to_string());
    }
    if header[12] != 0 {
        return Err("the PNG is interlaced, which a stamp does not read: save it without interlacing".to_string());
    }
    let channels: usize = match (kind, depth) {
        (0, 1 | 2 | 4 | 8 | 16) => 1,
        (2, 8 | 16)             => 3,
        (3, 1 | 2 | 4 | 8)      => 1,
        (4, 8 | 16)             => 2,
        (6, 8 | 16)             => 4,
        _ => return Err(format!("the PNG could not be read: color type {kind} at {depth} bits is not one PNG allows")),
    };

    let bits = channels * depth as usize;
    let stride = (width as usize).checked_mul(bits).map(|b| b.div_ceil(8));
    let size = stride.and_then(|s| (s + 1).checked_mul(height as usize));
    let (Some(stride), Some(size)) = (stride, size) else {
        return Err(format!("the PNG is too large: {width} by {height} pixels"));
    };
    // No more than the rows hold, so a file that inflates past its own size stops there.
    let raw = decompress_to_vec_zlib_with_limit(&png_idat_bytes(bytes), size)
        .map_err(|_| "the PNG could not be read: its image data is damaged or longer than its size".to_string())?;
    let rows = unfilter_png_rows(stride, (bits / 8).max(1), height as usize, &raw)
        .ok_or("the PNG could not be read: its image data is damaged or cut short")?;

    let palette = if kind == 3 {
        let plte = png_chunk(bytes, b"PLTE").filter(|p| !p.is_empty() && p.len() % 3 == 0)
            .ok_or("the PNG could not be read: its palette is missing")?;
        Some((plte, png_chunk(bytes, b"tRNS").unwrap_or(&[])))
    } else {
        None
    };

    let colors = if matches!(kind, 0 | 4) { 1 } else { 3 };
    let count = width as usize * height as usize;
    let mut pixels = Vec::with_capacity(count * colors as usize);
    let mut alpha = Vec::with_capacity(count);
    let to_8 = |v: u16| -> u8 {
        match depth {
            16 => (v >> 8) as u8,
            8  => v as u8,
            _  => (v as u32 * 255 / ((1u32 << depth) - 1)) as u8,
        }
    };
    for row in rows.chunks_exact(stride) {
        for x in 0..width as usize {
            let at = |c: usize| sample(row, x * channels + c, depth);
            match palette {
                Some((plte, trns)) => {
                    let index = at(0) as usize;
                    let color = plte.get(index * 3..index * 3 + 3).unwrap_or(&[0, 0, 0]);
                    pixels.extend_from_slice(color);
                    alpha.push(trns.get(index).copied().unwrap_or(255));
                }
                None => {
                    for c in 0..colors as usize {
                        pixels.push(to_8(at(c)));
                    }
                    alpha.push(if matches!(kind, 4 | 6) { to_8(at(channels - 1)) } else { 255 });
                }
            }
        }
    }

    Ok(Png {
        width,
        height,
        colors,
        pixels,
        alpha: alpha.iter().any(|&a| a != 255).then_some(alpha),
        dpi: resolution(bytes),
    })
}

/// Sample `index` of a row, at `depth` bits a sample: below 8 bits, samples share bytes from the high bit down.
fn sample(row: &[u8], index: usize, depth: u8) -> u16 {
    match depth {
        16 => u16::from_be_bytes([row[index * 2], row[index * 2 + 1]]),
        8  => row[index] as u16,
        _  => {
            let bit = index * depth as usize;
            let shift = 8 - depth as usize - bit % 8;
            ((row[bit / 8] >> shift) & ((1u8 << depth) - 1)) as u16
        }
    }
}

/// The pixels to an inch the `pHYs` chunk records, across and down, when its unit is the meter.
fn resolution(bytes: &[u8]) -> Option<(f32, f32)> {
    let phys = png_chunk(bytes, b"pHYs").filter(|p| p.len() >= 9)?;
    let across = u32::from_be_bytes([phys[0], phys[1], phys[2], phys[3]]);
    let down   = u32::from_be_bytes([phys[4], phys[5], phys[6], phys[7]]);
    (phys[8] == 1 && across > 0 && down > 0).then(|| (across as f32 * 0.0254, down as f32 * 0.0254))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use miniz_oxide::deflate::compress_to_vec_zlib;

    fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(tag);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0, 0, 0, 0]); // the CRC, which the decoder doesn't check
    }

    /// A PNG of `rows` (each without its filter byte), with the chunks of `extra` before its data.
    pub(crate) fn png(width: u32, height: u32, depth: u8, kind: u8, rows: &[Vec<u8>], extra: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut out = SIGNATURE.to_vec();
        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&height.to_be_bytes());
        header.extend_from_slice(&[depth, kind, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &header);
        for (tag, data) in extra {
            chunk(&mut out, tag, data);
        }
        let raw: Vec<u8> = rows.iter().flat_map(|r| std::iter::once(0).chain(r.iter().copied())).collect();
        chunk(&mut out, b"IDAT", &compress_to_vec_zlib(&raw, 6));
        chunk(&mut out, b"IEND", &[]);
        out
    }

    /// A PNG's `pHYs` chunk of `dpi` pixels to an inch, both ways.
    pub(crate) fn phys(dpi: f32) -> (&'static [u8; 4], Vec<u8>) {
        let ppm = (dpi / 0.0254).round() as u32;
        let mut data = Vec::new();
        data.extend_from_slice(&ppm.to_be_bytes());
        data.extend_from_slice(&ppm.to_be_bytes());
        data.push(1);
        (b"pHYs", data)
    }

    #[test]
    fn an_rgb_png_decodes_to_its_pixels_with_no_alpha() {
        let png = decode(&png(2, 1, 8, 2, &[vec![255, 0, 0, 0, 0, 255]], &[])).unwrap();
        assert_eq!((png.width, png.height, png.colors), (2, 1, 3));
        assert_eq!(png.pixels, [255, 0, 0, 0, 0, 255]);
        assert!(png.alpha.is_none());
        assert!(png.dpi.is_none());
    }

    #[test]
    fn an_rgba_png_keeps_its_alpha_apart_and_an_opaque_one_has_none() {
        let png1 = decode(&png(1, 1, 8, 6, &[vec![10, 20, 30, 128]], &[])).unwrap();
        assert_eq!(png1.pixels, [10, 20, 30]);
        assert_eq!(png1.alpha, Some(vec![128]));
        let opaque = decode(&png(1, 1, 8, 6, &[vec![10, 20, 30, 255]], &[])).unwrap();
        assert!(opaque.alpha.is_none());
    }

    #[test]
    fn sixteen_bits_keep_the_high_byte() {
        let png = decode(&png(1, 1, 16, 6, &[vec![0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0x80, 0x00]], &[])).unwrap();
        assert_eq!(png.pixels, [0x12, 0x56, 0x9A]);
        assert_eq!(png.alpha, Some(vec![0x80]));
    }

    #[test]
    fn a_one_bit_gray_png_spreads_its_bits_to_black_and_white() {
        // 10 pixels: 1011001110, padded to two bytes.
        let png = decode(&png(10, 1, 1, 0, &[vec![0b1011_0011, 0b1000_0000]], &[])).unwrap();
        assert_eq!(png.colors, 1);
        assert_eq!(png.pixels, [255, 0, 255, 255, 0, 0, 255, 255, 255, 0]);
    }

    #[test]
    fn gray_with_alpha_decodes_to_gray_and_alpha() {
        let png = decode(&png(1, 1, 8, 4, &[vec![40, 0]], &[])).unwrap();
        assert_eq!((png.colors, png.pixels.clone()), (1, vec![40]));
        assert_eq!(png.alpha, Some(vec![0]));
        assert_eq!(png.rgb(), [40, 40, 40]);
    }

    #[test]
    fn an_indexed_png_reads_its_palette_and_its_transparency() {
        let extra: [(&[u8; 4], Vec<u8>); 2] = [(b"PLTE", vec![255, 0, 0, 0, 255, 0]), (b"tRNS", vec![0])];
        // Two pixels at 4 bits: index 1, then index 0.
        let png = decode(&png(2, 1, 4, 3, &[vec![0x10]], &extra)).unwrap();
        assert_eq!(png.pixels, [0, 255, 0, 255, 0, 0]);
        assert_eq!(png.alpha, Some(vec![255, 0]));
    }

    #[test]
    fn the_resolution_comes_from_phys_in_pixels_to_an_inch() {
        let png = decode(&png(1, 1, 8, 2, &[vec![0, 0, 0]], &[phys(300.0)])).unwrap();
        let (across, down) = png.dpi.unwrap();
        assert!((across - 300.0).abs() < 0.05 && (down - 300.0).abs() < 0.05);
    }

    #[test]
    fn what_is_not_a_png_is_named_in_the_error() {
        let jpeg = decode(b"\xFF\xD8\xFF\xE0rest").err().unwrap();
        assert_eq!(jpeg, "the image is a JPEG, and a stamp takes a PNG: convert it to PNG");
        assert!(decode(b"").err().unwrap().contains("is empty"));
    }

    #[test]
    fn an_interlaced_png_is_an_error_that_says_to_save_it_without() {
        let mut bytes = png(1, 1, 8, 2, &[vec![0, 0, 0]], &[]);
        bytes[8 + 8 + 12] = 1; // the interlace method of IHDR
        assert!(decode(&bytes).err().unwrap().contains("save it without interlacing"));
    }

    #[test]
    fn a_damaged_png_is_an_error_not_a_panic() {
        let mut bytes = png(4, 4, 8, 2, &[vec![0; 12], vec![0; 12], vec![0; 12], vec![0; 12]], &[]);
        bytes.truncate(40);
        assert!(decode(&bytes).is_err());
        let too_wide = png(u32::MAX, u32::MAX, 16, 6, &[], &[]);
        assert!(decode(&too_wide).is_err());
    }
}
