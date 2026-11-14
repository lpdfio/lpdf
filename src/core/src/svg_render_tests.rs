//! SVG images in documents (`svg.rs`): drawn as vector wherever an image goes, shared when used again, painted in the
//! color of the text around them when they say `currentColor`, and refused with an error that names the image. The
//! drawing itself is in `svg_tests.rs`.

use std::path::PathBuf;

use crate::pdf::ImageRegistry;
use crate::ruling_render_tests::streams;
use crate::{LpdfEngine, RenderEncryption};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/assets/images/svg").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A document whose `<assets>` declare `images` (named as their files are) and whose first section is `section`.
fn document(images: &[&str], section: &str) -> String {
    let assets: String = images.iter().map(|name| format!(r#"<image name="{name}" src="{name}"/>"#)).collect();
    format!(r#"<lpdf version="1"><assets>{assets}</assets><document size="a4">{section}</document></lpdf>"#)
}

fn render(images: &[(&str, Vec<u8>)], section: &str) -> Result<Vec<u8>, String> {
    let mut engine = LpdfEngine::new("");
    for (name, bytes) in images {
        engine.load_image(name, bytes);
    }
    let names: Vec<&str> = images.iter().map(|(name, _)| *name).collect();
    engine.render_xml(&document(&names, section), None, &RenderEncryption::default())
}

fn rendered(images: &[(&str, Vec<u8>)], section: &str) -> Vec<u8> {
    render(images, section).unwrap_or_else(|e| panic!("renders: {e}"))
}

fn text(pdf: &[u8]) -> String {
    String::from_utf8_lossy(pdf).into_owned()
}

/// The content stream of the first page.
fn page_content(pdf: &[u8]) -> String {
    streams(pdf).into_iter().find(|stream| stream.contains(" Do")).expect("a content stream that draws an image")
}

/// The operands of each use of the operator `op` in `content`.
fn operands(content: &str, op: &str) -> Vec<Vec<f32>> {
    content
        .lines()
        .filter(|line| line.split_whitespace().last() == Some(op))
        .map(|line| line.split_whitespace().filter_map(|word| word.parse().ok()).collect())
        .collect()
}

fn is_color(found: &[f32], wanted: [f32; 3]) -> bool {
    found.len() == 3 && found.iter().zip(wanted).all(|(found, wanted)| (found - wanted).abs() < 0.005)
}

const LOGO: &str = r#"<layout margin="40pt"><img name="logo" width="100pt"/></layout>"#;

// ── Where an SVG goes ─────────────────────────────────────────────────────────

#[test]
fn draws_an_img_as_a_form_of_the_unit_square_and_not_as_an_image() {
    let pdf = rendered(&[("logo", fixture("shapes.svg"))], &format!("<section>{LOGO}</section>"));
    let text = text(&pdf);
    assert!(text.contains("/Subtype /Form") && text.contains("/BBox [0 0 1 1]"), "the picture is a form of the unit square");
    assert!(!text.contains("/Subtype /Image"), "no raster image");
}

#[test]
fn scales_the_unit_square_to_the_size_the_svg_has() {
    // 200 by 120 pixels, drawn 100pt wide: 60pt high.
    let pdf = rendered(&[("logo", fixture("shapes.svg"))], &format!("<section>{LOGO}</section>"));
    let scale = &operands(&page_content(&pdf), "cm")[0];
    assert!((scale[0] - 100.0).abs() < 0.01 && (scale[3] - 60.0).abs() < 0.01, "{scale:?}");
}

#[test]
fn keeps_the_proportions_of_the_view_box_inside_a_box_it_does_not_fill() {
    // The picture is 200 by 100 (a view box of 100 by 50 at twice the size): a 60pt square box holds it 60 by 30.
    let pdf = rendered(
        &[("logo", fixture("view-box.svg"))],
        r#"<section><layout margin="40pt"><img name="logo" width="60pt" height="60pt"/></layout></section>"#,
    );
    let scale = &operands(&page_content(&pdf), "cm")[0];
    assert!((scale[0] - 60.0).abs() < 0.01 && (scale[3] - 30.0).abs() < 0.01, "{scale:?}");
}

#[test]
fn keeps_the_proportions_of_a_small_fractional_size() {
    // 10.2 by 0.4: whole pixels would make it 11 by 1, a picture almost three times too high.
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10.2" height="0.4"><rect width="10.2" height="0.4"/></svg>"#.to_vec();
    let pdf = rendered(&[("logo", svg)], r#"<section><layout margin="40pt"><img name="logo" width="102pt"/></layout></section>"#);
    let scale = &operands(&page_content(&pdf), "cm")[0];
    assert!((scale[0] - 102.0).abs() < 0.01 && (scale[3] - 4.0).abs() < 0.01, "{scale:?}");
}

#[test]
fn sizes_an_svg_with_no_size_to_what_it_draws() {
    // 60 by 30 pixels: its natural size, which an `<img>` with no width takes as it takes any picture's: a pixel a point.
    let pdf = rendered(
        &[("logo", fixture("no-size.svg"))],
        r#"<section><layout margin="40pt"><img name="logo"/></layout></section>"#,
    );
    let scale = &operands(&page_content(&pdf), "cm")[0];
    assert!((scale[0] - 60.0).abs() < 0.01 && (scale[3] - 30.0).abs() < 0.01, "{scale:?}");
}

#[test]
fn sizes_an_svg_in_points_and_millimetres_as_it_says_when_the_img_has_no_width() {
    // 72 by 36 points, written in points and in millimetres: drawn 72pt wide, not 96.
    for (unit_width, unit_height) in [("72pt", "36pt"), ("25.4mm", "12.7mm"), ("1in", "0.5in")] {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{unit_width}" height="{unit_height}"><rect width="10" height="10"/></svg>"#
        );
        let pdf = rendered(
            &[("logo", svg.into_bytes())],
            r#"<section><layout margin="40pt"><img name="logo"/></layout></section>"#,
        );
        let scale = &operands(&page_content(&pdf), "cm")[0];
        assert!((scale[0] - 72.0).abs() < 0.01 && (scale[3] - 36.0).abs() < 0.01, "{unit_width}: {scale:?}");
    }
}

#[test]
fn draws_an_svg_in_a_header_a_layer_and_a_markdown_file_too() {
    let logo = fixture("current-color.svg");
    for (place, section) in [
        (
            "a region pinned to the top",
            r#"<section><layout margin="40pt"><region pin="top"><img name="logo" width="24pt"/></region><text>Body</text></layout></section>"#,
        ),
        (
            "a region pinned to the bottom",
            r#"<section><layout margin="40pt"><region pin="bottom"><img name="logo" width="24pt"/></region><text>Body</text></layout></section>"#,
        ),
        (
            "a layer",
            r#"<section><layout margin="40pt"><text>Body</text></layout><layer><img-at name="logo" x="100pt" y="100pt" width="24pt"/></layer></section>"#,
        ),
        (
            "a markdown file",
            r#"<section><markdown><markdown-content><![CDATA[Before

![the logo](logo)

After]]></markdown-content></markdown></section>"#,
        ),
    ] {
        let pdf = rendered(&[("logo", logo.clone())], section);
        let text = text(&pdf);
        assert!(text.contains("/BBox [0 0 1 1]") && !text.contains("/Subtype /Image"), "{place}");
        assert!(page_content(&pdf).contains("/I0 Do"), "{place}");
    }
}

#[test]
fn writes_one_form_for_an_svg_however_often_it_is_drawn() {
    let section = r#"<section><layout margin="40pt"><img name="logo" width="40pt"/><img name="logo" width="80pt"/><img name="logo" width="20pt"/></layout></section>"#;
    let pdf = rendered(&[("logo", fixture("shapes.svg"))], section);
    assert_eq!(text(&pdf).matches("/BBox [0 0 1 1]").count(), 1);
    assert_eq!(page_content(&pdf).matches("/I0 Do").count(), 3);
}

#[test]
fn writes_a_form_for_each_distinct_svg() {
    let section = r#"<section><layout margin="40pt"><img name="a" width="40pt"/><img name="b" width="40pt"/></layout></section>"#;
    let pdf = rendered(&[("a", fixture("shapes.svg")), ("b", fixture("marker.svg"))], section);
    assert_eq!(text(&pdf).matches("/BBox [0 0 1 1]").count(), 2);
}

#[test]
fn draws_an_svg_with_a_radius_clipped_like_any_picture() {
    let pdf = rendered(
        &[("logo", fixture("shapes.svg"))],
        r#"<section><layout margin="40pt"><img name="logo" width="100pt" radius="12pt"/></layout></section>"#,
    );
    let content = page_content(&pdf);
    assert!(content.lines().any(|line| line == "W") && content.contains("/I0 Do"), "{content}");
}

#[test]
fn writes_the_group_a_svg_fades_as_one_as_a_transparency_group_the_picture_names() {
    let pdf = rendered(&[("logo", fixture("structure.svg"))], &format!("<section>{LOGO}</section>"));
    let text = text(&pdf);
    assert_eq!(text.matches("/S /Transparency").count(), 3, "the three faded groups");
    assert!(text.contains("/G1 ") && text.contains("/S0 "), "the picture names the groups and the graphics states");
    assert!(text.contains("/CA 0.5") && text.contains("/ca 0.4"), "the alphas");
}

// ── currentColor ──────────────────────────────────────────────────────────────

#[test]
fn paints_current_color_in_the_color_of_the_text_around_the_image() {
    let section = r##"<section><layout margin="40pt"><column color="#336699"><img name="icon" width="24pt"/></column></layout></section>"##;
    let content = page_content(&rendered(&[("icon", fixture("current-color.svg"))], section));
    let before_draw = &content[..content.find("/I0 Do").expect("the picture is drawn")];
    let blue = [0.2, 0.4, 0.6];
    assert!(operands(before_draw, "rg").iter().any(|fill| is_color(fill, blue)), "{content}");
    assert!(operands(before_draw, "RG").iter().any(|stroke| is_color(stroke, blue)), "{content}");
}

#[test]
fn paints_current_color_in_the_text_color_of_the_tokens_where_no_box_sets_one() {
    let section = r#"<section><layout margin="40pt"><img name="icon" width="24pt"/></layout></section>"#;
    let xml = document(&["icon"], section).replace(
        "<assets>",
        r##"<tokens><colors><color name="text" value="#445566"/></colors></tokens><assets>"##,
    );
    let mut engine = LpdfEngine::new("");
    engine.load_image("icon", &fixture("current-color.svg"));
    let content = page_content(&engine.render_xml(&xml, None, &RenderEncryption::default()).expect("renders"));
    assert!(operands(&content, "rg").iter().any(|fill| is_color(fill, [0x44 as f32 / 255.0, 0x55 as f32 / 255.0, 0x66 as f32 / 255.0])), "{content}");
}

#[test]
fn paints_current_color_of_an_img_at_in_the_color_of_its_layer() {
    let section = r##"<section><layout margin="40pt"><text>Body</text></layout><layer color="#aa2200"><img-at name="icon" x="100pt" y="100pt" width="24pt"/></layer></section>"##;
    let content = page_content(&rendered(&[("icon", fixture("current-color.svg"))], section));
    assert!(operands(&content, "rg").iter().any(|fill| is_color(fill, [170.0 / 255.0, 34.0 / 255.0, 0.0])), "{content}");
}

#[test]
fn sets_no_color_before_an_svg_that_never_says_current_color() {
    let section = r##"<section><layout margin="40pt"><column color="#336699"><img name="logo" width="24pt"/></column></layout></section>"##;
    let content = page_content(&rendered(&[("logo", fixture("shapes.svg"))], section));
    let before_draw = &content[..content.find("/I0 Do").expect("the picture is drawn")];
    assert!(!before_draw.contains("0.2 0.4 0.6"), "{before_draw}");
}

#[test]
fn draws_the_same_form_in_the_color_of_each_place_it_is_drawn() {
    let section = r##"<section><layout margin="40pt"><column color="#336699"><img name="icon" width="24pt"/></column><column color="#aa2200"><img name="icon" width="24pt"/></column></layout></section>"##;
    let pdf = rendered(&[("icon", fixture("current-color.svg"))], section);
    assert_eq!(text(&pdf).matches("/BBox [0 0 1 1]").count(), 1, "one form");
    let content = page_content(&pdf);
    let fills = operands(&content, "rg");
    assert!(fills.iter().any(|fill| is_color(fill, [0.2, 0.4, 0.6])) && fills.iter().any(|fill| is_color(fill, [170.0 / 255.0, 34.0 / 255.0, 0.0])), "{content}");
}

// ── What is refused ───────────────────────────────────────────────────────────

#[test]
fn refuses_an_svg_it_cannot_draw_with_an_error_that_names_the_image_and_the_tag() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <text x="1" y="8">Hi</text>
</svg>"#
        .to_vec();
    let error = render(&[("logo", svg)], &format!("<section>{LOGO}</section>")).unwrap_err();
    assert_eq!(
        error,
        "image 'logo': the SVG has a <text> at line 2, col 3, which can't be drawn yet: convert the text to outlines"
    );
}

