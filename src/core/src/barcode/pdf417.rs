//! PDF417, ISO/IEC 15438: data compacted into codewords, error correction codewords added, and the codewords drawn row
//! by row between a start and a stop pattern, each row with an indicator codeword at either end.
//!
//! The data is compacted as text (ASCII in four submodes, two characters to a codeword), as numbers (runs of 13 digits
//! or more, about three digits to a codeword) or as bytes (six to five codewords), switching where each is shorter.
//! Text beyond ASCII goes in as UTF-8 bytes behind an ECI that says so.

use super::pdf417_patterns::PATTERNS;
use super::{Fit, Matrix};
use crate::parse::{BarcodeEc, BarcodeOptions};

const LATCH_TO_TEXT: u16 = 900;
const LATCH_TO_BYTE_PADDED: u16 = 901;
const LATCH_TO_NUMERIC: u16 = 902;
const SHIFT_TO_BYTE: u16 = 913;
const LATCH_TO_BYTE: u16 = 924;
const ECI_CHARSET: u16 = 927;
/// The ECI number of UTF-8.
const ECI_UTF8: u16 = 26;
/// The codeword that pads the data to fill the symbol.
const PAD: u16 = 900;

/// The most codewords a symbol holds, its length codeword and its error correction included.
const MAX_CODEWORDS: usize = 928;
const MIN_ROWS: usize = 3;
const MAX_ROWS: usize = 90;
const MAX_COLUMNS: usize = 30;
/// How many modules high a row is drawn: the standard's least, three times a module's width.
const ROW_HEIGHT: u32 = 3;
/// The start pattern: 17 modules. The stop pattern: 18 modules.
const START: (u32, u32) = (0x1fea8, 17);
const STOP: (u32, u32) = (0x3fa29, 18);
/// The shape a PDF417 with one size takes: the closest to three times as wide as high.
const PREFERRED_RATIO: f32 = 3.0;

/// The text submodes: upper case, lower case, mixed (digits and some punctuation) and punctuation.
#[derive(Clone, Copy, PartialEq)]
enum Submode {
    Alpha,
    Lower,
    Mixed,
    Punctuation,
}

/// The characters of the mixed submode, by value; 0 marks a value with no character of its own here.
const MIXED: [u8; 30] = [
    b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'&', b'\r', b'\t', b',', b':', b'#', b'-', b'.', b'$',
    b'/', b'+', b'%', b'*', b'=', b'^', 0, b' ', 0, 0, 0,
];
/// The characters of the punctuation submode, by value.
const PUNCTUATION: [u8; 30] = [
    b';', b'<', b'>', b'@', b'[', b'\\', b']', b'_', b'`', b'~', b'!', b'\r', b'\t', b',', b':', b'\n', b'-', b'.',
    b'$', b'/', b'"', b'|', b'*', b'(', b')', b'?', b'{', b'}', b'\'', 0,
];

/// Check `rows` and `columns` against what a PDF417 has.
pub(super) fn check_shape(rows: Option<u8>, columns: Option<u8>) -> Result<(), String> {
    if let Some(r) = rows.filter(|r| !(MIN_ROWS..=MAX_ROWS).contains(&(*r as usize))) {
        return Err(format!("<barcode> a pdf417 has {MIN_ROWS} to {MAX_ROWS} rows, not {r}"));
    }
    if let Some(c) = columns.filter(|c| !(1..=MAX_COLUMNS).contains(&(*c as usize))) {
        return Err(format!("<barcode> a pdf417 has 1 to {MAX_COLUMNS} columns, not {c}"));
    }
    Ok(())
}

/// Encode `data` as a PDF417 in the shape `fit` and the options ask for.
pub(super) fn encode(data: &str, options: &BarcodeOptions, fit: Fit) -> Result<Matrix, String> {
    let codewords = compact(data.as_bytes());
    let level = match options.ec {
        BarcodeEc::Pdf417(level) => {
            if codewords.len() + 1 + ec_count(level) > MAX_CODEWORDS {
                return Err(format!("PDF417: the data is too long for a PDF417 at ec {level}"));
            }
            level
        }
        _ => recommended_level(codewords.len())
            .ok_or_else(|| "PDF417: the data is too long for a PDF417".to_string())?,
    };
    let needed = codewords.len() + 1 + ec_count(level);
    let (cols, rows) = shape(needed, options, fit).ok_or_else(|| "PDF417: the data is too long for a PDF417".to_string())?;

    // The length codeword counts itself, the data and the padding.
    let padding = cols * rows - needed;
    let mut full = Vec::with_capacity(cols * rows);
    full.push((1 + codewords.len() + padding) as u16);
    full.extend_from_slice(&codewords);
    full.resize(full.len() + padding, PAD);
    let ecc = error_correction(&full, level);
    full.extend_from_slice(&ecc);
    Ok(draw(&full, cols, rows, level))
}

