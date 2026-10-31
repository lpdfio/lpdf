//! PDF/A (ISO 19005), which `<output pdfa>` asks for: the level, the date the file carries, the XMP packet that
//! names both, and the sRGB profile of the output intent. What a PDF/A file refuses is checked in `pdf.rs`, where
//! the fonts, the colours and the fields are known.

use crate::parse::Meta;

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

/// The date a PDF/A file is made, from the render's `created_on`: written as `CreationDate` and `ModDate` in the
/// Info dictionary and as the dates of the XMP packet, with the precision and the time zone it was given in.
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

/// The date of a render of a document that asks for `pdfa`. The engine has no clock, so the render must give
/// `created_on`: missing or not a date, it is an error that says so.
pub(crate) fn file_date(pdfa: PdfA, created_on: Option<&str>) -> Result<FileDate, String> {
    let level = pdfa.attribute();
    let Some(value) = created_on else {
        return Err(format!(
            "<output pdfa=\"{level}\"> needs the date the file is made, and the render got no created_on: a PDF/A \
            file carries its date, and the engine has no clock. Pass the date with the render, as createdOn, \
            CreatedOn or created_on in the SDK's render options, such as 2026-10-31T09:30:00Z"
        ));
    };
    FileDate::parse(value).ok_or_else(|| format!(
        "<output pdfa=\"{level}\">: created_on '{value}' is not a date a PDF/A file can carry: give an ISO 8601 date, \
        or date and time, such as 2026-10-31 or 2026-10-31T09:30:00Z"
    ))
}

/// Check before the layout that a render of a document with `pdfa` has the date it needs, so that it fails without
/// the work of a render. Nothing to check without `pdfa`.
pub(crate) fn check_render(pdfa: Option<PdfA>, created_on: Option<&str>) -> Result<(), String> {
    match pdfa {
        Some(level) => file_date(level, created_on).map(|_| ()),
        None => Ok(()),
    }
}

/// The XMP packet of a PDF/A file: the part and level, and the same title, author, subject, keywords, creator,
/// producer and dates as the Info dictionary. UTF-8, with no `bytes` or `encoding` in its header, which PDF/A
/// forbids.
pub(crate) fn xmp_packet(pdfa: PdfA, meta: &Meta, date: &FileDate) -> Vec<u8> {
    let mut properties = String::new();
    let mut add = |name: &str, value: String| properties.push_str(&format!("   <{name}>{value}</{name}>\n"));
    add("pdfaid:part", pdfa.part().to_string());
    add("pdfaid:conformance", pdfa.conformance().to_string());
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

    format!(
        "<?xpacket begin=\"\u{FEFF}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
        <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
        \x20<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
        \x20 <rdf:Description rdf:about=\"\"\n\
        \x20   xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"\n\
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

    #[test]
    fn a_missing_or_malformed_created_on_is_an_error_that_names_it() {
        let missing = file_date(PdfA::Level2b, None).unwrap_err();
        assert!(missing.contains("pdfa=\"2b\"") && missing.contains("created_on") && missing.contains("createdOn"), "{missing}");
        let malformed = file_date(PdfA::Level2u, Some("yesterday")).unwrap_err();
        assert!(malformed.contains("'yesterday'") && malformed.contains("2026-10-31T09:30:00Z"), "{malformed}");
        assert!(check_render(None, None).is_ok(), "no standard, no date needed");
    }

    #[test]
    fn the_xmp_packet_names_the_level_and_repeats_the_info_dictionary() {
        let meta = Meta {
            title: "Q3 <report> & notes".into(), author: "Ann Lee".into(), subject: "Sales".into(),
            keywords: "sales, q3".into(), creator: "Billing".into(),
        };
        let xmp = String::from_utf8(xmp_packet(PdfA::Level2u, &meta, &date("2026-10-31T09:30:00Z"))).unwrap();
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
        let xmp = String::from_utf8(xmp_packet(PdfA::Level2b, &Meta::default(), &date("2026-10-31"))).unwrap();
        assert!(xmp.contains("<pdfaid:conformance>B</pdfaid:conformance>"), "{xmp}");
        for absent in ["dc:title", "dc:creator", "dc:description", "pdf:Keywords", "xmp:CreatorTool"] {
            assert!(!xmp.contains(absent), "{absent}\n{xmp}");
        }
        assert!(xmp.contains("<xmp:CreateDate>2026-10-31</xmp:CreateDate>"), "{xmp}");
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
