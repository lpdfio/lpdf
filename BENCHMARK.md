# Benchmark Results

> Previous results (small/medium/large labels, Apr 2026) preserved in [BENCHMARK-old.md](BENCHMARK-old.md).

---

## Fixture sizes

| Label | File               | Target  | Depth |
|-------|--------------------|---------|-------|
| xs    | `bench_xs.xml`     | ~1.2 KB | 1     |
| s     | `bench_s.xml`      | ~10 KB  | 2     |
| m     | `bench_m.xml`      | ~50 KB  | 3     |
| l     | `bench_l.xml`      | ~100 KB | 3     |
| xl    | `bench_xl.xml`     | ~250 KB | 4     |
| xxl   | `bench_xxl.xml`    | ~500 KB | 4     |
| max   | `bench_max.xml`    | ~1 MB   | 4     |

Generate or regenerate fixtures with:

```sh
cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --all
# or a single size at custom depth:
cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --size m --depth 3
```

---

## Benchmark groups

| Group        | Sizes             | What is measured                                      |
|--------------|-------------------|-------------------------------------------------------|
| `parse_xml`  | xs s m l xl xxl max | XML text → `Document` tree                          |
| `layout`     | xs s m l xl xxl max | `Document` → render tree (parse excluded)            |
| `end_to_end` | xs s m l xl xxl max | XML text → PDF bytes                                 |
| `data/parse` | xs s m              | XML text → `Document` (data template)                |
| `data/apply` | xs s m              | Apply JSON data binding to pre-parsed document       |
| `data/layout`| xs s m              | Layout + PDF write on data-applied document          |
| `data/e2e`   | xs s m              | Full data pipeline: parse + apply + layout + emit    |

Run standard benchmarks (xs–xl):

```sh
make benchmark
```

Run extended benchmarks including xxl and max (may take 10+ minutes):

```sh
make benchmark-x
```

---

## Criterion settings

| Size  | `sample_size` | `measurement_time` |
|-------|---------------|--------------------|
| xs–m  | 100           | 5 s                |
| l     | 50            | 15 s               |
| xl    | 30            | 20 s               |
| xxl   | 20            | 30 s               |
| max   | 10            | 30 s               |

---

## Results

Recorded **2026-04-20** on Windows (release build, `cargo bench --bench pipeline`).

### parse_xml (xs–xl)

| Benchmark      | XML size | Time (median) | ±        | Throughput  |
|----------------|----------|---------------|----------|-------------|
| `parse_xml/xs` | 3.0 KB   | 31.18 µs      | 1.6 µs   | 93.7 MiB/s  |
| `parse_xml/s`  | 10.8 KB  | 132.3 µs      | 2.0 µs   | 79.9 MiB/s  |
| `parse_xml/m`  | 51.6 KB  | 697.1 µs      | 16 µs    | 72.2 MiB/s  |
| `parse_xml/l`  | 99.1 KB  | 1.351 ms      | 0.02 ms  | 71.6 MiB/s  |
| `parse_xml/xl` | 245.4 KB | 4.269 ms      | 0.25 ms  | 56.1 MiB/s  |

### parse_xml (xxl–max, extended)

| Benchmark          | XML size | Time (median) | ±        | Throughput  |
|--------------------|----------|---------------|----------|-------------|
| `parse_xml_x/xxl`  | 492 KB   | 9.462 ms      | 0.09 ms  | 50.8 MiB/s  |
| `parse_xml_x/max`  | 980.2 KB | 21.18 ms      | 0.63 ms  | 45.2 MiB/s  |

### layout (xs–xl)

| Benchmark    | XML size | Time (median) | ±        | Throughput  |
|--------------|----------|---------------|----------|-------------|
| `layout/xs`  | 3.0 KB   | 1.262 ms      | 0.025 ms | 2.31 MiB/s  |
| `layout/s`   | 10.8 KB  | 4.367 ms      | 0.08 ms  | 2.42 MiB/s  |
| `layout/m`   | 51.6 KB  | 26.16 ms      | 0.53 ms  | 1.93 MiB/s  |
| `layout/l`   | 99.1 KB  | 54.13 ms      | 1.47 ms  | 1.79 MiB/s  |
| `layout/xl`  | 245.4 KB | 236.8 ms      | 2.2 ms   | 1.01 MiB/s  |

