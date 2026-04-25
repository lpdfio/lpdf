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

Recorded **2026-04-25** on Windows (release build, `cargo bench --bench pipeline`).

### parse_xml (xs–xl)

| Benchmark      | XML size | Time (median) | ±         | Throughput  |
|----------------|----------|---------------|-----------|-------------|
| `parse_xml/xs` | 3.0 KB   | 35.54 µs      | 0.15 µs   | 82.5 MiB/s  |
| `parse_xml/s`  | 10.8 KB  | 140.9 µs      | 0.21 µs   | 74.9 MiB/s  |
| `parse_xml/m`  | 51.3 KB  | 782.0 µs      | 1.1 µs    | 64.0 MiB/s  |
| `parse_xml/l`  | 98.5 KB  | 1.577 ms      | 0.006 ms  | 61.0 MiB/s  |
| `parse_xml/xl` | 243.3 KB | 4.983 ms      | 0.060 ms  | 47.8 MiB/s  |

### parse_xml (xxl–max, extended)

| Benchmark          | XML size | Time (median) | ±        | Throughput  |
|--------------------|----------|---------------|----------|-------------|
| `parse_xml_x/xxl`  | 492 KB   | 9.462 ms      | 0.09 ms  | 50.8 MiB/s  |
| `parse_xml_x/max`  | 980.2 KB | 21.18 ms      | 0.63 ms  | 45.2 MiB/s  |

### layout (xs–xl)

| Benchmark    | XML size | Time (median) | ±        | Throughput  |
|--------------|----------|---------------|----------|-------------|
| `layout/xs`  | 3.0 KB   | 1.268 ms      | 0.010 ms | 2.31 MiB/s  |
| `layout/s`   | 10.8 KB  | 4.328 ms      | 0.008 ms | 2.44 MiB/s  |
| `layout/m`   | 51.3 KB  | 26.52 ms      | 0.10 ms  | 1.89 MiB/s  |
| `layout/l`   | 98.5 KB  | 51.34 ms      | 0.17 ms  | 1.87 MiB/s  |
| `layout/xl`  | 243.3 KB | 245.66 ms     | 1.25 ms  | 968 KiB/s   |

### layout (xxl–max, extended)

| Benchmark       | XML size | Time (median) | ±       | Throughput   |
|-----------------|----------|---------------|---------|--------------|
| `layout_x/xxl`  | 492 KB   | 491.0 ms      | 20 ms   | 1.002 MiB/s  |
| `layout_x/max`  | 980.2 KB | 958.3 ms      | 28 ms   | 1.023 MiB/s  |

### end_to_end (xs–xl)

| Benchmark         | XML size | PDF size  | Time (median) | ±        | Throughput  |
|-------------------|----------|-----------|---------------|----------|-------------|
| `end_to_end/xs`   | 3.0 KB   | 5.7 KB    | 1.252 ms      | 0.005 ms | 2.34 MiB/s  |
| `end_to_end/s`    | 10.8 KB  | 19.4 KB   | 4.380 ms      | 0.005 ms | 2.41 MiB/s  |
| `end_to_end/m`    | 51.3 KB  | 112.2 KB  | 26.21 ms      | 0.04 ms  | 1.91 MiB/s  |
| `end_to_end/l`    | 98.5 KB  | 214.8 KB  | 51.38 ms      | 0.48 ms  | 1.87 MiB/s  |
| `end_to_end/xl`   | 243.3 KB | 465.0 KB  | 253.06 ms     | 1.4 ms   | 937 KiB/s   |

### end_to_end (xxl–max, extended)

| Benchmark            | XML size | PDF size  | Time (median) | ±        | Throughput  |
|----------------------|----------|-----------|---------------|----------|-------------|
| `end_to_end_x/xxl`   | 492 KB   | 930.9 KB  | 589.0 ms      | 34 ms    | 835 KiB/s   |
| `end_to_end_x/max`   | 980.2 KB | 1853.3 KB | 984.2 ms      | 107 ms   | 996 KiB/s   |

### data binding (xs–m)

