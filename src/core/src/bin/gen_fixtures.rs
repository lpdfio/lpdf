//! # gen_fixtures
//!
//! CLI tool that generates static lpdf XML fixture files of a target size.
//!
//! ## Usage
//!
//! ```sh
//! # generate a ~50 KB static fixture at depth 3 and write to stdout
//! cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --size m --depth 3
//!
//! # write to a specific path
//! cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- \
//!     --size m --depth 3 --out test/fixtures/bench_m.xml
//!
//! # generate all standard static bench fixtures (bench_xs through bench_max)
//! cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --all
//!
//! # generate data-binding fixtures (bench_data_s + bench_data_m, XML + JSON)
//! cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --data
//!
//! # generate both static and data fixtures
//! cargo run --manifest-path src/core/Cargo.toml --bin gen_fixtures -- --all --data
//! ```
//!
//! ## Size tiers
//!
//! | Label | Target bytes |
//! |-------|-------------|
//! | xs    |     ~1 200  |
//! | s     |    ~10 000  |
//! | m     |    ~50 000  |
//! | l     |   ~100 000  |
//! | xl    |   ~250 000  |
//! | xxl   |   ~500 000  |
//! | max   | ~1 000 000  |
//!
//! ## Depth levels
//!
//! | Depth | Elements added                                    |
//! |-------|---------------------------------------------------|
//! |  1    | stack + text only                                 |
//! |  2    | + flank, divider, frame                           |
//! |  3    | + grid (3-col), cluster, split                    |
//! |  4    | + table, nested grid                              |
//! |  5    | + deep nested containers, spans, multi-col tables |

use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{self, Write as IoWrite};
use std::path::{Path, PathBuf};

// ── Size table ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct SizeTier {
    label:        &'static str,
    target_bytes: usize,
    default_depth: u8,
}

const TIERS: &[SizeTier] = &[
    SizeTier { label: "xs",  target_bytes:     1_200, default_depth: 1 },
    SizeTier { label: "s",   target_bytes:    10_000, default_depth: 2 },
    SizeTier { label: "m",   target_bytes:    50_000, default_depth: 3 },
    SizeTier { label: "l",   target_bytes:   100_000, default_depth: 3 },
    SizeTier { label: "xl",  target_bytes:   250_000, default_depth: 4 },
    SizeTier { label: "xxl", target_bytes:   500_000, default_depth: 4 },
    SizeTier { label: "max", target_bytes: 1_000_000, default_depth: 4 },
];

fn tier_by_label(label: &str) -> Option<SizeTier> {
    TIERS.iter().copied().find(|t| t.label == label)
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opts = parse_args(&args);

    let fixtures_dir = opts.out.clone().unwrap_or_else(|| {
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(manifest).join("../../test/fixtures")
    });

    if opts.all {
        for tier in TIERS {
            let path = fixtures_dir.join(format!("bench_{}.xml", tier.label));
            let depth = opts.depth.unwrap_or(tier.default_depth);
            let xml = generate(tier, depth);
            write_file(&path, xml.as_bytes());
            eprintln!("wrote {} ({} B) → {}", tier.label, xml.len(), path.display());
        }
    }

    if opts.data {
        // Data-binding fixtures: s (~10 KB) and m (~50 KB)
        for (label, target) in &[("s", 10_000usize), ("m", 50_000usize)] {
            let (xml, json) = generate_data(label, *target);
            let xml_path  = fixtures_dir.join(format!("bench_data_{label}.xml"));
            let json_path = fixtures_dir.join(format!("bench_data_{label}.json"));
            write_file(&xml_path,  xml.as_bytes());
            write_file(&json_path, json.as_bytes());
            eprintln!("wrote bench_data_{label}.xml  ({} B) → {}", xml.len(),  xml_path.display());
            eprintln!("wrote bench_data_{label}.json ({} B) → {}", json.len(), json_path.display());
        }
    }

    if !opts.all && !opts.data {
        // Single-fixture mode
        let label = opts
            .size
            .as_deref()
            .unwrap_or_else(|| usage_exit("--size is required (or use --all / --data)"));
        let tier = tier_by_label(label)
            .unwrap_or_else(|| usage_exit(&format!("unknown size '{label}'; valid: xs s m l xl xxl max")));
        let depth = opts.depth.unwrap_or(tier.default_depth);
        let xml = generate(&tier, depth);

        match opts.out {
            Some(path) => {
                write_file(&path, xml.as_bytes());
                eprintln!("wrote {} B → {}", xml.len(), path.display());
            }
            None => {
                io::stdout()
                    .write_all(xml.as_bytes())
                    .expect("write to stdout failed");
            }
        }
    }
}

