//! BMP images, decoded to 8-bit pixels for `raster.rs`: palettes of 1, 4 and 8 bits, run-length encoded (RLE4 and
//! RLE8) or not, and 16, 24 and 32 bits to a pixel, with or without bit fields, alpha among them. A BMP that holds a
//! JPEG or a PNG inside it is refused: the file it was made from is that file.

use crate::raster::{self, Header, Raster};

const BI_RGB: u32 = 0;
const BI_RLE8: u32 = 1;
const BI_RLE4: u32 = 2;
const BI_BITFIELDS: u32 = 3;
const BI_JPEG: u32 = 4;
const BI_PNG: u32 = 5;
const BI_ALPHABITFIELDS: u32 = 6;

/// Where a BMP's file header ends and its bitmap header begins.
const FILE_HEADER: usize = 14;

/// What a BMP's headers say.
struct Bmp {
    width:       u32,
    height:      u32,
    /// Whether the first row of the pixels is the top one; most BMPs start from the bottom.
    top_down:    bool,
    bits:        u16,
    compression: u32,
    /// The bits of red, green, blue and alpha in a pixel of 16 or 32 bits.
    masks:       [u32; 4],
    /// The colors of a palette, in RGB.
    palette:     Vec<[u8; 3]>,
    /// Where the pixels start in the file.
    pixels_at:   usize,
}

fn cut_short() -> String {
    "the BMP could not be read: its header is cut short".to_string()
}

fn u16_at(bytes: &[u8], at: usize) -> Result<u16, String> {
    bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(cut_short)
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(cut_short)
}

fn read(bytes: &[u8]) -> Result<Bmp, String> {
    let pixels_at = u32_at(bytes, 10)? as usize;
    let size = u32_at(bytes, FILE_HEADER)? as usize;
    let (width, height, bits, compression) = match size {
        // The OS/2 1.x header: sizes of 16 bits, always from the bottom, never compressed.
        12 => (
            i64::from(u16_at(bytes, 18)?),
            i64::from(u16_at(bytes, 20)?),
            u16_at(bytes, 24)?,
            BI_RGB,
        ),
        40 | 52 | 56 | 64 | 108 | 124 => (
            i64::from(u32_at(bytes, 18)? as i32),
            i64::from(u32_at(bytes, 22)? as i32),
            u16_at(bytes, 28)?,
            u32_at(bytes, 30)?,
        ),
        other => return Err(format!("the BMP could not be read: a header of {other} bytes is not one BMP has")),
    };
    match compression {
        BI_JPEG => return Err("the BMP holds a JPEG, which Lpdf reads as a JPEG file: load that instead".to_string()),
        BI_PNG => return Err("the BMP holds a PNG, which Lpdf reads as a PNG file: load that instead".to_string()),
        BI_RLE8 if bits != 8 => return Err("the BMP could not be read: RLE8 needs 8 bits to a pixel".to_string()),
        BI_RLE4 if bits != 4 => return Err("the BMP could not be read: RLE4 needs 4 bits to a pixel".to_string()),
        BI_BITFIELDS | BI_ALPHABITFIELDS if !matches!(bits, 16 | 32) => {
            return Err("the BMP could not be read: bit fields need 16 or 32 bits to a pixel".to_string());
        }
        BI_RGB | BI_RLE8 | BI_RLE4 | BI_BITFIELDS | BI_ALPHABITFIELDS => {}
        other => return Err(format!("the BMP's compression ({other}) is not one Lpdf reads: save it uncompressed")),
    }
    if !matches!(bits, 1 | 4 | 8 | 16 | 24 | 32) {
        return Err(format!("the BMP has {bits} bits to a pixel, which BMP doesn't allow"));
    }
    let width = u32::try_from(width).map_err(|_| "the BMP has a negative width".to_string())?;
    let top_down = height < 0;
    let height = u32::try_from(height.unsigned_abs()).map_err(|_| "the BMP is too tall".to_string())?;

    // A bitmap header of 40 bytes puts its bit fields after it; the longer ones hold them.
    let bitfields = matches!(compression, BI_BITFIELDS | BI_ALPHABITFIELDS);
    let mut after = FILE_HEADER + size;
    let masks = if bitfields {
        let at = FILE_HEADER + 40;
        let count = if size > 40 { if size >= 56 { 4 } else { 3 } } else if compression == BI_ALPHABITFIELDS { 4 } else { 3 };
        let mut masks = [0u32; 4];
        for (i, mask) in masks.iter_mut().take(count).enumerate() {
            *mask = u32_at(bytes, at + i * 4)?;
        }
        if size == 40 {
            after += count * 4;
        }
        masks
    } else if bits == 16 {
        [0x7C00, 0x03E0, 0x001F, 0]
    } else {
        [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0]
    };

    let palette = if bits <= 8 {
        let entry = if size == 12 { 3 } else { 4 };
        let used = if size == 12 { 0 } else { u32_at(bytes, 46)? as usize };
        let most = 1usize << bits;
        let count = if used == 0 || used > most { most } else { used };
        let end = pixels_at.min(bytes.len());
        bytes
            .get(after..end)
            .unwrap_or(&[])
            .chunks_exact(entry)
            .take(count)
            .map(|bgr| [bgr[2], bgr[1], bgr[0]])
            .collect()
    } else {
        Vec::new()
    };
    Ok(Bmp { width, height, top_down, bits, compression, masks, palette, pixels_at })
}

