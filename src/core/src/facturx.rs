//! Factur-X and ZUGFeRD e-invoices (Factur-X 1.09.2 and ZUGFeRD 2.5.2, one joint spec): a PDF/A-3 file whose pages are
//! the invoice people read, and which carries the invoice's XML, which accounting software reads, as `factur-x.xml`,
//! or `xrechnung.xml` for the XRECHNUNG profile.
//!
//! The file's name is the switch: in a `pdfa` 3 document, an attachment named so is the invoice. The engine reads the
//! profile from BT-24 of its XML, names it in the XMP packet with the Factur-X extension schema (`pdfa::xmp_packet`),
//! and gives the file the type and relationship the spec asks when the document leaves them out. What the document
//! writes is written as given. The XML is carried as it is: its content is the caller's to check, with the spec's XSD
//! and Schematron or a checker such as the Mustang CLI. `facturx-strict` on `<output>` refuses the types and
//! relationships the spec doesn't allow.

use crate::attachments::{Attachment, EmbeddedFile, Relationship};
use crate::parse::Output;

/// The name of the invoice's XML for every profile but XRECHNUNG.
const FACTUR_X: &str = "factur-x.xml";

/// The name of the invoice's XML for the XRECHNUNG profile.
const XRECHNUNG: &str = "xrechnung.xml";

/// The type the spec gives the invoice's XML (p. 26), which a file the document gives no `mime` gets.
const INVOICE_MIME: &str = "text/xml";

/// The namespace of the extension schema's properties, with the `#` the spec's XMP ends it with (p. 30).
pub(crate) const NAMESPACE: &str = "urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#";

/// The namespaces of a CII invoice that BT-24 is read in.
const RSM: &str = "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100";
const RAM: &str = "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100";

/// The types of the formats the spec lists for the files a Factur-X invoice carries beside its XML (p. 32): PDF, TXT,
/// GIF, TIFF, JPG, CSV, XML, JSON, XLS, XLSX and ODS, as IANA registers them. `facturx-strict` refuses any other.
const OTHER_FILE_TYPES: [&str; 12] = [
    "application/pdf",
    "text/plain",
    "image/gif",
    "image/tiff",
    "image/jpeg",
    "text/csv",
    "text/xml",
    "application/xml",
    "application/json",
    "application/vnd.ms-excel",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.oasis.opendocument.spreadsheet",
];

/// A Factur-X profile, which BT-24 of the invoice's XML names: how much of the invoice the XML holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profile {
    Minimum,
    BasicWl,
    Basic,
    En16931,
    Extended,
    /// XRECHNUNG, the profile of invoices to German public bodies, with the version of the XRechnung standard BT-24
    /// names, such as `3.0`.
    XRechnung(String),
}

impl Profile {
    /// The profile a BT-24 value names (spec p. 51), with the spaces around it trimmed, as the spec's own examples
    /// have them. A value that ends in `xrechnung_<version>` is XRECHNUNG, whatever stands before it: the spec only
    /// points to the XRechnung standard for it, whose version changes.
    fn from_guideline(value: &str) -> Option<Self> {
        match value.trim() {
            "urn:factur-x.eu:1p0:minimum" => Some(Self::Minimum),
            "urn:factur-x.eu:1p0:basicwl" => Some(Self::BasicWl),
            "urn:cen.eu:en16931:2017#compliant#urn:factur-x.eu:1p0:basic" => Some(Self::Basic),
            "urn:cen.eu:en16931:2017" => Some(Self::En16931),
            "urn:cen.eu:en16931:2017#conformant#urn:factur-x.eu:1p0:extended" => Some(Self::Extended),
            other => xrechnung_version(other).map(|version| Self::XRechnung(version.to_string())),
        }
    }

