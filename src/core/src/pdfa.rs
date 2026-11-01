//! PDF/A (ISO 19005), which `<output pdfa>` asks for: the level, the date the file carries, the XMP packet that
//! names both, and the sRGB profile of the output intent. What a PDF/A file refuses is checked in `pdf.rs`, where
//! the fonts, the colours and the fields are known.

use crate::parse::{Meta, Output};
use crate::pdfua::PdfUa;
use crate::pdfx::PdfXVersion;

/// A level of PDF/A, as `<output pdfa>` writes it. Part 2 is the first that allows the opacity, shadows and
/// gradients the engine draws. Level b keeps the look of the pages; level u adds that every character maps to
/// Unicode, which every embedded font of the engine does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfA {
    Level2b,
    Level2u,
}

impl PdfA {
    /// The level of a `pdfa` attribute: `2b` or `2u`.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "2b" => Ok(PdfA::Level2b),
            "2u" => Ok(PdfA::Level2u),
            other => Err(format!("pdfa '{other}' is not a PDF/A level the engine writes: use 2b or 2u")),
        }
    }

    /// The value of the `pdfa` attribute, for messages: `2b`.
    pub fn attribute(self) -> &'static str {
        match self {
            PdfA::Level2b => "2b",
            PdfA::Level2u => "2u",
        }
    }

    /// The part of ISO 19005, as `pdfaid:part` writes it.
    fn part(self) -> u8 {
        2
    }

    /// The conformance level, as `pdfaid:conformance` writes it.
    fn conformance(self) -> &'static str {
        match self {
            PdfA::Level2b => "B",
            PdfA::Level2u => "U",
        }
    }
}

/// The sRGB profile of the output intent, which gives the RGB colours and images a meaning (`assets/icc/README.md`).
pub(crate) const SRGB_PROFILE: &[u8] = include_bytes!("../assets/icc/sRGB-v2-magic.icc");

/// What the output intent calls the profile: the colour space's registered name.
pub(crate) const SRGB_CONDITION: &str = "sRGB IEC61966-2.1";

/// The producer the Info dictionary and the XMP packet both name.
pub(crate) const PRODUCER: &str = "Lpdf (lpdf.io)";

/// The date a file is made, from the render's `created_on`, written with the precision and the time zone it was
/// given in: a PDF/A file's `CreationDate` and `ModDate` in the Info dictionary and the dates of its XMP packet, and a
/// plain PDF's `CreationDate` and the `ModDate` of each file it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileDate {
    year:  u16,
    month: u8,
    day:   u8,
    /// Hour, minute and second; `None` for a date alone.
    time:  Option<(u8, u8, u8)>,
    zone:  Option<Zone>,
}

/// A time zone: UTC, or an offset from it in minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    Utc,
    Offset(i16),
}

