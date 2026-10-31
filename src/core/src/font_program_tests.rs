//! The font program each kind of embedded face is written as, and the CIDFont it goes under (ISO 32000-1, 9.9):
//! TrueType outlines as `/FontFile2` under a `/CIDFontType2`, CFF outlines as an OpenType `/FontFile3` under a
//! `/CIDFontType0`, and the CFF2 outlines of a variable OpenType font turned into TrueType ones at the font's default
//! instance. Each program is a subset, and the glyph a reader finds in it for a character is the face's. The WASI
//! crate has its own `LpdfEngine`, so these run with the core library only.

use std::collections::{HashMap, HashSet};

use miniz_oxide::inflate::decompress_to_vec_zlib;
use ttf_parser::{Face, GlyphId, Tag};

use crate::{LpdfEngine, RenderEncryption};

const STATIC_TRUETYPE: &[u8] = include_bytes!("../../../test/assets/fonts/Montserrat-Regular.ttf");
const STATIC_CFF: &[u8] = include_bytes!("../../../test/assets/fonts/SourceSans3-Regular.otf");
const VARIABLE_CFF2: &[u8] = include_bytes!("../../../test/assets/fonts/SourceSans3VF-Upright.otf");
const VARIABLE_TRUETYPE: &[u8] = include_bytes!("../../../test/assets/fonts/SourceSans3VF-Upright.ttf");

const FONTS: [(&str, &[u8]); 4] = [
    ("static TrueType", STATIC_TRUETYPE),
    ("static CFF", STATIC_CFF),
    ("variable CFF2", VARIABLE_CFF2),
    ("variable TrueType", VARIABLE_TRUETYPE),
];

const TEXT: &str = "Façade, naïve, Ørsted — “quoted”, €5 and 3 × 4 = 12";

/// What a PDF holds for the embedded font `brand`.
struct Embedded {
    cid_font: String,
    descriptor: String,
    /// The dictionary of the font program's stream.
    program_dict: String,
    /// The font program, inflated.
    program: Vec<u8>,
    /// The code the content stream draws each character with, from the ToUnicode CMap.
    codes: HashMap<char, u16>,
}

