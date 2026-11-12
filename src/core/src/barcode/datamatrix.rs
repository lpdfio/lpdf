//! Data Matrix (ECC 200) through the `datamatrix` crate, in the standard's 24 squares and 6 rectangles, and GS1 Data
//! Matrix with FNC1 first and between fields.
//!
//! The crate writes FNC1 only first, and separates GS1 fields with the GS character, where GS1 asks for FNC1. So the
//! GS1 data is encoded in ASCII mode with GS as the separator, its GS codewords become FNC1, and the crate's error
//! correction and placement draw the result.

use datamatrix::data::EncodationType;
use datamatrix::errorcode::encode_error;
use datamatrix::placement::{Bitmap, MatrixMap};
use datamatrix::{DataMatrix, DataMatrixBuilder, SymbolList, SymbolSize};

use super::gs1::{self, Field};
use super::{Fit, Matrix};
use crate::parse::BarcodeOptions;

/// The standard's sizes, rows by columns: 24 squares, then 6 rectangles. The DMRE rectangles of ISO/IEC 21471 are left
/// out, since many scanners don't read them.
const SIZES: [(u8, u8, SymbolSize); 30] = [
    (10, 10, SymbolSize::Square10),
    (12, 12, SymbolSize::Square12),
    (14, 14, SymbolSize::Square14),
    (16, 16, SymbolSize::Square16),
    (18, 18, SymbolSize::Square18),
    (20, 20, SymbolSize::Square20),
    (22, 22, SymbolSize::Square22),
    (24, 24, SymbolSize::Square24),
    (26, 26, SymbolSize::Square26),
    (32, 32, SymbolSize::Square32),
    (36, 36, SymbolSize::Square36),
    (40, 40, SymbolSize::Square40),
    (44, 44, SymbolSize::Square44),
    (48, 48, SymbolSize::Square48),
    (52, 52, SymbolSize::Square52),
    (64, 64, SymbolSize::Square64),
    (72, 72, SymbolSize::Square72),
    (80, 80, SymbolSize::Square80),
    (88, 88, SymbolSize::Square88),
    (96, 96, SymbolSize::Square96),
    (104, 104, SymbolSize::Square104),
    (120, 120, SymbolSize::Square120),
    (132, 132, SymbolSize::Square132),
    (144, 144, SymbolSize::Square144),
    (8, 18, SymbolSize::Rect8x18),
    (8, 32, SymbolSize::Rect8x32),
    (12, 26, SymbolSize::Rect12x26),
    (12, 36, SymbolSize::Rect12x36),
    (16, 36, SymbolSize::Rect16x36),
    (16, 48, SymbolSize::Rect16x48),
];

/// The codeword of a character `c` in ASCII mode is `c + 1`, so 30 is the GS character.
const GS_CODEWORD: u8 = 30;
/// FNC1 in ASCII mode.
const FNC1_CODEWORD: u8 = 232;
/// The first pad codeword, which ends the data in ASCII mode.
const PAD_CODEWORD: u8 = 129;

/// Check `rows` and `columns` against the standard's sizes.
pub(super) fn check_shape(rows: Option<u8>, columns: Option<u8>) -> Result<(), String> {
    let list = |pick: fn(&(u8, u8, SymbolSize)) -> u8| {
        let mut values: Vec<u8> = SIZES.iter().map(pick).collect();
        values.sort_unstable();
        values.dedup();
        values.iter().map(u8::to_string).collect::<Vec<_>>().join(", ")
    };
    if let Some(r) = rows.filter(|r| !SIZES.iter().any(|s| s.0 == *r)) {
        return Err(format!("<barcode> a Data Matrix has no size of {r} rows; its rows are {}", list(|s| s.0)));
    }
    if let Some(c) = columns.filter(|c| !SIZES.iter().any(|s| s.1 == *c)) {
        return Err(format!("<barcode> a Data Matrix has no size of {c} columns; its columns are {}", list(|s| s.1)));
    }
    if let (Some(r), Some(c)) = (rows, columns) {
        if !SIZES.iter().any(|s| s.0 == r && s.1 == c) {
            let sizes: Vec<String> = SIZES.iter().map(|s| format!("{}x{}", s.0, s.1)).collect();
            return Err(format!(
                "<barcode> a Data Matrix has no size of {r} rows by {c} columns; its sizes are {}",
                sizes.join(", ")
            ));
        }
    }
    Ok(())
}

/// Encode text as a Data Matrix: Latin-1 as it is, and other text as UTF-8 behind an ECI that says so.
pub(super) fn encode(data: &str, options: &BarcodeOptions, fit: Fit) -> Result<Matrix, String> {
    let symbol = choose(options, fit, |sizes| DataMatrixBuilder::new().with_symbol_list(sizes).encode_str(data).ok())?;
    Ok(matrix_of(&symbol.bitmap()))
}

