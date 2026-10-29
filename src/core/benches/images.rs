use criterion::{criterion_group, criterion_main, Criterion};
use lpdf::bench_render_xml_with_image;
use miniz_oxide::deflate::compress_to_vec_zlib;
use std::path::PathBuf;

/// The name the document gives its image: asset names are lowercase letters, digits and `-`.
const IMAGE: &str = "photo";

/// The size of the generated pictures.
const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn image_xml(file: &str) -> String {
    format!(
        r#"<lpdf version="1">
  <assets>
    <image name="{IMAGE}" src="{file}" />
  </assets>
  <document size="a4">
    <section>
      <layout margin="0pt">
        <frame width="595pt" height="842pt">
          <img name="{IMAGE}" width="595pt" height="842pt" />
        </frame>
      </layout>
    </section>
  </document>
</lpdf>"#
    )
}

/// A photo of the test assets, so the bench has no fixture of its own to go missing. Where it comes from is in
/// `test/assets/images/README.md`.
fn jpeg() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/assets/images/chapter-05-caterpillar.jpg");
    std::fs::read(&path).unwrap_or_else(|e| panic!("the photo of the image bench, {}, can't be read: {e}", path.display()))
}

/// One pixel of the generated picture: smooth bands under a texture, so that the PNG's deflate has work to do,
/// and an alpha that grows down the picture.
fn pixel(x: u32, y: u32) -> [u8; 4] {
    [
        (x * 255 / WIDTH) as u8,
        (y * 255 / HEIGHT) as u8,
        ((x ^ y) & 0xff) as u8,
        (128 + y * 127 / HEIGHT) as u8,
    ]
}

/// The generated picture as a PNG of 8 bits per channel, in RGB or in RGBA, with no PNG filter. The two differ
/// only in the alpha channel.
fn png(alpha: bool) -> Vec<u8> {
    let channels = if alpha { 4 } else { 3 };
    let mut scanlines = Vec::with_capacity((1 + WIDTH as usize * channels) * HEIGHT as usize);
    for y in 0..HEIGHT {
        scanlines.push(0);
        for x in 0..WIDTH {
            scanlines.extend_from_slice(&pixel(x, y)[..channels]);
        }
    }

    let mut header = Vec::new();
    header.extend_from_slice(&WIDTH.to_be_bytes());
    header.extend_from_slice(&HEIGHT.to_be_bytes());
    header.extend_from_slice(&[8, if alpha { 6 } else { 2 }, 0, 0, 0]);

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    push_chunk(&mut png, b"IHDR", &header);
    push_chunk(&mut png, b"IDAT", &compress_to_vec_zlib(&scanlines, 6));
    push_chunk(&mut png, b"IEND", &[]);
    png
}

fn push_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    png.extend_from_slice(&crc32(kind, data).to_be_bytes());
}

/// The CRC-32 of a chunk's type and data, as PNG writes it.
fn crc32(kind: &[u8], data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in kind.iter().chain(data) {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn bench_image_formats(c: &mut Criterion) {
    let jpeg = jpeg();
    let png_rgb = png(false);
    let png_rgba = png(true);

    let mut group = c.benchmark_group("image");
    for (name, file, bytes, has_mask) in [
        ("jpeg",     "photo.jpg",      &jpeg,      false),
        ("png_rgb",  "photo-rgb.png",  &png_rgb,   false),
        ("png_rgba", "photo-rgba.png", &png_rgba,  true),
    ] {
        let xml = image_xml(file);
        // An image the engine leaves out would still render a page, and the bench would time nothing.
        let pdf = bench_render_xml_with_image(&xml, IMAGE, bytes).unwrap();
        assert!(contains(&pdf, b"/Subtype /Image"), "{name}: the image is not in the PDF");
        assert_eq!(contains(&pdf, b"/SMask"), has_mask, "{name}: the alpha channel is not what the format has");
        group.bench_function(name, |b| {
            b.iter(|| bench_render_xml_with_image(&xml, IMAGE, bytes).unwrap())
        });
    }
    group.finish();
}

fn contains(pdf: &[u8], needle: &[u8]) -> bool {
    pdf.windows(needle.len()).any(|window| window == needle)
}

criterion_group!(benches, bench_image_formats);
criterion_main!(benches);