/// The header of a BMP: its size, and whether it has alpha, which only bit fields with an alpha mask give.
pub(crate) fn header(bytes: &[u8]) -> Result<Header, String> {
    let bmp = read(bytes)?;
    let alpha = matches!(bmp.compression, BI_BITFIELDS | BI_ALPHABITFIELDS) && bmp.masks[3] != 0;
    Ok(Header { width: bmp.width, height: bmp.height, alpha })
}

/// Decode a BMP whose header is `header` to 8-bit RGB, with its alpha when it has one.
pub(crate) fn decode(bytes: &[u8], header: Header) -> Result<Raster, String> {
    let bmp = read(bytes)?;
    // Ours: RGB and alpha, and the palette indexes of a run-length encoded BMP.
    raster::reserve(header, 5)?;
    let data = bytes.get(bmp.pixels_at..).ok_or("the BMP could not be read: its pixels are missing")?;
    let (width, height) = (bmp.width as usize, bmp.height as usize);
    let count = width * height;
    let mut rgb = Vec::with_capacity(count * 3);
    let mut alpha = header.alpha.then(|| Vec::with_capacity(count));

    // The file's row of the image's row `y`, counted from the top.
    let file_row = |y: usize| if bmp.top_down { y } else { height - 1 - y };
    let color_of = |index: u8| bmp.palette.get(usize::from(index)).copied().unwrap_or([0, 0, 0]);

    if matches!(bmp.compression, BI_RLE8 | BI_RLE4) {
        let indexes = run_lengths(data, width, height, bmp.compression == BI_RLE4);
        for y in 0..height {
            let row = &indexes[file_row(y) * width..][..width];
            rgb.extend(row.iter().flat_map(|&index| color_of(index)));
        }
        return Ok(Raster { width: bmp.width, height: bmp.height, colors: 3, pixels: rgb, alpha: None });
    }

    let bits = usize::from(bmp.bits);
    let row_bytes = (width * bits).div_ceil(8);
    let stride = row_bytes.div_ceil(4) * 4;
    if data.len() < stride * (height - 1) + row_bytes {
        return Err("the BMP could not be read: its pixels are cut short".to_string());
    }
    for y in 0..height {
        let row = &data[file_row(y) * stride..][..row_bytes];
        match bits {
            1 | 4 | 8 => {
                let mask = ((1u16 << bits) - 1) as u8;
                for x in 0..width {
                    let bit = x * bits;
                    let index = (row[bit / 8] >> (8 - bits - bit % 8)) & mask;
                    rgb.extend(color_of(index));
                }
            }
            24 => {
                for bgr in row.chunks_exact(3) {
                    rgb.extend([bgr[2], bgr[1], bgr[0]]);
                }
            }
            _ => {
                let size = bits / 8;
                for pixel in row.chunks_exact(size) {
                    let value = if size == 2 {
                        u32::from(u16::from_le_bytes([pixel[0], pixel[1]]))
                    } else {
                        u32::from_le_bytes([pixel[0], pixel[1], pixel[2], pixel[3]])
                    };
                    rgb.extend([0, 1, 2].map(|c| channel(value, bmp.masks[c])));
                    if let Some(alpha) = alpha.as_mut() {
                        alpha.push(channel(value, bmp.masks[3]));
                    }
                }
            }
        }
    }
    Ok(Raster { width: bmp.width, height: bmp.height, colors: 3, pixels: rgb, alpha })
}

/// The value of the bits `mask` picks out of `pixel`, scaled to 8 bits.
fn channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let width = 32 - (mask >> shift).leading_zeros();
    let most = if width >= 32 { u64::from(u32::MAX) } else { (1u64 << width) - 1 };
    (u64::from((pixel & mask) >> shift) * 255 / most) as u8
}

