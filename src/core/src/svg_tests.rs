//! SVG images (`svg.rs`, `svg_scan.rs`, `svg_entities.rs`, `svg_draw.rs`): what is drawn, what is refused with an error that names the tag,
//! and the limits that stop a file before `usvg` builds anything from it. The documents that draw them are in
//! `svg_render_tests.rs`.

use std::path::PathBuf;

use crate::svg::{self, Drawing};
use crate::{svg_entities, svg_scan};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/assets/images/svg").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn drawing(svg_text: &str) -> Drawing {
    svg::prepare(svg_text.as_bytes()).unwrap_or_else(|e| panic!("{svg_text}: {e}"))
}

fn refused(svg_text: &str) -> String {
    match svg::prepare(svg_text.as_bytes()) {
        Ok(_) => panic!("{svg_text} should be refused"),
        Err(error) => error,
    }
}

/// The content of the picture's form, inflated.
fn content(drawing: &Drawing, form: usize) -> String {
    let inflated = miniz_oxide::inflate::decompress_to_vec_zlib(&drawing.forms[form].stream).expect("a zlib stream");
    String::from_utf8(inflated).expect("content is text")
}

/// How many times the operator `op` is in `content`: the last word of a line.
fn count(content: &str, op: &str) -> usize {
    content.lines().filter(|line| line.split_whitespace().last() == Some(op)).count()
}

/// The operands of each use of the operator `op` in `content`.
fn operands(content: &str, op: &str) -> Vec<Vec<f32>> {
    content
        .lines()
        .filter(|line| line.split_whitespace().last() == Some(op))
        .map(|line| line.split_whitespace().filter_map(|word| word.parse().ok()).collect())
        .collect()
}

/// Whether `found` is `wanted`, to the precision a color is written in.
fn is_color(found: &[f32], wanted: [f32; 3]) -> bool {
    found.len() == 3 && found.iter().zip(wanted).all(|(found, wanted)| (found - wanted).abs() < 0.005)
}

/// An SVG document around `body`.
fn svg(body: &str) -> String {
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100" height="100">{body}</svg>"#)
}

// ── Which bytes are an SVG ────────────────────────────────────────────────────

