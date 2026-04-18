use criterion::{criterion_group, criterion_main, Criterion};
use lpdf::bench_render_xml_with_image;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures")
}

fn image_xml(name: &str) -> String {
    format!(
        r#"<lpdf version="1">
  <document size="a4" margin="0pt">
    <assets>
      <image name="{name}" src="{name}" />
    </assets>
    <pages>
      <page>
        <frame width="595pt" height="842pt">
          <img name="{name}" width="595pt" height="842pt" />
        </frame>
      </page>
    </pages>
  </document>
</lpdf>"#
    )
}

fn try_load(name: &str) -> Option<Vec<u8>> {
    let path = fixtures_dir().join(name);
    std::fs::read(&path).ok()
}

fn bench_image_formats(c: &mut Criterion) {
    let jpeg_bytes     = try_load("bench_photo.jpg");
    let png_rgb_bytes  = try_load("bench_photo_rgb.png");
    let png_rgba_bytes = try_load("bench_photo_rgba.png");

    if jpeg_bytes.is_none() && png_rgb_bytes.is_none() && png_rgba_bytes.is_none() {
        eprintln!("skipping images bench: no image fixtures found");
        eprintln!("  add these files to test/fixtures/ to enable this bench:");
        eprintln!("    bench_photo.jpg         (~100 KB JPEG, 800x600)");
        eprintln!("    bench_photo_rgb.png     (same image as RGB PNG)");
        eprintln!("    bench_photo_rgba.png    (same image as RGBA PNG)");
        return;
    }

    let mut group = c.benchmark_group("image");

    if let Some(ref bytes) = jpeg_bytes {
        let xml = image_xml("bench_photo.jpg");
        group.bench_function("jpeg", |b| {
            b.iter(|| bench_render_xml_with_image(&xml, "bench_photo.jpg", bytes).unwrap())
        });
    }

    if let Some(ref bytes) = png_rgb_bytes {
        let xml = image_xml("bench_photo_rgb.png");
        group.bench_function("png_rgb", |b| {
            b.iter(|| bench_render_xml_with_image(&xml, "bench_photo_rgb.png", bytes).unwrap())
        });
    }

    if let Some(ref bytes) = png_rgba_bytes {
        let xml = image_xml("bench_photo_rgba.png");
        group.bench_function("png_rgba", |b| {
            b.iter(|| bench_render_xml_with_image(&xml, "bench_photo_rgba.png", bytes).unwrap())
        });
    }

    group.finish();
}

criterion_group!(benches, bench_image_formats);
criterion_main!(benches);
