//! Instructions and estimated cycles of each stage of the core pipeline for `bench_fonts`: `bench_m` in
//! embedded fonts, two of them below U+024F and one with Cyrillic above it. Its benchmarks have the id
//! `fonts`, and the names of the stages of `counts.rs`, which is why they are a binary of their own.
//!
//! The fonts are loaded in the setup, as an engine loads them once before it renders, and layout
//! measures their text with their own advances, as `LpdfEngine::render_pdf` does. The entry points that
//! do that are new in 0.30, so this binary needs the `embedded-fonts` feature, which bench.yml turns on
//! for a checkout whose lib.rs has them: an earlier release has no `fonts` rows.

mod callgrind;
// Shared with counts.rs, of which this binary uses the fixtures only.
#[allow(dead_code)]
mod common;
mod embedded_fonts;

use std::hint::black_box;

use gungraun::{library_benchmark, library_benchmark_group, main};
use lpdf::{
    BenchDoc, BenchFonts, BenchPages, bench_layout_doc_with_fonts, bench_parse,
    bench_render_doc_with_fonts, bench_render_xml_with_fonts, bench_write_pdf_with_fonts,
};

use callgrind::config;
use common::fixture;
use embedded_fonts::{fixture_with_fonts, laid_out_with_fonts, parsed_with_fonts};

// XML text to a document. Parse reads no font bytes, so the fonts don't take part.
#[library_benchmark]
#[bench::fonts(args = ("bench_fonts"), setup = fixture)]
fn parse(xml: String) -> BenchDoc {
    black_box(bench_parse(black_box(&xml)).expect("the fixture parses"))
}

// A parsed document to laid-out pages.
#[library_benchmark]
#[bench::fonts(args = ("bench_fonts"), setup = parsed_with_fonts)]
fn layout((doc, fonts): (BenchDoc, BenchFonts)) -> BenchPages {
    black_box(bench_layout_doc_with_fonts(
        black_box(doc),
        black_box(&fonts),
    ))
}

// Laid-out pages to the bytes of the PDF, with the fonts subset and embedded.
#[library_benchmark]
#[bench::fonts(args = ("bench_fonts"), setup = laid_out_with_fonts)]
fn write_pdf((pages, fonts): (BenchPages, BenchFonts)) -> Vec<u8> {
    black_box(
        bench_write_pdf_with_fonts(black_box(pages), black_box(&fonts))
            .expect("the pages are written"),
    )
}

// A parsed document to the bytes of the PDF: layout and the PDF write together.
#[library_benchmark]
#[bench::fonts(args = ("bench_fonts"), setup = parsed_with_fonts)]
fn render_doc((doc, fonts): (BenchDoc, BenchFonts)) -> Vec<u8> {
    black_box(
        bench_render_doc_with_fonts(black_box(doc), black_box(&fonts))
            .expect("the document renders"),
    )
}

// XML text to the bytes of the PDF.
#[library_benchmark]
#[bench::fonts(args = ("bench_fonts"), setup = fixture_with_fonts)]
fn end_to_end((xml, fonts): (String, BenchFonts)) -> Vec<u8> {
    black_box(
        bench_render_xml_with_fonts(black_box(&xml), black_box(&fonts))
            .expect("the fixture renders"),
    )
}

library_benchmark_group!(
    name = stages,
    benchmarks = [parse, layout, write_pdf, render_doc, end_to_end]
);

main!(config = config(), library_benchmark_groups = stages);
