//! PDF/UA (ISO 14289-1), which `<output pdfua>` asks for: a tagged PDF that a screen reader can read. This module
//! holds the part, and what a document must say before it is laid out: its title and language, what each image and
//! barcode shows, and headings that go down one level at a time. The tags themselves come from the layout
//! (`layout::tag_of`) and are written by `pdf.rs` (`tagging`).

use crate::encrypt::Permissions;
use crate::parse::{Meta, Node, NodeKind, Role, SectionLayout};

/// A part of PDF/UA, as `<output pdfua>` writes it. Part 1 is the one the laws and the validators point at; part 2
/// is built on PDF 2.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfUa {
    Part1,
}

impl PdfUa {
    /// The part of a `pdfua` attribute: `1`.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "1" => Ok(PdfUa::Part1),
            other => Err(format!("pdfua '{other}' is not a PDF/UA part the engine writes: use 1")),
        }
    }

    /// The value of the `pdfua` attribute, for messages: `1`.
    pub fn attribute(self) -> &'static str {
        match self {
            PdfUa::Part1 => "1",
        }
    }

    /// The part of ISO 14289, as `pdfuaid:part` writes it.
    pub(crate) fn part(self) -> u8 {
        match self {
            PdfUa::Part1 => 1,
        }
    }
}

/// The start of every PDF/UA error, which names the attribute that asks for it.
fn refusal(part: PdfUa, reason: &str) -> String {
    format!("<output pdfua=\"{}\">: {reason}", part.attribute())
}

/// An encrypted PDF/UA file lets assistive technology read it (ISO 14289-1, 7.16: bit 10 of `/P`), so an `allow`
/// that leaves out `accessibility` is an error. The engine doesn't add it to what the author allowed.
pub(crate) fn check_permissions(part: PdfUa, permissions: Permissions) -> Result<(), String> {
    if permissions.accessibility {
        return Ok(());
    }
    Err(refusal(part, "an encrypted PDF/UA file lets a screen reader read its text, and allow leaves out \
        accessibility: add accessibility to allow (allow=\"print accessibility\")"))
}

/// Check what a PDF/UA document must say before it is laid out: a title, which a reader shows in place of the file
/// name (7.1), the language of its text as a language tag (7.2), an `alt` on every image and barcode of the flow, a
/// first heading of h1 and headings that skip no level on the way down (7.4.2), and no form field, which draws its
/// text in Helvetica, a font that is not embedded (7.21.4.1).
pub(crate) fn check(part: Option<PdfUa>, meta: &Meta, sections: &[SectionLayout]) -> Result<(), String> {
    let Some(part) = part else { return Ok(()) };
    if meta.title.trim().is_empty() {
        return Err(refusal(part, "a PDF/UA file has a title, which a reader shows in place of the file name: add \
            title to <meta>"));
    }
    if meta.lang.is_empty() {
        return Err(refusal(part, "a PDF/UA file says the language of its text, which a screen reader reads it in: \
            add lang to <meta>, such as lang=\"en-US\""));
    }
    if !is_language_tag(&meta.lang) {
        return Err(refusal(part, &format!("lang '{}' is not a language tag: give one such as en, en-US or \
            de-CH", meta.lang)));
    }
    let mut last_level = 0;
    for section in sections {
        for node in &section.children {
            check_node(part, node, &mut last_level)?;
        }
    }
    Ok(())
}

/// Whether `value` is a language tag as PDF reads one (RFC 3066): letters, then parts of letters and digits, each one
/// to eight long, joined by hyphens.
fn is_language_tag(value: &str) -> bool {
    let mut parts = value.split('-');
    let first_is_letters = parts.next().is_some_and(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphabetic()));
    first_is_letters && parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// Check `node` and the nodes below it in reading order. `last_level` is the level of the heading before it, 0 before
/// the first. A header or a footer is decoration, so its images need no `alt` and its headings don't count.
fn check_node(part: PdfUa, node: &Node, last_level: &mut u8) -> Result<(), String> {
    if node.role == Role::Artifact {
        return Ok(());
    }
    match node.kind {
        NodeKind::Img if node.alt().is_none() => {
            let name = node.image_name.as_deref().unwrap_or_default();
            return Err(refusal(part, &format!("the image '{name}' has no alt, and a PDF/UA file says what each \
                image shows: add alt with a description, or alt=\"\" when it is decoration")));
        }
        NodeKind::Barcode if node.alt().is_none() => {
            let data = excerpt(node.barcode_data.as_deref().unwrap_or_default());
            return Err(refusal(part, &format!("the barcode of \"{data}\" has no alt, and a PDF/UA file says what \
                each barcode is for, since its data means nothing read aloud: add alt, such as alt=\"QR code to pay \
                this invoice\", or alt=\"\" when it is decoration")));
        }
        NodeKind::Field => {
            let name = node.field_name.as_deref().map(|n| format!(" '{n}'")).unwrap_or_default();
            return Err(refusal(part, &format!("the form field{name} can't be in a PDF/UA file yet: a field draws \
                its text in Helvetica, which is not embedded, and PDF/UA embeds every font. Remove the field, or \
                remove pdfua")));
        }
        _ => {}
    }
    if let Role::Heading(level) = node.role {
        let words = excerpt(&node.text_runs.iter().map(|run| run.text.as_str()).collect::<Vec<_>>().join(" "));
        if *last_level == 0 && level != 1 {
            return Err(refusal(part, &format!("the first heading, \"{words}\", is h{level}, and the headings of a \
                PDF/UA file start at h1: make it h1")));
        }
        if level > *last_level + 1 {
            return Err(refusal(part, &format!("the heading \"{words}\" is h{level} after an h{last}, and the \
                headings of a PDF/UA file go down one level at a time: make it h{next}", last = *last_level,
                next = *last_level + 1)));
        }
        *last_level = level;
    }
    for child in &node.children {
        check_node(part, child, last_level)?;
    }
    Ok(())
}