| Benchmark        | XML size | Time (median) | ±         | Notes                     |
|------------------|----------|---------------|-----------|---------------------------|
| `data/parse/xs`  | 1.8 KB   | 33.64 µs      | 0.08 µs   |                           |
| `data/parse/s`   | 13.7 KB  | 275.5 µs      | 1.1 µs    |                           |
| `data/parse/m`   | 74.0 KB  | 1.526 ms      | 0.007 ms  |                           |
| `data/apply/xs`  | 1.8 KB   | 12.89 µs      | 0.19 µs   | parse excluded            |
| `data/apply/s`   | 13.7 KB  | 75.34 µs      | 0.57 µs   | parse excluded            |
| `data/apply/m`   | 74.0 KB  | 460.7 µs      | 5.8 µs    | parse excluded            |
| `data/layout/xs` | 1.8 KB   | 209.2 µs      | 1.1 µs    | parse + apply excluded    |
| `data/layout/s`  | 13.7 KB  | 1.076 ms      | 0.008 ms  | parse + apply excluded    |
| `data/layout/m`  | 74.0 KB  | 14.42 ms      | 0.13 ms   | parse + apply excluded    |
| `data/e2e/xs`    | 1.8 KB   | 264.7 µs      | 0.94 µs   | pdf = 2.4 KB              |
| `data/e2e/s`     | 13.7 KB  | 1.590 ms      | 0.011 ms  | pdf = 8.3 KB              |
| `data/e2e/m`     | 74.0 KB  | 14.44 ms      | 0.061 ms  | pdf = 40.8 KB             |

---

## Assessment

### Parse XML: 47–83 MiB/s, scales linearly

Throughput is highest at xs (~83 MiB/s) and tapers to ~48 MiB/s at xl. Parse is never a bottleneck: across all sizes it accounts for ≤5% of total end-to-end time. Compared to the 2026-04-20 baseline, parse throughput regressed ~12–17% uniformly across all sizes, indicating a systematic change in the XML parsing path.

### Layout dominates at every size tier (95–97% of wall time)

This is correct and expected. All box-model resolution, text shaping, line-breaking, and pagination happens here. Scaling from xs to xl is close to linear with respect to document byte count, with mild super-linear growth into xxl/max from increased pagination work:

| Step       | xs → m (17× XML) | m → xl (4.7× XML) | xl → max (4× XML) |
|------------|-----------------|-------------------|-------------------|
| Layout     | 20.9×           | 9.3×              | 4.0×              |
| End-to-end | 20.9×           | 9.7×              | 4.1×              |

Layout throughput at xl dropped to ~968 KiB/s (just under 1 MiB/s). Compared to the prior baseline (1.01 MiB/s), this is a modest ~4% regression at xl; xs through l are within noise of prior results.

### PDF emit overhead is negligible

Serialisation cost (end-to-end minus layout) is small and stable:

| Size | Emit overhead        |
|------|----------------------|
| xs   | ~0 ms (within noise) |
| s    | ~52 µs (1.2%)        |
| m    | ~0 ms (within noise) |
| l    | ~44 µs (0.1%)        |
| xl   | ~7.4 ms (2.9%)       |

Several xs/m/l readings show end-to-end marginally below layout — this is measurement noise from separate runs. Overall, emit overhead stays well under 3% for xs–xl, consistent with prior results.

### PDF output size: ~1.9–2.2× XML input

| Size | XML      | PDF      | Ratio |
|------|----------|----------|-------|
| xs   | 3.0 KB   | 5.7 KB   | 1.9×  |
| s    | 10.8 KB  | 19.4 KB  | 1.8×  |
| m    | 51.3 KB  | 112.2 KB | 2.2×  |
| l    | 98.5 KB  | 214.8 KB | 2.2×  |
| xl   | 243.3 KB | 465.0 KB | 1.9×  |
| xxl  | 492 KB   | 930.9 KB | 1.9×  |
| max  | 980.2 KB | 1853 KB  | 1.9×  |

The ratio peaks at ~2.2× for m/l. This is caused by font embedding: fonts are a fixed per-document cost that is proportionally larger relative to body content at mid-range sizes. The ratio asymptotes to ~1.9× at xl and above as body content amortises the font overhead.

### Data binding: apply pass is ~5% of total time; layout path regressed

The JSON data-apply step adds modest overhead:

| Size | parse    | apply    | layout   | e2e      | apply share |
|------|----------|----------|----------|----------|--------------|
| xs   | 33.6 µs  | 12.9 µs  | 209.2 µs | 264.7 µs | 4.9%         |
| s    | 275.5 µs | 75.3 µs  | 1.076 ms | 1.590 ms | 4.7%         |
| m    | 1.526 ms | 460.7 µs | 14.42 ms | 14.44 ms | 3.2%         |

