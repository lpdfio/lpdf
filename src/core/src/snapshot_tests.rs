// Snapshot tests: render each fixture XML to PDF, hash with SHA-256, compare
// against the stored hash in test/snapshots/.
//
// Generate / update snapshots:
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

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn run_snapshot(name: &str) {
    let xml_path = fixtures_dir().join(format!("{name}.xml"));
    let xml = std::fs::read_to_string(&xml_path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", xml_path.display()));

    let pdf_bytes = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml)
        .unwrap_or_else(|e| panic!("render failed for {name}: {e}"));

    let hash = sha256_hex(&pdf_bytes);
    let snap_path = snapshots_dir().join(format!("{name}.pdf.sha256"));

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::write(&snap_path, &hash)
            .unwrap_or_else(|e| panic!("failed to write snapshot {}: {e}", snap_path.display()));
        println!("updated snapshot {name}: {hash}");
    } else {
        let stored = std::fs::read_to_string(&snap_path).unwrap_or_else(|_| {
            panic!(
                "snapshot missing for {name} — run with UPDATE_SNAPSHOTS=1 to generate it\n  {}",
                snap_path.display()
            )
        });
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
