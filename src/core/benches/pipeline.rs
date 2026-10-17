use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use lpdf::{bench_data_apply, bench_parse, bench_render_doc, bench_render_doc_with_images, bench_render_xml};
use std::path::PathBuf;
use std::time::Duration;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures")
}

fn read_fixture(name: &str) -> String {
    let path = fixtures_dir().join(format!("{name}.xml"));
    std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "fixture not found: {name}.xml — run: \
             cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --all"
        )
    })
}

fn read_json_fixture(name: &str) -> String {
    let path = fixtures_dir().join(format!("{name}.json"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("JSON fixture not found: {name}.json"))
}

// ── Size tiers ─────────────────────────────────────────────────────────────────
//
// CORE_SIZES: xs–xl, run by `make benchmark`.
// BENCH_X_SIZES: xxl + max, run by `make benchmark-x` (opt-in).
// DATA_SIZES: xs/s/m data-binding fixtures.

const CORE_SIZES: &[(&str, &str)] = &[
    ("xs",  "bench_xs"),
    ("s",   "bench_s"),
    ("m",   "bench_m"),
    ("l",   "bench_l"),
    ("xl",  "bench_xl"),
];

const BENCH_X_SIZES: &[(&str, &str)] = &[
    ("xxl", "bench_xxl"),
    ("max", "bench_max"),
];

const DATA_SIZES: &[(&str, &str, &str)] = &[
    ("xs", "data-invoice",  "data-invoice"),
    ("s",  "bench_data_s",  "bench_data_s"),
    ("m",  "bench_data_m",  "bench_data_m"),
];

// PAGE_LABELS: the sizes of the layer, image and barcode fixtures, `bench_{family}_{label}`.

const PAGE_LABELS: &[&str] = &["s", "m", "l"];

// ── Criterion config per size tier ────────────────────────────────────────────

fn apply_size_config(
    group: &mut criterion::BenchmarkGroup<criterion::measurement::WallTime>,
    label: &str,
) {
    match label {
        "l" => {
            group.sample_size(50);
            group.measurement_time(Duration::from_secs(15));
        }
        "xl" => {
            group.sample_size(30);
            group.measurement_time(Duration::from_secs(20));
        }
        "xxl" => {
            group.sample_size(20);
            group.measurement_time(Duration::from_secs(30));
        }
        "max" => {
            group.sample_size(10);
            group.measurement_time(Duration::from_secs(30));
        }
        _ => {} // xs/s/m: criterion defaults (100 samples, 5 s)
    }
}

// ── Core pipeline benchmarks (xs–xl) ─────────────────────────────────────────

fn bench_parse_xml(c: &mut Criterion) {
    let fixtures: Vec<_> = CORE_SIZES
        .iter()
        .map(|(label, name)| (*label, read_fixture(name)))
        .collect();

    let mut group = c.benchmark_group("parse_xml");
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), xml, |b, xml| {
            b.iter(|| bench_parse(xml).unwrap())
        });
    }
    group.finish();
}

fn bench_layout(c: &mut Criterion) {
    let fixtures: Vec<_> = CORE_SIZES
        .iter()
        .map(|(label, name)| (*label, read_fixture(name)))
        .collect();

    let mut group = c.benchmark_group("layout");
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.iter_batched(
                || bench_parse(xml).unwrap(),
                |doc| bench_render_doc(doc).unwrap(),
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_end_to_end(c: &mut Criterion) {
    let fixtures: Vec<_> = CORE_SIZES
        .iter()
        .map(|(label, name)| (*label, read_fixture(name)))
        .collect();

    let mut group = c.benchmark_group("end_to_end");
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        let pdf_bytes = bench_render_xml(xml).unwrap().len();
        eprintln!("  [end_to_end/{label}] xml={:.1}KB → pdf={:.1}KB",
            xml.len() as f64 / 1024.0,
            pdf_bytes as f64 / 1024.0);
        group.bench_with_input(BenchmarkId::from_parameter(label), xml, |b, xml| {
            b.iter(|| bench_render_xml(xml).unwrap())
        });
    }
    group.finish();
}

// ── Extended benchmarks (xxl + max) — run via `make benchmark-x` ─────────────

fn bench_parse_xml_x(c: &mut Criterion) {
    let fixtures: Vec<_> = BENCH_X_SIZES
        .iter()
        .map(|(label, name)| (*label, read_fixture(name)))
        .collect();

    let mut group = c.benchmark_group("parse_xml_x");
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), xml, |b, xml| {
            b.iter(|| bench_parse(xml).unwrap())
        });
    }
    group.finish();
}

