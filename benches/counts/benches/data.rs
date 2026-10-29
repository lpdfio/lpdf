//! Instructions and estimated cycles of each stage of the core pipeline for `data`, this crate's
//! `fixtures/data.xml` with `fixtures/data.json`, and of the stage between parse and layout that binds the
//! data. Its benchmarks have the id `data`, and the names of the stages of `counts.rs`, which is why they
//! are a binary of their own.
//!
//! The data is read in the setup. Every release since v0.24.0 has `bench_data_apply` and reads the
//! fixture, so each build has these rows; layout and the PDF write apart need the `split-stages`
//! feature, as in `counts.rs`.

mod callgrind;
// Shared with counts.rs, of which this binary uses the fixtures only.
#[allow(dead_code)]
mod common;
mod data_binding;

use std::hint::black_box;

use gungraun::{library_benchmark, library_benchmark_group, main};
use lpdf::{BenchDoc, bench_data_apply, bench_parse, bench_render_doc};
#[cfg(feature = "split-stages")]
use lpdf::{BenchPages, bench_layout_doc, bench_write_pdf};

use callgrind::config;
use common::fixture;
#[cfg(feature = "split-stages")]
use data_binding::bound_laid_out;
use data_binding::{bound, fixture_with_data, parsed_with_data};

// XML text to a document, with the data still to bind.
#[library_benchmark]
#[bench::data(args = ("data"), setup = fixture)]
fn parse(xml: String) -> BenchDoc {
    black_box(bench_parse(black_box(&xml)).expect("the fixture parses"))
}

// A parsed document and its data to the document with the data bound.
#[library_benchmark]
#[bench::data(args = ("data"), setup = parsed_with_data)]
fn data((doc, json): (BenchDoc, String)) -> BenchDoc {
    black_box(bench_data_apply(black_box(doc), black_box(&json)).expect("the data binds"))
}

// A bound document to laid-out pages.
#[cfg(feature = "split-stages")]
#[library_benchmark]
#[bench::data(args = ("data"), setup = bound)]
fn layout(doc: BenchDoc) -> BenchPages {
    black_box(bench_layout_doc(black_box(doc)))
}

// Laid-out pages to the bytes of the PDF.
#[cfg(feature = "split-stages")]
#[library_benchmark]
#[bench::data(args = ("data"), setup = bound_laid_out)]
fn write_pdf(pages: BenchPages) -> Vec<u8> {
    black_box(bench_write_pdf(black_box(pages)).expect("the pages are written"))
}

// A bound document to the bytes of the PDF: layout and the PDF write together.
#[library_benchmark]
#[bench::data(args = ("data"), setup = bound)]
fn render_doc(doc: BenchDoc) -> Vec<u8> {
    black_box(bench_render_doc(black_box(doc)).expect("the document renders"))
}

// XML text and its data to the bytes of the PDF.
#[library_benchmark]
#[bench::data(args = ("data"), setup = fixture_with_data)]
fn end_to_end((xml, json): (String, String)) -> Vec<u8> {
    let doc = bench_parse(black_box(&xml)).expect("the fixture parses");
    let doc = bench_data_apply(doc, black_box(&json)).expect("the data binds");
    black_box(bench_render_doc(doc).expect("the document renders"))
}

library_benchmark_group!(
    name = stages,
    benchmarks = [parse, data, render_doc, end_to_end]
);

#[cfg(feature = "split-stages")]
library_benchmark_group!(name = split_stages, benchmarks = [layout, write_pdf]);

#[cfg(not(feature = "split-stages"))]
main!(config = config(), library_benchmark_groups = stages);

#[cfg(feature = "split-stages")]
main!(
    config = config(),
    library_benchmark_groups = [stages, split_stages]
);
