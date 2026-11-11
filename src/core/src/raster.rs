//! The images a render decodes to pixels before it embeds them: WebP, GIF and TIFF, read with image-rs's decoders,
//! and BMP, read by `bmp.rs`. A JPEG goes into the PDF as it is, and a PNG through `pdf.rs`.
//!
//! Each image is read twice. Its header is read before the layout, for its size, whether it has alpha, and what the
//! engine refuses: an animation, a TIFF of several pages, and the kinds of pixels it doesn't read. Its pixels are read
//! once, when `pdf.rs` writes it, as 8-bit gray or RGB with the alpha apart, which `pdf.rs` writes as a soft mask.

use std::io::Cursor;
use std::num::NonZeroU64;

use tiff::decoder::{Decoder as TiffDecoder, DecodingResult, Limits};
use tiff::tags::Tag;
use tiff::ColorType;

use crate::bmp;
use crate::memory;

/// A format the engine decodes to pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    WebP,
    Gif,
    Tiff,
    Bmp,
}

impl Format {
    /// The format of `bytes`, from its signature, when it is one the engine decodes.
    pub(crate) fn of(bytes: &[u8]) -> Option<Format> {
        if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            Some(Format::WebP)
        } else if bytes.starts_with(b"GIF8") {
            Some(Format::Gif)
        } else if [b"II\x2A\x00", b"MM\x00\x2A", b"II\x2B\x00", b"MM\x00\x2B"].iter().any(|s| bytes.starts_with(*s)) {
            Some(Format::Tiff)
        } else if bytes.starts_with(b"BM") {
            Some(Format::Bmp)
        } else {
            None
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Format::WebP => "WebP",
            Format::Gif  => "GIF",
            Format::Tiff => "TIFF",
            Format::Bmp  => "BMP",
        }
    }
}

/// What an image's header says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub width:  u32,
    pub height: u32,
    /// Whether its pixels carry alpha, and so a soft mask goes with it in the PDF.
    pub alpha:  bool,
}

/// An image decoded to 8-bit pixels.
#[derive(Debug)]
pub(crate) struct Raster {
    pub width:  u32,
    pub height: u32,
    /// The channels of `pixels`: 1 for gray, 3 for RGB.
    pub colors: u8,
    /// The pixels, row after row, top first, a byte to a channel.
    pub pixels: Vec<u8>,
    /// The alpha of each pixel, a byte each, when the header says the image has alpha.
    pub alpha:  Option<Vec<u8>>,
}

impl Raster {
    fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Raster {
        let mut pixels = Vec::with_capacity(rgba.len() / 4 * 3);
        let mut alpha = Vec::with_capacity(rgba.len() / 4);
        for pixel in rgba.chunks_exact(4) {
            pixels.extend_from_slice(&pixel[..3]);
            alpha.push(pixel[3]);
        }
        Raster { width, height, colors: 3, pixels, alpha: Some(alpha) }
    }

    fn pixel_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// Keep the alpha only when the header says there is one, so the soft mask `pdf.rs` made room for from the header
    /// is the one it writes: opaque where the pixels had none.
    fn with_alpha(mut self, alpha: bool) -> Raster {
        if !alpha {
            self.alpha = None;
        } else if self.alpha.is_none() {
            self.alpha = Some(vec![255; self.pixel_count()]);
        }
        self
    }
}

/// Read the header of an image of `format`, refusing what the engine doesn't draw.
pub(crate) fn header(format: Format, bytes: &[u8]) -> Result<Header, String> {
    let header = match format {
        Format::WebP => webp_header(bytes),
        Format::Gif  => gif_header(bytes),
        Format::Tiff => tiff_header(bytes),
        Format::Bmp  => bmp::header(bytes),
    }?;
    if header.width == 0 || header.height == 0 {
        return Err(format!("the {} has no pixels", format.name()));
    }
    Ok(header)
}

/// Decode an image of `format` to 8-bit pixels, after checking that its pixels fit in the render's memory cap.
pub(crate) fn decode(format: Format, bytes: &[u8]) -> Result<Raster, String> {
    let header = header(format, bytes)?;
    let raster = match format {
        Format::WebP => webp_pixels(bytes, header),
        Format::Gif  => gif_pixels(bytes, header),
        Format::Tiff => tiff_pixels(bytes, header),
        Format::Bmp  => bmp::decode(bytes, header),
    }?;
    Ok(raster.with_alpha(header.alpha))
}

