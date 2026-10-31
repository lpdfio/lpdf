//! Files a document carries inside the PDF: the `<attachments>` of `<document>`, one `<file>` each.
//!
//! The document says what each file is called, its type, how it relates to the document and a line about it; the
//! bytes are loaded apart, as an image's are, under the file's `ref` or its name. Each file is written as an
//! embedded file stream with a file specification, listed in the catalog's `EmbeddedFiles` name tree, which a reader
//! shows in its attachments panel, and in its `/AF`, the associated files PDF/A-3 and PDF 2.0 read (PDF 1.7, 7.11.3,
//! 7.11.4 and 7.7.4).

use std::collections::{HashMap, HashSet};

use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::types::AssociationKind;
use pdf_writer::writers::Catalog;
use pdf_writer::{Filter, Name, Pdf, Primitive, Ref};

use crate::pdfa::PdfA;

/// The type of a file whose `mime` is not given: any file.
const DEFAULT_MIME: &str = "application/octet-stream";

/// How a file relates to the document: `relationship`, written as the file's `AFRelationship`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Relationship {
    /// The file the document was made from.
    Source,
    /// The data the pages show, such as the XML of an e-invoice.
    Data,
    /// The same content in another form.
    Alternative,
    /// More than the pages show.
    Supplement,
    #[default]
    Unspecified,
}

impl Relationship {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "source" => Ok(Self::Source),
            "data" => Ok(Self::Data),
            "alternative" => Ok(Self::Alternative),
            "supplement" => Ok(Self::Supplement),
            "unspecified" => Ok(Self::Unspecified),
            other => Err(format!(
                "relationship '{other}' is not one of source, data, alternative, supplement and unspecified"
            )),
        }
    }

    fn kind(self) -> AssociationKind {
        match self {
            Self::Source => AssociationKind::Source,
            Self::Data => AssociationKind::Data,
            Self::Alternative => AssociationKind::Alternative,
            Self::Supplement => AssociationKind::Supplement,
            Self::Unspecified => AssociationKind::Unspecified,
        }
    }
}

/// A `<file>` of `<attachments>`: what the document says about a file it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    /// The name a reader shows the file under and saves it as.
    pub name:         String,
    /// What the bytes were loaded under: the `ref`, or the name.
    pub key:          String,
    pub mime:         String,
    pub relationship: Relationship,
    pub description:  Option<String>,
}

/// Reads one file from its attributes, which `attribute` gives by name. `src` is the SDK's: the engine never reads it.
fn read_file<'a>(attribute: impl Fn(&str) -> Result<Option<&'a str>, String>) -> Result<Attachment, String> {
    let name = attribute("name")?.ok_or("every file needs a name, the one a reader shows it under")?;
    if name.is_empty() {
        return Err("a file's name is empty: give it the name a reader shows it under, such as invoice.xml".to_string());
    }
    let in_file = |e: String| format!("file '{name}': {e}");
    let mime = match attribute("mime")? {
        Some(mime) if is_mime_type(mime) => mime,
        Some(mime) => {
            return Err(in_file(format!(
                "mime '{mime}' is not a type and a subtype such as text/xml or application/pdf: write one, or leave mime \
                out for {DEFAULT_MIME}"
            )));
        }
        None => DEFAULT_MIME,
    };
    let relationship = attribute("relationship")?.map(Relationship::parse).transpose().map_err(in_file)?.unwrap_or_default();
    Ok(Attachment {
        name:         name.to_string(),
        key:          attribute("ref")?.unwrap_or(name).to_string(),
        mime:         mime.to_string(),
        relationship,
        description:  attribute("description")?.map(str::to_string),
    })
}

/// A media type and a subtype, in the letters PDF/A-3 accepts in an embedded file's `Subtype` (veraPDF rule 6.8-1),
/// as the schema's `MimeType` has them.
fn is_mime_type(value: &str) -> bool {
    let part = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.+-".contains(&b));
    value.split_once('/').is_some_and(|(kind, subtype)| part(kind) && part(subtype))
}

/// Refuses two files with the same name: a reader lists the files by name, and the name tree holds each name once.
fn check_names(files: &[Attachment]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for file in files {
        if !seen.insert(file.name.as_str()) {
            return Err(format!("two files are named '{}': give each file a name of its own", file.name));
        }
    }
    Ok(())
}