fn write_file(path: &Path, data: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| {
            eprintln!("error creating directory {}: {e}", parent.display());
            std::process::exit(1);
        });
    }
    fs::write(path, data).unwrap_or_else(|e| {
        eprintln!("error writing {}: {e}", path.display());
        std::process::exit(1);
    });
}

fn usage_exit(msg: &str) -> ! {
    eprintln!("error: {msg}");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  gen_fixtures --size <xs|s|m|l|xl|xxl|max> [--depth <1-5>] [--out <path>]");
    eprintln!("  gen_fixtures --all  [--depth <1-5>] [--out <fixtures-dir>]  (static fixtures)");
    eprintln!("  gen_fixtures --data [--out <fixtures-dir>]                  (data-binding fixtures)");
    eprintln!("  gen_fixtures --all --data                                   (all fixtures)");
    std::process::exit(1);
}

// ── Argument parsing ──────────────────────────────────────────────────────────

struct Opts {
    size:  Option<String>,
    depth: Option<u8>,
    out:   Option<PathBuf>,
    all:   bool,
    data:  bool,
}

fn parse_args(args: &[String]) -> Opts {
    let mut size  = None;
    let mut depth = None;
    let mut out   = None;
    let mut all   = false;
    let mut data  = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--size" => {
                i += 1;
                size = Some(args.get(i).cloned().unwrap_or_else(|| usage_exit("--size requires a value")));
            }
            "--depth" => {
                i += 1;
                let v = args.get(i).unwrap_or_else(|| usage_exit("--depth requires a value"));
                depth = Some(v.parse::<u8>().unwrap_or_else(|_| usage_exit("--depth must be 1-5")));
            }
            "--out" => {
                i += 1;
                out = Some(PathBuf::from(args.get(i).unwrap_or_else(|| usage_exit("--out requires a value"))));
            }
            "--all"  => { all  = true; }
            "--data" => { data = true; }
            "--help" | "-h" => {
                eprintln!("gen_fixtures — lpdf benchmark fixture generator");
                eprintln!();
                eprintln!("  --size  <xs|s|m|l|xl|xxl|max>   target size tier (static XML)");
                eprintln!("  --depth <1-5>                    structural depth (default: per-tier)");
                eprintln!("  --out   <path>                   output file or dir (default: stdout / test/fixtures)");
                eprintln!("  --all                            generate all static bench fixtures");
                eprintln!("  --data                           generate data-binding fixtures (XML + JSON)");
                std::process::exit(0);
            }
            other => {
                usage_exit(&format!("unknown argument: {other}"));
            }
        }
        i += 1;
    }
    Opts { size, depth, out, all, data }
}

// ── Generator ─────────────────────────────────────────────────────────────────

struct Gen {
    buf:     String,
    counter: usize,
    depth:   u8,
}

impl Gen {
    fn new(depth: u8) -> Self {
        Gen { buf: String::with_capacity(8192), counter: 0, depth }
    }

    fn next(&mut self) -> usize {
        self.counter += 1;
        self.counter
    }

    // ── Building blocks ───────────────────────────────────────────────────────

    fn heading(&mut self, label: &str) {
        let n = self.next();
        writeln!(
            self.buf,
            r#"          <text font-size="xl" font="Helvetica-Bold">{label} {n}</text>"#
        ).unwrap();
    }