/// Check, before a decoder allocates them, that `bytes_per_pixel` bytes for each pixel of `header` fit in the cap.
pub(crate) fn reserve(header: Header, bytes_per_pixel: u64) -> Result<(), String> {
    memory::reserve(u64::from(header.width) * u64::from(header.height) * bytes_per_pixel)
}

fn animated(format: Format, frames: u64) -> String {
    format!(
        "the {} is animated, with {frames} frames, and an image draws one: save the frame it should show as a still \
        image",
        format.name()
    )
}

// ── WebP ──────────────────────────────────────────────────────────────────────

fn webp_decoder(bytes: &[u8]) -> Result<image_webp::WebPDecoder<Cursor<&[u8]>>, String> {
    image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|e| format!("the WebP could not be read: {e}"))
}

fn webp_header(bytes: &[u8]) -> Result<Header, String> {
    let decoder = webp_decoder(bytes)?;
    if decoder.is_animated() && decoder.num_frames() > 1 {
        return Err(animated(Format::WebP, u64::from(decoder.num_frames())));
    }
    let (width, height) = decoder.dimensions();
    Ok(Header { width, height, alpha: decoder.has_alpha() })
}

fn webp_pixels(bytes: &[u8], header: Header) -> Result<Raster, String> {
    // The decoded RGBA, and for a lossless image without alpha a second RGBA it decodes into first, then ours.
    reserve(header, 12)?;
    let mut decoder = webp_decoder(bytes)?;
    let size = decoder.output_buffer_size().ok_or("the WebP is too large to decode")?;
    let mut buffer = vec![0; size];
    decoder.read_image(&mut buffer).map_err(|e| format!("the WebP could not be read: {e}"))?;
    let (width, height) = (header.width, header.height);
    Ok(if decoder.has_alpha() {
        Raster::from_rgba(width, height, &buffer)
    } else {
        Raster { width, height, colors: 3, pixels: buffer, alpha: None }
    })
}

// ── GIF ───────────────────────────────────────────────────────────────────────

fn gif_error(e: gif::DecodingError) -> String {
    format!("the GIF could not be read: {e}")
}

fn gif_header(bytes: &[u8]) -> Result<Header, String> {
    let mut options = gif::DecodeOptions::new();
    // Only the frames' headers: their pixels are skipped, so counting them costs little.
    options.skip_frame_decoding(true);
    let mut decoder = options.read_info(Cursor::new(bytes)).map_err(gif_error)?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let first = decoder.next_frame_info().map_err(gif_error)?.ok_or("the GIF has no image in it")?;
    // What the first frame leaves of the canvas uncovered is transparent, as a browser shows it.
    let covers = first.left == 0
        && first.top == 0
        && u32::from(first.width) >= width
        && u32::from(first.height) >= height;
    let alpha = first.transparent.is_some() || !covers;
    let mut frames = 1u64;
    while decoder.next_frame_info().map_err(gif_error)?.is_some() {
        frames += 1;
    }
    if frames > 1 {
        return Err(animated(Format::Gif, frames));
    }
    Ok(Header { width, height, alpha })
}

fn gif_pixels(bytes: &[u8], header: Header) -> Result<Raster, String> {
    // The frame's RGBA, as large as the canvas at most, the canvas, and ours.
    reserve(header, 12)?;
    let (width, height) = (header.width as usize, header.height as usize);
    let canvas_bytes = (width * height * 4) as u64;
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    // A frame larger than the canvas is refused: it would be cut to the canvas anyway.
    options.set_memory_limit(gif::MemoryLimit::Bytes(NonZeroU64::new(canvas_bytes).unwrap_or(NonZeroU64::MIN)));
    let mut decoder = options.read_info(Cursor::new(bytes)).map_err(gif_error)?;
    let frame = decoder.read_next_frame().map_err(gif_error)?.ok_or("the GIF has no image in it")?;

    let mut canvas = vec![0u8; width * height * 4];
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let frame_width = usize::from(frame.width).max(1);
    for (row, line) in frame.buffer.chunks_exact(frame_width * 4).enumerate() {
        let y = top + row;
        if y >= height || left >= width {
            break;
        }
        let shown = frame_width.min(width - left);
        let at = (y * width + left) * 4;
        canvas[at..at + shown * 4].copy_from_slice(&line[..shown * 4]);
    }
    Ok(Raster::from_rgba(header.width, header.height, &canvas))
}