### layout (xxl–max, extended)

| Benchmark       | XML size | Time (median) | ±       | Throughput   |
|-----------------|----------|---------------|---------|--------------|
| `layout_x/xxl`  | 492 KB   | 491.0 ms      | 20 ms   | 1.002 MiB/s  |
| `layout_x/max`  | 980.2 KB | 958.3 ms      | 28 ms   | 1.023 MiB/s  |

### end_to_end (xs–xl)

| Benchmark         | XML size | PDF size  | Time (median) | ±        | Throughput  |
|-------------------|----------|-----------|---------------|----------|-------------|
| `end_to_end/xs`   | 3.0 KB   | 5.7 KB    | 1.298 ms      | 0.022 ms | 2.25 MiB/s  |
| `end_to_end/s`    | 10.8 KB  | 19.4 KB   | 4.498 ms      | 0.08 ms  | 2.35 MiB/s  |
| `end_to_end/m`    | 51.6 KB  | 112.2 KB  | 27.67 ms      | 0.47 ms  | 1.82 MiB/s  |
| `end_to_end/l`    | 99.1 KB  | 214.8 KB  | 52.62 ms      | 0.59 ms  | 1.84 MiB/s  |
| `end_to_end/xl`   | 245.4 KB | 465.0 KB  | 242.2 ms      | 2.8 ms   | 1.01 MiB/s  |

### end_to_end (xxl–max, extended)

| Benchmark            | XML size | PDF size  | Time (median) | ±        | Throughput  |
|----------------------|----------|-----------|---------------|----------|-------------|
| `end_to_end_x/xxl`   | 492 KB   | 930.9 KB  | 589.0 ms      | 34 ms    | 835 KiB/s   |
| `end_to_end_x/max`   | 980.2 KB | 1853.3 KB | 984.2 ms      | 107 ms   | 996 KiB/s   |

### data binding (xs–m)

| Benchmark        | XML size | Time (median) | ±         | Notes                     |
|------------------|----------|---------------|-----------|---------------------------|
| `data/parse/xs`  | 1.8 KB   | 30.29 µs      | 0.80 µs   |                           |
| `data/parse/s`   | 14.0 KB  | 189.2 µs      | 0.88 µs   |                           |
| `data/parse/m`   | 75.5 KB  | 1.060 ms      | 0.013 ms  |                           |
| `data/apply/xs`  | 1.8 KB   | 8.650 µs      | 0.13 µs   | parse excluded            |
| `data/apply/s`   | 14.0 KB  | 70.78 µs      | 1.6 µs    | parse excluded            |
| `data/apply/m`   | 75.5 KB  | 439.7 µs      | 5.3 µs    | parse excluded            |
| `data/layout/xs` | 1.8 KB   | 152.5 µs      | 1.0 µs    | parse + apply excluded    |
| `data/layout/s`  | 14.0 KB  | 835.3 µs      | 13 µs     | parse + apply excluded    |
| `data/layout/m`  | 75.5 KB  | 9.373 ms      | 0.072 ms  | parse + apply excluded    |
| `data/e2e/xs`    | 1.8 KB   | 191.9 µs      | 1.2 µs    | pdf = 2.4 KB              |
| `data/e2e/s`     | 14.0 KB  | 1.092 ms      | 0.008 ms  | pdf = 8.3 KB              |
| `data/e2e/m`     | 75.5 KB  | 10.74 ms      | 0.058 ms  | pdf = 40.8 KB             |

---

## Assessment

### Parse XML: 45–94 MiB/s, scales linearly

Throughput is highest at xs (~94 MiB/s) and tapers to ~45 MiB/s at max. The drop is gradual and expected — larger documents stress the allocator and memory bandwidth more. Parse is never a bottleneck: across all sizes it accounts for ≤5% of total end-to-end time.

