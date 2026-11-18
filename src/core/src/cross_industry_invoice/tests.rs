//! The default map, and the committed invoices of `test/assets/files/facturx/`: each one LCrossIndustryInvoice reads
//! gives, with the default map, the data of the JSON beside it, and each one it refuses gives its error.
//!
//! `UPDATE_INVOICE_DATA=1 cargo test --manifest-path src/core/Cargo.toml cross_industry_invoice` writes the JSON files.
//! A diff of one says which value changed.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::*;

const FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/assets/files/facturx");

/// The XML files of the folder that are refused, with what the error says.
const REFUSED: [(&str, &str); 3] = [
    ("minimum.xml", "the invoice's profile is MINIMUM, whose XML is only a booking aid"),
    ("basic-wl.xml", "the invoice's profile is BASIC WL, whose XML is only a booking aid"),
    ("factur-xubl.xml", "the invoice XML is a <Invoice>, not a Cross Industry Invoice"),
];

fn invoices() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(FOLDER)
        .expect("the folder of the Factur-X test files")
        .map(|entry| entry.expect("a folder entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "xml"))
        .collect();
    files.sort();
    files
}

fn file_name(path: &Path) -> &str {
    path.file_name().and_then(|name| name.to_str()).expect("a file name")
}

fn read(name: &str) -> Value {
    let bytes = std::fs::read(Path::new(FOLDER).join(name)).expect("a committed invoice");
    extract(&bytes, None).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn the_default_map_reads_and_fits_the_schema() {
    let map = invoice_map(None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(map.prefix, "invoice");
}

#[test]
fn each_committed_invoice_reads_to_the_json_beside_it_or_is_refused() {
    let update = std::env::var("UPDATE_INVOICE_DATA").is_ok();
    let mut read_count = 0;
    for path in invoices() {
        let name = file_name(&path);
        let bytes = std::fs::read(&path).expect("the invoice");
        if let Some((_, says)) = REFUSED.iter().find(|(refused, _)| *refused == name) {
            let error = extract(&bytes, None).expect_err(name);
            assert!(error.starts_with(says), "{name}: {error}");
            continue;
        }
        let data = extract(&bytes, None).unwrap_or_else(|e| panic!("{name}: {e}"))["data"].take();
        let expected_path = path.with_extension("json");
        let written = format!("{}\n", serde_json::to_string_pretty(&data).expect("JSON"));
        if update {
            std::fs::write(&expected_path, &written).expect("the expected JSON");
        }
        let expected = std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
            panic!("{name} has no JSON beside it: run with UPDATE_INVOICE_DATA=1 to write it, then read it")
        });
        assert_eq!(written, expected, "{name} reads to other data than its JSON: run with UPDATE_INVOICE_DATA=1 and read the diff");
        read_count += 1;
    }
    assert!(read_count >= 20, "only {read_count} invoices were read");
}

/// The places an entry's values may be at in the data: `["lines", "*", "unit"]`.
fn places(entries: &[map::Entry], outer: &[String], into: &mut Vec<(String, Vec<String>)>) {
    for entry in entries {
        let mut at = outer.to_vec();
        at.push(entry.name.clone());
        match &entry.kind {
            map::Kind::Value(_) => into.push((entry.full_name.clone(), at)),
            map::Kind::Group(inner) => places(inner, &at, into),
            map::Kind::List(inner) => {
                at.push("*".to_string());
                places(inner, &at, into);
            }
        }
    }
}

/// The values at `at` in `data`, `*` standing for each item of a list.
fn values_at<'a>(data: &'a Value, at: &[String]) -> Vec<&'a Value> {
    let Some((first, rest)) = at.split_first() else { return vec![data] };
    match (first.as_str(), data) {
        ("*", Value::Array(items)) => items.iter().flat_map(|item| values_at(item, rest)).collect(),
        (key, Value::Object(object)) => object.get(key).map(|value| values_at(value, rest)).unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[test]
fn the_every_entry_invoice_fills_every_value_of_the_default_map() {
    let data = read("case-every-entry.xml")["data"].take();
    let map = invoice_map(None).unwrap();
    let mut all = Vec::new();
    places(&map.entries, &[], &mut all);
    let empty: Vec<&str> =
        all.iter().filter(|(_, at)| values_at(&data, at).is_empty()).map(|(name, _)| name.as_str()).collect();
    assert!(empty.is_empty(), "case-every-entry.xml leaves these entries empty: {}", empty.join(", "));
    assert!(all.len() > 150, "the default map has {} values", all.len());
}

#[test]
fn each_typed_value_is_named_after_its_entry_and_is_a_string_at_its_place() {
    for path in invoices() {
        let name = file_name(&path);
        if REFUSED.iter().any(|(refused, _)| *refused == name) {
            continue;
        }
        let extracted = read(name);
        for typed in extracted["typed"].as_array().unwrap() {
            let mut value = &extracted["data"];
            let mut entry = Vec::new();
            for step in typed["at"].as_array().unwrap() {
                value = match step {
                    Value::String(key) => {
                        entry.push(key.as_str());
                        &value[key.as_str()]
                    }
                    index => &value[index.as_u64().unwrap() as usize],
                };
            }
            assert!(value.is_string(), "{name}: {typed} finds {value}");
            assert_eq!(typed["name"], entry.join("."), "{name}: {typed}");
            assert!(["date", "amount", "code"].contains(&typed["type"].as_str().unwrap()), "{name}: {typed}");
        }
    }
}

#[test]
fn the_vat_totals_are_told_apart_by_their_currency_in_either_order() {
    let data = read("case-foreign-currency.xml")["data"].take();
    assert_eq!(data["totals"]["currency"], "GBP");
    assert_eq!(data["totals"]["vat"], "190.00");
    assert_eq!(data["totals"]["taxCurrency"], "EUR");
    assert_eq!(data["totals"]["vatInTaxCurrency"], "218.50");
    let xml = std::fs::read_to_string(Path::new(FOLDER).join("case-foreign-currency.xml")).unwrap();
    let (gbp, eur) = (r#"currencyID="GBP">190.00"#, r#"currencyID="EUR">218.50"#);
    assert!(xml.find(gbp) < xml.find(eur), "the pound total comes first in the file");
    let swapped = xml.replace(gbp, "{gbp}").replace(eur, gbp).replace("{gbp}", eur);
    assert_ne!(swapped, xml, "the two totals were swapped");
    let data = extract(swapped.as_bytes(), None).unwrap()["data"].take();
    assert_eq!((data["totals"]["vat"].as_str(), data["totals"]["vatInTaxCurrency"].as_str()), (Some("190.00"), Some("218.50")));
}

#[test]
fn a_value_a_typed_entry_lacks_is_absent_and_a_list_with_no_items_is_empty() {
    let data = read("en16931.xml")["data"].take();
    assert!(data["totals"].get("vatInTaxCurrency").is_none());
    assert!(data["seller"].get("contacts").is_some_and(|contacts| contacts == &Value::Array(vec![])));
    assert!(data.get("payee").is_none(), "a group the invoice lacks is absent");
    assert_eq!(data["issued"], "2026-10-31", "a date in its normal form");
}

/// The invoice of `en16931.xml` with `from` replaced by `to`.
fn en16931_with(from: &str, to: &str) -> String {
    let xml = std::fs::read_to_string(Path::new(FOLDER).join("en16931.xml")).unwrap();
    assert!(xml.contains(from), "{from}");
    xml.replacen(from, to, 1)
}

#[test]
fn a_required_value_the_invoice_lacks_is_an_error_that_names_the_entry() {
    let xml = en16931_with("<ram:Name>Northwind Paper SAS</ram:Name>", "");
    let error = extract(xml.as_bytes(), None).unwrap_err();
    assert_eq!(error, "the invoice map's entry 'seller.name' is required, and the invoice has none");
    let xml = en16931_with("<ram:Name>Envelopes, C5, box of 500</ram:Name>", "<ram:Name> </ram:Name>");
    let map = r#"<cross-industry-invoice-map prefix="invoice">
      <list name="lines" path="rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem">
        <value name="name" path="ram:SpecifiedTradeProduct/ram:Name" required="true"/>
      </list>
    </cross-industry-invoice-map>"#;
    let error = extract(xml.as_bytes(), Some(map)).unwrap_err();
    assert_eq!(error, "the invoice map's entry 'lines.name' is required, and the invoice has none in item 2 of 'lines'");
}

#[test]
fn a_value_that_finds_several_in_an_invoice_is_an_error() {
    let xml = en16931_with(
        "<ram:ID schemeID=\"VA\">FR32123456789</ram:ID>",
        "<ram:ID schemeID=\"VA\">FR32123456789</ram:ID>\n        </ram:SpecifiedTaxRegistration>\n        \
         <ram:SpecifiedTaxRegistration>\n          <ram:ID schemeID=\"VA\">FR99999999999</ram:ID>",
    );
    let error = extract(xml.as_bytes(), None).unwrap_err();
    assert!(error.starts_with("the invoice map's entry 'seller.vatId' finds 2 values in the invoice"), "{error}");
}

#[test]
fn a_date_is_read_in_the_format_its_attribute_names() {
    let map = r#"<cross-industry-invoice-map prefix="invoice">
      <value name="issued" path="rsm:ExchangedDocument/ram:IssueDateTime/udt:DateTimeString" type="date"/>
    </cross-industry-invoice-map>"#;
    for (written, normal) in [
        (r#"format="102">20261031"#, "2026-10-31"),
        (r#"format="610">202610"#, "2026-10"),
        (r#"format="616">202644"#, "2026-W44"),
    ] {
        let xml = en16931_with(r#"format="102">20261031"#, written);
        assert_eq!(extract(xml.as_bytes(), Some(map)).unwrap()["data"]["issued"], normal, "{written}");
    }
    for written in [r#"format="102">2026-10-31"#, r#"format="102">20261331"#, r#"format="610">20261031"#, r#"format="203">202610311200"#] {
        let xml = en16931_with(r#"format="102">20261031"#, written);
        let error = extract(xml.as_bytes(), Some(map)).unwrap_err();
        assert!(error.starts_with("the invoice map's entry 'issued' is a date, and the invoice writes"), "{written}: {error}");
    }
}

#[test]
fn an_invoice_that_is_not_a_cross_industry_invoice_or_not_xml_is_refused() {
    let error = extract(b"\xff\xfe", None).unwrap_err();
    assert!(error.contains("not UTF-8"), "{error}");
    let error = extract(b"<rsm:CrossIndustryInvoice", None).unwrap_err();
    assert!(error.starts_with("the invoice XML is not well-formed XML"), "{error}");
    let error = extract(en16931_with("urn:cen.eu:en16931:2017", "urn:example:other").as_bytes(), None).unwrap_err();
    assert!(error.starts_with("the invoice XML: BT-24 'urn:example:other' names no Factur-X profile"), "{error}");
    let with_bom = format!("\u{FEFF}{}", en16931_with("", ""));
    assert!(extract(with_bom.as_bytes(), None).is_ok());
}

#[test]
fn an_invoice_with_a_dtd_or_too_many_nodes_is_refused() {
    let xml = en16931_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>", "<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY a \"b\">]>");
    let error = extract(xml.as_bytes(), None).unwrap_err();
    assert!(error.starts_with("the invoice XML has a DTD"), "{error}");
    let options = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: 10 };
    let error = roxmltree::Document::parse_with_options("<a><b/><b/><b/><b/><b/><b/><b/><b/><b/><b/></a>", options).unwrap_err();
    assert!(matches!(error, roxmltree::Error::NodesLimitReached), "the limit roxmltree enforces: {error}");
    let many = format!("<a>{}</a>", "<b/>".repeat(MAX_NODES as usize));
    let error = parse_xml(&many, "the invoice XML").unwrap_err();
    assert_eq!(error, format!("the invoice XML has more than {MAX_NODES} XML nodes, the most the engine reads in an invoice or a map"));
}

#[test]
fn a_map_given_takes_over_whole() {
    let map = r#"<cross-industry-invoice-map prefix="rechnung">
      <value name="nummer" path="rsm:ExchangedDocument/ram:ID"/>
    </cross-industry-invoice-map>"#;
    let data = extract(en16931_with("", "").as_bytes(), Some(map)).unwrap()["data"].take();
    assert_eq!(data, serde_json::json!({ "nummer": "2026-0412" }));
}

// ── generatePdf ───────────────────────────────────────────────────────────────

fn document(output: &str, attachments: &str) -> Document {
    crate::parse::parse(&format!(
        "<lpdf version=\"1\"><document>{output}{attachments}<section><layout><text>x</text></layout></section></document></lpdf>"
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

fn invoice_bytes(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(FOLDER).join(name)).unwrap()
}

#[test]
fn generate_pdf_attaches_the_invoice_and_makes_the_file_pdfa_3b_unless_it_sets_3a_or_3u() {
    let mut doc = document("", "");
    assert_eq!(prepare(&mut doc, &invoice_bytes("en16931.xml")), Ok("factur-x.xml"));
    assert_eq!(doc.output.pdfa, Some(PdfA::Level3b));
    assert_eq!(doc.attachments.iter().map(|file| file.name.as_str()).collect::<Vec<_>>(), ["factur-x.xml"]);
    for (level, kept) in [("3a", PdfA::Level3a), ("3u", PdfA::Level3u)] {
        let mut doc = document(&format!("<output pdfa=\"{level}\"/>"), "");
        prepare(&mut doc, &invoice_bytes("en16931.xml")).unwrap();
        assert_eq!(doc.output.pdfa, Some(kept));
    }
    let mut doc = document("", "<attachments><file name=\"notes.txt\"/></attachments>");
    assert_eq!(prepare(&mut doc, &invoice_bytes("xrechnung-3.0.xml")), Ok("xrechnung.xml"));
    assert_eq!(doc.attachments.len(), 2);
}

#[test]
fn generate_pdf_refuses_what_a_factur_x_invoice_cant_be() {
    for (output, attachments, invoice, says) in [
        ("<output pdfa=\"2b\"/>", "", "en16931.xml", "<output pdfa=\"2b\">: generatePdf makes a Factur-X invoice, which is a PDF/A-3 file"),
        ("<output pdfa=\"2b\"/>", "<attachments><file name=\"notes.txt\"/></attachments>", "en16931.xml",
         "<output pdfa=\"2b\">: generatePdf makes a Factur-X invoice, which is a PDF/A-3 file"),
        ("<output pdfa=\"3b\"/>", "<attachments><file name=\"xrechnung.xml\"/></attachments>", "en16931.xml",
         "the document declares the file 'xrechnung.xml' in its attachments, and generatePdf attaches the invoice itself"),
        ("<output encrypt=\"aes-256\"/>", "", "en16931.xml", "<output encrypt>: generatePdf makes the file PDF/A-3, which forbids encryption"),
        ("", "<attachments><file name=\"factur-x.xml\"/></attachments>", "en16931.xml",
         "the document declares the file 'factur-x.xml' in its attachments, and generatePdf attaches the invoice itself"),
        ("", "<attachments><file name=\"notes.xml\" ref=\"factur-x.xml\"/></attachments>", "en16931.xml",
         "the file 'notes.xml' has the ref 'factur-x.xml', the name generatePdf gives the invoice"),
        ("", "", "minimum.xml", "the invoice's profile is MINIMUM"),
        ("", "", "basic-wl.xml", "the invoice's profile is BASIC WL"),
    ] {
        let xml = format!(
            "<lpdf version=\"1\"><document>{output}{attachments}<section><layout><text>x</text></layout></section></document></lpdf>"
        );
        let resources = crate::parse::Resources { invoice: true, ..Default::default() };
        let mut doc = match crate::parse::parse_with(&xml, &resources) {
            Ok(doc) => doc,
            Err(e) => panic!("{output}{attachments}: {e}"),
        };
        let error = prepare(&mut doc, &invoice_bytes(invoice)).unwrap_err();
        assert!(error.starts_with(says), "{output}{attachments} {invoice}: {error}");
    }
}

/// A template whose text binds `path` with `attribute`, checked against the default map.
fn template_check(attribute: &str, path: &str) -> Result<(), String> {
    let doc = crate::parse::parse(&format!(
        "<lpdf version=\"1\"><document><section><layout><column {attribute}=\"{path}\"><text data-value=\"x\">x</text></column></layout></section></document></lpdf>"
    ))
    .unwrap();
    template::check(&doc, &invoice_map(None).unwrap())
}

#[test]
fn the_template_paths_under_the_prefix_are_checked_against_the_map() {
    assert_eq!(template_check("data-if", "invoice:seller.name"), Ok(()));
    assert_eq!(template_check("data-source", "invoice:lines"), Ok(()));
    assert_eq!(template_check("data-if", "invoice:lines[0].name"), Ok(()));
    assert_eq!(template_check("data-if", "note"), Ok(()), "a path without a prefix is the caller's");
    for (attribute, path, says) in [
        ("data-if", "invoice:sellr.name", "data-if=\"invoice:sellr.name\": 'sellr' is not in the map: the invoice map has number, typeCode, issued"),
        ("data-if", "invoice:seller.nmae", "data-if=\"invoice:seller.nmae\": 'nmae' is not in the map: 'seller' has name, street"),
        ("data-if", "rechnung:seller.name", "data-if=\"rechnung:seller.name\": 'rechnung:' is not the prefix of the invoice map, which is 'invoice:'"),
        ("data-source", "invoice:seller", "data-source=\"invoice:seller\": 'seller' is a group, and data-source repeats an element over a list"),
        ("data-if", "invoice:lines.name", "data-if=\"invoice:lines.name\": 'lines' is a list: repeat an element over it"),
        ("data-if", "invoice:number.x", "data-if=\"invoice:number.x\": 'number' is a value, which holds no 'x'"),
        ("data-if", "invoice:number[0]", "'number' is not a list, and [ ] picks an item of a list"),
        ("data-if", "invoice:lines[x]", "'lines[x]' picks an item of a list with a number in [ ]"),
    ] {
        let error = template_check(attribute, path).unwrap_err();
        assert!(error.starts_with(&format!("{attribute}=\"{path}\": ")) && error.contains(says), "{path}: {error}");
    }
}

#[test]
fn a_bound_value_must_be_a_value_of_the_map() {
    let doc = crate::parse::parse(
        "<lpdf version=\"1\"><document><section><layout><text data-value=\"invoice:seller\">x</text></layout></section></document></lpdf>",
    )
    .unwrap();
    let error = template::check(&doc, &invoice_map(None).unwrap()).unwrap_err();
    assert!(error.starts_with("data-value=\"invoice:seller\": 'seller' is a group, and data-value reads a value, such as invoice:seller.name"), "{error}");
}
