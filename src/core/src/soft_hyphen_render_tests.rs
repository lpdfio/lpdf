//! Soft hyphens in a written PDF: the hyphen a line broken at one ends in, marked as the soft hyphen it stands for, in
//! a built-in font and in an embedded one, in a file that is tagged and in one that is not, and the PDF/A file that a
//! soft hyphen in a font without a glyph for it no longer stops. The WASI crate has its own `LpdfEngine`, so these run
//! with the core library only.

use std::path::PathBuf;

use crate::pdfx_render_tests::flate_streams;
use crate::{LpdfEngine, RenderEncryption};

/// How the marked content that a broken line's hyphen is drawn in begins.
const HYPHEN_SPAN: &str = "/Span <<\n  /ActualText <FEFF00AD>\n>> BDC\n";

fn font(file: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/assets/fonts").join(file);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// An unlicensed engine with Crimson Text, which has no glyph for the soft hyphen, loaded as `body`.
fn engine() -> LpdfEngine {
    let mut engine = LpdfEngine::new("");
    engine.load_font("body", &font("CrimsonText-Regular.ttf"));
    engine.set_created_on("2026-11-12T09:00:00Z");
    engine
}

/// A document in `font` whose text, in a column 70pt wide, breaks once at a soft hyphen and has another that no line
/// breaks at, with `output` and `meta` in it.
fn document(font: &str, output: &str, meta: &str) -> String {
    format!(
        r##"<lpdf version="1"><assets><font name="body" src="body.ttf"/></assets><document font="{font}">{meta}{output}
        <section><layout><column width="70pt"><text>Donau&#xAD;dampf&#xAD;schiff&#xAD;fahrt</text></column></layout></section>
        </document></lpdf>"##
    )
}

fn render(xml: &str) -> Vec<u8> {
    engine().render_xml(xml, None, &RenderEncryption::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// The decoded content stream of the page of `pdf`: the one stream that sets a font in the body's size.
fn page_content(pdf: &[u8]) -> String {
    let streams: Vec<String> = flate_streams(pdf).into_iter().map(|s| String::from_utf8_lossy(&s).into_owned())
        .filter(|s| s.contains(" 11 Tf")).collect();
    assert_eq!(streams.len(), 1, "one page");
    streams[0].clone()
}

/// The bytes the hyphen's marked content shows: the string of the one `Tj` in it, a hex string or a literal one.
fn hyphen_shown(content: &str) -> Vec<u8> {
    let at = content.find(HYPHEN_SPAN).unwrap_or_else(|| panic!("no hyphen marked as a soft hyphen: {content}"));
    let rest = &content[at + HYPHEN_SPAN.len()..];
    let (shown, after) = rest.split_once(" Tj\n").expect("a Tj");
    assert!(after.starts_with("EMC\n"), "the hyphen alone is marked: {rest}");
    if let Some(hex) = shown.strip_prefix('<').and_then(|s| s.strip_suffix('>')) {
        return (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    }
    literal_bytes(shown.strip_prefix('(').and_then(|s| s.strip_suffix(')')).expect("a string"))
}

/// The bytes of the inside of a PDF literal string, its escapes read.
fn literal_bytes(literal: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chars = literal.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c as u8);
            continue;
        }
        match chars.next().expect("an escape") {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            'b' => out.push(8),
            'f' => out.push(12),
            digit @ '0'..='7' => {
                let mut value = digit.to_digit(8).unwrap();
                for _ in 0..2 {
                    match chars.peek().and_then(|d| d.to_digit(8)) {
                        Some(d) => { value = value * 8 + d; chars.next(); }
                        None => break,
                    }
                }
                out.push(value as u8);
            }
            other => out.push(other as u8),
        }
    }
    out
}

#[test]
fn a_built_in_font_draws_the_hyphen_with_its_code_for_the_soft_hyphen_marked_as_one() {
    let content = page_content(&render(&document("Helvetica", "", "")));
    assert_eq!(content.matches(HYPHEN_SPAN).count(), 1, "one line is broken: {content}");
    // Code 0xAD of WinAnsiEncoding: the hyphen's glyph, under the code of the soft hyphen.
    assert_eq!(hyphen_shown(&content), [0xAD]);
}

#[test]
fn an_embedded_font_draws_its_hyphen_glyph_marked_as_the_soft_hyphen_it_has_no_glyph_for() {
    let pdf = render(&document("body", "", ""));
    let content = page_content(&pdf);
    assert_eq!(content.matches(HYPHEN_SPAN).count(), 1, "one line is broken: {content}");
    let [high, low] = hyphen_shown(&content)[..] else { panic!("one glyph, two bytes") };
    // The glyph it shows is the font's `-`, by the font's ToUnicode map.
    let to_unicode = flate_streams(&pdf).into_iter().map(|s| String::from_utf8_lossy(&s).into_owned())
        .find(|s| s.contains("beginbfchar") && !s.contains("LpdfAttribution")).expect("the body font's ToUnicode map");
    let entry = format!("<{high:02X}{low:02X}> <002D>");
    assert!(to_unicode.contains(&entry), "no {entry} in {to_unicode}");
}

#[test]
fn a_pdfa_file_takes_a_soft_hyphen_in_a_font_that_has_no_glyph_for_it() {
    let pdf = render(&document("body", r#"<output pdfa="2b"/>"#, ""));
    assert_eq!(page_content(&pdf).matches(HYPHEN_SPAN).count(), 1);
}

#[test]
fn in_a_tagged_file_the_hyphen_is_marked_inside_the_words_of_its_paragraph() {
    let meta = r#"<meta title="Soft hyphens" lang="de"/>"#;
    let content = page_content(&render(&document("body", r#"<output pdfua="1"/>"#, meta)));
    let at = content.find(HYPHEN_SPAN).expect("the hyphen");
    // The marked content still open where the hyphen's begins: the paragraph's, with its MCID. Each begins at the tag
    // that starts the line its operator, after the property list, ends.
    let before = &content[..at];
    let mut open: Vec<&str> = Vec::new();
    for (end, operator) in before.match_indices(" BDC\n").chain(before.match_indices(" BMC\n")).chain(before.match_indices("EMC\n"))
        .collect::<std::collections::BTreeMap<_, _>>()
    {
        if operator == "EMC\n" {
            open.pop();
        } else {
            let start = before[..end].rfind("\n/").map_or(0, |at| at + 1);
            open.push(&before[start..end]);
        }
    }
    assert!(matches!(open.as_slice(), [paragraph] if paragraph.starts_with("/P <<") && paragraph.contains("/MCID")), "{open:?}");
}

#[test]
fn a_link_broken_at_a_soft_hyphen_is_described_by_its_whole_word() {
    let xml = document("body", r#"<output pdfua="1"/>"#, r#"<meta title="Soft hyphens" lang="de"/>"#).replace(
        "<text>Donau&#xAD;dampf&#xAD;schiff&#xAD;fahrt</text>",
        r#"<text><span href="https://lpdf.io">Donau&#xAD;dampf&#xAD;schiff&#xAD;fahrt</span></text>"#,
    );
    let pdf = String::from_utf8_lossy(&render(&xml)).into_owned();
    assert!(pdf.contains("/Contents (Donaudampfschifffahrt)"), "{}", &pdf[pdf.find("/Annot").unwrap_or(0)..][..400]);
}

#[test]
fn the_pdfua_sample_that_verapdf_checks_has_a_word_broken_at_a_soft_hyphen() {
    let pdf = crate::pdfua_render_tests::render_fixture();
    let hyphens: usize = flate_streams(&pdf).iter().map(|s| String::from_utf8_lossy(s).matches(HYPHEN_SPAN).count()).sum();
    assert!(hyphens > 0, "no word of it breaks at a soft hyphen");
}
