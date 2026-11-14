//! The JPEG and PNG files a render embeds (`pdf.rs`), from the files of `test/assets/images/jpeg-png`, which `make.mjs`
//! there writes from one pattern: a gray JPEG written as gray, a CMYK JPEG and an interlaced PNG refused with an error
//! that names the image, and a palette PNG that keeps the transparency its `tRNS` chunk gives.

use std::path::PathBuf;

use miniz_oxide::inflate::decompress_to_vec_zlib;

use crate::{LpdfEngine, RenderEncryption};

/// The pattern of `make.mjs`: 40 by 30 pixels of 16 colors, in blocks of 5 by 5.
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

fn read(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/assets/images/jpeg-png").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
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
    let Err(error) = render(bytes) else { panic!("a render that refuses the image rendered") };
    assert!(error.starts_with("image 'pic': "), "the error names the image: {error}");
    error
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..].windows(needle.len()).position(|window| window == needle).map(|at| from + at)
}

/// The dictionary of each image object of `pdf`, from its `/Subtype /Image` to its end.
fn image_dictionaries(pdf: &[u8]) -> Vec<String> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = find(pdf, b"/Subtype /Image", from) {
        let end = find(pdf, b">>", at).expect("the end of the dictionary");
        found.push(String::from_utf8_lossy(&pdf[at..end]).into_owned());
        from = end;
    }
    found
}

/// The number the key `/SMask`, `/Length` and the like gives in `dictionary`, as in `/SMask 12 0 R`.
fn number_after(dictionary: &str, key: &str) -> usize {
    dictionary.split(key).nth(1).and_then(|rest| rest.split_whitespace().next()).and_then(|word| word.parse().ok())
        .unwrap_or_else(|| panic!("{key} in {dictionary}"))
}

/// The data, inflated, of the stream of object `number`, read by the `/Length` of its dictionary.
fn stream(pdf: &[u8], number: usize) -> Vec<u8> {
    let object = find(pdf, format!("\n{number} 0 obj\n").as_bytes(), 0).expect("the object");
    let start = find(pdf, b"\nstream\n", object).expect("a stream");
    let length = number_after(&String::from_utf8_lossy(&pdf[object..start]), "/Length ");
    let start = start + b"\nstream\n".len();
    decompress_to_vec_zlib(&pdf[start..start + length]).expect("a FlateDecode stream")
}

/// The pixels of the one image of `pdf` with a soft mask, and the alpha of the mask, one byte a pixel.
fn pixels_and_alpha(pdf: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let at = find(pdf, b"/SMask ", 0).expect("a soft mask");
    let mask = number_after(&String::from_utf8_lossy(&pdf[at..find(pdf, b">>", at).expect("its end")]), "/SMask ");
    // The image is the object whose dictionary holds `/SMask`: its number is on the line of its `obj`.
    let header = pdf[..at].windows(7).rposition(|window| window == b" 0 obj\n").expect("the image object");
    let line = pdf[..header].iter().rposition(|&byte| byte == b'\n').map_or(0, |at| at + 1);
    let image: usize = String::from_utf8_lossy(&pdf[line..header]).parse().expect("the image's number");
    (stream(pdf, image), stream(pdf, mask))
}

#[test]
fn a_gray_jpeg_is_written_as_gray() {
    let bytes = read("gray.jpg");
    assert_eq!(crate::pdf::image_natural_size(&bytes), Some((40, 30)));
    let pdf = render(&bytes).expect("a gray JPEG renders");
    let dictionaries = image_dictionaries(&pdf);
    let image = dictionaries.iter().find(|dictionary| dictionary.contains("/DCTDecode")).expect("the JPEG");
    assert!(image.contains("/ColorSpace /DeviceGray"), "{image}");
    assert!(image.contains("/Width 40") && image.contains("/Height 30"), "{image}");
    assert!(find(&pdf, &bytes, 0).is_some(), "the JPEG's bytes go in as they are");
}

#[test]
fn a_color_jpeg_is_written_as_rgb() {
    let bytes = read("../chapter-05-caterpillar.jpg");
    let pdf = render(&bytes).expect("a color JPEG renders");
    let dictionaries = image_dictionaries(&pdf);
    let image = dictionaries.iter().find(|dictionary| dictionary.contains("/DCTDecode")).expect("the JPEG");
    assert!(image.contains("/ColorSpace /DeviceRGB"), "{image}");
}

#[test]
fn a_cmyk_jpeg_is_refused_with_what_to_save_instead() {
    let error = refused(&read("cmyk.jpg"));
    assert!(error.contains("the JPEG is in CMYK") && error.contains("save it in RGB"), "{error}");
}