    /// The profile as `fx:ConformanceLevel` names it (p. 30).
    pub(crate) fn conformance_level(&self) -> &'static str {
        match self {
            Self::Minimum => "MINIMUM",
            Self::BasicWl => "BASIC WL",
            Self::Basic => "BASIC",
            Self::En16931 => "EN 16931",
            Self::Extended => "EXTENDED",
            Self::XRechnung(_) => "XRECHNUNG",
        }
    }

    /// `fx:Version`: `1.0`, the version of the Factur-X schema, and for XRECHNUNG the XRechnung version of BT-24. The
    /// spec's text asks `2p1` for XRECHNUNG (p. 71), but its samples write the XRechnung version, and the Mustang CLI
    /// accepts `3.0` and refuses `2p1`.
    pub(crate) fn version(&self) -> &str {
        match self {
            Self::XRechnung(version) => version,
            _ => "1.0",
        }
    }

    /// The relationship of the invoice's XML when the document leaves it out: data for MINIMUM and BASIC WL, whose
    /// pages hold more than the XML, and alternative for the others, the one both France and Germany allow (p. 28).
    fn default_relationship(&self) -> Relationship {
        match self {
            Self::Minimum | Self::BasicWl => Relationship::Data,
            _ => Relationship::Alternative,
        }
    }

    /// The relationships the spec allows the invoice's XML (p. 28), in France or in Germany: the engine doesn't know
    /// the country, so it allows what either allows.
    fn allowed_relationships(&self) -> &'static [Relationship] {
        match self {
            Self::Minimum | Self::BasicWl => &[Relationship::Data],
            Self::Basic | Self::En16931 | Self::Extended => {
                &[Relationship::Alternative, Relationship::Source, Relationship::Data]
            }
            Self::XRechnung(_) => &[Relationship::Alternative],
        }
    }
}

/// The version at the end of an XRechnung BT-24 value, after its last `xrechnung_`: numbers with dots between them,
/// such as `3.0`.
fn xrechnung_version(value: &str) -> Option<&str> {
    const MARK: &str = "xrechnung_";
    let version = &value[value.rfind(MARK)? + MARK.len()..];
    let is_number = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    version.split('.').all(is_number).then_some(version)
}

/// Whether a file of this name is the invoice's XML.
fn is_invoice_name(name: &str) -> bool {
    name == FACTUR_X || name == XRECHNUNG
}

/// The invoice of a Factur-X document: the name of the file that holds its XML, and the profile the XML names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Invoice<'a> {
    pub name:    &'a str,
    pub profile: &'a Profile,
}

/// The invoice among the files of a render, which `apply` found.
pub(crate) fn invoice<'a>(files: &'a [EmbeddedFile]) -> Option<Invoice<'a>> {
    files.iter().find_map(|file| {
        file.invoice.as_ref().map(|profile| Invoice { name: file.attachment.name.as_str(), profile })
    })
}

/// Refuses, when the document is read, an invoice file the engine can't write: in a document that isn't PDF/A-3,
/// which the spec asks for (p. 25), and beside another file that could be the invoice, since the XMP packet names one.
/// Two files with one name were refused before (`attachments::check_names`).
pub(crate) fn check_document(files: &[Attachment], output: &Output) -> Result<(), String> {
    let mut invoices = files.iter().filter(|file| is_invoice_name(&file.name));
    let Some(invoice) = invoices.next() else { return Ok(()) };
    let switch = format!(
        "the file '{}' makes the document a Factur-X invoice, which is a PDF/A-3 file",
        invoice.name
    );
    match output.pdfa {
        Some(level) if level.part() == 3 => {}
        Some(level) => {
            return Err(format!(
                "<output pdfa=\"{}\">: {switch}: write pdfa=\"3a\", pdfa=\"3b\" or pdfa=\"3u\", or give the file \
                another name if it is not the invoice",
                level.attribute()
            ));
        }
        None => {
            return Err(format!(
                "{switch}, and <output> has no pdfa: add pdfa=\"3a\", pdfa=\"3b\" or pdfa=\"3u\" to <output>, or give \
                the file another name if it is not the invoice"
            ));
        }
    }
    if let Some(other) = invoices.next() {
        return Err(format!(
            "the files '{}' and '{}' could each be the invoice of a Factur-X document, whose metadata names one: keep \
            {FACTUR_X} for the profiles of Factur-X, or {XRECHNUNG} for XRECHNUNG, and give the other file another name",
            invoice.name, other.name
        ));
    }
    Ok(())
}

