//! The image formats a render decodes (`raster.rs`, `bmp.rs`), from the files of `test/assets/images/formats`, which
//! `make.mjs` there writes from one pattern: each file read back to that pattern, drawn in a render, or refused with
//! an error that names the image.

use std::path::PathBuf;

use crate::raster::{self, Format, Raster};
use crate::{LpdfEngine, RenderEncryption};

/// The pattern of `make.mjs`: 40 by 30 pixels of these 16 colors, in blocks of 5 by 5.
const WIDTH: usize = 40;
const HEIGHT: usize = 30;
const PALETTE: [[u8; 3]; 16] = [
    [0, 0, 0], [255, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [0, 255, 255], [255, 0, 255],
    [128, 0, 0], [0, 128, 0], [0, 0, 128], [128, 128, 0], [0, 128, 128], [128, 0, 128], [192, 192, 192],
    [128, 128, 128],
];

fn index(x: usize, y: usize) -> usize {
    (x / 5 + y / 5 * 3) % 16
}

fn alpha(x: usize) -> u8 {
    ((x * 255) as f64 / (WIDTH - 1) as f64).round() as u8
}

fn read(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/assets/images/formats").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn decoded(name: &str) -> Raster {
    let bytes = read(name);
    let format = Format::of(&bytes).unwrap_or_else(|| panic!("{name}: not a format the engine decodes"));
    raster::decode(format, &bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The color of the pixel at `x`, `y`.
fn rgb(raster: &Raster, x: usize, y: usize) -> [u8; 3] {
    let at = (y * raster.width as usize + x) * 3;
    [raster.pixels[at], raster.pixels[at + 1], raster.pixels[at + 2]]
}

/// Render a document of one `<img>` of `bytes` at its natural size, as an SDK renders it.
fn render(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut engine = LpdfEngine::new("");
    engine.load_image("pic", bytes);
    engine.render_xml(
        r#"<lpdf version="1"><assets><image name="pic" src="pic"/></assets>
        <document><section><layout><img name="pic"/></layout></section></document></lpdf>"#,
        None,
        &RenderEncryption::default(),
    )
}

fn refused(bytes: &[u8]) -> String {
    let error = render(bytes).expect_err("a render that refuses the image");
    assert!(error.starts_with("image 'pic': "), "the error names the image: {error}");
    error
}

#[test]
fn lossless_files_read_back_to_the_colors_they_were_made_of() {
    for name in [
        "webp-lossless-alpha.webp", "gif-opaque.gif", "gif-interlaced.gif", "tiff-rgb.tif", "tiff-rgb-lzw.tif",
        "tiff-rgb-lzw-predictor.tif", "tiff-rgb-deflate.tif", "tiff-rgb-packbits.tif", "tiff-rgba.tif",
        "tiff-thumbnail.tif", "tiff-sips-alpha.tif", "bmp-sips.bmp", "bmp-sips-alpha.bmp",
    ] {
        let raster = decoded(name);
        assert_eq!((raster.width, raster.height, raster.colors), (WIDTH as u32, HEIGHT as u32, 3), "{name}");
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                // A fully transparent pixel may keep any color: a lossless WebP drops it.
                if raster.alpha.as_ref().is_some_and(|alpha| alpha[y * WIDTH + x] == 0) {
                    continue;
                }
                assert_eq!(rgb(&raster, x, y), PALETTE[index(x, y)], "{name} at ({x}, {y})");
            }
        }
    }
}

#[test]
fn alpha_is_read_where_the_file_has_it_and_nowhere_else() {
    for name in ["webp-lossless-alpha.webp", "tiff-rgba.tif", "tiff-sips-alpha.tif", "bmp-sips-alpha.bmp"] {
        let alphas = decoded(name).alpha.unwrap_or_else(|| panic!("{name} has alpha"));
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                assert_eq!(alphas[y * WIDTH + x], alpha(x), "{name} at ({x}, {y})");
            }
        }
    }
    for name in ["webp-lossy.webp", "gif-opaque.gif", "tiff-rgb.tif", "tiff-gray16.tif", "bmp-sips.bmp"] {
        assert_eq!(decoded(name).alpha, None, "{name} is opaque");
    }
}

