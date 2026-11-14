//! Instructions and estimated cycles of the stages of the fixture `svg`: three SVG pictures, a plot, a diagram and an
//! icon that paints in its text's color, drawn four times each. Its benchmarks have the id `svg`, and the names of
//! the stages of `counts.rs`, which is why they are a binary of their own.
//!
//! The `load` stage reads each SVG and writes it as PDF content, which an engine does once, when the picture is loaded.
//! The `render_doc` stage is layout and the PDF write of the parsed document, the pictures loaded in the setup. The
//! entry points that do that are new with SVG, so this binary needs the `svg` feature, which bench.yml turns on for a
//! checkout whose lib.rs has them: an earlier release has no `svg` rows.

mod callgrind;
// Shared with counts.rs, of which this binary uses the fixtures only.
#[allow(dead_code)]
mod common;
mod svg_images;

use std::hint::black_box;

use gungraun::{library_benchmark, library_benchmark_group, main};
use lpdf::{BenchDoc, BenchImages, bench_render_doc_with_loaded_images};

use callgrind::config;
use svg_images::{files, load as load_images, parsed_with_images};

// The bytes of the SVG files to the pictures an engine holds. The name is the stage's, `load`, which `allocations.rs`
// and the report know it by.
#[library_benchmark]
#[bench::svg(args = ("svg"), setup = files)]
fn load(files: Vec<(&'static str, Vec<u8>)>) -> BenchImages {
    black_box(load_images(black_box(&files)))
}

// A parsed document to the bytes of the PDF: layout and the PDF write together, with the pictures loaded.
#[library_benchmark]
#[bench::svg(args = ("svg"), setup = parsed_with_images)]
fn render_doc((doc, images): (BenchDoc, BenchImages)) -> Vec<u8> {
    black_box(
        bench_render_doc_with_loaded_images(black_box(doc), black_box(&images))
            .expect("the document renders"),
    )
}

library_benchmark_group!(name = stages, benchmarks = [load, render_doc]);

main!(config = config(), library_benchmark_groups = stages);