#[test]
fn recognizes_an_svg_by_its_root_element() {
    assert!(svg::is_svg(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"));
    assert!(svg::is_svg(b"  \n<svg>"));
    assert!(svg::is_svg(b"\xEF\xBB\xBF<svg>"), "a byte order mark");
    assert!(svg::is_svg(b"<?xml version=\"1.0\"?>\n<!-- made by hand -->\n<svg>"));
    assert!(svg::is_svg(b"<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x.dtd\" [ <!ENTITY a \"b\"> ]>\n<svg>"));
}

#[test]
fn does_not_take_other_bytes_for_an_svg() {
    assert!(!svg::is_svg(b"<svgfoo>"), "a root that only starts with svg");
    assert!(!svg::is_svg(b"<html><svg>"));
    assert!(!svg::is_svg(b"\x89PNG\r\n\x1A\n"));
    assert!(!svg::is_svg(b"<!-- never closed <svg>"));
    assert!(!svg::is_svg(b""));
}

// ── The size ──────────────────────────────────────────────────────────────────

#[test]
fn takes_the_size_from_width_and_height() {
    let drawing = drawing(&svg(""));
    assert_eq!((drawing.width, drawing.height), (100.0, 100.0));
}

#[test]
fn takes_the_size_from_the_view_box_when_there_is_no_width_or_height() {
    let drawing = drawing(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 80 30"/>"#);
    assert_eq!((drawing.width, drawing.height), (80.0, 30.0));
}

#[test]
fn sizes_an_svg_with_no_size_at_all_to_what_it_draws() {
    let drawing = drawing(std::str::from_utf8(&fixture("no-size.svg")).unwrap());
    assert_eq!((drawing.width, drawing.height), (60.0, 30.0));
}

#[test]
fn keeps_a_fractional_size_as_it_is_so_a_small_picture_keeps_its_proportions() {
    let drawing = drawing(r#"<svg xmlns="http://www.w3.org/2000/svg" width="10.2" height="0.4"/>"#);
    assert!((drawing.width - 10.2).abs() < 1e-4 && (drawing.height - 0.4).abs() < 1e-4, "{} by {}", drawing.width, drawing.height);
    let drawing = self::drawing(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1.5 1"/>"#);
    assert_eq!((drawing.width, drawing.height), (1.5, 1.0));
}

/// The size of an SVG whose root is `<svg` and `attributes`.
fn size_of(attributes: &str) -> (f32, f32) {
    let drawing = drawing(&format!(r#"<svg xmlns="http://www.w3.org/2000/svg" {attributes}/>"#));
    (drawing.width, drawing.height)
}

fn assert_size(attributes: &str, width: f32, height: f32) {
    let (w, h) = size_of(attributes);
    assert!((w - width).abs() < 0.01 && (h - height).abs() < 0.01, "{attributes}: {w} by {h}, not {width} by {height}");
}

#[test]
fn takes_a_size_in_points_inches_and_millimetres_as_the_size_it_says() {
    // 72 points are an inch, and a point is what the engine draws in: 72pt is 72pt.
    assert_size(r#"width="72pt" height="1in""#, 72.0, 72.0);
    assert_size(r#"width="460.8pt" height="345.6pt""#, 460.8, 345.6);
    assert_size(r#"width="25.4mm" height="2.54cm""#, 72.0, 72.0);
    assert_size(r#"width="6pc" height="1.5pc""#, 72.0, 18.0);
    // An A4 page, as Inkscape writes it.
    assert_size(r#"width="210mm" height="297mm""#, 595.2756, 841.8898);
}

#[test]
fn takes_a_size_in_pixels_or_with_no_unit_as_a_pixel_to_the_point() {
    assert_size(r#"width="96" height="48""#, 96.0, 48.0);
    assert_size(r#"width="96px" height="48px""#, 96.0, 48.0);
    assert_size(r#"viewBox="0 0 96 48""#, 96.0, 48.0);
}

#[test]
fn takes_the_units_of_the_width_for_both_axes_so_the_picture_keeps_its_proportions() {
    // The picture is drawn at the size `usvg` read, and a natural size of another shape would distort it.
    assert_size(r#"width="72pt" height="96px""#, 72.0, 72.0);
    assert_size(r#"width="96px" height="72pt""#, 96.0, 96.0);
    // With no width, or none that is a length, the height gives the units, and the view box the proportions.
    assert_size(r#"height="72pt" viewBox="0 0 100 50""#, 144.0, 72.0);
    assert_size(r#"width="wide" height="72pt" viewBox="0 0 100 50""#, 144.0, 72.0);
    assert_size(r#"height="48" viewBox="0 0 100 50""#, 96.0, 48.0);
}

// ── What is drawn ─────────────────────────────────────────────────────────────

#[test]
fn draws_every_basic_shape_with_its_strokes_dashes_and_fill_rules() {
    let drawing = drawing(std::str::from_utf8(&fixture("shapes.svg")).unwrap());
    assert_eq!(drawing.forms.len(), 1, "no group is faded as a whole");
    let content = content(&drawing, 0);
    for operator in ["c", "l", "m", "h", "S", "f", "B*", "d", "J", "j", "w", "M"] {
        assert!(count(&content, operator) > 0, "{operator:?} in\n{content}");
    }
    // The picture's unit square: the SVG's 200 by 120, flipped.
    let flip = &operands(&content, "cm")[0];
    assert_eq!(flip.len(), 6);
    assert!((flip[0] - 1.0 / 200.0).abs() < 1e-6 && (flip[3] + 1.0 / 120.0).abs() < 1e-6 && flip[5] == 1.0, "{flip:?}");
}

#[test]
fn draws_use_style_clips_and_opacity() {
    let drawing = drawing(std::str::from_utf8(&fixture("structure.svg")).unwrap());
    let content = content(&drawing, 0);
    assert!(count(&content, "W") >= 1, "the clip of the window: {content}");
    // The two tiles are the one rect of `defs`, drawn twice with the class's fill.
    let fills = operands(&content, "rg");
    assert_eq!(fills.iter().filter(|fill| is_color(fill, [23.0 / 255.0, 99.0 / 255.0, 207.0 / 255.0])).count(), 2, "{content}");
    // The warm rect takes its fill from the id rule, which beats the class and `*` rules.
    assert!(fills.iter().any(|fill| is_color(fill, [215.0 / 255.0, 111.0 / 255.0, 4.0 / 255.0])), "{content}");
    // `*` gives every shape round joins.
    assert!(operands(&content, "j").iter().all(|join| join == &[1.0]), "{content}");
}

#[test]
fn fades_a_group_of_several_shapes_as_one() {
    let drawing = drawing(std::str::from_utf8(&fixture("structure.svg")).unwrap());
    // The pair of shapes at half opacity, the same pair at 0.4 through its `use`, and the square with a fill and a
    // stroke at half opacity: each is a form of its own, beside the picture.
    assert_eq!(drawing.forms.len(), 4);
    assert!(drawing.states.contains(&(0.5, 0.5)) && drawing.states.contains(&(0.4, 0.4)), "{:?}", drawing.states);
    assert_eq!(count(&content(&drawing, 0), "Do"), 3, "the picture invokes the three groups");
    for form in &drawing.forms[1..] {
        assert!(form.bbox[2] > form.bbox[0] && form.bbox[3] > form.bbox[1], "{:?}", form.bbox);
    }
}

#[test]
fn fades_a_shape_with_only_a_fill_by_its_own_alpha() {
    let drawing = drawing(&svg(r##"<rect width="10" height="10" fill="#ff0000" opacity="0.25"/>"##));
    assert_eq!(drawing.forms.len(), 1);
    assert_eq!(drawing.states, vec![(0.25, 1.0)]);
}

#[test]
fn fades_a_shape_with_a_fill_and_a_stroke_as_a_group_since_its_stroke_covers_its_fill() {
    let drawing = drawing(&svg(r##"<rect width="10" height="10" fill="#ff0000" stroke="#0000ff" stroke-width="4" opacity="0.5"/>"##));
    assert_eq!(drawing.forms.len(), 2, "the stroke and the fill are faded together");
}

#[test]
fn paints_a_fill_opacity_and_a_stroke_opacity_each_by_its_own_alpha() {
    let drawing = drawing(&svg(
        r##"<rect width="10" height="10" fill="#ff0000" fill-opacity="0.5" stroke="#0000ff" stroke-opacity="0.25"/>"##,
    ));
    assert_eq!(drawing.forms.len(), 1);
    assert_eq!(drawing.states, vec![(0.5, 0.25)]);
}

#[test]
fn draws_a_marker_at_each_vertex() {
    let drawing = drawing(std::str::from_utf8(&fixture("marker.svg")).unwrap());
    // Four vertices, four dots, each a circle of four curves, and the polyline itself.
    assert!(count(&content(&drawing, 0), "c") >= 16);
}

#[test]
fn skips_what_does_not_show_in_a_still_picture() {
    let drawing = drawing(&svg(
        r##"<title>t</title><desc>d</desc><metadata><x:y xmlns:x="urn:x"/></metadata><script>alert(1)</script>
        <rect width="5" height="5" fill="none"/><circle r="3" opacity="0"/><set attributeName="x"/>"##,
    ));
    assert_eq!(count(&content(&drawing, 0), "f"), 0);
}

// ── currentColor ──────────────────────────────────────────────────────────────

#[test]
fn leaves_current_color_to_the_page_and_keeps_the_colors_the_file_writes() {
    let drawing = drawing(std::str::from_utf8(&fixture("current-color.svg")).unwrap());
    assert!(drawing.current_color);
    let content = content(&drawing, 0);
    // Only the red stroke of the square is a color of its own.
    let strokes = operands(&content, "RG");
    assert_eq!(strokes.len(), 1, "{content}");
    assert!(is_color(&strokes[0], [183.0 / 255.0, 28.0 / 255.0, 28.0 / 255.0]), "{content}");
    assert_eq!(count(&content, "rg"), 0, "no fill sets a color: {content}");
}

#[test]
fn says_a_file_that_never_writes_current_color_does_not_use_it() {
    assert!(!drawing(std::str::from_utf8(&fixture("shapes.svg")).unwrap()).current_color);
}

#[test]
fn keeps_the_color_the_root_sets_for_current_color() {
    let drawing = drawing(std::str::from_utf8(&fixture("current-color-root.svg")).unwrap());
    assert!(!drawing.current_color, "the root's color is the file's own");
    let strokes = operands(&content(&drawing, 0), "RG");
    assert!(strokes.len() == 1 && is_color(&strokes[0], [0.0, 131.0 / 255.0, 143.0 / 255.0]), "{strokes:?}");
}

#[test]
fn takes_current_color_from_a_color_a_shape_sets_itself() {
    let drawing = drawing(&svg(r##"<rect width="9" height="9" fill="currentColor" color="#112233"/>"##));
    assert!(!drawing.current_color);
    let fills = operands(&content(&drawing, 0), "rg");
    assert!(fills.len() == 1 && is_color(&fills[0], [17.0 / 255.0, 34.0 / 255.0, 51.0 / 255.0]), "{fills:?}");
}

#[test]
fn keeps_a_color_the_file_writes_that_is_the_first_the_engine_would_stand_in_with() {
    // The engine reads `currentColor` as `#010203` unless the file holds that color, which this one does.
    let drawing = drawing(&svg(r##"<rect width="9" height="9" fill="currentColor"/><rect width="9" height="9" fill="#010203"/>"##));
    assert!(drawing.current_color);
    let fills = operands(&content(&drawing, 0), "rg");
    assert!(fills.len() == 1 && is_color(&fills[0], [1.0 / 255.0, 2.0 / 255.0, 3.0 / 255.0]), "{fills:?}");
}

#[test]
fn draws_each_kind_of_transform() {
    let cases = [
        ("translate(3 4)", [1.0, 0.0, 0.0, 1.0, 3.0, 4.0]),
        ("scale(2 3)", [2.0, 0.0, 0.0, 3.0, 0.0, 0.0]),
        ("rotate(90)", [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]),
        ("skewX(45)", [1.0, 0.0, 1.0, 1.0, 0.0, 0.0]),
        ("skewY(45)", [1.0, 1.0, 0.0, 1.0, 0.0, 0.0]),
        ("matrix(2 0 0 3 5 7)", [2.0, 0.0, 0.0, 3.0, 5.0, 7.0]),
    ];
    for (transform, wanted) in cases {
        let drawing = drawing(&svg(&format!(r#"<g transform="{transform}"><rect width="10" height="10"/></g>"#)));
        // The first matrix is the picture's own, which puts the SVG's space on the unit square.
        let moves = operands(&content(&drawing, 0), "cm");
        let found = &moves[1];
        assert!(found.iter().zip(wanted).all(|(found, wanted)| (found - wanted).abs() < 1e-3), "{transform}: {found:?}");
    }
}

#[test]
fn draws_a_transform_on_a_shape_and_a_use() {
    let drawing = drawing(&svg(
        r##"<defs><rect id="r" width="4" height="4"/></defs><rect width="4" height="4" transform="scale(2)"/><use href="#r" transform="translate(9 9)"/>"##,
    ));
    let moves = operands(&content(&drawing, 0), "cm");
    assert!(moves.iter().any(|m| m == &vec![2.0, 0.0, 0.0, 2.0, 0.0, 0.0]) && moves.iter().any(|m| m == &vec![1.0, 0.0, 0.0, 1.0, 9.0, 9.0]), "{moves:?}");
}

#[test]
fn draws_the_stroke_joins_caps_and_miter_limit() {
    let line = |attributes: &str| format!(r##"<path d="M0 0 L9 9 L0 18" fill="none" stroke="#000" {attributes}/>"##);
    for (attributes, operator, wanted) in [
        (r#"stroke-linejoin="miter""#, "j", 0.0),
        (r#"stroke-linejoin="round""#, "j", 1.0),
        (r#"stroke-linejoin="bevel""#, "j", 2.0),
        (r#"stroke-linecap="butt""#, "J", 0.0),
        (r#"stroke-linecap="round""#, "J", 1.0),
        (r#"stroke-linecap="square""#, "J", 2.0),
        (r#"stroke-miterlimit="2""#, "M", 2.0),
        (r#"stroke-miterlimit="7.5""#, "M", 7.5),
        (r#"stroke-width="3""#, "w", 3.0),
    ] {
        let drawing = drawing(&svg(&line(attributes)));
        assert_eq!(operands(&content(&drawing, 0), operator), vec![vec![wanted]], "{attributes}");
    }
}

#[test]
fn draws_dashes_with_their_offset() {
    let drawing = drawing(&svg(r##"<path d="M0 0 L50 0" fill="none" stroke="#000" stroke-dasharray="6 3" stroke-dashoffset="2"/>"##));
    assert!(content(&drawing, 0).contains("[6 3] 2 d"), "{}", content(&drawing, 0));
}

#[test]
fn reads_each_way_to_write_a_color() {
    let fill = |paint: &str| {
        let drawing = drawing(&svg(&format!(r#"<rect width="5" height="5" fill="{paint}"/>"#)));
        operands(&content(&drawing, 0), "rg").pop().unwrap_or_default()
    };
    assert!(is_color(&fill("#f00"), [1.0, 0.0, 0.0]));
    assert!(is_color(&fill("#336699"), [0.2, 0.4, 0.6]));
    assert!(is_color(&fill("rgb(0, 128, 255)"), [0.0, 128.0 / 255.0, 1.0]));
    assert!(is_color(&fill("rgb(100%, 0%, 50%)"), [1.0, 0.0, 0.5]));
    assert!(is_color(&fill("teal"), [0.0, 128.0 / 255.0, 128.0 / 255.0]));
    assert!(is_color(&fill("RED"), [1.0, 0.0, 0.0]));
    // `none` paints nothing, and a shape with neither a fill nor a stroke draws nothing.
    let none = drawing(&svg(r#"<rect width="5" height="5" fill="none"/>"#));
    assert_eq!(count(&content(&none, 0), "f") + count(&content(&none, 0), "S"), 0);
}

#[test]
fn reads_the_properties_a_style_attribute_sets() {
    let drawing = drawing(&svg(
        r##"<rect width="9" height="9" style="fill:#00ff00; stroke:#ff0000; stroke-width:3; stroke-linejoin:bevel; stroke-miterlimit:2; stroke-linecap:round; fill-opacity:0.5"/>"##,
    ));
    let content = content(&drawing, 0);
    assert!(is_color(&operands(&content, "rg")[0], [0.0, 1.0, 0.0]) && is_color(&operands(&content, "RG")[0], [1.0, 0.0, 0.0]), "{content}");
    assert_eq!(operands(&content, "w"), vec![vec![3.0]]);
    assert_eq!((operands(&content, "j"), operands(&content, "J"), operands(&content, "M")), (vec![vec![2.0]], vec![vec![1.0]], vec![vec![2.0]]));
    assert_eq!(drawing.states, vec![(0.5, 1.0)]);
}

#[test]
fn lets_a_style_attribute_beat_a_presentation_attribute_and_a_style_block_beat_both() {
    let drawing = drawing(&svg(
        r##"<style>.c { fill: #0000ff }</style><rect class="c" width="5" height="5" fill="#ff0000" style="stroke:#00ff00"/><rect width="5" height="5" fill="#ff0000" style="fill:#00ff00"/>"##,
    ));
    let fills = operands(&content(&drawing, 0), "rg");
    assert!(is_color(&fills[0], [0.0, 0.0, 1.0]) && is_color(&fills[1], [0.0, 1.0, 0.0]), "{fills:?}");
}

// ── What is refused ───────────────────────────────────────────────────────────

#[test]
fn refuses_text_and_names_the_tag_and_the_line() {
    let error = refused("<svg xmlns=\"http://www.w3.org/2000/svg\">\n  <text x=\"1\" y=\"2\">Hi</text>\n</svg>");
    assert!(error.contains("<text> at line 2, col 3"), "{error}");
    assert!(error.contains("convert the text to outlines"), "{error}");
}

#[test]
fn refuses_an_image_inside_an_svg() {
    let error = refused(&svg(r#"<image href="data:image/png;base64,AAAA" width="10" height="10"/>"#));
    assert!(error.contains("<image> at line 1"), "{error}");
}

#[test]
fn refuses_a_foreign_object() {
    assert!(refused(&svg("<foreignObject width=\"5\" height=\"5\"/>")).contains("<foreignObject>"));
}

#[test]
fn refuses_a_use_of_another_file_and_an_import_and_a_url_outside_the_file() {
    assert!(refused(&svg(r#"<use href="other.svg#a"/>"#)).contains("another file or a URL"));
    assert!(refused(&svg(r#"<use xlink:href="https://example.com/a.svg#a"/>"#)).contains("<use>"));
    assert!(refused(&svg("<style>@import url(https://example.com/a.css);</style>")).contains("@import"));
    assert!(refused(&svg(r#"<rect width="5" height="5" fill="url(https://example.com/a.svg#g)"/>"#)).contains("`url(`"));
    assert!(refused(&svg(r#"<style>rect { fill: url( 'other.svg#g' ) }</style>"#)).contains("<style>"));
}

#[test]
fn does_not_refuse_what_is_only_named_in_a_title_or_a_vocabulary_of_another_tool() {
    drawing(&svg(
        r#"<title><text>Not a text</text></title><metadata><text xmlns="urn:other"/></metadata><rect width="5" height="5"/>"#,
    ));
}

#[test]
fn refuses_a_mask_a_filter_and_a_pattern_that_are_used_and_names_them() {
    let mask = refused(&svg(
        r##"<mask id="m"><rect width="5" height="5" fill="#fff"/></mask><rect id="r" width="9" height="9" mask="url(#m)"/>"##,
    ));
    assert!(mask.contains("<mask> at line 1"), "{mask}");
    let filter = refused(&svg(
        r##"<filter id="blur"><feGaussianBlur stdDeviation="2"/></filter><rect width="9" height="9" filter="url(#blur)"/>"##,
    ));
    assert!(filter.contains("<filter>"), "{filter}");
    let pattern = refused(&svg(
        r##"<pattern id="p" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="2"/></pattern><rect width="9" height="9" fill="url(#p)"/>"##,
    ));
    assert!(pattern.contains("<pattern>"), "{pattern}");
}

#[test]
fn refuses_a_gradient_that_is_used_until_gradients_are_drawn() {
    let linear = refused(&svg(
        r##"<defs><linearGradient id="g"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/></linearGradient></defs><rect width="9" height="9" fill="url(#g)"/>"##,
    ));
    assert!(linear.contains("<linearGradient>") && linear.contains("yet"), "{linear}");
    let radial = refused(&svg(
        r##"<defs><radialGradient id="g"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/></radialGradient></defs><circle r="9" stroke="url(#g)" fill="none"/>"##,
    ));
    assert!(radial.contains("<radialGradient>"), "{radial}");
}

#[test]
fn draws_a_file_that_defines_a_mask_a_filter_a_pattern_and_a_gradient_it_never_uses() {
    drawing(&svg(
        r##"<defs><mask id="m"><rect width="5" height="5"/></mask><filter id="f"><feOffset dx="1"/></filter>
        <pattern id="p" width="2" height="2"><rect width="1" height="1"/></pattern>
        <linearGradient id="g"><stop offset="0" stop-color="#000"/></linearGradient></defs><rect width="5" height="5"/>"##,
    ));
}

#[test]
fn refuses_a_blend_mode() {
    let error = refused(&svg(r##"<rect width="9" height="9" fill="#f00" style="mix-blend-mode: multiply"/>"##));
    assert!(error.contains("mix-blend-mode"), "{error}");
}

#[test]
fn refuses_a_clip_path_whose_shapes_mix_fill_rules() {
    let error = refused(&svg(
        r##"<clipPath id="c"><rect width="5" height="5" clip-rule="evenodd"/><rect x="9" width="5" height="5"/></clipPath><rect width="30" height="30" clip-path="url(#c)"/>"##,
    ));
    assert!(error.contains("clip-rule") && error.contains("<clipPath>"), "{error}");
}

#[test]
fn refuses_a_clip_path_on_a_child_of_a_clip_path() {
    let error = refused(&svg(
        r##"<clipPath id="inner"><rect width="3" height="3"/></clipPath>
        <clipPath id="c"><rect width="5" height="5" clip-path="url(#inner)"/></clipPath><rect width="30" height="30" clip-path="url(#c)"/>"##,
    ));
    assert!(error.contains("clip-path on a child of a <clipPath>"), "{error}");
}

#[test]
fn draws_a_clip_path_whose_shapes_share_a_fill_rule_and_a_clip_path_with_a_clip_path_of_its_own() {
    let drawing = drawing(&svg(
        r##"<clipPath id="outer"><rect width="20" height="20"/></clipPath>
        <clipPath id="c" clip-path="url(#outer)" clip-rule="evenodd"><rect width="5" height="5" clip-rule="evenodd"/><rect x="9" width="5" height="5" clip-rule="evenodd"/></clipPath>
        <rect width="30" height="30" clip-path="url(#c)"/>"##,
    ));
    let content = content(&drawing, 0);
    assert_eq!(count(&content, "W") + count(&content, "W*"), 2, "{content}");
}

#[test]
fn says_malformed_xml_and_bytes_that_are_no_text() {
    assert!(refused("<svg xmlns=\"http://www.w3.org/2000/svg\"><g></svg>").starts_with("the SVG could not be read: "));
    let error = svg::prepare(b"<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- \xFF\xFE -->").unwrap_err();
    assert_eq!(error, "the SVG is not UTF-8 text");
}

#[test]
fn reads_an_svg_with_a_doctype_and_entities() {
    let text = r##"<?xml version="1.0"?><!DOCTYPE svg [ <!ENTITY red "#ff0000"> ]>
        <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="5" height="5" fill="&red;"/></svg>"##;
    assert!(content(&drawing(text), 0).contains("1 0 0 rg\n"));
}

// ── The limits ────────────────────────────────────────────────────────────────

/// An SVG of `levels` groups, each holding `fan` uses of the one before, over a single rect: `fan` to the power of
/// `levels` rects once the uses are followed.
fn use_tower(levels: usize, fan: usize) -> String {
    let mut defs = String::from(r#"<rect id="l0" width="1" height="1"/>"#);
    for level in 1..=levels {
        let uses = format!(r##"<use href="#l{}"/>"##, level - 1).repeat(fan);
        defs.push_str(&format!(r#"<g id="l{level}">{uses}</g>"#));
    }
    svg(&format!(r##"<defs>{defs}</defs><use href="#l{levels}"/>"##))
}

#[test]
fn refuses_a_use_bomb_before_usvg_builds_it() {
    // A kilobyte of `use` that stands for 10 to the sixth rects.
    let bomb = use_tower(6, 10);
    assert!(bomb.len() < 1_500, "the file is small: {} bytes", bomb.len());
    let error = refused(&bomb);
    assert!(error.contains("more than 100000 elements") && error.contains("the limit is 100000"), "{error}");
}

#[test]
fn draws_a_tower_of_use_under_the_limit() {
    let drawing = drawing(&use_tower(3, 10));
    assert_eq!(count(&content(&drawing, 0), "f"), 1_000);
}

#[test]
fn refuses_a_use_that_refers_to_itself() {
    let error = refused(&svg(r##"<g id="a"><use href="#a"/></g>"##));
    assert!(error.contains("refers to itself"), "{error}");
}

#[test]
fn refuses_a_chain_of_use_deeper_than_the_limit_and_a_document_nested_deeper_than_it() {
    let chain: String =
        (0..150).map(|i| format!(r##"<g id="c{i}"><use href="#c{}"/></g>"##, i + 1)).collect::<String>() + r#"<rect id="c150" width="1" height="1"/>"#;
    let error = refused(&svg(&format!(r##"{chain}<use href="#c0"/>"##)));
    assert!(error.contains("levels deep, past the limit of 100"), "{error}");
    let nested = format!("{}{}", "<g>".repeat(101), "</g>".repeat(101));
    let error = refused(&svg(&nested));
    assert!(error.contains("<g> at line 1") && error.contains("past the limit of 100"), "{error}");
}

#[test]
fn draws_a_document_nested_as_deep_as_the_limit_allows() {
    // The root is the first level.
    drawing(&svg(&format!("{}<rect width=\"1\" height=\"1\"/>{}", "<g>".repeat(98), "</g>".repeat(98))));
}

#[test]
fn refuses_more_path_commands_than_the_limit() {
    let path = format!("M0 0{}", " l1 1".repeat(1_000_001));
    let error = refused(&svg(&format!(r#"<path d="{path}"/>"#)));
    assert!(error.contains("more than 1000000 path commands"), "{error}");
}

#[test]
fn counts_the_segments_a_command_repeats_on_as_commands() {
    // One `M` and a million pairs of numbers after it, each pair a line.
    let path = format!("M0 0 {}", "1 1 ".repeat(1_000_001));
    let error = refused(&svg(&format!(r#"<path d="{path}"/>"#)));
    assert!(error.contains("path commands"), "{error}");
}

#[test]
fn counts_the_path_commands_of_each_use_copy() {
    // 1,000 lines, used 1,001 times.
    let path = format!("M0 0{}", " l1 1".repeat(1_000));
    let uses = r##"<use href="#p"/>"##.repeat(1_001);
    let error = refused(&svg(&format!(r##"<defs><path id="p" d="{path}"/></defs>{uses}"##)));
    assert!(error.contains("path commands"), "{error}");
}

#[test]
fn counts_a_marker_at_each_vertex_of_its_path() {
    // 1,000 vertices and a marker of 200 shapes: 200,000 shapes.
    let marker = r#"<circle r="1"/>"#.repeat(200);
    let path = format!("M0 0{}", " l1 1".repeat(999));
    let error = refused(&svg(&format!(
        r##"<defs><marker id="m">{marker}</marker></defs><path d="{path}" marker-mid="url(#m)"/>"##
    )));
    assert!(error.contains("more than 100000 elements"), "{error}");
}

#[test]
fn counts_a_clip_path_for_each_shape_that_refers_to_it() {
    // 40,000 shapes, each clipped by a clip path of 3 shapes.
    let clip = r#"<clipPath id="c"><rect width="5" height="5"/><rect width="5" height="5"/><rect width="5" height="5"/></clipPath>"#;
    let shapes = r##"<rect width="2" height="2" clip-path="url(#c)"/>"##.repeat(40_000);
    let error = refused(&svg(&format!("{clip}{shapes}")));
    assert!(error.contains("more than 100000 elements"), "{error}");
}

#[test]
fn counts_what_a_style_sheet_refers_to_for_every_shape() {
    let clip = r#"<clipPath id="c"><rect width="5" height="5"/><rect width="5" height="5"/><rect width="5" height="5"/></clipPath>"#;
    let shapes = r#"<rect width="2" height="2"/>"#.repeat(40_000);
    let error = refused(&svg(&format!("<style>rect {{ clip-path: url(#c) }}</style>{clip}{shapes}")));
    assert!(error.contains("more than 100000 elements"), "{error}");
}

#[test]
fn refuses_more_xml_nodes_than_the_limit_as_the_file_is_read() {
    let error = refused(&svg(&"<g/>".repeat(400_001)));
    assert!(error.contains("more than 400000 XML nodes"), "{error}");
}

#[test]
fn draws_a_map_of_many_shapes_under_the_limits() {
    let drawing = drawing(&svg(&r##"<path d="M0 0 L5 5 L9 0 Z" fill="#336699"/>"##.repeat(30_000)));
    assert_eq!(count(&content(&drawing, 0), "f"), 30_000);
}

#[test]
fn counts_a_chain_of_use_whose_lower_elements_come_first_in_the_file() {
    // Each element is counted once, so a chain written bottom-up is met through counts already made: its depth must
    // be kept with them, or a chain of any length is read, and `usvg` nests as deep.
    let chain: String = (1..=150).map(|i| format!(r##"<use id="c{i}" href="#c{}"/>"##, i - 1)).collect();
    let error = refused(&svg(&format!(r#"<rect id="c0" width="1" height="1"/>{chain}"#)));
    assert!(error.contains("levels deep, past the limit of 100"), "{error}");
    let chain: String = (1..=90).map(|i| format!(r##"<use id="c{i}" href="#c{}"/>"##, i - 1)).collect();
    drawing(&svg(&format!(r#"<rect id="c0" width="1" height="1"/>{chain}"#)));
}

#[test]
fn refuses_an_fe_image_that_names_a_file_or_holds_a_picture() {
    for href in ["/etc/hosts", "other.svg", "data:image/svg+xml;base64,PHN2Zy8+"] {
        let error = refused(&svg(&format!(
            r##"<filter id="f"><feImage href="{href}"/></filter><rect width="9" height="9" filter="url(#f)"/>"##
        )));
        assert!(error.contains("<feImage>") && error.contains("another file"), "{href}: {error}");
    }
}

#[test]
fn gives_usvg_no_way_to_read_a_file_or_a_data_url() {
    assert!(svg::resolves_no_picture());
}

#[test]
fn refuses_entities_that_expand_past_the_limit_from_a_file_of_two_kilobytes() {
    // 100 uses of an entity of 255 uses of one of 1,000 characters: 25 MB in one attribute.
    let text = format!(
        r#"<?xml version="1.0"?><!DOCTYPE svg [ <!ENTITY a "{}"> <!ENTITY b "{}"> ]>
        <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><g id="{}"/></svg>"#,
        "x".repeat(1_000),
        "&a;".repeat(255),
        "&b;".repeat(100)
    );
    assert!(text.len() < 3_000, "{} bytes", text.len());
    let error = refused(&text);
    assert!(error.contains("entities expand to more than 1000000 bytes"), "{error}");
}

#[test]
fn refuses_an_entity_that_names_a_file_and_a_parameter_entity() {
    let external = r#"<!DOCTYPE svg [ <!ENTITY x SYSTEM "file:///etc/hosts"> ]><svg xmlns="http://www.w3.org/2000/svg"/>"#;
    assert!(refused(external).contains("refers to another file"));
    let parameter = r#"<!DOCTYPE svg [ <!ENTITY % p "x"> ]><svg xmlns="http://www.w3.org/2000/svg"/>"#;
    assert!(refused(parameter).contains("parameter entity"));
}

#[test]
fn reads_the_entities_a_drawing_program_writes() {
    // Illustrator's old exports name their namespaces with entities.
    let text = r##"<?xml version="1.0"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd" [
    <!ENTITY ns_svg "http://www.w3.org/2000/svg">
    <!ENTITY ns_xlink "http://www.w3.org/1999/xlink">
    <!ENTITY red "#ff0000">
]>
<svg xmlns="&ns_svg;" xmlns:xlink="&ns_xlink;" width="10" height="10"><rect width="5" height="5" fill="&red;"/></svg>"##;
    assert_eq!(operands(&content(&drawing(text), 0), "rg"), vec![vec![1.0, 0.0, 0.0]]);
}

#[test]
fn reads_a_doctype_with_no_entities_and_one_with_nested_entities_inside_the_limit() {
    drawing(r#"<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "svg11.dtd"><svg xmlns="http://www.w3.org/2000/svg"/>"#);
    let nested = r##"<!DOCTYPE svg [ <!ENTITY a "0123456789"> <!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;"> ]>
        <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><g id="&b;&b;&b;&b;"/></svg>"##;
    drawing(nested);
}

#[test]
fn refuses_a_clip_path_whose_shapes_overlap_since_one_path_would_leave_a_hole_where_they_meet() {
    let clip = |a: &str, b: &str, rule: &str| {
        svg(&format!(
            r##"<clipPath id="c"><path d="{a}"{rule}/><path d="{b}"{rule}/></clipPath><rect width="30" height="30" clip-path="url(#c)"/>"##
        ))
    };
    // The second square winds the other way: under the nonzero rule their overlap would wind to 0.
    let error = refused(&clip("M0 0H10V10H0Z", "M5 5V15H15V5Z", ""));
    assert!(error.contains("<clipPath> whose shapes overlap"), "{error}");
    // The same under the even-odd rule, where any overlap is a hole.
    let error = refused(&clip("M0 0H10V10H0Z", "M5 5H15V15H5Z", r#" clip-rule="evenodd""#));
    assert!(error.contains("overlap"), "{error}");
    // Shapes that don't overlap, or only touch, are one clip.
    drawing(&clip("M0 0H10V10H0Z", "M12 0H20V10H12Z", ""));
    drawing(&clip("M0 0H10V10H0Z", "M10 0H20V10H10Z", ""));
}

/// The file of two kilobytes whose entities expand to 25 MB, with `declaration` for its XML declaration, `before` ahead
/// of its DOCTYPE and `inside` ahead of its declarations.
fn entity_bomb(declaration: &str, before: &str, inside: &str) -> String {
    format!(
        r#"{declaration}{before}<!DOCTYPE svg [ {inside} <!ENTITY a "{}"> <!ENTITY b "{}"> ]>
        <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><g id="{}"/></svg>"#,
        "x".repeat(1_000),
        "&a;".repeat(255),
        "&b;".repeat(100)
    )
}

#[test]
fn bounds_the_entities_however_the_file_hides_its_doctype_or_its_declarations() {
    let plain = r#"<?xml version="1.0"?>"#;
    for (declaration, before, inside) in [
        (plain, "", ""),
        // A DOCTYPE in a comment is none, and the real one is the next.
        (plain, "<!-- <!DOCTYPE x> -->", ""),
        (plain, "<!-- <!DOCTYPE x [ <!ENTITY a \"y\"> ]> -->\n<?pi <!DOCTYPE z> ?>", ""),
        // A comment `<!-->` goes on to the next `-->`, and holds a DOCTYPE of its own.
        (plain, "<!--> <!DOCTYPE x [ <!ENTITY a \"y\"> ]> -->", ""),
        // The values of the XML declaration are quoted, and may hold a `?>`.
        (r#"<?xml version="1.0" encoding="?>"?>"#, "", ""),
        // An entity declared in a comment, or a processing instruction, is none, and the real one comes after it.
        (plain, "", "<!-- <!ENTITY a \"x\"> -->"),
        (plain, "", "<?pi <!ENTITY a \"x\"> ?>"),
        // A declaration of an element, an attribute list or a notation ends at its first `>`, whatever quotes it holds,
        // and its quote is not one the entities after it can be read from.
        (plain, "", "<!ELEMENT e '>"),
        (plain, "", "<!ELEMENT e \">"),
        (plain, "", "<!ATTLIST svg x CDATA \">"),
        (plain, "", "<!NOTATION n \"[]]>"),
    ] {
        let text = entity_bomb(declaration, before, inside);
        assert!(svg_scan::parse_xml(&text).is_ok(), "the XML reader should read {before} {inside}");
        let error = svg::prepare(text.as_bytes()).expect_err("the bomb must be refused");
        assert!(error.contains("entities expand to more than 1000000 bytes"), "{before} {inside}: {error}");
    }
}

#[test]
fn does_not_take_a_prolog_the_xml_reader_refuses_for_one_it_reads() {
    let plain = r#"<?xml version="1.0"?>"#;
    // What our reader can't read is refused by it, with its own error, and not left to the XML reader that comes next.
    let ours = "its prolog is not well formed";
    for (before, inside) in [
        // A declaration the tokenizer has no name for, and a stray quote that ends the one before it.
        ("", "<!BOGUS>"),
        ("", "<!ATTLIST svg x CDATA \"<!ENTITY a 'x'>\">"),
        // A comment that doesn't end, and an entity with no `>`.
        ("<!-- ", ""),
        ("", "<!ENTITY c \"x\""),
    ] {
        let text = entity_bomb(plain, before, inside);
        assert!(svg_scan::parse_xml(&text).is_err(), "the XML reader should refuse {before} {inside}");
        let error = svg::prepare(text.as_bytes()).expect_err("the file must be refused");
        assert!(error.contains(ours), "{before} {inside}: {error}");
    }
    // White space of another kind is none: U+00A0 before the DOCTYPE is no gap that lets a DOCTYPE be read.
    let text = entity_bomb(plain, "\u{a0}", "");
    assert!(svg_scan::parse_xml(&text).is_err());
    let error = svg::prepare(text.as_bytes()).expect_err("the file must be refused");
    assert!(error.contains("could not be read") && !error.contains("entities expand"), "{error}");
}

/// Numbers from a fixed seed, so a run of the test is the same each time.
struct Dice(u64);

impl Dice {
    fn below(&mut self, count: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as usize % count
    }

    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[self.below(from.len())]
    }
}

/// What a prolog may hold where our reader and the XML reader could part: quotes and `>` inside declarations, comments
/// that end early or never, instructions, white space of other kinds, declarations of the same name twice, and
/// entities that name each other, a file or a parameter.
const PROLOG_PIECES: &[&str] = &[
    " ",
    "\n",
    "\u{a0}",
    "'",
    "\"",
    ">",
    "<",
    "]",
    "%",
    "-->",
    "<!ELEMENT e ANY>",
    "<!ELEMENT e '>",
    "<!ELEMENT e \">",
    "<!ATTLIST e a CDATA \">",
    "<!ATTLIST e a CDATA 'x'>",
    "<!NOTATION n \"[]]>",
    "<!NOTATION n SYSTEM 'a'>",
    "<!-- c -->",
    "<!-->",
    "<!--->",
    "<!-- <!ENTITY z 'v'> -->",
    "<!--<!DOCTYPE d [ <!ENTITY z 'v'>]>-->",
    "<? p ?>",
    "<?p <!ENTITY z 'v'> ?>",
    "<?>",
    "<!ENTITY q 'v'>",
    "<!ENTITY q \"<!ENTITY z 'v'>\">",
    "<!ENTITY z 'small'>",
    "<!ENTITY y '&z;'>",
    "<!ENTITY s SYSTEM 'f'>",
    "<!ENTITY % p 'x'>",
    "<!ENTITY",
    "<!ENTITY z",
];

/// The declarations of the entity every file of the next test refers to, whose value is `PAYLOAD`.
const PAYLOAD_DECLARATIONS: &[&str] = &[
    "<!ENTITY z \"PAYLOAD\">",
    "<!ENTITY z 'PAYLOAD'>",
    "<!ENTITY  z\t\"PAYLOAD\"\n>",
    "<!ENTITY z\u{1680} \"PAYLOAD\">",
];

#[test]
fn reads_a_prolog_as_the_xml_reader_does_whatever_its_declarations_hold() {
    const LIMIT: u64 = 100;
    let payload = "x".repeat(2 * LIMIT as usize);
    let mut dice = Dice(0x2545_f491_4f6c_dd1d);
    let mut read_and_grown = 0;
    for _ in 0..100_000 {
        let declaration = dice.pick(&["", r#"<?xml version="1.0"?>"#, r#"<?xml version="1.0" encoding="?>"?>"#]);
        let before: String = (0..dice.below(3)).map(|_| dice.pick(PROLOG_PIECES)).collect();
        let head = dice.pick(&["<!DOCTYPE svg [", "<!DOCTYPE svg PUBLIC \"p\" \"a[>.dtd\" [", "<!DOCTYPE\tsvg\n["]);
        let mut inside: Vec<&str> = (0..1 + dice.below(6)).map(|_| dice.pick(PROLOG_PIECES)).collect();
        inside.insert(dice.below(inside.len() + 1), dice.pick(PAYLOAD_DECLARATIONS));
        let close = dice.pick(&["]>", "] >", "]\n>"]);
        let reference = dice.pick(&["&z;", "&y;", "&z\u{1680};"]);
        let text = format!(
            r#"{declaration}{before}{head}{}{close}<svg xmlns="http://www.w3.org/2000/svg" id="{reference}"/>"#,
            inside.concat()
        )
        .replace("PAYLOAD", &payload);

        // A file the XML reader refuses is refused whatever we say. One it reads, we read to the same entities.
        let Ok(doc) = svg_scan::parse_xml(&text) else { continue };
        let grown = doc.root_element().attribute("id").map_or(0, str::len) as u64;
        let verdict = svg_entities::check_within(&text, LIMIT);
        // We may refuse a file the XML reader reads, for its entities or for one that names a file or a parameter, but
        // not for being unreadable.
        if let Err(error) = &verdict {
            assert!(!error.contains("could not be read"), "the XML reader reads {text}: {error}");
        }
        if grown > LIMIT {
            read_and_grown += 1;
            assert!(verdict.is_err(), "the XML reader built {grown} bytes from {text}");
        }
    }
    // The generator reaches the case under test, and not by a rare chance.
    assert!(read_and_grown > 1_000, "{read_and_grown} files the XML reader grew a payload from");
}

#[test]
fn reads_an_entity_whose_value_holds_a_bracket_a_greater_than_and_a_comment_mark() {
    let text = r##"<!-- made by hand -->
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://example.com/[x].dtd" [
    <!-- an entity of the namespace -->
    <!ENTITY ns "http://www.w3.org/2000/svg">
    <!ENTITY odd "a]>b<!-- c">
]>
<svg xmlns="&ns;" width="10" height="10"><rect width="5" height="5" id="&odd;"/></svg>"##;
    drawing(text);
}

#[test]
fn refuses_a_parameter_entity_wherever_the_subset_uses_one() {
    let used = r#"<!DOCTYPE svg [ <!ENTITY a "x"> %a; ]><svg xmlns="http://www.w3.org/2000/svg"/>"#;
    assert!(refused(used).contains("parameter entity"));
}