/// The level the standard recommends for this many data codewords, or the highest that fits when the data is too long
/// for that one; `None` when even level 0 doesn't fit.
fn recommended_level(data: usize) -> Option<u8> {
    let recommended = match data {
        0..=40 => 2,
        41..=160 => 3,
        161..=320 => 4,
        _ => 5,
    };
    (0..=recommended).rev().find(|&level| data + 1 + ec_count(level) <= MAX_CODEWORDS)
}

/// The number of error correction codewords of a level: 2 at level 0, doubling with each level up to 512 at 8.
fn ec_count(level: u8) -> usize {
    2 << level
}

/// The columns and rows for `needed` codewords: what `rows` and `columns` fix, else the shape `fit` asks for. Data that
/// needs more than the rows given gets more rows, and more than 90 rows at the columns given more columns.
fn shape(needed: usize, options: &BarcodeOptions, fit: Fit) -> Option<(usize, usize)> {
    let min_rows = options.rows.map_or(MIN_ROWS, |r| r as usize);
    let rows_at = |cols: usize| needed.div_ceil(cols).max(min_rows);
    if let Some(columns) = options.columns {
        return (columns as usize..=MAX_COLUMNS).map(|c| (c, rows_at(c))).find(|&(_, r)| r <= MAX_ROWS);
    }
    let mut candidates: Vec<(usize, usize)> =
        (1..=MAX_COLUMNS).map(|c| (c, rows_at(c))).filter(|&(_, r)| r <= MAX_ROWS).collect();
    if options.rows.is_some() {
        // The rows given hold the data at these columns; at none, the most columns take more rows.
        let within: Vec<(usize, usize)> = candidates.iter().copied().filter(|&(_, r)| r == min_rows).collect();
        if within.is_empty() {
            return candidates.last().copied();
        }
        candidates = within;
    }
    let size = |(c, r): (usize, usize)| ((17 * c + 69) as f32, (r as u32 * ROW_HEIGHT) as f32);
    match fit {
        Fit::Box { width, height } => {
            let module = |shape: (usize, usize)| {
                let (w, h) = size(shape);
                (width / w).min(height / h)
            };
            candidates.into_iter().fold(None, |best: Option<(usize, usize)>, shape| match best {
                Some(b) if module(b) >= module(shape) => Some(b),
                _ => Some(shape),
            })
        }
        Fit::OneSide => {
            let off = |shape: (usize, usize)| {
                let (w, h) = size(shape);
                (w / h - PREFERRED_RATIO).abs()
            };
            candidates.into_iter().fold(None, |best: Option<(usize, usize)>, shape| match best {
                Some(b) if off(b) <= off(shape) => Some(b),
                _ => Some(shape),
            })
        }
    }
}

// ── Compaction ───────────────────────────────────────────────────────────────

/// The data codewords of `data`: text compaction from the start, as a symbol begins in it, switching to numeric
/// compaction for 13 digits or more in a row and to byte compaction for what text can't hold.
fn compact(data: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    if data.iter().any(|b| !b.is_ascii()) {
        out.extend_from_slice(&[ECI_CHARSET, ECI_UTF8]);
    }
    let mut mode_is_text = true;
    let mut submode = Submode::Alpha;
    let mut at = 0;
    while at < data.len() {
        let digits = digit_run(data, at);
        if digits >= 13 {
            out.push(LATCH_TO_NUMERIC);
            mode_is_text = false;
            submode = Submode::Alpha;
            compact_numbers(&data[at..at + digits], &mut out);
            at += digits;
            continue;
        }
        let text = text_run(data, at);
        if text >= 5 || (text > 0 && at + text == data.len() && mode_is_text) {
            if !mode_is_text {
                out.push(LATCH_TO_TEXT);
                mode_is_text = true;
                submode = Submode::Alpha;
            }
            submode = compact_text(&data[at..at + text], submode, &mut out);
            at += text;
            continue;
        }
        let bytes = byte_run(data, at).max(1);
        let run = &data[at..at + bytes];
        if bytes == 1 && mode_is_text {
            out.push(SHIFT_TO_BYTE);
            out.push(run[0] as u16);
        } else {
            compact_bytes(run, &mut out);
            mode_is_text = false;
            submode = Submode::Alpha;
        }
        at += bytes;
    }
    out
}

/// How many digits there are in a row from `at`.
fn digit_run(data: &[u8], at: usize) -> usize {
    data[at..].iter().take_while(|b| b.is_ascii_digit()).count()
}