/// The files of an `<attachments>` element.
pub(crate) fn from_xml(element: &roxmltree::Node) -> Result<Vec<Attachment>, String> {
    let mut files = Vec::new();
    for child in element.children().filter(|n| n.is_element()) {
        let at = || crate::parse::node_loc(&child);
        if child.tag_name().name() != "file" {
            return Err(format!("<{}>{}: unknown element in <attachments>: each file is a <file>", child.tag_name().name(), at()));
        }
        files.push(read_file(|name| Ok(child.attribute(name))).map_err(|e| format!("<file>{}: {e}", at()))?);
    }
    check_names(&files).map_err(|e| format!("<attachments>: {e}"))?;
    Ok(files)
}

/// The files of the kit tree's `attachments`, a list of objects with the attributes of `<file>`. Only a string is
/// read: a value given another way is an error, never one that is not there.
pub(crate) fn from_tree(value: &serde_json::Value) -> Result<Vec<Attachment>, String> {
    let list = value.as_array().ok_or("document attachments must be a list of files")?;
    let mut files = Vec::new();
    for (index, entry) in list.iter().enumerate() {
        let object = entry.as_object().ok_or_else(|| format!("document attachments[{index}] must be an object"))?;
        let attribute = |name: &str| match object.get(name) {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(s)) => Ok(Some(s.as_str())),
            Some(other) => Err(format!("{name} must be a string, not {other}")),
        };
        files.push(read_file(attribute).map_err(|e| format!("document attachments[{index}]: {e}"))?);
    }
    check_names(&files).map_err(|e| format!("document attachments: {e}"))?;
    Ok(files)
}

/// Refuses attachments in a PDF/A-2 document: the standard lets it carry only files that are PDF/A themselves
/// (veraPDF rule 6.8-5), which the engine can't check.
pub(crate) fn check_pdfa(files: &[Attachment], pdfa: Option<PdfA>) -> Result<(), String> {
    match (files.first(), pdfa) {
        (Some(file), Some(level)) => Err(format!(
            "<output pdfa=\"{}\">: the file '{}' can't be in a PDF/A-2 file, which carries only files that are PDF/A \
            themselves, and the engine can't check that: remove the attachments, or remove pdfa",
            level.attribute(), file.name
        )),
        _ => Ok(()),
    }
}

/// The bytes loaded for the files of documents, by the key they were loaded under.
#[derive(Debug, Clone, Default)]
pub struct FileRegistry {
    files: HashMap<String, Vec<u8>>,
}

impl FileRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&mut self, key: &str, bytes: Vec<u8>) {
        self.files.insert(key.to_string(), bytes);
    }

    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.files.get(key).map(Vec::as_slice)
    }
}

/// A file of the document with the bytes loaded for it.
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedFile<'a> {
    pub attachment: &'a Attachment,
    pub bytes:      &'a [u8],
}

/// Each file of `files` with the bytes `registry` has for it, in document order, or an error that names the first
/// file that has none and says how to give it some.
pub fn resolve<'a>(files: &'a [Attachment], registry: &'a FileRegistry) -> Result<Vec<EmbeddedFile<'a>>, String> {
    files
        .iter()
        .map(|attachment| match registry.get(&attachment.key) {
            Some(bytes) if i32::try_from(bytes.len()).is_ok() => Ok(EmbeddedFile { attachment, bytes }),
            Some(bytes) => Err(format!(
                "file '{}' of <attachments> is {} bytes, more than the 2 GB a PDF can say a file holds",
                attachment.name, bytes.len()
            )),
            None => Err(format!(
                "file '{}' of <attachments> has no bytes: give it a src the SDK reads, or load the file on the engine \
                with loadFile('{}', bytes) before the render",
                attachment.name, attachment.key
            )),
        })
        .collect()
}

// ── Writing ───────────────────────────────────────────────────────────────────

/// A string written as hex, `<…>`. The strings of a file are written so because the RC4-128 pass encrypts a
/// string in place: a literal string's ciphertext can hold a parenthesis or a backslash that ends or escapes it,
/// where a hex string's stays hex digits.
#[derive(Clone, Copy)]
struct HexStr<'a>(&'a [u8]);