impl FileDate {
    /// Read an ISO 8601 date, or date and time: `2026-10-31`, `2026-10-31T09:30`, `2026-10-31T09:30:00`, with an
    /// optional fraction of a second, which is dropped, and an optional `Z` or `+01:00`. A space may stand for the
    /// `T`, as Python's `str()` of a datetime writes it.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let (date, rest) = value.split_at_checked(10)?;
        let mut parts = date.split('-');
        let year: u16 = number(parts.next()?, 4)?;
        let month: u8 = number(parts.next()?, 2)?;
        let day: u8 = number(parts.next()?, 2)?;
        if parts.next().is_some() || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        if rest.is_empty() {
            return Some(FileDate { year, month, day, time: None, zone: None });
        }
        let rest = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
        let zone_at = rest.find(['Z', '+', '-']).unwrap_or(rest.len());
        let (clock, zone) = rest.split_at(zone_at);
        let clock = clock.split_once('.').map_or(clock, |(whole, fraction)| {
            if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) { "" } else { whole }
        });
        let mut fields = clock.split(':');
        let hour: u8 = number(fields.next()?, 2)?;
        let minute: u8 = number(fields.next()?, 2)?;
        let second: u8 = match fields.next() {
            Some(s) => number(s, 2)?,
            None => 0,
        };
        if fields.next().is_some() || hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        let zone = match zone {
            "" => None,
            "Z" => Some(Zone::Utc),
            offset => {
                let sign: i16 = if offset.starts_with('-') { -1 } else { 1 };
                let (hours, minutes) = offset[1..].split_once(':')?;
                let hours: u8 = number(hours, 2)?;
                let minutes: u8 = number(minutes, 2)?;
                if hours > 23 || minutes > 59 {
                    return None;
                }
                Some(Zone::Offset(sign * (hours as i16 * 60 + minutes as i16)))
            }
        };
        Some(FileDate { year, month, day, time: Some((hour, minute, second)), zone })
    }

    /// As a PDF date (ISO 32000-1, 7.9.4): `D:20261031093000+01'00`.
    pub(crate) fn pdf(&self) -> String {
        let mut out = format!("D:{:04}{:02}{:02}", self.year, self.month, self.day);
        if let Some((hour, minute, second)) = self.time {
            out.push_str(&format!("{hour:02}{minute:02}{second:02}"));
            match self.zone {
                Some(Zone::Utc) => out.push('Z'),
                Some(Zone::Offset(minutes)) => {
                    let sign = if minutes < 0 { '-' } else { '+' };
                    let minutes = minutes.unsigned_abs();
                    out.push_str(&format!("{sign}{:02}'{:02}", minutes / 60, minutes % 60));
                }
                None => {}
            }
        }
        out
    }

    /// As an XMP date: `2026-10-31T09:30:00+01:00`.
    fn xmp(&self) -> String {
        let mut out = format!("{:04}-{:02}-{:02}", self.year, self.month, self.day);
        if let Some((hour, minute, second)) = self.time {
            out.push_str(&format!("T{hour:02}:{minute:02}:{second:02}"));
            match self.zone {
                Some(Zone::Utc) => out.push('Z'),
                Some(Zone::Offset(minutes)) => {
                    let sign = if minutes < 0 { '-' } else { '+' };
                    let minutes = minutes.unsigned_abs();
                    out.push_str(&format!("{sign}{:02}:{:02}", minutes / 60, minutes % 60));
                }
                None => {}
            }
        }
        out
    }
}

/// A number of exactly `digits` ASCII digits.
fn number<T: std::str::FromStr>(text: &str, digits: usize) -> Option<T> {
    if text.len() != digits || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The standards a document's `<output>` asks for: PDF/A, PDF/UA, PDF/X, or PDF/UA with one of the others. A file that
/// meets one carries its date and an XMP packet that names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Standards {
    pub pdfa:  Option<PdfA>,
    pub pdfua: Option<PdfUa>,
    pub pdfx:  Option<PdfXVersion>,
}

impl Standards {
    pub(crate) fn of(output: &Output) -> Self {
        Standards { pdfa: output.pdfa, pdfua: output.pdfua, pdfx: output.pdfx.as_ref().map(|pdfx| pdfx.version) }
    }

    /// Whether the document asks for any standard.
    pub(crate) fn any(self) -> bool {
        self.pdfa.is_some() || self.pdfua.is_some() || self.pdfx.is_some()
    }

    /// The attributes of `<output>` that ask for them, for messages: `pdfa="2b" pdfua="1"`.
    fn attributes(self) -> String {
        let pdfa = self.pdfa.map(|level| format!("pdfa=\"{}\"", level.attribute()));
        let pdfua = self.pdfua.map(|part| format!("pdfua=\"{}\"", part.attribute()));
        let pdfx = self.pdfx.map(|version| format!("pdfx=\"{}\"", version.attribute()));
        [pdfa, pdfua, pdfx].into_iter().flatten().collect::<Vec<_>>().join(" ")
    }

    /// What a file that meets them is called, for messages: `PDF/A`, `PDF/UA`, or `PDF/A and PDF/UA`.
    fn names(self) -> String {
        let names = [(self.pdfa.is_some(), "PDF/A"), (self.pdfua.is_some(), "PDF/UA"), (self.pdfx.is_some(), "PDF/X")];
        names.iter().filter(|(asked, _)| *asked).map(|(_, name)| *name).collect::<Vec<_>>().join(" and ")
    }
}