fn bench_layout_x(c: &mut Criterion) {
    let fixtures: Vec<_> = BENCH_X_SIZES
        .iter()
        .map(|(label, name)| (*label, read_fixture(name)))
        .collect();

    let mut group = c.benchmark_group("layout_x");
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.iter_batched(
                || bench_parse(xml).unwrap(),
                |doc| bench_render_doc(doc).unwrap(),
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_end_to_end_x(c: &mut Criterion) {
    let fixtures: Vec<_> = BENCH_X_SIZES
        .iter()
        .map(|(label, name)| (*label, read_fixture(name)))
        .collect();

    let mut group = c.benchmark_group("end_to_end_x");
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        let pdf_bytes = bench_render_xml(xml).unwrap().len();
        eprintln!("  [end_to_end_x/{label}] xml={:.1}KB → pdf={:.1}KB",
            xml.len() as f64 / 1024.0,
            pdf_bytes as f64 / 1024.0);
        group.bench_with_input(BenchmarkId::from_parameter(label), xml, |b, xml| {
            b.iter(|| bench_render_xml(xml).unwrap())
        });
    }
    group.finish();
}

// ── Data-binding benchmarks — split breakdown (xs–m) ─────────────────────────
//
//   data/parse   — XML text → Document          (no apply, no layout)
//   data/apply   — data binding pass only       (parse excluded via iter_batched)
//   data/layout  — layout + emit only           (parse + apply excluded)
//   data/e2e     — full pipeline for comparison

fn bench_data_parse(c: &mut Criterion) {
    let fixtures: Vec<_> = DATA_SIZES
        .iter()
        .map(|(label, xml_name, _json)| (*label, read_fixture(xml_name)))
        .collect();

    let mut group = c.benchmark_group("data/parse");
    for (label, xml) in &fixtures {
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), xml, |b, xml| {
            b.iter(|| bench_parse(xml).unwrap())
        });
    }
    group.finish();
}