// ── TIFF ──────────────────────────────────────────────────────────────────────

fn tiff_error(e: tiff::TiffError) -> String {
    format!("the TIFF could not be read: {e}")
}

fn tiff_decoder(bytes: &[u8]) -> Result<TiffDecoder<Cursor<&[u8]>>, String> {
    let mut limits = Limits::default();
    // The memory cap bounds the pixels, checked before they are read (`reserve`).
    limits.decoding_buffer_size = usize::MAX;
    limits.intermediate_buffer_size = usize::MAX;
    Ok(TiffDecoder::new(Cursor::new(bytes)).map_err(tiff_error)?.with_limits(limits))
}

/// The kinds of TIFF pixels the engine reads: gray or RGB, with alpha or without.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TiffPixels {
    /// Gray, the samples of each pixel: 1, or 2 with alpha.
    Gray { samples: u8 },
    /// RGB, the samples of each pixel: 3, or 4 with alpha.
    Rgb { samples: u8 },
}

impl TiffPixels {
    fn of(color: ColorType) -> Result<(TiffPixels, u8), String> {
        let read = |bits: u8, pixels: TiffPixels| -> Result<(TiffPixels, u8), String> {
            let allowed = match pixels {
                TiffPixels::Gray { samples: 1 } => matches!(bits, 1 | 2 | 4 | 8 | 16),
                _ => matches!(bits, 8 | 16),
            };
            if allowed {
                Ok((pixels, bits))
            } else {
                Err(format!("the TIFF has {bits}-bit samples, which Lpdf doesn't read: save it with 8-bit samples"))
            }
        };
        match color {
            ColorType::Gray(bits)  => read(bits, TiffPixels::Gray { samples: 1 }),
            ColorType::GrayA(bits) => read(bits, TiffPixels::Gray { samples: 2 }),
            ColorType::Multiband { bit_depth, num_samples: 2 } => read(bit_depth, TiffPixels::Gray { samples: 2 }),
            ColorType::RGB(bits)   => read(bits, TiffPixels::Rgb { samples: 3 }),
            ColorType::RGBA(bits)  => read(bits, TiffPixels::Rgb { samples: 4 }),
            ColorType::CMYK(_) | ColorType::CMYKA(_) => {
                Err("the TIFF is in CMYK, which Lpdf doesn't read: save it in RGB".to_string())
            }
            ColorType::YCbCr(_) => Err("the TIFF is in YCbCr, which Lpdf doesn't read: save it in RGB".to_string()),
            ColorType::Lab(_)   => Err("the TIFF is in CIE Lab, which Lpdf doesn't read: save it in RGB".to_string()),
            _ => Err("the TIFF's kind of pixels is not one Lpdf reads: save it in RGB or gray".to_string()),
        }
    }

    fn alpha(self) -> bool {
        matches!(self, TiffPixels::Gray { samples: 2 } | TiffPixels::Rgb { samples: 4 })
    }
}

/// The error of a TIFF compression the engine doesn't read, by the number of the `Compression` tag: it reads none
/// (1), Group 4 fax (4), LZW (5), Deflate (8 and 32946) and PackBits (32773).
fn tiff_compression_error(compression: u16) -> Option<String> {
    match compression {
        1 | 4 | 5 | 8 | 32773 | 32946 => None,
        2 | 3 => Some(
            "the TIFF is a Group 3 fax, which Lpdf doesn't read: save it with Group 4 or LZW compression".to_string(),
        ),
        6 | 7 => Some(
            "the TIFF's pixels are compressed as JPEG, which Lpdf doesn't read in a TIFF: save it with LZW or no \
            compression, or as a JPEG file"
                .to_string(),
        ),
        other => Some(format!(
            "the TIFF's pixels are compressed with a method Lpdf doesn't read ({other}): save it with LZW or no \
            compression"
        )),
    }
}