/// `TEXT` in the font `brand`, loaded from `font`, at PDF/A-2u when `pdfa` is set.
fn render(font: &[u8], pdfa: bool) -> Vec<u8> {
    let mut engine = LpdfEngine::new("");
    engine.load_font("brand", font);
    engine.set_created_on("2026-10-31T09:30:00Z");
    let output = if pdfa { r#"<output pdfa="2u"/>"# } else { "" };
    let xml = format!(
        r#"<lpdf version="1"><assets><font name="brand" src="brand.otf"/></assets><document font="brand">{output}<section><layout><text>{TEXT}</text></layout></section></document></lpdf>"#
    );
    engine.render_xml(&xml, None, &RenderEncryption::default()).expect("renders")
}

fn embedded(font: &[u8]) -> Embedded {
    embedded_in(&render(font, false))
}

fn embedded_in(pdf: &[u8]) -> Embedded {
    let dicts = dictionaries(pdf);
    let dict = |number: u32| dicts[&number].clone();
    let font = dicts.values()
        .find(|d| d.contains("/Subtype /Type0") && d.contains("/BaseFont /brand"))
        .expect("the Type0 font brand");
    let cid_font = dict(reference(font, "/DescendantFonts ["));
    let descriptor = dict(reference(&cid_font, "/FontDescriptor "));
    let program_number = ["/FontFile2 ", "/FontFile3 "].iter()
        .find(|key| descriptor.contains(**key))
        .map(|key| reference(&descriptor, key))
        .expect("a font program");
    let cmap = String::from_utf8(stream(pdf, &dicts, reference(font, "/ToUnicode "))).expect("an ASCII CMap");
    let codes = cmap.lines()
        .filter_map(|line| line.strip_prefix('<')?.split_once("> <"))
        .filter_map(|(code, unicode)| Some((
            char::from_u32(u32::from_str_radix(unicode.strip_suffix('>')?, 16).ok()?)?,
            u16::from_str_radix(code, 16).ok()?,
        )))
        .collect();
    Embedded {
        cid_font,
        program_dict: dict(program_number),
        program: stream(pdf, &dicts, program_number),
        descriptor,
        codes,
    }
}

/// The dictionary of each object of `pdf`, by its number, up to its stream if it has one.
fn dictionaries(pdf: &[u8]) -> HashMap<u32, String> {
    const HEADER: &[u8] = b" 0 obj\n";
    let mut found = HashMap::new();
    let mut from = 0;
    while let Some(at) = find(pdf, HEADER, from) {
        let line = pdf[..at].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        let start = at + HEADER.len();
        let end = [find(pdf, b"\nstream\n", start), find(pdf, b"\nendobj", start)].into_iter().flatten().min()
            .expect("an object end");
        if let Ok(number) = String::from_utf8_lossy(&pdf[line..at]).parse() {
            found.insert(number, String::from_utf8_lossy(&pdf[start..end]).into_owned());
        }
        from = end;
    }
    found
}

/// The number of the object that `key` in `dict` refers to, as in `/FontFile3 12 0 R`.
fn reference(dict: &str, key: &str) -> u32 {
    dict.split(key).nth(1).and_then(|rest| rest.split_whitespace().next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("{key} in {dict}"))
}

/// The data of stream object `number`, inflated, read by the `/Length` of its dictionary.
fn stream(pdf: &[u8], dicts: &HashMap<u32, String>, number: u32) -> Vec<u8> {
    let length = reference(&dicts[&number], "/Length ") as usize;
    let header = format!("\n{number} 0 obj\n");
    let object = find(pdf, header.as_bytes(), 0).expect("the object");
    let start = find(pdf, b"\nstream\n", object).expect("a stream") + b"\nstream\n".len();
    decompress_to_vec_zlib(&pdf[start..start + length]).expect("a FlateDecode stream")
}

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack[from..].windows(needle.len()).position(|w| w == needle).map(|i| from + i)
}

/// The tags of the tables of an OpenType font program.
fn tables(program: &[u8]) -> Vec<String> {
    let count = u16::from_be_bytes([program[4], program[5]]) as usize;
    (0..count).map(|i| String::from_utf8_lossy(&program[12 + 16 * i..16 + 16 * i]).into_owned()).collect()
}

// ── The program and the CIDFont of each kind ──────────────────────────────────

#[test]
fn a_cff_font_is_an_opentype_program_under_a_type_0_cidfont() {
    let font = embedded(STATIC_CFF);
    assert!(font.cid_font.contains("/Subtype /CIDFontType0\n"), "{}", font.cid_font);
    assert!(font.descriptor.contains("/FontFile3 ") && !font.descriptor.contains("/FontFile2"), "{}", font.descriptor);
    assert!(font.program_dict.contains("/Subtype /OpenType"), "{}", font.program_dict);
    assert!(!font.program_dict.contains("/Length1"), "{}", font.program_dict);
    assert_eq!(&font.program[..4], b"OTTO");
    let program = Face::parse(&font.program, 0).expect("the program");
    let cff = program.tables().cff.expect("a CFF table");
    assert!(cff.glyph_cid(GlyphId(1)).is_some(), "the CFF is CID-keyed, so a CID names each glyph");
}

#[test]
fn a_cff2_font_is_a_truetype_program_under_a_type_2_cidfont() {
    let font = embedded(VARIABLE_CFF2);
    assert_truetype_program(&font);
    let tables = tables(&font.program);
    assert!(!tables.iter().any(|tag| tag.starts_with("CFF")), "{tables:?}");
}

#[test]
fn a_variable_truetype_font_is_a_truetype_program_under_a_type_2_cidfont() {
    assert_truetype_program(&embedded(VARIABLE_TRUETYPE));
}

#[test]
fn a_static_truetype_font_is_a_truetype_program_under_a_type_2_cidfont() {
    assert_truetype_program(&embedded(STATIC_TRUETYPE));
}

