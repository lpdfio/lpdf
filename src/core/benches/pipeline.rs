use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use lpdf::{bench_data_apply, bench_parse, bench_render_doc, bench_render_xml};
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

criterion_main!(core_benches, x_benches, data_benches);
