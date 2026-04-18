use criterion::{criterion_group, criterion_main, Criterion};
use lpdf::{bench_render_xml, bench_render_xml_with_font};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures")
}

fn font_xml(font_ref: &str) -> String {
    format!(
        r#"<lpdf version="1">
  <document size="a4" margin="40pt">
    <assets>
      <font name="BenchFont" src="{font_ref}" />
    </assets>
    <pages>
      <page>
        <stack gap="m">
          <text font="BenchFont" font-size="m">The quick brown fox jumps over the lazy dog.</text>
          <text font="BenchFont" font-size="m">Pack my box with five dozen liquor jugs.</text>
          <text font="BenchFont" font-size="m">How vexingly quick daft zebras jump!</text>
          <text font="BenchFont" font-size="m">The five boxing wizards jump quickly.</text>
          <text font="BenchFont" font-size="m">Sphinx of black quartz, judge my vow.</text>
        </stack>
      </page>
    </pages>
  </document>
</lpdf>"#
    )
}

fn builtin_xml() -> String {
    r#"<lpdf version="1">
  <document size="a4" margin="40pt">
    <pages>
      <page>
        <stack gap="m">
          <text font="Helvetica" font-size="m">The quick brown fox jumps over the lazy dog.</text>
          <text font="Helvetica" font-size="m">Pack my box with five dozen liquor jugs.</text>
          <text font="Helvetica" font-size="m">How vexingly quick daft zebras jump!</text>
          <text font="Helvetica" font-size="m">The five boxing wizards jump quickly.</text>
          <text font="Helvetica" font-size="m">Sphinx of black quartz, judge my vow.</text>
        </stack>
      </page>
    </pages>
  </document>
</lpdf>"#.to_string()
}

fn bench_subsetting(c: &mut Criterion) {
    let font_path = fixtures_dir().join("bench_font.ttf");
    let font_bytes = match std::fs::read(&font_path) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("skipping fonts bench: bench_font.ttf not found at {}", font_path.display());
            eprintln!("  add a small public-domain TTF to test/fixtures/bench_font.ttf to enable this bench");
            return;
        }
    };

    let xml_builtin    = builtin_xml();
    let xml_one_font   = font_xml("bench_font.ttf");

    let mut group = c.benchmark_group("subsetting");

    group.bench_function("builtin_only", |b| {
        b.iter(|| bench_render_xml(&xml_builtin).unwrap())
    });

    group.bench_function("one_font", |b| {
        b.iter(|| bench_render_xml_with_font(&xml_one_font, "BenchFont", &font_bytes).unwrap())
    });

    group.finish();
}

criterion_group!(benches, bench_subsetting);
criterion_main!(benches);