fn bench_data_apply_group(c: &mut Criterion) {
    let fixtures: Vec<_> = DATA_SIZES
        .iter()
        .map(|(label, xml_name, json_name)| {
            (*label, read_fixture(xml_name), read_json_fixture(json_name))
        })
        .collect();

    let mut group = c.benchmark_group("data/apply");
    for (label, xml, json) in &fixtures {
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.iter_batched(
                || bench_parse(xml).unwrap(),
                |doc| bench_data_apply(doc, json).unwrap(),
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_data_layout(c: &mut Criterion) {
    let fixtures: Vec<_> = DATA_SIZES
        .iter()
        .map(|(label, xml_name, json_name)| {
            (*label, read_fixture(xml_name), read_json_fixture(json_name))
        })
        .collect();

    let mut group = c.benchmark_group("data/layout");
    for (label, xml, json) in &fixtures {
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.iter_batched(
                || {
                    let doc = bench_parse(xml).unwrap();
                    bench_data_apply(doc, json).unwrap()
                },
                |doc| bench_render_doc(doc).unwrap(),
                criterion::BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_data_e2e(c: &mut Criterion) {
    let fixtures: Vec<_> = DATA_SIZES
        .iter()
        .map(|(label, xml_name, json_name)| {
            (*label, read_fixture(xml_name), read_json_fixture(json_name))
        })
        .collect();

    let mut group = c.benchmark_group("data/e2e");
    for (label, xml, json) in &fixtures {
        group.throughput(Throughput::Bytes(xml.len() as u64));
        let doc = bench_parse(xml).unwrap();
        let doc = bench_data_apply(doc, json).unwrap();
        let pdf_bytes = bench_render_doc(doc).unwrap().len();
        eprintln!("  [data/e2e/{label}] xml={:.1}KB → pdf={:.1}KB",
            xml.len() as f64 / 1024.0,
            pdf_bytes as f64 / 1024.0);
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.iter(|| {
                let doc = bench_parse(xml).unwrap();
                let doc = bench_data_apply(doc, json).unwrap();
                bench_render_doc(doc).unwrap()
            })
        });
    }
    group.finish();
}

// ── Layer, image and barcode benchmarks (s–l) ─────────────────────────────────
//
//   layers/*  — shapes, text-at and a layer faded as a group
//   images/*  — pictures in the flow and img-at, fitted to their proportions
//   qr/*      — QR codes in the flow and barcode-at
//   linear/*  — Code 128 and EAN-13 in the flow and barcode-at
//
// Each is one thing, since a page that holds a QR code spends nearly all of its time encoding it. Two groups each:
//
//   */parse   — XML text → Document
//   */e2e     — full pipeline: parse, size the pictures, layout, PDF write
//
// The layout and the PDF write are `*/e2e` less `*/parse`. The pictures are loaded and sized as the engine
// sizes them, so the fitting of images is in the pipeline.

/// The name the image fixtures give their picture.
const PHOTO: &str = "photo";

/// A photo the repo already holds, as the image bench uses.
fn photo() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/6-book/assets/images/chapter-05-caterpillar.jpg");
    std::fs::read(&path).unwrap_or_else(|e| panic!("the photo of the image fixtures, {}, can't be read: {e}", path.display()))
}

fn contains(pdf: &[u8], needle: &[u8]) -> bool {
    pdf.windows(needle.len()).any(|window| window == needle)
}

/// The two groups of one family of fixtures. `marker` is what the PDF of every fixture has when the engine
/// drew what the family is for, where the PDF has a mark of it: a fixture it reads as something else would
/// still render a page, and the bench would time nothing. A barcode is drawn as plain rectangles and has no
/// mark, so the tests of `gen_fixtures` count the barcodes the engine lays out instead.
fn bench_pages(
    c:      &mut Criterion,
    family: &str,
    images: &[(&str, &[u8])],
    marker: Option<&[u8]>,
) {
    let fixtures: Vec<_> = PAGE_LABELS
        .iter()
        .map(|label| (*label, read_fixture(&format!("bench_{family}_{label}"))))
        .collect();

    for (label, xml) in &fixtures {
        let pdf = bench_render_doc_with_images(bench_parse(xml).unwrap(), images).unwrap();
        if let Some(marker) = marker {
            assert!(
                contains(&pdf, marker),
                "{family}/{label}: the PDF has no {}",
                String::from_utf8_lossy(marker)
            );
        }
        eprintln!("  [{family}/e2e/{label}] xml={:.1}KB → pdf={:.1}KB",
            xml.len() as f64 / 1024.0,
            pdf.len() as f64 / 1024.0);
    }

    let mut group = c.benchmark_group(format!("{family}/parse"));
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(label), xml, |b, xml| {
            b.iter(|| bench_parse(xml).unwrap())
        });
    }
    group.finish();

    let mut group = c.benchmark_group(format!("{family}/e2e"));
    for (label, xml) in &fixtures {
        apply_size_config(&mut group, label);
        group.throughput(Throughput::Bytes(xml.len() as u64));
        group.bench_function(BenchmarkId::from_parameter(label), |b| {
            b.iter(|| bench_render_doc_with_images(bench_parse(xml).unwrap(), images).unwrap())
        });
    }
    group.finish();
}

fn bench_layers(c: &mut Criterion) {
    // The faded layer is written as a transparency group.
    bench_pages(c, "layers", &[], Some(b"/Group"));
}

fn bench_images(c: &mut Criterion) {
    let photo = photo();
    bench_pages(c, "images", &[(PHOTO, &photo)], Some(b"/Subtype /Image"));
}

fn bench_qr(c: &mut Criterion) {
    bench_pages(c, "qr", &[], None);
}

fn bench_linear(c: &mut Criterion) {
    bench_pages(c, "linear", &[], None);
}

// ── Registration ──────────────────────────────────────────────────────────────

criterion_group!(
    core_benches,
    bench_parse_xml,
    bench_layout,
    bench_end_to_end
);

criterion_group!(
    x_benches,
    bench_parse_xml_x,
    bench_layout_x,
    bench_end_to_end_x
);

criterion_group!(
    data_benches,
    bench_data_parse,
    bench_data_apply_group,
    bench_data_layout,
    bench_data_e2e
);

criterion_group!(
    page_benches,
    bench_layers,
    bench_images,
    bench_qr,
    bench_linear
);

criterion_main!(core_benches, x_benches, data_benches, page_benches);