/// In a PDF/A-3 document, reads the profile of the invoice file, gives it the type and the relationship the spec asks
/// when the document leaves them out, and marks it as the invoice; with `facturx-strict`, refuses what the spec doesn't
/// allow (`check_strict`). A document without an invoice file is left as it is.
pub(crate) fn apply(files: &mut [EmbeddedFile], output: &Output) -> Result<(), String> {
    // `check_document` refused an invoice file in any other document.
    if !output.pdfa.is_some_and(|level| level.part() == 3) {
        return Ok(());
    }
    let Some(invoice) = files.iter_mut().find(|file| is_invoice_name(&file.attachment.name)) else { return Ok(()) };
    let profile = read_profile(invoice.bytes, &invoice.attachment.name)?;
    if invoice.attachment.mime.is_none() {
        invoice.mime = INVOICE_MIME;
    }
    if invoice.attachment.relationship.is_none() {
        invoice.relationship = profile.default_relationship();
    }
    invoice.invoice = Some(profile);
    if output.facturx_strict {
        check_strict(files)?;
    }
    Ok(())
}

/// The profile BT-24 of the invoice's XML names: `rsm:ExchangedDocumentContext` /
/// `ram:GuidelineSpecifiedDocumentContextParameter` / `ram:ID` of a CII invoice, matched by namespace, whatever the
/// prefixes. XML that does not parse, and a BT-24 that is missing or names no profile, are errors that say what to
/// change. Nothing else in the XML is read.
fn read_profile(bytes: &[u8], name: &str) -> Result<Profile, String> {
    let in_file = |reason: String| format!("file '{name}', the invoice of a Factur-X document: {reason}");
    let text = std::str::from_utf8(bytes)
        .map_err(|_| in_file("its XML is not UTF-8, the encoding the engine reads it in: save it as UTF-8".to_string()))?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let document = roxmltree::Document::parse(text).map_err(|e| {
        in_file(format!("its XML does not parse ({e}): give the invoice's CII XML, or give the file another name"))
    })?;
    let is = |node: &roxmltree::Node, namespace: &str, local: &str| {
        node.is_element() && node.tag_name().namespace() == Some(namespace) && node.tag_name().name() == local
    };
    let root = document.root_element();
    let guideline = is(&root, RSM, "CrossIndustryInvoice")
        .then(|| root.children().find(|n| is(n, RSM, "ExchangedDocumentContext")))
        .flatten()
        .and_then(|context| context.children().find(|n| is(n, RAM, "GuidelineSpecifiedDocumentContextParameter")))
        .and_then(|parameter| parameter.children().find(|n| is(n, RAM, "ID")));
    let Some(guideline) = guideline else {
        return Err(in_file(format!(
            "its XML names no profile. A Factur-X invoice's XML is a CII invoice, rsm:CrossIndustryInvoice, that names \
            its profile in rsm:ExchangedDocumentContext / ram:GuidelineSpecifiedDocumentContextParameter / ram:ID \
            (BT-24), such as urn:cen.eu:en16931:2017 for EN 16931: add it. {PROFILES}"
        )));
    };
    let value = guideline.text().unwrap_or("");
    Profile::from_guideline(value).ok_or_else(|| {
        in_file(format!("BT-24 '{}' names no Factur-X profile. {PROFILES}", value.trim()))
    })
}

/// The BT-24 values of the profiles, for the errors of `read_profile`.
const PROFILES: &str = "The profiles are urn:factur-x.eu:1p0:minimum (MINIMUM), urn:factur-x.eu:1p0:basicwl (BASIC WL), \
    urn:cen.eu:en16931:2017#compliant#urn:factur-x.eu:1p0:basic (BASIC), urn:cen.eu:en16931:2017 (EN 16931), \
    urn:cen.eu:en16931:2017#conformant#urn:factur-x.eu:1p0:extended (EXTENDED), and a value that ends in \
    xrechnung_<version>, such as urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0 (XRECHNUNG)";

