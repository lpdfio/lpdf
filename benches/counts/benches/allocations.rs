//! Allocations, bytes allocated and peak memory of each stage of the core pipeline, for each fixture whose
//! instructions `counts.rs`, `fonts.rs` and `data.rs` count.
//!
//! A global allocator that wraps the system's counts every allocation a stage makes, however deep in the
//! stack, and the bytes the stage holds. It needs no Valgrind and runs on any machine. It is a binary of
//! its own, since in the Callgrind binary its own instructions would be counted.
//!
//! Like `counts.rs`, it may only call what every release since v0.24.0 has, and measures layout and the
//! PDF write apart with the `split-stages` feature. Each benchmark runs in a process of its own, as under
//! Gungraun, so that what one leaves behind, such as a buffer made on first use, doesn't change the count
//! of the next. Reading the fixture, and parsing or laying it out for a later stage, is setup, which
//! isn't counted. With the `markdown`, `code` and `embedded-fonts` features, it counts those fixtures too,
//! as `counts.rs` and `fonts.rs` do, the fonts of `fonts` loaded in the setup. `data` has a stage of its
//! own, `data`, which binds its data, as in `data.rs`.
//!
//! `cargo bench --bench allocations` prints a JSON line for each benchmark, which
//! `scripts/bench-counts-report.mjs` reads:
//!
//! ```text
//! {"stage":"parse","fixture":"xs","allocations":228,"bytes_allocated":72031,"peak_bytes":33012}
//! ```

mod common;
mod data_binding;
#[cfg(feature = "embedded-fonts")]
mod embedded_fonts;

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::io::Write;
use std::process::{Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};

use lpdf::{bench_data_apply, bench_parse, bench_render_doc, bench_render_xml};
#[cfg(feature = "split-stages")]
use lpdf::{bench_layout_doc, bench_write_pdf};
#[cfg(feature = "embedded-fonts")]
use lpdf::{
    bench_layout_doc_with_fonts, bench_render_doc_with_fonts, bench_render_xml_with_fonts,
    bench_write_pdf_with_fonts,
};

#[cfg(feature = "split-stages")]
use common::laid_out;
use common::{fixture, parsed};
#[cfg(feature = "split-stages")]
use data_binding::bound_laid_out;
use data_binding::{bound, fixture_with_data, parsed_with_data};
#[cfg(feature = "embedded-fonts")]
use embedded_fonts::{fixture_with_fonts, laid_out_with_fonts, parsed_with_fonts};

/// The stages, by the names of their benchmark functions in `counts.rs`.
#[cfg(not(feature = "split-stages"))]
const STAGES: &[&str] = &["parse", "render_doc", "end_to_end"];
#[cfg(feature = "split-stages")]
const STAGES: &[&str] = &["parse", "layout", "write_pdf", "render_doc", "end_to_end"];

/// The sizes of the checkout's `bench_xs.xml` to `bench_xl.xml`, by the ids of their benchmarks.
const SIZES: &[&str] = &["xs", "s", "m", "l", "xl"];

/// The fixtures this build has, by the ids of their benchmarks: the sizes, then this crate's own, of
/// which `justified` is `fixtures/justified.xml`.
fn fixtures() -> Vec<&'static str> {
    let mut fixtures = SIZES.to_vec();
    fixtures.push("justified");
    #[cfg(feature = "markdown")]
    fixtures.push("markdown");
    #[cfg(feature = "code")]
    fixtures.push("code");
    #[cfg(feature = "embedded-fonts")]
    fixtures.push("fonts");
    fixtures.push("data");
    fixtures
}

