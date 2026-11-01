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

fn images_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/assets/images")
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

/// A fixture with pictures: the images `photo`, `avatar` and `tall` are read from `image-sample.png`,
/// `image-square.png` and `image-tall.png` in `test/assets/images` and loaded as an SDK would, so the pictures
/// are in the PDF. `tall` is four times as high as it is wide.
fn run_snapshot_with_image(name: &str) {
    let xml_path = fixtures_dir().join(format!("{name}.xml"));
    let xml = std::fs::read_to_string(&xml_path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", xml_path.display()));
    let photo  = std::fs::read(images_dir().join("image-sample.png")).expect("image-sample.png");
    let avatar = std::fs::read(images_dir().join("image-square.png")).expect("image-square.png");
    let tall   = std::fs::read(images_dir().join("image-tall.png")).expect("image-tall.png");

    let images: [(&str, &[u8]); 3] = [("photo", &photo), ("avatar", &avatar), ("tall", &tall)];
    let pdf_bytes = crate::LpdfEngine::render_xml_with_images_to_pdf_bytes(&xml, &images)
        .unwrap_or_else(|e| panic!("render failed for {name}: {e}"));
    check_snapshot(name, &pdf_bytes);
}

/// `showcase-encryption` rendered as the host would ask: the cipher and the permissions of its `<output>` (no
/// copying or modifying), and what the render call says. Its `<output>` asks for AES-256, and `cipher` takes its
/// place, so the RC4 snapshot is the same document in RC4-128. The plain one, with `skip_encryption`, is the
/// document before it had an `<output>`: it may not change.
fn run_snapshot_encrypted(name: &str, cipher: &str, encryption: crate::RenderEncryption) {
    let xml_path = fixtures_dir().join("showcase-encryption.xml");
    let xml = std::fs::read_to_string(&xml_path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", xml_path.display()))
        .replace(r#"encrypt="aes-256""#, &format!(r#"encrypt="{cipher}""#));
    let pdf_bytes = crate::LpdfEngine::render_xml_with_encryption(&xml, &encryption)
        .unwrap_or_else(|e| panic!("render failed for {name}: {e}"));
    check_snapshot(name, &pdf_bytes);
}

/// User and owner passwords, and fixed entropy, so that AES-256, which is random otherwise, gives the same file
/// every run.
fn with_passwords() -> crate::RenderEncryption {
    crate::RenderEncryption {
        user_password:  Some("user".to_string()),
        owner_password: Some("owner".to_string()),
        entropy:        (0u8..48).collect(),
        ..Default::default()
    }
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
fn showcase_code() { run_snapshot("showcase-code"); }
#[test]
fn showcase_code_split() { run_snapshot("showcase-code-split"); }
#[test]
fn showcase_markdown() { run_snapshot_with_image("showcase-markdown"); }
#[test]
fn showcase_markdown_styles() { run_snapshot("showcase-markdown-styles"); }
#[test]
fn showcase_markdown_split() { run_snapshot("showcase-markdown-split"); }
#[test]
fn showcase_markdown_wrap() { run_snapshot("showcase-markdown-wrap"); }
/// One section for each look a `.md` file can start from, each with its styles inline, over the same Markdown:
/// `markdown-sample.md` in the fixtures directory. There is no fixture file, because the looks are the engine's.
fn run_snapshot_markdown_presets(name: &str) {
    let sample = std::fs::read_to_string(fixtures_dir().join("markdown-sample.md")).expect("markdown-sample.md");
    let mut sections = String::new();
    for preset in crate::markdown::style_presets() {
        let styles = &preset.styles[preset.styles.find("<markdown-styles>").expect("a markdown-styles element")..];
        sections.push_str(&format!(
            "<section><markdown margin=\"54pt\">{styles}<markdown-content><![CDATA[{sample}]]></markdown-content></markdown></section>"
        ));
    }
    let xml = format!(r#"<lpdf version="1"><document size="a4">{sections}</document></lpdf>"#);
    let pdf_bytes = crate::LpdfEngine::render_xml_to_pdf_bytes(&xml)
        .unwrap_or_else(|e| panic!("render failed for {name}: {e}"));
    check_snapshot(name, &pdf_bytes);
}

#[test]
fn showcase_markdown_presets() { run_snapshot_markdown_presets("showcase-markdown-presets"); }
#[test]
fn showcase_text_wide_words() { run_snapshot("showcase-text-wide-words"); }
#[test]
fn showcase_markdown_tables() { run_snapshot("showcase-markdown-tables"); }
#[test]
fn showcase_markdown_breaks() { run_snapshot("showcase-markdown-breaks"); }

/// The two lines of the breaks fixture that end in two spaces are what the snapshot shows as breaks: an editor that
/// strips them turns them into spaces and the snapshot into a different page, so this says why.
#[test]
fn the_breaks_fixture_keeps_the_lines_that_end_in_two_spaces() {
    let fixture = std::fs::read_to_string(fixtures_dir().join("showcase-markdown-breaks.xml")).expect("the fixture");
    let ending = fixture.lines().filter(|line| line.ends_with("  ") && !line.trim().is_empty()).count();
    assert_eq!(ending, 3, "three lines of showcase-markdown-breaks.xml end in two spaces, and an editor stripped one");
}
#[test]
fn showcase_table_auto() { run_snapshot("showcase-table-auto"); }
#[test]
fn showcase_table_lines() { run_snapshot("showcase-table-lines"); }
#[test]
fn showcase_lists() { run_snapshot("showcase-lists"); }
#[test]
fn showcase_lists_split() { run_snapshot("showcase-lists-split"); }
#[test]
fn showcase_lists_in_rows() { run_snapshot("showcase-lists-in-rows"); }
#[test]
fn showcase_footnote() { run_snapshot("showcase-footnote"); }
#[test]
fn showcase_footnote_counter() { run_snapshot("showcase-footnote-counter"); }
#[test]
fn showcase_footnote_clip() { run_snapshot("showcase-footnote-clip"); }
#[test]
fn showcase_grid() { run_snapshot("showcase-grid"); }
#[test]
fn showcase_row() { run_snapshot("showcase-row"); }
#[test]
fn showcase_row_between() { run_snapshot("showcase-row-between"); }
#[test]
fn showcase_row_split() { run_snapshot("showcase-row-split"); }
#[test]
fn showcase_row_split_flags() { run_snapshot("showcase-row-split-flags"); }
#[test]
fn showcase_row_split_image() { run_snapshot_with_image("showcase-row-split-image"); }
#[test]
fn showcase_page_clip() { run_snapshot_with_image("showcase-page-clip"); }
#[test]
fn showcase_table() { run_snapshot("showcase-table"); }
#[test]
fn showcase_table_keep() { run_snapshot("showcase-table-keep"); }
#[test]
fn showcase_table_colspan() { run_snapshot("showcase-table-colspan"); }
#[test]
fn showcase_table_rowspan() { run_snapshot("showcase-table-rowspan"); }
#[test]
fn showcase_text_max_lines() { run_snapshot("showcase-text-max-lines"); }
#[test]
fn showcase_text_line_height() { run_snapshot("showcase-text-line-height"); }
#[test]
fn showcase_text_justify() { run_snapshot("showcase-text-justify"); }
#[test]
fn showcase_inherited_style() { run_snapshot("showcase-inherited-style"); }
#[test]
fn showcase_colors() { run_snapshot("showcase-colors"); }
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
fn showcase_encryption() { run_snapshot_encrypted("showcase-encryption", "aes-256", crate::RenderEncryption::skipping(true)); }
#[test]
fn showcase_encryption_rc4() { run_snapshot_encrypted("showcase-encryption-rc4", "rc4-128", with_passwords()); }
#[test]
fn showcase_encryption_aes256() { run_snapshot_encrypted("showcase-encryption-aes256", "aes-256", with_passwords()); }
#[test]
fn showcase_pdfa_2b() { check_snapshot("showcase-pdfa-2b", &crate::pdfa_render_tests::render_fixture("2b")); }
#[test]
fn showcase_pdfa_2u() { check_snapshot("showcase-pdfa-2u", &crate::pdfa_render_tests::render_fixture("2u")); }
/// The structure of a tagged PDF, with the fonts and pictures loaded as an SDK loads them.
#[test]
fn showcase_pdfua() { check_snapshot("showcase-pdfua", &crate::pdfua_render_tests::render_fixture()); }

#[test]
fn showcase_pdfx() { check_snapshot("showcase-pdfx", &crate::pdfx_render_tests::render_fixture()); }
/// Two files the PDF carries, loaded from their `src` as an SDK loads them.
#[test]
fn showcase_attachments() { check_snapshot("showcase-attachments", &crate::attachment_render_tests::render_fixture()); }
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