/// The start of a text, for an error that names it.
fn excerpt(text: &str) -> String {
    const MAX: usize = 40;
    let text = text.trim();
    match text.char_indices().nth(MAX) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;

    fn check_xml(meta: &str, layout: &str) -> Result<(), String> {
        let xml = format!(
            r#"<lpdf version="1"><assets><image name="logo" src="logo.png"/></assets><document><meta {meta}/><output pdfua="1"/><section><layout>{layout}</layout></section></document></lpdf>"#
        );
        let mut doc = parse(&xml)?;
        let sections = doc.section_layouts();
        check(doc.output.pdfua, &doc.meta, &sections)
    }

    const META: &str = r#"title="Invoice" lang="en-US""#;

    #[test]
    fn a_part_is_read_from_its_attribute_value() {
        assert_eq!(PdfUa::parse("1"), Ok(PdfUa::Part1));
        for other in ["2", "ua1", ""] {
            assert!(PdfUa::parse(other).unwrap_err().contains("use 1"), "{other}");
        }
    }

    #[test]
    fn a_document_with_a_title_a_language_and_described_images_passes() {
        let layout = r#"<text role="h1">Invoice</text><img name="logo" alt="Acme logo"/><img name="logo" alt=""/>
            <barcode type="qr" data="pay" height="40pt" alt="QR code to pay this invoice"/><text role="h2">Fees</text>"#;
        assert_eq!(check_xml(META, layout), Ok(()));
    }

    #[test]
    fn a_missing_title_or_language_is_an_error_that_says_what_to_add() {
        let no_title = check_xml(r#"lang="en""#, "<text>x</text>").unwrap_err();
        assert!(no_title.starts_with("<output pdfua=\"1\">") && no_title.contains("add title to <meta>"), "{no_title}");
        let no_lang = check_xml(r#"title="T""#, "<text>x</text>").unwrap_err();
        assert!(no_lang.contains("add lang to <meta>"), "{no_lang}");
        let bad_lang = check_xml(r#"title="T" lang="english language""#, "<text>x</text>").unwrap_err();
        assert!(bad_lang.contains("'english language' is not a language tag"), "{bad_lang}");
    }

    #[test]
    fn language_tags_are_letters_then_parts_of_letters_and_digits() {
        for tag in ["en", "en-US", "de-CH", "zh-Hant-TW", "es-419", "x-klingon"] {
            assert!(is_language_tag(tag), "{tag}");
        }
        for not_tag in ["", "en_US", "en-", "-en", "123", "en US", "abcdefghi"] {
            assert!(!is_language_tag(not_tag), "{not_tag}");
        }
    }

    #[test]
    fn an_image_or_a_barcode_without_alt_is_an_error_that_names_it() {
        let image = check_xml(META, r#"<img name="logo"/>"#).unwrap_err();
        assert!(image.contains("the image 'logo' has no alt") && image.contains("alt=\"\""), "{image}");
        let barcode = check_xml(META, r#"<barcode type="qr" data="https://pay.example/1" height="40pt"/>"#).unwrap_err();
        assert!(barcode.contains("the barcode of \"https://pay.example/1\" has no alt"), "{barcode}");
    }

    #[test]
    fn an_image_in_a_header_needs_no_alt() {
        let layout = r#"<region pin="top"><img name="logo"/></region><text>Body</text>"#;
        assert_eq!(check_xml(META, layout), Ok(()));
    }

    #[test]
    fn headings_start_at_h1_and_skip_no_level_on_the_way_down() {
        let first = check_xml(META, r#"<text role="h2">Fees</text>"#).unwrap_err();
        assert!(first.contains("the first heading, \"Fees\", is h2") && first.contains("make it h1"), "{first}");
        let skipped = check_xml(META, r#"<text role="h1">A</text><text role="h3">Fees</text>"#).unwrap_err();
        assert!(skipped.contains("\"Fees\" is h3 after an h1") && skipped.contains("make it h2"), "{skipped}");
        let back_up = r#"<text role="h1">A</text><text role="h2">B</text><text role="h3">C</text><text role="h2">D</text><text role="h1">E</text>"#;
        assert_eq!(check_xml(META, back_up), Ok(()), "a heading may go back up any number of levels");
    }

    #[test]
    fn a_form_field_is_an_error_until_fields_have_an_embedded_font() {
        let error = check_xml(META, r#"<field type="text" name="email"/>"#).unwrap_err();
        assert!(error.contains("the form field 'email'") && error.contains("Helvetica"), "{error}");
    }

    #[test]
    fn an_encrypted_file_must_let_a_screen_reader_read_it() {
        let error = check_permissions(PdfUa::Part1, Permissions::NONE).unwrap_err();
        assert!(error.contains("add accessibility to allow"), "{error}");
        assert_eq!(check_permissions(PdfUa::Part1, Permissions::ALL), Ok(()));
    }
}