/// The XMP media management ids a PDF/X file carries (`xmpMM:DocumentID` and `xmpMM:InstanceID`), as `uuid:` URIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XmpIds {
    pub document: String,
    pub instance: String,
}

/// The date of a render of a document that asks for a standard. The engine has no clock, so the render must give
/// `created_on`: missing or not a date, it is an error that says so.
pub(crate) fn file_date(standards: Standards, created_on: Option<&str>) -> Result<FileDate, String> {
    let (attributes, names) = (standards.attributes(), standards.names());
    let Some(value) = created_on else {
        return Err(format!(
            "<output {attributes}> needs the date the file is made, and the render got no created_on: a {names} \
            file carries its date, and the engine has no clock. Pass the date with the render, as createdOn, \
            CreatedOn or created_on in the SDK's render options, such as 2026-10-31T09:30:00Z"
        ));
    };
    FileDate::parse(value).ok_or_else(|| format!(
        "<output {attributes}>: created_on '{value}' is not a date a {names} file can carry: give an ISO 8601 date, \
        or date and time, such as 2026-10-31 or 2026-10-31T09:30:00Z"
    ))
}

/// The date of a render of a plain PDF, read as a PDF/A file's is: `None` without `created_on`, which leaves the file
/// undated, so the same input gives the same bytes, and an error that names the value when it is not a date.
pub(crate) fn plain_date(created_on: Option<&str>) -> Result<Option<FileDate>, String> {
    created_on
        .map(|value| FileDate::parse(value).ok_or_else(|| format!(
            "created_on '{value}' is not a date a PDF file can carry: give an ISO 8601 date, or date and time, such \
            as 2026-10-31 or 2026-10-31T09:30:00Z"
        )))
        .transpose()
}

/// Check before the layout that the render's `created_on` is a date, and that a render of a document that asks for a
/// standard has one, so that it fails without the work of a render.
pub(crate) fn check_render(output: &Output, created_on: Option<&str>) -> Result<(), String> {
    let standards = Standards::of(output);
    if standards.any() {
        file_date(standards, created_on).map(|_| ())
    } else {
        plain_date(created_on).map(|_| ())
    }
}

