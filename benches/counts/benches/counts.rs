//! Instructions, estimated cycles and allocations of each stage of the core pipeline, for each size of
//! `bench_xs` to `bench_xl`: the `CORE_SIZES` of `src/core/benches/pipeline.rs`.
//!
//! Callgrind counts the instructions and simulates the caches for the estimated cycles, and DHAT counts
//! the allocations, each in a run of its own. Both count the benchmark function only: reading the
//! fixture, and parsing or laying it out for a later stage, is setup, which neither counts.
//!
//! bench.yml copies this crate into a checkout of each build it compares, so it may only call what every
//! release since v0.24.0 has: `bench_parse`, `bench_render_doc` and `bench_render_xml`. The stages that
//! split layout from the PDF write need the `split-stages` feature, which bench.yml turns on for a
//! checkout whose lib.rs has them.

use std::hint::black_box;
use std::path::PathBuf;

use gungraun::{Dhat, LibraryBenchmarkConfig, library_benchmark, library_benchmark_group, main};
use lpdf::{BenchDoc, bench_parse, bench_render_doc, bench_render_xml};
#[cfg(feature = "split-stages")]
use lpdf::{BenchPages, bench_layout_doc, bench_write_pdf};

/// std seeds every `HashMap` with bytes from `getrandom`, so each run would hash in another order and
/// count up to 1% more or fewer instructions for the same build. std reaches `getrandom` through a weak
/// symbol, so this definition takes its place in this binary: the same bytes in every run, and the same
/// numbers. It changes nothing in the code measured, only the seed it is given.
///
/// # Safety
///
/// `buffer` must be valid for writes of `length` bytes, as `getrandom(2)` requires of its callers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrandom(buffer: *mut u8, length: usize, _flags: u32) -> isize {
    // SAFETY: the caller passes a buffer of `length` writable bytes, as for the system call.
    unsafe { std::ptr::write_bytes(buffer, 0x5a, length) };
    length as isize
}

/// The text of a fixture of the checkout this crate was copied into.
fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures")
        .join(format!("{name}.xml"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the fixture {} can't be read: {e}", path.display()))
}

fn parsed(name: &str) -> BenchDoc {
    bench_parse(&fixture(name)).unwrap_or_else(|e| panic!("{name} doesn't parse: {e}"))
}

#[cfg(feature = "split-stages")]
fn laid_out(name: &str) -> BenchPages {
    bench_layout_doc(parsed(name))
}

// XML text to a document.
#[library_benchmark]
#[bench::xs(args = ("bench_xs"), setup = fixture)]
#[bench::s(args = ("bench_s"), setup = fixture)]
#[bench::m(args = ("bench_m"), setup = fixture)]
#[bench::l(args = ("bench_l"), setup = fixture)]
#[bench::xl(args = ("bench_xl"), setup = fixture)]
fn parse(xml: String) -> BenchDoc {
    black_box(bench_parse(black_box(&xml)).expect("the fixture parses"))
}

// A parsed document to laid-out pages.
#[cfg(feature = "split-stages")]
#[library_benchmark]
#[bench::xs(args = ("bench_xs"), setup = parsed)]
#[bench::s(args = ("bench_s"), setup = parsed)]
#[bench::m(args = ("bench_m"), setup = parsed)]
#[bench::l(args = ("bench_l"), setup = parsed)]
#[bench::xl(args = ("bench_xl"), setup = parsed)]
fn layout(doc: BenchDoc) -> BenchPages {
    black_box(bench_layout_doc(black_box(doc)))
}

// Laid-out pages to the bytes of the PDF.
#[cfg(feature = "split-stages")]
#[library_benchmark]
#[bench::xs(args = ("bench_xs"), setup = laid_out)]
#[bench::s(args = ("bench_s"), setup = laid_out)]
#[bench::m(args = ("bench_m"), setup = laid_out)]
#[bench::l(args = ("bench_l"), setup = laid_out)]
#[bench::xl(args = ("bench_xl"), setup = laid_out)]
fn write_pdf(pages: BenchPages) -> Vec<u8> {
    black_box(bench_write_pdf(black_box(pages)).expect("the pages are written"))
}

// A parsed document to the bytes of the PDF: layout and the PDF write together.
#[library_benchmark]
#[bench::xs(args = ("bench_xs"), setup = parsed)]
#[bench::s(args = ("bench_s"), setup = parsed)]
#[bench::m(args = ("bench_m"), setup = parsed)]
#[bench::l(args = ("bench_l"), setup = parsed)]
#[bench::xl(args = ("bench_xl"), setup = parsed)]
fn render_doc(doc: BenchDoc) -> Vec<u8> {
    black_box(bench_render_doc(black_box(doc)).expect("the document renders"))
}

// XML text to the bytes of the PDF.
#[library_benchmark]
#[bench::xs(args = ("bench_xs"), setup = fixture)]
#[bench::s(args = ("bench_s"), setup = fixture)]
#[bench::m(args = ("bench_m"), setup = fixture)]
#[bench::l(args = ("bench_l"), setup = fixture)]
#[bench::xl(args = ("bench_xl"), setup = fixture)]
fn end_to_end(xml: String) -> Vec<u8> {
    black_box(bench_render_xml(black_box(&xml)).expect("the fixture renders"))
}

/// glibc's malloc, which Callgrind counts, grows its heap in steps that depend on the address the heap
/// starts at, and so on the size of the binary: a large block is carved from the heap or mapped on its
/// own depending on whether the heap has room left, and frees then merge differently. Any change to the
/// code moved small renders by up to 1% that way. One large heap from the start, never given back,
/// leaves the allocator with the same work for the same calls. DHAT replaces malloc, so its counts
/// don't depend on this.
const GLIBC_TUNABLES: &str =
    "glibc.malloc.top_pad=268435456:glibc.malloc.trim_threshold=4294967296";

fn config() -> LibraryBenchmarkConfig {
    let mut config = LibraryBenchmarkConfig::default();
    config
        .tool(Dhat::default())
        .env("GLIBC_TUNABLES", GLIBC_TUNABLES);
    config
}

library_benchmark_group!(name = stages, benchmarks = [parse, render_doc, end_to_end]);

#[cfg(feature = "split-stages")]
library_benchmark_group!(name = split_stages, benchmarks = [layout, write_pdf]);

#[cfg(not(feature = "split-stages"))]
main!(config = config(), library_benchmark_groups = stages);

#[cfg(feature = "split-stages")]
main!(
    config = config(),
    library_benchmark_groups = [stages, split_stages]
);