impl Primitive for HexStr<'_> {
    fn write(self, buf: &mut Vec<u8>) {
        const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
        buf.reserve(2 + 2 * self.0.len());
        buf.push(b'<');
        for &byte in self.0 {
            buf.push(DIGITS[usize::from(byte >> 4)]);
            buf.push(DIGITS[usize::from(byte & 0x0f)]);
        }
        buf.push(b'>');
    }
}

/// The objects of a file: its embedded file stream and its file specification.
pub(crate) struct FileIds {
    stream: Ref,
    spec:   Ref,
}

/// Ids for the objects of each file, from `next`.
pub(crate) fn allocate(files: &[EmbeddedFile], mut next: impl FnMut() -> Ref) -> Vec<FileIds> {
    files.iter().map(|_| FileIds { stream: next(), spec: next() }).collect()
}

/// Text as a PDF text string: as it is when it is printable ASCII, which PDFDocEncoding shares, and UTF-16BE after a
/// byte-order mark otherwise, as `TextStr` writes it. The keys of a name tree are sorted by these bytes.
fn text_string_bytes(text: &str) -> Vec<u8> {
    if text.bytes().all(|b| matches!(b, 32..=126)) {
        return text.as_bytes().to_vec();
    }
    let mut bytes = vec![0xfe, 0xff];
    bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    bytes
}

/// The catalog's entries for the files: the `EmbeddedFiles` name tree, by name in the order of their bytes as the
/// tree requires, and `/AF`, in document order.
pub(crate) fn write_catalog(catalog: &mut Catalog, files: &[EmbeddedFile], ids: &[FileIds]) {
    if files.is_empty() {
        return;
    }
    let mut keyed: Vec<(Vec<u8>, Ref)> =
        files.iter().zip(ids).map(|(file, id)| (text_string_bytes(&file.attachment.name), id.spec)).collect();
    keyed.sort();
    {
        let mut names = catalog.names();
        let mut tree = names.insert(Name(b"EmbeddedFiles")).dict();
        let mut entries = tree.insert(Name(b"Names")).array();
        for (key, spec) in &keyed {
            entries.item(HexStr(key)).item(*spec);
        }
    }
    catalog.insert(Name(b"AF")).array().items(ids.iter().map(|id| id.spec));
}

