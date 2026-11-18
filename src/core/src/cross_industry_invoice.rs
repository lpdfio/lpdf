//! `LCrossIndustryInvoice`: a Factur-X / ZUGFeRD invoice PDF made from the invoice's XML, an EN 16931 Cross Industry
//! Invoice. The SDKs' three calls are thin over this module:
//!
//! - `extractInvoiceData` reads the invoice's values through a map, into plain data (`extract`);
//! - `generatePdf` binds a template to those values under the map's prefix, beside the caller's data, makes the file
//!   PDF/A-3 and attaches the XML (`prepare`, `bind_template`), or does the last two for a document built in code;
//! - `defaultInvoiceMap` gives Lpdf's map (`DEFAULT_MAP`).
//!
//! The engine never writes the XML and doesn't validate it in full: it checks what reading needs. The map is checked
//! whole against FeRD's EXTENDED schema before any invoice is read (`map.rs`). An invoice must be well-formed, have no
//! DTD and at most `MAX_NODES` nodes, be a `rsm:CrossIndustryInvoice`, and name a profile the engine knows that is not
//! MINIMUM or BASIC WL, whose XML is only a booking aid. Each value the map marks `required` must be there.

// The paths are spelled out because the WASI build includes this file from another crate.
#[path = "cross_industry_invoice/map.rs"]
mod map;
#[path = "cross_industry_invoice/path.rs"]
mod path;
#[path = "cross_industry_invoice/read.rs"]
mod read;
#[path = "cross_industry_invoice/schema.rs"]
mod schema;
#[path = "cross_industry_invoice/template.rs"]
mod template;

use serde_json::{json, Value};

use crate::attachments::Attachment;
use crate::facturx::Profile;
use crate::parse::Document;
use crate::pdfa::PdfA;

pub(crate) use map::InvoiceMap;

/// Lpdf's default map: every value EN 16931 numbers, most common first, under the prefix `invoice`. It applies when
/// a call is given no map.
pub(crate) const DEFAULT_MAP: &str = include_str!("../assets/cross-industry-invoice-map.xml");

/// The most XML nodes the engine reads in an invoice or a map: an invoice of tens of thousands of lines fits.
pub(crate) const MAX_NODES: u32 = 2_000_000;

/// Parses XML that comes from outside: a DTD is refused, since its entities can make a small file expand without end,
/// and so is a file of more than `MAX_NODES` nodes. `what` names the file in the errors: `the invoice XML`.
pub(crate) fn parse_xml<'a>(text: &'a str, what: &str) -> Result<roxmltree::Document<'a>, String> {
    let options = roxmltree::ParsingOptions { allow_dtd: false, nodes_limit: MAX_NODES };
    roxmltree::Document::parse_with_options(text, options).map_err(|error| match error {
        roxmltree::Error::DtdDetected => format!(
            "{what} has a DTD (<!DOCTYPE … [ … ]>), which the engine refuses, since its entities can make a small file \
            expand without end: remove it"
        ),
        roxmltree::Error::NodesLimitReached => {
            format!("{what} has more than {MAX_NODES} XML nodes, the most the engine reads in an invoice or a map")
        }
        error => format!("{what} is not well-formed XML: {error}"),
    })
}

/// The map a call is given, or the default, read and checked.
pub(crate) fn invoice_map(text: Option<&str>) -> Result<InvoiceMap, String> {
    map::parse(text.unwrap_or(DEFAULT_MAP))
}

/// The invoice's XML as text: UTF-8, the encoding of every Factur-X invoice, without a byte order mark.
fn invoice_text(bytes: &[u8]) -> Result<&str, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "the invoice XML is not UTF-8, the encoding of a Factur-X invoice: save it as UTF-8".to_string())?;
    Ok(text.strip_prefix('\u{FEFF}').unwrap_or(text))
}

/// The profile of a parsed invoice, once it is a Cross Industry Invoice with a profile the engine knows that is not
/// MINIMUM or BASIC WL.
fn check_invoice(document: &roxmltree::Document) -> Result<Profile, String> {
    let root = document.root_element();
    let rsm = schema::Namespace::Rsm.uri();
    if root.tag_name().namespace() != Some(rsm) || root.tag_name().name() != "CrossIndustryInvoice" {
        return Err(format!(
            "the invoice XML is a <{}>, not a Cross Industry Invoice: LCrossIndustryInvoice reads the EN 16931 XML a \
            Factur-X or ZUGFeRD invoice carries, whose root is rsm:CrossIndustryInvoice in the namespace {rsm}",
            root.tag_name().name()
        ));
    }
    let profile = crate::facturx::profile_of(document).map_err(|reason| format!("the invoice XML: {reason}"))?;
    if profile.is_booking_aid() {
        return Err(format!(
            "the invoice's profile is {}, whose XML is only a booking aid: the PDF of such an invoice is the legal \
            invoice and shows the lines the XML doesn't hold. LCrossIndustryInvoice makes the PDF from the XML, so it \
            reads BASIC, EN 16931, EXTENDED and XRECHNUNG. For a {} invoice, render its template with its data and \
            attach the XML as factur-x.xml",
            profile.conformance_level(),
            profile.conformance_level()
        ));
    }
    Ok(profile)
}