    fn divider(&mut self) {
        writeln!(self.buf, r#"          <divider/>"#).unwrap();
    }

    // Depth-1 block: a stack of heading + paragraphs
    fn block_stack(&mut self, idx: usize) {
        writeln!(self.buf, r#"          <stack gap="m">"#).unwrap();
        self.heading(&format!("Section {idx}"));
        for _ in 0..3 {
            let n = self.next();
            writeln!(
                self.buf,
                r#"            <text font-size="m">Paragraph {n}. The layout engine processes each node in the document tree, computing sizes and positions. Each text node is measured using font advance-width tables and line-breaking rules applied to the available width.</text>"#
            ).unwrap();
        }
        writeln!(self.buf, r#"          </stack>"#).unwrap();
    }

    // Depth-2: flank row (label + value)
    fn block_flank(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r#"          <flank>
            <text font-size="m" font="Helvetica-Bold">Field {n}</text>
            <text font-size="m" align="right">Value {n}</text>
          </flank>"#
        ).unwrap();
    }

    // Depth-2: framed callout box
    fn block_frame(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r##"          <frame background="#f1f5f9" border="xs #e2e8f0" padding="m" radius="s">
            <stack gap="xs">
              <text font-size="m" font="Helvetica-Bold">Callout {n}</text>
              <text font-size="s" color="#64748b">This callout box highlights item {n} in the document flow and exercises background, border, padding, and radius rendering in the layout engine.</text>
            </stack>
          </frame>"##
        ).unwrap();
    }

    // Depth-3: 3-column grid of cards
    fn block_grid(&mut self) {
        let n = self.next();
        writeln!(self.buf, r#"          <grid cols="3" gap="m">"#).unwrap();
        for col in 1..=3 {
            let cn = self.next();
            writeln!(
                self.buf,
                r##"            <frame background="#f8fafc" border="xs #e2e8f0" padding="m" radius="s">
              <stack gap="s">
                <text font-size="m" font="Helvetica-Bold">Grid {n}-{col}</text>
                <text font-size="s" color="#475569">Card {cn}: column {col} of 3. Grid cells size to their content with equal column widths determined by the container.</text>
              </stack>
            </frame>"##
            ).unwrap();
        }
        writeln!(self.buf, r#"          </grid>"#).unwrap();
    }

    // Depth-3: cluster of badges
    fn block_cluster(&mut self) {
        writeln!(self.buf, r#"          <cluster gap="s">"#).unwrap();
        for _ in 0..6 {
            let n = self.next();
            writeln!(
                self.buf,
                r##"            <frame background="#dbeafe" padding="xs" radius="xs">
              <text font-size="xs" font="Helvetica-Bold" color="#1d4ed8">Tag {n}</text>
            </frame>"##
            ).unwrap();
        }
        writeln!(self.buf, r#"          </cluster>"#).unwrap();
    }

    // Depth-3: split row (left + right)
    fn block_split(&mut self) {
        let n = self.next();
        let m = self.next();
        writeln!(
            self.buf,
            r##"          <split>
            <text font-size="s" color="#64748b">Left side item {n}</text>
            <text font-size="s" align="right" color="#64748b">Right side item {m}</text>
          </split>"##
        ).unwrap();
    }

    // Depth-4: a simple 3-row table
    fn block_table(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r##"          <table cols="1fr 1fr 1fr" border="xs #e2e8f0">
            <thead background="#f1f5f9">
              <td><text font-size="s" font="Helvetica-Bold">Name</text></td>
              <td><text font-size="s" font="Helvetica-Bold">Value</text></td>
              <td><text font-size="s" font="Helvetica-Bold">Status</text></td>
            </thead>"##
        ).unwrap();
        for row in 1..=4 {
            let rn = self.next();
            writeln!(
                self.buf,
                r##"            <tr>
              <td><text font-size="s">Item {n}-{row}</text></td>
              <td><text font-size="s" align="right">{rn}</text></td>
              <td><text font-size="s" color="#16a34a">Active</text></td>
            </tr>"##
            ).unwrap();
        }
        writeln!(self.buf, r#"          </table>"#).unwrap();
    }

    // Depth-4: nested grid inside a frame (more complex subtree)
    fn block_nested_grid(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r##"          <frame background="#fff7ed" border="xs #fed7aa" padding="m" radius="s">
            <stack gap="m">
              <text font-size="m" font="Helvetica-Bold">Nested container {n}</text>
              <grid cols="2" gap="s">"##
        ).unwrap();
        for col in 1..=4 {
            let cn = self.next();
            writeln!(
                self.buf,
                r##"                <stack gap="xs">
                  <text font-size="s" font="Helvetica-Bold">Sub {n}.{col}</text>
                  <text font-size="xs" color="#78350f">Detail {cn}: nested layout within a framed container with background, padding and border radius applied to the outer box.</text>
                </stack>"##
            ).unwrap();
        }
        writeln!(
            self.buf,
            r#"              </grid>
            </stack>
          </frame>"#
        ).unwrap();
    }

    // Depth-5: deeply-nested structure (frame → grid → stack → flank → spans)
    fn block_deep(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r##"          <frame background="#f0fdf4" border="xs #bbf7d0" padding="m" radius="m">
            <stack gap="m">
              <text font-size="l" font="Helvetica-Bold">Deep section {n}</text>
              <grid cols="2" gap="m">"##
        ).unwrap();
        for col in 1..=2 {
            let cn = self.next();
            writeln!(
                self.buf,
                r##"                <stack gap="s">
                  <flank>
                    <text font-size="m" font="Helvetica-Bold">Col {col} · {cn}</text>
                    <frame background="#dcfce7" padding="xs" radius="xs">
                      <text font-size="xs" color="#166534">Active</text>
                    </frame>
                  </flank>
                  <text font-size="s">Deeply nested content {cn}. <span font="Helvetica-Bold">Bold spans</span> and <span color="#2563eb">coloured spans</span> exercise the inline text-run splitting path in the layout engine alongside the nested container sizing logic.</text>
                  <cluster gap="xs">"##
            ).unwrap();
            for t in 1..=3 {
                let tn = self.next();
                writeln!(
                    self.buf,
                    r##"                    <frame background="#bbf7d0" padding="xs" radius="xs">
                      <text font-size="xs" color="#14532d">Tag {tn}.{t}</text>
                    </frame>"##
                ).unwrap();
            }
            writeln!(self.buf, r#"                  </cluster>"#).unwrap();
            writeln!(self.buf, r#"                </stack>"#).unwrap();
        }
        writeln!(
            self.buf,
            r#"              </grid>
            </stack>
          </frame>"#
        ).unwrap();
    }

    // ── Section builder ───────────────────────────────────────────────────────

    /// Emit one `<section>` worth of content.
    fn emit_section(&mut self, section_idx: usize) {
        writeln!(self.buf, r#"    <section>"#).unwrap();
        writeln!(self.buf, r#"      <layout>"#).unwrap();
        writeln!(self.buf, r#"      <stack gap="l">"#).unwrap();

        // Always emit a heading + some body blocks
        self.heading(&format!("Section {section_idx}"));

        match self.depth {
            1 => {
                for i in 1..=3 {
                    self.block_stack(i);
                }
            }
            2 => {
                self.block_stack(1);
                self.block_flank();
                self.block_flank();
                self.divider();
                self.block_frame();
                self.block_stack(2);
            }
            3 => {
                self.block_stack(1);
                self.divider();
                self.block_grid();
                self.block_split();
                self.block_cluster();
                self.block_frame();
            }
            4 => {
                self.block_stack(1);
                self.divider();
                self.block_table();
                self.divider();
                self.block_nested_grid();
                self.block_grid();
            }
            _ => {
                // depth 5+
                self.block_deep();
                self.divider();
                self.block_table();
                self.block_nested_grid();
                self.block_cluster();
            }
        }

        writeln!(self.buf, r#"      </stack>"#).unwrap();
        writeln!(self.buf, r#"      </layout>"#).unwrap();
        writeln!(self.buf, r#"    </section>"#).unwrap();
    }
}

// ── Top-level generate ────────────────────────────────────────────────────────

fn generate(tier: &SizeTier, depth: u8) -> String {
    // Clamp depth to supported range
    let depth = depth.clamp(1, 5);

    let mut ctx = Gen::new(depth);

    // XML header + document open
    let header = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Generated by gen_fixtures: size={} depth={depth} target={}B -->
<lpdf version="1">
  <document size="a4" margin="40pt">
    <meta title="Benchmark Fixture {}" />
"#,
        tier.label, tier.target_bytes, tier.label.to_uppercase()
    );
    ctx.buf.push_str(&header);

    let footer = "  </document>\n</lpdf>\n";

    // Estimate minimum overhead so we don't overshoot badly
    let overhead = header.len() + footer.len();
    let content_target = tier.target_bytes.saturating_sub(overhead);

    let mut section_idx = 1;
    loop {
        let before = ctx.buf.len() - header.len();
        ctx.emit_section(section_idx);
        let after = ctx.buf.len() - header.len();

        if after >= content_target {
            break;
        }
        // Safety valve: stop if a single section produces nothing (shouldn't happen)
        if after == before {
            break;
        }
        section_idx += 1;
    }

    ctx.buf.push_str(footer);
    ctx.buf
}

// ── Data-binding fixture generator ───────────────────────────────────────────

/// Generate a pair of (XML template, JSON data) for the data-binding bench.
///
/// The XML contains `data-value`, `data-source`, `data-if`, and `data-if-not`
/// attributes. Its size is driven by the number of top-level sections; each
/// section is ~850 bytes of XML template. The JSON provides the data arrays
/// that fill those sections.
///
/// JSON shape:
/// ```json
/// {
///   "invoice_number": "INV-S-001",
///   "company": "Benchmark Corp",
///   "sections": [
///     { "title": "...", "ref": "...", "client": "...", "address": "...",
///       "items": [{ "description": "...", "qty": "1", "amount": "$0.00" }, ...],
///       "total": "$0.00", "paid": true },
///     ...
///   ]
/// }
/// ```
pub fn generate_data(label: &str, target_bytes: usize) -> (String, String) {
    // Each template section is ~850 bytes.  Derive section count from target.
    let bytes_per_section: usize = 870;
    let doc_overhead: usize = 600; // header + footer + document-level fields
    let n_sections = ((target_bytes.saturating_sub(doc_overhead)) / bytes_per_section).max(1);

    let xml = build_data_xml(label, n_sections);
    let json = build_data_json(label, n_sections);
    (xml, json)
}

fn build_data_xml(label: &str, n_sections: usize) -> String {
    let mut buf = String::with_capacity(n_sections * 900 + 700);

    buf.push_str(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Generated by gen_fixtures: data label={label} sections={n_sections} -->
<lpdf version="1">
  <assets>
    <font name="body"    core="Helvetica"/>
    <font name="heading" core="Helvetica-Bold"/>
  </assets>
  <document size="a4" margin="48pt" font="body">
    <section>
      <layout>
      <stack gap="l">

          <!-- document header -->
          <flank>
            <text font="heading" font-size="xl">Report</text>
            <text data-value="invoice_number" align="right">INV-0</text>
          </flank>
          <text data-value="company" font="heading">Company Name</text>
          <divider/>

"#
    ));

    for i in 1..=n_sections {
        let si = format!("sections[{j}]", j = i - 1);
        buf.push_str(&format!(
            r##"          <!-- section {i} -->
          <stack gap="m">
            <flank>
              <text font="heading" font-size="l" data-value="{si}.title">Section {i}</text>
              <text data-value="{si}.ref" align="right">REF-{i:03}</text>
            </flank>
            <stack gap="xs">
              <text font="heading" data-value="{si}.client">Client {i}</text>
              <text data-value="{si}.address">Address {i}</text>
            </stack>
            <divider/>
            <stack data-source="{si}.items" gap="xs">
              <flank>
                <text data-value="description">Item</text>
                <text data-value="amount" align="right">$0.00</text>
              </flank>
            </stack>
            <divider/>
            <flank>
              <text font="heading">Total</text>
              <text data-value="{si}.total" align="right" font="heading">$0.00</text>
            </flank>
            <frame data-if="{si}.paid" background="#dcfce7" padding="xs" radius="xs">
              <text align="center" color="#166534">Paid</text>
            </frame>
            <frame data-if-not="{si}.paid" background="#fef9c3" padding="xs" radius="xs">
              <text align="center" color="#854d0e">Pending</text>
            </frame>
          </stack>
          <divider/>

"##
        ));
    }

    buf.push_str(
        r#"      </stack>
      </layout>
    </section>
  </document>
</lpdf>
"#,
    );

    buf
}

fn build_data_json(label: &str, n_sections: usize) -> String {
    let items_per_section = 5usize;
    let mut buf = String::with_capacity(n_sections * 350 + 100);

    buf.push_str(&format!(
        r#"{{
  "invoice_number": "INV-{label}-001",
  "company": "Benchmark Corp {label}",
  "sections": [
"#
    ));

    for i in 1..=n_sections {
        let total_cents = items_per_section * 15000 + i * 100;
        let total_dollars = total_cents / 100;
        let total_cents_part = total_cents % 100;
        let paid = i % 3 != 0; // every 3rd section is unpaid

        buf.push_str(&format!(
            r#"    {{
      "title": "Section {i} — {label} Report",
      "ref": "REF-{i:03}",
      "client": "Client Organisation {i}",
      "address": "{i} Benchmark Avenue, Test City",
      "items": [
"#
        ));

        for j in 1..=items_per_section {
            let amount = 15000u64 + (i as u64 * 100 + j as u64 * 50);
            let dollars = amount / 100;
            let cents = amount % 100;
            let comma = if j < items_per_section { "," } else { "" };
            buf.push_str(&format!(
                r#"        {{ "description": "Service {i}.{j} — deliverable item for section {i}", "qty": "{j}", "amount": "${dollars}.{cents:02}" }}{comma}
"#
            ));
        }

        let comma = if i < n_sections { "," } else { "" };
        buf.push_str(&format!(
            r#"      ],
      "total": "${total_dollars}.{total_cents_part:02}",
      "paid": {paid}
    }}{comma}
"#
        ));
    }

    buf.push_str(
        r#"  ]
}
"#,
    );

    buf
}