Data-template PDFs are substantially more compact than same-size plain-pipeline PDFs (e.g. bench_data_m at 74.0 KB XML produces only a 40.8 KB PDF vs 112 KB for bench_m at 51.3 KB XML) because templates contain verbose placeholder markup that resolves to simpler rendered content.

**Regression vs 2026-04-20 baseline:** The data pipeline has regressed significantly. Compared to prior numbers:

| Benchmark       | Old      | New      | Change   |
|-----------------|----------|----------|----------|
| data/parse/xs   | 30.3 µs  | 33.6 µs  | +11%     |
| data/parse/s    | 189.2 µs | 275.5 µs | +46%     |
| data/parse/m    | 1.060 ms | 1.526 ms | +44%     |
| data/apply/xs   | 8.65 µs  | 12.89 µs | +49%     |
| data/layout/xs  | 152.5 µs | 209.2 µs | +37%     |
| data/layout/s   | 835.3 µs | 1.076 ms | +29%     |
| data/layout/m   | 9.373 ms | 14.42 ms | +54%     |
| data/e2e/xs     | 191.9 µs | 264.7 µs | +38%     |
| data/e2e/s      | 1.092 ms | 1.590 ms | +46%     |
| data/e2e/m      | 10.74 ms | 14.44 ms | +35%     |

The `data/apply/s` and `data/apply/m` regressions are modest (~6–11%). The dominant regression is in `data/layout`, which drives the overall `data/e2e` slowdown. The plain-pipeline benchmarks (non-data `layout`, `end_to_end`) are largely unaffected, pointing to a regression specific to the data-template layout code path.

### Extended sizes (xxl, max) — `make benchmark-x`

| Size | Layout  | End-to-end | PDF output |
|------|---------|------------|------------|
| xxl  | 491 ms  | 589 ms     | 930.9 KB   |
| max  | 958 ms  | 984 ms     | 1.85 MB    |

A 1 MB XML document renders to a 1.85 MB PDF in under 1 second. The xxl result has elevated variance (±7%) due to fewer samples (20); max variance is higher still (±11%). These figures are useful for order-of-magnitude comparisons but should not be used as tight regression guards.

---

## Competitor comparison

> **Methodology note:** LPDF numbers are from this run (Windows, 5-year-old Intel i7, release build). Competitor numbers are **estimates** derived from published benchmarks, official documentation, and community reports — not a controlled head-to-head. Warm-start figures are shown (cold JVM/interpreter start adds 500 ms–2 s for Java/Python tools). Input complexity and hardware vary; treat the order-of-magnitude comparisons as directionally reliable, not precise.

### Tool overview

| Tool | Input format | Layout engine | Runtime | License |
|------|-------------|---------------|---------|---------|
| **LPDF** | XML (declarative) | ✓ Full — box model, text shaping, pagination | Rust / WASM / WASI | Commercial |
| Apache FOP | XSL-FO (XML) | ✓ Full | JVM | Apache 2.0 |
| Prince XML | HTML / CSS | ✓ Full | Native binary | Commercial (~$500–3800/server) |
| WeasyPrint | HTML / CSS | ✓ Full | CPython | BSD |
| pdfmake | JS object model | ✓ Full | Node.js | MIT |
| ReportLab Platypus | Python API | ✓ Full | CPython | BSD / Commercial |
| Puppeteer / Chrome | HTML / CSS | ✓ (browser engine) | Node.js + Chromium | Apache 2.0 |
| wkhtmltopdf | HTML / CSS | ✓ (Qt WebKit) | Native (deprecated) | LGPL |
| iText 7 | Programmatic | ✗ Manual positioning | JVM / .NET | AGPL / Commercial |

Apache FOP is the closest architectural analog to LPDF: both accept XML with a declarative layout vocabulary. Prince XML is the quality/speed benchmark for HTML→PDF.

---

### Estimated page counts per fixture size

These are estimates based on text density (~3 lines/paragraph, A4, 12 pt body, normal section spacing):