/// The stages of a fixture: those of `STAGES`, and for `data` the binding of its data after parse.
fn stages(fixture_id: &str) -> Vec<&'static str> {
    let mut stages = STAGES.to_vec();
    if fixture_id == "data" {
        stages.insert(1, "data");
    }
    stages
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static BYTES_ALLOCATED: AtomicU64 = AtomicU64::new(0);
/// Bytes allocated less bytes freed since the stage began: below 0 when the stage has freed more of its
/// input than it has allocated.
static LIVE: AtomicI64 = AtomicI64::new(0);
static PEAK: AtomicI64 = AtomicI64::new(0);

/// The system's allocator, counting what passes through it. A realloc counts as an allocation of its new
/// size and a free of the old one, as DHAT counts it.
struct Counting;

// SAFETY: every call goes to the system allocator with the caller's arguments; the counting around it
// touches only atomics.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller keeps the contract of `GlobalAlloc::alloc`, which is the system's.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            allocated(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as for `alloc`.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            allocated(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by this allocator, so by the system's, with `layout`.
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size() as i64, Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: as for `dealloc`, and the caller keeps the contract of `GlobalAlloc::realloc`.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            LIVE.fetch_sub(layout.size() as i64, Relaxed);
            allocated(new_size);
        }
        new_ptr
    }
}

fn allocated(size: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
    BYTES_ALLOCATED.fetch_add(size as u64, Relaxed);
    let live = LIVE.fetch_add(size as i64, Relaxed) + size as i64;
    PEAK.fetch_max(live, Relaxed);
}

/// What one stage allocated. The peak is the most the heap grew over where it stood when the stage
/// began.
struct Counts {
    allocations: u64,
    bytes_allocated: u64,
    peak_bytes: i64,
}

/// Runs `stage` and counts what it allocates. What it returns is freed after the count.
fn counted<T>(stage: impl FnOnce() -> T) -> Counts {
    ALLOCATIONS.store(0, Relaxed);
    BYTES_ALLOCATED.store(0, Relaxed);
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    let output = black_box(stage());
    let counts = Counts {
        allocations: ALLOCATIONS.load(Relaxed),
        bytes_allocated: BYTES_ALLOCATED.load(Relaxed),
        peak_bytes: PEAK.load(Relaxed),
    };
    drop(output);
    counts
}

/// The counts of one stage of one fixture, or `None` for a stage this build doesn't have.
fn measure(stage: &str, fixture_id: &str) -> Option<Counts> {
    #[cfg(feature = "embedded-fonts")]
    if fixture_id == "fonts" {
        return measure_with_fonts(stage);
    }
    if fixture_id == "data" {
        return measure_with_data(stage);
    }
    let name = if SIZES.contains(&fixture_id) {
        format!("bench_{fixture_id}")
    } else {
        fixture_id.to_string()
    };
    let counts = match stage {
        "parse" => {
            let xml = fixture(&name);
            counted(|| bench_parse(black_box(&xml)).expect("the fixture parses"))
        }
        #[cfg(feature = "split-stages")]
        "layout" => {
            let doc = parsed(&name);
            counted(|| bench_layout_doc(black_box(doc)))
        }
        #[cfg(feature = "split-stages")]
        "write_pdf" => {
            let pages = laid_out(&name);
            counted(|| bench_write_pdf(black_box(pages)).expect("the pages are written"))
        }
        "render_doc" => {
            let doc = parsed(&name);
            counted(|| bench_render_doc(black_box(doc)).expect("the document renders"))
        }
        "end_to_end" => {
            let xml = fixture(&name);
            counted(|| bench_render_xml(black_box(&xml)).expect("the fixture renders"))
        }
        _ => return None,
    };
    Some(counts)
}

