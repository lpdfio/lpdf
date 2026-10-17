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
//! # generate all standard static bench fixtures (bench_xs through bench_max), and the layer
//! # and media fixtures (bench_layers_*, bench_media_*)
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
//! |  1    | column + text only                                |
//! |  2    | + row with a fill child, divider, frame           |
//! |  3    | + grid (3-col), row, row justified between        |
//! |  4    | + table, nested grid                              |
//! |  5    | + deep nested containers, spans, multi-col tables |
//!
//! ## Page fixtures: layers, images, QR codes, linear barcodes
//!
//! `--all` also writes `bench_{kind}_{s,m,l}.xml` for four kinds, at the sizes of the static tiers s, m and l.
//! Depth does not apply to them. Each section is one page, and each kind holds one thing, since a page that
//! holds a QR code spends nearly all of its time encoding it:
//!
//! | Kind   | A page holds |
//! |--------|--------------|
//! | layers | a layer under the flow (shapes, a text-at), a layer over it (a wrapped text-at that ends in an ellipsis, one that wraps, `{page}` and `{pages}`), and a layer faded to 35% |
//! | images | pictures in the flow (one size given, two sizes given, in a grid), and three img-at in a layer |
//! | qr     | three QR codes in the flow (width, height, both), and two barcode-at in a layer |
//! | linear | Code 128 and EAN-13 in the flow (width, height, both, with and without digits), and two barcode-at in a layer |
//!
//! The image fixtures name one picture, `photo`, and a bench loads it.

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

/// The data-binding fixtures: a label and a target size in bytes.
const DATA_TIERS: &[(&str, usize)] = &[("s", 10_000), ("m", 50_000)];