fn assert_truetype_program(font: &Embedded) {
    assert!(font.cid_font.contains("/Subtype /CIDFontType2\n"), "{}", font.cid_font);
    assert!(font.descriptor.contains("/FontFile2 ") && !font.descriptor.contains("/FontFile3"), "{}", font.descriptor);
    let length1 = format!("/Length1 {}\n", font.program.len());
    assert!(font.program_dict.contains(&length1), "{length1} in {}", font.program_dict);
    assert_eq!(&font.program[..4], &[0, 1, 0, 0]);
    assert!(tables(&font.program).contains(&"glyf".to_string()), "{:?}", tables(&font.program));
}

#[test]
fn only_a_type_2_cidfont_of_a_pdfa_file_has_a_cid_to_gid_map() {
    let cff = embedded_in(&render(STATIC_CFF, true));
    assert!(cff.cid_font.contains("/Subtype /CIDFontType0") && !cff.cid_font.contains("/CIDToGIDMap"), "{}", cff.cid_font);
    let cff2 = embedded_in(&render(VARIABLE_CFF2, true));
    assert!(cff2.cid_font.contains("/CIDToGIDMap /Identity"), "{}", cff2.cid_font);
}

// ── The glyphs in it ──────────────────────────────────────────────────────────

#[test]
fn each_program_is_a_subset_of_the_glyphs_drawn() {
    for (kind, bytes) in FONTS {
        let face = Face::parse(bytes, 0).expect("the face");
        let program = embedded(bytes).program;
        let subset = Face::parse(&program, 0).unwrap_or_else(|e| panic!("{kind}: the program parses: {e}"));
        let drawn: HashSet<GlyphId> = TEXT.chars().filter_map(|c| face.glyph_index(c)).collect();
        // .notdef and the glyphs drawn, and for TrueType the parts of a composite glyph, such as an accent.
        assert!(subset.number_of_glyphs() as usize > drawn.len(), "{kind}");
        assert!(subset.number_of_glyphs() < face.number_of_glyphs() / 10, "{kind}: {} glyphs", subset.number_of_glyphs());
        for tag in [b"fvar", b"gvar", b"HVAR", b"CFF2"] {
            assert!(subset.raw_face().table(Tag::from_bytes(tag)).is_none(), "{kind}: no {}", String::from_utf8_lossy(tag));
        }
    }
}

/// The glyph a reader draws for `code` in `program`: under a Type 2 CIDFont the code is the glyph's ID (an
/// Identity CIDToGIDMap), under a Type 0 one the CFF's charset maps it, as a CID, to a glyph.
fn glyph_for_code(program: &Face, code: u16) -> GlyphId {
    match program.tables().cff {
        Some(cff) => (0..program.number_of_glyphs()).map(GlyphId)
            .find(|&gid| cff.glyph_cid(gid) == Some(code))
            .unwrap_or_else(|| panic!("a glyph with CID {code}")),
        None => GlyphId(code),
    }
}

#[test]
fn the_glyph_a_reader_finds_for_each_character_has_the_faces_advance_and_bounds() {
    for (kind, bytes) in FONTS {
        let face = Face::parse(bytes, 0).expect("the face");
        let font = embedded(bytes);
        let program = Face::parse(&font.program, 0).expect("the program");
        for c in TEXT.chars().filter(|c| !c.is_whitespace()) {
            let original = face.glyph_index(c).unwrap_or_else(|| panic!("{kind}: a glyph for {c}"));
            let code = *font.codes.get(&c).unwrap_or_else(|| panic!("{kind}: a code for {c}"));
            let drawn = glyph_for_code(&program, code);
            assert_eq!(program.glyph_hor_advance(drawn), face.glyph_hor_advance(original), "{kind}: the advance of {c}");
            assert_eq!(program.glyph_bounding_box(drawn), face.glyph_bounding_box(original), "{kind}: the bounds of {c}");
        }
    }
}