#[test]
fn an_interlaced_png_is_refused_with_what_to_save_instead() {
    let error = refused(&read("interlaced.png"));
    assert!(error.contains("the PNG is interlaced") && error.contains("save it without interlacing"), "{error}");
}

#[test]
fn a_palette_png_without_transparency_has_no_soft_mask() {
    let pdf = render(&read("palette.png")).expect("a palette PNG renders");
    assert!(find(&pdf, b"/SMask", 0).is_none());
}

#[test]
fn a_palette_png_keeps_the_transparency_its_trns_chunk_gives() {
    let pdf = render(&read("palette-trns.png")).expect("a palette PNG with transparency renders");
    let (pixels, alphas) = pixels_and_alpha(&pdf);
    assert_eq!((pixels.len(), alphas.len()), (WIDTH * HEIGHT * 3, WIDTH * HEIGHT));
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let at = y * WIDTH + x;
            // Entry 0 is transparent, and entry 1 half so: the page shows through the first and, less, the second.
            let want = match index(x, y) {
                0 => 0,
                1 => 128,
                _ => 255,
            };
            assert_eq!(alphas[at], want, "the alpha at ({x}, {y})");
            assert_eq!(pixels[at * 3..at * 3 + 3], PALETTE[index(x, y)], "the color at ({x}, {y})");
        }
    }
}

#[test]
fn a_jpeg_of_two_components_is_refused_with_what_to_save_instead() {
    // The header of a JPEG whose frame says 2 components, which is neither gray nor color.
    let error = refused(b"\xFF\xD8\xFF\xC0\x00\x0B\x08\x00\x1E\x00\x28\x02\x01\x11\x00");
    assert!(error.contains("the JPEG has 2 color components") && error.contains("save it in RGB"), "{error}");
}

#[test]
fn a_gray_or_rgb_png_keeps_the_color_its_trns_chunk_makes_transparent() {
    for (name, color_space, bits, mask) in [
        ("rgb-key.png", "/DeviceRGB", 8, "[255 255 0 0 0 0]"),
        ("gray-key.png", "/DeviceGray", 8, "[34 34]"),
        ("gray16-key.png", "/DeviceGray", 16, "[8738 8738]"),
    ] {
        let pdf = render(&read(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        let dictionaries = image_dictionaries(&pdf);
        assert_eq!(dictionaries.len(), 1, "{name}: one image, with no soft mask");
        let image = &dictionaries[0];
        assert!(image.contains(&format!("/ColorSpace {color_space}")), "{name}: {image}");
        assert!(image.contains(&format!("/BitsPerComponent {bits}")), "{name}: {image}");
        assert!(image.contains(&format!("/Mask {mask}")), "{name}: the color key, as samples of the image: {image}");
    }
}

#[test]
fn a_gray_or_rgb_png_without_trns_has_no_color_key() {
    let pdf = render(&read("rgb.png")).expect("an RGB PNG renders");
    assert!(find(&pdf, b"/Mask", 0).is_none() && find(&pdf, b"/SMask", 0).is_none());
}

// ── Samples for the external checks ──────────────────────────────────────────

/// The PDF/A-2u fixture with a gray JPEG, `jpeg-png/gray.jpg` loaded as `gray`, a palette PNG with transparency,
/// `jpeg-png/palette-trns.png` as `keyed`, and a 16-bit gray and an RGB PNG with a transparent color, `gray16-key.png`
/// as `graykey` and `rgb-key.png` as `rgbkey`, at the top of its first layout, for `make check-pdfa`.
pub(crate) fn pdfa_sample() -> Vec<u8> {
    let xml = crate::pdfa_render_tests::fixture("2u");
    let assets = xml.find("</assets>").expect("assets");
    let xml = format!(
        r#"{}<image name="gray" src="../assets/images/jpeg-png/gray.jpg"/><image name="keyed" src="../assets/images/jpeg-png/palette-trns.png"/><image name="graykey" src="../assets/images/jpeg-png/gray16-key.png"/><image name="rgbkey" src="../assets/images/jpeg-png/rgb-key.png"/>{}"#,
        &xml[..assets], &xml[assets..]
    );
    let layout = xml.find("<layout").expect("a layout");
    let end = layout + xml[layout..].find('>').expect("the end of the layout's tag") + 1;
    let pictures = r#"<img name="gray" width="160pt" alt="A pattern of 16 grays"/><img name="keyed" width="160pt" alt="A pattern of 16 colors, two of them see-through"/><img name="graykey" width="160pt" alt="A pattern of 16 grays, one of them see-through"/><img name="rgbkey" width="160pt" alt="A pattern of 16 colors, one of them see-through"/>"#;
    let xml = format!("{}{pictures}{}", &xml[..end], &xml[end..]);
    crate::pdfa_render_tests::render(&xml).expect("renders")
}