/// `extractInvoiceData`: the values of `invoice_xml` read through the map given, or the default, as
/// `{ "data": …, "typed": [{ "at": ["lines", 0, "unit"], "type": "code", "name": "lines.unit" }, …] }`. The data is
/// what a template binds; `typed` says where each value of a type is, for the SDK to apply the `format` function of
/// its type to, with its entry's name.
pub(crate) fn extract(invoice_xml: &[u8], map: Option<&str>) -> Result<Value, String> {
    let map = invoice_map(map)?;
    let text = invoice_text(invoice_xml)?;
    let document = parse_xml(text, "the invoice XML")?;
    check_invoice(&document)?;
    crate::memory::check()?;
    let extracted = read::read(document.root_element(), &map.entries)?;
    let typed: Vec<Value> = extracted
        .typed
        .into_iter()
        .map(|typed| json!({ "at": typed.at, "type": typed.ty.name(), "name": typed.name }))
        .collect();
    Ok(json!({ "data": extracted.data, "typed": typed }))
}

/// What `generatePdf` adds to a document, a template or one built in code, once it is read: the invoice file, named
/// for the profile of `invoice_xml`, and PDF/A-3, 3b unless the document sets 3a or 3u. Returns the name, under which
/// the render loads the invoice's bytes.
///
/// Returns an error for an invoice `check_invoice` refuses, a document that declares the invoice file itself or
/// gives another file the invoice's name as its `ref`, a PDF/A level other than 3a, 3b or 3u, and `encrypt` or
/// `pdfx`, which PDF/A-3 doesn't go with.
pub(crate) fn prepare(doc: &mut Document, invoice_xml: &[u8]) -> Result<&'static str, String> {
    let profile = check_invoice(&parse_xml(invoice_text(invoice_xml)?, "the invoice XML")?)?;
    let name = profile.file_name();
    if let Some(file) = doc.attachments.iter().find(|file| crate::facturx::is_invoice_name(&file.name)) {
        return Err(format!(
            "the document declares the file '{}' in its attachments, and generatePdf attaches the invoice itself: remove \
            the file from the attachments",
            file.name
        ));
    }
    if let Some(file) = doc.attachments.iter().find(|file| file.key == name) {
        return Err(format!(
            "the file '{}' has the ref '{name}', the name generatePdf gives the invoice: give it another ref",
            file.name
        ));
    }
    match doc.output.pdfa {
        Some(level) if level.part() == 3 => {}
        Some(level) => {
            return Err(format!(
                "<output pdfa=\"{}\">: generatePdf makes a Factur-X invoice, which is a PDF/A-3 file: write \
                pdfa=\"3a\", pdfa=\"3b\" or pdfa=\"3u\", or leave pdfa out for 3b",
                level.attribute()
            ));
        }
        None if doc.output.encryption.is_some() => {
            return Err("<output encrypt>: generatePdf makes the file PDF/A-3, which forbids encryption: remove \
                encrypt and allow"
                .to_string());
        }
        None if doc.output.pdfx.is_some() => {
            return Err("<output pdfx>: generatePdf makes the file PDF/A-3, which doesn't go with PDF/X: remove pdfx \
                and its profile"
                .to_string());
        }
        None => {
            doc.output.pdfa = Some(PdfA::Level3b);
            doc.output.pdfa_by_invoice = true;
        }
    }
    doc.attachments.push(Attachment {
        name:         name.to_string(),
        key:          name.to_string(),
        mime:         None,
        relationship: None,
        description:  None,
    });
    crate::attachments::check_standards(&doc.attachments, &doc.output)?;
    Ok(name)
}

/// Binds a template for `generatePdf`: checks its paths that have a prefix against the map, then binds `invoice_data`,
/// the values the SDK read with `extract` and formatted, under the map's prefix, beside the caller's `data`.
pub(crate) fn bind_template(
    doc: &mut Document, data: Option<Value>, map: Option<&str>, invoice_data: Value,
) -> Result<(), String> {
    let map = invoice_map(map)?;
    template::check(doc, &map)?;
    let root = data.unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    crate::data::apply_with(doc, root, crate::data::Prefixed::one(&map.prefix, invoice_data))
}

#[cfg(test)]
#[path = "cross_industry_invoice/tests.rs"]
mod tests;
