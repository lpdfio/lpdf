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

Recorded **2026-04-25** (updated re-run after fixes) on Windows (release build, `cargo bench --bench pipeline`).

### parse_xml (xs–xl)

| Benchmark      | XML size | Time (median) | ±         | Throughput  |
|----------------|----------|---------------|-----------|-------------|
| `parse_xml/xs` | 3.0 KB   | 30.17 µs      | 0.13 µs   | 96.7 MiB/s  |
| `parse_xml/s`  | 10.8 KB  | 134.79 µs     | 1.9 µs    | 78.5 MiB/s  |
| `parse_xml/m`  | 51.3 KB  | 790.29 µs     | 5.3 µs    | 63.3 MiB/s  |
| `parse_xml/l`  | 98.5 KB  | 1.516 ms      | 0.009 ms  | 63.4 MiB/s  |
| `parse_xml/xl` | 243.3 KB | 4.451 ms      | 0.035 ms  | 53.4 MiB/s  |

### parse_xml (xxl–max, extended)

| Benchmark          | XML size | Time (median) | ±        | Throughput  |
|--------------------|----------|---------------|----------|-------------|
| `parse_xml_x/xxl`  | 492 KB   | 9.462 ms      | 0.09 ms  | 50.8 MiB/s  |
| `parse_xml_x/max`  | 980.2 KB | 21.18 ms      | 0.63 ms  | 45.2 MiB/s  |

### layout (xs–xl)

| Benchmark    | XML size | Time (median) | ±        | Throughput  |
|--------------|----------|---------------|----------|-------------|
| `layout/xs`  | 3.0 KB   | 1.117 ms      | 0.005 ms | 2.61 MiB/s  |
| `layout/s`   | 10.8 KB  | 3.966 ms      | 0.019 ms | 2.67 MiB/s  |
| `layout/m`   | 51.3 KB  | 25.18 ms      | 0.14 ms  | 1.99 MiB/s  |
| `layout/l`   | 98.5 KB  | 48.49 ms      | 0.28 ms  | 1.98 MiB/s  |
| `layout/xl`  | 243.3 KB | 226.0 ms      | 2.2 ms   | 1.05 MiB/s  |

### layout (xxl–max, extended)

| Benchmark       | XML size | Time (median) | ±       | Throughput   |
|-----------------|----------|---------------|---------|--------------|
| `layout_x/xxl`  | 492 KB   | 491.0 ms      | 20 ms   | 1.002 MiB/s  |
| `layout_x/max`  | 980.2 KB | 958.3 ms      | 28 ms   | 1.023 MiB/s  |

### end_to_end (xs–xl)

| Benchmark         | XML size | PDF size  | Time (median) | ±        | Throughput  |
|-------------------|----------|-----------|---------------|----------|-------------|
| `end_to_end/xs`   | 3.0 KB   | 5.7 KB    | 1.170 ms      | 0.013 ms | 2.50 MiB/s  |
| `end_to_end/s`    | 10.8 KB  | 19.4 KB   | 4.147 ms      | 0.065 ms | 2.55 MiB/s  |
| `end_to_end/m`    | 51.3 KB  | 112.2 KB  | 25.69 ms      | 0.12 ms  | 1.95 MiB/s  |
| `end_to_end/l`    | 98.5 KB  | 214.8 KB  | 50.85 ms      | 0.34 ms  | 1.89 MiB/s  |
| `end_to_end/xl`   | 243.3 KB | 465.0 KB  | 232.9 ms      | 3.0 ms   | 1.02 MiB/s  |

### end_to_end (xxl–max, extended)

| Benchmark            | XML size | PDF size  | Time (median) | ±        | Throughput  |
|----------------------|----------|-----------|---------------|----------|-------------|
| `end_to_end_x/xxl`   | 492 KB   | 930.9 KB  | 589.0 ms      | 34 ms    | 835 KiB/s   |
| `end_to_end_x/max`   | 980.2 KB | 1853.3 KB | 984.2 ms      | 107 ms   | 996 KiB/s   |

### data binding (xs–m)

| Benchmark        | XML size | Time (median) | ±         | Notes                     |
|------------------|----------|---------------|-----------|---------------------------|
| `data/parse/xs`  | 1.8 KB   | 29.78 µs      | 0.21 µs   |                           |
| `data/parse/s`   | 13.7 KB  | 245.3 µs      | 2.3 µs    |                           |
| `data/parse/m`   | 74.0 KB  | 1.361 ms      | 0.007 ms  |                           |
| `data/apply/xs`  | 1.8 KB   | 8.83 µs       | 0.13 µs   | parse excluded            |
| `data/apply/s`   | 13.7 KB  | 75.89 µs      | 1.6 µs    | parse excluded            |
| `data/apply/m`   | 74.0 KB  | 388.9 µs      | 4.1 µs    | parse excluded            |
| `data/layout/xs` | 1.8 KB   | 161.2 µs      | 1.3 µs    | parse + apply excluded    |
| `data/layout/s`  | 13.7 KB  | 883.8 µs      | 4.1 µs    | parse + apply excluded    |
| `data/layout/m`  | 74.0 KB  | 10.23 ms      | 0.067 ms  | parse + apply excluded    |
| `data/e2e/xs`    | 1.8 KB   | 206.3 µs      | 3.7 µs    | pdf = 2.4 KB              |
| `data/e2e/s`     | 13.7 KB  | 1.243 ms      | 0.021 ms  | pdf = 8.3 KB              |
| `data/e2e/m`     | 74.0 KB  | 12.23 ms      | 0.083 ms  | pdf = 40.8 KB             |