#[test]
fn associated_alpha_is_taken_out_of_the_colors() {
    let raster = decoded("tiff-rgba-associated.tif");
    for y in 0..HEIGHT {
        // Where the alpha is small the multiplied colors kept too few bits to compare.
        for x in (0..WIDTH).filter(|&x| alpha(x) >= 64) {
            let (got, want) = (rgb(&raster, x, y), PALETTE[index(x, y)]);
            assert!(got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 4), "at ({x}, {y}): {got:?} for {want:?}");
        }
    }
}

#[test]
fn lossy_webp_reads_as_libwebp_reads_it() {
    for (name, reference) in [
        ("webp-lossy.webp", "webp-lossy-libwebp.png"),
        ("webp-lossy-alpha.webp", "webp-lossy-alpha-libwebp.png"),
    ] {
        let raster = decoded(name);
        let libwebp = crate::png::decode(&read(reference)).unwrap_or_else(|e| panic!("{reference}: {e}"));
        let (mut total, mut most) = (0u64, 0u8);
        for (&ours, theirs) in raster.pixels.iter().zip(libwebp.rgb()) {
            total += u64::from(ours.abs_diff(theirs));
            most = most.max(ours.abs_diff(theirs));
        }
        let mean = total as f64 / raster.pixels.len() as f64;
        assert!(mean < 1.0 && most <= 8, "{name}: {mean:.2} a channel on average, {most} at most");
        assert_eq!(raster.alpha, libwebp.alpha, "{name}: the alpha");
    }
}

#[test]
fn gray_tiffs_read_as_gray_from_16_bits_and_from_fax_scans() {
    let raster = decoded("tiff-gray16.tif");
    assert_eq!(raster.colors, 1);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            assert_eq!(raster.pixels[y * WIDTH + x], (index(x, y) * 17) as u8, "at ({x}, {y})");
        }
    }
    // White is zero in these, as in a fax: an odd index is black.
    for name in ["tiff-bilevel.tif", "tiff-fax-g4.tif"] {
        let raster = decoded(name);
        assert_eq!(raster.colors, 1, "{name}");
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let want = if index(x, y) % 2 == 1 { 0 } else { 255 };
                assert_eq!(raster.pixels[y * WIDTH + x], want, "{name} at ({x}, {y})");
            }
        }
    }
}

#[test]
fn the_transparent_color_of_a_gif_is_transparent() {
    let raster = decoded("gif-transparent.gif");
    let alphas = raster.alpha.as_ref().expect("alpha");
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let want = if index(x, y) == 0 { 0 } else { 255 };
            assert_eq!(alphas[y * WIDTH + x], want, "at ({x}, {y})");
        }
    }
}

#[test]
fn what_a_gif_frame_leaves_of_its_canvas_is_transparent() {
    let raster = decoded("gif-offset.gif");
    let alphas = raster.alpha.as_ref().expect("alpha");
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let inside = (10..30).contains(&x) && (5..15).contains(&y);
            assert_eq!(alphas[y * WIDTH + x], if inside { 255 } else { 0 }, "at ({x}, {y})");
            if inside {
                assert_eq!(rgb(&raster, x, y), PALETTE[index(x, y)], "at ({x}, {y})");
            }
        }
    }
}

#[test]
fn each_format_is_laid_out_at_its_natural_size() {
    for name in ["webp-lossy.webp", "gif-opaque.gif", "tiff-rgb-lzw.tif", "tiff-fax-g4.tif", "bmp-sips.bmp"] {
        assert_eq!(crate::pdf::image_natural_size(&read(name)), Some((40, 30)), "{name}");
    }
}