/// The palette indexes of a run-length encoded BMP, row after row as the file has them. A pixel the runs skip, or
/// that they never reach when the file ends early, is index 0.
fn run_lengths(data: &[u8], width: usize, height: usize, four_bits: bool) -> Vec<u8> {
    let mut indexes = vec![0u8; width * height];
    let (mut x, mut y) = (0usize, 0usize);
    let mut put = |x: usize, y: usize, index: u8| {
        if x < width && y < height {
            indexes[y * width + x] = index;
        }
    };
    let nibble = |byte: u8, k: usize| if k % 2 == 0 { byte >> 4 } else { byte & 0x0F };
    let mut i = 0;
    while i + 1 < data.len() && y < height {
        let (count, value) = (usize::from(data[i]), data[i + 1]);
        i += 2;
        if count > 0 {
            for k in 0..count {
                put(x, y, if four_bits { nibble(value, k) } else { value });
                x += 1;
            }
            continue;
        }
        match value {
            0 => {
                x = 0;
                y += 1;
            }
            1 => break,
            2 => {
                let (Some(&dx), Some(&dy)) = (data.get(i), data.get(i + 1)) else { break };
                x += usize::from(dx);
                y += usize::from(dy);
                i += 2;
            }
            n => {
                let n = usize::from(n);
                let bytes = if four_bits { n.div_ceil(2) } else { n };
                let Some(run) = data.get(i..i + bytes) else { break };
                for k in 0..n {
                    put(x, y, if four_bits { nibble(run[k / 2], k) } else { run[k] });
                    x += 1;
                }
                // Absolute runs end on a 16-bit boundary.
                i += bytes.div_ceil(2) * 2;
            }
        }
    }
    indexes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A BMP of a bitmap header, a palette and the pixels.
    fn bmp(dib: &[u8], palette: &[u8], pixels: &[u8]) -> Vec<u8> {
        let at = FILE_HEADER + dib.len() + palette.len();
        let mut out = b"BM".to_vec();
        out.extend(((at + pixels.len()) as u32).to_le_bytes());
        out.extend([0; 4]);
        out.extend((at as u32).to_le_bytes());
        out.extend(dib);
        out.extend(palette);
        out.extend(pixels);
        out
    }

    /// A bitmap header of 40 bytes.
    fn info(width: i32, height: i32, bits: u16, compression: u32) -> Vec<u8> {
        let mut dib = Vec::new();
        dib.extend(40u32.to_le_bytes());
        dib.extend(width.to_le_bytes());
        dib.extend(height.to_le_bytes());
        dib.extend(1u16.to_le_bytes());
        dib.extend(bits.to_le_bytes());
        dib.extend(compression.to_le_bytes());
        dib.extend([0; 20]);
        dib
    }

    fn decoded(bytes: &[u8]) -> Raster {
        decode(bytes, header(bytes).unwrap()).unwrap()
    }

    #[test]
    fn a_24_bit_bmp_is_read_from_the_bottom_row_up_and_its_rows_are_padded_to_4_bytes() {
        // 2 by 2: the bottom row red and green, the top row blue and white, each row 6 bytes and 2 of padding.
        let pixels = [0, 0, 255, 0, 255, 0, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0];
        let raster = decoded(&bmp(&info(2, 2, 24, BI_RGB), &[], &pixels));
        assert_eq!((raster.width, raster.height, raster.colors), (2, 2, 3));
        assert_eq!(raster.pixels, vec![0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 0]);
        assert_eq!(raster.alpha, None);
    }

    #[test]
    fn a_negative_height_reads_the_rows_from_the_top() {
        // The file's first row is red and its second blue, in BGR.
        let pixels = [0, 0, 255, 0, 255, 0, 0, 0];
        let raster = decoded(&bmp(&info(1, -2, 24, BI_RGB), &[], &pixels));
        assert_eq!(raster.pixels, vec![255, 0, 0, 0, 0, 255]);
    }

    #[test]
    fn a_palette_bmp_takes_its_colors_from_the_palette() {
        // Index 0 is black and 1 is yellow, in BGR with a fourth byte.
        let palette = [0, 0, 0, 0, 0, 255, 255, 0];
        let raster = decoded(&bmp(&info(2, 1, 8, BI_RGB), &palette, &[1, 0, 0, 0]));
        assert_eq!(raster.pixels, vec![255, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn a_1_bit_bmp_reads_its_pixels_from_the_highest_bit() {
        let palette = [0, 0, 0, 0, 255, 255, 255, 0];
        let raster = decoded(&bmp(&info(3, 1, 1, BI_RGB), &palette, &[0b1010_0000, 0, 0, 0]));
        assert_eq!(raster.pixels, vec![255, 255, 255, 0, 0, 0, 255, 255, 255]);
    }

    #[test]
    fn the_os2_header_has_a_palette_of_3_bytes_an_entry() {
        let mut dib = Vec::new();
        dib.extend(12u32.to_le_bytes());
        dib.extend(1u16.to_le_bytes());
        dib.extend(1u16.to_le_bytes());
        dib.extend(1u16.to_le_bytes());
        dib.extend(8u16.to_le_bytes());
        let palette = [0, 0, 0, 255, 0, 0];
        let raster = decoded(&bmp(&dib, &palette, &[1, 0, 0, 0]));
        assert_eq!(raster.pixels, vec![0, 0, 255]);
    }

    #[test]
    fn rle8_runs_fill_the_rows_and_a_delta_skips_pixels() {
        let palette = [0, 0, 0, 0, 0, 0, 255, 0, 0, 255, 0, 0];
        // Bottom row: two of index 1, end of line. Top row: move right by one, one of index 2, end of the bitmap.
        let runs = [2, 1, 0, 0, 0, 2, 1, 0, 1, 2, 0, 1];
        let raster = decoded(&bmp(&info(2, 2, 8, BI_RLE8), &palette, &runs));
        assert_eq!(raster.pixels, vec![0, 0, 0, 0, 255, 0, 255, 0, 0, 255, 0, 0]);
    }

    #[test]
    fn rle4_runs_alternate_the_two_indexes_of_their_byte_and_absolute_runs_are_word_aligned() {
        let palette = [0, 0, 0, 0, 0, 0, 255, 0, 0, 255, 0, 0];
        // A run of three pixels alternating 1 and 2; an absolute run of five, in three bytes and one of padding;
        // then one pixel of index 1, which a run read off the word boundary would miss.
        let runs = [3, 0x12, 0, 5, 0x21, 0x21, 0x20, 0, 1, 0x10, 0, 1];
        let raster = decoded(&bmp(&info(9, 1, 4, BI_RLE4), &palette, &runs));
        let (red, green) = ([255, 0, 0], [0, 255, 0]);
        let expected: Vec<u8> = [red, green, red, green, red, green, red, green, red].concat();
        assert_eq!(raster.pixels, expected);
    }

    #[test]
    fn a_16_bit_bmp_is_5_5_5_without_bit_fields() {
        let white = 0x7FFFu16.to_le_bytes();
        let raster = decoded(&bmp(&info(1, 1, 16, BI_RGB), &[], &[white[0], white[1], 0, 0]));
        assert_eq!(raster.pixels, vec![255, 255, 255]);
    }

    #[test]
    fn bit_fields_with_an_alpha_mask_give_the_image_alpha() {
        let mut dib = info(1, 1, 32, BI_ALPHABITFIELDS);
        let masks: Vec<u8> =
            [0x00FF_0000u32, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000].iter().flat_map(|m| m.to_le_bytes()).collect();
        dib.extend(&masks);
        let bytes = bmp(&dib, &[], &[0x10, 0x20, 0x30, 0x80]);
        assert!(header(&bytes).unwrap().alpha);
        let raster = decoded(&bytes);
        assert_eq!(raster.pixels, vec![0x30, 0x20, 0x10]);
        assert_eq!(raster.alpha, Some(vec![0x80]));
    }

    #[test]
    fn a_32_bit_bmp_without_bit_fields_is_opaque() {
        let bytes = bmp(&info(1, 1, 32, BI_RGB), &[], &[0x10, 0x20, 0x30, 0x00]);
        assert!(!header(&bytes).unwrap().alpha);
        assert_eq!(decoded(&bytes).pixels, vec![0x30, 0x20, 0x10]);
    }

    #[test]
    fn a_bmp_that_holds_a_jpeg_is_refused() {
        let error = header(&bmp(&info(1, 1, 0, BI_JPEG), &[], &[])).unwrap_err();
        assert!(error.contains("holds a JPEG"), "{error}");
    }

    #[test]
    fn pixels_that_are_cut_short_are_an_error() {
        let bytes = bmp(&info(4, 4, 24, BI_RGB), &[], &[0; 10]);
        let error = decode(&bytes, header(&bytes).unwrap()).unwrap_err();
        assert!(error.contains("cut short"), "{error}");
    }

    #[test]
    fn a_header_that_is_cut_short_is_an_error() {
        assert!(header(b"BM\0\0\0\0\0\0\0\0\x36\0\0\0\x28\0").unwrap_err().contains("cut short"));
    }

    #[test]
    fn a_bmp_whose_pixels_would_pass_the_memory_cap_is_refused_before_they_are_read() {
        let bytes = bmp(&info(30_000, 30_000, 24, BI_RGB), &[], &[]);
        let _cap = crate::memory::start(1);
        let error = decode(&bytes, header(&bytes).unwrap()).unwrap_err();
        assert!(error.contains("its cap of 1 MB"), "{error}");
    }
}
