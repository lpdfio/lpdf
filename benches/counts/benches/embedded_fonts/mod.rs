//! The setup of the benchmarks of `bench_fonts`, which `fonts.rs` and `allocations.rs` share: the fixture,
//! parsed or laid out, with its embedded fonts loaded.

use lpdf::{BenchDoc, BenchFonts, BenchPages, bench_font_srcs, bench_fonts, bench_layout_doc_with_fonts};

use crate::common::{fixture, fixture_folder, parsed};

/// The embedded fonts of the fixture `name`, each read from the file its `src` names, relative to the fixture, as an
/// SDK reads it, and loaded as an engine loads them. A file that isn't there stops the run.
pub fn fonts(name: &str) -> BenchFonts {
    let folder = fixture_folder(name);
    let srcs = bench_font_srcs(&fixture(name)).unwrap_or_else(|e| panic!("{name} doesn't parse: {e}"));
    let files: Vec<(String, Vec<u8>)> = srcs
        .into_iter()
        .map(|(font, src)| {
            let path = folder.join(&src);
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("the font {font} of {name}, {}, can't be read: {e}", path.display()));
            (font, bytes)
        })
        .collect();
    let loaded: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(font, bytes)| (font.as_str(), bytes.as_slice()))
        .collect();
    bench_fonts(&loaded)
}

pub fn fixture_with_fonts(name: &str) -> (String, BenchFonts) {
    (fixture(name), fonts(name))
}

pub fn parsed_with_fonts(name: &str) -> (BenchDoc, BenchFonts) {
    (parsed(name), fonts(name))
}

pub fn laid_out_with_fonts(name: &str) -> (BenchPages, BenchFonts) {
    let fonts = fonts(name);
    (bench_layout_doc_with_fonts(parsed(name), &fonts), fonts)
}