/// How many bytes from `at` text compaction can take before a run of 13 digits or a byte it can't hold.
fn text_run(data: &[u8], at: usize) -> usize {
    let mut end = at;
    while end < data.len() {
        let digits = digit_run(data, end);
        if digits >= 13 {
            break;
        }
        if digits > 0 {
            end += digits;
            continue;
        }
        if !is_text(data[end]) {
            break;
        }
        end += 1;
    }
    end - at
}

/// How many bytes from `at` go in byte compaction: up to a run of 13 digits or of 5 text characters, which are
/// shorter in their own compaction.
fn byte_run(data: &[u8], at: usize) -> usize {
    let mut end = at;
    while end < data.len() {
        if digit_run(data, end) >= 13 || (end > at && text_run(data, end) >= 5) {
            break;
        }
        end += 1;
    }
    end - at
}

fn is_text(b: u8) -> bool {
    b == b'\t' || b == b'\n' || b == b'\r' || (32..=126).contains(&b)
}

fn is_upper(b: u8) -> bool {
    b == b' ' || b.is_ascii_uppercase()
}

fn is_lower(b: u8) -> bool {
    b == b' ' || b.is_ascii_lowercase()
}

fn mixed_value(b: u8) -> Option<u8> {
    MIXED.iter().position(|&m| m == b && m != 0).map(|v| v as u8)
}

fn punctuation_value(b: u8) -> Option<u8> {
    PUNCTUATION.iter().position(|&p| p == b && p != 0).map(|v| v as u8)
}

/// Compact text in the four submodes, from `submode`, two values to a codeword. Returns the submode it ends in.
fn compact_text(text: &[u8], mut submode: Submode, out: &mut Vec<u16>) -> Submode {
    const LATCH_LOWER: u8 = 27;
    const LATCH_MIXED: u8 = 28;
    const LATCH_ALPHA: u8 = 28;
    const SHIFT_UPPER: u8 = 27;
    const SHIFT_PUNCTUATION: u8 = 29;
    const LATCH_PUNCTUATION: u8 = 25;
    const LATCH_ALPHA_FROM_PUNCTUATION: u8 = 29;
    const SPACE: u8 = 26;

    let mut values: Vec<u8> = Vec::with_capacity(text.len() * 2);
    let mut i = 0;
    while i < text.len() {
        let b = text[i];
        match submode {
            Submode::Alpha if is_upper(b) => values.push(if b == b' ' { SPACE } else { b - b'A' }),
            Submode::Lower if is_lower(b) => values.push(if b == b' ' { SPACE } else { b - b'a' }),
            Submode::Mixed if mixed_value(b).is_some() => values.push(mixed_value(b).unwrap_or_default()),
            Submode::Punctuation if punctuation_value(b).is_some() => {
                values.push(punctuation_value(b).unwrap_or_default());
            }
            Submode::Alpha | Submode::Lower if is_lower(b) => {
                values.push(LATCH_LOWER);
                submode = Submode::Lower;
                continue;
            }
            Submode::Lower if is_upper(b) => {
                values.extend_from_slice(&[SHIFT_UPPER, b - b'A']);
            }
            Submode::Alpha | Submode::Lower if mixed_value(b).is_some() => {
                values.push(LATCH_MIXED);
                submode = Submode::Mixed;
                continue;
            }
            Submode::Mixed if is_upper(b) => {
                values.push(LATCH_ALPHA);
                submode = Submode::Alpha;
                continue;
            }
            Submode::Mixed if is_lower(b) => {
                values.push(LATCH_LOWER);
                submode = Submode::Lower;
                continue;
            }
            Submode::Mixed if text.get(i + 1).is_some_and(|&next| punctuation_value(next).is_some()) => {
                values.push(LATCH_PUNCTUATION);
                submode = Submode::Punctuation;
                continue;
            }
            Submode::Punctuation => {
                values.push(LATCH_ALPHA_FROM_PUNCTUATION);
                submode = Submode::Alpha;
                continue;
            }
            // Punctuation that no submode here holds: shift to punctuation for this one character.
            _ => values.extend_from_slice(&[SHIFT_PUNCTUATION, punctuation_value(b).unwrap_or_default()]),
        }
        i += 1;
    }
    // An odd count is padded with 29: a shift to punctuation for a value that never comes, except in the punctuation
    // submode, where 29 latches to upper case, which is where the next text then starts.
    if values.len() % 2 == 1 {
        values.push(SHIFT_PUNCTUATION);
        if submode == Submode::Punctuation {
            submode = Submode::Alpha;
        }
    }
    out.extend(values.chunks(2).map(|pair| pair[0] as u16 * 30 + pair[1] as u16));
    submode
}

