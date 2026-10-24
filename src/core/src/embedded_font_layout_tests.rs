//! Layout of text in an embedded font, checked in a rendered document against the font's own
//! advances. The WASI crate has its own `LpdfEngine`, so these run with the core library only.

use ttf_parser::Face;

const MONTSERRAT: &[u8] = include_bytes!("../../../examples/5-brochure/assets/fonts/Montserrat-Regular.ttf");

/// Lay out `paragraph` in the embedded font `brand` and return the text nodes of the page.
fn laid_out_text_nodes(paragraph: &str) -> Vec<serde_json::Value> {
    let mut engine = crate::LpdfEngine::new("");
    engine.load_font("brand", MONTSERRAT);
    let xml = format!(
        r##"<lpdf version="1"><assets><font name="brand" src="brand.ttf"/></assets><document font="brand"><section><layout><text>{paragraph}</text></layout></section></document></lpdf>"##
    );
    let json: serde_json::Value = serde_json::from_str(&engine.render(&xml)).expect("layout JSON");
    let nodes = json["pages"][0]["nodes"][0]["nodes"].as_array().expect("a box of text nodes");
    nodes.iter().filter(|n| n["type"] == "text").cloned().collect()
}

/// Width of `text` at `size` pt by the face's own advances.
fn face_width(text: &str, size: f32) -> f32 {
    let face = Face::parse(MONTSERRAT, 0).expect("fixture face parses");
    text.chars()
        .map(|c| face.glyph_index(c).unwrap_or(ttf_parser::GlyphId(0)))
        .map(|gid| face.glyph_hor_advance(gid).unwrap_or(0) as f32 * size / face.units_per_em() as f32)
        .sum()
}

#[test]
fn a_span_after_accents_and_curly_quotes_starts_where_the_text_before_it_ends() {
    let nodes = laid_out_text_nodes(r##"Crème “brûlée” — 5 € <span color="#ff0000">span</span> end"##);
    let before = nodes[0]["content"].as_str().expect("content");
    let span_x = nodes[1]["x"].as_f64().expect("x") as f32;
    let size = nodes[0]["size"].as_f64().expect("size") as f32;
    let expected = nodes[0]["x"].as_f64().expect("x") as f32 + face_width(&format!("{before} "), size);
    assert!((span_x - expected).abs() < 0.05, "span at {span_x}, text before it ends at {expected}");
}

#[test]
fn a_cyrillic_span_starts_where_the_text_before_it_ends() {
    let nodes = laid_out_text_nodes(r##"Привет, «мир» — 5 ₽ <span color="#ff0000">span</span> end"##);
    let before = nodes[0]["content"].as_str().expect("content");
    let span_x = nodes[1]["x"].as_f64().expect("x") as f32;
    let size = nodes[0]["size"].as_f64().expect("size") as f32;
    let expected = nodes[0]["x"].as_f64().expect("x") as f32 + face_width(&format!("{before} "), size);
    assert!((span_x - expected).abs() < 0.05, "span at {span_x}, text before it ends at {expected}");
}

#[test]
fn a_paragraph_with_accents_wraps_where_the_face_widths_say() {
    let words = "Ça été très naïve — “déjà vu” à Zürich, señor Åström, 5 € ";
    let paragraph = words.repeat(8);
    let nodes = laid_out_text_nodes(&paragraph);
    let (size, box_width) = (nodes[0]["size"].as_f64().expect("size") as f32, nodes[0]["line_width"].as_f64().expect("width") as f32);

    let mut expected: Vec<String> = Vec::new();
    for word in paragraph.split_whitespace() {
        match expected.last_mut() {
            Some(line) if face_width(&format!("{line} {word}"), size) <= box_width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => expected.push(word.to_string()),
        }
    }
    let lines: Vec<&str> = nodes.iter().map(|n| n["content"].as_str().expect("content")).collect();
    assert!(lines.len() > 1, "the paragraph is long enough to wrap");
    assert_eq!(lines, expected);
}
