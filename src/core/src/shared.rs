//! Functions shared between `lib.rs` (WASM/wasm-bindgen crate) and
//! `core-wasi/main.rs` (WASI binary crate).
//!
//! This module is included by `lib.rs` via `mod shared` and by
//! `core-wasi/main.rs` via `#[path = "../../core/src/shared.rs"] mod shared`.

use std::collections::HashMap;

/// Extract per-glyph advance widths for printable ASCII (code points 32–126)
/// from raw TrueType/OpenType font bytes, normalised to 1/1000 em units.
/// Returns `None` if the font cannot be parsed (WOFF/WOFF2, corrupt data, etc.).
pub(crate) fn extract_font_widths(bytes: &[u8]) -> Option<super::tokens::FontWidths> {
    let face = ttf_parser::Face::parse(bytes, 0).ok()?;
    let upm = face.units_per_em() as u32;
    if upm == 0 { return None; }

    let mut ascii: Vec<u16> = Vec::with_capacity(95);
    let mut sum: u32 = 0;
    let mut count: u32 = 0;

    for cp in 32u32..=126 {
        let ch = char::from_u32(cp).unwrap_or(' ');
        let adv = face.glyph_index(ch)
            .and_then(|gid| face.glyph_hor_advance(gid))
            .unwrap_or_else(|| face.glyph_hor_advance(ttf_parser::GlyphId(0)).unwrap_or(0));
        let w = ((adv as u32 * 1000 + upm / 2) / upm) as u16;
        ascii.push(w);
        if w > 0 { sum += w as u32; count += 1; }
    }

    let default = if count > 0 { ((sum + count / 2) / count) as u16 } else { 500 };
    Some(super::tokens::FontWidths { default, ascii })
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
    doc: super::parse::Document,
    font_widths: HashMap<String, super::tokens::FontWidths>,
    license_key: &str,
    now_unix: i64,
) -> String {
    super::layout::set_font_widths(font_widths);
    let pages: Vec<super::render::RenderPage> =
        doc.pages.iter().flat_map(super::layout::layout_page).collect();

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
        serde_json::json!({
            "type": "lpdf:watermark",
            "text": "made with lpdf.io",
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
