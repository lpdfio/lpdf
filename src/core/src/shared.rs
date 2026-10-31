//! Functions shared between `lib.rs` (WASM/wasm-bindgen crate) and
//! `core-wasi/main.rs` (WASI binary crate).
//!
//! This module is included by `lib.rs` via `mod shared` and by
//! `core-wasi/main.rs` via `#[path = "../../core/src/shared.rs"] mod shared`.

use std::collections::HashMap;

/// Extract the advance width of every character the font maps from raw TrueType/OpenType
/// font bytes, normalised to 1/1000 em units. A character the font does not map gets the
/// advance of glyph 0 (.notdef), which is the glyph the encoder writes for it.
/// Returns `None` if the font cannot be parsed (WOFF/WOFF2, corrupt data, etc.).
pub(crate) fn extract_font_widths(bytes: &[u8]) -> Option<super::tokens::FontWidths> {
    let face = ttf_parser::Face::parse(bytes, 0).ok()?;
    let upm = face.units_per_em() as u32;
    if upm == 0 { return None; }

    let per_mille = |gid: ttf_parser::GlyphId| {
        let advance = face.glyph_hor_advance(gid).unwrap_or(0) as u32;
        ((advance * 1000 + upm / 2) / upm) as u16
    };

    let mut advances: HashMap<char, u16> = HashMap::new();
    if let Some(cmap) = face.tables().cmap {
        for subtable in cmap.subtables.into_iter().filter(|s| s.is_unicode()) {
            subtable.codepoints(|cp| {
                let Some(c) = char::from_u32(cp) else { return };
                // `Face::glyph_index` is the lookup the encoder uses, so the table lists
                // exactly the characters it can encode.
                if let Some(gid) = face.glyph_index(c) {
                    advances.entry(c).or_insert_with(|| per_mille(gid));
                }
            });
        }
    }

    let default = per_mille(ttf_parser::GlyphId(0));
    Some(super::tokens::FontWidths::new(default, Vec::new(), advances))
}

/// Parse a JSON permissions object into an `encrypt::Permissions` value.
///
/// All boolean fields (`print`, `modify`, `copy`, `annotate`, `fill_forms`,
/// `accessibility`, `assemble`, `print_hq`) default to `true` when absent.
pub(crate) fn parse_permissions_json(json: &str) -> super::encrypt::Permissions {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
    let get_bool = |key: &str| v.get(key).and_then(|b| b.as_bool()).unwrap_or(true);
    super::encrypt::Permissions {
        print:         get_bool("print"),
        modify:        get_bool("modify"),
        copy:          get_bool("copy"),
        annotate:      get_bool("annotate"),
        fill_forms:    get_bool("fill_forms"),
        accessibility: get_bool("accessibility"),
        assemble:      get_bool("assemble"),
        print_hq:      get_bool("print_hq"),
    }
}

/// Core render-tree-to-JSON logic shared by the WASM engine (`lib.rs`) and the
/// WASI binary (`core-wasi/main.rs`).
///
/// `font_widths` must already be merged by the caller:
/// - The WASM engine merges engine-level widths (from `set_font_metrics`) with
///   doc-level widths; doc-level takes precedence.
/// - The WASI binary uses doc-level widths only.
pub(crate) fn render_doc_shared(
    mut doc: super::parse::Document,
    font_widths: HashMap<String, super::tokens::FontWidths>,
    license_key: &str,
    now_unix: i64,
) -> String {
    super::layout::set_font_widths(font_widths);
    let section_layouts = doc.section_layouts();
    let pages: Vec<super::render::RenderPage> =
        super::layout::layout_document(&section_layouts);

    let fonts: serde_json::Map<String, serde_json::Value> = doc.fonts
        .into_iter()
        .map(|(name, def)| {
            let v = match def {
                super::tokens::FontDef::Core(b) => serde_json::json!({ "core": b }),
                super::tokens::FontDef::Ref(s)  => serde_json::json!({ "ref": s }),
            };
            (name, v)
        })
        .collect();

    let keywords: Vec<&str> = doc.meta.keywords
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s: &&str| !s.is_empty())
        .collect();

    let meta = serde_json::json!({
        "title":    doc.meta.title,
        "author":   doc.meta.author,
        "subject":  doc.meta.subject,
        "keywords": keywords,
        "creator":  doc.meta.creator,
        "fonts":    fonts,
    });

    let status = super::license::check(license_key, now_unix);
    let watermark = if status.is_licensed() {
        serde_json::Value::Null
    } else {
        // Mirrors the line pdf.rs draws on unlicensed PDFs: the lpdf mark and "Lpdf".
        serde_json::json!({
            "type": "lpdf:watermark",
            "text": "Lpdf",
            "url":  "https://lpdf.io"
        })
    };

    let mut output = super::render::pages_to_json(&pages, meta, watermark);
    if let Some(warn) = status.warning() {
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&output) {
            val["license_warning"] = serde_json::Value::String(warn.to_string());
            output = val.to_string();
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONTSERRAT: &[u8] = include_bytes!("../../../test/assets/fonts/Montserrat-Regular.ttf");

    /// The advance of `c` in the face itself, in font units scaled to 1/1000 em.
    fn real_advance(face: &ttf_parser::Face, c: char) -> f32 {
        let gid = face.glyph_index(c).expect("the face maps the character");
        face.glyph_hor_advance(gid).expect("the glyph has an advance") as f32 * 1000.0
            / face.units_per_em() as f32
    }

    #[test]
    fn lists_the_real_advance_of_characters_beyond_ascii() {
        let face = ttf_parser::Face::parse(MONTSERRAT, 0).expect("fixture face parses");
        let widths = extract_font_widths(MONTSERRAT).expect("fixture face has widths");
        for c in "éÉñü€“”’–—Жд".chars() {
            let listed = widths.advance(c) as f32;
            assert!((listed - real_advance(&face, c)).abs() <= 0.5, "advance of {c:?} is {listed}");
        }
        assert_ne!(widths.advance('—'), widths.advance('i'), "an em dash is not as narrow as an i");
    }

    #[test]
    fn keeps_the_ascii_advances_of_before() {
        let face = ttf_parser::Face::parse(MONTSERRAT, 0).expect("fixture face parses");
        let upm = face.units_per_em() as u32;
        let widths = extract_font_widths(MONTSERRAT).expect("fixture face has widths");
        for cp in 32u32..=126 {
            let c = char::from_u32(cp).expect("ascii is a char");
            let advance = face.glyph_hor_advance(face.glyph_index(c).expect("ascii is mapped")).expect("has advance") as u32;
            assert_eq!(widths.advance(c) as u32, (advance * 1000 + upm / 2) / upm, "advance of {c:?}");
        }
    }

    #[test]
    fn counts_an_unmapped_character_as_glyph_zero() {
        let face = ttf_parser::Face::parse(MONTSERRAT, 0).expect("fixture face parses");
        let widths = extract_font_widths(MONTSERRAT).expect("fixture face has widths");
        let notdef = face.glyph_hor_advance(ttf_parser::GlyphId(0)).expect("glyph 0 has an advance") as f32 * 1000.0
            / face.units_per_em() as f32;
        assert!(face.glyph_index('\u{4e2d}').is_none(), "the fixture face has no CJK");
        assert!((widths.advance('\u{4e2d}') as f32 - notdef).abs() <= 0.5);
    }
}
