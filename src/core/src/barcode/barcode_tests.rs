//! The barcode types, checked with ZXing (`rxing`): each barcode is laid out from XML, its bars or modules drawn into a
//! bitmap with a quiet zone around it, and decoded, which must give back its data, and GS1 data as GS1.

use rxing::{BarcodeFormat, DecodeHints, RXingResult, RXingResultMetadataType, RXingResultMetadataValue};

use super::{code128, code39, datamatrix, ean, gs1, pdf417};
use crate::render::{RenderBarcode, RenderNode, RenderedBarcodeKind};

// ── Laying out, drawing and decoding ─────────────────────────────────────────

fn document(content: &str) -> String {
    format!(
        r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt">{content}</layout></section></document></lpdf>"#
    )
}

/// The node a `<barcode>` element lays out as: the barcode, or the gray box that stands in for data it can't hold.
fn laid_out(element: &str) -> RenderNode {
    let mut doc = crate::parse::parse(&document(element)).unwrap_or_else(|e| panic!("{e}\n    {element}"));
    let mut pages = crate::layout::layout_document(&doc.section_layouts()).expect("the document lays out");
    std::mem::take(&mut pages[0].nodes)
        .into_iter()
        .find(|node| matches!(node, RenderNode::Barcode(_) | RenderNode::Box(_)))
        .expect("the page has the barcode")
}

fn barcode_of(element: &str) -> RenderBarcode {
    match laid_out(element) {
        RenderNode::Barcode(barcode) => barcode,
        _ => panic!("{element} lays out as the placeholder box"),
    }
}

/// Whether the element lays out as the placeholder: a gray box with a red border.
fn is_placeholder(element: &str) -> bool {
    matches!(laid_out(element), RenderNode::Box(b) if b.fill.as_deref() == Some("#eeeeee"))
}

/// The error the parser gives for a document of the element.
fn refusal(element: &str) -> String {
    crate::parse::parse(&document(element)).err().unwrap_or_else(|| panic!("{element} is read without an error"))
}

/// The barcode drawn into a bitmap: a linear one 3 pixels to a module and 60 high, a 2D one 4 pixels to a module and
/// its rows in proportion, each with a quiet zone of 10 modules.
fn bitmap(barcode: &RenderBarcode) -> (Vec<u8>, u32, u32) {
    match &barcode.kind {
        RenderedBarcodeKind::Linear { bars, .. } => {
            let modules: Vec<bool> = bars
                .iter()
                .enumerate()
                .flat_map(|(i, &run)| std::iter::repeat_n(i % 2 == 0, run as usize))
                .collect();
            raster(&modules, modules.len(), 3, 60)
        }
        RenderedBarcodeKind::Matrix { modules, cols, rows, .. } => {
            let (cell_w, cell_h) = (barcode.width / *cols as f32, barcode.height / *rows as f32);
            raster(modules, *cols as usize, 4, (4.0 * cell_h / cell_w).round() as usize)
        }
        RenderedBarcodeKind::Qr { .. } => panic!("QR codes are not tested here"),
    }
}

fn raster(modules: &[bool], cols: usize, cell_w: usize, cell_h: usize) -> (Vec<u8>, u32, u32) {
    const QUIET: usize = 10;
    let rows = modules.len() / cols;
    let (width, height) = ((cols + 2 * QUIET) * cell_w, rows * cell_h + 2 * QUIET * cell_w);
    let mut luma = vec![255u8; width * height];
    for (i, _) in modules.iter().enumerate().filter(|(_, dark)| **dark) {
        let (x0, y0) = ((QUIET + i % cols) * cell_w, QUIET * cell_w + (i / cols) * cell_h);
        for y in y0..y0 + cell_h {
            luma[y * width + x0..y * width + x0 + cell_w].fill(0);
        }
    }
    (luma, width as u32, height as u32)
}

fn decode_with(barcode: &RenderBarcode, format: BarcodeFormat, mut hints: DecodeHints) -> RXingResult {
    let (luma, width, height) = bitmap(barcode);
    rxing::helpers::detect_in_luma_with_hints(luma, width, height, Some(format), &mut hints)
        .unwrap_or_else(|e| panic!("ZXing doesn't read the {format:?}: {e:?}"))
}

/// What ZXing reads from the barcode an element lays out as.
fn decoded(element: &str, format: BarcodeFormat) -> String {
    decode_with(&barcode_of(element), format, DecodeHints::default()).getText().to_string()
}

