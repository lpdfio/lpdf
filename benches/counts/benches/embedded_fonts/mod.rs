//! The setup of the benchmarks of `bench_fonts`, which `fonts.rs` and `allocations.rs` share: the fixture,
//! parsed or laid out, with its embedded fonts loaded.

use std::path::PathBuf;

use lpdf::{BenchDoc, BenchFonts, BenchPages, bench_fonts, bench_layout_doc_with_fonts};

use crate::common::{fixture, parsed};

/// The fonts of `bench_fonts.xml`, by the keys it names them with, in the examples of the checkout.
const FONTS: &[(&str, &str)] = &[
    ("body", "6-book/assets/fonts/CrimsonText-Regular.ttf"),
    ("heading", "6-book/assets/fonts/CrimsonText-SemiBold.ttf"),
    ("sans", "5-brochure/assets/fonts/Montserrat-Regular.ttf"),
];

/// The embedded fonts of `bench_fonts.xml`, loaded as an engine loads them.
pub fn fonts() -> BenchFonts {
    let examples = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let files: Vec<(&str, Vec<u8>)> = FONTS
        .iter()
        .map(|&(name, file)| {
            let path = examples.join(file);
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("the font {} can't be read: {e}", path.display()));
            (name, bytes)
        })
        .collect();
    let loaded: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(name, bytes)| (*name, bytes.as_slice()))
        .collect();
    bench_fonts(&loaded)
}

pub fn fixture_with_fonts(name: &str) -> (String, BenchFonts) {
    (fixture(name), fonts())
}

pub fn parsed_with_fonts(name: &str) -> (BenchDoc, BenchFonts) {
    (parsed(name), fonts())
}

pub fn laid_out_with_fonts(name: &str) -> (BenchPages, BenchFonts) {
    let fonts = fonts();
    (bench_layout_doc_with_fonts(parsed(name), &fonts), fonts)
}
