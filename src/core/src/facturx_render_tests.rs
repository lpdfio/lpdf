//! Factur-X renders of whole documents, with the fonts and files loaded as an SDK loads them: the profile, the
//! metadata and the defaults each case of the fixture gets, what is refused, `facturx-strict`, and the samples
//! `make check-facturx` hands to the Mustang CLI. The WASI crate has its own `LpdfEngine`, so these run with the core
//! library only.

use std::path::{Path, PathBuf};

use crate::{LpdfEngine, RenderEncryption, SourceKind};

const CREATED_ON: &str = "2026-10-31T09:30:00Z";

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The fixture `showcase-facturx.xml`.
fn fixture() -> String {
    String::from_utf8(read(&test_dir().join("fixtures/showcase-facturx.xml"))).expect("UTF-8")
}

/// The `<output>` and the `<attachments>` of the fixture as it is written, which `document` replaces.
const FIXTURE_OUTPUT: &str = r#"<output pdfa="3b" />"#;

/// The fixture with `output` as the attributes of its `<output>` and `files` as the content of its `<attachments>`.
fn document(output: &str, files: &str) -> String {
    let xml = fixture();
    assert!(xml.contains(FIXTURE_OUTPUT), "the fixture's <output> is {FIXTURE_OUTPUT}");
    let start = xml.find("<attachments>").expect("the fixture's <attachments>") + "<attachments>".len();
    let end = xml.find("</attachments>").expect("the end of the fixture's <attachments>");
    format!("{}{files}{}", &xml[..start], &xml[end..]).replace(FIXTURE_OUTPUT, &format!("<output {output} />"))
}