/// Encode GS1 fields as a GS1 Data Matrix: FNC1 first, and FNC1 after each field of varying length that isn't the last.
pub(super) fn encode_gs1(fields: &[Field], options: &BarcodeOptions, fit: Fit) -> Result<Matrix, String> {
    let text = gs1_text(fields);
    let symbol = choose(options, fit, |sizes| gs1_builder(sizes).encode(&text).ok())?;
    let (mut data, _) = with_fnc1_separators(symbol.data_codewords().to_vec());
    let ecc = encode_error(&data, symbol.size);
    data.extend_from_slice(&ecc);
    Ok(matrix_of(&MatrixMap::new_with_codewords(&data, symbol.size).bitmap()))
}

/// The GS1 Data Matrix's data codewords, without the padding and the error correction, for the tests.
#[cfg(test)]
pub(super) fn gs1_data_codewords(fields: &[Field]) -> Vec<u8> {
    let symbol = gs1_builder(SymbolList::default()).encode(&gs1_text(fields)).expect("the fields fit a Data Matrix");
    let (mut data, end) = with_fnc1_separators(symbol.data_codewords().to_vec());
    data.truncate(end);
    data
}

/// The characters of the fields, with GS after each field of varying length that isn't the last.
fn gs1_text(fields: &[Field]) -> Vec<u8> {
    let mut text = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        text.extend_from_slice(field.ai.as_bytes());
        text.extend_from_slice(field.value.as_bytes());
        if i + 1 < fields.len() && !gs1::has_predefined_length(field.ai) {
            text.push(0x1D);
        }
    }
    text
}

/// A builder for GS1 data: FNC1 first, ASCII mode alone, so that each GS is one codeword, and no macros.
fn gs1_builder(sizes: SymbolList) -> DataMatrixBuilder {
    DataMatrixBuilder::new()
        .with_symbol_list(sizes)
        .with_encodation_types(EncodationType::Ascii)
        .with_macros(false)
        .with_fnc1_start(true)
}

/// The data codewords with each GS of the data, before the padding, made FNC1, and where the padding starts.
fn with_fnc1_separators(mut data: Vec<u8>) -> (Vec<u8>, usize) {
    let end = data.iter().position(|&c| c == PAD_CODEWORD).unwrap_or(data.len());
    for codeword in &mut data[..end] {
        if *codeword == GS_CODEWORD {
            *codeword = FNC1_CODEWORD;
        }
    }
    (data, end)
}

/// The symbol for the size `fit` and the options ask for, encoded by `encode` in the sizes it is given, or `None`
/// when the data doesn't fit them.
///
/// The candidates are the sizes the `rows` and `columns` allow, or the squares when neither is set and the box gives
/// one side only. In a box, the size whose modules come out largest wins; with one side, the smallest that holds the
/// data. Data that fits none of them takes the next size up: the smallest of the same kind, square or rectangle,
/// that holds it, then the smallest of any kind.
fn choose(
    options: &BarcodeOptions,
    fit: Fit,
    encode: impl Fn(SymbolList) -> Option<DataMatrix>,
) -> Result<DataMatrix, String> {
    let set = |s: &&(u8, u8, SymbolSize)| {
        options.rows.is_none_or(|r| s.0 == r) && options.columns.is_none_or(|c| s.1 == c)
    };
    let shaped = options.rows.is_some() || options.columns.is_some();
    let mut candidates: Vec<&(u8, u8, SymbolSize)> =
        SIZES.iter().filter(set).filter(|s| shaped || matches!(fit, Fit::Box { .. }) || s.0 == s.1).collect();

    if let Fit::Box { width, height } = fit {
        let module = |s: &(u8, u8, SymbolSize)| (width / s.1 as f32).min(height / s.0 as f32);
        candidates.sort_by(|a, b| module(b).total_cmp(&module(a)).then((a.0 as u32 * a.1 as u32).cmp(&(b.0 as u32 * b.1 as u32))));
        for size in &candidates {
            if let Some(symbol) = encode(SymbolList::with_whitelist([size.2])) {
                return Ok(symbol);
            }
        }
    } else if let Some(symbol) = encode(SymbolList::with_whitelist(candidates.iter().map(|s| s.2))) {
        return Ok(symbol);
    }

    let square = candidates.first().is_none_or(|s| s.0 == s.1);
    let same_kind = SIZES.iter().filter(|s| (s.0 == s.1) == square).map(|s| s.2);
    encode(SymbolList::with_whitelist(same_kind))
        .or_else(|| encode(SymbolList::with_whitelist(SIZES.iter().map(|s| s.2))))
        .ok_or_else(|| "Data Matrix: the data needs more than the largest size, 144x144, holds".to_string())
}

/// The modules of a bitmap, row by row.
fn matrix_of(bitmap: &Bitmap<bool>) -> Matrix {
    let (cols, rows) = (bitmap.width(), bitmap.height());
    let mut modules = vec![false; cols * rows];
    for (x, y) in bitmap.pixels() {
        modules[y * cols + x] = true;
    }
    Matrix { modules, cols: cols as u32, rows: rows as u32, row_height: 1 }
}
