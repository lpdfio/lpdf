//! `LCrossIndustryInvoice.generatePdf`, rendered as an SDK renders it: the template `showcase-cross-industry-invoice`
//! with the values of an invoice, a document built in code, and the template at level 3a. Each is a Factur-X file,
//! PDF/A-3 with the invoice attached byte for byte, and a render with no invoice shows the template's own text. The
//! three renders have snapshot hashes, and are among the samples `make check-facturx` hands to the Mustang CLI, which
//! runs veraPDF on them.

use std::path::{Path, PathBuf};

use miniz_oxide::deflate::compress_to_vec_zlib;

use crate::{cross_industry_invoice, data, parse, InvoiceTemplate, LpdfEngine, RenderEncryption};

const CREATED_ON: &str = "2026-11-18T10:00:00Z";

fn test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The template `showcase-cross-industry-invoice.xml`.
fn template() -> String {
    String::from_utf8(read(&test_dir().join("fixtures/showcase-cross-industry-invoice.xml"))).expect("UTF-8")
}

/// The committed invoice `name` of `test/assets/files/facturx`.
fn invoice(name: &str) -> Vec<u8> {
    read(&test_dir().join("assets/files/facturx").join(name))
}

/// An unlicensed engine with the template's font loaded under its name, as an SDK loads it.
fn engine() -> LpdfEngine {
    let mut engine = LpdfEngine::new("");
    engine.load_font("body", &read(&test_dir().join("assets/fonts/SourceSans3-Regular.otf")));
    engine.set_created_on(CREATED_ON);
    engine
}

/// The values of `invoice_xml` with the default map, as `extract_invoice_data` reads them, which an SDK binds as they
/// are when the call has no `format`.
fn values(invoice_xml: &[u8]) -> String {
    cross_industry_invoice::extract(invoice_xml, None).unwrap_or_else(|e| panic!("{e}"))["data"].to_string()
}

/// `generatePdf` with `template`, the invoice `invoice_xml`, the default map and the caller's `data`.
fn generate(template: &str, invoice_xml: &[u8], data: Option<&str>) -> Result<Vec<u8>, String> {
    let values = values(invoice_xml);
    let invoice = InvoiceTemplate { xml: invoice_xml, map: None, data: &values };
    engine().render_xml_with(template, data, &RenderEncryption::default(), Some(invoice))
}

/// A page built in code, as a kit tree: it has no bindings, so it shows the values the code put in it.
fn tree() -> String {
    r#"{"version":1,"type":"document","attrs":{"size":"a4","font":"body","assets":{"fonts":[{"name":"body","src":"body.otf"}]},
       "meta":{"title":"Invoice 2026-0412","lang":"en"}},
       "nodes":[{"type":"section","attrs":{},"nodes":[{"type":"layout","attrs":{"margin":"48pt"},"nodes":[
         {"type":"text","attrs":{"font-size":"20pt"},"nodes":["Northwind Paper SAS"]},
         {"type":"text","attrs":{},"nodes":["Invoice 2026-0412, 31 October 2026"]},
         {"type":"text","attrs":{},"nodes":["Total 69.60 EUR"]}]}]}]}"#
        .to_string()
}

