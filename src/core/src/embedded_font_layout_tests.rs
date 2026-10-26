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

/// Lay out `body` as a justified text in a box `box_width` wide, in the embedded font `brand` or in Helvetica
/// when `embedded` is false, and return the text nodes of the page, line by line.
fn justified_text_nodes(body: &str, embedded: bool, box_width: f32) -> Vec<serde_json::Value> {
    let mut engine = crate::LpdfEngine::new("");
    engine.load_font("brand", MONTSERRAT);
    let (assets, font) = if embedded { (r#"<assets><font name="brand" src="brand.ttf"/></assets>"#, "brand") } else { ("", "Helvetica") };
    let xml = format!(
        r##"<lpdf version="1">{assets}<document font="{font}"><section><layout><column width="{box_width}pt"><text align="justify">{body}</text></column></layout></section></document></lpdf>"##
    );
    let json: serde_json::Value = serde_json::from_str(&engine.render(&xml)).expect("layout JSON");
    let nodes = json["pages"][0]["nodes"][0]["nodes"][0]["nodes"].as_array().expect("a box of text nodes");
    nodes.iter().flat_map(flatten_links).collect()
}

/// A link holds its text; the other nodes are what they are.
fn flatten_links(node: &serde_json::Value) -> Vec<serde_json::Value> {
    if node["type"] == "link" {
        return node["nodes"].as_array().expect("link children").iter().filter(|n| n["type"] == "text").cloned().collect();
    }
    if node["type"] == "text" { vec![node.clone()] } else { Vec::new() }
}

/// The text nodes of a line, from the left, as (x, width) where the width counts the gap that layout added at each space.
fn spans_of(nodes: &[serde_json::Value], width_of: impl Fn(&str, f32) -> f32) -> Vec<(f32, f32, f32)> {
    nodes
        .iter()
        .map(|n| {
            let content = n["content"].as_str().expect("content");
            let size = n["size"].as_f64().expect("size") as f32;
            let gap = n["word_gap"].as_f64().expect("word_gap") as f32;
            let x = n["x"].as_f64().expect("x") as f32;
            (x, width_of(content, size) + gap * content.matches(' ').count() as f32, n["y"].as_f64().expect("y") as f32)
        })
        .collect()
}

fn lines_of(spans: &[(f32, f32, f32)]) -> Vec<Vec<(f32, f32, f32)>> {
    let mut lines: Vec<Vec<(f32, f32, f32)>> = Vec::new();
    for span in spans {
        match lines.last_mut() {
            Some(line) if (line[0].2 - span.2).abs() < 0.01 => line.push(*span),
            _ => lines.push(vec![*span]),
        }
    }
    lines
}

const PARAGRAPH: &str = "The keepers wrote every evening in the record, in a hand that changed every few years, and the oldest entries were kept in a cupboard with spare wicks and one brass bell that nobody remembered buying.";

#[test]
fn every_line_of_a_justified_paragraph_in_an_embedded_font_but_the_last_ends_at_the_right_edge() {
    let nodes = justified_text_nodes(PARAGRAPH, true, 300.0);
    let lines = lines_of(&spans_of(&nodes, face_width));
    assert!(lines.len() > 2, "the paragraph wraps");
    let left = lines[0][0].0;
    for line in &lines[..lines.len() - 1] {
        let end = line.last().map(|(x, w, _)| x + w).expect("a line has text");
        assert!((end - (left + 300.0)).abs() < 0.05, "a line ends at {end}, not at the right edge {}", left + 300.0);
    }
    let last = lines.last().and_then(|l| l.last()).expect("a last line");
    assert!(last.0 + last.1 < left + 300.0 - 1.0, "the last line stays left-aligned");
}

#[test]
fn a_justified_line_with_spans_has_no_overlap_and_even_gaps_in_both_kinds_of_font() {
    let body = r##"The keepers wrote <span color="#ff0000">every evening</span> in the record, in <span href="https://lpdf.io">a hand</span> that changed every few years, and the oldest entries were kept in a cupboard with <span color="#0000ff">spare wicks</span> and one brass bell that nobody remembered buying."##;
    for embedded in [true, false] {
        let nodes = justified_text_nodes(body, embedded, 300.0);
        let width_of = |text: &str, size: f32| if embedded { face_width(text, size) } else { crate::layout::text_width("Helvetica", text, size) };
        let lines = lines_of(&spans_of(&nodes, width_of));
        assert!(lines.len() > 2, "the paragraph wraps");
        let left = lines[0][0].0;
        for line in &lines[..lines.len() - 1] {
            let mut gaps = Vec::new();
            for pair in line.windows(2) {
                gaps.push(pair[1].0 - (pair[0].0 + pair[0].1));
            }
            let end = line.last().map(|(x, w, _)| x + w).expect("a line has text");
            assert!((end - (left + 300.0)).abs() < 0.1, "embedded {embedded}: a line ends at {end}, not {}", left + 300.0);
            let widest = gaps.iter().cloned().fold(f32::MIN, f32::max);
            let narrowest = gaps.iter().cloned().fold(f32::MAX, f32::min);
            assert!(narrowest > 0.0, "embedded {embedded}: words overlap, gaps {gaps:?}");
            assert!(widest - narrowest < 0.1, "embedded {embedded}: uneven gaps {gaps:?}");
        }
    }
}

#[test]
fn justified_lines_with_accents_curly_quotes_and_cyrillic_end_at_the_right_edge() {
    // The layout JSON rounds `word_gap` to hundredths, so the sum over a line is checked to 0.15 pt.
    let latin = "Ça été très naïve — “déjà vu” à Zürich, señor Åström, 5 € pour la crème brûlée de la maison ".repeat(3);
    let cyrillic = "Привет, «мир» — это длинный абзац текста, который переносится на несколько строк и выравнивается по ширине ".repeat(3);
    for body in [latin, cyrillic] {
        let nodes = justified_text_nodes(&body, true, 500.0);
        let lines = lines_of(&spans_of(&nodes, face_width));
        assert!(lines.len() > 2, "the paragraph wraps");
        let left = lines[0][0].0;
        for line in &lines[..lines.len() - 1] {
            let end = line.last().map(|(x, w, _)| x + w).expect("a line has text");
            assert!((end - (left + 500.0)).abs() < 0.15, "a line ends at {end}, not at the right edge {}", left + 500.0);
        }
    }
}

/// The text nodes of the first page of a layout holding `body`, with the font `brand` loaded.
fn text_nodes_of_layout(body: &str) -> Vec<serde_json::Value> {
    let mut engine = crate::LpdfEngine::new("");
    engine.load_font("brand", MONTSERRAT);
    let xml = format!(
        r##"<lpdf version="1"><assets><font name="brand" src="brand.ttf"/></assets><document><section><layout margin="20pt">{body}</layout></section></document></lpdf>"##
    );
    let json: serde_json::Value = serde_json::from_str(&engine.render(&xml)).expect("layout JSON");
    let mut out = Vec::new();
    fn walk(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        if v["type"] == "text" {
            out.push(v.clone());
        }
        if let Some(children) = v["nodes"].as_array() {
            for child in children {
                walk(child, out);
            }
        }
    }
    walk(&json["pages"][0], &mut out);
    out
}

#[test]
fn a_code_in_an_embedded_font_keeps_its_spaces_and_places_a_span_where_the_face_says() {
    let nodes = text_nodes_of_layout(r##"<code font="brand">    let <span color="#ff0000">xy</span> = 1;</code>"##);
    let contents: Vec<&str> = nodes.iter().map(|n| n["content"].as_str().expect("content")).collect();
    assert_eq!(contents, vec!["    let ", "xy", " = 1;"]);
    let size = nodes[0]["size"].as_f64().expect("size") as f32;
    let start = nodes[0]["x"].as_f64().expect("x") as f32;
    let span_x = nodes[1]["x"].as_f64().expect("x") as f32;
    let after_x = nodes[2]["x"].as_f64().expect("x") as f32;
    assert!((span_x - (start + face_width("    let ", size))).abs() < 0.05, "span at {span_x}");
    assert!((after_x - (span_x + face_width("xy", size))).abs() < 0.05, "text after the span at {after_x}");
}

#[test]
fn a_long_line_of_code_in_an_embedded_font_wraps_where_the_face_widths_say() {
    // A box 150pt wide, with the 8pt of padding on each side leaves 134pt for the lines.
    let words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
    let nodes = text_nodes_of_layout(&format!(r#"<code font="brand" width="150pt">{words}</code>"#));
    let size = nodes[0]["size"].as_f64().expect("size") as f32;
    let lines: Vec<&str> = nodes.iter().map(|n| n["content"].as_str().expect("content")).collect();
    assert!(lines.len() > 2, "the line wraps");
    for line in &lines {
        assert!(face_width(line, size) <= 134.0 + 0.01, "{line:?} is wider than the box");
    }
    for (line, next) in lines.iter().zip(&lines[1..]) {
        let first_word = next.split(' ').next().expect("a word");
        assert!(face_width(&format!("{line} {first_word}"), size) > 134.0, "{first_word:?} fits on the line before it");
    }
    assert_eq!(lines.join(" "), words, "no word is lost and none is cut");
}
