// Snapshot tests: render each fixture XML to PDF, hash with SHA-256, compare
// against the stored hash in test/snapshots/.
//
// Generate / update snapshots (also writes PDFs to test/snapshots/pdf/ for visual review):
//   UPDATE_SNAPSHOTS=1 cargo test --manifest-path src/core/Cargo.toml snapshot
//
// Normal run (CI):
//   cargo test --manifest-path src/core/Cargo.toml snapshot

use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/fixtures")
}

fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/snapshots")
}

fn pdf_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/snapshots/pdf")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_snapshot(name: &str, pdf_bytes: &[u8]) {
    let dir = snapshots_dir();
    let pdf_out = pdf_dir();
    std::fs::create_dir_all(&pdf_out)
        .unwrap_or_else(|e| panic!("failed to create pdf dir: {e}"));

    let hash = sha256_hex(pdf_bytes);

    std::fs::write(dir.join(format!("{name}.pdf.sha256")), &hash)
        .unwrap_or_else(|e| panic!("failed to write snapshot {name}: {e}"));
    std::fs::write(pdf_out.join(format!("{name}.pdf")), pdf_bytes)
        .unwrap_or_else(|e| panic!("failed to write pdf {name}: {e}"));

    println!("updated snapshot {name}: {hash}");
}

fn run_snapshot(name: &str) {
    let xml_path = fixtures_dir().join(format!("{name}.xml"));
    let xml = std::fs::read_to_string(&xml_path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", xml_path.display()));

    let pdf_bytes = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml)
        .unwrap_or_else(|e| panic!("render failed for {name}: {e}"));

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        write_snapshot(name, &pdf_bytes);
    } else {
        let snap_path = snapshots_dir().join(format!("{name}.pdf.sha256"));
        let stored = std::fs::read_to_string(&snap_path).unwrap_or_else(|_| {
            panic!(
                "snapshot missing for {name} — run with UPDATE_SNAPSHOTS=1 to generate it\n  {}",
                snap_path.display()
            )
        });
        let hash = sha256_hex(&pdf_bytes);
        assert_eq!(
            stored.trim(), hash,
            "PDF output changed for {name}. Run with UPDATE_SNAPSHOTS=1 to accept the new output."
        );
    }
}

macro_rules! snapshot_test {
    ($name:ident) => {
        #[test]
        fn $name() {
            run_snapshot(stringify!($name));
        }
    };
}

snapshot_test!(example1);
snapshot_test!(example2);
snapshot_test!(example3);
snapshot_test!(example4);
snapshot_test!(example5);
snapshot_test!(example6);
snapshot_test!(example7);
snapshot_test!(example8);
snapshot_test!(example9);
snapshot_test!(example10);
snapshot_test!(example11);

snapshot_test!(bench_xs);
snapshot_test!(bench_s);
snapshot_test!(bench_m);
snapshot_test!(bench_l);
snapshot_test!(bench_xl);

#[test]
fn showcase_cluster() { run_snapshot("showcase-cluster"); }
#[test]
fn showcase_flank() { run_snapshot("showcase-flank"); }
#[test]
fn showcase_frame() { run_snapshot("showcase-frame"); }
#[test]
fn showcase_grid() { run_snapshot("showcase-grid"); }
#[test]
fn showcase_split() { run_snapshot("showcase-split"); }
#[test]
fn showcase_stack() { run_snapshot("showcase-stack"); }
#[test]
fn showcase_table() { run_snapshot("showcase-table"); }
#[test]
fn showcase_barcode() { run_snapshot("showcase-barcode"); }
#[test]
fn showcase_forms() { run_snapshot("showcase-forms"); }
#[test]
fn showcase_encryption() { run_snapshot("showcase-encryption"); }
#[test]
fn showcase_canvas_overlay() { run_snapshot("showcase-canvas-overlay"); }
#[test]
fn showcase_region_chrome()  { run_snapshot("showcase-region-chrome"); }

// ── Canvas snapshot tests ─────────────────────────────────────────────────────

/// Build a minimal canvas JSON document with all primitive types and render to
/// PDF, verifying the bytes are a valid PDF and snapshot-stable.
#[test]
fn canvas_snapshot() {
    let json = r##"{
        "version": 1,
        "type": "canvas-document",
        "nodes": [
            {
                "type": "canvas-section",
                "attrs": { "width": 595, "height": 842 },
                "nodes": [
                    {
                        "type": "canvas-rect",
                        "attrs": { "x": 40, "y": 40, "w": 200, "h": 100,
                                   "fill": "#4a90e2", "stroke": "#1a5276",
                                   "strokeWidth": 2, "borderRadius": 8 }
                    },
                    {
                        "type": "canvas-rect",
                        "attrs": { "x": 260, "y": 40, "w": 200, "h": 100,
                                   "fill": "#f0f0f0" }
                    },
                    {
                        "type": "canvas-line",
                        "attrs": { "x1": 40, "y1": 170, "x2": 555, "y2": 170,
                                   "stroke": "#333333", "strokeWidth": 1 }
                    },
                    {
                        "type": "canvas-ellipse",
                        "attrs": { "cx": 140, "cy": 280, "rx": 80, "ry": 50,
                                   "fill": "#f39c12", "stroke": "#d68910",
                                   "strokeWidth": 2 }
                    },
                    {
                        "type": "canvas-circle",
                        "attrs": { "cx": 400, "cy": 280, "r": 60,
                                   "fill": "#27ae60" }
                    },
                    {
                        "type": "canvas-path",
                        "attrs": { "d": "M 40 400 L 200 350 L 360 400 Z",
                                   "fill": "#8e44ad", "fillRuleEvenodd": false }
                    },
                    {
                        "type": "canvas-text",
                        "attrs": { "x": 40, "y": 460, "content": "Hello Canvas!",
                                   "font": "Helvetica", "size": 24,
                                   "color": "#1a1a1a" }
                    },
                    {
                        "type": "canvas-text",
                        "attrs": { "x": 40, "y": 500, "content": "Centered text",
                                   "font": "Helvetica", "size": 14,
                                   "color": "#666666", "align": "center",
                                   "width": 515 }
                    },
                    {
                        "type": "canvas-layer",
                        "attrs": { "opacity": 0.5 },
                        "nodes": [
                            {
                                "type": "canvas-rect",
                                "attrs": { "x": 40, "y": 540, "w": 515, "h": 60,
                                           "fill": "#e74c3c" }
                            }
                        ]
                    }
                ]
            }
        ]
    }"##;

    let engine = crate::LpdfEngine::new("");
    let result = engine.render_tree_pdf(json);
    let pdf_bytes = result.expect("canvas render should succeed");

    // Verify it's a valid PDF.
    assert!(pdf_bytes.starts_with(b"%PDF-"), "output should start with %PDF-");

    // Snapshot / update.
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        write_snapshot("canvas_snapshot", &pdf_bytes);
    } else {
        let snap_path = snapshots_dir().join("canvas_snapshot.pdf.sha256");
        let hash = sha256_hex(&pdf_bytes);
        if snap_path.exists() {
            let stored = std::fs::read_to_string(&snap_path).expect("read snapshot");
            assert_eq!(
                stored.trim(), hash,
                "canvas PDF output changed — run with UPDATE_SNAPSHOTS=1 to accept"
            );
        } else {
            write_snapshot("canvas_snapshot", &pdf_bytes);
            println!("created initial canvas snapshot: {hash}");
        }
    }
}