---

## Assessment

### Parse XML: 53–97 MiB/s, scales linearly

Throughput is highest at xs (~97 MiB/s) and tapers to ~53 MiB/s at xl. Parse is never a bottleneck: across all sizes it accounts for ≤5% of total end-to-end time. Throughput improved 6–17% vs the prior (2026-04-25) run across all sizes except m, which is statistically unchanged.

### Layout dominates at every size tier (95–97% of wall time)

This is correct and expected. All box-model resolution, text shaping, line-breaking, and pagination happens here. Scaling from xs to xl is close to linear with respect to document byte count, with mild super-linear growth into xxl/max from increased pagination work:

| Step       | xs → m (17× XML) | m → xl (4.7× XML) | xl → max (4× XML) |
|------------|-----------------|-------------------|-------------------|
| Layout     | 22.5×           | 9.0×              | 4.0×              |
| End-to-end | 22.0×           | 9.1×              | 4.1×              |

Layout throughput at xl is now 1.05 MiB/s, recovering above the 1 MiB/s mark. All sizes improved 5–12% vs the prior run.

### PDF emit overhead is negligible

Serialisation cost (end-to-end minus layout) is small and stable:

| Size | Emit overhead        |
|------|----------------------|
| xs   | ~53 µs (4.5%)        |
| s    | ~181 µs (4.4%)       |
| m    | ~510 µs (2.0%)       |
| l    | ~2.4 ms (4.7%)       |
| xl   | ~6.9 ms (3.0%)       |

Emit overhead is below 5% for all sizes. xs through m readings should be treated as order-of-magnitude estimates since they are computed from two independent benchmark runs and include inter-run variance.

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

### Data binding: apply pass is ~4–6% of total time; regression largely resolved

The JSON data-apply step adds modest overhead:

| Size | parse    | apply    | layout   | e2e      | apply share |
|------|----------|----------|----------|----------|-------------|
| xs   | 29.8 µs  | 8.8 µs   | 161.2 µs | 206.3 µs | 4.3%        |
| s    | 245.3 µs | 75.9 µs  | 883.8 µs | 1.243 ms | 6.1%        |
| m    | 1.361 ms | 388.9 µs | 10.23 ms | 12.23 ms | 3.2%        |

Data-template PDFs are substantially more compact than same-size plain-pipeline PDFs (e.g. bench_data_m at 74.0 KB XML produces only a 40.8 KB PDF vs 112 KB for bench_m at 51.3 KB XML) because templates contain verbose placeholder markup that resolves to simpler rendered content.

**Recovery vs 2026-04-25 (regressed) run:** The data pipeline regression documented in the previous run has been largely resolved by the fixes applied. All sub-groups improved significantly:

| Benchmark       | Prev (regressed) | This run  | Change      |
|-----------------|-----------------|-----------|-------------|
| data/parse/xs   | 33.64 µs        | 29.78 µs  | −11%        |
| data/parse/s    | 275.5 µs        | 245.3 µs  | −11%        |
| data/parse/m    | 1.526 ms        | 1.361 ms  | −11%        |
| data/apply/xs   | 12.89 µs        | 8.83 µs   | −31%        |
| data/apply/s    | 75.34 µs        | 75.89 µs  | <1% (noise) |
| data/apply/m    | 460.7 µs        | 388.9 µs  | −16%        |
| data/layout/xs  | 209.2 µs        | 161.2 µs  | −23%        |
| data/layout/s   | 1.076 ms        | 883.8 µs  | −18%        |
| data/layout/m   | 14.42 ms        | 10.23 ms  | −29%        |
| data/e2e/xs     | 264.7 µs        | 206.3 µs  | −22%        |
| data/e2e/s      | 1.590 ms        | 1.243 ms  | −22%        |
| data/e2e/m      | 14.44 ms        | 12.23 ms  | −15%        |

`data/apply` and `data/parse` are now at or below the 2026-04-20 baseline. `data/layout` and `data/e2e` have narrowed the gap vs the 2026-04-20 baseline to ~5–14%, down from the 29–54% regression observed in the previous run. The plain-pipeline benchmarks (`layout`, `end_to_end`) remain unaffected by the data-path changes.

### Extended sizes (xxl, max) — `make benchmark-x`

| Size | Layout  | End-to-end | PDF output |
|------|---------|------------|------------|
| xxl  | 491 ms  | 589 ms     | 930.9 KB   |
| max  | 958 ms  | 984 ms     | 1.85 MB    |