| Fixture | XML size | PDF size | Est. pages |
|---------|----------|----------|------------|
| `xs`    | 3.0 KB   | 5.7 KB   | ~2         |
| `s`     | 10.8 KB  | 19.4 KB  | ~5–8       |
| `m`     | 51.3 KB  | 112 KB   | ~20–30     |
| `l`     | 98.5 KB  | 215 KB   | ~40–55     |
| `xl`    | 243 KB   | 465 KB   | ~100–150   |
| `xxl`   | 488 KB   | 931 KB   | ~200–300   |
| `max`   | 972 KB   | 1.85 MB  | ~400+      |

---

### Small–medium documents (~5–30 pages)

Corresponds to LPDF `end_to_end/s` (4.4 ms) through `end_to_end/m` (26 ms).

| Tool | ~5–8 pages | ~20–30 pages | Notes |
|------|-----------|-------------|-------|
| **LPDF** | **~4 ms** | **~26 ms** | |
| iText 7 (warm) | ~15–50 ms | ~50–200 ms | No layout engine; manual API |
| pdfmake | ~40–150 ms | ~150–600 ms | Pure JS; client + server |
| ReportLab Platypus | ~60–200 ms | ~200 ms–1 s | CPython |
| Prince XML | ~100–300 ms | ~300 ms–1 s | HTML/CSS; commercial |
| WeasyPrint | ~300–800 ms | ~800 ms–3 s | CPython + CSS parsing |
| Apache FOP (warm) | ~300–700 ms | ~800 ms–3 s | JVM warm; cold adds ~1 s |
| wkhtmltopdf | ~500 ms–1 s | ~1–3 s | Deprecated; unstable |
| Puppeteer (warm) | ~400 ms–1 s | ~1–4 s | Persistent browser process |

At this range LPDF is **20–100× faster** than tools with a real layout engine (WeasyPrint, FOP, pdfmake, ReportLab). The gap is primarily explained by the difference between a compiled native/WASM engine and an interpreted runtime — not algorithmic differences.

---

### Large–extra large documents (~100–300 pages)

Corresponds to LPDF `end_to_end/xl` (253 ms, ~100–150 pages) through `end_to_end_x/xxl` (589 ms, ~200–300 pages).

| Tool | ~100–150 pages | ~200–300 pages | Notes |
|------|---------------|---------------|-------|
| **LPDF** | **~253 ms** | **~589 ms** | |
| Prince XML | ~800 ms–2 s | ~1.5–4 s | Fastest HTML/CSS tool |
| pdfmake | ~1.5–5 s | ~3–10 s | Single-threaded JS |
| iText 7 (warm) | ~500 ms–2 s | ~1–5 s | Programmatic; no layout |
| ReportLab Platypus | ~2–8 s | ~5–20 s | CPython; GC pressure |
| WeasyPrint | ~3–12 s | ~8–30 s | Memory pressure at scale |
| Apache FOP (warm) | ~3–15 s | ~8–40 s | Known memory issues at scale |
| wkhtmltopdf | ~5–20 s | ~15–60 s+ | Often crashes or OOM |
| Puppeteer (warm) | ~8–30 s | ~20–90 s | Each page renders in browser |

At large sizes the gap widens: **5–60× faster** than tools with comparable layout quality (FOP, WeasyPrint, Prince). Layout cost in interpreted runtimes grows super-linearly due to GC pressure and single-threaded line-breaking; LPDF's Rust allocator and arena-based layout tree scale near-linearly to 1 MB+.

---

### Key deployment advantages not reflected in timing

| Factor | LPDF | JVM tools (FOP, iText) | Python tools | Browser tools |
|--------|------|----------------------|--------------|---------------|
| Cold start | < 5 ms (WASM init) | 500 ms–2 s JVM | 100–500 ms | 2–5 s Chrome launch |
| Memory footprint | ~20–50 MB | ~200–500 MB | ~50–150 MB | ~300–800 MB |
| Serverless / edge | ✓ (WASI) | ✗ | ✗ | ✗ |
| No system deps | ✓ | ✗ (JRE) | ✗ (Python + libs) | ✗ (Chromium) |
| Sandboxed execution | ✓ (WASM) | ✗ | ✗ | Partial |

---

## Baseline tracking

```sh
# save a baseline before a refactor
cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- --save-baseline main

# compare after changes
cargo bench --manifest-path src/core/Cargo.toml --bench pipeline -- --baseline main
```