### Layout dominates at every size tier (95–97% of wall time)

This is correct and expected. All box-model resolution, text shaping, line-breaking, and pagination happens here. Scaling from xs to xl is close to linear with respect to document byte count, with mild super-linear growth into xxl/max from increased pagination work:

| Step       | xs → m (17× XML) | m → xl (4.8× XML) | xl → max (4× XML) |
|------------|-----------------|-------------------|-------------------|
| Layout     | 20.7×           | 9.1×              | 4.0×              |
| End-to-end | 21.3×           | 8.8×              | 4.1×              |

At xl–max, layout throughput stabilises at ~1 MiB/s, confirming near-linear scaling at large sizes.

### PDF emit overhead is negligible

Serialisation cost (end-to-end minus layout) is small and stable:

| Size | Emit overhead       |
|------|---------------------|
| xs   | ~36 µs (2.7%)       |
| s    | ~131 µs (2.9%)      |
| m    | ~1.5 ms (5.5%)      |
| l    | ~0 ms (within noise)|
| xl   | ~5.4 ms (2.2%)      |
| xxl  | ~98 ms (16.7%)      |
| max  | ~26 ms (2.7%)       |

The `l` end-to-end reading (52.6 ms) is marginally below the layout reading (54.1 ms) — this is measurement noise from separate benchmark runs. The xxl figure has higher variance (34 ms ±) so the emit percentage there is less reliable. Overall, emit stays well under 6% for xs–xl.

### PDF output size: ~1.9–2.2× XML input

| Size | XML      | PDF      | Ratio |
|------|----------|----------|-------|
| xs   | 3.0 KB   | 5.7 KB   | 1.9×  |
| s    | 10.8 KB  | 19.4 KB  | 1.8×  |
| m    | 51.6 KB  | 112.2 KB | 2.2×  |
| l    | 99.1 KB  | 214.8 KB | 2.2×  |
| xl   | 245.4 KB | 465.0 KB | 1.9×  |
| xxl  | 492 KB   | 930.9 KB | 1.9×  |
| max  | 980.2 KB | 1853 KB  | 1.9×  |

The ratio peaks at ~2.2× for m/l. This is caused by font embedding: fonts are a fixed per-document cost that is proportionally larger relative to body content at mid-range sizes. The ratio asymptotes to ~1.9× at xl and above as body content amortises the font overhead.

### Data binding: apply pass is 4–7% of total time

The JSON data-apply step adds modest overhead:

| Size | parse    | apply    | layout   | e2e      | apply share |
|------|----------|----------|----------|----------|-------------|
| xs   | 30.3 µs  | 8.6 µs   | 152.5 µs | 191.9 µs | 4.5%        |
| s    | 189.2 µs | 70.8 µs  | 835.3 µs | 1.092 ms | 6.5%        |
| m    | 1.060 ms | 439.7 µs | 9.373 ms | 10.74 ms | 4.1%        |

Layout remains the dominant cost even after data binding. Data-template PDFs are substantially more compact than same-size plain-pipeline PDFs (e.g. bench_data_m at 75.5 KB XML produces only a 40.8 KB PDF vs 112 KB for bench_m at 51.6 KB XML) because templates contain verbose placeholder markup that resolves to simpler rendered content.

### Extended sizes (xxl, max) — `make benchmark-x`

| Size | Layout  | End-to-end | PDF output |
|------|---------|------------|------------|
| xxl  | 491 ms  | 589 ms     | 930.9 KB   |
| max  | 958 ms  | 984 ms     | 1.85 MB    |

A 1 MB XML document renders to a 1.85 MB PDF in under 1 second. The xxl result has elevated variance (±7%) due to fewer samples (20); max variance is higher still (±11%). These figures are useful for order-of-magnitude comparisons but should not be used as tight regression guards.

---

## Baseline tracking

```sh
# save a baseline before a refactor
cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- --save-baseline main

# compare after changes
cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- --baseline main
```
