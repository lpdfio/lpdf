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
    check_snapshot(name, &pdf_bytes);
}

/// A fixture with pictures: the images `photo` and `avatar` are read from `image-sample.png` and
/// `image-square.png` in the fixtures directory and loaded as an SDK would, so the pictures are in the PDF.
fn run_snapshot_with_image(name: &str) {
    let xml_path = fixtures_dir().join(format!("{name}.xml"));
    let xml = std::fs::read_to_string(&xml_path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", xml_path.display()));
    let photo  = std::fs::read(fixtures_dir().join("image-sample.png")).expect("image-sample.png");
    let avatar = std::fs::read(fixtures_dir().join("image-square.png")).expect("image-square.png");

    let pdf_bytes = crate::LpdfEngine::render_xml_with_images_to_pdf_bytes(&xml, &[("photo", &photo), ("avatar", &avatar)])
        .unwrap_or_else(|e| panic!("render failed for {name}: {e}"));
    check_snapshot(name, &pdf_bytes);
}

fn check_snapshot(name: &str, pdf_bytes: &[u8]) {
    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        write_snapshot(name, pdf_bytes);
    } else {
        let snap_path = snapshots_dir().join(format!("{name}.pdf.sha256"));
        let stored = std::fs::read_to_string(&snap_path).unwrap_or_else(|_| {
            panic!(
                "snapshot missing for {name} — run with UPDATE_SNAPSHOTS=1 to generate it\n  {}",
                snap_path.display()
            )
        });
        let hash = sha256_hex(pdf_bytes);
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
fn showcase_column() { run_snapshot("showcase-column"); }
#[test]
fn showcase_row_fill() { run_snapshot("showcase-row-fill"); }
#[test]
fn showcase_frame() { run_snapshot("showcase-frame"); }
#[test]
fn showcase_footnote() { run_snapshot("showcase-footnote"); }
#[test]
fn showcase_footnote_counter() { run_snapshot("showcase-footnote-counter"); }
#[test]
fn showcase_grid() { run_snapshot("showcase-grid"); }
#[test]
fn showcase_row() { run_snapshot("showcase-row"); }
#[test]
fn showcase_row_between() { run_snapshot("showcase-row-between"); }
#[test]
fn showcase_table() { run_snapshot("showcase-table"); }
#[test]
fn showcase_text_max_lines() { run_snapshot("showcase-text-max-lines"); }
#[test]
fn showcase_text_line_height() { run_snapshot("showcase-text-line-height"); }
#[test]
fn showcase_inherited_style() { run_snapshot("showcase-inherited-style"); }
#[test]
fn showcase_shadow() { run_snapshot("showcase-shadow"); }
#[test]
fn showcase_gradient() { run_snapshot("showcase-gradient"); }
#[test]
fn showcase_shape() { run_snapshot("showcase-shape"); }
#[test]
fn showcase_running_header() { run_snapshot("showcase-running-header"); }
#[test]
fn showcase_running_header_dictionary() { run_snapshot("showcase-running-header-dictionary"); }
#[test]
fn showcase_image_box() { run_snapshot_with_image("showcase-image-box"); }
#[test]
fn showcase_image_fill() { run_snapshot_with_image("showcase-image-fill"); }
#[test]
fn showcase_barcode() { run_snapshot("showcase-barcode"); }
#[test]
fn showcase_barcode_fit() { run_snapshot("showcase-barcode-fit"); }
#[test]
fn showcase_forms() { run_snapshot("showcase-forms"); }
#[test]
fn showcase_encryption() { run_snapshot("showcase-encryption"); }
#[test]
fn showcase_canvas_overlay()  { run_snapshot("showcase-canvas-overlay"); }
#[test]
fn showcase_canvas_underlay() { run_snapshot("showcase-canvas-underlay"); }
#[test]
fn showcase_canvas_layer()    { run_snapshot("showcase-canvas-layer"); }
#[test]
fn showcase_canvas_rect()     { run_snapshot("showcase-canvas-rect"); }
#[test]
fn showcase_canvas_text()     { run_snapshot("showcase-canvas-text"); }
#[test]
fn showcase_canvas_opacity()  { run_snapshot("showcase-canvas-opacity"); }
#[test]
fn showcase_region_chrome()  { run_snapshot("showcase-region-chrome"); }
#[test]
fn showcase_region_page_scope() { run_snapshot("showcase-region-page-scope"); }
#[test]
fn showcase_toc() { run_snapshot("showcase-toc"); }