/// The three renders of the snapshots and of the external check, by the name of their snapshot.
pub(crate) fn cases() -> Vec<(&'static str, Vec<u8>)> {
    let en16931 = invoice("en16931.xml");
    let note = Some(r#"{"note":"Payable within 30 days, by bank transfer."}"#);
    let at_3a = template().replace("<meta ", "<output pdfa=\"3a\"/>\n    <meta ");
    let code = engine().render_json_with(&tree(), &RenderEncryption::default(), Some(&en16931));
    vec![
        ("showcase-cross-industry-invoice", generate(&template(), &en16931, note)),
        ("showcase-cross-industry-invoice-code", code),
        ("showcase-cross-industry-invoice-3a", generate(&at_3a, &en16931, note)),
    ]
    .into_iter()
    .map(|(name, pdf)| (name, pdf.unwrap_or_else(|e| panic!("{name}: {e}"))))
    .collect()
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// Whether `pdf` carries `bytes` as a file, compressed as the engine compresses an embedded file.
fn carries(pdf: &[u8], bytes: &[u8]) -> bool {
    let compressed = compress_to_vec_zlib(bytes, 6);
    pdf.windows(compressed.len()).any(|w| w == compressed.as_slice())
}

#[test]
fn each_render_is_pdfa_3_and_carries_the_invoice_as_factur_x_xml() {
    let en16931 = invoice("en16931.xml");
    for (name, pdf) in cases() {
        let level = if name.ends_with("-3a") { "A" } else { "B" };
        let written = text(&pdf);
        assert!(written.contains("<pdfaid:part>3</pdfaid:part>"), "{name}");
        assert!(written.contains(&format!("<pdfaid:conformance>{level}</pdfaid:conformance>")), "{name}");
        assert!(written.contains("<fx:DocumentFileName>factur-x.xml</fx:DocumentFileName>"), "{name}");
        assert!(written.contains("<fx:ConformanceLevel>EN 16931</fx:ConformanceLevel>"), "{name}");
        assert!(written.contains("/F (factur-x.xml)") && written.contains("/AFRelationship /Alternative"), "{name}");
        assert!(carries(&pdf, &en16931), "{name}: the invoice is not carried byte for byte");
    }
}

#[test]
fn an_xrechnung_invoice_is_attached_as_xrechnung_xml() {
    let pdf = generate(&template(), &invoice("xrechnung-3.0.xml"), None).unwrap();
    let written = text(&pdf);
    assert!(written.contains("<fx:DocumentFileName>xrechnung.xml</fx:DocumentFileName>"));
    assert!(written.contains("<fx:ConformanceLevel>XRECHNUNG</fx:ConformanceLevel>"));
}

/// The words of the texts of `doc`'s layouts, in document order.
fn words(doc: &parse::Document) -> Vec<String> {
    fn walk(node: &parse::Node, into: &mut Vec<String>) {
        into.extend(node.text_runs.iter().map(|run| run.text.clone()));
        node.children.iter().for_each(|child| walk(child, into));
    }
    let mut into = Vec::new();
    for section in &doc.sections {
        for child in &section.children {
            if let parse::SectionChild::Layout { children, .. } = child {
                for layout_child in children {
                    if let parse::LayoutChild::Content(node) = layout_child {
                        walk(node, &mut into);
                    }
                }
            }
        }
    }
    into
}

#[test]
fn the_template_binds_the_invoice_under_its_prefix_beside_the_callers_data() {
    let mut doc = parse::parse(&template()).unwrap();
    let values: serde_json::Value = serde_json::from_str(&values(&invoice("en16931.xml"))).unwrap();
    let data = serde_json::json!({ "note": "Payable within 30 days." });
    cross_industry_invoice::bind_template(&mut doc, Some(data), None, values).unwrap();
    let words = words(&doc);
    for expected in ["Northwind Paper SAS", "FR32123456789", "2026-0412", "Atelier Lakeside SARL", "2026-10-31", "2026-11-30",
        "A4 paper, 80 g/m², ream of 500 sheets", "Envelopes, C5, box of 500", "H87", "18.50", "11.60", "69.60",
        "Payable within 30 days."]
    {
        assert!(words.iter().any(|word| word == expected), "{expected} is not on the page: {words:?}");
    }
    assert_eq!(words.iter().filter(|word| *word == "20").count(), 3, "two lines and one VAT rate: {words:?}");
    assert!(!words.iter().any(|word| word == "VAT in"), "the line of the tax currency stays out without one: {words:?}");
}

#[test]
fn a_render_with_no_invoice_shows_the_templates_own_text_for_its_invoice_paths() {
    // With no data, as the VS Code preview renders it, and with data that has no invoice.
    let without = parse::parse(&template()).unwrap();
    let mut with_data = parse::parse(&template()).unwrap();
    data::apply(&mut with_data, serde_json::json!({ "note": "A note of the caller" })).unwrap();
    for doc in [without, with_data] {
        let words = words(&doc);
        for expected in ["Seller", "Buyer", "0000", "Item", "VAT in", "0.00"] {
            assert!(words.iter().any(|word| word == expected), "{expected} is not on the page: {words:?}");
        }
    }
    assert!(engine().render_xml(&template(), None, &RenderEncryption::default()).is_ok());
    let words = words(&{
        let mut doc = parse::parse(&template()).unwrap();
        data::apply(&mut doc, serde_json::json!({ "note": "A note of the caller" })).unwrap();
        doc
    });
    assert!(words.iter().any(|word| word == "A note of the caller"), "the caller's data is bound: {words:?}");
}

#[test]
fn a_template_path_the_map_lacks_stops_the_render_with_its_name() {
    let typo = template().replace("invoice:seller.name", "invoice:sellr.name");
    let error = generate(&typo, &invoice("en16931.xml"), None).unwrap_err();
    assert!(error.starts_with("data-value=\"invoice:sellr.name\": 'sellr' is not in the map"), "{error}");
}

#[test]
fn generate_pdf_refuses_a_template_that_declares_the_invoice_or_sets_another_level() {
    let declared = template().replace(
        "<meta ",
        "<attachments><file name=\"factur-x.xml\" src=\"../assets/files/facturx/en16931.xml\"/></attachments>\n    <meta ",
    );
    let error = generate(&declared, &invoice("en16931.xml"), None).unwrap_err();
    assert!(error.starts_with("the document declares the file 'factur-x.xml'"), "{error}");
    let at_2b = template().replace("<meta ", "<output pdfa=\"2b\"/>\n    <meta ");
    let error = generate(&at_2b, &invoice("en16931.xml"), None).unwrap_err();
    assert!(error.starts_with("<output pdfa=\"2b\">: generatePdf makes a Factur-X invoice"), "{error}");
    // The SDK's extract refuses it first; the render refuses it too.
    let minimum = invoice("minimum.xml");
    let invoice = InvoiceTemplate { xml: &minimum, map: None, data: "{}" };
    let error = engine().render_xml_with(&template(), None, &RenderEncryption::default(), Some(invoice)).unwrap_err();
    assert!(error.starts_with("the invoice's profile is MINIMUM"), "{error}");
}

#[test]
fn what_pdfa_refuses_names_generate_pdf_when_the_call_set_the_level() {
    let helvetica = template().replace(r#"<document size="a4" font="body">"#, r#"<document size="a4">"#);
    let error = generate(&helvetica, &invoice("en16931.xml"), None).unwrap_err();
    assert!(error.starts_with("generatePdf makes the invoice a PDF/A-3b file: the text \"Northwind Paper SAS\" is in Helvetica"), "{error}");
    let cmyk = template().replacen("#5b6475", "cmyk(0, 0, 0, 70)", 1);
    let error = generate(&cmyk, &invoice("en16931.xml"), None).unwrap_err();
    assert!(error.starts_with("generatePdf makes the invoice a PDF/A-3b file: the color cmyk(0, 0, 0, 70) is CMYK"), "{error}");
    assert!(error.ends_with("such as a hex one"), "{error}");
    // A level the template sets itself is the template's, which can drop it.
    let own = cmyk.replace("<meta ", "<output pdfa=\"3b\"/>\n    <meta ");
    let error = generate(&own, &invoice("en16931.xml"), None).unwrap_err();
    assert!(error.starts_with("<output pdfa=\"3b\">: the color cmyk(0, 0, 0, 70) is CMYK"), "{error}");
    assert!(error.ends_with("such as a hex one, or remove pdfa"), "{error}");
    // The date PDF/A needs, too.
    let values = values(&invoice("en16931.xml"));
    let invoice_xml = invoice("en16931.xml");
    let mut undated = engine();
    undated.set_created_on("the 31st");
    let invoice = InvoiceTemplate { xml: &invoice_xml, map: None, data: &values };
    let error = undated.render_xml_with(&template(), None, &RenderEncryption::default(), Some(invoice)).unwrap_err();
    assert!(error.starts_with("generatePdf makes the invoice a PDF/A-3b file: created_on 'the 31st' is not a date"), "{error}");
    let invoice = InvoiceTemplate { xml: &invoice_xml, map: None, data: &values };
    let mut no_date = LpdfEngine::new("");
    no_date.load_font("body", &read(&test_dir().join("assets/fonts/SourceSans3-Regular.otf")));
    let error = no_date.render_xml_with(&template(), None, &RenderEncryption::default(), Some(invoice)).unwrap_err();
    assert!(error.starts_with("generatePdf makes the invoice a PDF/A-3b file, which carries the date it is made"), "{error}");
}

#[test]
fn a_document_built_in_code_gets_the_invoice_and_pdfa_3b_too() {
    let pdf = engine().render_json_with(&tree(), &RenderEncryption::default(), Some(&invoice("extended.xml"))).unwrap();
    let written = text(&pdf);
    assert!(written.contains("<pdfaid:conformance>B</pdfaid:conformance>"));
    assert!(written.contains("<fx:ConformanceLevel>EXTENDED</fx:ConformanceLevel>"));
    assert!(carries(&pdf, &invoice("extended.xml")));
}

#[test]
fn the_same_input_gives_the_same_bytes() {
    assert_eq!(cases(), cases());
}