#[test]
fn each_format_renders_as_an_image_with_a_soft_mask_where_it_has_alpha() {
    for (name, color_space, soft_mask) in [
        ("webp-lossy.webp", "/DeviceRGB", false),
        ("webp-lossy-alpha.webp", "/DeviceRGB", true),
        ("gif-transparent.gif", "/DeviceRGB", true),
        ("tiff-rgb-deflate.tif", "/DeviceRGB", false),
        ("tiff-fax-g4.tif", "/DeviceGray", false),
        ("bmp-sips-alpha.bmp", "/DeviceRGB", true),
    ] {
        let pdf = render(&read(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let text = String::from_utf8_lossy(&pdf);
        let image = text.find("/Subtype /Image").map(|at| &text[at..]).unwrap_or_else(|| panic!("{name}: no image"));
        assert!(image.contains("/Width 40") && image.contains("/Height 30"), "{name}");
        assert!(text.contains(&format!("/ColorSpace {color_space}")), "{name}: {color_space}");
        assert_eq!(text.contains("/SMask"), soft_mask, "{name}: a soft mask is {soft_mask}");
    }
}

#[test]
fn animations_and_tiffs_of_several_pages_are_refused() {
    let error = refused(&read("gif-animated.gif"));
    assert!(error.contains("the GIF is animated, with 2 frames, and an image draws one"), "{error}");
    let error = refused(&read("webp-animated.webp"));
    assert!(error.contains("the WebP is animated, with 2 frames"), "{error}");
    let error = refused(&read("tiff-pages.tif"));
    assert!(error.contains("the TIFF has 2 pages, and an image draws one"), "{error}");
}

#[test]
fn a_thumbnail_in_a_tiff_is_not_a_page() {
    assert!(render(&read("tiff-thumbnail.tif")).is_ok());
}

#[test]
fn the_kinds_of_tiff_that_are_not_read_are_refused_with_what_to_do() {
    let error = refused(&read("tiff-jpeg.tif"));
    assert!(error.contains("compressed as JPEG") && error.contains("save it with LZW"), "{error}");
    let error = refused(&read("tiff-cmyk.tif"));
    assert!(error.contains("the TIFF is in CMYK"), "{error}");
    let error = refused(&read("tiff-fax-g3.tif"));
    assert!(error.contains("the TIFF is a Group 3 fax") && error.contains("save it with Group 4"), "{error}");
    let error = refused(&read("tiff-palette4.tif"));
    assert!(error.contains("the TIFF has a color palette"), "{error}");
}

#[test]
fn avif_and_jpeg_2000_are_refused_by_name() {
    let error = refused(b"\x00\x00\x00\x1Cftypavif\x00\x00\x00\x00");
    assert!(error.contains("AVIF/HEIC images are not supported; use JPEG, PNG, WebP, GIF, TIFF or BMP"), "{error}");
    let error = refused(b"\x00\x00\x00\x0CjP  \r\n\x87\n\x00\x00\x00\x14ftypjp2 ");
    assert!(error.contains("JPEG 2000 images are not supported"), "{error}");
    let error = refused(b"\xFF\x4F\xFF\x51\x00\x2F");
    assert!(error.contains("JPEG 2000 images are not supported"), "{error}");
}

#[test]
fn a_file_cut_short_is_an_error_and_never_a_panic() {
    for name in [
        "webp-lossy.webp", "webp-lossless-alpha.webp", "gif-opaque.gif", "gif-interlaced.gif", "tiff-rgb.tif",
        "tiff-rgb-lzw.tif", "tiff-rgb-deflate.tif", "tiff-rgb-packbits.tif", "tiff-fax-g4.tif", "bmp-sips.bmp",
        "bmp-sips-alpha.bmp",
    ] {
        let bytes = read(name);
        // A cut anywhere returns, as an error or as a render: what is cut may be only what the pixels don't need.
        for keep in [1, bytes.len() / 4, bytes.len() * 3 / 4, bytes.len() - 1] {
            let _ = render(&bytes[..keep]);
        }
        // A cut through the middle takes pixels or the header, which the render can't do without.
        assert!(render(&bytes[..bytes.len() / 2]).is_err(), "{name} cut in half rendered");
    }
}

#[test]
fn a_file_that_claims_more_pixels_than_the_memory_cap_holds_is_refused_before_they_are_read() {
    // The GIF's canvas, bytes 6 to 9, said to be 65535 by 65535: 4 billion pixels.
    let mut bytes = read("gif-offset.gif");
    bytes[6..10].copy_from_slice(&[0xFF; 4]);
    let error = refused(&bytes);
    assert!(error.contains("needs more memory than its cap of 1024 MB"), "{error}");
}
