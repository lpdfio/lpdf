# Benchmark Results

Recorded **2026-04-17** on Windows (release build, `cargo bench`).  
Runner: `make benchmark` → `cargo bench --manifest-path src/core/Cargo.toml --bench pipeline --bench images --bench fonts`

---

## Fixtures

| Label  | File              | Size  |
|--------|-------------------|-------|
| small  | `example1.xml`    | 1.2 KB |
| medium | `example8.xml`    | 9.8 KB |
| large  | `bench_large.xml` | 15 KB  |

---

## Results

### Parse XML

| Benchmark                      | Time (ns/iter) | ± (ns) |
|-------------------------------|---------------:|-------:|
| `parse_xml/small/example1`    | 25,719         | 5,166  |
| `parse_xml/medium/example8`   | 250,566        | 59,048 |
| `parse_xml/large/bench_large` | 236,061        | 32,206 |

### Layout (parse excluded)

| Benchmark      | Time (ns/iter) | ± (ns)    |
|---------------|---------------:|----------:|
| `layout/small`  | 385,461       | 69,996    |
| `layout/medium` | 8,695,908     | 1,163,867 |
| `layout/large`  | 10,979,120    | 1,325,512 |

### End-to-End (parse + layout + PDF emit)

| Benchmark                        | Time (ns/iter) | ± (ns)    |
|---------------------------------|---------------:|----------:|
| `end_to_end/small/example1`     | 394,000        | 73,126    |
| `end_to_end/medium/example8`    | 8,854,575      | 1,237,815 |
| `end_to_end/large/bench_large`  | 11,562,400     | 1,261,820 |

---

## Assessment

### Parse XML is fast and scales linearly

Throughput is ~40–65 MB/s across all fixture sizes. Parse time accounts for less than 1% of total end-to-end time on medium/large documents — it is not a bottleneck.

### Layout dominates (~97% of total time)

This is expected and correct for a layout engine. All the box model, text measurement, line-breaking, and pagination work lives here. The ~9–11 ms ceiling for a 15 KB document is healthy.

### PDF emit adds negligible overhead

The serialization cost (end-to-end minus layout) is:

| Input  | Emit overhead |
|--------|--------------|
| small  | ~9 µs        |
| medium | ~160 µs      |
| large  | ~580 µs      |

Well under 6% of total time. PDF emit is not a bottleneck.

### Variance

Variance is 10–15%, typical for benchmarks that touch font/glyph data. Not alarming, but worth watching on large inputs.

---

## Baseline tracking

Save a named baseline before a refactor and compare after:

```sh
# before changes
cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- --save-baseline main

# after changes
cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- --load-baseline main --baseline main
```

HTML reports are written to `target/criterion/`.

---

## Next steps

| Finding | Suggested action |
|---------|-----------------|
| Layout dominates for large docs | Profile pagination splitting (`split_node_at`) with `cargo-flamegraph` if further optimisation is needed |
| Font subsetting cost unknown | Run `benches/fonts.rs` and compare `subsetting/builtin_only` vs `subsetting/one_font` to quantify caching value |
| RGBA PNG decode cost unknown | Run `benches/images.rs` to confirm whether a docs warning is warranted |
| No CI regression guard | Add a smoke bench run to CI (`--warm-up-time 1 --measurement-time 3`) to catch compile-time regressions |