fn tiff_header(bytes: &[u8]) -> Result<Header, String> {
    let mut decoder = tiff_decoder(bytes)?;
    let (width, height) = decoder.dimensions().map_err(tiff_error)?;
    // The compression and a palette first: the decoder refuses either with a message that says less.
    let compression = decoder.find_tag_unsigned::<u16>(Tag::Compression).map_err(tiff_error)?.unwrap_or(1);
    if let Some(error) = tiff_compression_error(compression) {
        return Err(error);
    }
    if decoder.find_tag_unsigned::<u16>(Tag::PhotometricInterpretation).map_err(tiff_error)? == Some(3) {
        return Err("the TIFF has a color palette, which Lpdf doesn't read: save it in RGB".to_string());
    }
    // Each channel in a plane of its own, which the decoder reads only the first of.
    let samples = decoder.find_tag_unsigned::<u16>(Tag::SamplesPerPixel).map_err(tiff_error)?.unwrap_or(1);
    if samples > 1 && decoder.find_tag_unsigned::<u16>(Tag::PlanarConfiguration).map_err(tiff_error)? == Some(2) {
        return Err(
            "the TIFF keeps each color in a plane of its own, which Lpdf doesn't read: save it with the pixels \
            interleaved"
                .to_string(),
        );
    }
    let (pixels, _) = TiffPixels::of(decoder.colortype().map_err(tiff_error)?)?;
    // A reduced-resolution copy of the image, such as a thumbnail, is not a page of its own.
    let mut pages = 1u64;
    while decoder.more_images() {
        decoder.next_image().map_err(tiff_error)?;
        let subfile = decoder.find_tag_unsigned::<u32>(Tag::NewSubfileType).map_err(tiff_error)?.unwrap_or(0);
        if subfile & 1 == 0 {
            pages += 1;
        }
    }
    if pages > 1 {
        return Err(format!(
            "the TIFF has {pages} pages, and an image draws one: save each page as a file of its own"
        ));
    }
    Ok(Header { width, height, alpha: pixels.alpha() })
}

fn tiff_pixels(bytes: &[u8], header: Header) -> Result<Raster, String> {
    let mut decoder = tiff_decoder(bytes)?;
    let (pixels, bits) = TiffPixels::of(decoder.colortype().map_err(tiff_error)?)?;
    // The decoded samples, up to 16 bits each of four, then ours.
    reserve(header, 12)?;
    // Associated alpha has the colors multiplied by it already, which the soft mask would do a second time.
    let associated = decoder
        .find_tag_unsigned_vec::<u16>(Tag::ExtraSamples)
        .map_err(tiff_error)?
        .is_some_and(|extra| extra.first() == Some(&1));
    // 16-bit samples are narrowed to 8 as they are read; fewer than 8 stay packed until `unpack`.
    let bits = bits.min(8);
    let samples = match decoder.read_image().map_err(tiff_error)? {
        DecodingResult::U8(samples) => samples,
        DecodingResult::U16(samples) => samples.iter().map(|&s| (s >> 8) as u8).collect(),
        _ => return Err(
            "the TIFF's samples are floating-point or signed numbers, which Lpdf doesn't read: save it with 8-bit \
            samples"
                .to_string(),
        ),
    };
    let (width, height) = (header.width, header.height);
    let raster = match pixels {
        TiffPixels::Gray { samples: 1 } => {
            let gray = unpack(&samples, width, height, bits, |v| scale_to_8(v, bits))?;
            Raster { width, height, colors: 1, pixels: gray, alpha: None }
        }
        TiffPixels::Gray { samples: channels } | TiffPixels::Rgb { samples: channels } => {
            let channels = usize::from(channels);
            let count = width as usize * height as usize;
            if samples.len() < count * channels {
                return Err("the TIFF could not be read: its image data is cut short".to_string());
            }
            let colors = if pixels.alpha() { channels - 1 } else { channels };
            let mut color = Vec::with_capacity(count * colors);
            let mut alpha = pixels.alpha().then(|| Vec::with_capacity(count));
            for pixel in samples.chunks_exact(channels).take(count) {
                match alpha.as_mut() {
                    Some(alpha) => {
                        let a = pixel[colors];
                        alpha.push(a);
                        color.extend(pixel[..colors].iter().map(|&c| if associated { unmultiply(c, a) } else { c }));
                    }
                    None => color.extend_from_slice(pixel),
                }
            }
            Raster { width, height, colors: colors as u8, pixels: color, alpha }
        }
    };
    Ok(raster)
}