/// The counts of one stage of `bench_fonts`, its fonts loaded in the setup, or `None` for a stage that isn't one.
#[cfg(feature = "embedded-fonts")]
fn measure_with_fonts(stage: &str) -> Option<Counts> {
    const NAME: &str = "bench_fonts";
    let counts = match stage {
        "parse" => {
            let xml = fixture(NAME);
            counted(|| bench_parse(black_box(&xml)).expect("the fixture parses"))
        }
        "layout" => {
            let (doc, fonts) = parsed_with_fonts(NAME);
            counted(|| bench_layout_doc_with_fonts(black_box(doc), &fonts))
        }
        "write_pdf" => {
            let (pages, fonts) = laid_out_with_fonts(NAME);
            counted(|| {
                bench_write_pdf_with_fonts(black_box(pages), &fonts).expect("the pages are written")
            })
        }
        "render_doc" => {
            let (doc, fonts) = parsed_with_fonts(NAME);
            counted(|| {
                bench_render_doc_with_fonts(black_box(doc), &fonts).expect("the document renders")
            })
        }
        "end_to_end" => {
            let (xml, fonts) = fixture_with_fonts(NAME);
            counted(|| {
                bench_render_xml_with_fonts(black_box(&xml), &fonts).expect("the fixture renders")
            })
        }
        _ => return None,
    };
    Some(counts)
}

/// The counts of one stage of `data`, with its data read in the setup, or `None` for a stage that isn't one.
fn measure_with_data(stage: &str) -> Option<Counts> {
    const NAME: &str = "data";
    let counts = match stage {
        "parse" => {
            let xml = fixture(NAME);
            counted(|| bench_parse(black_box(&xml)).expect("the fixture parses"))
        }
        "data" => {
            let (doc, json) = parsed_with_data(NAME);
            counted(|| bench_data_apply(black_box(doc), &json).expect("the data binds"))
        }
        #[cfg(feature = "split-stages")]
        "layout" => {
            let doc = bound(NAME);
            counted(|| bench_layout_doc(black_box(doc)))
        }
        #[cfg(feature = "split-stages")]
        "write_pdf" => {
            let pages = bound_laid_out(NAME);
            counted(|| bench_write_pdf(black_box(pages)).expect("the pages are written"))
        }
        "render_doc" => {
            let doc = bound(NAME);
            counted(|| bench_render_doc(black_box(doc)).expect("the document renders"))
        }
        "end_to_end" => {
            let (xml, json) = fixture_with_data(NAME);
            counted(|| {
                let doc = bench_parse(black_box(&xml)).expect("the fixture parses");
                let doc = bench_data_apply(doc, &json).expect("the data binds");
                bench_render_doc(doc).expect("the document renders")
            })
        }
        _ => return None,
    };
    Some(counts)
}

/// Runs each benchmark in a process of its own and prints their lines.
fn measure_all() -> ExitCode {
    let this = std::env::current_exe().expect("the path of this binary");
    let mut stdout = std::io::stdout().lock();
    for fixture_id in fixtures() {
        for stage in stages(fixture_id) {
            let run = Command::new(&this)
                .args([stage, fixture_id])
                .stderr(Stdio::inherit())
                .output()
                .expect("the benchmark's process starts");
            if !run.status.success() {
                eprintln!("allocations: {stage} {fixture_id} failed ({})", run.status);
                return ExitCode::FAILURE;
            }
            stdout
                .write_all(&run.stdout)
                .expect("stdout takes the line");
        }
    }
    ExitCode::SUCCESS
}

/// With no arguments, every benchmark; with a stage and a fixture, the process that measures one.
/// `cargo bench` adds `--bench`, which is left out.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|arg| arg != "--bench")
        .collect();
    match args.as_slice() {
        [] => measure_all(),
        [stage, fixture_id] => match measure(stage, fixture_id) {
            Some(counts) => {
                println!(
                    r#"{{"stage":"{stage}","fixture":"{fixture_id}","allocations":{},"bytes_allocated":{},"peak_bytes":{}}}"#,
                    counts.allocations, counts.bytes_allocated, counts.peak_bytes,
                );
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("allocations: this build has no stage '{stage}'");
                ExitCode::from(2)
            }
        },
        _ => {
            eprintln!("usage: allocations [<stage> <fixture>]");
            ExitCode::from(2)
        }
    }
}