/// With `facturx-strict`, refuses what the spec doesn't allow: an invoice whose type isn't `text/xml` (p. 26) or whose
/// relationship its profile doesn't allow (p. 28), and another file whose type isn't one of a format the spec lists
/// (p. 32), or that has no type. A type is compared without regard to case, as MIME types are.
fn check_strict(files: &[EmbeddedFile]) -> Result<(), String> {
    let refuse = |reason: String| Err(format!("<output facturx-strict=\"true\">: {reason}, or remove facturx-strict"));
    for file in files {
        let name = &file.attachment.name;
        let Some(profile) = &file.invoice else {
            let Some(mime) = &file.attachment.mime else {
                return refuse(format!(
                    "the file '{name}' has no mime, and a Factur-X invoice carries only files of the formats its spec \
                    lists: give it the type of one ({})",
                    OTHER_FILE_TYPES.join(", ")
                ));
            };
            if !OTHER_FILE_TYPES.iter().any(|allowed| mime.eq_ignore_ascii_case(allowed)) {
                return refuse(format!(
                    "the file '{name}' has mime '{mime}', and a Factur-X invoice carries only files of the formats its \
                    spec lists: PDF, TXT, GIF, TIFF, JPG, CSV, XML, JSON, XLS, XLSX and ODS ({}). Convert the file to \
                    one of them, or leave it out",
                    OTHER_FILE_TYPES.join(", ")
                ));
            }
            continue;
        };
        if !file.mime.eq_ignore_ascii_case(INVOICE_MIME) {
            return refuse(format!(
                "the invoice '{name}' has mime '{}', and the spec types it {INVOICE_MIME}: write mime=\"{INVOICE_MIME}\" \
                or leave mime out",
                file.mime
            ));
        }
        let allowed = profile.allowed_relationships();
        if !allowed.contains(&file.relationship) {
            let names: Vec<&str> = allowed.iter().map(|relationship| relationship.attribute()).collect();
            return refuse(format!(
                "the invoice '{name}' has relationship '{}', and the profile {} allows {}: write one of them or leave \
                relationship out",
                file.relationship.attribute(),
                profile.conformance_level(),
                names.join(" or ")
            ));
        }
    }
    Ok(())
}

/// The description of the Factur-X properties for PDF/A, which predefines only its own schemas and those of XMP: the
/// spec's example block (`2. FACTUR-X_extension_schema_example.xmp.txt` of its package), an entry of the bag of
/// `pdfaExtension:schemas` (`pdfa::xmp_packet`).
pub(crate) const EXTENSION_SCHEMA: &str = "<rdf:li rdf:parseType=\"Resource\">\n\
      <pdfaSchema:schema>Factur-X PDFA Extension Schema</pdfaSchema:schema>\n\
      <pdfaSchema:namespaceURI>urn:factur-x:pdfa:CrossIndustryDocument:invoice:1p0#</pdfaSchema:namespaceURI>\n\
      <pdfaSchema:prefix>fx</pdfaSchema:prefix>\n\
      <pdfaSchema:property>\n\
       <rdf:Seq>\n\
        <rdf:li rdf:parseType=\"Resource\">\n\
         <pdfaProperty:name>DocumentFileName</pdfaProperty:name>\n\
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>\n\
         <pdfaProperty:category>external</pdfaProperty:category>\n\
         <pdfaProperty:description>The name of the embedded XML document</pdfaProperty:description>\n\
        </rdf:li>\n\
        <rdf:li rdf:parseType=\"Resource\">\n\
         <pdfaProperty:name>DocumentType</pdfaProperty:name>\n\
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>\n\
         <pdfaProperty:category>external</pdfaProperty:category>\n\
         <pdfaProperty:description>The type of the hybrid document in capital letters, e.g. INVOICE or ORDER</pdfaProperty:description>\n\
        </rdf:li>\n\
        <rdf:li rdf:parseType=\"Resource\">\n\
         <pdfaProperty:name>Version</pdfaProperty:name>\n\
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>\n\
         <pdfaProperty:category>external</pdfaProperty:category>\n\
         <pdfaProperty:description>The actual version of the standard applying to the embedded XML document</pdfaProperty:description>\n\
        </rdf:li>\n\
        <rdf:li rdf:parseType=\"Resource\">\n\
         <pdfaProperty:name>ConformanceLevel</pdfaProperty:name>\n\
         <pdfaProperty:valueType>Text</pdfaProperty:valueType>\n\
         <pdfaProperty:category>external</pdfaProperty:category>\n\
         <pdfaProperty:description>The conformance level of the embedded XML document</pdfaProperty:description>\n\
        </rdf:li>\n\
       </rdf:Seq>\n\
      </pdfaSchema:property>\n\
     </rdf:li>\n";

