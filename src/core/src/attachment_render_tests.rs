//! Renders of documents that carry files, with the files loaded from their `src` as an SDK loads them: the objects
//! each file gets, its date, what is refused, and the samples `make check-attachments` and `make check-encryption`
//! hand to qpdf. The WASI crate has its own `LpdfEngine`, so these run with the core library only.

use std::path::{Path, PathBuf};

use miniz_oxide::deflate::compress_to_vec_zlib;

use crate::{LpdfEngine, RenderEncryption, SourceKind};

const CREATED_ON: &str = "2026-10-31T09:30:00Z";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The fixture `showcase-attachments.xml`.
pub(crate) fn fixture() -> String {
    String::from_utf8(read(&fixtures_dir().join("showcase-attachments.xml"))).expect("UTF-8")
}

/// The bytes of each file the fixture carries, by the `src` it names them with.
fn fixture_file(src: &str) -> Vec<u8> {
    read(&fixtures_dir().join(src))
}

/// An unlicensed engine with the files `xml` names with a `src` loaded under their `ref` or name, read relative to
/// the fixtures, as an SDK reads them.
fn engine(xml: &str, created_on: Option<&str>) -> LpdfEngine {
    let mut engine = LpdfEngine::new("");
    for source in crate::source_files(xml).expect("the fixture parses") {
        if source.kind == SourceKind::File {
            engine.load_file(&source.key, &fixture_file(&source.src));
        }
    }
    if let Some(date) = created_on {
        engine.set_created_on(date);
    }
    engine
}

fn render_with(xml: &str, created_on: Option<&str>, encryption: &RenderEncryption) -> Result<Vec<u8>, String> {
    engine(xml, created_on).render_xml(xml, None, encryption)
}

