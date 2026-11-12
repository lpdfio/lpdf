//! The encoders of `<barcode>`: what each type draws for its data, as the bars of a linear code or the modules of a
//! 2D one. QR codes come from the `qrcode` crate and are laid out in `layout.rs`; Data Matrix codes from the
//! `datamatrix` crate, with the GS1 separators written here; the rest are encoded here.

#[path = "barcode/code128.rs"]
mod code128;
#[path = "barcode/code39.rs"]
mod code39;
#[path = "barcode/datamatrix.rs"]
mod datamatrix;
#[path = "barcode/ean.rs"]
mod ean;
#[path = "barcode/gs1.rs"]
mod gs1;
#[path = "barcode/pdf417.rs"]
mod pdf417;
#[path = "barcode/pdf417_patterns.rs"]
mod pdf417_patterns;

#[cfg(test)]
#[path = "barcode/barcode_tests.rs"]
mod barcode_tests;

use crate::parse::{BarcodeOptions, BarcodeType};

/// A linear barcode as drawn: the run lengths of its bars and spaces, in modules, starting with a bar, and the
/// text `hrt` shows under them.
pub(crate) struct Linear {
    pub bars: Vec<u8>,
    pub text: String,
}

/// A 2D barcode as drawn: its modules row by row, dark ones true.
pub(crate) struct Matrix {
    pub modules: Vec<bool>,
    pub cols: u32,
    pub rows: u32,
    /// How many modules high a row is: 1 for a Data Matrix, whose modules are square, and 3 for a PDF417, whose rows
    /// are three times as high as a module is wide.
    pub row_height: u32,
}

impl Matrix {
    /// The code's width and height, in modules.
    pub fn size_in_modules(&self) -> (f32, f32) {
        (self.cols as f32, (self.rows * self.row_height) as f32)
    }
}

/// What a 2D code chooses its shape for.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Fit {
    /// A box of this width and height, in points: the shape that comes out largest in it.
    Box { width: f32, height: f32 },
    /// One size only: a Data Matrix takes its smallest square, and a PDF417 the shape closest to three times as wide
    /// as high.
    OneSide,
}

/// The size of a linear barcode at its default bar width: the width of a module and the height of the bars, in points.
pub(crate) fn linear_module(kind: BarcodeType) -> (f32, f32) {
    /// EAN and UPC at the standard magnification of 1: bars 0.33mm wide, and 22.85mm high, or 18.23mm for EAN-8.
    const EAN_MODULE: f32 = 0.33 * 72.0 / 25.4;
    match kind {
        BarcodeType::Ean13 | BarcodeType::UpcA => (EAN_MODULE, 22.85 * 72.0 / 25.4),
        BarcodeType::Ean8 => (EAN_MODULE, 18.23 * 72.0 / 25.4),
        // A Code 128, Code 39 or GS1-128 bar at its default width, and a height that reads well at it.
        _ => (1.0, 40.0),
    }
}

/// Whether a type is a linear barcode, which `hrt` shows the data of.
pub(crate) fn is_linear(kind: BarcodeType) -> bool {
    matches!(
        kind,
        BarcodeType::Code128 | BarcodeType::Ean13 | BarcodeType::Code39 | BarcodeType::Ean8 | BarcodeType::UpcA
            | BarcodeType::Gs1128
    )
}

/// Encode the data of a linear barcode, or say why it can't be.
pub(crate) fn encode_linear(kind: BarcodeType, data: &str, options: &BarcodeOptions) -> Result<Linear, String> {
    match kind {
        BarcodeType::Code128 => Ok(Linear { bars: code128::encode(data)?, text: data.to_string() }),
        BarcodeType::Gs1128 => {
            let fields = gs1::parse(data)?;
            Ok(Linear { bars: code128::encode_gs1(&fields)?, text: data.trim().to_string() })
        }
        // The digits with their check digit, as EAN-8 and UPC-A show them.
        BarcodeType::Ean13 => {
            let (bars, digits) = ean::encode_ean13(data)?;
            Ok(Linear { bars, text: digits })
        }
        BarcodeType::Ean8 => {
            let (bars, digits) = ean::encode_ean8(data)?;
            Ok(Linear { bars, text: digits })
        }
        BarcodeType::UpcA => {
            let (bars, digits) = ean::encode_upca(data)?;
            Ok(Linear { bars, text: digits })
        }
        BarcodeType::Code39 => Ok(Linear { bars: code39::encode(data, options.check)?, text: data.to_string() }),
        BarcodeType::Qr | BarcodeType::DataMatrix | BarcodeType::Gs1DataMatrix | BarcodeType::Pdf417 => {
            Err(format!("{kind:?} is not a linear barcode"))
        }
    }
}

/// Encode the data of a Data Matrix or a PDF417 in the shape `fit` and the options ask for, or say why it can't be.
pub(crate) fn encode_matrix(kind: BarcodeType, data: &str, options: &BarcodeOptions, fit: Fit) -> Result<Matrix, String> {
    match kind {
        BarcodeType::DataMatrix => datamatrix::encode(data, options, fit),
        BarcodeType::Gs1DataMatrix => datamatrix::encode_gs1(&gs1::parse(data)?, options, fit),
        BarcodeType::Pdf417 => pdf417::encode(data, options, fit),
        _ => Err(format!("{kind:?} is not a Data Matrix or a PDF417")),
    }
}

/// Check the `rows` and `columns` a barcode is given against its type: a PDF417 has 3 to 90 rows and 1 to 30 columns,
/// and a Data Matrix one of the standard's sizes. The other types ignore them.
pub(crate) fn check_shape(kind: BarcodeType, rows: Option<u8>, columns: Option<u8>) -> Result<(), String> {
    match kind {
        BarcodeType::Pdf417 => pdf417::check_shape(rows, columns),
        BarcodeType::DataMatrix | BarcodeType::Gs1DataMatrix => datamatrix::check_shape(rows, columns),
        _ => Ok(()),
    }
}

/// The GS1 check digit of `digits`: the weights 3 and 1 from the right, the sum taken up to the next ten. EAN-13,
/// EAN-8, UPC-A and the GTINs and SSCCs of GS1 data all end in one.
fn gs1_check_digit(digits: &[u8]) -> u8 {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| if i % 2 == 0 { d as u32 * 3 } else { d as u32 })
        .sum();
    ((10 - sum % 10) % 10) as u8
}

/// Run-length encode modules into alternating bar and space widths. The first module must be a bar.
fn run_lengths(modules: &[bool]) -> Vec<u8> {
    let mut out = Vec::new();
    let Some(&first) = modules.first() else { return out };
    let mut current = first;
    let mut run = 0u8;
    for &module in modules {
        if module == current {
            run += 1;
        } else {
            out.push(run);
            current = module;
            run = 1;
        }
    }
    out.push(run);
    out
}