/// Writes each file's embedded file stream, compressed, with its type, its size and, when the file has a date,
/// `mod_date` as its `ModDate`, and its file specification: `F` and `UF` with its name, `Desc` with its description,
/// and its `AFRelationship`.
pub(crate) fn write_objects(pdf: &mut Pdf, files: &[EmbeddedFile], ids: &[FileIds], mod_date: Option<&str>) {
    for (file, id) in files.iter().zip(ids) {
        let attachment = file.attachment;
        let compressed = compress_to_vec_zlib(file.bytes, 6);
        {
            let mut stream = pdf.embedded_file(id.stream, &compressed);
            stream.filter(Filter::FlateDecode);
            stream.subtype(Name(attachment.mime.as_bytes()));
            let mut params = stream.params();
            // `resolve` refused a file whose size doesn't fit.
            params.size(i32::try_from(file.bytes.len()).unwrap_or(i32::MAX));
            if let Some(date) = mod_date {
                params.pair(Name(b"ModDate"), HexStr(date.as_bytes()));
            }
        }
        let mut spec = pdf.file_spec(id.spec);
        spec.pair(Name(b"F"), HexStr(attachment.name.as_bytes()));
        spec.pair(Name(b"UF"), HexStr(&text_string_bytes(&attachment.name)));
        if let Some(description) = &attachment.description {
            spec.pair(Name(b"Desc"), HexStr(&text_string_bytes(description)));
        }
        spec.embedded_file_with_unicode(id.stream);
        spec.association_kind(attachment.relationship.kind());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xml_files(inner: &str) -> Result<Vec<Attachment>, String> {
        let xml = format!("<attachments>{inner}</attachments>");
        let doc = roxmltree::Document::parse(&xml).expect("test XML");
        from_xml(&doc.root_element())
    }

    #[test]
    fn a_file_takes_the_defaults_of_the_schema() {
        let files = xml_files(r#"<file name="terms.txt"/>"#).unwrap();
        assert_eq!(files, vec![Attachment {
            name:         "terms.txt".to_string(),
            key:          "terms.txt".to_string(),
            mime:         "application/octet-stream".to_string(),
            relationship: Relationship::Unspecified,
            description:  None,
        }]);
    }

    #[test]
    fn a_file_is_loaded_under_its_ref_when_it_has_one() {
        let files = xml_files(r#"<file name="factur-x.xml" ref="invoice-42" mime="text/xml" relationship="data"/>"#).unwrap();
        assert_eq!(files[0].key, "invoice-42");
        assert_eq!(files[0].relationship, Relationship::Data);
    }

    #[test]
    fn an_unknown_relationship_is_an_error_that_lists_the_values() {
        let error = xml_files(r#"<file name="a.xml" relationship="parent"/>"#).unwrap_err();
        assert!(error.contains("relationship 'parent'") && error.contains("source, data, alternative"), "{error}");
        assert!(error.contains("file 'a.xml'"), "{error}");
    }

    #[test]
    fn two_files_with_the_same_name_are_an_error() {
        let error = xml_files(r#"<file name="a.xml"/><file name="a.xml" ref="other"/>"#).unwrap_err();
        assert!(error.contains("two files are named 'a.xml'"), "{error}");
    }

    #[test]
    fn a_mime_that_is_not_a_type_and_a_subtype_is_an_error() {
        for mime in ["xml", "text/", "text/xml; charset=utf-8"] {
            let error = xml_files(&format!(r#"<file name="a.xml" mime="{mime}"/>"#)).unwrap_err();
            assert!(error.contains(&format!("mime '{mime}'")), "{error}");
        }
        assert!(xml_files(r#"<file name="a" mime="application/vnd.ms-excel"/>"#).is_ok());
    }

    #[test]
    fn a_file_without_a_name_is_an_error() {
        assert!(xml_files(r#"<file src="a.xml"/>"#).unwrap_err().contains("needs a name"));
        assert!(xml_files(r#"<file name=""/>"#).unwrap_err().contains("name is empty"));
    }

    #[test]
    fn the_tree_reads_the_same_files_and_refuses_a_value_that_is_not_a_string() {
        let tree = serde_json::json!([{ "name": "a.xml", "mime": "text/xml", "relationship": "data", "description": "A" }]);
        assert_eq!(from_tree(&tree).unwrap(), xml_files(r#"<file name="a.xml" mime="text/xml" relationship="data" description="A"/>"#).unwrap());

        let error = from_tree(&serde_json::json!([{ "name": "a.xml", "relationship": 3 }])).unwrap_err();
        assert!(error.contains("attachments[0]") && error.contains("relationship must be a string"), "{error}");
        assert!(from_tree(&serde_json::json!({ "name": "a.xml" })).unwrap_err().contains("must be a list"));
    }

    #[test]
    fn a_file_with_no_bytes_is_an_error_that_names_the_key_to_load() {
        let files = xml_files(r#"<file name="factur-x.xml" ref="invoice-42"/>"#).unwrap();
        let error = resolve(&files, &FileRegistry::new()).unwrap_err();
        assert!(error.contains("file 'factur-x.xml'") && error.contains("loadFile('invoice-42', bytes)"), "{error}");

        let mut registry = FileRegistry::new();
        registry.load("invoice-42", b"<invoice/>".to_vec());
        assert_eq!(resolve(&files, &registry).unwrap()[0].bytes, b"<invoice/>");
    }

    #[test]
    fn attachments_in_a_pdfa_2_document_are_an_error() {
        let files = xml_files(r#"<file name="a.xml"/>"#).unwrap();
        let error = check_pdfa(&files, Some(PdfA::parse("2b").unwrap())).unwrap_err();
        assert!(error.contains("pdfa=\"2b\"") && error.contains("remove the attachments, or remove pdfa"), "{error}");
        assert!(check_pdfa(&files, None).is_ok());
        assert!(check_pdfa(&[], Some(PdfA::parse("2u").unwrap())).is_ok());
    }

    #[test]
    fn a_hex_string_is_written_in_hex_digits() {
        let mut buf = Vec::new();
        HexStr(b"a)\\").write(&mut buf);
        assert_eq!(buf, b"<61295C>");
    }

    #[test]
    fn a_name_that_is_not_printable_ascii_is_written_as_utf_16() {
        assert_eq!(text_string_bytes("a.xml"), b"a.xml");
        assert_eq!(text_string_bytes("é"), vec![0xfe, 0xff, 0x00, 0xe9]);
    }
}