/// The fixture rendered as an SDK renders it, unlicensed and undated: for the snapshot tests.
pub(crate) fn render_fixture() -> Vec<u8> {
    render_with(&fixture(), None, &RenderEncryption::default()).unwrap_or_else(|e| panic!("showcase-attachments: {e}"))
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// The fixture with `<output>` asking for encryption with `cipher`.
fn encrypted_fixture(cipher: &str) -> String {
    fixture().replace("<attachments>", &format!("<output encrypt=\"{cipher}\" />\n    <attachments>"))
}

/// `text`, printable ASCII without a parenthesis or a backslash, as the literal string pdf-writer writes it.
fn literal(text: &str) -> String {
    format!("({text})")
}

#[test]
fn each_file_is_an_embedded_file_with_a_file_specification() {
    let pdf = text(&render_fixture());
    assert_eq!(pdf.matches("/Type /EmbeddedFile").count(), 2, "{pdf}");
    assert_eq!(pdf.matches("/Type /Filespec").count(), 2);
    for expected in [
        "/Subtype /text#2Fxml".to_string(), "/Subtype /text#2Fplain".to_string(),
        format!("/F {}", literal("invoice.xml")), format!("/UF {}", literal("invoice.xml")),
        format!("/F {}", literal("terms.txt")), format!("/UF {}", literal("terms.txt")),
        format!("/Desc {}", literal("The invoice as data, for software to read")),
        "/AFRelationship /Data".to_string(), "/AFRelationship /Supplement".to_string(),
    ] {
        assert!(pdf.contains(&expected), "missing {expected}");
    }
    let invoice = fixture_file("../assets/files/invoice.xml");
    assert!(pdf.contains(&format!("/Size {}", invoice.len())), "the size of the file before it is compressed");
}

#[test]
fn the_bytes_are_carried_as_they_are_compressed() {
    let pdf = render_fixture();
    for src in ["../assets/files/invoice.xml", "../assets/files/terms.txt"] {
        let compressed = compress_to_vec_zlib(&fixture_file(src), 6);
        assert!(pdf.windows(compressed.len()).any(|w| w == compressed.as_slice()), "{src} is not in the PDF");
    }
}

#[test]
fn the_catalog_lists_the_files_by_name_and_as_associated_files() {
    let pdf = text(&render_fixture());
    let catalog = pdf.split("/Type /Catalog").nth(1).expect("a catalog").split("endobj").next().unwrap();
    assert!(catalog.contains("/EmbeddedFiles"), "{catalog}");
    // A name tree's keys in the order of their bytes, whatever the document's order.
    let names = catalog.split("/Names [").nth(1).expect("the name tree's entries");
    assert!(names.find(&literal("invoice.xml")).unwrap() < names.find(&literal("terms.txt")).unwrap(), "{catalog}");
    assert!(catalog.contains("/AF ["), "{catalog}");
}

#[test]
fn the_files_are_dated_as_the_document_is_and_undated_without_a_date() {
    let dated = text(&render_with(&fixture(), Some(CREATED_ON), &RenderEncryption::default()).unwrap());
    assert_eq!(dated.matches(&format!("/ModDate {}", literal("D:20261031093000Z"))).count(), 2, "{dated}");
    assert!(dated.contains(&format!("/CreationDate {}", literal("D:20261031093000Z"))), "{dated}");

    let undated = text(&render_fixture());
    assert!(!undated.contains("/ModDate") && !undated.contains("/CreationDate"));
}

#[test]
fn a_date_with_a_fraction_a_zone_or_an_offset_is_written_as_a_pdfa_file_writes_it() {
    for (created_on, expected) in [
        ("2026-10-31T09:30:00.000Z", "D:20261031093000Z"),
        ("2026-10-31T09:30:00+01:00", "D:20261031093000+01'00"),
        ("2026-10-31T09:30:00.123-05:30", "D:20261031093000-05'30"),
        ("2026-10-31T09:30", "D:20261031093000"),
        ("2026-10-31T09:30:00", "D:20261031093000"),
        ("2026-10-31", "D:20261031"),
    ] {
        let pdf = text(&render_with(&fixture(), Some(created_on), &RenderEncryption::default()).unwrap());
        assert!(pdf.contains(&format!("/CreationDate {}", literal(expected))), "{created_on}: {pdf}");
        assert_eq!(pdf.matches(&format!("/ModDate {}", literal(expected))).count(), 2, "{created_on}");
    }
}

/// The fixture's one file as a JSON kit tree, with nothing else on its one page.
const TREE_WITH_A_FILE: &str = r#"{"version":1,"type":"document","attrs":{"attachments":[{"name":"a.txt","relationship":"supplement"}]},
    "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{},"nodes":[{"type":"text","nodes":["x"]}]}]}]}"#;

#[test]
fn a_created_on_that_is_not_a_date_is_an_error_before_the_layout_on_both_paths() {
    for created_on in ["yesterday", "31.10.2026"] {
        // No file is loaded, so an error that names the date came before the files were looked for, and the layout.
        let mut engine = LpdfEngine::new("");
        engine.set_created_on(created_on);
        let errors = [
            engine.render_xml(&fixture(), None, &RenderEncryption::default()).unwrap_err(),
            engine.render_json(TREE_WITH_A_FILE, &RenderEncryption::default()).unwrap_err(),
        ];
        for error in errors {
            assert!(error.contains(&format!("created_on '{created_on}'")), "{error}");
            assert!(error.contains("2026-10-31T09:30:00Z"), "the form to give: {error}");
        }
    }
}

#[test]
fn the_json_path_dates_its_files_as_the_xml_path_does() {
    let mut engine = LpdfEngine::new("");
    engine.load_file("a.txt", b"A");
    engine.set_created_on("2026-10-31T09:30:00.000+01:00");
    let pdf = text(&engine.render_json(TREE_WITH_A_FILE, &RenderEncryption::default()).unwrap());
    let expected = literal("D:20261031093000+01'00");
    assert!(pdf.contains(&format!("/CreationDate {expected}")) && pdf.contains(&format!("/ModDate {expected}")), "{pdf}");
}

#[test]
fn the_same_input_gives_the_same_bytes() {
    assert_eq!(render_fixture(), render_fixture());
}

#[test]
fn a_document_without_attachments_has_no_embedded_file() {
    let xml = r#"<lpdf version="1"><document><section><layout><text>x</text></layout></section></document></lpdf>"#;
    let pdf = text(&render_with(xml, None, &RenderEncryption::default()).unwrap());
    assert!(!pdf.contains("/EmbeddedFiles") && !pdf.contains("/AF") && !pdf.contains("/Filespec"));
}

#[test]
fn a_file_that_was_not_loaded_is_an_error_before_the_layout() {
    let xml = fixture();
    let error = LpdfEngine::new("").render_xml(&xml, None, &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("file 'invoice.xml'") && error.contains("loadFile('invoice.xml', bytes)"), "{error}");
}

#[test]
fn a_file_loaded_under_its_ref_is_carried_under_its_name() {
    let xml = r#"<lpdf version="1"><document><attachments><file name="factur-x.xml" ref="invoice-42" mime="text/xml"/>
        </attachments><section><layout><text>x</text></layout></section></document></lpdf>"#;
    let mut engine = LpdfEngine::new("");
    engine.load_file("invoice-42", b"<invoice/>");
    let pdf = text(&engine.render_xml(xml, None, &RenderEncryption::default()).unwrap());
    assert!(pdf.contains(&format!("/UF {}", literal("factur-x.xml"))) && !pdf.contains("invoice-42"), "{pdf}");
}

#[test]
fn attachments_with_pdfa_are_an_error_that_says_what_to_change() {
    let xml = fixture().replace("<attachments>", "<output pdfa=\"2b\" />\n    <attachments>");
    let error = render_with(&xml, Some(CREATED_ON), &RenderEncryption::default()).unwrap_err();
    assert!(error.contains("pdfa=\"2b\"") && error.contains("remove the attachments, or remove pdfa"), "{error}");
}

#[test]
fn an_encrypted_document_encrypts_its_files() {
    let encryption = RenderEncryption { owner_password: Some("owner".to_string()), entropy: (0u8..48).collect(), ..Default::default() };
    let pdf = render_with(&encrypted_fixture("aes-256"), None, &encryption).unwrap();
    let compressed = compress_to_vec_zlib(&fixture_file("../assets/files/invoice.xml"), 6);
    assert!(!pdf.windows(compressed.len()).any(|w| w == compressed.as_slice()), "the file is in the clear");
    assert!(text(&pdf).contains("/Type /EmbeddedFile"));
}

/// Writes the files `scripts/check-attachments.mjs` and `scripts/check-encryption.mjs` open with qpdf, when
/// `LPDF_ATTACHMENTS_OUT` names a folder: the fixture, dated, as an SDK renders it, and its files as loaded, under
/// their names, to compare what qpdf gives back with. Without the variable the test does nothing.
#[test]
fn writes_attachment_samples_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_ATTACHMENTS_OUT") else { return };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir.join("files")).expect("the samples' folder");
    let xml = fixture();
    let pdf = render_with(&xml, Some(CREATED_ON), &RenderEncryption::default()).expect("the fixture renders");
    std::fs::write(dir.join("attachments.pdf"), pdf).expect("a sample");
    for source in crate::source_files(&xml).expect("the fixture parses") {
        if source.kind == SourceKind::File {
            std::fs::write(dir.join("files").join(&source.key), fixture_file(&source.src)).expect("a file");
        }
    }
}

/// The fixture asking for encryption with `cipher`, rendered with `encryption`, or with `skip_encryption`: the
/// samples of `writes_samples_for_the_external_check` in `lib.rs`.
pub(crate) fn render_encrypted(cipher: &str, encryption: &RenderEncryption) -> Vec<u8> {
    render_with(&encrypted_fixture(cipher), None, encryption).unwrap_or_else(|e| panic!("encrypted attachments: {e}"))
}