#[test]
fn refuses_a_use_bomb_with_an_error_that_names_the_limit_before_it_is_drawn() {
    // A kilobyte of `use` that stands for 10 to the sixth rects.
    let mut defs = String::from(r#"<rect id="l0" width="1" height="1"/>"#);
    for level in 1..=6 {
        let uses = format!(r##"<use href="#l{}"/>"##, level - 1).repeat(10);
        defs.push_str(&format!(r#"<g id="l{level}">{uses}</g>"#));
    }
    let bomb = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><defs>{defs}</defs><use href="#l6"/></svg>"##);
    let error = render(&[("logo", bomb.into_bytes())], &format!("<section>{LOGO}</section>")).unwrap_err();
    assert!(error.starts_with("image 'logo': the SVG draws more than 100000 elements"), "{error}");
}

#[test]
fn leaves_an_svg_that_cannot_be_drawn_alone_when_the_document_never_declares_it() {
    let bad = br#"<svg xmlns="http://www.w3.org/2000/svg"><text>x</text></svg>"#.to_vec();
    let mut engine = LpdfEngine::new("");
    engine.load_image("unused", &bad);
    let xml = r#"<lpdf version="1"><document size="a4"><section><layout><text>Body</text></layout></section></document></lpdf>"#;
    engine.render_xml(xml, None, &RenderEncryption::default()).expect("renders");
}

#[test]
fn reads_an_svg_when_it_is_loaded_and_forgets_it_when_the_name_is_loaded_again() {
    let mut registry = ImageRegistry::new();
    registry.load("pic", fixture("shapes.svg"));
    assert!(registry.drawing("pic").is_some() && registry.error("pic").is_none());
    registry.load("pic", b"<svg xmlns=\"http://www.w3.org/2000/svg\"><image/></svg>".to_vec());
    assert!(registry.drawing("pic").is_none());
    assert!(registry.error("pic").is_some_and(|error| error.contains("<image>")));
    registry.load("pic", fixture("../image-square.png"));
    assert!(registry.drawing("pic").is_none() && registry.error("pic").is_none());
}

#[test]
fn draws_the_repos_own_mark() {
    let mark = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lpdf-mark.svg")).expect("the mark");
    let pdf = rendered(&[("logo", mark)], &format!("<section>{LOGO}</section>"));
    assert!(text(&pdf).contains("/BBox [0 0 1 1]"));
}

// ── Samples for the external checks ──────────────────────────────────────────

/// The standards' fixture `xml` with an SVG picture, `svg/structure.svg` loaded as `mark`, at the top of the first
/// layout of the document, with the alternative text PDF/UA asks of every picture.
fn with_svg(xml: &str) -> String {
    let assets = xml.find("</assets>").expect("assets");
    let xml = format!(r#"{}<image name="mark" src="../assets/images/svg/structure.svg"/>{}"#, &xml[..assets], &xml[assets..]);
    let layout = xml.find("<layout").expect("a layout");
    let end = layout + xml[layout..].find('>').expect("the end of the layout's tag") + 1;
    let alt = "Tiles, a clipped circle and squares faded as groups";
    format!(r#"{}<img name="mark" width="240pt" alt="{alt}"/>{}"#, &xml[..end], &xml[end..])
}

/// The PDF/A-2u fixture with an SVG picture, for `make check-pdfa`.
pub(crate) fn pdfa_sample() -> Vec<u8> {
    crate::pdfa_render_tests::render(&with_svg(&crate::pdfa_render_tests::fixture("2u"))).expect("renders")
}

/// The PDF/UA fixture with an SVG picture, for `make check-pdfua`.
pub(crate) fn pdfua_sample() -> Vec<u8> {
    crate::pdfua_render_tests::render(&with_svg(&crate::pdfua_render_tests::fixture())).expect("renders")
}

/// The PDF/X fixture with an SVG picture, for `make check-pdfx`.
pub(crate) fn pdfx_sample() -> Vec<u8> {
    crate::pdfx_render_tests::render(&with_svg(&crate::pdfx_render_tests::fixture())).expect("renders")
}

#[test]
fn ties_the_rgb_of_an_svg_picture_to_srgb_in_a_pdfx_file_as_it_does_for_every_page() {
    let pdf = text(&pdfx_sample());
    // Each form of the picture, the faded groups too, names `DefaultRGB`, and the groups blend in the ICC profile.
    let forms: Vec<&str> = pdf.split("endobj").filter(|object| object.contains("/Subtype /Form") && object.contains("/S0 ")).collect();
    assert_eq!(forms.len(), 4, "the picture and its three faded groups");
    for form in forms {
        assert!(form.contains("/DefaultRGB"), "{form}");
    }
    assert!(!pdf.contains("/CS /DeviceRGB") || pdf.contains("/ICCBased"), "the groups blend in sRGB, as the page's");
}

#[test]
fn draws_an_svg_in_a_pdfa_document_and_in_a_pdfua_document_as_a_figure_with_its_alternative_text() {
    for pdf in [pdfa_sample(), pdfua_sample()] {
        let text = text(&pdf);
        assert!(text.contains("/BBox [0 0 1 1]") && !text.contains("/Subtype /Image /"), "vector");
    }
    // PDF/A-2u is not tagged; PDF/UA says what each thing is.
    let tagged = text(&pdfua_sample());
    assert!(tagged.contains("/S /Figure") && tagged.contains("/Alt "), "a figure with an alternative text");
}