/// The samples of rows packed `bits` to a sample, each row starting on a byte, one byte each after `each`.
fn unpack(packed: &[u8], width: u32, height: u32, bits: u8, each: impl Fn(u8) -> u8) -> Result<Vec<u8>, String> {
    let (width, height) = (width as usize, height as usize);
    if bits == 8 {
        if packed.len() < width * height {
            return Err("the TIFF could not be read: its image data is cut short".to_string());
        }
        return Ok(packed[..width * height].iter().map(|&v| each(v)).collect());
    }
    let bits = usize::from(bits);
    let stride = (width * bits).div_ceil(8);
    if packed.len() < stride * height {
        return Err("the TIFF could not be read: its image data is cut short".to_string());
    }
    let mask = (1u16 << bits) as u8 - 1;
    let mut out = Vec::with_capacity(width * height);
    for row in packed.chunks_exact(stride).take(height) {
        for x in 0..width {
            let bit = x * bits;
            let shift = 8 - bits - bit % 8;
            out.push(each((row[bit / 8] >> shift) & mask));
        }
    }
    Ok(out)
}

/// A sample of `bits` bits, scaled to 8: the darkest 0 and the lightest 255.
fn scale_to_8(value: u8, bits: u8) -> u8 {
    if bits == 8 {
        return value;
    }
    (u32::from(value) * 255 / ((1u32 << bits) - 1)) as u8
}

fn unmultiply(color: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    (u32::from(color) * 255 / u32::from(alpha)).min(255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_formats_are_told_from_their_signatures() {
        assert_eq!(Format::of(b"RIFF\0\0\0\0WEBPVP8 "), Some(Format::WebP));
        assert_eq!(Format::of(b"GIF89a"), Some(Format::Gif));
        assert_eq!(Format::of(b"GIF87a"), Some(Format::Gif));
        assert_eq!(Format::of(b"II\x2A\x00"), Some(Format::Tiff));
        assert_eq!(Format::of(b"MM\x00\x2A"), Some(Format::Tiff));
        assert_eq!(Format::of(b"II\x2B\x00"), Some(Format::Tiff));
        assert_eq!(Format::of(b"BM"), Some(Format::Bmp));
        assert_eq!(Format::of(b"\x89PNG\r\n\x1A\n"), None);
        assert_eq!(Format::of(b"\xFF\xD8\xFF"), None);
    }

    #[test]
    fn packed_samples_are_read_from_the_highest_bit_and_each_row_starts_on_a_byte() {
        // 3 pixels of 1 bit each, two rows: 101 then 010, each row padded to a byte.
        let rows = [0b1010_0000, 0b0100_0000];
        assert_eq!(unpack(&rows, 3, 2, 1, |v| v).unwrap(), vec![1, 0, 1, 0, 1, 0]);
        // 2 pixels of 4 bits: 0x3 and 0xC.
        assert_eq!(unpack(&[0x3C], 2, 1, 4, |v| v).unwrap(), vec![3, 12]);
    }

    #[test]
    fn packed_samples_that_are_cut_short_are_an_error() {
        assert!(unpack(&[0xFF], 9, 1, 1, |v| v).is_err());
    }

    #[test]
    fn a_sample_is_scaled_to_8_bits_from_black_to_white() {
        assert_eq!(scale_to_8(0, 1), 0);
        assert_eq!(scale_to_8(1, 1), 255);
        assert_eq!(scale_to_8(2, 2), 170);
        assert_eq!(scale_to_8(15, 4), 255);
    }

    #[test]
    fn associated_alpha_is_taken_out_of_the_colors() {
        assert_eq!(unmultiply(64, 128), 127);
        assert_eq!(unmultiply(200, 255), 200);
        assert_eq!(unmultiply(10, 0), 0);
    }

    #[test]
    fn the_alpha_follows_the_header() {
        let raster = Raster { width: 2, height: 1, colors: 1, pixels: vec![0, 0], alpha: None };
        assert_eq!(raster.with_alpha(true).alpha, Some(vec![255, 255]));
        let raster = Raster { width: 1, height: 1, colors: 1, pixels: vec![0], alpha: Some(vec![9]) };
        assert_eq!(raster.with_alpha(false).alpha, None);
    }
}