/// The page fixtures (layers, images, QR codes, linear barcodes): a label and a target size in bytes, the sizes
/// of the static tiers s, m and l.
const PAGE_TIERS: &[(&str, usize)] = &[("s", 10_000), ("m", 50_000), ("l", 100_000)];

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
        for (kind, _, _) in PAGE_KINDS {
            for (label, target) in PAGE_TIERS {
                let xml = generate_pages(kind, label, *target);
                let path = fixtures_dir.join(format!("bench_{kind}_{label}.xml"));
                write_file(&path, xml.as_bytes());
                eprintln!("wrote bench_{kind}_{label}.xml ({} B) → {}", xml.len(), path.display());
            }
        }
    }

    if opts.data {
        // Data-binding fixtures: s (~10 KB) and m (~50 KB)
        for (label, target) in DATA_TIERS {
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
    eprintln!("  gen_fixtures --all  [--depth <1-5>] [--out <fixtures-dir>]  (static, layer and media fixtures)");
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
                eprintln!("  --all                            generate all static bench fixtures, and the layer and media ones");
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

    // Depth-1 block: a column of heading + paragraphs
    fn block_column(&mut self, idx: usize) {
        writeln!(self.buf, r#"          <column gap="m">"#).unwrap();
        self.heading(&format!("Section {idx}"));
        for _ in 0..3 {
            let n = self.next();
            writeln!(
                self.buf,
                r#"            <text font-size="m">Paragraph {n}. The layout engine processes each node in the document tree, computing sizes and positions. Each text node is measured using font advance-width tables and line-breaking rules applied to the available width.</text>"#
            ).unwrap();
        }
        writeln!(self.buf, r#"          </column>"#).unwrap();
    }

    // Depth-2: label + value, the value filling the rest
    fn block_label_value(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r#"          <row>
            <text font-size="m" font="Helvetica-Bold">Field {n}</text>
            <text width="fill" font-size="m" align="right">Value {n}</text>
          </row>"#
        ).unwrap();
    }

    // Depth-2: framed callout box
    fn block_frame(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r##"          <frame background="#f1f5f9" border="xs #e2e8f0" padding="m" radius="s">
            <column gap="xs">
              <text font-size="m" font="Helvetica-Bold">Callout {n}</text>
              <text font-size="s" color="#64748b">This callout box highlights item {n} in the document flow and exercises background, border, padding, and radius rendering in the layout engine.</text>
            </column>
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
              <column gap="s">
                <text font-size="m" font="Helvetica-Bold">Grid {n}-{col}</text>
                <text font-size="s" color="#475569">Card {cn}: column {col} of 3. Grid cells size to their content with equal column widths determined by the container.</text>
              </column>
            </frame>"##
            ).unwrap();
        }
        writeln!(self.buf, r#"          </grid>"#).unwrap();
    }

    // Depth-3: row of badges
    fn block_row(&mut self) {
        writeln!(self.buf, r#"          <row gap="s">"#).unwrap();
        for _ in 0..6 {
            let n = self.next();
            writeln!(
                self.buf,
                r##"            <frame background="#dbeafe" padding="xs" radius="xs">
              <text font-size="xs" font="Helvetica-Bold" color="#1d4ed8">Tag {n}</text>
            </frame>"##
            ).unwrap();
        }
        writeln!(self.buf, r#"          </row>"#).unwrap();
    }

    // Depth-3: left + right, at the two edges
    fn block_edges(&mut self) {
        let n = self.next();
        let m = self.next();
        writeln!(
            self.buf,
            r##"          <row justify="between">
            <text font-size="s" color="#64748b">Left side item {n}</text>
            <text font-size="s" align="right" color="#64748b">Right side item {m}</text>
          </row>"##
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
            <column gap="m">
              <text font-size="m" font="Helvetica-Bold">Nested container {n}</text>
              <grid cols="2" gap="s">"##
        ).unwrap();
        for col in 1..=4 {
            let cn = self.next();
            writeln!(
                self.buf,
                r##"                <column gap="xs">
                  <text font-size="s" font="Helvetica-Bold">Sub {n}.{col}</text>
                  <text font-size="xs" color="#78350f">Detail {cn}: nested layout within a framed container with background, padding and border radius applied to the outer box.</text>
                </column>"##
            ).unwrap();
        }
        writeln!(
            self.buf,
            r#"              </grid>
            </column>
          </frame>"#
        ).unwrap();
    }

    // Depth-5: deeply-nested structure (frame → grid → column → row → spans)
    fn block_deep(&mut self) {
        let n = self.next();
        writeln!(
            self.buf,
            r##"          <frame background="#f0fdf4" border="xs #bbf7d0" padding="m" radius="m">
            <column gap="m">
              <text font-size="l" font="Helvetica-Bold">Deep section {n}</text>
              <grid cols="2" gap="m">"##
        ).unwrap();
        for col in 1..=2 {
            let cn = self.next();
            writeln!(
                self.buf,
                r##"                <column gap="s">
                  <row>
                    <text font-size="m" font="Helvetica-Bold">Col {col} · {cn}</text>
                    <frame width="fill" background="#dcfce7" padding="xs" radius="xs">
                      <text font-size="xs" color="#166534">Active</text>
                    </frame>
                  </row>
                  <text font-size="s">Deeply nested content {cn}. <span font="Helvetica-Bold">Bold spans</span> and <span color="#2563eb">coloured spans</span> exercise the inline text-run splitting path in the layout engine alongside the nested container sizing logic.</text>
                  <row gap="xs">"##
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
            writeln!(self.buf, r#"                  </row>"#).unwrap();
            writeln!(self.buf, r#"                </column>"#).unwrap();
        }
        writeln!(
            self.buf,
            r#"              </grid>
            </column>
          </frame>"#
        ).unwrap();
    }

    // ── Section builder ───────────────────────────────────────────────────────

    /// Emit one `<section>` worth of content.
    fn emit_section(&mut self, section_idx: usize) {
        writeln!(self.buf, r#"    <section>"#).unwrap();
        writeln!(self.buf, r#"      <layout margin="40pt">"#).unwrap();
        writeln!(self.buf, r#"      <column gap="l">"#).unwrap();

        // Always emit a heading + some body blocks
        self.heading(&format!("Section {section_idx}"));

        match self.depth {
            1 => {
                for i in 1..=3 {
                    self.block_column(i);
                }
            }
            2 => {
                self.block_column(1);
                self.block_label_value();
                self.block_label_value();
                self.divider();
                self.block_frame();
                self.block_column(2);
            }
            3 => {
                self.block_column(1);
                self.divider();
                self.block_grid();
                self.block_edges();
                self.block_row();
                self.block_frame();
            }
            4 => {
                self.block_column(1);
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
                self.block_row();
            }
        }

        writeln!(self.buf, r#"      </column>"#).unwrap();
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
  <document size="a4">
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

// ── Layer, image and barcode fixture generators ──────────────────────────────

/// The page fixtures, named `bench_{kind}_{label}.xml`: what a page holds, and the `<assets>` block the
/// document declares, or empty. Each kind is one thing, since a document that holds a QR code spends nearly
/// all of its time encoding it, and would show nothing of the rest.
const PAGE_KINDS: &[(&str, &str, fn(&mut String, usize))] = &[
    ("layers", "",           layers_section),
    ("images", IMAGE_ASSETS, images_section),
    ("qr",     "",           qr_section),
    ("linear", "",           linear_section),
];

/// One page fixture of `kind` (a name in `PAGE_KINDS`) at about `target_bytes`: as many pages as it takes.
pub fn generate_pages(kind: &str, label: &str, target_bytes: usize) -> String {
    let (_, assets, section) = PAGE_KINDS
        .iter()
        .find(|(name, _, _)| *name == kind)
        .unwrap_or_else(|| panic!("unknown page fixture '{kind}'"));

    let header = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!-- Generated by gen_fixtures: {kind} label={label} target={target_bytes}B -->
<lpdf version="1">
{assets}  <document size="a4">
    <meta title="Benchmark Fixture {kind} {}" />
"#,
        label.to_uppercase()
    );
    let footer = "  </document>\n</lpdf>\n";

    let content_target = target_bytes.saturating_sub(header.len() + footer.len());
    let mut body = String::new();
    let mut n = 0;
    while body.len() < content_target {
        n += 1;
        section(&mut body, n);
    }
    format!("{header}{body}{footer}")
}

/// A page of layers: shapes and a text under the flow, over it a wrapped text that ends in an ellipsis and a
/// page number, and a group of shapes and text faded to 35%.
fn layers_section(buf: &mut String, n: usize) {
    write!(
        buf,
        r##"    <section>
      <layer>
        <rect x="0pt" y="0pt" width="595pt" height="72pt" fill="#1e3a8a" />
        <circle x="506pt" y="14pt" r="22pt" fill="#3b82f6" />
        <line x1="40pt" y1="770pt" x2="555pt" y2="770pt" stroke="#cbd5e1" stroke-width="0.5pt" />
        <path d="M 40 800 L 100 800 L 70 826 Z" fill="#e0e7ff" stroke="#4f46e5" stroke-width="1pt" />
        <text-at x="40pt" y="22pt" font-size="xl" font="Helvetica-Bold" color="#ffffff">Layers {n}</text-at>
      </layer>
      <layout margin="96pt 40pt 72pt">
        <column gap="m">
          <text font-size="l" font="Helvetica-Bold">Page {n}</text>
          <text font-size="m">Paragraph {n}. The flow of the page is laid out as it always is, between the layer that paints under it and the layers that paint over it. Each layer is drawn after the flow has found its pages.</text>
          <text font-size="m">Paragraph {n}. A layer holds shapes, text, images and barcodes at exact places on the page, and a layer with an opacity is drawn as one group and faded as a whole.</text>
        </column>
      </layout>
      <layer>
        <text-at x="40pt" y="640pt" width="260pt" max-lines="3" ellipsis="true" font-size="s" color="#475569">Note {n}. A text in a layer wraps at its width and keeps at most the lines it is given, and the last line ends in dots when the text does not fit. This one is longer than three lines on purpose, so that the engine has to cut it short and end it in dots, as it does with the long names and descriptions that a printed form has to fit in a box of a fixed size.</text-at>
        <text-at x="320pt" y="640pt" width="235pt" max-lines="4" font-size="s" color="#475569">Note {n}, wrapped and not cut. Four lines are enough for this text, which has no dots at its end.</text-at>
        <text-at anchor="bottom-right" x="-40pt" y="-30pt" font-size="xs" align="right" color="#64748b">Page {{page}} of {{pages}}</text-at>
      </layer>
      <layer opacity="0.35">
        <rect x="60pt" y="300pt" width="300pt" height="160pt" fill="#f59e0b" radius="8pt" />
        <circle x="280pt" y="340pt" r="70pt" fill="#ef4444" />
        <text-at x="80pt" y="360pt" font-size="xxl" font="Helvetica-Bold" color="#111827">Draft {n}</text-at>
      </layer>
    </section>
"##
    )
    .unwrap();
}

/// What the image fixtures declare for their picture: the name the document uses, and a file for it.
const IMAGE_ASSETS: &str = r#"  <assets>
    <image name="photo" src="photo.jpg" />
  </assets>
"#;

/// A page of pictures: in the flow with one size given and with two, and in a layer.
fn images_section(buf: &mut String, n: usize) {
    write!(
        buf,
        r##"    <section>
      <layout margin="40pt">
        <column gap="m">
          <text font-size="xl" font="Helvetica-Bold">Images {n}</text>
          <text font-size="m">Pictures in the flow keep their proportions. One size sets the other, and two sizes are a maximum that the picture fits in.</text>
          <row gap="m" align="start">
            <img name="photo" width="140pt" />
            <img name="photo" width="140pt" height="140pt" />
            <img name="photo" height="120pt" />
          </row>
          <grid cols="4" gap="s">
            <img name="photo" width="90pt" />
            <img name="photo" width="90pt" height="60pt" />
            <img name="photo" height="80pt" />
            <img name="photo" width="100pt" height="100pt" />
            <img name="photo" width="60pt" />
            <img name="photo" width="120pt" height="50pt" />
            <img name="photo" height="60pt" />
            <img name="photo" width="70pt" height="90pt" />
          </grid>
        </column>
      </layout>
      <layer>
        <img-at name="photo" x="300pt" y="680pt" width="90pt" />
        <img-at name="photo" anchor="bottom-right" x="-40pt" y="-40pt" width="150pt" height="60pt" />
        <img-at name="photo" anchor="bottom-left" x="40pt" y="-40pt" height="80pt" />
      </layer>
    </section>
"##
    )
    .unwrap();
}

/// A page of QR codes: in the flow with a width, a height and both, and two in a layer.
fn qr_section(buf: &mut String, n: usize) {
    write!(
        buf,
        r##"    <section>
      <layout margin="40pt">
        <column gap="m">
          <text font-size="xl" font="Helvetica-Bold">QR codes {n}</text>
          <row gap="l" align="start">
            <barcode type="qr" data="https://example.com/qr/{n}" width="96pt" />
            <barcode type="qr" data="https://example.com/qr/{n}/height" height="72pt" />
            <barcode type="qr" data="LP-{n:05}" width="120pt" height="80pt" ec="H" />
          </row>
        </column>
      </layout>
      <layer>
        <barcode-at type="qr" data="https://example.com/qr/{n}/page" anchor="bottom-left" x="40pt" y="-40pt" height="60pt" />
        <barcode-at type="qr" data="https://example.com/qr/{n}/corner" anchor="bottom-right" x="-40pt" y="-40pt" width="60pt" ec="Q" />
      </layer>
    </section>
"##
    )
    .unwrap();
}

/// A page of linear barcodes, Code 128 and EAN-13: in the flow with a width, a height and both, with the digits
/// under them or not, and two in a layer.
fn linear_section(buf: &mut String, n: usize) {
    write!(
        buf,
        r##"    <section>
      <layout margin="40pt">
        <column gap="m">
          <text font-size="xl" font="Helvetica-Bold">Linear barcodes {n}</text>
          <row gap="l" align="start">
            <barcode type="code128" data="LP-{n:05}" width="200pt" />
            <barcode type="code128" data="LP-{n:05}-B" height="52pt" hrt="true" />
            <barcode type="code128" data="LP-{n:05}-C" width="160pt" height="40pt" />
          </row>
          <row gap="l" align="start">
            <barcode type="ean13" data="590123412345" width="140pt" />
            <barcode type="ean13" data="590123412345" height="70pt" hrt="true" />
            <barcode type="ean13" data="590123412345" width="160pt" height="60pt" />
          </row>
          <row gap="l" align="start">
            <barcode type="code128" data="LP-{n:05}-D" width="120pt" />
            <barcode type="ean13" data="400638133393" width="100pt" />
            <barcode type="code128" data="LP-{n:05}-E" height="30pt" />
            <barcode type="ean13" data="400638133393" height="50pt" />
          </row>
        </column>
      </layout>
      <layer>
        <barcode-at type="code128" data="LP-{n:05}" x="40pt" y="700pt" width="200pt" height="40pt" />
        <barcode-at type="ean13" data="590123412345" anchor="bottom-right" x="-40pt" y="-40pt" height="60pt" />
      </layer>
    </section>
"##
    )
    .unwrap();
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
  <document size="a4" font="body">
    <section>
      <layout margin="48pt">
      <column gap="l">

          <!-- document header -->
          <row>
            <text font="heading" font-size="xl">Report</text>
            <text width="fill" data-value="invoice_number" align="right">INV-0</text>
          </row>
          <text data-value="company" font="heading">Company Name</text>
          <divider/>

"#
    ));

    for i in 1..=n_sections {
        let si = format!("sections[{j}]", j = i - 1);
        buf.push_str(&format!(
            r##"          <!-- section {i} -->
          <column gap="m">
            <row>
              <text font="heading" font-size="l" data-value="{si}.title">Section {i}</text>
              <text width="fill" data-value="{si}.ref" align="right">REF-{i:03}</text>
            </row>
            <column gap="xs">
              <text font="heading" data-value="{si}.client">Client {i}</text>
              <text data-value="{si}.address">Address {i}</text>
            </column>
            <divider/>
            <column data-source="{si}.items" gap="xs">
              <row>
                <text data-value="description">Item</text>
                <text width="fill" data-value="amount" align="right">$0.00</text>
              </row>
            </column>
            <divider/>
            <row>
              <text font="heading">Total</text>
              <text width="fill" data-value="{si}.total" align="right" font="heading">$0.00</text>
            </row>
            <frame data-if="{si}.paid" background="#dcfce7" padding="xs" radius="xs">
              <text align="center" color="#166534">Paid</text>
            </frame>
            <frame data-if-not="{si}.paid" background="#fef9c3" padding="xs" radius="xs">
              <text align="center" color="#854d0e">Pending</text>
            </frame>
          </column>
          <divider/>

"##
        ));
    }

    buf.push_str(
        r#"      </column>
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

#[cfg(test)]
mod tests {
    use super::*;

    fn committed(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures").join(name);
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    // The files are compared with `==` and not `assert_eq!`, which would print a megabyte of XML.
    fn assert_committed(name: &str, generated: &str) {
        assert!(
            committed(name) == generated,
            "test/fixtures/{name} is not what gen_fixtures writes: run `make gen-fixtures`, or change the generator to match"
        );
    }

    #[test]
    fn the_static_fixtures_are_what_the_generator_writes() {
        for tier in TIERS {
            assert_committed(&format!("bench_{}.xml", tier.label), &generate(tier, tier.default_depth));
        }
    }

    #[test]
    fn the_page_fixtures_are_what_the_generator_writes() {
        for (kind, _, _) in PAGE_KINDS {
            for (label, target) in PAGE_TIERS {
                assert_committed(&format!("bench_{kind}_{label}.xml"), &generate_pages(kind, label, *target));
            }
        }
    }

    fn contains(pdf: &[u8], needle: &[u8]) -> bool {
        pdf.windows(needle.len()).any(|window| window == needle)
    }

    // A fixture the engine reads differently than it was written would still render a page, and its bench would
    // time something else than layers, pictures and barcodes. So the engine is asked what it lays out: every
    // section is one page, and holds the layers, images or barcodes of its kind, all of them.
    #[test]
    fn the_page_fixtures_hold_what_they_are_for() {
        let photo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/6-book/assets/images/chapter-05-caterpillar.jpg");
        let photo = fs::read(&photo_path).unwrap_or_else(|e| panic!("{}: {e}", photo_path.display()));

        // The bigger tiers are more of the same pages, and take long to lay out in a debug build.
        let (label, target) = PAGE_TIERS[0];

        // (kind, what the layout calls it, how many a page holds)
        for (kind, node, per_page) in [("layers", "canvas-layer", 3), ("images", "image", 14), ("qr", "barcode", 5), ("linear", "barcode", 12)] {
            let xml = generate_pages(kind, label, target);
            let pages = xml.matches("<section>").count();

            let mut engine = lpdf::LpdfEngine::new("");
            engine.load_image("photo", &photo);
            let layout = engine.render(&xml);
            let found = layout.matches(&format!(r#""type":"{node}""#)).count();
            assert_eq!(found, pages * per_page, "{kind} {label}: {pages} pages should hold {per_page} {node} each");
            let laid_out: serde_json::Value = serde_json::from_str(&layout).unwrap();
            assert_eq!(laid_out["pages"].as_array().map(Vec::len), Some(pages), "{kind} {label}: every section should be one page");

            let pdf = lpdf::bench_render_doc_with_images(lpdf::bench_parse(&xml).unwrap(), &[("photo", &photo)]).unwrap();
            match kind {
                "layers" => assert!(contains(&pdf, b"/Group"), "layers {label}: the faded layer is not drawn as a group"),
                "images" => assert!(contains(&pdf, b"/Subtype /Image"), "images {label}: the picture is not in the PDF"),
                _ => {}
            }
        }
    }

    #[test]
    fn the_data_fixtures_are_what_the_generator_writes() {
        for (label, target) in DATA_TIERS {
            let (xml, json) = generate_data(label, *target);
            assert_committed(&format!("bench_data_{label}.xml"), &xml);
            assert_committed(&format!("bench_data_{label}.json"), &json);
        }
    }
}