/// The XMP packet of a PDF/A, PDF/UA or PDF/X file: the part and level of each standard, and the same title, author,
/// subject, keywords, creator, producer and dates as the Info dictionary. UTF-8, with no `bytes` or `encoding` in its
/// header, which PDF/A forbids. PDF/A-2 accepts only the properties of the schemas it predefines, so a file that is
/// PDF/A and PDF/UA describes the PDF/UA identification schema in an extension schema (ISO 19005-2, 6.6.2.3). A PDF/X
/// file adds its version, that it is not trapped, as the Info dictionary says, and the media management `ids`.
pub(crate) fn xmp_packet(standards: Standards, meta: &Meta, date: &FileDate, ids: Option<&XmpIds>) -> Vec<u8> {
    let mut properties = String::new();
    let mut add = |name: &str, value: String| properties.push_str(&format!("   <{name}>{value}</{name}>\n"));
    if let Some(pdfa) = standards.pdfa {
        add("pdfaid:part", pdfa.part().to_string());
        add("pdfaid:conformance", pdfa.conformance().to_string());
    }
    if let Some(pdfua) = standards.pdfua {
        add("pdfuaid:part", pdfua.part().to_string());
    }
    if let Some(pdfx) = standards.pdfx {
        add("pdfxid:GTS_PDFXVersion", pdfx.name().to_string());
        add("pdf:Trapped", "False".to_string());
    }
    if let Some(ids) = ids {
        add("xmpMM:DocumentID", ids.document.clone());
        add("xmpMM:InstanceID", ids.instance.clone());
        add("xmpMM:VersionID", "1".to_string());
        add("xmpMM:RenditionClass", "default".to_string());
    }
    add("dc:format", "application/pdf".to_string());
    let alt = |text: &str| format!("<rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt>", xml_text(text));
    if !meta.title.is_empty() {
        add("dc:title", alt(&meta.title));
    }
    if !meta.author.is_empty() {
        add("dc:creator", format!("<rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq>", xml_text(&meta.author)));
    }
    if !meta.subject.is_empty() {
        add("dc:description", alt(&meta.subject));
    }
    if !meta.keywords.is_empty() {
        add("pdf:Keywords", xml_text(&meta.keywords));
    }
    add("pdf:Producer", xml_text(PRODUCER));
    if !meta.creator.is_empty() {
        add("xmp:CreatorTool", xml_text(&meta.creator));
    }
    let when = date.xmp();
    add("xmp:CreateDate", when.clone());
    add("xmp:ModifyDate", when.clone());
    add("xmp:MetadataDate", when);

    let mut namespaces = String::new();
    if standards.pdfa.is_some() {
        namespaces.push_str("    xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"\n");
    }
    if standards.pdfua.is_some() {
        namespaces.push_str("    xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\"\n");
    }
    if standards.pdfx.is_some() {
        namespaces.push_str("    xmlns:pdfxid=\"http://www.npes.org/pdfx/ns/id/\"\n");
    }
    if ids.is_some() {
        namespaces.push_str("    xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\"\n");
    }
    let extension = if standards.pdfa.is_some() && standards.pdfua.is_some() { PDFUA_EXTENSION_SCHEMA } else { "" };

    format!(
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
        <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
        \x20<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
        {extension}\
        \x20 <rdf:Description rdf:about=\"\"\n\
        {namespaces}\
        \x20   xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n\
        \x20   xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\"\n\
        \x20   xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n\
        {properties}\
        \x20 </rdf:Description>\n\
        \x20</rdf:RDF>\n\
        </x:xmpmeta>\n\
        <?xpacket end=\"w\"?>"
    )
    .into_bytes()
}

/// The PDF/UA identification schema described for PDF/A, which predefines only its own schemas and those of XMP:
/// its namespace, its prefix and its one property, `part`.
const PDFUA_EXTENSION_SCHEMA: &str = "  <rdf:Description rdf:about=\"\"\n\
    xmlns:pdfaExtension=\"http://www.aiim.org/pdfa/ns/extension/\"\n\
    xmlns:pdfaSchema=\"http://www.aiim.org/pdfa/ns/schema#\"\n\
    xmlns:pdfaProperty=\"http://www.aiim.org/pdfa/ns/property#\">\n\
   <pdfaExtension:schemas>\n\
    <rdf:Bag>\n\
     <rdf:li rdf:parseType=\"Resource\">\n\
      <pdfaSchema:schema>PDF/UA Universal Accessibility Schema</pdfaSchema:schema>\n\
      <pdfaSchema:namespaceURI>http://www.aiim.org/pdfua/ns/id/</pdfaSchema:namespaceURI>\n\
      <pdfaSchema:prefix>pdfuaid</pdfaSchema:prefix>\n\
      <pdfaSchema:property>\n\
       <rdf:Seq>\n\
        <rdf:li rdf:parseType=\"Resource\">\n\
         <pdfaProperty:name>part</pdfaProperty:name>\n\
         <pdfaProperty:valueType>Integer</pdfaProperty:valueType>\n\
         <pdfaProperty:category>internal</pdfaProperty:category>\n\
         <pdfaProperty:description>The part of ISO 14289 the file conforms to</pdfaProperty:description>\n\
        </rdf:li>\n\
       </rdf:Seq>\n\
      </pdfaSchema:property>\n\
     </rdf:li>\n\
    </rdf:Bag>\n\
   </pdfaExtension:schemas>\n\
  </rdf:Description>\n";

/// `text` as XML character data: the markup characters escaped, and the characters XML 1.0 cannot hold at all,
/// such as most control characters, left out.
fn xml_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> FileDate {
        FileDate::parse(value).unwrap_or_else(|| panic!("'{value}' parses"))
    }

    #[test]
    fn a_level_is_read_from_its_attribute_value() {
        assert_eq!(PdfA::parse("2b"), Ok(PdfA::Level2b));
        assert_eq!(PdfA::parse("2u"), Ok(PdfA::Level2u));
        for other in ["2a", "1b", "3b", "2B", ""] {
            let error = PdfA::parse(other).unwrap_err();
            assert!(error.contains("use 2b or 2u"), "{error}");
        }
    }

    #[test]
    fn dates_are_written_in_the_precision_and_zone_they_were_given() {
        let cases = [
            ("2026-10-31", "D:20261031", "2026-10-31"),
            ("2026-10-31T09:30", "D:20261031093000", "2026-10-31T09:30:00"),
            ("2026-10-31T09:30:15Z", "D:20261031093015Z", "2026-10-31T09:30:15Z"),
            ("2026-10-31T09:30:15.123Z", "D:20261031093015Z", "2026-10-31T09:30:15Z"),
            ("2026-10-31T09:30:15.1234567+01:00", "D:20261031093015+01'00", "2026-10-31T09:30:15+01:00"),
            ("2026-10-31 09:30:15-05:30", "D:20261031093015-05'30", "2026-10-31T09:30:15-05:30"),
            ("2024-02-29T00:00:00", "D:20240229000000", "2024-02-29T00:00:00"),
        ];
        for (value, pdf, xmp) in cases {
            assert_eq!(date(value).pdf(), pdf, "{value}");
            assert_eq!(date(value).xmp(), xmp, "{value}");
        }
    }

    #[test]
    fn what_is_not_a_date_does_not_parse() {
        for value in [
            "", "2026", "2026-10", "31.10.2026", "2026-13-01", "2026-02-29", "2026-04-31", "2026-10-31T",
            "2026-10-31T24:00", "2026-10-31T09:60", "2026-10-31T9:30", "2026-10-31T09:30:15.", "2026-10-31T09:30+1",
            "2026-10-31T09:30+25:00", "2026-10-31X09:30", "now", "2026-10-31T09:30:15ZZ",
        ] {
            assert_eq!(FileDate::parse(value), None, "{value}");
        }
    }

    fn pdfa(level: PdfA) -> Standards {
        Standards { pdfa: Some(level), ..Standards::default() }
    }

    #[test]
    fn a_missing_or_malformed_created_on_is_an_error_that_names_it() {
        let missing = file_date(pdfa(PdfA::Level2b), None).unwrap_err();
        assert!(missing.contains("pdfa=\"2b\"") && missing.contains("created_on") && missing.contains("createdOn"), "{missing}");
        let malformed = file_date(pdfa(PdfA::Level2u), Some("yesterday")).unwrap_err();
        assert!(malformed.contains("'yesterday'") && malformed.contains("2026-10-31T09:30:00Z"), "{malformed}");
        assert!(check_render(&Output::default(), None).is_ok(), "no standard, no date needed");
    }

    #[test]
    fn a_pdfua_render_needs_created_on_too_and_the_error_names_each_standard() {
        let ua = Standards { pdfua: Some(PdfUa::Part1), ..Standards::default() };
        let missing = file_date(ua, None).unwrap_err();
        assert!(missing.starts_with("<output pdfua=\"1\">") && missing.contains("a PDF/UA file carries its date"), "{missing}");
        let both = Standards { pdfa: Some(PdfA::Level2b), pdfua: Some(PdfUa::Part1), pdfx: None };
        let missing = file_date(both, None).unwrap_err();
        assert!(missing.starts_with("<output pdfa=\"2b\" pdfua=\"1\">") && missing.contains("PDF/A and PDF/UA"), "{missing}");
    }

    #[test]
    fn a_plain_render_reads_its_date_as_a_pdfa_render_does() {
        for (value, pdf) in [
            ("2026-10-31T09:30:00.000Z", "D:20261031093000Z"),
            ("2026-10-31T09:30:00Z", "D:20261031093000Z"),
            ("2026-10-31T09:30:00+01:00", "D:20261031093000+01'00"),
            ("2026-10-31T09:30", "D:20261031093000"),
            ("2026-10-31T09:30:00", "D:20261031093000"),
            ("2026-10-31", "D:20261031"),
        ] {
            let date = plain_date(Some(value)).unwrap_or_else(|e| panic!("{value}: {e}"));
            assert_eq!(date.map(|d| d.pdf()).as_deref(), Some(pdf), "{value}");
            assert!(check_render(&Output::default(), Some(value)).is_ok(), "{value}");
        }
        assert_eq!(plain_date(None), Ok(None), "no date, an undated file");
    }

    #[test]
    fn a_plain_created_on_that_is_not_a_date_is_an_error_that_names_it_and_gives_the_form() {
        for value in ["yesterday", "31.10.2026"] {
            for error in [plain_date(Some(value)).unwrap_err(), check_render(&Output::default(), Some(value)).unwrap_err()] {
                assert!(error.contains(&format!("created_on '{value}'")), "{error}");
                assert!(error.contains("2026-10-31T09:30:00Z") && !error.contains("pdfa"), "{error}");
            }
        }
    }

    #[test]
    fn the_xmp_packet_names_the_level_and_repeats_the_info_dictionary() {
        let meta = Meta {
            title: "Q3 <report> & notes".into(), author: "Ann Lee".into(), subject: "Sales".into(),
            keywords: "sales, q3".into(), creator: "Billing".into(), lang: "en".into(),
        };
        let xmp = String::from_utf8(xmp_packet(pdfa(PdfA::Level2u), &meta, &date("2026-10-31T09:30:00Z"), None)).unwrap();
        for expected in [
            "<pdfaid:part>2</pdfaid:part>", "<pdfaid:conformance>U</pdfaid:conformance>",
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Q3 &lt;report&gt; &amp; notes</rdf:li></rdf:Alt></dc:title>",
            "<dc:creator><rdf:Seq><rdf:li>Ann Lee</rdf:li></rdf:Seq></dc:creator>",
            "<dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">Sales</rdf:li></rdf:Alt></dc:description>",
            "<pdf:Keywords>sales, q3</pdf:Keywords>", "<pdf:Producer>Lpdf (lpdf.io)</pdf:Producer>",
            "<xmp:CreatorTool>Billing</xmp:CreatorTool>", "<xmp:CreateDate>2026-10-31T09:30:00Z</xmp:CreateDate>",
            "<xmp:ModifyDate>2026-10-31T09:30:00Z</xmp:ModifyDate>",
        ] {
            assert!(xmp.contains(expected), "{expected}\n{xmp}");
        }
        assert!(xmp.starts_with("<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>"), "{xmp}");
        assert!(!xmp.contains("bytes=") && !xmp.contains("encoding="), "{xmp}");
        assert!(xmp.ends_with("<?xpacket end=\"w\"?>"), "{xmp}");
    }

    #[test]
    fn the_xmp_packet_leaves_out_what_the_meta_does_not_set() {
        let xmp = String::from_utf8(xmp_packet(pdfa(PdfA::Level2b), &Meta::default(), &date("2026-10-31"), None)).unwrap();
        assert!(xmp.contains("<pdfaid:conformance>B</pdfaid:conformance>"), "{xmp}");
        for absent in ["dc:title", "dc:creator", "dc:description", "pdf:Keywords", "xmp:CreatorTool"] {
            assert!(!xmp.contains(absent), "{absent}\n{xmp}");
        }
        assert!(xmp.contains("<xmp:CreateDate>2026-10-31</xmp:CreateDate>"), "{xmp}");
    }

    #[test]
    fn a_pdfua_packet_names_its_part_and_describes_the_schema_only_beside_pdfa() {
        let meta = Meta { title: "Invoice".into(), ..Meta::default() };
        let ua = Standards { pdfua: Some(PdfUa::Part1), ..Standards::default() };
        let xmp = String::from_utf8(xmp_packet(ua, &meta, &date("2026-10-31"), None)).unwrap();
        assert!(xmp.contains("<pdfuaid:part>1</pdfuaid:part>") && xmp.contains("xmlns:pdfuaid="), "{xmp}");
        assert!(!xmp.contains("pdfaid:") && !xmp.contains("pdfaExtension"), "{xmp}");
        let both = Standards { pdfa: Some(PdfA::Level2u), pdfua: Some(PdfUa::Part1), pdfx: None };
        let xmp = String::from_utf8(xmp_packet(both, &meta, &date("2026-10-31"), None)).unwrap();
        assert!(xmp.contains("<pdfaid:part>2</pdfaid:part>") && xmp.contains("<pdfuaid:part>1</pdfuaid:part>"), "{xmp}");
        assert!(xmp.contains("<pdfaSchema:prefix>pdfuaid</pdfaSchema:prefix>"), "{xmp}");
    }

    #[test]
    fn a_pdfx_packet_names_its_version_the_trapping_and_the_ids() {
        let meta = Meta { title: "Brochure".into(), ..Meta::default() };
        let x = Standards { pdfx: Some(PdfXVersion::X4), ..Standards::default() };
        let ids = XmpIds { document: "uuid:0-1".into(), instance: "uuid:2-3".into() };
        let xmp = String::from_utf8(xmp_packet(x, &meta, &date("2026-10-31T09:30:00Z"), Some(&ids))).unwrap();
        for expected in [
            "xmlns:pdfxid=\"http://www.npes.org/pdfx/ns/id/\"", "<pdfxid:GTS_PDFXVersion>PDF/X-4</pdfxid:GTS_PDFXVersion>",
            "<pdf:Trapped>False</pdf:Trapped>", "xmlns:xmpMM=\"http://ns.adobe.com/xap/1.0/mm/\"",
            "<xmpMM:DocumentID>uuid:0-1</xmpMM:DocumentID>", "<xmpMM:InstanceID>uuid:2-3</xmpMM:InstanceID>",
            "<xmpMM:VersionID>1</xmpMM:VersionID>", "<xmpMM:RenditionClass>default</xmpMM:RenditionClass>",
            "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Brochure</rdf:li></rdf:Alt></dc:title>",
            "<xmp:CreateDate>2026-10-31T09:30:00Z</xmp:CreateDate>",
        ] {
            assert!(xmp.contains(expected), "{expected}\n{xmp}");
        }
        assert!(!xmp.contains("pdfaid:") && !xmp.contains("pdfaExtension"), "{xmp}");
    }

    #[test]
    fn a_pdfx_render_needs_created_on_and_the_error_names_each_standard() {
        let x = Standards { pdfx: Some(PdfXVersion::X4), ..Standards::default() };
        let missing = file_date(x, None).unwrap_err();
        assert!(missing.starts_with("<output pdfx=\"4\">") && missing.contains("a PDF/X file carries its date"), "{missing}");
        let both = Standards { pdfua: Some(PdfUa::Part1), pdfx: Some(PdfXVersion::X4), ..Standards::default() };
        let missing = file_date(both, None).unwrap_err();
        assert!(missing.starts_with("<output pdfua=\"1\" pdfx=\"4\">") && missing.contains("PDF/UA and PDF/X"), "{missing}");
    }

    #[test]
    fn characters_xml_cannot_hold_are_left_out_of_the_packet() {
        assert_eq!(xml_text("a\u{1}b\tc\u{FFFE}"), "ab\tc");
    }

    #[test]
    fn the_bundled_profile_is_an_srgb_display_profile_of_icc_version_2() {
        assert_eq!(u32::from_be_bytes(SRGB_PROFILE[0..4].try_into().unwrap()) as usize, SRGB_PROFILE.len());
        assert_eq!(SRGB_PROFILE[8], 2, "major version");
        assert_eq!(&SRGB_PROFILE[12..16], b"mntr");
        assert_eq!(&SRGB_PROFILE[16..20], b"RGB ");
        assert_eq!(&SRGB_PROFILE[36..40], b"acsp");
    }
}