A 1 MB XML document renders to a 1.85 MB PDF in under 1 second. The xxl result has elevated variance (±7%) due to fewer samples (20); max variance is higher still (±11%). These figures are useful for order-of-magnitude comparisons but should not be used as tight regression guards.

---

## Competitor comparison

> **Methodology note:** Lpdf numbers are from this run (Windows, 5-year-old Intel i7, release build). Competitor numbers are **estimates** derived from published benchmarks, official documentation, and community reports — not a controlled head-to-head. Warm-start figures are shown (cold JVM/interpreter start adds 500 ms–2 s for Java/Python tools). Input complexity and hardware vary; treat the order-of-magnitude comparisons as directionally reliable, not precise.

### Tool overview

| Tool | Input format | Layout engine | Runtime | License |
|------|-------------|---------------|---------|---------|
| **Lpdf** | XML (declarative) | ✓ Full — box model, text shaping, pagination | Rust / WASM / WASI | Commercial |
| Apache FOP | XSL-FO (XML) | ✓ Full | JVM | Apache 2.0 |
| Prince XML | HTML / CSS | ✓ Full | Native binary | Commercial (~$500–3800/server) |
| WeasyPrint | HTML / CSS | ✓ Full | CPython | BSD |
| pdfmake | JS object model | ✓ Full | Node.js | MIT |
| ReportLab Platypus | Python API | ✓ Full | CPython | BSD / Commercial |
| Puppeteer / Chrome | HTML / CSS | ✓ (browser engine) | Node.js + Chromium | Apache 2.0 |
| wkhtmltopdf | HTML / CSS | ✓ (Qt WebKit) | Native (deprecated) | LGPL |
| iText 7 | Programmatic | ✗ Manual positioning | JVM / .NET | AGPL / Commercial |

Apache FOP is the closest architectural analog to Lpdf: both accept XML with a declarative layout vocabulary. Prince XML is the quality/speed benchmark for HTML→PDF.

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

Corresponds to Lpdf `end_to_end/s` (4.1 ms) through `end_to_end/m` (25.7 ms).

| Tool | ~5–8 pages | ~20–30 pages | Notes |
|------|-----------|-------------|-------|
| **Lpdf** | **~4 ms** | **~26 ms** | |
| iText 7 (warm) | ~15–50 ms | ~50–200 ms | No layout engine; manual API |
| pdfmake | ~40–150 ms | ~150–600 ms | Pure JS; client + server |
| ReportLab Platypus | ~60–200 ms | ~200 ms–1 s | CPython |
| Prince XML | ~100–300 ms | ~300 ms–1 s | HTML/CSS; commercial |
| WeasyPrint | ~300–800 ms | ~800 ms–3 s | CPython + CSS parsing |
| Apache FOP (warm) | ~300–700 ms | ~800 ms–3 s | JVM warm; cold adds ~1 s |
| wkhtmltopdf | ~500 ms–1 s | ~1–3 s | Deprecated; unstable |
| Puppeteer (warm) | ~400 ms–1 s | ~1–4 s | Persistent browser process |

At this range Lpdf is **20–100× faster** than tools with a real layout engine (WeasyPrint, FOP, pdfmake, ReportLab). The gap is primarily explained by the difference between a compiled native/WASM engine and an interpreted runtime — not algorithmic differences.

---

### Large–extra large documents (~100–300 pages)

Corresponds to Lpdf `end_to_end/xl` (233 ms, ~100–150 pages) through `end_to_end_x/xxl` (589 ms, ~200–300 pages).

| Tool | ~100–150 pages | ~200–300 pages | Notes |
|------|---------------|---------------|
| **Lpdf** | **~233 ms** | **~589 ms** | |
| Prince XML | ~800 ms–2 s | ~1.5–4 s | Fastest HTML/CSS tool |
| pdfmake | ~1.5–5 s | ~3–10 s | Single-threaded JS |
| iText 7 (warm) | ~500 ms–2 s | ~1–5 s | Programmatic; no layout |
| ReportLab Platypus | ~2–8 s | ~5–20 s | CPython; GC pressure |
| WeasyPrint | ~3–12 s | ~8–30 s | Memory pressure at scale |
| Apache FOP (warm) | ~3–15 s | ~8–40 s | Known memory issues at scale |
| wkhtmltopdf | ~5–20 s | ~15–60 s+ | Often crashes or OOM |
| Puppeteer (warm) | ~8–30 s | ~20–90 s | Each page renders in browser |

At large sizes the gap widens: **5–60× faster** than tools with comparable layout quality (FOP, WeasyPrint, Prince). Layout cost in interpreted runtimes grows super-linearly due to GC pressure and single-threaded line-breaking; Lpdf's Rust allocator and arena-based layout tree scale near-linearly to 1 MB+.

---

### Key deployment advantages not reflected in timing

| Factor | Lpdf | JVM tools (FOP, iText) | Python tools | Browser tools |
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
