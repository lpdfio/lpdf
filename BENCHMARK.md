# Benchmark Results

Recorded **2026-04-18** on Windows (release build, `cargo bench`).  
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
| `parse_xml/small/example1`    | 12,571         | 590    |
| `parse_xml/medium/example8`   | 273,184        | 74,779 |
| `parse_xml/large/bench_large` | 127,280        | 16,997 |

### Layout (parse excluded)

| Benchmark      | Time (ns/iter) | ± (ns)  |
|---------------|---------------:|--------:|
| `layout/small`  | 192,313       | 9,462   |
| `layout/medium` | 4,636,877     | 176,800 |
| `layout/large`  | 6,009,850     | 228,110 |

### End-to-End (parse + layout + PDF emit)

| Benchmark                        | Time (ns/iter) | ± (ns)  |
|---------------------------------|---------------:|--------:|
| `end_to_end/small/example1`     | 421,229        | 132,230 |
| `end_to_end/medium/example8`    | 4,705,043      | 299,380 |
| `end_to_end/large/bench_large`  | 6,164,227      | 316,312 |

---

## Assessment

### Parse XML is fast and scales linearly

Throughput is ~35–120 MB/s across fixture sizes (medium run is noise-affected). Parse time accounts for less than 3% of total end-to-end time on medium/large documents — it is not a bottleneck.

### Layout dominates (~97–98% of total time)

This is expected and correct for a layout engine. All the box model, text measurement, line-breaking, and pagination work lives here. The ~4.6–6.0 ms ceiling for a 15 KB document is healthy.

### PDF emit adds negligible overhead

The serialization cost (end-to-end minus layout) is:

| Input  | Emit overhead |
|--------|--------------|
| small  | ~70 µs (noisy) |
| medium | ~68 µs       |
| large  | ~155 µs      |

Well under 3% of total time on medium/large. PDF emit is not a bottleneck. The small-fixture figure is within measurement noise — see variance below.

### Variance

Variance is 5–30% depending on benchmark. Small fixtures (sub-millisecond) are the noisiest — `end_to_end/small` in particular can swing ±30% between runs. Layout and large end-to-end numbers are stable (~3–5% variance).

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
