use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use lpdf::{bench_parse, bench_render_doc, bench_render_xml};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures")
}

fn read_fixture(name: &str) -> String {
    let path = fixtures_dir().join(format!("{name}.xml"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()))
}

fn bench_parse_xml(c: &mut Criterion) {
    let small  = read_fixture("example1");
    let medium = read_fixture("example8");
    let large  = read_fixture("bench_large");

    let mut group = c.benchmark_group("parse_xml");
    group.bench_with_input(BenchmarkId::new("small",  "example1"),    &small,  |b, xml| b.iter(|| bench_parse(xml).unwrap()));
    group.bench_with_input(BenchmarkId::new("medium", "example8"),    &medium, |b, xml| b.iter(|| bench_parse(xml).unwrap()));
    group.bench_with_input(BenchmarkId::new("large",  "bench_large"), &large,  |b, xml| b.iter(|| bench_parse(xml).unwrap()));
    group.finish();
}

fn bench_layout(c: &mut Criterion) {
    let small  = read_fixture("example1");
    let medium = read_fixture("example8");
    let large  = read_fixture("bench_large");

    let doc_small  = bench_parse(&small).unwrap();
    let doc_medium = bench_parse(&medium).unwrap();
    let doc_large  = bench_parse(&large).unwrap();

    let mut group = c.benchmark_group("layout");
    group.bench_function("small",  |b| b.iter_batched(
        || bench_parse(&small).unwrap(),
        |doc| bench_render_doc(doc).unwrap(),
        criterion::BatchSize::SmallInput,
    ));
    group.bench_function("medium", |b| b.iter_batched(
        || bench_parse(&medium).unwrap(),
        |doc| bench_render_doc(doc).unwrap(),
        criterion::BatchSize::SmallInput,
    ));
    group.bench_function("large",  |b| b.iter_batched(
        || bench_parse(&large).unwrap(),
        |doc| bench_render_doc(doc).unwrap(),
        criterion::BatchSize::SmallInput,
    ));
    // Suppress unused variable warnings for pre-parsed docs (they were for
    // documentation purposes to show the separation of stages).
    let _ = doc_small;
    let _ = doc_medium;
    let _ = doc_large;
    group.finish();
}

fn bench_end_to_end(c: &mut Criterion) {
    let small  = read_fixture("example1");
    let medium = read_fixture("example8");
    let large  = read_fixture("bench_large");

    let mut group = c.benchmark_group("end_to_end");
    group.bench_with_input(BenchmarkId::new("small",  "example1"),    &small,  |b, xml| b.iter(|| bench_render_xml(xml).unwrap()));
    group.bench_with_input(BenchmarkId::new("medium", "example8"),    &medium, |b, xml| b.iter(|| bench_render_xml(xml).unwrap()));
    group.bench_with_input(BenchmarkId::new("large",  "bench_large"), &large,  |b, xml| b.iter(|| bench_render_xml(xml).unwrap()));
    group.finish();
}

criterion_group!(benches, bench_parse_xml, bench_layout, bench_end_to_end);
criterion_main!(benches);