/// A `<file>` named `name` whose bytes are `src` of `test/assets/files/facturx`, with more `attributes`.
fn file(name: &str, src: &str, attributes: &str) -> String {
    format!(r#"<file name="{name}" src="../assets/files/facturx/{src}" {attributes}/>"#)
}

/// The files beside the invoice that a Factur-X invoice may carry, each of a format the spec lists: a spreadsheet, a
/// PDF, the invoice in UBL and in EDIFACT, under the names the spec gives those two (p. 32).
fn listed_files() -> String {
    [
        file("lines.xlsx", "lines.xlsx", r#"mime="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet""#),
        file("delivery-note.pdf", "delivery-note.pdf", r#"mime="application/pdf" relationship="supplement""#),
        file("factur-xubl.xml", "factur-xubl.xml", r#"mime="application/xml" relationship="alternative""#),
        file("factur-xedifact.edi", "factur-xedifact.edi", r#"mime="text/plain" relationship="alternative""#),
    ]
    .concat()
}

/// Two files of formats the spec doesn't list: a PNG, as the spec's own XRECHNUNG sample carries, and a file with no
/// mime.
fn unlisted_files() -> String {
    [
        r#"<file name="logo.png" src="../assets/images/image-square.png" mime="image/png"/>"#.to_string(),
        file("notes.txt", "notes.txt", ""),
    ]
    .concat()
}

/// The cases of the fixture that have a snapshot hash and a sample of `make check-facturx` each, by name: the
/// fixture as it is written, each profile, levels 3u and 3a, the defaults written as given, BT-24 as real files write
/// it, other files beside the invoice, and `facturx-strict` on what the spec allows.
pub(crate) fn cases() -> Vec<(&'static str, String)> {
    let level = |level: &str| format!(r#"pdfa="{level}""#);
    let invoice = |src: &str| file("factur-x.xml", src, "");
    vec![
        ("en16931", fixture()),
        ("minimum", document(&level("3b"), &invoice("minimum.xml"))),
        ("basic-wl", document(&level("3b"), &invoice("basic-wl.xml"))),
        ("basic", document(&level("3b"), &invoice("basic.xml"))),
        ("extended", document(&level("3b"), &invoice("extended.xml"))),
        ("en16931-3u", document(&level("3u"), &invoice("en16931.xml"))),
        ("en16931-3a", document(&level("3a"), &invoice("en16931.xml"))),
        ("xrechnung-3.0", document(&level("3b"), &file("xrechnung.xml", "xrechnung-3.0.xml", ""))),
        ("xrechnung-2.1", document(&level("3b"), &file("xrechnung.xml", "xrechnung-2.1.xml", ""))),
        ("xrechnung-as-factur-x", document(&level("3b"), &invoice("xrechnung-3.0.xml"))),
        ("application-xml", document(&level("3b"), &file("factur-x.xml", "en16931.xml", r#"mime="application/xml""#))),
        ("minimum-alternative", document(&level("3b"), &file("factur-x.xml", "minimum.xml", r#"relationship="alternative""#))),
        ("bt24-spaces", document(&level("3b"), &invoice("en16931-spaces.xml"))),
        ("bt24-prefix", document(&level("3b"), &invoice("en16931-prefix.xml"))),
        ("other-files", document(&level("3b"), &format!("{}{}{}", invoice("en16931.xml"), listed_files(), unlisted_files()))),
        ("strict", document(r#"pdfa="3b" facturx-strict="true""#, &format!("{}{}", invoice("en16931.xml"), listed_files()))),
    ]
}

/// The name of a case's snapshot: the fixture's own for the fixture as it is written.
pub(crate) fn snapshot_name(case: &str) -> String {
    if case == "en16931" { "showcase-facturx".to_string() } else { format!("showcase-facturx-{case}") }
}

/// An unlicensed engine with the fonts of the fixture loaded under their names, and the files `xml` names with a
/// `src` under their `ref` or name, read relative to the fixtures, as an SDK loads them.
fn engine(xml: &str) -> LpdfEngine {
    let mut engine = LpdfEngine::new("");
    let fonts = test_dir().join("assets/fonts");
    for (name, file) in
        [("body", "CrimsonText-Regular.ttf"), ("heading", "CrimsonText-SemiBold.ttf"), ("sans", "Montserrat-Regular.ttf")]
    {
        engine.load_font(name, &read(&fonts.join(file)));
    }
    for source in crate::source_files(xml).expect("the document parses") {
        if source.kind == SourceKind::File {
            engine.load_file(&source.key, &read(&test_dir().join("fixtures").join(&source.src)));
        }
    }
    engine.set_created_on(CREATED_ON);
    engine
}

fn render(xml: &str) -> Result<Vec<u8>, String> {
    engine(xml).render_xml(xml, None, &RenderEncryption::default())
}

/// A case of `cases` rendered as an SDK renders it, unlicensed: for the snapshot tests.
pub(crate) fn render_case(xml: &str) -> Vec<u8> {
    render(xml).unwrap_or_else(|e| panic!("{e}"))
}

/// The case `name` of `cases`.
fn case(name: &str) -> String {
    cases().into_iter().find(|(case, _)| *case == name).unwrap_or_else(|| panic!("no case {name}")).1
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// The XMP packet of a PDF the engine wrote, which it leaves uncompressed.
fn xmp(pdf: &[u8]) -> String {
    let pdf = text(pdf);
    let start = pdf.find("<?xpacket begin").expect("an XMP packet");
    let end = pdf[start..].find("<?xpacket end=\"w\"?>").expect("the end of the packet") + start;
    pdf[start..end + "<?xpacket end=\"w\"?>".len()].to_string()
}

/// The four Factur-X properties of a file's XMP packet, in the order the spec lists them: `fx:DocumentType`,
/// `fx:DocumentFileName`, `fx:Version` and `fx:ConformanceLevel`; `None` for one that is missing.
fn invoice_properties(pdf: &[u8]) -> [Option<String>; 4] {
    let packet = xmp(pdf);
    ["DocumentType", "DocumentFileName", "Version", "ConformanceLevel"].map(|name| {
        let open = format!("<fx:{name}>");
        let start = packet.find(&open)? + open.len();
        let end = packet[start..].find('<')? + start;
        Some(packet[start..end].to_string())
    })
}

fn expected(file_name: &str, version: &str, level: &str) -> [Option<String>; 4] {
    ["INVOICE", file_name, version, level].map(|value| Some(value.to_string()))
}

/// The `/AFRelationship` and the `/Subtype` written for the file named `name`: its file specification and its
/// embedded file stream, which the engine writes one after the other.
fn written(pdf: &[u8], name: &str) -> (String, String) {
    let pdf = text(pdf);
    let spec = pdf.find(&format!("/F ({name})")).unwrap_or_else(|| panic!("no file specification for {name}"));
    let spec_end = spec + pdf[spec..].find("endobj").expect("the end of the specification");
    let relationship = pdf[spec..spec_end].split("/AFRelationship /").nth(1).expect("an /AFRelationship");
    let relationship = relationship.split(|c: char| !c.is_ascii_alphanumeric()).next().unwrap_or("").to_string();
    let stream = pdf[..spec].rfind("/Type /EmbeddedFile").expect("the file's stream before its specification");
    let subtype = pdf[stream..].split("/Subtype /").nth(1).expect("a /Subtype");
    let subtype = subtype.split(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>').next().unwrap_or("").to_string();
    (relationship, subtype.replace("#2F", "/"))
}

fn refused(xml: &str) -> String {
    render(xml).expect_err("a refused render")
}

// ── What a Factur-X file gets ─────────────────────────────────────────────────

#[test]
fn each_profile_is_read_from_bt24_and_named_in_the_xmp_with_its_default_relationship() {
    for (name, level, relationship) in [
        ("minimum", "MINIMUM", "Data"),
        ("basic-wl", "BASIC WL", "Data"),
        ("basic", "BASIC", "Alternative"),
        ("en16931", "EN 16931", "Alternative"),
        ("extended", "EXTENDED", "Alternative"),
    ] {
        let pdf = render(&case(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "1.0", level), "{name}");
        assert_eq!(written(&pdf, "factur-x.xml"), (relationship.to_string(), "text/xml".to_string()), "{name}");
        let packet = xmp(&pdf);
        assert!(packet.contains("<pdfaid:part>3</pdfaid:part>") && packet.contains("<pdfaid:conformance>B</pdfaid:conformance>"), "{name}");
    }
}

#[test]
fn level_3u_is_named_in_the_xmp_packet() {
    let pdf = render(&case("en16931-3u")).expect("renders");
    let packet = xmp(&pdf);
    assert!(packet.contains("<pdfaid:part>3</pdfaid:part>") && packet.contains("<pdfaid:conformance>U</pdfaid:conformance>"));
    assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "1.0", "EN 16931"));
}

#[test]
fn level_3a_is_named_in_the_xmp_packet_and_tags_the_invoice() {
    let pdf = render(&case("en16931-3a")).expect("renders");
    let packet = xmp(&pdf);
    assert!(packet.contains("<pdfaid:part>3</pdfaid:part>") && packet.contains("<pdfaid:conformance>A</pdfaid:conformance>"));
    assert!(!packet.contains("pdfuaid"), "{packet}");
    assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "1.0", "EN 16931"));
    assert!(String::from_utf8_lossy(&pdf).contains("/StructTreeRoot"));
}

#[test]
fn xrechnung_takes_its_version_from_bt24() {
    for (name, version) in [("xrechnung-3.0", "3.0"), ("xrechnung-2.1", "2.1")] {
        let pdf = render(&case(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(invoice_properties(&pdf), expected("xrechnung.xml", version, "XRECHNUNG"), "{name}");
        assert_eq!(written(&pdf, "xrechnung.xml"), ("Alternative".to_string(), "text/xml".to_string()), "{name}");
    }
}

#[test]
fn an_xrechnung_bt24_in_factur_x_xml_is_written_as_given() {
    let pdf = render(&case("xrechnung-as-factur-x")).expect("renders");
    assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "3.0", "XRECHNUNG"));
}

#[test]
fn the_document_file_name_is_the_name_the_file_is_carried_under() {
    let pdf = text(&render(&fixture()).expect("renders"));
    assert!(pdf.contains("/F (factur-x.xml)") && pdf.contains("/UF (factur-x.xml)"), "the file's F and UF");
    assert!(pdf.contains("<fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>"));
    assert!(pdf.contains("/Desc (The invoice as data, for accounting software to read)"), "the description as given");
}

#[test]
fn a_mime_and_a_relationship_the_document_writes_are_written_as_given() {
    let pdf = render(&case("application-xml")).expect("renders");
    assert_eq!(written(&pdf, "factur-x.xml"), ("Alternative".to_string(), "application/xml".to_string()));
    let pdf = render(&case("minimum-alternative")).expect("renders");
    assert_eq!(written(&pdf, "factur-x.xml"), ("Alternative".to_string(), "text/xml".to_string()));
    assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "1.0", "MINIMUM"));
    for (relationship, written_as) in
        [("unspecified", "Unspecified"), ("supplement", "Supplement"), ("source", "Source"), ("data", "Data")]
    {
        let xml = document(r#"pdfa="3b""#, &file("factur-x.xml", "en16931.xml", &format!(r#"relationship="{relationship}""#)));
        let pdf = render(&xml).unwrap_or_else(|e| panic!("{relationship}: {e}"));
        assert_eq!(written(&pdf, "factur-x.xml"), (written_as.to_string(), "text/xml".to_string()), "{relationship}");
    }
}

#[test]
fn bt24_is_read_with_spaces_around_it_and_under_another_prefix() {
    for name in ["bt24-spaces", "bt24-prefix"] {
        let pdf = render(&case(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "1.0", "EN 16931"), "{name}");
    }
}

#[test]
fn the_other_files_are_carried_as_the_document_declares_them() {
    let pdf = render(&case("other-files")).expect("renders");
    for (name, relationship, mime) in [
        ("lines.xlsx", "Unspecified", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        ("delivery-note.pdf", "Supplement", "application/pdf"),
        ("factur-xubl.xml", "Alternative", "application/xml"),
        ("factur-xedifact.edi", "Alternative", "text/plain"),
        ("logo.png", "Unspecified", "image/png"),
        ("notes.txt", "Unspecified", "application/octet-stream"),
    ] {
        assert_eq!(written(&pdf, name), (relationship.to_string(), mime.to_string()), "{name}");
    }
    assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "1.0", "EN 16931"));
    let pdf = text(&pdf);
    let af = pdf.split("/AF [").nth(1).expect("the catalog's /AF").split(']').next().unwrap();
    assert_eq!(af.matches(" R").count(), 7, "the invoice and six files: {af}");
}

/// The extension schema the spec's example block describes (`2. FACTUR-X_extension_schema_example.xmp.txt` of the
/// Factur-X 1.09.2 package): its name, namespace and prefix, and each property's name, value type, category and
/// description. No checker reads it: the Mustang CLI and veraPDF both passed a wrong namespace on 2026-11-02.
const SPEC_SCHEMA: (&str, &str, &str) =
    ("Factur-X PDFA Extension Schema", "urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#", "fx");
const SPEC_PROPERTIES: [(&str, &str, &str, &str); 4] = [
    ("DocumentFileName", "Text", "external", "The name of the embedded XML document"),
    ("DocumentType", "Text", "external", "The type of the hybrid document in capital letters, e.g. INVOICE or ORDER"),
    ("Version", "Text", "external", "The actual version of the standard applying to the embedded XML document"),
    ("ConformanceLevel", "Text", "external", "The conformance level of the embedded XML document"),
];

#[test]
fn the_extension_schema_is_the_one_of_the_specs_example() {
    const SCHEMA: &str = "http://www.aiim.org/pdfa/ns/schema#";
    const PROPERTY: &str = "http://www.aiim.org/pdfa/ns/property#";
    let packet = xmp(&render(&fixture()).expect("renders"));
    let document = roxmltree::Document::parse(&packet).expect("the XMP packet is XML");
    let child = |node: roxmltree::Node, namespace: &str, name: &str| {
        node.children()
            .find(|n| n.tag_name().namespace() == Some(namespace) && n.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(str::to_string)
    };
    let schemas: Vec<_> = document
        .descendants()
        .filter(|n| n.tag_name().namespace() == Some(SCHEMA) && n.tag_name().name() == "prefix")
        .filter_map(|n| n.parent())
        .collect();
    let schema = schemas.iter().find(|s| child(**s, SCHEMA, "prefix").as_deref() == Some("fx")).expect("the fx schema");
    assert_eq!(
        (child(*schema, SCHEMA, "schema"), child(*schema, SCHEMA, "namespaceURI"), child(*schema, SCHEMA, "prefix")),
        (Some(SPEC_SCHEMA.0.to_string()), Some(SPEC_SCHEMA.1.to_string()), Some(SPEC_SCHEMA.2.to_string())),
    );
    let properties: Vec<_> = schema
        .descendants()
        .filter(|n| n.tag_name().namespace() == Some(PROPERTY) && n.tag_name().name() == "name")
        .filter_map(|n| n.parent())
        .map(|p| {
            [child(p, PROPERTY, "name"), child(p, PROPERTY, "valueType"), child(p, PROPERTY, "category"), child(p, PROPERTY, "description")]
        })
        .collect();
    let spec: Vec<_> = SPEC_PROPERTIES.iter().map(|(a, b, c, d)| [*a, *b, *c, *d].map(|v| Some(v.to_string()))).collect();
    assert_eq!(properties, spec);
    // The namespace the properties are written in is the one the schema describes.
    assert!(packet.contains(&format!("xmlns:fx=\"{}\"", SPEC_SCHEMA.1)), "{packet}");
}

#[test]
fn a_pdfa_3_file_without_the_invoice_file_has_no_factur_x_metadata() {
    let pdf = render(&document(r#"pdfa="3b""#, &listed_files())).expect("renders");
    let packet = xmp(&pdf);
    assert!(packet.contains("<pdfaid:part>3</pdfaid:part>"), "{packet}");
    assert!(!packet.contains("fx:") && !packet.contains("Factur-X") && !packet.contains("pdfaExtension"), "{packet}");
    assert_eq!(written(&pdf, "lines.xlsx").0, "Unspecified");
}

#[test]
fn a_pdfa_3_file_with_pdfua_describes_both_schemas_in_one_bag() {
    let xml = fixture().replace(FIXTURE_OUTPUT, r#"<output pdfa="3u" pdfua="1" />"#);
    let packet = xmp(&render(&xml).expect("renders"));
    assert_eq!(packet.matches("<pdfaExtension:schemas>").count(), 1, "{packet}");
    assert!(packet.contains("<pdfaSchema:prefix>pdfuaid</pdfaSchema:prefix>") && packet.contains("<pdfaSchema:prefix>fx</pdfaSchema:prefix>"));
    assert!(packet.contains("<pdfuaid:part>1</pdfuaid:part>") && packet.contains("<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>"));
}

#[test]
fn the_same_input_gives_the_same_bytes() {
    for (name, xml) in [("en16931", fixture()), ("other-files", case("other-files"))] {
        assert_eq!(render(&xml).expect("renders"), render(&xml).expect("renders"), "{name}");
    }
}

#[test]
fn the_json_path_makes_the_same_invoice() {
    let tree = |output: &str, files: &str| format!(
        r#"{{"version":1,"type":"document","attrs":{{"font":"body","assets":{{"fonts":[{{"name":"body","src":"body.ttf"}}]}},
        "output":{output},"attachments":[{files}]}},
        "nodes":[{{"type":"section","attrs":{{}},"nodes":[{{"type":"layout","attrs":{{}},"nodes":[{{"type":"text","nodes":["Invoice"]}}]}}]}}]}}"#
    );
    let mut engine = engine("<lpdf/>");
    engine.load_file("factur-x.xml", &read(&test_dir().join("assets/files/facturx/minimum.xml")));
    engine.load_file("notes.txt", b"Notes");
    let render = |json: &str| engine.render_json(json, &RenderEncryption::default());
    for level in ["3b", "3u"] {
        let pdf = render(&tree(&format!(r#"{{"pdfa":"{level}"}}"#), r#"{"name":"factur-x.xml"}"#)).unwrap_or_else(|e| panic!("{level}: {e}"));
        assert_eq!(invoice_properties(&pdf), expected("factur-x.xml", "1.0", "MINIMUM"), "{level}");
        assert_eq!(written(&pdf, "factur-x.xml"), ("Data".to_string(), "text/xml".to_string()), "{level}");
    }
    let error = render(&tree(r#"{"pdfa":"2u"}"#, r#"{"name":"factur-x.xml"}"#)).unwrap_err();
    assert!(error.contains("pdfa=\"3a\", pdfa=\"3b\" or pdfa=\"3u\""), "{error}");
    let strict = tree(r#"{"pdfa":"3b","facturx-strict":true}"#, r#"{"name":"factur-x.xml"},{"name":"notes.txt"}"#);
    let error = render(&strict).unwrap_err();
    assert!(error.contains("facturx-strict") && error.contains("'notes.txt' has no mime"), "{error}");
}

// ── What it refuses ───────────────────────────────────────────────────────────

#[test]
fn the_invoice_file_in_a_document_that_is_not_pdfa_3_is_an_error_that_says_what_to_change() {
    for output in [r#"pdfa="2b""#, r#"pdfa="2u""#] {
        let error = refused(&document(output, &file("factur-x.xml", "en16931.xml", "")));
        assert!(error.contains(&format!("<output {output}>")) && error.contains("Factur-X invoice"), "{error}");
        assert!(error.contains("write pdfa=\"3a\", pdfa=\"3b\" or pdfa=\"3u\"") && error.contains("another name"), "{error}");
    }
    let xml = document(r#"pdfa="3b""#, &file("xrechnung.xml", "xrechnung-3.0.xml", "")).replace(r#"<output pdfa="3b" />"#, "");
    let error = refused(&xml);
    assert!(error.contains("'xrechnung.xml'") && error.contains("<output> has no pdfa") && error.contains("add pdfa=\"3a\", pdfa=\"3b\" or pdfa=\"3u\""), "{error}");
    // Checked when the document is read, before a file is looked for.
    assert!(crate::parse::parse(&xml).is_err());
}

/// The fixture carrying `bytes` as its invoice, loaded under `factur-x.xml`.
fn render_invoice(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let xml = fixture();
    let mut engine = engine(&xml);
    engine.load_file("factur-x.xml", bytes);
    engine.render_xml(&xml, None, &RenderEncryption::default())
}

#[test]
fn a_bt24_that_is_missing_or_names_no_profile_is_an_error_that_says_what_to_change() {
    let en16931 = String::from_utf8(read(&test_dir().join("assets/files/facturx/en16931.xml"))).expect("UTF-8");
    let missing = en16931.replace("<ram:ID>urn:cen.eu:en16931:2017</ram:ID>", "");
    let error = render_invoice(missing.as_bytes()).unwrap_err();
    assert!(error.contains("file 'factur-x.xml'") && error.contains("names no profile") && error.contains("(BT-24)"), "{error}");
    let unknown = en16931.replace("urn:cen.eu:en16931:2017<", "urn:factur-x.eu:1p0:comfort<");
    let error = render_invoice(unknown.as_bytes()).unwrap_err();
    assert!(error.contains("BT-24 'urn:factur-x.eu:1p0:comfort' names no Factur-X profile"), "{error}");
    assert!(error.contains("urn:cen.eu:en16931:2017 (EN 16931)"), "the values to write: {error}");
}

#[test]
fn xml_that_does_not_parse_is_an_error_that_names_the_file() {
    for bytes in [b"<rsm:CrossIndustryInvoice>".as_slice(), b"factur-x"] {
        let error = render_invoice(bytes).unwrap_err();
        assert!(error.contains("file 'factur-x.xml'") && error.contains("does not parse"), "{error}");
    }
}

#[test]
fn two_files_that_could_be_the_invoice_are_an_error() {
    let both = format!("{}{}", file("factur-x.xml", "en16931.xml", ""), file("xrechnung.xml", "xrechnung-3.0.xml", ""));
    let error = refused(&document(r#"pdfa="3b""#, &both));
    assert!(error.contains("'factur-x.xml' and 'xrechnung.xml' could each be the invoice") && error.contains("keep factur-x.xml"), "{error}");
    let twice = format!("{}{}", file("factur-x.xml", "en16931.xml", ""), file("factur-x.xml", "minimum.xml", r#"ref="other""#));
    let error = refused(&document(r#"pdfa="3b""#, &twice));
    assert!(error.contains("two files are named 'factur-x.xml'"), "{error}");
}

// ── facturx-strict ────────────────────────────────────────────────────────────

/// The document with `files` rendered with `facturx-strict` and without it: the error with it, and that it renders
/// without it.
fn strict_error(files: &str) -> String {
    render(&document(r#"pdfa="3b""#, files)).unwrap_or_else(|e| panic!("renders without facturx-strict: {e}"));
    refused(&document(r#"pdfa="3b" facturx-strict="true""#, files))
}

#[test]
fn strict_refuses_an_invoice_typed_other_than_text_xml() {
    let error = strict_error(&file("factur-x.xml", "en16931.xml", r#"mime="application/xml""#));
    assert!(error.starts_with("<output facturx-strict=\"true\">") && error.contains("mime 'application/xml'"), "{error}");
    assert!(error.contains("write mime=\"text/xml\" or leave mime out") && error.ends_with("or remove facturx-strict"), "{error}");
}

#[test]
fn strict_refuses_a_relationship_the_profile_does_not_allow() {
    for (src, name, relationship, allowed) in [
        ("minimum.xml", "factur-x.xml", "alternative", "allows data:"),
        ("basic-wl.xml", "factur-x.xml", "source", "allows data:"),
        ("en16931.xml", "factur-x.xml", "supplement", "allows alternative or source or data"),
        ("extended.xml", "factur-x.xml", "unspecified", "allows alternative or source or data"),
        ("xrechnung-3.0.xml", "xrechnung.xml", "data", "XRECHNUNG allows alternative"),
    ] {
        let error = strict_error(&file(name, src, &format!(r#"relationship="{relationship}""#)));
        assert!(error.contains(&format!("relationship '{relationship}'")) && error.contains(allowed), "{src}: {error}");
    }
    for (src, relationship) in [("en16931.xml", "source"), ("en16931.xml", "data"), ("minimum.xml", "data")] {
        let xml = document(r#"pdfa="3b" facturx-strict="true""#, &file("factur-x.xml", src, &format!(r#"relationship="{relationship}""#)));
        render(&xml).unwrap_or_else(|e| panic!("{src} {relationship}: {e}"));
    }
}

#[test]
fn strict_refuses_another_file_of_a_format_the_spec_does_not_list_or_with_no_mime() {
    let invoice = file("factur-x.xml", "en16931.xml", "");
    let png = r#"<file name="logo.png" src="../assets/images/image-square.png" mime="image/png"/>"#;
    let error = strict_error(&format!("{invoice}{png}"));
    assert!(error.contains("'logo.png' has mime 'image/png'") && error.contains("PDF, TXT, GIF"), "{error}");
    let error = strict_error(&format!("{invoice}{}", file("notes.txt", "notes.txt", "")));
    assert!(error.contains("'notes.txt' has no mime") && error.contains("text/plain"), "{error}");
}

#[test]
fn strict_writes_what_the_spec_allows_as_it_is_written_without_it() {
    let strict = case("strict");
    let relaxed = strict.replace(r#" facturx-strict="true""#, "");
    assert_ne!(strict, relaxed);
    assert_eq!(render(&strict).expect("renders"), render(&relaxed).expect("renders"));
    let upper = document(r#"pdfa="3b" facturx-strict="true""#, &file("factur-x.xml", "en16931.xml", r#"mime="TEXT/XML""#));
    render(&upper).expect("a type is compared without regard to case");
}

#[test]
fn strict_does_nothing_without_the_invoice_file() {
    let files = unlisted_files();
    let strict = document(r#"pdfa="3b" facturx-strict="true""#, &files);
    assert_eq!(render(&strict).expect("renders"), render(&document(r#"pdfa="3b""#, &files)).expect("renders"));
    let plain = r#"<lpdf version="1"><document><output facturx-strict="true"/><attachments><file name="notes.txt" src="../assets/files/facturx/notes.txt"/></attachments><section><layout><text>x</text></layout></section></document></lpdf>"#;
    render(plain).expect("a plain PDF with the flag renders");
}

// ── The samples of the external check ─────────────────────────────────────────

/// Writes the files `scripts/check-facturx.mjs` hands to the Mustang CLI, when `LPDF_FACTURX_OUT` names a folder: each
/// case of `cases`, rendered unlicensed, as `<case>.pdf`. Without the variable the test does nothing, so a plain
/// `cargo test` needs no other tool.
#[test]
fn writes_facturx_samples_for_the_external_check() {
    let Some(dir) = std::env::var_os("LPDF_FACTURX_OUT") else { return };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).expect("the samples' folder");
    for (name, xml) in cases() {
        std::fs::write(dir.join(format!("{name}.pdf")), render_case(&xml)).expect("a sample");
    }
}