/// Compact digits, 44 at a time: each group with a 1 in front, written in base 900.
fn compact_numbers(digits: &[u8], out: &mut Vec<u16>) {
    for group in digits.chunks(44) {
        let mut number: Vec<u8> = std::iter::once(1).chain(group.iter().map(|d| d - b'0')).collect();
        let mut base900 = Vec::new();
        while !(number.len() == 1 && number[0] == 0) {
            // Divide the decimal number by 900, keeping the remainder.
            let mut remainder = 0u32;
            let mut quotient = Vec::with_capacity(number.len());
            for &d in &number {
                let current = remainder * 10 + d as u32;
                let q = current / 900;
                remainder = current % 900;
                if !(quotient.is_empty() && q == 0) {
                    quotient.push(q as u8);
                }
            }
            base900.push(remainder as u16);
            number = if quotient.is_empty() { vec![0] } else { quotient };
        }
        out.extend(base900.iter().rev());
    }
}

/// Compact bytes: six to five codewords in base 900, and what is left one to a codeword.
fn compact_bytes(bytes: &[u8], out: &mut Vec<u16>) {
    out.push(if bytes.len() % 6 == 0 { LATCH_TO_BYTE } else { LATCH_TO_BYTE_PADDED });
    let mut chunks = bytes.chunks_exact(6);
    for chunk in &mut chunks {
        let mut value = chunk.iter().fold(0u64, |v, &b| (v << 8) | b as u64);
        let mut five = [0u16; 5];
        for slot in five.iter_mut().rev() {
            *slot = (value % 900) as u16;
            value /= 900;
        }
        out.extend_from_slice(&five);
    }
    out.extend(chunks.remainder().iter().map(|&b| b as u16));
}

// ── Error correction ─────────────────────────────────────────────────────────

/// The Reed-Solomon codewords of `data` at `level`, over the integers modulo 929 with 3 as the generator.
fn error_correction(data: &[u16], level: u8) -> Vec<u16> {
    const PRIME: u32 = 929;
    let k = ec_count(level);
    // The generator polynomial, the product of (x - 3^j) for j = 1..k, lowest power first, without its leading 1.
    let mut generator = vec![1u32];
    let mut power = 1u32;
    for _ in 0..k {
        power = power * 3 % PRIME;
        let mut next = vec![0u32; generator.len() + 1];
        for (i, &g) in generator.iter().enumerate() {
            next[i + 1] = (next[i + 1] + g) % PRIME;
            next[i] = (next[i] + PRIME - g * power % PRIME) % PRIME;
        }
        generator = next;
    }
    let coefficients = &generator[..k];

    let mut e = vec![0u32; k];
    for &d in data {
        let t1 = (d as u32 + e[k - 1]) % PRIME;
        for j in (1..k).rev() {
            let t2 = t1 * coefficients[j] % PRIME;
            e[j] = (e[j - 1] + PRIME - t2) % PRIME;
        }
        let t2 = t1 * coefficients[0] % PRIME;
        e[0] = (PRIME - t2) % PRIME;
    }
    e.iter().rev().map(|&v| ((PRIME - v) % PRIME) as u16).collect()
}

// ── Drawing ──────────────────────────────────────────────────────────────────

/// Draw the codewords in `rows` rows of `cols`: start, left indicator, data, right indicator, stop.
fn draw(codewords: &[u16], cols: usize, rows: usize, level: u8) -> Matrix {
    let width = 17 * cols + 69;
    let mut modules = Vec::with_capacity(width * rows);
    let (r, c, l) = (rows - 1, cols - 1, level as usize);
    for y in 0..rows {
        let cluster = y % 3;
        let base = 30 * (y / 3);
        let (left, right) = match cluster {
            0 => (base + r / 3, base + c),
            1 => (base + l * 3 + r % 3, base + r / 3),
            _ => (base + c, base + l * 3 + r % 3),
        };
        push_pattern(&mut modules, START);
        push_codeword(&mut modules, cluster, left);
        for &codeword in &codewords[y * cols..(y + 1) * cols] {
            push_codeword(&mut modules, cluster, codeword as usize);
        }
        push_codeword(&mut modules, cluster, right);
        push_pattern(&mut modules, STOP);
    }
    Matrix { modules, cols: width as u32, rows: rows as u32, row_height: ROW_HEIGHT }
}

fn push_codeword(modules: &mut Vec<bool>, cluster: usize, value: usize) {
    push_pattern(modules, ((1 << 16) | PATTERNS[cluster][value] as u32, 17));
}

/// Add the `length` modules of `pattern`, most significant bit first.
fn push_pattern(modules: &mut Vec<bool>, (pattern, length): (u32, u32)) {
    modules.extend((0..length).rev().map(|bit| (pattern >> bit) & 1 == 1));
}

#[cfg(test)]
pub(super) fn codewords_of(data: &str) -> Vec<u16> {
    compact(data.as_bytes())
}

#[cfg(test)]
pub(super) fn error_correction_of(data: &[u16], level: u8) -> Vec<u16> {
    error_correction(data, level)
}