/// The symbology identifier ZXing gives, such as `]C1` for a GS1-128 and `]d2` for a GS1 Data Matrix.
fn symbology_identifier(result: &RXingResult) -> String {
    match result.getRXingResultMetadata().get(&RXingResultMetadataType::SYMBOLOGY_IDENTIFIER) {
        Some(RXingResultMetadataValue::SymbologyIdentifier(id)) => id.clone(),
        other => panic!("no symbology identifier: {other:?}"),
    }
}

/// The modules a barcode's box is wide and high, and how many points a module takes.
fn matrix_shape(barcode: &RenderBarcode) -> (u32, u32) {
    match &barcode.kind {
        RenderedBarcodeKind::Matrix { cols, rows, .. } => (*cols, *rows),
        _ => panic!("not a 2D barcode"),
    }
}

/// A PDF417's columns of codewords, from its width in modules.
fn pdf417_columns(barcode: &RenderBarcode) -> u32 {
    (matrix_shape(barcode).0 - 69) / 17
}

const GS1_DATA: &str = "(01)09501101530003(17)261231(10)AB12";

// ── Code 39 ──────────────────────────────────────────────────────────────────

#[test]
fn a_code39_reads_back_as_its_data() {
    assert_eq!(decoded(r#"<barcode type="code39" data="LPDF-2026 $/+%."/>"#, BarcodeFormat::CODE_39), "LPDF-2026 $/+%.");
}

#[test]
fn a_code39_with_check_ends_in_its_mod_43_character() {
    let element = r#"<barcode type="code39" data="CODE39" check="true"/>"#;
    // C=12 O=24 D=13 E=14 3=3 9=9: 75 mod 43 is 32, W.
    assert_eq!(decoded(element, BarcodeFormat::CODE_39), "CODE39W", "a scanner not set to check it reads it as data");
    let checked = DecodeHints { AssumeCode39CheckDigit: Some(true), ..DecodeHints::default() };
    assert_eq!(decode_with(&barcode_of(element), BarcodeFormat::CODE_39, checked).getText(), "CODE39");
}

#[test]
fn a_code39_without_check_has_none() {
    let plain = barcode_of(r#"<barcode type="code39" data="CODE39"/>"#);
    let checked = barcode_of(r#"<barcode type="code39" data="CODE39" check="true"/>"#);
    assert_eq!(decode_with(&plain, BarcodeFormat::CODE_39, DecodeHints::default()).getText(), "CODE39");
    assert!(plain.width < checked.width, "the check character adds one character's width");
}

#[test]
fn a_code39_refuses_characters_outside_its_43_and_points_to_code128() {
    let error = code39::encode("lpdf", false).unwrap_err();
    assert!(error.contains("'l' is not one of its characters") && error.contains("code128"), "{error}");
    assert!(is_placeholder(r#"<barcode type="code39" data="lpdf"/>"#));
}

// ── EAN-13, EAN-8 and UPC-A ──────────────────────────────────────────────────

#[test]
fn an_ean13_reads_back_with_any_first_digit() {
    for data in ["5901234123457", "4006381333931", "0012345678905", "9780201379624"] {
        let element = format!(r#"<barcode type="ean13" data="{data}"/>"#);
        assert_eq!(decoded(&element, BarcodeFormat::EAN_13), data);
    }
}

#[test]
fn an_ean8_gets_its_check_digit_added_or_checked() {
    assert_eq!(decoded(r#"<barcode type="ean8" data="9638507"/>"#, BarcodeFormat::EAN_8), "96385074");
    assert_eq!(decoded(r#"<barcode type="ean8" data="96385074"/>"#, BarcodeFormat::EAN_8), "96385074");
    let error = ean::encode_ean8("96385075").unwrap_err();
    assert!(error.contains("invalid check digit (got 5, expected 4)"), "{error}");
    assert!(is_placeholder(r#"<barcode type="ean8" data="96385075"/>"#));
}

#[test]
fn a_upca_gets_its_check_digit_added_or_checked() {
    assert_eq!(decoded(r#"<barcode type="upca" data="03600029145"/>"#, BarcodeFormat::UPC_A), "036000291452");
    assert_eq!(decoded(r#"<barcode type="upca" data="036000291452"/>"#, BarcodeFormat::UPC_A), "036000291452");
    let error = ean::encode_upca("036000291453").unwrap_err();
    assert!(error.contains("invalid check digit (got 3, expected 2)"), "{error}");
    assert!(is_placeholder(r#"<barcode type="upca" data="036000291453"/>"#));
}

#[test]
fn hrt_shows_the_check_digit_of_an_ean8_and_a_upca() {
    let text = |element: &str| match barcode_of(element).kind {
        RenderedBarcodeKind::Linear { hrt, .. } => hrt,
        _ => None,
    };
    assert_eq!(text(r#"<barcode type="ean8" data="9638507" hrt="true"/>"#).as_deref(), Some("96385074"));
    assert_eq!(text(r#"<barcode type="upca" data="03600029145" hrt="true"/>"#).as_deref(), Some("036000291452"));
}

#[test]
fn an_ean8_and_a_upca_are_the_standard_symbol_without_a_size() {
    let module = 0.33 * 72.0 / 25.4;
    let ean8 = barcode_of(r#"<barcode type="ean8" data="9638507"/>"#);
    assert!((ean8.width - 67.0 * module).abs() < 0.01 && (ean8.height - 18.23 * 72.0 / 25.4).abs() < 0.01);
    let upca = barcode_of(r#"<barcode type="upca" data="03600029145"/>"#);
    assert!((upca.width - 95.0 * module).abs() < 0.01 && (upca.height - 22.85 * 72.0 / 25.4).abs() < 0.01);
}

// ── GS1-128 ──────────────────────────────────────────────────────────────────

#[test]
fn a_gs1_128_reads_back_as_gs1_with_its_fields() {
    let barcode = barcode_of(&format!(r#"<barcode type="gs1-128" data="{GS1_DATA}"/>"#));
    let hints = DecodeHints { AssumeGs1: Some(true), ..DecodeHints::default() };
    let result = decode_with(&barcode, BarcodeFormat::CODE_128, hints);
    assert_eq!(symbology_identifier(&result), "]C1");
    assert_eq!(result.getText(), "]C101095011015300031726123110AB12");
}

#[test]
fn a_gs1_128_ends_a_field_of_varying_length_with_fnc1() {
    let barcode = barcode_of(r#"<barcode type="gs1-128" data="(10)AB12(17)261231(21)XY9"/>"#);
    let hints = DecodeHints { AssumeGs1: Some(true), ..DecodeHints::default() };
    assert_eq!(decode_with(&barcode, BarcodeFormat::CODE_128, hints).getText(), "]C110AB12\u{1d}1726123121XY9");

    // The codewords themselves, as a scanner gives GS for FNC1 and for a GS alike: FNC1 first, after (10) and not
    // after (17), whose length is predefined, nor after the last field.
    let fields = gs1::parse("(10)AB12(17)261231(21)XY9").unwrap();
    let values = code128::gs1_values(&fields);
    let fnc1: Vec<usize> = values.iter().enumerate().filter(|(_, v)| **v == code128::FNC1_VALUE).map(|(i, _)| i).collect();
    assert_eq!(fnc1.len(), 2, "{values:?}");
    assert_eq!(fnc1[0], 1, "FNC1 right after the start symbol: {values:?}");
}

#[test]
fn a_gs1_128_packs_digits_in_pairs_and_a_code128_does_not() {
    let gs1 = barcode_of(r#"<barcode type="gs1-128" data="(00)106141411234567897"/>"#);
    let code128 = barcode_of(r#"<barcode type="code128" data="00106141411234567897"/>"#);
    assert!(gs1.width < code128.width * 0.7, "{} against {}", gs1.width, code128.width);
}

#[test]
fn gs1_hrt_shows_the_fields_in_parentheses() {
    match barcode_of(&format!(r#"<barcode type="gs1-128" data=" {GS1_DATA} " hrt="true"/>"#)).kind {
        RenderedBarcodeKind::Linear { hrt, .. } => assert_eq!(hrt.as_deref(), Some(GS1_DATA)),
        _ => panic!("a linear barcode"),
    }
}

#[test]
fn data_that_isnt_gs1_draws_the_placeholder_with_the_reason() {
    for (data, reason) in [
        ("0109501101530003", "write the data as fields"),
        ("(01)0950110153000", "(01) takes 14 digits"),
        ("(01)09501101530004", "has the check digit 4, and 3 is right"),
        ("(1)A", "'(1)' is not an AI"),
        ("(10)", "(10) has no value"),
        ("(10)A#B", "'#', which is not one of GS1's characters"),
    ] {
        let error = gs1::parse(data).unwrap_err();
        assert!(error.contains(reason), "{data}: {error}");
        assert!(is_placeholder(&format!(r#"<barcode type="gs1-128" data="{data}"/>"#)), "{data}");
        assert!(is_placeholder(&format!(r#"<barcode type="gs1-datamatrix" data="{data}" width="60pt"/>"#)), "{data}");
    }
}

#[test]
fn a_gs1_128_holds_48_characters_at_most() {
    let long = "(10)ABCDEFGHIJKLMNOPQRST(21)ABCDEFGHIJKLMNOPQRST(240)ABCDEFGHIJ";
    assert!(is_placeholder(&format!(r#"<barcode type="gs1-128" data="{long}"/>"#)));
}

// ── Data Matrix ──────────────────────────────────────────────────────────────

#[test]
fn a_data_matrix_reads_back_square_and_rectangular() {
    let square = r#"<barcode type="datamatrix" data="Lpdf Data Matrix 2026" width="80pt"/>"#;
    assert_eq!(decoded(square, BarcodeFormat::DATA_MATRIX), "Lpdf Data Matrix 2026");
    let (cols, rows) = matrix_shape(&barcode_of(square));
    assert_eq!(cols, rows, "one size makes a square");

    let rectangle = r#"<barcode type="datamatrix" data="Lpdf 2026" width="180pt" height="40pt"/>"#;
    assert_eq!(decoded(rectangle, BarcodeFormat::DATA_MATRIX), "Lpdf 2026");
    let (cols, rows) = matrix_shape(&barcode_of(rectangle));
    assert!(cols > rows, "a wide box makes a rectangle: {cols}x{rows}");
}

#[test]
fn a_data_matrix_reads_back_text_beyond_latin1() {
    let element = r#"<barcode type="datamatrix" data="Ζήτω — 2026" width="80pt"/>"#;
    assert_eq!(decoded(element, BarcodeFormat::DATA_MATRIX), "Ζήτω — 2026");
}

#[test]
fn a_gs1_data_matrix_reads_back_as_gs1_with_its_fields() {
    let barcode = barcode_of(r#"<barcode type="gs1-datamatrix" data="(01)09501101530003(10)AB12(17)261231" width="60pt"/>"#);
    let result = decode_with(&barcode, BarcodeFormat::DATA_MATRIX, DecodeHints::default());
    assert_eq!(symbology_identifier(&result), "]d2");
    assert_eq!(result.getText(), "010950110153000310AB12\u{1d}17261231");
}

#[test]
fn a_gs1_data_matrix_separates_fields_with_fnc1_and_not_gs() {
    let fields = gs1::parse("(01)09501101530003(10)AB12(17)261231(21)XY9").unwrap();
    let codewords = datamatrix::gs1_data_codewords(&fields);
    assert_eq!(codewords[0], 232, "FNC1 first: {codewords:?}");
    assert_eq!(codewords.iter().filter(|&&c| c == 232).count(), 2, "FNC1 after (10) only: {codewords:?}");
    assert!(!codewords.contains(&30), "no GS: {codewords:?}");
}

#[test]
fn a_data_matrix_in_a_box_takes_the_shape_with_the_largest_modules() {
    // "Lpdf" is four codewords: more than a 10x10 holds, and the 8x18 holds five.
    let square_box = barcode_of(r#"<barcode type="datamatrix" data="Lpdf" width="100pt" height="100pt"/>"#);
    assert_eq!(matrix_shape(&square_box), (12, 12));
    let wide_box = barcode_of(r#"<barcode type="datamatrix" data="Lpdf" width="200pt" height="50pt"/>"#);
    assert_eq!(matrix_shape(&wide_box), (18, 8));
    assert!(wide_box.width <= 200.0 && wide_box.height <= 50.0);
}

#[test]
fn a_data_matrix_with_only_a_height_is_a_square_of_that_height() {
    let barcode = barcode_of(r#"<barcode type="datamatrix" data="Lpdf" height="50pt"/>"#);
    assert_eq!(matrix_shape(&barcode), (12, 12), "the smallest square that holds four codewords");
    assert_eq!((barcode.width, barcode.height), (50.0, 50.0));
}

#[test]
fn a_data_matrix_takes_the_rows_and_columns_it_is_given() {
    let fixed = barcode_of(r#"<barcode type="datamatrix" data="Lpdf" rows="12" columns="36" width="200pt"/>"#);
    assert_eq!(matrix_shape(&fixed), (36, 12));
    let rows_only = barcode_of(r#"<barcode type="datamatrix" data="Lpdf" rows="16" width="200pt" height="200pt"/>"#);
    assert_eq!(matrix_shape(&rows_only).1, 16, "a size of 16 rows");
    assert_eq!(decoded(r#"<barcode type="datamatrix" data="Lpdf" rows="12" columns="36" width="200pt"/>"#, BarcodeFormat::DATA_MATRIX), "Lpdf");
}

#[test]
fn a_data_matrix_takes_the_next_size_up_for_data_its_size_cant_hold() {
    let data = "Lpdf lays out documents and writes them as PDF";
    let barcode = barcode_of(&format!(r#"<barcode type="datamatrix" data="{data}" rows="10" columns="10" width="80pt"/>"#));
    let (cols, rows) = matrix_shape(&barcode);
    assert!(cols == rows && cols > 10, "a larger square: {cols}x{rows}");
    assert_eq!(decode_with(&barcode, BarcodeFormat::DATA_MATRIX, DecodeHints::default()).getText(), data);
}

#[test]
fn a_data_matrix_refuses_a_size_the_standard_hasnt() {
    assert!(refusal(r#"<barcode type="datamatrix" data="x" rows="11" width="50pt"/>"#).contains("no size of 11 rows"));
    assert!(refusal(r#"<barcode type="datamatrix" data="x" columns="11" width="50pt"/>"#).contains("no size of 11 columns"));
    let error = refusal(r#"<barcode type="datamatrix" data="x" rows="12" columns="18" width="50pt"/>"#);
    assert!(error.contains("no size of 12 rows by 18 columns") && error.contains("12x26"), "{error}");
}

// ── PDF417 ───────────────────────────────────────────────────────────────────

#[test]
fn a_pdf417_reads_back_at_every_error_correction() {
    let data = "Boarding pass LPDF 2026 seat 12A";
    for ec in ["", r#" ec="0""#, r#" ec="4""#, r#" ec="8""#] {
        let element = format!(r#"<barcode type="pdf417" data="{data}" width="300pt"{ec}/>"#);
        assert_eq!(decoded(&element, BarcodeFormat::PDF_417), data, "{ec}");
    }
}

#[test]
fn a_pdf417_reads_back_text_numbers_and_bytes() {
    for data in [
        "Mixed text: a-z, A-Z & punctuation ;<>@[]_`~!\"|(){}'? at 09:30.",
        "12345678901234567890123456789012345678901234567890123456789",
        "Order 1234567890123456 for 42 items",
        "Binary \u{7f} bytes and Ünïcödé — 2026",
        "x",
    ] {
        let escaped = data.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
        let element = format!(r#"<barcode type="pdf417" data="{escaped}" width="400pt"/>"#);
        assert_eq!(decoded(&element, BarcodeFormat::PDF_417), data);
    }
}

#[test]
fn a_pdf417_reads_back_text_after_a_byte_that_follows_punctuation() {
    // Two punctuation characters after a digit latch to the punctuation submode, an odd count pads with 29, which
    // there latches to upper case, and the text after the byte goes on from it.
    for data in ["AB1<>\u{7f}ABCDE", "AB1<>\u{7f}abcde", "AB1<>?\u{7f}ABCDE", "9;<\u{7f}x;<\u{7f}YZabc"] {
        let escaped = data.replace('<', "&lt;").replace('>', "&gt;");
        let element = format!(r#"<barcode type="pdf417" data="{escaped}" width="300pt"/>"#);
        assert_eq!(decoded(&element, BarcodeFormat::PDF_417), data);
    }
}

#[test]
fn a_pdf417_numeric_run_takes_fewer_codewords_than_text() {
    let digits = pdf417::codewords_of("1234567890123456789012345678901234567890");
    let text = pdf417::codewords_of("ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMN");
    assert!(digits.len() < text.len(), "{} against {}", digits.len(), text.len());
}

#[test]
fn a_pdf417_error_correction_makes_a_reed_solomon_codeword_at_every_level() {
    // The data and its error correction, read as a polynomial with the first codeword the highest power, are zero at
    // 3^1 to 3^k modulo 929: the roots of the generator, which is how a scanner checks them.
    let data: Vec<u16> = vec![12, 453, 178, 121, 239, 900, 0, 928, 77, 5, 300, 812];
    for level in 0..=8u8 {
        let mut codeword = data.clone();
        codeword.extend(pdf417::error_correction_of(&data, level));
        let k = 2u32 << level;
        for j in 1..=k {
            let root = (0..j).fold(1u32, |p, _| p * 3 % 929);
            let value = codeword.iter().fold(0u32, |acc, &c| (acc * root + c as u32) % 929);
            assert_eq!(value, 0, "level {level}: not zero at 3^{j}");
        }
    }
}

#[test]
fn a_pdf417_with_one_size_is_about_three_times_as_wide_as_high() {
    let barcode = barcode_of(r#"<barcode type="pdf417" data="Boarding pass LPDF 2026 seat 12A, gate B7" width="300pt"/>"#);
    let ratio = barcode.width / barcode.height;
    assert!((2.0..4.5).contains(&ratio), "{ratio}");
    assert_eq!(barcode.width, 300.0);
    let by_height = barcode_of(r#"<barcode type="pdf417" data="Boarding pass LPDF 2026 seat 12A, gate B7" height="60pt"/>"#);
    assert_eq!(by_height.height, 60.0);
    assert_eq!(pdf417_columns(&by_height), pdf417_columns(&barcode), "the same shape by either side");
}

#[test]
fn a_pdf417_in_a_box_takes_the_shape_with_the_largest_modules() {
    let data = "Boarding pass LPDF 2026 seat 12A, gate B7";
    let wide = barcode_of(&format!(r#"<barcode type="pdf417" data="{data}" width="500pt" height="40pt"/>"#));
    let tall = barcode_of(&format!(r#"<barcode type="pdf417" data="{data}" width="150pt" height="300pt"/>"#));
    assert!(pdf417_columns(&wide) > pdf417_columns(&tall), "{} against {}", pdf417_columns(&wide), pdf417_columns(&tall));
    assert!(wide.width <= 500.0 && wide.height <= 40.0 && tall.width <= 150.0 && tall.height <= 300.0);
    assert_eq!(decoded(&format!(r#"<barcode type="pdf417" data="{data}" width="150pt" height="300pt"/>"#), BarcodeFormat::PDF_417), data);
}

#[test]
fn a_pdf417_takes_the_rows_and_columns_it_is_given() {
    let data = "Boarding pass LPDF 2026";
    let fixed = barcode_of(&format!(r#"<barcode type="pdf417" data="{data}" rows="12" columns="4" width="300pt"/>"#));
    assert_eq!((pdf417_columns(&fixed), matrix_shape(&fixed).1), (4, 12));
    let columns = barcode_of(&format!(r#"<barcode type="pdf417" data="{data}" columns="2" width="300pt"/>"#));
    assert_eq!(pdf417_columns(&columns), 2);
    let rows = barcode_of(&format!(r#"<barcode type="pdf417" data="{data}" rows="20" width="300pt" height="300pt"/>"#));
    assert_eq!(matrix_shape(&rows).1, 20);
    assert_eq!(decoded(&format!(r#"<barcode type="pdf417" data="{data}" rows="12" columns="4" width="300pt"/>"#), BarcodeFormat::PDF_417), data);
}

#[test]
fn a_pdf417_gets_more_rows_then_more_columns_for_data_that_needs_them() {
    let data = "Lpdf lays out documents and writes them as PDF, ".repeat(4);
    let rows = barcode_of(&format!(r#"<barcode type="pdf417" data="{data}" rows="3" columns="10" width="400pt"/>"#));
    assert!(matrix_shape(&rows).1 > 3 && pdf417_columns(&rows) == 10, "more rows");
    let columns = barcode_of(&format!(r#"<barcode type="pdf417" data="{}" columns="1" width="400pt"/>"#, data.repeat(3)));
    assert!(pdf417_columns(&columns) > 1 && matrix_shape(&columns).1 <= 90, "more columns past 90 rows");
    assert_eq!(decode_with(&columns, BarcodeFormat::PDF_417, DecodeHints::default()).getText(), data.repeat(3));
}

#[test]
fn a_pdf417_refuses_rows_and_columns_it_hasnt() {
    assert!(refusal(r#"<barcode type="pdf417" data="x" rows="2" width="50pt"/>"#).contains("3 to 90 rows, not 2"));
    assert!(refusal(r#"<barcode type="pdf417" data="x" rows="91" width="50pt"/>"#).contains("3 to 90 rows, not 91"));
    assert!(refusal(r#"<barcode type="pdf417" data="x" columns="31" width="50pt"/>"#).contains("1 to 30 columns, not 31"));
}

#[test]
fn data_too_long_for_a_pdf417_draws_the_placeholder() {
    let data = "\u{3a9}".repeat(1200);
    assert!(is_placeholder(&format!(r#"<barcode type="pdf417" data="{data}" width="400pt"/>"#)));
}

// ── What every type shares ───────────────────────────────────────────────────

#[test]
fn a_2d_code_without_a_size_is_refused_as_a_qr_code_is() {
    for kind in ["datamatrix", "gs1-datamatrix", "pdf417"] {
        let error = refusal(&format!(r#"<barcode type="{kind}" data="(01)09501101530003"/>"#));
        assert!(error.contains("needs a 'width' or a 'height'"), "{kind}: {error}");
    }
    let fill = barcode_of(r#"<barcode type="datamatrix" data="Lpdf" width="fill"/>"#);
    assert!(fill.width > 500.0, "fill takes the slot's width: {}", fill.width);
}

#[test]
fn an_ec_that_is_neither_a_letter_nor_a_level_is_refused() {
    for ec in ["9", "X", "10", "m"] {
        let error = refusal(&format!(r#"<barcode type="pdf417" data="x" width="50pt" ec="{ec}"/>"#));
        assert!(error.contains(&format!("invalid ec '{ec}'")), "{error}");
    }
}

#[test]
fn each_type_ignores_the_ec_of_the_other_kind() {
    let render = |element: &str| match barcode_of(element).kind {
        RenderedBarcodeKind::Qr { modules, .. } | RenderedBarcodeKind::Matrix { modules, .. } => modules,
        _ => panic!("a QR code or a 2D code"),
    };
    assert_eq!(
        render(r#"<barcode type="qr" data="Lpdf" width="50pt" ec="8"/>"#),
        render(r#"<barcode type="qr" data="Lpdf" width="50pt"/>"#)
    );
    assert_eq!(
        render(r#"<barcode type="pdf417" data="Lpdf" width="200pt" ec="H"/>"#),
        render(r#"<barcode type="pdf417" data="Lpdf" width="200pt"/>"#)
    );
}

#[test]
fn rows_columns_and_check_are_ignored_by_the_types_that_dont_use_them() {
    let modules = |element: &str| match barcode_of(element).kind {
        RenderedBarcodeKind::Qr { modules, .. } => modules,
        RenderedBarcodeKind::Linear { bars, .. } => bars.into_iter().map(|b| b > 1).collect(),
        _ => panic!("a QR code or a linear barcode"),
    };
    assert_eq!(
        modules(r#"<barcode type="qr" data="Lpdf" width="50pt" rows="7" columns="2" check="true"/>"#),
        modules(r#"<barcode type="qr" data="Lpdf" width="50pt"/>"#)
    );
    assert_eq!(
        modules(r#"<barcode type="code128" data="Lpdf" rows="7" columns="2" check="true"/>"#),
        modules(r#"<barcode type="code128" data="Lpdf"/>"#)
    );
}

#[test]
fn the_new_types_work_in_a_layer() {
    let xml = r#"<lpdf version="1"><document size="a4"><section><layout margin="28pt"/>
        <layer><barcode-at type="pdf417" data="In a layer" x="50pt" y="50pt" width="200pt"/></layer></section></document></lpdf>"#;
    let mut doc = crate::parse::parse(xml).unwrap();
    let pages = crate::layout::layout_document(&doc.section_layouts()).expect("lays out");
    fn has_matrix(nodes: &[RenderNode]) -> bool {
        nodes.iter().any(|node| match node {
            RenderNode::Barcode(b) => matches!(b.kind, RenderedBarcodeKind::Matrix { symbology: "pdf417", .. }),
            RenderNode::CanvasLayer(layer) => has_matrix(&layer.children),
            RenderNode::Box(b) => has_matrix(&b.children),
            _ => false,
        })
    }
    assert!(has_matrix(&pages[0].nodes), "the layer draws the PDF417");
}
