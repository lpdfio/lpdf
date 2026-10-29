use criterion::{criterion_group, criterion_main, Criterion};
use lpdf::{bench_render_xml, bench_render_xml_with_font};
use std::path::PathBuf;

/// The font file the bench embeds: Crimson Text, one of the fonts the tests keep in `test/assets/fonts` (SIL Open
/// Font License, `OFL-CrimsonText.txt` beside it), so the bench has no fixture of its own to go missing.
const FONT_FILE: &str = "CrimsonText-Regular.ttf";

fn font_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/assets/fonts").join(FONT_FILE)
}

fn font_xml(font_ref: &str) -> String {
    format!(
        r#"<lpdf version="1">
  <assets>
    <font name="bench-font" src="{font_ref}" />
  </assets>
  <document size="a4">
    <section>
      <layout margin="40pt">
        <column gap="m">
          <text font="bench-font" font-size="m">The quick brown fox jumps over the lazy dog.</text>
          <text font="bench-font" font-size="m">Pack my box with five dozen liquor jugs.</text>
          <text font="bench-font" font-size="m">How vexingly quick daft zebras jump!</text>
          <text font="bench-font" font-size="m">The five boxing wizards jump quickly.</text>
          <text font="bench-font" font-size="m">Sphinx of black quartz, judge my vow.</text>
        </column>
      </layout>
    </section>
  </document>
</lpdf>"#
    )
}

fn builtin_xml() -> String {
    r#"<lpdf version="1">
  <document size="a4">
    <section>
      <layout margin="40pt">
        <column gap="m">
          <text font="Helvetica" font-size="m">The quick brown fox jumps over the lazy dog.</text>
          <text font="Helvetica" font-size="m">Pack my box with five dozen liquor jugs.</text>
          <text font="Helvetica" font-size="m">How vexingly quick daft zebras jump!</text>
          <text font="Helvetica" font-size="m">The five boxing wizards jump quickly.</text>
          <text font="Helvetica" font-size="m">Sphinx of black quartz, judge my vow.</text>
        </column>
      </layout>
    </section>
  </document>
</lpdf>"#.to_string()
}

fn bench_subsetting(c: &mut Criterion) {
    let path = font_path();
    let font_bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("the font of the fonts bench, {}, can't be read: {e}", path.display()));

    let xml_builtin    = builtin_xml();
    let xml_one_font   = font_xml(FONT_FILE);

    // A font the engine leaves out would still render a page, and the bench would time nothing.
    let embedded = bench_render_xml_with_font(&xml_one_font, "bench-font", &font_bytes).unwrap();
    assert!(embedded.windows(b"/FontFile2".len()).any(|window| window == b"/FontFile2"), "the font is not embedded in the PDF");

    let mut group = c.benchmark_group("subsetting");

    group.bench_function("builtin_only", |b| {
        b.iter(|| bench_render_xml(&xml_builtin).unwrap())
    });

    group.bench_function("one_font", |b| {
        b.iter(|| bench_render_xml_with_font(&xml_one_font, "bench-font", &font_bytes).unwrap())
    });

    group.finish();
}

criterion_group!(benches, bench_subsetting);
criterion_main!(benches);