/// The four Factur-X properties of the XMP packet (p. 30), each as `(name, value)`: the document's type, the name of
/// the invoice's file, which equals its file specification's `F`, the version and the profile.
pub(crate) fn properties<'a>(invoice: Invoice<'a>) -> [(&'static str, &'a str); 4] {
    [
        ("fx:DocumentType", "INVOICE"),
        ("fx:DocumentFileName", invoice.name),
        ("fx:Version", invoice.profile.version()),
        ("fx:ConformanceLevel", invoice.profile.conformance_level()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A CII invoice whose BT-24 is `guideline`, with the namespace prefixes `rsm` and `ram`, as the spec's samples
    /// write them, unless `ram` says otherwise.
    fn invoice_xml(guideline: &str, ram: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rsm:CrossIndustryInvoice xmlns:rsm="{RSM}" xmlns:{ram}="{RAM}">
  <rsm:ExchangedDocumentContext>
    <{ram}:GuidelineSpecifiedDocumentContextParameter>
      <{ram}:ID>{guideline}</{ram}:ID>
    </{ram}:GuidelineSpecifiedDocumentContextParameter>
  </rsm:ExchangedDocumentContext>
</rsm:CrossIndustryInvoice>"#
        )
    }

    fn profile(guideline: &str) -> Result<Profile, String> {
        read_profile(invoice_xml(guideline, "ram").as_bytes(), FACTUR_X)
    }

    #[test]
    fn each_profile_is_read_from_its_bt24_value() {
        for (value, expected, level) in [
            ("urn:factur-x.eu:1p0:minimum", Profile::Minimum, "MINIMUM"),
            ("urn:factur-x.eu:1p0:basicwl", Profile::BasicWl, "BASIC WL"),
            ("urn:cen.eu:en16931:2017#compliant#urn:factur-x.eu:1p0:basic", Profile::Basic, "BASIC"),
            ("urn:cen.eu:en16931:2017", Profile::En16931, "EN 16931"),
            ("urn:cen.eu:en16931:2017#conformant#urn:factur-x.eu:1p0:extended", Profile::Extended, "EXTENDED"),
        ] {
            let read = profile(value).unwrap_or_else(|e| panic!("{value}: {e}"));
            assert_eq!(read, expected, "{value}");
            assert_eq!(read.conformance_level(), level, "{value}");
            assert_eq!(read.version(), "1.0", "{value}");
        }
    }

    #[test]
    fn an_xrechnung_value_gives_its_version_whatever_stands_before_it() {
        for (value, version) in [
            ("urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0", "3.0"),
            ("urn:cen.eu:en16931:2017#compliant#urn:xoev-de:kosit:standard:xrechnung_2.1", "2.1"),
            ("urn:cen.eu:en16931:2017#compliant#urn:xeinkauf.de:kosit:xrechnung_3.0#conformant#urn:xeinkauf.de:kosit:extension:xrechnung_3.0", "3.0"),
        ] {
            let read = profile(value).unwrap_or_else(|e| panic!("{value}: {e}"));
            assert_eq!(read, Profile::XRechnung(version.to_string()), "{value}");
            assert_eq!((read.conformance_level(), read.version()), ("XRECHNUNG", version), "{value}");
        }
        for value in ["urn:xeinkauf.de:kosit:xrechnung_", "urn:xeinkauf.de:kosit:xrechnung_3.", "urn:x:xrechnung_three"] {
            assert_eq!(Profile::from_guideline(value), None, "{value}");
        }
    }

    #[test]
    fn bt24_is_read_with_spaces_around_it_and_any_prefix() {
        assert_eq!(profile("\n        urn:cen.eu:en16931:2017  \n      "), Ok(Profile::En16931));
        let other_prefix = invoice_xml("urn:factur-x.eu:1p0:minimum", "a");
        assert_eq!(read_profile(other_prefix.as_bytes(), FACTUR_X), Ok(Profile::Minimum));
        let bom = format!("\u{FEFF}{}", invoice_xml("urn:cen.eu:en16931:2017", "ram"));
        assert_eq!(read_profile(bom.as_bytes(), FACTUR_X), Ok(Profile::En16931));
    }

    #[test]
    fn a_bt24_in_another_namespace_is_not_read() {
        let xml = invoice_xml("urn:cen.eu:en16931:2017", "ram")
            .replace(&format!("xmlns:ram=\"{RAM}\""), "xmlns:ram=\"urn:example:not-ram\"");
        let error = read_profile(xml.as_bytes(), FACTUR_X).unwrap_err();
        assert!(error.contains("names no profile") && error.contains("(BT-24)"), "{error}");
    }

    #[test]
    fn a_missing_or_unknown_bt24_is_an_error_that_lists_the_profiles() {
        let missing = invoice_xml("x", "ram").replace("<ram:ID>x</ram:ID>", "");
        let error = read_profile(missing.as_bytes(), FACTUR_X).unwrap_err();
        assert!(error.starts_with("file 'factur-x.xml'") && error.contains("names no profile"), "{error}");
        assert!(error.contains("urn:factur-x.eu:1p0:basicwl (BASIC WL)"), "{error}");

        let error = profile("urn:factur-x.eu:1p0:comfort").unwrap_err();
        assert!(error.contains("BT-24 'urn:factur-x.eu:1p0:comfort' names no Factur-X profile"), "{error}");
        assert!(error.contains("xrechnung_<version>"), "{error}");

        let ubl = r#"<Invoice xmlns="urn:oasis:names:specification:ubl:schema:xsd:Invoice-2"><ID>1</ID></Invoice>"#;
        assert!(read_profile(ubl.as_bytes(), FACTUR_X).unwrap_err().contains("names no profile"));
    }

    #[test]
    fn xml_that_does_not_parse_is_an_error_that_names_the_file() {
        for bytes in [b"<rsm:CrossIndustryInvoice>".as_slice(), b"not xml", b""] {
            let error = read_profile(bytes, XRECHNUNG).unwrap_err();
            assert!(error.starts_with("file 'xrechnung.xml'") && error.contains("does not parse"), "{error}");
        }
        let error = read_profile(&[0x3c, 0xff, 0xfe], FACTUR_X).unwrap_err();
        assert!(error.contains("not UTF-8"), "{error}");
    }

    #[test]
    fn the_default_relationship_is_data_for_the_two_smallest_profiles_and_alternative_for_the_rest() {
        assert_eq!(Profile::Minimum.default_relationship(), Relationship::Data);
        assert_eq!(Profile::BasicWl.default_relationship(), Relationship::Data);
        for profile in [Profile::Basic, Profile::En16931, Profile::Extended, Profile::XRechnung("3.0".into())] {
            assert_eq!(profile.default_relationship(), Relationship::Alternative, "{profile:?}");
            assert!(profile.allowed_relationships().contains(&Relationship::Alternative), "{profile:?}");
        }
    }
}
